use pulseflow_governor::{
    adaptive,
    config::Config,
    model::{LearningStage, RuntimeMetrics, RuntimeTuning},
};

#[test]
fn adaptive_tuning_waits_for_evidence() {
    let config = Config::default();
    let tuning = RuntimeTuning::from(&config.control);
    let metrics = RuntimeMetrics {
        samples: config.agent_policy.minimum_samples_before_adaptation - 1,
        policy_confidence: 0.8,
        flow_stability: 0.5,
        prediction_rmse: 0.2,
        ..RuntimeMetrics::default()
    };
    let suggestion = adaptive::recommend(
        &tuning,
        &metrics,
        LearningStage::BoundedAdaptive,
        &config.agent_policy,
    );
    assert!(!suggestion.eligible);
    assert!(!suggestion.applied);
}

#[test]
fn shadow_stage_never_applies_candidate() {
    let mut config = Config::default();
    config.agent_policy.minimum_samples_before_adaptation = 30;
    config.agent_policy.adaptation_interval_samples = 30;
    config.agent_policy.allow_bounded_adaptation = true;
    let tuning = RuntimeTuning::from(&config.control);
    let metrics = RuntimeMetrics {
        samples: 30,
        policy_confidence: 0.8,
        flow_stability: 0.5,
        prediction_rmse: 0.2,
        ..RuntimeMetrics::default()
    };
    let suggestion = adaptive::recommend(
        &tuning,
        &metrics,
        LearningStage::Shadow,
        &config.agent_policy,
    );
    assert!(suggestion.eligible);
    assert!(!suggestion.applied);
    assert!(suggestion.deltas.maximum_absolute_delta() <= config.agent_policy.maximum_gain_step);
}

#[test]
fn bounded_stage_respects_explicit_gate() {
    let mut config = Config::default();
    config.agent_policy.minimum_samples_before_adaptation = 30;
    config.agent_policy.adaptation_interval_samples = 30;
    config.agent_policy.allow_bounded_adaptation = true;
    let tuning = RuntimeTuning::from(&config.control);
    let metrics = RuntimeMetrics {
        samples: 30,
        policy_confidence: 1.0,
        flow_stability: 0.5,
        prediction_rmse: 0.2,
        ..RuntimeMetrics::default()
    };
    let suggestion = adaptive::recommend(
        &tuning,
        &metrics,
        LearningStage::BoundedAdaptive,
        &config.agent_policy,
    );
    assert!(suggestion.applied);
    assert!(suggestion.proposed_tuning.is_some());
    assert!(suggestion.deltas.maximum_absolute_delta() <= config.agent_policy.maximum_gain_step);
}
