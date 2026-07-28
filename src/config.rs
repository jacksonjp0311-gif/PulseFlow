use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub bind: String,
    pub sample_interval_ms: u64,
    pub signal_stale_after_ms: u64,
    pub event_ledger_path: String,
    pub storage: StorageConfig,
    pub governor: GovernorConfig,
    pub control: ControlConfig,
    pub weights: StressWeights,
    pub analytics: AnalyticsConfig,
    pub agent_policy: AgentPolicyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub enabled: bool,
    pub directory: String,
    pub recent_history_capacity: usize,
    pub maximum_query_samples: usize,
    pub metadata_flush_every_samples: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernorConfig {
    pub minimum_dwell_ms: u64,
    pub thermal_guard_c: f64,
    pub thermal_release_c: f64,
    pub allow_above_normal_priority: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlConfig {
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
    pub eco_enter: f64,
    pub eco_exit: f64,
    pub responsive_enter: f64,
    pub responsive_exit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressWeights {
    pub cpu: f64,
    pub memory: f64,
    pub gpu_utilization: f64,
    pub gpu_temperature: f64,
    pub io_pressure: f64,
    pub latency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsConfig {
    pub rolling_window_samples: usize,
    pub epsilon: f64,
    #[serde(default = "default_minimum_activity_stddev")]
    pub minimum_activity_stddev: f64,
    #[serde(default = "default_pressure_limit")]
    pub pressure_limit: f64,
    #[serde(default = "default_queue_limit")]
    pub queue_limit: f64,
    #[serde(default = "default_thermal_drift_limit")]
    pub thermal_drift_limit_c_per_sample: f64,
    #[serde(default = "default_residue_limit")]
    pub residue_limit: f64,
    #[serde(default = "default_forecast_horizon")]
    pub forecast_horizon_samples: usize,
    #[serde(default = "default_minimum_comparison_samples")]
    pub minimum_comparison_samples: u64,
}

fn default_minimum_activity_stddev() -> f64 {
    0.01
}
fn default_pressure_limit() -> f64 {
    0.75
}
fn default_queue_limit() -> f64 {
    48.0
}
fn default_thermal_drift_limit() -> f64 {
    0.15
}
fn default_residue_limit() -> f64 {
    0.25
}
fn default_forecast_horizon() -> usize {
    5
}
fn default_minimum_comparison_samples() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPolicyConfig {
    pub maximum_concurrency: u32,
    pub maximum_batch_size: u32,
    pub minimum_batch_size: u32,
    pub allow_bounded_adaptation: bool,
    pub minimum_samples_before_adaptation: u64,
    pub adaptation_interval_samples: u64,
    pub maximum_gain_step: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8791".into(),
            sample_interval_ms: 1_000,
            signal_stale_after_ms: 10_000,
            event_ledger_path: "state/pulseflow-events.jsonl".into(),
            storage: StorageConfig {
                enabled: true,
                directory: "state/sessions".into(),
                recent_history_capacity: 3_600,
                maximum_query_samples: 10_000,
                metadata_flush_every_samples: 10,
            },
            governor: GovernorConfig {
                minimum_dwell_ms: 8_000,
                thermal_guard_c: 82.0,
                thermal_release_c: 76.0,
                allow_above_normal_priority: false,
            },
            control: ControlConfig {
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
            },
            weights: StressWeights {
                cpu: 0.30,
                memory: 0.14,
                gpu_utilization: 0.22,
                gpu_temperature: 0.18,
                io_pressure: 0.10,
                latency: 0.06,
            },
            analytics: AnalyticsConfig {
                rolling_window_samples: 300,
                epsilon: 0.000_001,
                minimum_activity_stddev: default_minimum_activity_stddev(),
                pressure_limit: default_pressure_limit(),
                queue_limit: default_queue_limit(),
                thermal_drift_limit_c_per_sample: default_thermal_drift_limit(),
                residue_limit: default_residue_limit(),
                forecast_horizon_samples: default_forecast_horizon(),
                minimum_comparison_samples: default_minimum_comparison_samples(),
            },
            agent_policy: AgentPolicyConfig {
                maximum_concurrency: 16,
                maximum_batch_size: 512,
                minimum_batch_size: 1,
                allow_bounded_adaptation: false,
                minimum_samples_before_adaptation: 300,
                adaptation_interval_samples: 30,
                maximum_gain_step: 0.01,
            },
        }
    }
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        serde_json::from_str(&text).map_err(|error| format!("invalid {}: {error}", path.display()))
    }
}
