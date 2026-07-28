use crate::{
    config::AgentPolicyConfig,
    model::{AgentDirective, ControlSnapshot, LearningStage, RuntimeMetrics, Telemetry},
    system::SystemProfile,
};

/// Adaptive memory guard thresholds derived from the host profile.
#[derive(Debug, Clone, Copy)]
pub struct MemoryGuard {
    pub soft_percent: f64,
    pub hard_percent: f64,
    pub critical_percent: f64,
}

impl Default for MemoryGuard {
    fn default() -> Self {
        Self {
            soft_percent: 75.0,
            hard_percent: 85.0,
            critical_percent: 95.0,
        }
    }
}

impl From<&SystemProfile> for MemoryGuard {
    fn from(profile: &SystemProfile) -> Self {
        Self {
            soft_percent: profile.memory_soft_percent,
            hard_percent: profile.memory_hard_percent,
            critical_percent: profile.memory_critical_percent,
        }
    }
}

pub fn recommend(
    telemetry: &Telemetry,
    control: &ControlSnapshot,
    metrics: &RuntimeMetrics,
    stage: LearningStage,
    config: &AgentPolicyConfig,
) -> AgentDirective {
    recommend_with_guard(
        telemetry,
        control,
        metrics,
        stage,
        config,
        MemoryGuard::default(),
        "hold",
    )
}

pub fn recommend_with_guard(
    telemetry: &Telemetry,
    control: &ControlSnapshot,
    metrics: &RuntimeMetrics,
    stage: LearningStage,
    config: &AgentPolicyConfig,
    guard: MemoryGuard,
    futurist_envelope: &str,
) -> AgentDirective {
    let temperature_pressure = telemetry
        .gpu
        .as_ref()
        .and_then(|gpu| gpu.temperature_c)
        .map(|temperature| ((temperature - 60.0) / 25.0).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let queue = (telemetry.io.input_queue + telemetry.io.output_queue) as f64;
    let queue_pressure = (queue / 64.0).clamp(0.0, 1.0);
    let soft = guard.soft_percent;
    let hard = guard.hard_percent;
    let critical = guard.critical_percent;
    let span = (hard - soft).max(5.0);
    let memory_pressure = ((telemetry.memory_percent - soft) / span).clamp(0.0, 1.0);
    let residual_pressure = control.residue_memory.max(0.0).clamp(0.0, 1.0);
    let stability = metrics.flow_stability.clamp(0.0, 1.0);

    let capacity = (control.capacity_signal
        * (1.0 - 0.55 * temperature_pressure)
        * (1.0 - 0.25 * queue_pressure)
        * (1.0 - 0.35 * memory_pressure)
        * (0.70 + 0.30 * stability)
        * (1.0 - 0.25 * residual_pressure))
        .clamp(0.05, 1.0);

    let mut concurrency = ((config.maximum_concurrency.max(1) as f64 * capacity).round() as u32)
        .clamp(1, config.maximum_concurrency.max(1));
    let minimum_batch = config.minimum_batch_size.max(1);
    let maximum_batch = config.maximum_batch_size.max(minimum_batch);
    let mut batch = ((maximum_batch as f64 * capacity * capacity).round() as u32)
        .clamp(minimum_batch, maximum_batch);

    let memory_constrained = telemetry.memory_percent >= hard;
    let memory_critical = telemetry.memory_percent >= critical;
    let memory_elevated = telemetry.memory_percent >= soft;
    let futurist_contract = matches!(
        futurist_envelope,
        "contract_agent" | "suggest_eco" | "thermal_watch"
    );
    if memory_critical || futurist_envelope == "contract_agent" {
        concurrency = 1;
        batch = minimum_batch;
    } else if telemetry.memory_percent >= ((hard + critical) * 0.5) {
        concurrency = concurrency.min(2);
        batch = batch.min(16).max(minimum_batch);
    } else if memory_constrained || futurist_contract {
        concurrency = concurrency.min(4);
        batch = batch.min(32).max(minimum_batch);
    } else if memory_elevated {
        concurrency = concurrency.min(concurrency.max(1));
        batch = batch
            .min((maximum_batch as f64 * 0.75).round() as u32)
            .max(minimum_batch);
    }
    let homeostasis_evidence = metrics.samples >= 8;
    if homeostasis_evidence {
        if metrics.homeostatic_slack <= 0.15 {
            concurrency = 1;
            batch = batch.min(8).max(minimum_batch);
        } else if metrics.homeostatic_slack <= 0.30 {
            concurrency = concurrency.min(2);
            batch = batch.min(16).max(minimum_batch);
        } else if metrics.homeostatic_slack <= 0.45 {
            concurrency = concurrency.min(4);
            batch = batch.min(32).max(minimum_batch);
        }
    }
    let protect = control.phase == "protect" || temperature_pressure >= 0.80 || memory_critical;
    let constrained = protect
        || memory_constrained
        || control.filtered_stress > control.setpoint + 0.10
        || residual_pressure > 0.10;
    let abundant = !constrained
        && control.capacity_signal >= 0.75
        && stability >= 0.80
        && queue_pressure < 0.50
        && (!homeostasis_evidence || metrics.homeostatic_slack > 0.55);

    let (model_route, allow_background, token_scale, retrieval_scale, reason) = if memory_critical
        || futurist_envelope == "contract_agent"
    {
        (
            "efficient",
            false,
            0.45,
            0.40,
            "Critical memory pressure is active; serialize work and suspend background memory activity.",
        )
    } else if protect || futurist_envelope == "thermal_watch" {
        (
            "efficient",
            false,
            0.45,
            0.40,
            "Thermal protection is active; route to the efficient model and serialize background work.",
        )
    } else if memory_constrained || futurist_envelope == "suggest_eco" {
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
    let homeostasis_note = if homeostasis_evidence {
        format!(
            " Homeostatic slack {:.2}; recovery balance {:.2}.",
            metrics.homeostatic_slack, metrics.recovery_balance
        )
    } else {
        String::new()
    };

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
                "{}{} Directive remains shadow-only until {} finalized samples are available.",
                reason, homeostasis_note, config.minimum_samples_before_adaptation
            )
        } else {
            format!("{reason}{homeostasis_note}")
        },
    }
}
