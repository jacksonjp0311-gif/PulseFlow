use crate::{
    config::AgentPolicyConfig,
    model::{AgentDirective, ControlSnapshot, LearningStage, RuntimeMetrics, Telemetry},
};

pub fn recommend(
    telemetry: &Telemetry,
    control: &ControlSnapshot,
    metrics: &RuntimeMetrics,
    stage: LearningStage,
    config: &AgentPolicyConfig,
) -> AgentDirective {
    let temperature_pressure = telemetry
        .gpu
        .as_ref()
        .and_then(|gpu| gpu.temperature_c)
        .map(|temperature| ((temperature - 60.0) / 25.0).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let queue = (telemetry.io.input_queue + telemetry.io.output_queue) as f64;
    let queue_pressure = (queue / 64.0).clamp(0.0, 1.0);
    let memory_pressure = ((telemetry.memory_percent - 75.0) / 20.0).clamp(0.0, 1.0);
    let residual_pressure = control.residue_memory.max(0.0).clamp(0.0, 1.0);
    let stability = metrics.flow_stability.clamp(0.0, 1.0);

    let capacity = (control.capacity_signal
        * (1.0 - 0.55 * temperature_pressure)
        * (1.0 - 0.25 * queue_pressure)
        * (1.0 - 0.35 * memory_pressure)
        * (0.70 + 0.30 * stability)
        * (1.0 - 0.25 * residual_pressure))
        .clamp(0.05, 1.0);

    let concurrency = ((config.maximum_concurrency.max(1) as f64 * capacity).round() as u32)
        .clamp(1, config.maximum_concurrency.max(1));
    let minimum_batch = config.minimum_batch_size.max(1);
    let maximum_batch = config.maximum_batch_size.max(minimum_batch);
    let batch = ((maximum_batch as f64 * capacity * capacity).round() as u32)
        .clamp(minimum_batch, maximum_batch);

    let memory_constrained = telemetry.memory_percent >= 85.0;
    let memory_critical = telemetry.memory_percent >= 95.0;
    let protect = control.phase == "protect" || temperature_pressure >= 0.80 || memory_critical;
    let constrained = protect
        || memory_constrained
        || control.filtered_stress > control.setpoint + 0.10
        || residual_pressure > 0.10;
    let abundant = !constrained
        && control.capacity_signal >= 0.75
        && stability >= 0.80
        && queue_pressure < 0.50;

    let (model_route, allow_background, token_scale, retrieval_scale, reason) = if memory_critical {
        (
            "efficient",
            false,
            0.45,
            0.40,
            "Critical memory pressure is active; serialize work and suspend background memory activity.",
        )
    } else if protect {
        (
            "efficient",
            false,
            0.45,
            0.40,
            "Thermal protection is active; route to the efficient model and serialize background work.",
        )
    } else if memory_constrained {
        (
            "efficient",
            false,
            0.65,
            0.60,
            "Memory pressure is above the safe background-work boundary; contract token, retrieval, and batch pressure.",
        )
    } else if constrained {
        (
            "efficient",
            false,
            0.65,
            0.60,
            "Positive load, latency, or residual pressure is present; contract concurrency and batch pressure.",
        )
    } else if abundant {
        (
            "performance",
            true,
            1.00,
            1.00,
            "Stable resource headroom is available; controlled parallel work may advance.",
        )
    } else {
        (
            "balanced",
            metrics.flow_stability >= 0.70,
            0.82,
            0.80,
            "The system is inside the balanced operating envelope; hold moderate agent pressure.",
        )
    };

    let enough_evidence = metrics.samples >= config.minimum_samples_before_adaptation;
    let directive_live = stage == LearningStage::AgentPolicy && enough_evidence;

    AgentDirective {
        authority: control.capacity_signal,
        recommended_concurrency: if protect { 1 } else { concurrency },
        recommended_batch_size: if protect { minimum_batch } else { batch },
        allow_background_memory_work: allow_background,
        model_route: model_route.into(),
        token_budget_scale: token_scale,
        retrieval_depth_scale: retrieval_scale,
        shadow_only: !directive_live,
        reason: if !enough_evidence {
            format!(
                "{} Directive remains shadow-only until {} finalized samples are available.",
                reason, config.minimum_samples_before_adaptation
            )
        } else {
            reason.into()
        },
    }
}
