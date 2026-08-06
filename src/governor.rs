use crate::{
    config::GovernorConfig,
    model::{now_ms, QosLevel},
};

pub struct ProcessGovernor {
    pid: Option<u32>,
    config: GovernorConfig,
    applied: QosLevel,
    last_change_ms: u128,
}

#[derive(Debug, Clone)]
pub struct ApplyResult {
    pub changed: bool,
    pub applied: QosLevel,
    pub message: String,
}

impl ProcessGovernor {
    pub fn new(pid: Option<u32>, config: GovernorConfig) -> Self {
        Self {
            pid,
            config,
            applied: if pid.is_some() {
                QosLevel::Normal
            } else {
                QosLevel::MonitorOnly
            },
            last_change_ms: 0,
        }
    }

    pub fn apply(&mut self, requested: QosLevel) -> ApplyResult {
        let Some(pid) = self.pid else {
            return ApplyResult {
                changed: false,
                applied: QosLevel::MonitorOnly,
                message: "No target process; monitor-only mode.".into(),
            };
        };

        let now = now_ms();
        let dwell_elapsed =
            now.saturating_sub(self.last_change_ms) >= self.config.minimum_dwell_ms as u128;
        if requested == self.applied || !dwell_elapsed {
            return ApplyResult {
                changed: false,
                applied: self.applied,
                message: if requested == self.applied {
                    "QoS held.".into()
                } else {
                    "QoS transition deferred by the minimum dwell timer.".into()
                },
            };
        }

        let effective =
            if requested == QosLevel::Responsive && !self.config.allow_above_normal_priority {
                QosLevel::Normal
            } else {
                requested
            };

        match apply_platform_qos(pid, effective) {
            Ok(()) => {
                self.applied = effective;
                self.last_change_ms = now;
                ApplyResult {
                    changed: true,
                    applied: effective,
                    message: format!("Applied {effective:?} QoS to PID {pid}."),
                }
            }
            Err(error) => ApplyResult {
                changed: false,
                applied: self.applied,
                message: format!("QoS request failed for PID {pid}: {error}"),
            },
        }
    }
}

pub fn platform_governor_supported() -> bool {
    cfg!(target_os = "windows")
}

