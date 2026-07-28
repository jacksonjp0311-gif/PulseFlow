use crate::{
    analytics::{mean, stddev},
    config::{Config, ControlConfig},
    controller::PulseController,
    model::{
        GpuTelemetry, IoSignal, ObservationFrame, ProcessTelemetry, QosLevel, ReplayReport,
        RuntimeTuning, Telemetry,
    },
};

pub fn run_replay(
    session_id: &str,
    frames: &[ObservationFrame],
    config: &Config,
    tuning: &RuntimeTuning,
) -> ReplayReport {
    let mut candidate_config: ControlConfig = config.control.clone();
    tuning.apply_to(&mut candidate_config);
    let mut controller = PulseController::new(candidate_config, config.weights.clone(), false);
    let default_dt = (config.sample_interval_ms.max(1) as f64 / 1_000.0).clamp(0.001, 60.0);
    let mut previous_timestamp = None;
    let mut candidate_residues = Vec::with_capacity(frames.len());
    let mut candidate_modulation = Vec::with_capacity(frames.len());
    let mut baseline_residues = Vec::with_capacity(frames.len());
    let mut eco = 0;
    let mut normal = 0;
    let mut responsive = 0;
    let mut thermal = 0;

    for frame in frames {
        let dt = previous_timestamp
            .map(|previous: u128| frame.timestamp_ms.saturating_sub(previous) as f64 / 1_000.0)
            .filter(|value| *value > 0.0 && *value <= 60.0)
            .unwrap_or(default_dt);
        previous_timestamp = Some(frame.timestamp_ms);
        let telemetry = telemetry_from_frame(frame);
        let snapshot = controller.step(
            &telemetry,
            frame.action.mode,
            dt,
            true,
            config.governor.thermal_guard_c,
            config.governor.thermal_release_c,
        );
        baseline_residues.push(frame.controller.residue);
        candidate_residues.push(snapshot.residue);
        candidate_modulation.push(snapshot.modulation);
        match snapshot.requested_qos {
            QosLevel::Eco => eco += 1,
            QosLevel::Normal | QosLevel::MonitorOnly => normal += 1,
            QosLevel::Responsive => responsive += 1,
            QosLevel::ThermalProtect => thermal += 1,
        }
    }

    ReplayReport {
        session_id: session_id.to_string(),
        samples_replayed: frames.len() as u64,
        baseline_prediction_rmse: rmse(&baseline_residues),
        candidate_prediction_rmse: rmse(&candidate_residues),
        baseline_flow_stability: frames
            .last()
            .map(|frame| frame.metrics.flow_stability)
            .unwrap_or_default(),
        candidate_modulation_mean: mean(&candidate_modulation),
        candidate_modulation_stddev: stddev(&candidate_modulation),
        candidate_eco_requests: eco,
        candidate_normal_requests: normal,
        candidate_responsive_requests: responsive,
        candidate_thermal_requests: thermal,
        note: "Replay recomputes controller traces against recorded observations. It does not claim a causal throughput or temperature improvement until the candidate is tested in a controlled live A/B run.".into(),
    }
}

fn rmse(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        (values.iter().map(|value| value * value).sum::<f64>() / values.len() as f64).sqrt()
    }
}

fn telemetry_from_frame(frame: &ObservationFrame) -> Telemetry {
    let gpu_available = frame.machine.gpu_percent.is_some()
        || frame.machine.gpu_temperature_c.is_some()
        || frame.machine.gpu_power_w.is_some();
    let gpu = gpu_available.then(|| GpuTelemetry {
        name: None,
        utilization_percent: frame.machine.gpu_percent.unwrap_or_default(),
        memory_used_mb: frame.machine.gpu_memory_used_mb.unwrap_or_default(),
        memory_total_mb: frame.machine.gpu_memory_total_mb.unwrap_or_default(),
        temperature_c: frame.machine.gpu_temperature_c,
        power_w: frame.machine.gpu_power_w,
        power_limit_w: None,
    });
    let process = frame.machine.process_alive.map(|alive| ProcessTelemetry {
        pid: 0,
        cpu_percent: frame.machine.process_cpu_percent.unwrap_or_default(),
        memory_mb: frame.machine.process_memory_mb.unwrap_or_default(),
        alive,
    });
    Telemetry {
        timestamp_ms: frame.timestamp_ms,
        cpu_percent: frame.machine.cpu_percent,
        memory_percent: frame.machine.ram_percent,
        memory_used_gb: frame.machine.ram_used_gb,
        memory_total_gb: frame.machine.ram_total_gb,
        gpu,
        process,
        io: IoSignal {
            source: frame.workload.source.clone(),
            agent: frame.workload.agent.clone(),
            task_type: frame.workload.task_type.clone(),
            model: frame.workload.model.clone(),
            context_tokens: frame.workload.context_tokens,
            input_queue: frame.workload.input_queue,
            output_queue: frame.workload.output_queue,
            latency_ms: frame.outcome.latency_ms,
            tokens_per_second: frame.outcome.tokens_per_second,
            completed_units: frame.outcome.completed_units,
            success: frame.outcome.success,
            busy: frame.workload.busy,
            updated_at_ms: frame.timestamp_ms,
        },
        io_signal_fresh: frame.workload.signal_fresh,
        cpu_temperature_c: None,
        sensor_note:
            "Reconstructed from a PulseFlow observation frame for deterministic controller replay."
                .into(),
    }
}
