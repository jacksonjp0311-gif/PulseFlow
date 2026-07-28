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

#[derive(Debug)]
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
