//! Futurist Governor: multi-horizon pressure foresight.
//!
//! Advisory only. Forecasts never widen process authority, skip discovery,
//! or auto-enable QoS. They inform agent envelopes and operator UI.

use crate::analytics::{forecast_trajectory, mean, stddev};
use crate::model::{ObservationFrame, Telemetry};
use serde::{Deserialize, Serialize};

pub const HORIZONS: [usize; 3] = [1, 5, 15];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HorizonForecast {
    pub horizon_samples: u32,
    pub forecast: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelForecast {
    pub channel: String,
    pub unit: String,
    pub current: f64,
    pub trend_per_sample: f64,
    pub horizons: Vec<HorizonForecast>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FuturistSnapshot {
    pub schema_version: String,
    pub calibrated: bool,
    pub envelope: String,
    pub reason: String,
    pub pressure_risk: f64,
    pub channels: Vec<ChannelForecast>,
    pub skill: FuturistSkill,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FuturistSkill {
    pub samples_scored: u64,
    pub mae_h5: f64,
    pub mae_persist_h5: f64,
    pub relative_improvement: f64,
    pub beats_persist: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FuturistCalibration {
    pub schema_version: String,
    pub session_id: String,
    pub skill: FuturistSkill,
    pub channel: String,
}

/// Build a live futurist snapshot from rolling stress / RAM / ecosystem series.
pub fn snapshot_from_series(
    stress: &[f64],
    ram_percent: &[f64],
    ecosystem_pressure: &[f64],
    gpu_temperature_c: Option<f64>,
    pressure_limit: f64,
    epsilon: f64,
) -> FuturistSnapshot {
    let mut channels = Vec::new();
    if let Some(channel) = channel_forecast("stress", "normalized", stress, epsilon) {
        channels.push(channel);
    }
    let ram_norm: Vec<f64> = ram_percent
        .iter()
        .map(|v| (*v / 100.0).clamp(0.0, 1.0))
        .collect();
    if let Some(channel) = channel_forecast("ram", "normalized", &ram_norm, epsilon) {
        channels.push(channel);
    }
    if let Some(channel) = channel_forecast(
        "ecosystem_pressure",
        "normalized",
        ecosystem_pressure,
        epsilon,
    ) {
        channels.push(channel);
    }

    let stress_h5 = channels
        .iter()
        .find(|c| c.channel == "stress")
        .and_then(|c| c.horizons.iter().find(|h| h.horizon_samples == 5))
        .map(|h| h.forecast)
        .unwrap_or_else(|| stress.last().copied().unwrap_or(0.0));
    let ram_h5 = channels
        .iter()
        .find(|c| c.channel == "ram")
        .and_then(|c| c.horizons.iter().find(|h| h.horizon_samples == 5))
        .map(|h| h.forecast)
        .unwrap_or_else(|| ram_norm.last().copied().unwrap_or(0.0));
    let eco_h5 = channels
        .iter()
        .find(|c| c.channel == "ecosystem_pressure")
        .and_then(|c| c.horizons.iter().find(|h| h.horizon_samples == 5))
        .map(|h| h.forecast)
        .unwrap_or_else(|| ecosystem_pressure.last().copied().unwrap_or(0.0));

    let pressure_risk =
        ((stress_h5.max(eco_h5) - pressure_limit) / (1.0 - pressure_limit + 1e-9)).clamp(0.0, 1.0);
    let thermal_watch = gpu_temperature_c.is_some_and(|t| t >= 78.0);
    let (envelope, reason) = if thermal_watch {
        (
            "thermal_watch".into(),
            "GPU temperature is elevated; hold efficiency bias and avoid new thermal load.".into(),
        )
    } else if ram_h5 >= 0.92 || eco_h5 >= 0.85 {
        (
            "contract_agent".into(),
            "Futurist projects critical memory/ecosystem pressure within the short horizon.".into(),
        )
    } else if stress_h5 >= pressure_limit || ram_h5 >= 0.85 || pressure_risk >= 0.35 {
        (
            "suggest_eco".into(),
            "Futurist projects rising pressure; Eco QoS and contracted agent work are suggested."
                .into(),
        )
    } else {
        (
            "hold".into(),
            "Projected pressure remains inside the safe envelope.".into(),
        )
    };

    FuturistSnapshot {
        schema_version: "pulseflow.futurist.v1".into(),
        calibrated: false,
        envelope,
        reason,
        pressure_risk,
        channels,
        skill: FuturistSkill::default(),
    }
}

fn channel_forecast(
    name: &str,
    unit: &str,
    values: &[f64],
    epsilon: f64,
) -> Option<ChannelForecast> {
    if values.len() < 8 {
        return None;
    }
    let current = *values.last()?;
    let mut horizons = Vec::new();
    let mut trend = 0.0;
    for horizon in HORIZONS {
        let (forecast, slope, confidence) = forecast_trajectory(values, horizon, epsilon);
        trend = slope;
        if let Some(forecast) = forecast {
            horizons.push(HorizonForecast {
                horizon_samples: horizon as u32,
                forecast,
                confidence,
            });
        }
    }
    if horizons.is_empty() {
        return None;
    }
    Some(ChannelForecast {
        channel: name.into(),
        unit: unit.into(),
        current,
        trend_per_sample: trend,
        horizons,
    })
}

/// Score linear H=5 forecasts against a held-out stress series.
pub fn score_stress_skill(stress: &[f64], epsilon: f64) -> FuturistSkill {
    if stress.len() < 20 {
        return FuturistSkill::default();
    }
    let horizon = 5usize;
    let mut abs_err = Vec::new();
    let mut persist_err = Vec::new();
    for end in 12..stress.len().saturating_sub(horizon) {
        let window = &stress[end.saturating_sub(60)..end];
        let (forecast, _, _) = forecast_trajectory(window, horizon, epsilon);
        let Some(forecast) = forecast else {
            continue;
        };
        let actual = stress[end + horizon - 1];
        let persist = stress[end - 1];
        abs_err.push((forecast - actual).abs());
        persist_err.push((persist - actual).abs());
    }
    if abs_err.is_empty() {
        return FuturistSkill::default();
    }
    let mae_h5 = mean(&abs_err);
    let mae_persist_h5 = mean(&persist_err);
    let relative_improvement = if mae_persist_h5 > epsilon {
        ((mae_persist_h5 - mae_h5) / mae_persist_h5).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    FuturistSkill {
        samples_scored: abs_err.len() as u64,
        mae_h5,
        mae_persist_h5,
        relative_improvement,
        beats_persist: relative_improvement >= 0.10,
    }
}

pub fn calibrate_session(
    session_id: &str,
    frames: &[ObservationFrame],
    epsilon: f64,
) -> FuturistCalibration {
    let stress: Vec<f64> = frames
        .iter()
        .map(|frame| frame.controller.filtered_stress)
        .collect();
    FuturistCalibration {
        schema_version: "pulseflow.futurist-calibration.v1".into(),
        session_id: session_id.into(),
        skill: score_stress_skill(&stress, epsilon),
        channel: "stress".into(),
    }
}

/// Envelope assist from the latest telemetry when history is short.
pub fn bootstrap_from_telemetry(telemetry: &Telemetry, pressure_limit: f64) -> FuturistSnapshot {
    let stress = (telemetry.cpu_percent / 100.0 * 0.5 + telemetry.memory_percent / 100.0 * 0.5)
        .clamp(0.0, 1.0);
    let risk = ((stress - pressure_limit) / (1.0 - pressure_limit + 1e-9)).clamp(0.0, 1.0);
    let (envelope, reason) = if telemetry.memory_percent >= 92.0 {
        (
            "contract_agent".into(),
            "Live RAM is critical; contract agent pressure immediately.".into(),
        )
    } else if telemetry.memory_percent >= 85.0 || stress >= pressure_limit {
        (
            "suggest_eco".into(),
            "Live pressure is elevated; Eco assist and agent contraction suggested.".into(),
        )
    } else {
        (
            "hold".into(),
            "Insufficient history; holding on live telemetry.".into(),
        )
    };
    FuturistSnapshot {
        schema_version: "pulseflow.futurist.v1".into(),
        calibrated: false,
        envelope,
        reason,
        pressure_risk: risk,
        channels: vec![ChannelForecast {
            channel: "bootstrap".into(),
            unit: "normalized".into(),
            current: stress,
            trend_per_sample: 0.0,
            horizons: vec![HorizonForecast {
                horizon_samples: 1,
                forecast: stress,
                confidence: 0.2,
            }],
        }],
        skill: FuturistSkill::default(),
    }
}

/// Residual scale used by tests.
pub fn series_activity(values: &[f64]) -> f64 {
    stddev(values)
}