#[cfg(target_os = "windows")]
fn apply_platform_qos(pid: u32, level: QosLevel) -> Result<(), String> {
    use std::{ffi::c_void, mem::size_of, ptr::null_mut};

    type Handle = *mut c_void;
    type Bool = i32;
    type Dword = u32;

    const PROCESS_SET_INFORMATION: Dword = 0x0200;
    const PROCESS_QUERY_LIMITED_INFORMATION: Dword = 0x1000;
    const NORMAL_PRIORITY_CLASS: Dword = 0x00000020;
    const BELOW_NORMAL_PRIORITY_CLASS: Dword = 0x00004000;
    const ABOVE_NORMAL_PRIORITY_CLASS: Dword = 0x00008000;
    const PROCESS_POWER_THROTTLING: Dword = 4;
    const PROCESS_POWER_THROTTLING_CURRENT_VERSION: Dword = 1;
    const PROCESS_POWER_THROTTLING_EXECUTION_SPEED: Dword = 0x1;

    #[repr(C)]
    struct ProcessPowerThrottlingState {
        version: Dword,
        control_mask: Dword,
        state_mask: Dword,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(desired_access: Dword, inherit_handle: Bool, process_id: Dword) -> Handle;
        fn CloseHandle(object: Handle) -> Bool;
        fn SetPriorityClass(process: Handle, priority_class: Dword) -> Bool;
        fn SetProcessInformation(
            process: Handle,
            information_class: Dword,
            information: *mut c_void,
            information_size: Dword,
        ) -> Bool;
        fn GetLastError() -> Dword;
    }

    unsafe {
        let handle = OpenProcess(
            PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        );
        if handle == null_mut() {
            return Err(format!(
                "OpenProcess failed with Windows error {}",
                GetLastError()
            ));
        }

        let eco = matches!(level, QosLevel::Eco | QosLevel::ThermalProtect);
        let priority = match level {
            QosLevel::Eco | QosLevel::ThermalProtect => BELOW_NORMAL_PRIORITY_CLASS,
            QosLevel::Responsive => ABOVE_NORMAL_PRIORITY_CLASS,
            _ => NORMAL_PRIORITY_CLASS,
        };

        let priority_ok = SetPriorityClass(handle, priority) != 0;
        let mut throttling = ProcessPowerThrottlingState {
            version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            control_mask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            state_mask: if eco {
                PROCESS_POWER_THROTTLING_EXECUTION_SPEED
            } else {
                0
            },
        };
        let qos_ok = SetProcessInformation(
            handle,
            PROCESS_POWER_THROTTLING,
            &mut throttling as *mut _ as *mut c_void,
            size_of::<ProcessPowerThrottlingState>() as Dword,
        ) != 0;
        let error = GetLastError();
        CloseHandle(handle);

        if !priority_ok || !qos_ok {
            Err(format!(
                "SetPriorityClass/SetProcessInformation failed with Windows error {error}"
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_platform_qos(_pid: u32, _level: QosLevel) -> Result<(), String> {
    Err("active process QoS modulation is currently implemented only on Windows".into())
}

/// Apply QoS to an arbitrary PID (used by whole-system Pulse Mesh).
pub fn apply_qos_to_pid(pid: u32, level: QosLevel) -> Result<(), String> {
    apply_platform_qos(pid, level)
}

/// One mesh target for HUD / cortex tooling.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MeshTargetInfo {
    pub pid: u32,
    pub name: String,
    pub score: f64,
}

/// Result of a mesh control step (dwell-aware, change-only logging).
#[derive(Debug, Clone)]
pub struct MeshApplyResult {
    pub changed: bool,
    pub applied: QosLevel,
    pub targets: u32,
    pub target_list: Vec<MeshTargetInfo>,
    pub message: String,
    pub heartbeat: bool,
    pub transition: bool,
}

/// Stateful whole-system mesh controller: dwell, change detection, adaptive targeting.
#[derive(Debug, Default)]
pub struct MeshController {
    applied: QosLevel,
    last_pids: Vec<u32>,
    last_names: Vec<String>,
    last_change_ms: u128,
    last_log_ms: u128,
    last_targets: Vec<MeshTargetInfo>,
    pub transition_count: u64,
}

impl MeshController {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn last_targets(&self) -> &[MeshTargetInfo] {
        &self.last_targets
    }

    /// Adaptive target budget: more processes under RAM pressure.
    pub fn max_targets_for_ram(ram_percent: f64) -> usize {
        if ram_percent >= 92.0 {
            8
        } else if ram_percent >= 88.0 {
            6
        } else if ram_percent >= 84.0 {
            5
        } else {
            4
        }
    }

    /// Score floor relaxes when RAM is high so quiet boxes still get targets.
    pub fn score_floor_for_ram(ram_percent: f64) -> f64 {
        if ram_percent >= 90.0 {
            45.0
        } else if ram_percent >= 85.0 {
            55.0
        } else if ram_percent >= 80.0 {
            65.0
        } else {
            80.0
        }
    }

    /// Mesh RAM-first Eco threshold: slightly more aggressive than single-PID assist.
    pub fn mesh_eco_ram_threshold(profile_eco_enter: f64) -> f64 {
        (profile_eco_enter - 3.0).clamp(78.0, 96.0)
    }

    /// Force Eco when host RAM is the bottleneck (constrained desktop).
    pub fn prefer_mesh_qos(
        requested: QosLevel,
        ram_percent: f64,
        profile_eco_enter: f64,
    ) -> QosLevel {
        if matches!(requested, QosLevel::ThermalProtect) {
            return requested;
        }
        if ram_percent >= Self::mesh_eco_ram_threshold(profile_eco_enter) {
            return QosLevel::Eco;
        }
        requested
    }

    /// Apply mesh QoS with dwell + change-only semantics.
    /// `dwell_ms` defaults to governor minimum dwell; heartbeat logs every `heartbeat_ms`.
    pub fn step(
        &mut self,
        requested: QosLevel,
        ram_percent: f64,
        dwell_ms: u64,
        heartbeat_ms: u64,
    ) -> MeshApplyResult {
        if !platform_governor_supported() {
            return MeshApplyResult {
                changed: false,
                applied: QosLevel::MonitorOnly,
                targets: 0,
                target_list: Vec::new(),
                message: "Mesh QoS requires Windows.".into(),
                heartbeat: false,
                transition: false,
            };
        }
        if matches!(requested, QosLevel::MonitorOnly) {
            return MeshApplyResult {
                changed: false,
                applied: QosLevel::MonitorOnly,
                targets: 0,
                target_list: Vec::new(),
                message: "Mesh is observing only.".into(),
                heartbeat: false,
                transition: false,
            };
        }

        let max_targets = Self::max_targets_for_ram(ram_percent);
        let score_floor = Self::score_floor_for_ram(ram_percent);
        let selected = select_mesh_targets(max_targets, score_floor);
        let now = now_ms();
        let pids: Vec<u32> = selected.iter().map(|t| t.0).collect();
        let names: Vec<String> = selected.iter().map(|t| t.1.clone()).collect();
        let target_list: Vec<MeshTargetInfo> = selected
            .iter()
            .map(|(pid, name, score)| MeshTargetInfo {
                pid: *pid,
                name: name.clone(),
                score: *score,
            })
            .collect();

        if selected.is_empty() {
            let message = format!(
                "No mesh targets above score floor {score_floor:.0} (RAM {ram_percent:.0}%)."
            );
            self.last_targets.clear();
            return MeshApplyResult {
                changed: false,
                applied: self.applied,
                targets: 0,
                target_list: Vec::new(),
                message,
                heartbeat: false,
                transition: false,
            };
        }

        let set_changed = pids != self.last_pids || requested != self.applied;
        let dwell_elapsed =
            self.last_change_ms == 0 || now.saturating_sub(self.last_change_ms) >= dwell_ms as u128;
        let should_apply = set_changed && dwell_elapsed;

        if !should_apply {
            let heartbeat = now.saturating_sub(self.last_log_ms) >= heartbeat_ms as u128;
            if heartbeat {
                self.last_log_ms = now;
            }
            let message = if requested != self.applied && !dwell_elapsed {
                format!("Mesh QoS transition to {requested:?} deferred by dwell ({dwell_ms} ms).")
            } else {
                format!(
                    "Mesh holding {level:?} on {} targets: {}",
                    self.last_pids.len(),
                    self.last_names
                        .iter()
                        .zip(self.last_pids.iter())
                        .map(|(n, p)| format!("{n}#{p}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    level = self.applied
                )
            };
            return MeshApplyResult {
                changed: false,
                applied: self.applied,
                targets: self.last_pids.len() as u32,
                target_list: if self.last_targets.is_empty() {
                    target_list
                } else {
                    self.last_targets.clone()
                },
                message,
                heartbeat,
                transition: false,
            };
        }

        let mut ok = 0u32;
        let mut applied_names = Vec::new();
        for (pid, name, _) in &selected {
            if apply_platform_qos(*pid, requested).is_ok() {
                ok = ok.saturating_add(1);
                applied_names.push(format!("{name}#{pid}"));
            }
        }

        let transition = requested != self.applied || pids != self.last_pids;
        if transition {
            self.transition_count = self.transition_count.saturating_add(1);
        }
        self.applied = requested;
        self.last_pids = pids;
        self.last_names = names;
        self.last_change_ms = now;
        self.last_log_ms = now;
        self.last_targets = target_list.clone();

        MeshApplyResult {
            changed: true,
            applied: requested,
            targets: ok,
            target_list,
            message: format!(
                "Pulse mesh applied {requested:?} to {ok}/{} targets: {}",
                selected.len(),
                applied_names.join(", ")
            ),
            heartbeat: false,
            transition,
        }
    }
}

/// Select top user processes by combined CPU+memory pressure for mesh Eco.
/// Excludes the PulseFlow process and common critical system names.
pub fn select_mesh_targets(max_targets: usize, score_floor: f64) -> Vec<(u32, String, f64)> {
    use sysinfo::{PidExt, ProcessExt, System, SystemExt};
    let mut system = System::new();
    system.refresh_processes();
    let self_pid = std::process::id();
    let blocked = [
        "system",
        "registry",
        "smss.exe",
        "csrss.exe",
        "wininit.exe",
        "services.exe",
        "lsass.exe",
        "svchost.exe",
        "fontdrvhost.exe",
        "dwm.exe",
        "memory compression",
        "secure system",
        "pulseflow-governor",
    ];
    let mut scored: Vec<(u32, String, f64)> = system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            let id = pid.as_u32();
            if id == 0 || id == self_pid {
                return None;
            }
            let name = process.name().to_string();
            let lower = name.to_ascii_lowercase();
            if blocked.iter().any(|item| lower.contains(item)) {
                return None;
            }
            let cpu = process.cpu_usage() as f64;
            // sysinfo reports process memory in bytes on this platform.
            let mem_mb = process.memory() as f64 / 1024.0 / 1024.0;
            // Prefer memory hogs with some CPU activity (desktop mesh).
            let score = mem_mb * 0.65 + cpu * 8.0;
            if score < score_floor {
                return None;
            }
            Some((id, name, score))
        })
        .collect();
    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(max_targets.max(1));
    scored
}

/// Stateless convenience wrapper (tests / one-shot). Prefer `MeshController::step` in runtime.
pub fn apply_mesh_qos(level: QosLevel, max_targets: usize) -> (u32, String) {
    let mut mesh = MeshController::default();
    // Bypass dwell for one-shot by priming last_change_ms far in the past via step with empty state.
    let result = mesh.step(level, 90.0, 0, 30_000);
    let _ = max_targets;
    (result.targets, result.message)
}
