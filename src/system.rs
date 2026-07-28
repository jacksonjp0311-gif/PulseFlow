//! Know thy system: adaptive form-factor profiles for any host.
//!
//! PulseFlow does not assume a fixed desktop plant. It measures CPU count,
//! memory capacity, OS family, and available sensors, then derives stress
//! weights and pressure thresholds that fit servers, desktops, and small /
//! mobile-class hosts. Process QoS remains platform-gated; observation and
//! agent advice work everywhere.

use crate::config::StressWeights;
use serde::{Deserialize, Serialize};
use sysinfo::{System, SystemExt};

/// Coarse host class used to adapt stress weights and policy thresholds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FormFactor {
    /// Phones, tablets, or tiny VMs: scarce RAM, few cores.
    MobileClass,
    /// Laptops / small desktops under ~12 GB RAM.
    ConstrainedDesktop,
    /// Typical interactive workstation.
    #[default]
    Desktop,
    /// High core / high memory hosts or headless service nodes.
    Server,
}

impl FormFactor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MobileClass => "mobile_class",
            Self::ConstrainedDesktop => "constrained_desktop",
            Self::Desktop => "desktop",
            Self::Server => "server",
        }
    }
}

/// Live identity of the machine PulseFlow is governing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemProfile {
    pub schema_version: String,
    pub os: String,
    pub arch: String,
    pub form_factor: FormFactor,
    pub hostname: String,
    pub cpu_count: usize,
    pub memory_total_gb: f64,
    pub memory_used_gb: f64,
    pub memory_percent: f64,
    pub has_gpu_telemetry: bool,
    pub governor_supported: bool,
    pub observation_capable: bool,
    pub process_qos_capable: bool,
    pub agent_signal_capable: bool,
    pub adaptive_weights: StressWeights,
    pub memory_soft_percent: f64,
    pub memory_hard_percent: f64,
    pub memory_critical_percent: f64,
    pub eco_ram_enter_percent: f64,
    pub known_as: String,
    pub adaptation_reason: String,
}

impl Default for SystemProfile {
    fn default() -> Self {
        Self {
            schema_version: "pulseflow.system-profile.v1".into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            form_factor: FormFactor::Desktop,
            hostname: "unknown".into(),
            cpu_count: 1,
            memory_total_gb: 0.0,
            memory_used_gb: 0.0,
            memory_percent: 0.0,
            has_gpu_telemetry: false,
            governor_supported: cfg!(target_os = "windows"),
            observation_capable: true,
            process_qos_capable: cfg!(target_os = "windows"),
            agent_signal_capable: true,
            adaptive_weights: StressWeights {
                cpu: 0.28,
                memory: 0.24,
                gpu_utilization: 0.18,
                gpu_temperature: 0.14,
                io_pressure: 0.10,
                latency: 0.06,
            },
            memory_soft_percent: 75.0,
            memory_hard_percent: 85.0,
            memory_critical_percent: 95.0,
            eco_ram_enter_percent: 88.0,
            known_as: "unknown host".into(),
            adaptation_reason: "default profile".into(),
        }
    }
}

/// Probe the host and return a full adaptive profile.
pub fn probe(governor_supported: bool, has_gpu_telemetry: bool) -> SystemProfile {
    let mut system = System::new();
    system.refresh_memory();
    system.refresh_cpu();

    let cpu_count = system.cpus().len().max(1);
    // sysinfo 0.29 reports memory in bytes on Windows and some Linux builds,
    // and historically in KiB on others. Detect by magnitude.
    let total_raw = system.total_memory().max(1) as f64;
    let used_raw = (system.used_memory() as f64).min(total_raw);
    let (memory_total_gb, memory_used_gb) = if total_raw > 10_000_000.0 {
        // bytes → GiB
        (total_raw / 1_073_741_824.0, used_raw / 1_073_741_824.0)
    } else {
        // KiB → GiB
        (total_raw / 1_048_576.0, used_raw / 1_048_576.0)
    };
    let memory_percent = if total_raw > 0.0 {
        (used_raw / total_raw) * 100.0
    } else {
        0.0
    };
    let hostname = system
        .host_name()
        .unwrap_or_else(|| "unknown".into())
        .chars()
        .take(96)
        .collect::<String>();

    let override_factor = std::env::var("PULSEFLOW_FORM_FACTOR")
        .ok()
        .and_then(|value| parse_form_factor(&value));
    let form_factor = override_factor
        .unwrap_or_else(|| classify_form_factor(cpu_count, memory_total_gb, has_gpu_telemetry));

    let (weights, soft, hard, critical, eco_ram, reason) =
        adaptive_parameters(form_factor, memory_total_gb, has_gpu_telemetry);

    let known_as = format!(
        "{hostname} · {} · {cpu_count} cpu · {:.1} GB · {}",
        std::env::consts::OS,
        memory_total_gb,
        form_factor.as_str()
    );

    SystemProfile {
        schema_version: "pulseflow.system-profile.v1".into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        form_factor,
        hostname,
        cpu_count,
        memory_total_gb,
        memory_used_gb,
        memory_percent,
        has_gpu_telemetry,
        governor_supported,
        observation_capable: true,
        process_qos_capable: governor_supported,
        agent_signal_capable: true,
        adaptive_weights: weights,
        memory_soft_percent: soft,
        memory_hard_percent: hard,
        memory_critical_percent: critical,
        eco_ram_enter_percent: eco_ram,
        known_as,
        adaptation_reason: reason,
    }
}

