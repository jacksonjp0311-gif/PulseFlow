use crate::{
    authority::{authority_contract, AuthorityState},
    config::ControlConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{
    collections::VecDeque,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OperatingMode {
    Quiet,
    Balanced,
    Performance,
}

impl Default for OperatingMode {
    fn default() -> Self {
        Self::Balanced
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QosLevel {
    MonitorOnly,
    Eco,
    Normal,
    Responsive,
    ThermalProtect,
}

impl Default for QosLevel {
    fn default() -> Self {
        Self::MonitorOnly
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningStage {
    Recorder,
    Analytics,
    Replay,
    Shadow,
    BoundedAdaptive,
    AgentPolicy,
}

impl Default for LearningStage {
    fn default() -> Self {
        Self::Recorder
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpuTelemetry {
    pub name: Option<String>,
    pub utilization_percent: f64,
    pub memory_used_mb: f64,
    pub memory_total_mb: f64,
    pub temperature_c: Option<f64>,
    pub power_w: Option<f64>,
    pub power_limit_w: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoSignal {
    pub source: String,
    pub agent: String,
    pub task_type: String,
    pub model: String,
    pub context_tokens: u64,
    pub input_queue: u32,
    pub output_queue: u32,
    pub latency_ms: f64,
    pub tokens_per_second: f64,
    pub completed_units: u64,
    pub success: Option<bool>,
    pub busy: bool,
    pub updated_at_ms: u128,
}

impl Default for IoSignal {
    fn default() -> Self {
        Self {
            source: "none".into(),
            agent: "unbound".into(),
            task_type: "unknown".into(),
            model: "unknown".into(),
            context_tokens: 0,
            input_queue: 0,
            output_queue: 0,
            latency_ms: 0.0,
            tokens_per_second: 0.0,
            completed_units: 0,
            success: None,
            busy: false,
            updated_at_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SignalInput {
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default = "default_agent")]
    pub agent: String,
    #[serde(default = "default_task_type")]
    pub task_type: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub context_tokens: u64,
    #[serde(default)]
    pub input_queue: u32,
    #[serde(default)]
    pub output_queue: u32,
    #[serde(default)]
    pub latency_ms: f64,
    #[serde(default)]
    pub tokens_per_second: f64,
    #[serde(default)]
    pub completed_units: u64,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default)]
    pub busy: bool,
}

fn default_source() -> String {
    "external".into()
}
fn default_agent() -> String {
    "unbound".into()
}
fn default_task_type() -> String {
    "unknown".into()
}
fn default_model() -> String {
    "unknown".into()
}

impl From<SignalInput> for IoSignal {
    fn from(value: SignalInput) -> Self {
        Self {
            source: bounded_text(value.source, 96, "external"),
            agent: bounded_text(value.agent, 96, "unbound"),
            task_type: bounded_text(value.task_type, 96, "unknown"),
            model: bounded_text(value.model, 128, "unknown"),
            context_tokens: value.context_tokens.min(100_000_000),
            input_queue: value.input_queue.min(100_000),
            output_queue: value.output_queue.min(100_000),
            latency_ms: finite_clamp(value.latency_ms, 0.0, 60_000.0),
            tokens_per_second: finite_clamp(value.tokens_per_second, 0.0, 1_000_000.0),
            completed_units: value.completed_units.min(1_000_000_000),
            success: value.success,
            busy: value.busy,
            updated_at_ms: now_ms(),
        }
    }
}

fn bounded_text(value: String, max_chars: usize, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return fallback.into();
    }
    trimmed.chars().take(max_chars).collect()
}

fn finite_clamp(value: f64, minimum: f64, maximum: f64) -> f64 {
    if value.is_finite() {
        value.clamp(minimum, maximum)
    } else {
        minimum
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessTelemetry {
    pub pid: u32,
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub alive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Telemetry {
    pub timestamp_ms: u128,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub memory_used_gb: f64,
    pub memory_total_gb: f64,
    pub gpu: Option<GpuTelemetry>,
    pub process: Option<ProcessTelemetry>,
    pub io: IoSignal,
    pub io_signal_fresh: bool,
    pub cpu_temperature_c: Option<f64>,
    pub sensor_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ControlSnapshot {
    pub raw_stress: f64,
    pub filtered_stress: f64,
    pub setpoint: f64,
    pub error: f64,
    pub integral: f64,
    pub derivative: f64,
    pub predicted_stress: f64,
    pub residue: f64,
    pub residue_memory: f64,
    /// Backward-compatible alias for `controller_effort`.
    pub modulation: f64,
    /// Bounded controller estimate of available workload headroom.
    pub capacity_signal: f64,
    /// Fraction of the verified process-scoped control envelope that is authorized.
    pub control_authority: f64,
    /// Normalized event-triggered controller effort for the current interval.
    pub controller_effort: f64,
    /// Backward-compatible alias for `controller_effort`.
    pub applied_modulation: f64,
    /// One-sample-ahead stress forecast from the bounded trend estimator.
    pub forecast_stress: f64,
    /// Confidence in the forecast, normalized to [0, 1].
    pub forecast_confidence: f64,
    pub jitter: f64,
    pub phase: String,
    pub reason: String,
    pub requested_qos: QosLevel,
    pub applied_qos: QosLevel,
    pub transition_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RuntimeMetrics {
    pub samples: u64,
    pub elapsed_seconds: f64,
    pub average_tokens_per_second: f64,
    pub completed_units: u64,
    pub throughput_units_per_second: f64,
    pub flow_stability: f64,
    pub thermal_oscillation_c: Option<f64>,
    pub prediction_rmse: f64,
    pub cumulative_gpu_energy_joules: Option<f64>,
    pub energy_per_token_joules: Option<f64>,
    pub intervention_value: Option<f64>,
    pub policy_confidence: f64,
    pub latency_mean_ms: f64,
    pub latency_p95_ms: f64,
    pub queue_mean: f64,
    pub stress_mean: f64,
    pub oscillation_coherence: Option<f64>,
    pub turbulence_state: String,
    pub forecast_stress: Option<f64>,
    pub forecast_trend_per_sample: f64,
    pub forecast_confidence: f64,
    pub forecast_pressure_risk: f64,
    /// Sum of ΔV for V = 1/2(error²); negative values indicate aggregate contraction.
    pub lyapunov_delta_total: f64,
    /// Average decrement -ΔV across the rolling observation window.
    pub lyapunov_decrement_mean: f64,
    /// Fraction of intervals with ΔV < 0.
    pub contraction_confidence: f64,
    /// Fraction of near-neutral intervals within a bounded ΔV tolerance.
    pub marginal_fraction: f64,
    /// Applied QoS transitions normalized by elapsed minutes.
    pub trigger_density_per_minute: f64,
    /// Smallest observed time between applied QoS transitions.
    pub minimum_inter_event_ms: Option<u64>,
    /// Bottleneck-aware pressure across host resources and workload delay.
    pub ecosystem_pressure: f64,
    /// Pressure not visible in the instantaneous resource vector: trend plus residue.
    pub latent_pressure: f64,
    /// Provisional reserve for accepting work and returning to the observed envelope.
    pub homeostatic_slack: f64,
    /// Signed net pressure velocity over the rolling window, per minute.
    pub pressure_momentum_per_minute: f64,
    /// Mean falling pressure velocity during recovery intervals.
    pub recovery_rate_per_second: f64,
    /// Mean rising pressure velocity during accumulation intervals.
    pub accumulation_rate_per_second: f64,
    /// Recovery versus accumulation balance in [-1, 1].
    pub recovery_balance: f64,
    /// Mean absolute correlation between changes in independently observed resources.
    pub resource_coupling: Option<f64>,
    /// Median measured time for a detected pressure pulse to recover halfway.
    pub recovery_half_life_seconds: Option<f64>,
    /// Selected target memory divided by total used host memory.
    pub target_memory_share: Option<f64>,
    /// Signed per-resource pressure velocity over the rolling window, per minute.
    pub resource_momentum_per_minute: BTreeMap<String, f64>,
    /// Measured pulse half-life for each independently observed resource.
    pub resource_recovery_half_life_seconds: BTreeMap<String, f64>,
    /// Mean positive displacement across the resource-pressure vector.
    pub vector_accumulation: f64,
    /// Mean negative displacement across the resource-pressure vector.
    pub vector_dissipation: f64,
    /// Pressure that moved between channels while scalar pressure could appear stable.
    pub pressure_transduction: f64,
    /// Signed vector accumulation minus dissipation.
    pub net_vector_pressure: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentDirective {
    pub authority: f64,
    pub recommended_concurrency: u32,
    pub recommended_batch_size: u32,
    pub allow_background_memory_work: bool,
    pub model_route: String,
    pub token_budget_scale: f64,
    pub retrieval_depth_scale: f64,
    pub shadow_only: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TuningDelta {
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    pub kr: f64,
    pub residue_decay: f64,
    pub filter_alpha: f64,
    pub slew_per_sample: f64,
}

impl TuningDelta {
    pub fn between(current: &RuntimeTuning, proposed: &RuntimeTuning) -> Self {
        Self {
            kp: proposed.kp - current.kp,
            ki: proposed.ki - current.ki,
            kd: proposed.kd - current.kd,
            kr: proposed.kr - current.kr,
            residue_decay: proposed.residue_decay - current.residue_decay,
            filter_alpha: proposed.filter_alpha - current.filter_alpha,
            slew_per_sample: proposed.slew_per_sample - current.slew_per_sample,
        }
    }

    pub fn maximum_absolute_delta(&self) -> f64 {
        [
            self.kp,
            self.ki,
            self.kd,
            self.kr,
            self.residue_decay,
            self.filter_alpha,
            self.slew_per_sample,
        ]
        .into_iter()
        .map(f64::abs)
        .fold(0.0, f64::max)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdaptiveSuggestion {
    pub based_on_samples: u64,
    pub eligible: bool,
    pub applied: bool,
    pub confidence: f64,
    pub proposed_tuning: Option<RuntimeTuning>,
    pub deltas: TuningDelta,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkloadFrame {
    pub source: String,
    pub agent: String,
    pub task_type: String,
    pub model: String,
    pub context_tokens: u64,
    pub input_queue: u32,
    pub output_queue: u32,
    pub busy: bool,
    pub signal_fresh: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MachineFrame {
    pub cpu_percent: f64,
    pub ram_percent: f64,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub gpu_percent: Option<f64>,
    pub gpu_temperature_c: Option<f64>,
    pub gpu_power_w: Option<f64>,
    pub gpu_memory_used_mb: Option<f64>,
    pub gpu_memory_total_mb: Option<f64>,
    pub process_cpu_percent: Option<f64>,
    pub process_memory_mb: Option<f64>,
    pub process_alive: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ActionFrame {
    pub mode: OperatingMode,
    pub governor_active: bool,
    pub requested_qos: QosLevel,
    pub applied_qos: QosLevel,
    pub modulation_authority: f64,
    pub applied_modulation: f64,
    pub learning_stage: LearningStage,
    pub directive: AgentDirective,
    pub adaptive_suggestion: AdaptiveSuggestion,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResidualFrame {
    pub predicted_stress: f64,
    pub observed_stress: f64,
    pub residue: f64,
    pub residue_memory: f64,
    pub squared_prediction_error: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutcomeFrame {
    pub observed_at_ms: u128,
    pub horizon_ms: u64,
    pub alignment: String,
    pub latency_ms: f64,
    pub tokens_per_second: f64,
    pub completed_units: u64,
    pub success: Option<bool>,
    pub estimated_tokens_this_interval: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationFrame {
    pub schema_version: String,
    pub session_id: String,
    #[serde(default)]
    pub experiment_id: String,
    #[serde(default)]
    pub epoch_revision: u64,
    #[serde(default)]
    pub epoch_reason: String,
    #[serde(default)]
    pub tuning_revision: u64,
    pub sequence: u64,
    pub timestamp_ms: u128,
    pub workload: WorkloadFrame,
    pub machine: MachineFrame,
    pub controller: ControlSnapshot,
    pub action: ActionFrame,
    pub residue: ResidualFrame,
    pub outcome: OutcomeFrame,
    pub metrics: RuntimeMetrics,
}

impl ObservationFrame {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: String,
        experiment_id: String,
        epoch_revision: u64,
        epoch_reason: String,
        tuning_revision: u64,
        sequence: u64,
        telemetry: Telemetry,
        control: ControlSnapshot,
        mode: OperatingMode,
        governor_active: bool,
        learning_stage: LearningStage,
        directive: AgentDirective,
        adaptive_suggestion: AdaptiveSuggestion,
        dt_seconds: f64,
    ) -> Self {
        let process = telemetry.process.clone();
        let gpu = telemetry.gpu.clone();
        let residue_value = control.residue;
        let _ = dt_seconds;
        Self {
            schema_version: "pulseflow.observation.v3".into(),
            session_id,
            experiment_id,
            epoch_revision,
            epoch_reason,
            tuning_revision,
            sequence,
            timestamp_ms: telemetry.timestamp_ms,
            workload: WorkloadFrame {
                source: telemetry.io.source.clone(),
                agent: telemetry.io.agent.clone(),
                task_type: telemetry.io.task_type.clone(),
                model: telemetry.io.model.clone(),
                context_tokens: telemetry.io.context_tokens,
                input_queue: telemetry.io.input_queue,
                output_queue: telemetry.io.output_queue,
                busy: telemetry.io.busy,
                signal_fresh: telemetry.io_signal_fresh,
            },
            machine: MachineFrame {
                cpu_percent: telemetry.cpu_percent,
                ram_percent: telemetry.memory_percent,
                ram_used_gb: telemetry.memory_used_gb,
                ram_total_gb: telemetry.memory_total_gb,
                gpu_percent: gpu.as_ref().map(|value| value.utilization_percent),
                gpu_temperature_c: gpu.as_ref().and_then(|value| value.temperature_c),
                gpu_power_w: gpu.as_ref().and_then(|value| value.power_w),
                gpu_memory_used_mb: gpu.as_ref().map(|value| value.memory_used_mb),
                gpu_memory_total_mb: gpu.as_ref().map(|value| value.memory_total_mb),
                process_cpu_percent: process.as_ref().map(|value| value.cpu_percent),
                process_memory_mb: process.as_ref().map(|value| value.memory_mb),
                process_alive: process.as_ref().map(|value| value.alive),
            },
            controller: control.clone(),
            action: ActionFrame {
                mode,
                governor_active,
                requested_qos: control.requested_qos,
                applied_qos: control.applied_qos,
                modulation_authority: control.control_authority,
                applied_modulation: control.applied_modulation,
                learning_stage,
                directive,
                adaptive_suggestion,
            },
            residue: ResidualFrame {
                predicted_stress: control.predicted_stress,
                observed_stress: control.raw_stress,
                residue: residue_value,
                residue_memory: control.residue_memory,
                squared_prediction_error: residue_value * residue_value,
            },
            outcome: OutcomeFrame {
                observed_at_ms: 0,
                horizon_ms: 0,
                alignment: "pending_next_interval".into(),
                latency_ms: 0.0,
                tokens_per_second: 0.0,
                completed_units: 0,
                success: None,
                estimated_tokens_this_interval: 0.0,
            },
            metrics: RuntimeMetrics::default(),
        }
    }

    pub fn finalize_outcome(&mut self, telemetry: &Telemetry) {
        let horizon_ms = telemetry.timestamp_ms.saturating_sub(self.timestamp_ms);
        let horizon_seconds = (horizon_ms as f64 / 1_000.0).clamp(0.001, 60.0);
        self.outcome = OutcomeFrame {
            observed_at_ms: telemetry.timestamp_ms,
            horizon_ms: horizon_ms.min(u64::MAX as u128) as u64,
            alignment: "next_interval".into(),
            latency_ms: telemetry.io.latency_ms,
            tokens_per_second: telemetry.io.tokens_per_second,
            completed_units: telemetry.io.completed_units,
            success: telemetry.io.success,
            estimated_tokens_this_interval: telemetry.io.tokens_per_second.max(0.0)
                * horizon_seconds,
        };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEvent {
    pub timestamp_ms: u128,
    pub kind: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<EvidenceReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TargetIdentity {
    pub pid: u32,
    pub executable: String,
    pub executable_path_hash: String,
    pub connected_at_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvidenceReceipt {
    pub schema_version: String,
    pub receipt_id: String,
    pub timestamp_ms: u128,
    pub session_id: String,
    pub target_pid: Option<u32>,
    pub executable: Option<String>,
    pub executable_path_hash: Option<String>,
    pub requested_transition: String,
    pub previous_state: AuthorityState,
    pub resulting_state: AuthorityState,
    pub verification_checks: Vec<String>,
    pub qos_action_result: String,
    pub controller_configuration_hash: String,
    pub backend_build: String,
    pub success: bool,
    pub failure_reason: Option<String>,
    pub previous_receipt_hash: Option<String>,
    pub receipt_hash: String,
}

impl EvidenceReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: &str,
        target: Option<&TargetIdentity>,
        requested_transition: &str,
        previous_state: AuthorityState,
        resulting_state: AuthorityState,
        verification_checks: Vec<String>,
        qos_action_result: impl Into<String>,
        controller_configuration_hash: impl Into<String>,
        success: bool,
        failure_reason: Option<String>,
        previous_receipt_hash: Option<String>,
    ) -> Self {
        let timestamp_ms = now_ms();
        let target_pid = target.map(|identity| identity.pid);
        let executable = target.map(|identity| identity.executable.clone());
        let executable_path_hash = target.map(|identity| identity.executable_path_hash.clone());
        let controller_configuration_hash = controller_configuration_hash.into();
        let qos_action_result = qos_action_result.into();
        let payload = format!(
            "{session_id}|{target_pid:?}|{executable:?}|{requested_transition}|{previous_state:?}|{resulting_state:?}|{timestamp_ms}|{success}|{previous_receipt_hash:?}|{controller_configuration_hash}"
        );
        let receipt_hash = stable_hash(payload.as_bytes());
        Self {
            schema_version: "pulseflow.evidence-receipt.v1".into(),
            receipt_id: format!("pf-{}", &receipt_hash),
            timestamp_ms,
            session_id: session_id.into(),
            target_pid,
            executable,
            executable_path_hash,
            requested_transition: requested_transition.into(),
            previous_state,
            resulting_state,
            verification_checks,
            qos_action_result,
            controller_configuration_hash,
            backend_build: format!(
                "{}+{}",
                env!("CARGO_PKG_VERSION"),
                option_env!("PULSEFLOW_BUILD").unwrap_or("OBS-LAB-2035")
            ),
            success,
            failure_reason,
            previous_receipt_hash,
            receipt_hash,
        }
    }
}

/// Stable FNV-1a evidence-link hash. It is a deterministic integrity/linkage
/// checksum, not a substitute for a cryptographic signature.
pub fn stable_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeTuning {
    pub quiet_setpoint: f64,
    pub balanced_setpoint: f64,
    pub performance_setpoint: f64,
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    pub kr: f64,
    pub residue_decay: f64,
    pub filter_alpha: f64,
    pub slew_per_sample: f64,
}

impl From<&ControlConfig> for RuntimeTuning {
    fn from(config: &ControlConfig) -> Self {
        Self {
            quiet_setpoint: config.quiet_setpoint,
            balanced_setpoint: config.balanced_setpoint,
            performance_setpoint: config.performance_setpoint,
            kp: config.kp,
            ki: config.ki,
            kd: config.kd,
            kr: config.kr,
            residue_decay: config.residue_decay,
            filter_alpha: config.filter_alpha,
            slew_per_sample: config.slew_per_sample,
        }
    }
}

impl RuntimeTuning {
    pub fn apply_to(&self, target: &mut ControlConfig) {
        target.quiet_setpoint = self.quiet_setpoint.clamp(0.10, 0.90);
        target.balanced_setpoint = self.balanced_setpoint.clamp(0.10, 0.95);
        target.performance_setpoint = self.performance_setpoint.clamp(0.10, 0.98);
        target.kp = self.kp.clamp(0.0, 2.0);
        target.ki = self.ki.clamp(0.0, 1.0);
        target.kd = self.kd.clamp(0.0, 1.0);
        target.kr = self.kr.clamp(0.0, 2.0);
        target.residue_decay = self.residue_decay.clamp(0.0, 0.999);
        target.filter_alpha = self.filter_alpha.clamp(0.01, 1.0);
        target.slew_per_sample = self.slew_per_sample.clamp(0.001, 0.25);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TuningPatch {
    pub quiet_setpoint: Option<f64>,
    pub balanced_setpoint: Option<f64>,
    pub performance_setpoint: Option<f64>,
    pub kp: Option<f64>,
    pub ki: Option<f64>,
    pub kd: Option<f64>,
    pub kr: Option<f64>,
    pub residue_decay: Option<f64>,
    pub filter_alpha: Option<f64>,
    pub slew_per_sample: Option<f64>,
}

impl RuntimeTuning {
    pub fn apply_patch(&mut self, patch: TuningPatch) {
        macro_rules! set_if {
            ($field:ident, $min:expr, $max:expr) => {
                if let Some(value) = patch.$field {
                    if value.is_finite() {
                        self.$field = value.clamp($min, $max);
                    }
                }
            };
        }
        set_if!(quiet_setpoint, 0.10, 0.90);
        set_if!(balanced_setpoint, 0.10, 0.95);
        set_if!(performance_setpoint, 0.10, 0.98);
        set_if!(kp, 0.0, 2.0);
        set_if!(ki, 0.0, 1.0);
        set_if!(kd, 0.0, 1.0);
        set_if!(kr, 0.0, 2.0);
        set_if!(residue_decay, 0.0, 0.999);
        set_if!(filter_alpha, 0.01, 1.0);
        set_if!(slew_per_sample, 0.001, 0.25);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeState {
    pub app: String,
    pub version: String,
    pub platform: String,
    pub mode: OperatingMode,
    pub learning_stage: LearningStage,
    pub target_pid: Option<u32>,
    pub target_label: String,
    pub target_revision: u64,
    pub authority_state: AuthorityState,
    pub last_valid_authority_state: AuthorityState,
    pub target_identity: Option<TargetIdentity>,
    pub verification_receipt: Option<EvidenceReceipt>,
    pub last_receipt_hash: Option<String>,
    pub failed_invariant: Option<String>,
    pub discovered_pids: Vec<u32>,
    pub agent_bound: bool,
    pub governor_active: bool,
    pub governor_supported: bool,
    pub recording: bool,
    pub experiment_id: String,
    pub epoch_revision: u64,
    pub epoch_reason: String,
    pub session_id: String,
    pub session_started_at_ms: u128,
    pub session_samples: u64,
    pub session_bytes: u64,
    pub live_sequence: u64,
    pub telemetry: Telemetry,
    pub control: ControlSnapshot,
    pub metrics: RuntimeMetrics,
    pub directive: AgentDirective,
    pub adaptive_suggestion: AdaptiveSuggestion,
    pub tuning: RuntimeTuning,
    pub tuning_revision: u64,
    pub reset_revision: u64,
    pub events: Vec<RuntimeEvent>,
    pub latest_frame: Option<ObservationFrame>,
    #[serde(skip)]
    pub history: VecDeque<ObservationFrame>,
}

impl RuntimeState {
    pub fn new(
        target_pid: Option<u32>,
        target_label: String,
        governor_supported: bool,
        tuning: RuntimeTuning,
        history_capacity: usize,
    ) -> Self {
        let active = false;
        let authority_state = if target_pid.is_some() {
            AuthorityState::Connected
        } else {
            AuthorityState::Observation
        };
        let session_id = make_session_id(&target_label);
        let experiment_id = format!("exp-{}", now_ms());
        Self {
            app: "PulseFlow Governor".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            platform: std::env::consts::OS.into(),
            mode: OperatingMode::Balanced,
            learning_stage: LearningStage::Recorder,
            target_pid,
            target_label,
            target_revision: 0,
            authority_state,
            last_valid_authority_state: authority_state,
            target_identity: target_pid.map(|pid| TargetIdentity {
                pid,
                executable: format!("pid-{pid}"),
                executable_path_hash: String::new(),
                connected_at_ms: now_ms(),
            }),
            verification_receipt: None,
            last_receipt_hash: None,
            failed_invariant: None,
            discovered_pids: Vec::new(),
            agent_bound: false,
            governor_active: active,
            governor_supported,
            recording: true,
            experiment_id,
            epoch_revision: 1,
            epoch_reason: "boot".into(),
            session_id,
            session_started_at_ms: now_ms(),
            session_samples: 0,
            session_bytes: 0,
            live_sequence: 0,
            telemetry: Telemetry::default(),
            control: ControlSnapshot::default(),
            metrics: RuntimeMetrics::default(),
            directive: AgentDirective::default(),
            adaptive_suggestion: AdaptiveSuggestion::default(),
            tuning,
            tuning_revision: 0,
            reset_revision: 0,
            events: vec![RuntimeEvent {
                timestamp_ms: now_ms(),
                kind: "boot".into(),
                message: if target_pid.is_some() {
                    "Target supplied at startup; verification is required before governance.".into()
                } else {
                    "Monitor-only observation recording enabled.".into()
                },
                evidence: None,
            }],
            latest_frame: None,
            history: VecDeque::with_capacity(history_capacity.max(60)),
        }
    }

    pub fn push_event(
        &mut self,
        kind: impl Into<String>,
        message: impl Into<String>,
    ) -> RuntimeEvent {
        let event = RuntimeEvent {
            timestamp_ms: now_ms(),
            kind: kind.into(),
            message: message.into(),
            evidence: None,
        };
        self.events.push(event.clone());
        if self.events.len() > 160 {
            let remove = self.events.len() - 160;
            self.events.drain(0..remove);
        }
        event
    }

    pub fn push_receipt(&mut self, receipt: EvidenceReceipt) -> RuntimeEvent {
        self.last_receipt_hash = Some(receipt.receipt_hash.clone());
        if receipt.success {
            self.last_valid_authority_state = receipt.resulting_state;
        }
        self.verification_receipt = Some(receipt.clone());
        let event = RuntimeEvent {
            timestamp_ms: receipt.timestamp_ms,
            kind: "authority_receipt".into(),
            message: format!(
                "{}: {:?} → {:?}",
                receipt.requested_transition, receipt.previous_state, receipt.resulting_state
            ),
            evidence: Some(receipt),
        };
        self.events.push(event.clone());
        event
    }

    pub fn authority_contract(&self) -> crate::authority::AuthorityContract {
        authority_contract(self.authority_state)
    }

    pub fn push_frame(&mut self, frame: ObservationFrame, capacity: usize) {
        self.latest_frame = Some(frame.clone());
        self.history.push_back(frame);
        let capacity = capacity.max(60);
        while self.history.len() > capacity {
            self.history.pop_front();
        }
    }

    pub fn begin_new_session(&mut self) -> RuntimeEvent {
        self.begin_new_epoch("manual_session")
    }

    pub fn begin_new_epoch(&mut self, reason: impl Into<String>) -> RuntimeEvent {
        let reason = reason.into();
        self.session_id = make_session_id(&self.target_label);
        self.epoch_revision = self.epoch_revision.saturating_add(1);
        self.epoch_reason = reason.clone();
        self.session_started_at_ms = now_ms();
        self.session_samples = 0;
        self.session_bytes = 0;
        self.metrics = RuntimeMetrics::default();
        self.control = ControlSnapshot::default();
        self.adaptive_suggestion = AdaptiveSuggestion::default();
        self.history.clear();
        self.latest_frame = None;
        self.live_sequence = 0;
        self.reset_revision = self.reset_revision.saturating_add(1);
        self.push_event(
            "experiment_epoch",
            format!(
                "Epoch {} ({}) started as session {}.",
                self.epoch_revision, reason, self.session_id
            ),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub path: String,
    pub samples: u64,
    pub bytes: u64,
    pub started_at_ms: u128,
    pub last_sample_at_ms: u128,
    pub target_label: String,
    pub schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionSummary {
    pub session_id: String,
    pub samples: u64,
    pub duration_seconds: f64,
    pub average_tokens_per_second: f64,
    pub completed_units: u64,
    pub throughput_units_per_second: f64,
    pub flow_stability: f64,
    pub thermal_oscillation_c: Option<f64>,
    pub prediction_rmse: f64,
    pub total_gpu_energy_joules: Option<f64>,
    pub estimated_tokens: f64,
    pub energy_per_token_joules: Option<f64>,
    pub average_stress: f64,
    pub average_modulation: f64,
    pub cpu_mean: f64,
    pub cpu_variance: f64,
    pub cpu_peak: f64,
    pub cpu_burst_count: u64,
    pub gpu_mean: Option<f64>,
    pub gpu_variance: Option<f64>,
    pub gpu_peak: Option<f64>,
    pub ram_pressure_mean: f64,
    pub ram_pressure_slope: f64,
    pub thermal_mean_c: Option<f64>,
    pub thermal_slope_c_per_sample: Option<f64>,
    pub thermal_peak_c: Option<f64>,
    pub latency_p95_ms: f64,
    pub queue_mean: f64,
    pub residue_memory_mean: f64,
    pub oscillation_coherence: Option<f64>,
    pub process_cpu_mean: Option<f64>,
    pub applied_modulation_mean: f64,
    pub dropped_samples: u64,
    pub target_label: String,
    pub sampling_interval_ms: Option<u64>,
    pub modes: Vec<String>,
    pub homogeneous_mode: bool,
    pub experiment_id: String,
    pub epoch_revision: u64,
    pub lyapunov_delta_total: f64,
    pub lyapunov_decrement_mean: f64,
    pub contraction_confidence: f64,
    pub marginal_fraction: f64,
    pub trigger_density_per_minute: f64,
    pub minimum_inter_event_ms: Option<u64>,
    pub ecosystem_pressure: f64,
    pub latent_pressure: f64,
    pub homeostatic_slack: f64,
    pub pressure_momentum_per_minute: f64,
    pub recovery_rate_per_second: f64,
    pub accumulation_rate_per_second: f64,
    pub recovery_balance: f64,
    pub resource_coupling: Option<f64>,
    pub recovery_half_life_seconds: Option<f64>,
    pub target_memory_share: Option<f64>,
    pub resource_momentum_per_minute: BTreeMap<String, f64>,
    pub resource_recovery_half_life_seconds: BTreeMap<String, f64>,
    pub vector_accumulation: f64,
    pub vector_dissipation: f64,
    pub pressure_transduction: f64,
    pub net_vector_pressure: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LearningGraphPoint {
    pub offset_ms: u64,
    pub cpu: f64,
    pub ram: f64,
    pub gpu: Option<f64>,
    pub thermal: Option<f64>,
    pub stress: f64,
    pub ecosystem_pressure: f64,
    pub latent_pressure: f64,
    pub homeostatic_slack: f64,
    pub recovery_balance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LearningDataset {
    pub schema_version: String,
    pub iteration_id: String,
    pub created_at_ms: u128,
    pub app_version: String,
    pub source_schema_version: String,
    pub raw_checksum: String,
    pub raw_bytes: u64,
    pub summary: SessionSummary,
    pub points: Vec<LearningGraphPoint>,
    pub discoveries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LearningDatasetInfo {
    pub iteration_id: String,
    pub created_at_ms: u128,
    pub app_version: String,
    pub samples: u64,
    pub duration_seconds: f64,
    pub points: u64,
    pub raw_bytes_reclaimed: u64,
    pub ecosystem_pressure: f64,
    pub latent_pressure: f64,
    pub homeostatic_slack: f64,
    pub pressure_transduction: f64,
    pub net_vector_pressure: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComparisonReport {
    pub baseline: SessionSummary,
    pub candidate: SessionSummary,
    pub intervention_value_tokens_per_second: f64,
    pub throughput_delta_percent: Option<f64>,
    pub flow_stability_delta: f64,
    pub thermal_oscillation_delta_c: Option<f64>,
    pub energy_per_token_delta_percent: Option<f64>,
    pub prediction_rmse_delta: f64,
    pub comparable: bool,
    pub evidence_quality: f64,
    pub verdict: String,
    pub invalid_reasons: Vec<String>,
    pub cpu_mean_delta: f64,
    pub cpu_variance_delta: f64,
    pub cpu_peak_delta: f64,
    pub latency_p95_delta_ms: f64,
    pub queue_mean_delta: f64,
    pub coherence_delta: Option<f64>,
    pub intervention_cost: f64,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayReport {
    pub session_id: String,
    pub samples_replayed: u64,
    pub baseline_prediction_rmse: f64,
    pub candidate_prediction_rmse: f64,
    pub baseline_flow_stability: f64,
    pub candidate_modulation_mean: f64,
    pub candidate_modulation_stddev: f64,
    pub candidate_eco_requests: u64,
    pub candidate_normal_requests: u64,
    pub candidate_responsive_requests: u64,
    pub candidate_thermal_requests: u64,
    pub note: String,
}

pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub fn make_session_id(label: &str) -> String {
    let slug: String = label
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character.to_ascii_lowercase())
            } else if character == '-' || character == '_' || character.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .take(32)
        .collect();
    let slug = slug.trim_matches('-');
    let prefix = if slug.is_empty() { "pulseflow" } else { slug };
    format!("{prefix}-{}", now_ms())
}

pub fn safe_session_id(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 128 {
        return None;
    }
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        Some(value.to_string())
    } else {
        None
    }
}
