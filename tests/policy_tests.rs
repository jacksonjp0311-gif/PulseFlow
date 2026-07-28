use pulseflow_governor::{
    config::AgentPolicyConfig,
    model::{ControlSnapshot, LearningStage, RuntimeMetrics, Telemetry},
    policy,
};

fn policy_config() -> AgentPolicyConfig {
    AgentPolicyConfig {
        maximum_concurrency: 16,
        maximum_batch_size: 512,
        minimum_batch_size: 1,
        allow_bounded_adaptation: false,
        minimum_samples_before_adaptation: 300,
        adaptation_interval_samples: 30,
        maximum_gain_step: 0.01,
    }
}

fn abundant_control() -> ControlSnapshot {
    ControlSnapshot {
        modulation: 0.90,
        capacity_signal: 0.90,
        control_authority: 0.90,
        controller_effort: 0.90,
        applied_modulation: 0.90,
        setpoint: 0.66,
        filtered_stress: 0.30,
        phase: "advance".into(),
        ..ControlSnapshot::default()
    }
}

fn stable_metrics() -> RuntimeMetrics {
    RuntimeMetrics {
        samples: 400,
        flow_stability: 0.95,
        homeostatic_slack: 0.80,
        recovery_balance: 0.50,
        ..RuntimeMetrics::default()
    }
}

#[test]
fn high_memory_pressure_blocks_background_work_and_contracts_capacity() {
    let low_memory = Telemetry {
        memory_percent: 60.0,
        ..Telemetry::default()
    };
    let high_memory = Telemetry {
        memory_percent: 89.1,
        ..Telemetry::default()
    };

    let low = policy::recommend(
        &low_memory,
        &abundant_control(),
        &stable_metrics(),
        LearningStage::AgentPolicy,
        &policy_config(),
    );
    let high = policy::recommend(
        &high_memory,
        &abundant_control(),
        &stable_metrics(),
        LearningStage::AgentPolicy,
        &policy_config(),
    );

    assert!(low.allow_background_memory_work);
    assert!(!high.allow_background_memory_work);
    assert_eq!(high.model_route, "efficient");
    assert!(high.recommended_concurrency < low.recommended_concurrency);
    assert!(high.recommended_batch_size < low.recommended_batch_size);
    assert!(high.recommended_concurrency <= 4);
    assert!(high.recommended_batch_size <= 32);
    assert!(high.reason.contains("Memory pressure"));
}

#[test]
fn critical_memory_pressure_serializes_agent_work() {
    let telemetry = Telemetry {
        memory_percent: 97.0,
        ..Telemetry::default()
    };
    let directive = policy::recommend(
        &telemetry,
        &abundant_control(),
        &stable_metrics(),
        LearningStage::AgentPolicy,
        &policy_config(),
    );

    assert_eq!(directive.recommended_concurrency, 1);
    assert_eq!(directive.recommended_batch_size, 1);
    assert!(!directive.allow_background_memory_work);
    assert_eq!(directive.token_budget_scale, 0.45);
}

#[test]
fn workload_capacity_is_not_mistaken_for_controller_effort() {
    let telemetry = Telemetry {
        memory_percent: 60.0,
        ..Telemetry::default()
    };
    let mut control = abundant_control();
    control.controller_effort = 0.05;
    control.applied_modulation = 0.05;
    control.modulation = 0.05;

    let directive = policy::recommend(
        &telemetry,
        &control,
        &stable_metrics(),
        LearningStage::AgentPolicy,
        &policy_config(),
    );

    assert_eq!(directive.authority, 0.90);
    assert_eq!(directive.model_route, "performance");
    assert!(directive.recommended_concurrency > 1);
}
