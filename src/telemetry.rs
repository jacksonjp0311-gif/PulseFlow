use crate::model::{now_ms, GpuTelemetry, IoSignal, ProcessTelemetry, Telemetry};
use std::process::Command;
use sysinfo::{CpuExt, Pid, PidExt, ProcessExt, System, SystemExt};

pub struct TelemetryCollector {
    system: System,
    target_pid: Option<u32>,
    gpu_name: Option<String>,
    gpu_probe_counter: u8,
    last_gpu: Option<GpuTelemetry>,
}

impl TelemetryCollector {
    pub fn new(target_pid: Option<u32>) -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        Self {
            system,
            target_pid,
            gpu_name: probe_gpu_name(),
            gpu_probe_counter: 0,
            last_gpu: None,
        }
    }

    pub fn sample(&mut self, io: IoSignal, signal_stale_after_ms: u64) -> Telemetry {
        self.system.refresh_cpu();
        self.system.refresh_memory();
        self.system.refresh_processes();

        let cpu_percent = self.system.global_cpu_info().cpu_usage() as f64;
        let total_memory = self.system.total_memory() as f64;
        let used_memory = self.system.used_memory() as f64;
        let memory_percent = if total_memory > 0.0 {
            used_memory / total_memory * 100.0
        } else {
            0.0
        };

        let process = self.target_pid.map(|pid| {
            let process = self.system.process(Pid::from_u32(pid));
            ProcessTelemetry {
                pid,
                cpu_percent: process
                    .map(|value| value.cpu_usage() as f64)
                    .unwrap_or_default(),
                memory_mb: process
                    .map(|value| value.memory() as f64 / 1024.0 / 1024.0)
                    .unwrap_or_default(),
                alive: process.is_some(),
            }
        });

        if self.gpu_probe_counter == 0 {
            self.gpu_probe_counter = 1;
            if let Some(sample) = probe_nvidia_gpu(self.gpu_name.clone()) {
                self.last_gpu = Some(sample);
            }
        } else {
            self.gpu_probe_counter = 0;
        }

        let timestamp_ms = now_ms();
        let signal_age = timestamp_ms.saturating_sub(io.updated_at_ms);
        let io_signal_fresh = io.updated_at_ms > 0 && signal_age <= signal_stale_after_ms as u128;

        Telemetry {
            timestamp_ms,
            cpu_percent: cpu_percent.clamp(0.0, 100.0),
            memory_percent: memory_percent.clamp(0.0, 100.0),
            memory_used_gb: used_memory / 1024.0 / 1024.0 / 1024.0,
            memory_total_gb: total_memory / 1024.0 / 1024.0 / 1024.0,
            gpu: self.last_gpu.clone(),
            process,
            io,
            io_signal_fresh,
            cpu_temperature_c: None,
            sensor_note: "CPU temperature is left unavailable unless an explicit sensor adapter is connected. NVIDIA telemetry is read through nvidia-smi when present.".into(),
        }
    }
}

fn probe_gpu_name() -> Option<String> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|line| line.trim().to_string())
}

fn probe_nvidia_gpu(name: Option<String>) -> Option<GpuTelemetry> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let fields: Vec<&str> = text.lines().next()?.split(',').map(str::trim).collect();
    if fields.len() < 6 {
        return None;
    }
    let parse = |value: &str| value.parse::<f64>().ok();
    Some(GpuTelemetry {
        name,
        utilization_percent: parse(fields[0]).unwrap_or_default().clamp(0.0, 100.0),
        memory_used_mb: parse(fields[1]).unwrap_or_default().max(0.0),
        memory_total_mb: parse(fields[2]).unwrap_or_default().max(0.0),
        temperature_c: parse(fields[3]),
        power_w: parse(fields[4]),
        power_limit_w: parse(fields[5]),
    })
}
