use crate::{
    config::AgentPolicyConfig,
    model::{AdaptiveSuggestion, LearningStage, RuntimeMetrics, RuntimeTuning, TuningDelta},
};

/// Produces a bounded controller-tuning recommendation from finalized evidence.
///
/// The recommendation is generated only at configured checkpoints. Shadow mode
/// records it without applying it. Bounded-adaptive mode may apply it when the
/// explicit configuration gate is enabled.
pub fn recommend(
    current: &RuntimeTuning,
    metrics: &RuntimeMetrics,
    stage: LearningStage,
    config: &AgentPolicyConfig,
) -> AdaptiveSuggestion {
    let stage_supports_tuning = matches!(
        stage,
        LearningStage::Shadow | LearningStage::BoundedAdaptive
    );
    if !stage_supports_tuning {
        return AdaptiveSuggestion {
            based_on_samples: metrics.samples,
            proposed_tuning: Some(current.clone()),
            reason: "Adaptive tuning is inactive outside shadow or bounded-adaptive stage.".into(),
            ..AdaptiveSuggestion::default()
        };
    }

    if metrics.samples < config.minimum_samples_before_adaptation {
        return AdaptiveSuggestion {
            based_on_samples: metrics.samples,
            proposed_tuning: Some(current.clone()),
            confidence: metrics.policy_confidence,
            reason: format!(
                "Waiting for {} finalized samples before proposing controller changes.",
                config.minimum_samples_before_adaptation
            ),
            ..AdaptiveSuggestion::default()
        };
    }

    let interval = config.adaptation_interval_samples.max(1);
    if metrics.samples % interval != 0 {
        return AdaptiveSuggestion {
            based_on_samples: metrics.samples,
            proposed_tuning: Some(current.clone()),
            confidence: metrics.policy_confidence,
            reason: format!(
                "Evidence is sufficient; the next bounded tuning checkpoint occurs every {interval} finalized samples."
            ),
            ..AdaptiveSuggestion::default()
        };
    }

    let confidence = metrics.policy_confidence.clamp(0.0, 1.0);
    let maximum_step = config.maximum_gain_step.clamp(0.000_1, 0.05);

    // Binary floating-point can make an exact-looking update slightly exceed
    // its declared bound (for example, 0.34 + 0.01 - 0.34). Reserve a tiny
    // numerical margin so the measured delta remains within maximum_gain_step.
    let safe_maximum_step = maximum_step * (1.0 - 64.0 * f64::EPSILON);
    let step = safe_maximum_step * (0.25 + 0.75 * confidence);
    let mut proposed = current.clone();

    if metrics.prediction_rmse > 0.05 {
        proposed.kr = (proposed.kr + step).clamp(0.0, 2.0);
        proposed.residue_decay = (proposed.residue_decay + 0.5 * step).clamp(0.0, 0.999);
    }

    if metrics.flow_stability < 0.75 {
        proposed.kp = (proposed.kp - 0.5 * step).clamp(0.0, 2.0);
        proposed.filter_alpha = (proposed.filter_alpha - 0.5 * step).clamp(0.01, 1.0);
        proposed.slew_per_sample = (proposed.slew_per_sample - 0.25 * step).clamp(0.001, 0.25);
    } else if metrics.flow_stability > 0.90 && metrics.prediction_rmse < 0.03 {
        proposed.kp = (proposed.kp + 0.25 * step).clamp(0.0, 2.0);
        proposed.filter_alpha = (proposed.filter_alpha + 0.25 * step).clamp(0.01, 1.0);
    }

    let deltas = TuningDelta::between(current, &proposed);
    debug_assert!(
        deltas.maximum_absolute_delta() <= maximum_step,
        "adaptive tuning exceeded maximum_gain_step"
    );
    let changed = deltas.maximum_absolute_delta() > f64::EPSILON;
    let apply =
        changed && stage == LearningStage::BoundedAdaptive && config.allow_bounded_adaptation;

    let reason = if !changed {
        "The controller is inside the current tuning envelope; no bounded change is proposed."
            .into()
    } else if apply {
        "A bounded tuning checkpoint was reached and the explicitly enabled adaptive gate permits this small update.".into()
    } else if stage == LearningStage::Shadow {
        "A bounded candidate was generated in shadow mode and was not applied.".into()
    } else {
        "A bounded candidate was generated, but live gain writes are disabled by configuration."
            .into()
    };

    AdaptiveSuggestion {
        based_on_samples: metrics.samples,
        eligible: changed,
        applied: apply,
        confidence,
        proposed_tuning: Some(proposed),
        deltas,
        reason,
    }
}