fn parse_form_factor(value: &str) -> Option<FormFactor> {
    match value.trim().to_ascii_lowercase().as_str() {
        "mobile" | "mobile_class" | "phone" | "tablet" => Some(FormFactor::MobileClass),
        "constrained" | "constrained_desktop" | "laptop" => Some(FormFactor::ConstrainedDesktop),
        "desktop" | "workstation" => Some(FormFactor::Desktop),
        "server" | "headless" | "cloud" => Some(FormFactor::Server),
        _ => None,
    }
}

fn classify_form_factor(cpu_count: usize, memory_total_gb: f64, has_gpu: bool) -> FormFactor {
    if memory_total_gb > 0.0 && memory_total_gb < 6.0 && cpu_count <= 8 {
        return FormFactor::MobileClass;
    }
    if memory_total_gb > 0.0 && memory_total_gb < 12.0 {
        return FormFactor::ConstrainedDesktop;
    }
    if cpu_count >= 16 && memory_total_gb >= 32.0 && !has_gpu {
        return FormFactor::Server;
    }
    if cpu_count >= 24 && memory_total_gb >= 64.0 {
        return FormFactor::Server;
    }
    FormFactor::Desktop
}

fn adaptive_parameters(
    form_factor: FormFactor,
    memory_total_gb: f64,
    has_gpu: bool,
) -> (StressWeights, f64, f64, f64, f64, String) {
    match form_factor {
        FormFactor::MobileClass => (
            StressWeights {
                cpu: 0.26,
                memory: 0.38,
                gpu_utilization: if has_gpu { 0.12 } else { 0.0 },
                gpu_temperature: if has_gpu { 0.10 } else { 0.0 },
                io_pressure: 0.08,
                latency: 0.06,
            },
            70.0,
            80.0,
            90.0,
            82.0,
            format!(
                "Mobile-class profile ({memory_total_gb:.1} GB): memory-first weights and earlier guards."
            ),
        ),
        FormFactor::ConstrainedDesktop => (
            StressWeights {
                cpu: 0.28,
                memory: 0.32,
                gpu_utilization: if has_gpu { 0.16 } else { 0.0 },
                gpu_temperature: if has_gpu { 0.12 } else { 0.0 },
                io_pressure: 0.08,
                latency: 0.04,
            },
            72.0,
            82.0,
            92.0,
            85.0,
            format!(
                "Constrained desktop ({memory_total_gb:.1} GB): elevated RAM weight so stress tracks host memory."
            ),
        ),
        FormFactor::Server => (
            StressWeights {
                cpu: 0.34,
                memory: 0.24,
                gpu_utilization: if has_gpu { 0.12 } else { 0.0 },
                gpu_temperature: if has_gpu { 0.08 } else { 0.0 },
                io_pressure: 0.16,
                latency: 0.06,
            },
            78.0,
            88.0,
            96.0,
            90.0,
            format!(
                "Server profile ({memory_total_gb:.1} GB, multi-core): CPU/IO-forward weights with late Eco RAM trigger."
            ),
        ),
        FormFactor::Desktop => (
            StressWeights {
                cpu: 0.28,
                memory: 0.24,
                gpu_utilization: if has_gpu { 0.20 } else { 0.0 },
                gpu_temperature: if has_gpu { 0.14 } else { 0.0 },
                io_pressure: 0.08,
                latency: 0.06,
            },
            75.0,
            85.0,
            95.0,
            88.0,
            format!(
                "Desktop profile ({memory_total_gb:.1} GB): balanced ecosystem weights with RAM-aware Eco assist."
            ),
        ),
    }
}

/// Renormalize weights after sensors drop out so missing channels do not dilute stress.
pub fn renormalize_weights(weights: &StressWeights, has_gpu: bool, has_io: bool) -> StressWeights {
    let mut cpu = weights.cpu.max(0.0);
    let mut memory = weights.memory.max(0.0);
    let mut gpu_u = if has_gpu {
        weights.gpu_utilization.max(0.0)
    } else {
        0.0
    };
    let mut gpu_t = if has_gpu {
        weights.gpu_temperature.max(0.0)
    } else {
        0.0
    };
    let mut io = if has_io {
        weights.io_pressure.max(0.0)
    } else {
        0.0
    };
    let mut latency = if has_io {
        weights.latency.max(0.0)
    } else {
        0.0
    };
    let sum = cpu + memory + gpu_u + gpu_t + io + latency;
    if sum <= f64::EPSILON {
        return StressWeights {
            cpu: 0.5,
            memory: 0.5,
            gpu_utilization: 0.0,
            gpu_temperature: 0.0,
            io_pressure: 0.0,
            latency: 0.0,
        };
    }
    cpu /= sum;
    memory /= sum;
    gpu_u /= sum;
    gpu_t /= sum;
    io /= sum;
    latency /= sum;
    StressWeights {
        cpu,
        memory,
        gpu_utilization: gpu_u,
        gpu_temperature: gpu_t,
        io_pressure: io,
        latency,
    }
}
