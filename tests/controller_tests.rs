use pulseflow_governor::{
    config::{ControlConfig, StressWeights},
    controller::PulseController,
    model::{GpuTelemetry, IoSignal, OperatingMode, QosLevel, Telemetry},
};

fn telemetry(cpu: f64, ram: f64, gpu: f64, temperature: Option<f64>) -> Telemetry {
    Telemetry {
        timestamp_ms: 1,
        cpu_percent: cpu,
        memory_percent: ram,
        memory_used_gb: 8.0,
        memory_total_gb: 16.0,
        gpu: Some(GpuTelemetry {
            name: Some("test-gpu".into()),
            utilization_percent: gpu,
            memory_used_mb: 1_024.0,
            memory_total_mb: 4_096.0,
            temperature_c: temperature,
            power_w: Some(40.0),
            power_limit_w: Some(75.0),
        }),
        process: None,
        io: IoSignal::default(),
        io_signal_fresh: false,
        cpu_temperature_c: None,
        sensor_note: String::new(),
    }
}

#[test]
fn controller_outputs_remain_bounded() {
    let config = ControlConfig {
        quiet_setpoint: 0.50,
        balanced_setpoint: 0.66,
        performance_setpoint: 0.78,
        kp: 0.65,
        ki: 0.08,
        kd: 0.10,
        kr: 0.34,
        residue_decay: 0.82,
        filter_alpha: 0.24,
        slew_per_sample: 0.07,
        eco_enter: 0.40,
        eco_exit: 0.50,
        responsive_enter: 0.88,
        responsive_exit: 0.78,
    };
    let weights = StressWeights {
        cpu: 0.30,
        memory: 0.14,
        gpu_utilization: 0.22,
        gpu_temperature: 0.18,
        io_pressure: 0.10,
        latency: 0.06,
    };
    let mut controller = PulseController::new(config, weights, false);
    for _ in 0..100 {
        let snapshot = controller.step(
            &telemetry(100.0, 100.0, 100.0, Some(95.0)),
            OperatingMode::Balanced,
            1.0,
            true,
            82.0,
            76.0,
        );
        assert!((0.0..=1.0).contains(&snapshot.raw_stress));
        assert!((0.0..=1.0).contains(&snapshot.filtered_stress));
        assert!((0.0..=1.0).contains(&snapshot.modulation));
        assert!((0.0..=1.0).contains(&snapshot.capacity_signal));
        assert!((0.0..=1.0).contains(&snapshot.controller_effort));
        assert!((0.0..=1.0).contains(&snapshot.applied_modulation));
        assert!((0.0..=1.0).contains(&snapshot.control_authority));
        assert_eq!(snapshot.modulation, snapshot.applied_modulation);
        assert_eq!(snapshot.controller_effort, snapshot.applied_modulation);
        assert!((-1.0..=1.0).contains(&snapshot.residue));
    }
}

#[test]
fn thermal_guard_latches_and_releases() {
    let config = pulseflow_governor::config::Config::default();
    let mut controller = PulseController::new(config.control, config.weights, false);
    let hot = controller.step(
        &telemetry(60.0, 50.0, 75.0, Some(84.0)),
        OperatingMode::Balanced,
        1.0,
        true,
        82.0,
        76.0,
    );
    assert_eq!(hot.requested_qos, QosLevel::ThermalProtect);

    let still_latched = controller.step(
        &telemetry(40.0, 40.0, 30.0, Some(78.0)),
        OperatingMode::Balanced,
        1.0,
        true,
        82.0,
        76.0,
    );
    assert_eq!(still_latched.requested_qos, QosLevel::ThermalProtect);

    let released = controller.step(
        &telemetry(40.0, 40.0, 30.0, Some(75.0)),
        OperatingMode::Balanced,
        1.0,
        true,
        82.0,
        76.0,
    );
    assert_ne!(released.requested_qos, QosLevel::ThermalProtect);
}

#[test]
fn inactive_governor_is_monitor_only() {
    let config = pulseflow_governor::config::Config::default();
    let mut controller = PulseController::new(config.control, config.weights, true);
    let snapshot = controller.step(
        &telemetry(90.0, 90.0, 90.0, Some(70.0)),
        OperatingMode::Performance,
        1.0,
        false,
        82.0,
        76.0,
    );
    assert_eq!(snapshot.requested_qos, QosLevel::MonitorOnly);
    assert_eq!(snapshot.applied_modulation, 0.0);
    assert_eq!(snapshot.control_authority, 0.0);
}
