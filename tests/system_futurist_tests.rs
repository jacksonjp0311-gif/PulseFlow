use pulseflow_governor::{
    config::{AgentPolicyConfig, ControlConfig, StressWeights},
    controller::PulseController,
    futurist::{score_stress_skill, snapshot_from_series, HORIZONS},
    model::{ControlSnapshot, LearningStage, OperatingMode, QosLevel, RuntimeMetrics, Telemetry},
    policy::{recommend_with_guard, MemoryGuard},
    system::FormFactor,
};

#[test]
fn form_factor_labels_are_stable() {
    assert_eq!(FormFactor::MobileClass.as_str(), "mobile_class");
    assert_eq!(FormFactor::Server.as_str(), "server");
    assert_eq!(FormFactor::Desktop.as_str(), "desktop");
}

#[test]
fn high_ram_triggers_eco_assist_when_governor_active() {
    let config = ControlConfig {
        quiet_setpoint: 0.5,
        balanced_setpoint: 0.66,
        performance_setpoint: 0.78,
        kp: 0.65,
        ki: 0.08,
        kd: 0.1,
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
        cpu: 0.28,
        memory: 0.32,
        gpu_utilization: 0.0,
        gpu_temperature: 0.0,
        io_pressure: 0.0,
        latency: 0.0,
    };
    let mut controller = PulseController::new(config, weights, false);
    controller.set_eco_ram_enter_percent(85.0);
    let telemetry = Telemetry {
        cpu_percent: 20.0,
        memory_percent: 90.0,
        memory_used_gb: 7.0,
        memory_total_gb: 8.0,
        ..Telemetry::default()
    };
    let snap = controller.step(&telemetry, OperatingMode::Balanced, 1.0, true, 82.0, 76.0);
    assert_eq!(snap.requested_qos, QosLevel::Eco);
    assert!(snap.reason.contains("RAM"));
}

#[test]
fn futurist_emits_multi_horizon_channels() {
    let mut stress = Vec::new();
    let mut ram = Vec::new();
    let mut eco = Vec::new();
    for i in 0..40 {
        stress.push(0.30 + i as f64 * 0.005);
        ram.push(80.0 + i as f64 * 0.1);
        eco.push(0.40 + i as f64 * 0.004);
    }
    let snap = snapshot_from_series(&stress, &ram, &eco, Some(40.0), 0.75, 1e-6);
    assert!(!snap.channels.is_empty());
    let stress_channel = snap
        .channels
        .iter()
        .find(|channel| channel.channel == "stress")
        .expect("stress channel");
    assert_eq!(stress_channel.horizons.len(), HORIZONS.len());
    assert!(matches!(
        snap.envelope.as_str(),
        "hold" | "suggest_eco" | "contract_agent" | "thermal_watch"
    ));
}

#[test]
fn futurist_skill_scores_linear_series() {
    let series: Vec<f64> = (0..80).map(|i| 0.2 + i as f64 * 0.002).collect();
    let skill = score_stress_skill(&series, 1e-6);
    assert!(skill.samples_scored > 0);
    assert!(skill.mae_h5 < skill.mae_persist_h5 + 0.05);
}

#[test]
fn adaptive_memory_guard_uses_host_thresholds() {
    let telemetry = Telemetry {
        memory_percent: 83.0,
        ..Telemetry::default()
    };
    let control = ControlSnapshot {
        capacity_signal: 0.9,
        control_authority: 0.9,
        controller_effort: 0.1,
        applied_modulation: 0.1,
        filtered_stress: 0.3,
        setpoint: 0.66,
        phase: "advance".into(),
        ..ControlSnapshot::default()
    };
    let metrics = RuntimeMetrics {
        samples: 400,
        flow_stability: 0.95,
        homeostatic_slack: 0.8,
        ..RuntimeMetrics::default()
    };
    let config = AgentPolicyConfig {
        maximum_concurrency: 16,
        maximum_batch_size: 512,
        minimum_batch_size: 1,
        allow_bounded_adaptation: false,
        minimum_samples_before_adaptation: 300,
        adaptation_interval_samples: 30,
        maximum_gain_step: 0.01,
    };
    let tight = MemoryGuard {
        soft_percent: 70.0,
        hard_percent: 80.0,
        critical_percent: 90.0,
    };
    let directive = recommend_with_guard(
        &telemetry,
        &control,
        &metrics,
        LearningStage::AgentPolicy,
        &config,
        tight,
        "hold",
    );
    assert!(!directive.allow_background_memory_work);
    assert_eq!(directive.model_route, "efficient");
}
