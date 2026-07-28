use crate::{
    config::AnalyticsConfig,
    model::{ObservationFrame, QosLevel, RuntimeMetrics},
};
use std::collections::VecDeque;

#[derive(Debug, Clone)]
struct MetricSample {
    latency_ms: f64,
    tokens_per_second: f64,
    queue: f64,
    temperature_c: Option<f64>,
    raw_stress: f64,
    filtered_stress: f64,
    setpoint: f64,
    residue: f64,
    applied_modulation: f64,
    timestamp_ms: u128,
    applied_qos: QosLevel,
}

pub struct AnalyticsEngine {
    config: AnalyticsConfig,
    window: VecDeque<MetricSample>,
    samples: u64,
    elapsed_seconds: f64,
    cumulative_energy_joules: f64,
    energy_observed: bool,
    cumulative_estimated_tokens: f64,
    completed_units: u64,
}

impl AnalyticsEngine {
    pub fn new(config: AnalyticsConfig) -> Self {
        Self {
            config,
            window: VecDeque::new(),
            samples: 0,
            elapsed_seconds: 0.0,
            cumulative_energy_joules: 0.0,
            energy_observed: false,
            cumulative_estimated_tokens: 0.0,
            completed_units: 0,
        }
    }

    pub fn reset(&mut self) {
        self.window.clear();
        self.samples = 0;
        self.elapsed_seconds = 0.0;
        self.cumulative_energy_joules = 0.0;
        self.energy_observed = false;
        self.cumulative_estimated_tokens = 0.0;
        self.completed_units = 0;
    }

    pub fn update(&mut self, frame: &ObservationFrame, dt_seconds: f64) -> RuntimeMetrics {
        let dt = dt_seconds.clamp(0.001, 60.0);
        self.samples = self.samples.saturating_add(1);
        self.elapsed_seconds += dt;
        self.completed_units = self
            .completed_units
            .saturating_add(frame.outcome.completed_units);
        self.cumulative_estimated_tokens += frame.outcome.estimated_tokens_this_interval.max(0.0);
        if let Some(power_w) = frame
            .machine
            .gpu_power_w
            .filter(|value| value.is_finite() && *value >= 0.0)
        {
            self.cumulative_energy_joules += power_w * dt;
            self.energy_observed = true;
        }

        self.window.push_back(MetricSample {
            latency_ms: frame.outcome.latency_ms.max(0.0),
            tokens_per_second: frame.outcome.tokens_per_second.max(0.0),
            queue: (frame.workload.input_queue + frame.workload.output_queue) as f64,
            temperature_c: frame.machine.gpu_temperature_c,
            raw_stress: frame.controller.raw_stress,
            filtered_stress: frame.controller.filtered_stress,
            setpoint: frame.controller.setpoint,
            residue: frame.controller.residue,
            applied_modulation: frame.controller.applied_modulation,
            timestamp_ms: frame.timestamp_ms,
            applied_qos: frame.controller.applied_qos,
        });
        let capacity = self.config.rolling_window_samples.max(8);
        while self.window.len() > capacity {
            self.window.pop_front();
        }

        let latencies: Vec<f64> = self
            .window
            .iter()
            .filter_map(|sample| (sample.latency_ms > 0.0).then_some(sample.latency_ms))
            .collect();
        let flow_series: Vec<f64> = if latencies.len() >= 3 {
            latencies.clone()
        } else {
            self.window
                .iter()
                .map(|sample| sample.filtered_stress)
                .collect()
        };
        let temperatures: Vec<f64> = self
            .window
            .iter()
            .filter_map(|sample| sample.temperature_c)
            .collect();
        let residues: Vec<f64> = self.window.iter().map(|sample| sample.residue).collect();
        let tokens: Vec<f64> = self
            .window
            .iter()
            .map(|sample| sample.tokens_per_second)
            .collect();
        let queues: Vec<f64> = self.window.iter().map(|sample| sample.queue).collect();
        let raw_stresses: Vec<f64> = self.window.iter().map(|sample| sample.raw_stress).collect();
        let stresses: Vec<f64> = self
            .window
            .iter()
            .map(|sample| sample.filtered_stress)
            .collect();
        let modulations: Vec<f64> = self
            .window
            .iter()
            .map(|sample| sample.applied_modulation)
            .collect();

        let flow_mean = mean(&flow_series);
        let flow_stability = if flow_series.len() < 2 {
            1.0
        } else {
            (1.0 - stddev(&flow_series) / (flow_mean.abs() + self.config.epsilon.max(1e-12)))
                .clamp(0.0, 1.0)
        };
        let prediction_rmse = if residues.is_empty() {
            0.0
        } else {
            (residues.iter().map(|value| value * value).sum::<f64>() / residues.len() as f64).sqrt()
        };
        let sample_confidence = (self.samples as f64 / 300.0).clamp(0.0, 1.0);
        let policy_confidence =
            (sample_confidence * (1.0 - prediction_rmse.clamp(0.0, 1.0))).clamp(0.0, 1.0);
        let oscillation_coherence =
            oscillation_coherence(&raw_stresses, &stresses, self.config.epsilon);
        let (forecast_stress, forecast_trend, forecast_confidence) = forecast_trajectory(
            &stresses,
            self.config.forecast_horizon_samples,
            self.config.epsilon,
        );
        let thermal_slope = linear_slope(&temperatures);
        let pressure_risk = forecast_stress
            .map(|forecast| {
                ((forecast - self.config.pressure_limit)
                    / (1.0 - self.config.pressure_limit).max(0.01))
                .clamp(0.0, 1.0)
            })
            .unwrap_or(0.0);
        let turbulence_state = classify_turbulence(
            &stresses,
            mean(&queues),
            thermal_slope,
            mean(&residues.iter().map(|value| value.abs()).collect::<Vec<_>>()),
            mean(&modulations),
            &self.config,
        );
        let pulse_metrics = pulse_feedback_metrics(
            &self
                .window
                .iter()
                .map(|sample| {
                    (
                        sample.timestamp_ms,
                        sample.filtered_stress,
                        sample.setpoint,
                        sample.applied_qos,
                    )
                })
                .collect::<Vec<_>>(),
        );

        RuntimeMetrics {
            samples: self.samples,
            elapsed_seconds: self.elapsed_seconds,
            average_tokens_per_second: mean(&tokens),
            completed_units: self.completed_units,
            throughput_units_per_second: if self.elapsed_seconds > 0.0 {
                self.completed_units as f64 / self.elapsed_seconds
            } else {
                0.0
            },
            flow_stability,
            thermal_oscillation_c: (!temperatures.is_empty()).then(|| stddev(&temperatures)),
            prediction_rmse,
            cumulative_gpu_energy_joules: self
                .energy_observed
                .then_some(self.cumulative_energy_joules),
            energy_per_token_joules: if self.energy_observed
                && self.cumulative_estimated_tokens > 0.0
            {
                Some(self.cumulative_energy_joules / self.cumulative_estimated_tokens)
            } else {
                None
            },
            intervention_value: None,
            policy_confidence,
            latency_mean_ms: mean(&latencies),
            latency_p95_ms: percentile(&latencies, 0.95),
            queue_mean: mean(&queues),
            stress_mean: mean(&stresses),
            oscillation_coherence,
            turbulence_state,
            forecast_stress,
            forecast_trend_per_sample: forecast_trend,
            forecast_confidence,
            forecast_pressure_risk: pressure_risk,
            lyapunov_delta_total: pulse_metrics.lyapunov_delta_total,
            lyapunov_decrement_mean: pulse_metrics.lyapunov_decrement_mean,
            contraction_confidence: pulse_metrics.contraction_confidence,
            marginal_fraction: pulse_metrics.marginal_fraction,
            trigger_density_per_minute: pulse_metrics.trigger_density_per_minute,
            minimum_inter_event_ms: pulse_metrics.minimum_inter_event_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PulseFeedbackMetrics {
    lyapunov_delta_total: f64,
    lyapunov_decrement_mean: f64,
    contraction_confidence: f64,
    marginal_fraction: f64,
    trigger_density_per_minute: f64,
    minimum_inter_event_ms: Option<u64>,
}

fn pulse_feedback_metrics(samples: &[(u128, f64, f64, QosLevel)]) -> PulseFeedbackMetrics {
    if samples.len() < 2 {
        return PulseFeedbackMetrics::default();
    }
    let deltas: Vec<f64> = samples
        .windows(2)
        .map(|window| {
            let before = 0.5 * (window[0].1 - window[0].2).powi(2);
            let after = 0.5 * (window[1].1 - window[1].2).powi(2);
            after - before
        })
        .collect();
    let contraction_count = deltas.iter().filter(|delta| **delta < 0.0).count();
    let marginal_tolerance = 0.0005;
    let marginal_count = deltas
        .iter()
        .filter(|delta| delta.abs() <= marginal_tolerance)
        .count();
    let transition_times: Vec<u128> = samples
        .windows(2)
        .filter_map(|window| (window[0].3 != window[1].3).then_some(window[1].0))
        .collect();
    let elapsed_minutes = samples
        .last()
        .map(|last| last.0.saturating_sub(samples[0].0) as f64 / 60_000.0)
        .unwrap_or(0.0);
    let minimum_inter_event_ms = transition_times
        .windows(2)
        .map(|window| window[1].saturating_sub(window[0]) as u64)
        .min();
    let lyapunov_delta_total = deltas.iter().sum::<f64>();
    PulseFeedbackMetrics {
        lyapunov_delta_total,
        lyapunov_decrement_mean: -lyapunov_delta_total / deltas.len() as f64,
        contraction_confidence: contraction_count as f64 / deltas.len() as f64,
        marginal_fraction: marginal_count as f64 / deltas.len() as f64,
        trigger_density_per_minute: if elapsed_minutes > 0.0 {
            transition_times.len() as f64 / elapsed_minutes
        } else {
            0.0
        },
        minimum_inter_event_ms,
    }
}

pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

pub fn stddev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let average = mean(values);
    let variance = values
        .iter()
        .map(|value| {
            let delta = value - average;
            delta * delta
        })
        .sum::<f64>()
        / values.len() as f64;
    variance.sqrt()
}

pub fn variance(values: &[f64]) -> f64 {
    let deviation = stddev(values);
    deviation * deviation
}

pub fn linear_slope(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let x_mean = (values.len() - 1) as f64 / 2.0;
    let y_mean = mean(values);
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for (index, value) in values.iter().enumerate() {
        let x = index as f64 - x_mean;
        numerator += x * (*value - y_mean);
        denominator += x * x;
    }
    if denominator <= f64::EPSILON {
        0.0
    } else {
        numerator / denominator
    }
}

/// Normalized trajectory agreement over aligned rolling windows.
///
/// The traces are demeaned, then their normalized RMSE is converted to
/// agreement in [0, 1]. Identical constant traces are coherent; a constant and
/// a varying trace are not. Windows shorter than eight samples are incomplete.
pub fn oscillation_coherence(raw: &[f64], filtered: &[f64], epsilon: f64) -> Option<f64> {
    let length = raw.len().min(filtered.len());
    if length < 8 {
        return None;
    }
    let raw = &raw[raw.len() - length..];
    let filtered = &filtered[filtered.len() - length..];
    if raw
        .iter()
        .chain(filtered.iter())
        .any(|value| !value.is_finite())
    {
        return None;
    }
    let raw_mean = mean(raw);
    let filtered_mean = mean(filtered);
    let raw_std = stddev(raw);
    let filtered_std = stddev(filtered);
    let epsilon = epsilon.max(1e-12);
    if raw_std <= epsilon && filtered_std <= epsilon {
        return Some(if (raw_mean - filtered_mean).abs() <= epsilon {
            1.0
        } else {
            0.0
        });
    }
    if raw_std <= epsilon || filtered_std <= epsilon {
        return Some(0.0);
    }
    let mse = raw
        .iter()
        .zip(filtered)
        .map(|(left, right)| {
            let delta = (left - raw_mean) - (right - filtered_mean);
            delta * delta
        })
        .sum::<f64>()
        / length as f64;
    let scale = raw_std + filtered_std + epsilon;
    Some((1.0 - mse.sqrt() / scale).clamp(0.0, 1.0))
}

pub fn forecast_trajectory(
    values: &[f64],
    horizon_samples: usize,
    epsilon: f64,
) -> (Option<f64>, f64, f64) {
    if values.len() < 8 {
        return (None, 0.0, 0.0);
    }
    let window = &values[values.len().saturating_sub(60)..];
    let slope = linear_slope(window).clamp(-0.20, 0.20);
    let intercept = mean(window) - slope * ((window.len() - 1) as f64 / 2.0);
    let fitted: Vec<f64> = (0..window.len())
        .map(|index| intercept + slope * index as f64)
        .collect();
    let errors: Vec<f64> = window
        .iter()
        .zip(&fitted)
        .map(|(actual, expected)| actual - expected)
        .collect();
    let confidence = (1.0 - stddev(&errors) / (stddev(window) + epsilon.max(1e-12)))
        .clamp(0.0, 1.0)
        * (window.len() as f64 / 60.0).clamp(0.0, 1.0);
    let forecast =
        (window[window.len() - 1] + slope * horizon_samples.max(1) as f64).clamp(0.0, 1.0);
    (Some(forecast), slope, confidence)
}

fn classify_turbulence(
    stresses: &[f64],
    queue_mean: f64,
    thermal_slope: f64,
    residue_mean: f64,
    applied_modulation_mean: f64,
    config: &AnalyticsConfig,
) -> String {
    if stresses.len() < 8 {
        return "insufficient_data".into();
    }
    let activity = stddev(stresses);
    let pressure = mean(stresses);
    if pressure >= 0.95 || queue_mean >= config.queue_limit * 1.5 {
        return "saturated".into();
    }
    if pressure >= config.pressure_limit
        || queue_mean >= config.queue_limit
        || thermal_slope > config.thermal_drift_limit_c_per_sample
        || residue_mean > config.residue_limit
    {
        return "pressure_building".into();
    }
    if activity < config.minimum_activity_stddev {
        return "quiescent".into();
    }
    if applied_modulation_mean <= 1.0
        && thermal_slope.abs() <= config.thermal_drift_limit_c_per_sample
    {
        "controlled_turbulence".into()
    } else {
        "unstable".into()
    }
}

pub fn percentile(values: &[f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let index = ((sorted.len() - 1) as f64 * fraction.clamp(0.0, 1.0)).round() as usize;
    sorted[index]
}

pub fn summarize_session(
    session_id: &str,
    frames: &[ObservationFrame],
    epsilon: f64,
) -> crate::model::SessionSummary {
    use crate::model::SessionSummary;

    if frames.is_empty() {
        return SessionSummary {
            session_id: session_id.to_string(),
            ..SessionSummary::default()
        };
    }

    let tokens_per_second: Vec<f64> = frames
        .iter()
        .map(|frame| frame.outcome.tokens_per_second.max(0.0))
        .collect();
    let latencies: Vec<f64> = frames
        .iter()
        .filter_map(|frame| {
            (frame.outcome.latency_ms.is_finite() && frame.outcome.latency_ms > 0.0)
                .then_some(frame.outcome.latency_ms)
        })
        .collect();
    let stresses: Vec<f64> = frames
        .iter()
        .map(|frame| frame.controller.filtered_stress)
        .collect();
    let flow_series = if latencies.len() >= 3 {
        &latencies
    } else {
        &stresses
    };
    let temperatures: Vec<f64> = frames
        .iter()
        .filter_map(|frame| frame.machine.gpu_temperature_c)
        .filter(|value| value.is_finite())
        .collect();
    let residues: Vec<f64> = frames
        .iter()
        .map(|frame| frame.controller.residue)
        .filter(|value| value.is_finite())
        .collect();
    let modulations: Vec<f64> = frames
        .iter()
        .map(|frame| frame.controller.applied_modulation)
        .filter(|value| value.is_finite())
        .collect();
    let raw_stresses: Vec<f64> = frames
        .iter()
        .map(|frame| frame.controller.raw_stress)
        .filter(|value| value.is_finite())
        .collect();
    let cpus: Vec<f64> = frames
        .iter()
        .map(|frame| frame.machine.cpu_percent)
        .filter(|value| value.is_finite())
        .collect();
    let gpus: Vec<f64> = frames
        .iter()
        .filter_map(|frame| frame.machine.gpu_percent)
        .filter(|value| value.is_finite())
        .collect();
    let ram_pressures: Vec<f64> = frames
        .iter()
        .map(|frame| frame.machine.ram_percent)
        .filter(|value| value.is_finite())
        .collect();
    let queues: Vec<f64> = frames
        .iter()
        .map(|frame| (frame.workload.input_queue + frame.workload.output_queue) as f64)
        .collect();
    let residue_memories: Vec<f64> = frames
        .iter()
        .map(|frame| frame.controller.residue_memory.abs())
        .filter(|value| value.is_finite())
        .collect();
    let process_cpus: Vec<f64> = frames
        .iter()
        .filter_map(|frame| frame.machine.process_cpu_percent)
        .filter(|value| value.is_finite())
        .collect();

    let start_ms = frames
        .first()
        .map(|frame| frame.timestamp_ms)
        .unwrap_or_default();
    let end_ms = frames
        .last()
        .map(|frame| frame.outcome.observed_at_ms.max(frame.timestamp_ms))
        .unwrap_or(start_ms);
    let duration_seconds = (end_ms.saturating_sub(start_ms) as f64 / 1_000.0).max(0.0);

    let completed_units = frames.iter().fold(0u64, |sum, frame| {
        sum.saturating_add(frame.outcome.completed_units)
    });
    let estimated_tokens = frames
        .iter()
        .map(|frame| frame.outcome.estimated_tokens_this_interval.max(0.0))
        .sum::<f64>();

    let mut total_gpu_energy_joules = 0.0;
    let mut energy_samples = 0u64;
    for frame in frames {
        if let Some(power_w) = frame
            .machine
            .gpu_power_w
            .filter(|value| value.is_finite() && *value >= 0.0)
        {
            let horizon_seconds = (frame.outcome.horizon_ms as f64 / 1_000.0).clamp(0.0, 60.0);
            if horizon_seconds > 0.0 {
                total_gpu_energy_joules += power_w * horizon_seconds;
                energy_samples = energy_samples.saturating_add(1);
            }
        }
    }

    let flow_mean = mean(flow_series);
    let flow_stability = if flow_series.len() < 2 {
        1.0
    } else {
        (1.0 - stddev(flow_series) / (flow_mean.abs() + epsilon.max(1e-12))).clamp(0.0, 1.0)
    };
    let prediction_rmse = if residues.is_empty() {
        0.0
    } else {
        (residues.iter().map(|value| value * value).sum::<f64>() / residues.len() as f64).sqrt()
    };
    let energy = (energy_samples > 0).then_some(total_gpu_energy_joules);
    let cpu_mean = mean(&cpus);
    let burst_threshold = cpu_mean + 2.0 * stddev(&cpus);
    let cpu_burst_count = cpus
        .windows(2)
        .filter(|window| window[0] <= burst_threshold && window[1] > burst_threshold)
        .count() as u64;
    let dropped_samples = frames
        .windows(2)
        .map(|window| {
            window[1]
                .sequence
                .saturating_sub(window[0].sequence)
                .saturating_sub(1)
        })
        .sum();
    let intervals: Vec<f64> = frames
        .windows(2)
        .filter_map(|window| {
            let delta = window[1]
                .timestamp_ms
                .saturating_sub(window[0].timestamp_ms);
            (delta > 0).then_some(delta as f64)
        })
        .collect();
    let target_label = session_id
        .rsplit_once('-')
        .map(|(prefix, _)| prefix)
        .unwrap_or(session_id)
        .to_string();
    let pulse_metrics = pulse_feedback_metrics(
        &frames
            .iter()
            .map(|frame| {
                (
                    frame.timestamp_ms,
                    frame.controller.filtered_stress,
                    frame.controller.setpoint,
                    frame.controller.applied_qos,
                )
            })
            .collect::<Vec<_>>(),
    );
    let mut modes: Vec<String> = frames
        .iter()
        .map(|frame| format!("{:?}", frame.action.mode).to_lowercase())
        .collect();
    modes.sort();
    modes.dedup();
    let experiment_id = frames
        .first()
        .map(|frame| frame.experiment_id.clone())
        .unwrap_or_default();
    let epoch_revision = frames
        .first()
        .map(|frame| frame.epoch_revision)
        .unwrap_or_default();

    SessionSummary {
        session_id: session_id.to_string(),
        samples: frames.len() as u64,
        duration_seconds,
        average_tokens_per_second: mean(&tokens_per_second),
        completed_units,
        throughput_units_per_second: if duration_seconds > 0.0 {
            completed_units as f64 / duration_seconds
        } else {
            0.0
        },
        flow_stability,
        thermal_oscillation_c: (!temperatures.is_empty()).then(|| stddev(&temperatures)),
        prediction_rmse,
        total_gpu_energy_joules: energy,
        estimated_tokens,
        energy_per_token_joules: energy
            .and_then(|joules| (estimated_tokens > 0.0).then_some(joules / estimated_tokens)),
        average_stress: mean(&stresses),
        average_modulation: mean(&modulations),
        cpu_mean,
        cpu_variance: variance(&cpus),
        cpu_peak: cpus.iter().copied().fold(0.0, f64::max),
        cpu_burst_count,
        gpu_mean: (!gpus.is_empty()).then(|| mean(&gpus)),
        gpu_variance: (!gpus.is_empty()).then(|| variance(&gpus)),
        gpu_peak: (!gpus.is_empty()).then(|| gpus.iter().copied().fold(0.0, f64::max)),
        ram_pressure_mean: mean(&ram_pressures),
        ram_pressure_slope: linear_slope(&ram_pressures),
        thermal_mean_c: (!temperatures.is_empty()).then(|| mean(&temperatures)),
        thermal_slope_c_per_sample: (!temperatures.is_empty()).then(|| linear_slope(&temperatures)),
        thermal_peak_c: (!temperatures.is_empty()).then(|| {
            temperatures
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max)
        }),
        latency_p95_ms: percentile(&latencies, 0.95),
        queue_mean: mean(&queues),
        residue_memory_mean: mean(&residue_memories),
        oscillation_coherence: oscillation_coherence(&raw_stresses, &stresses, epsilon),
        process_cpu_mean: (!process_cpus.is_empty()).then(|| mean(&process_cpus)),
        applied_modulation_mean: mean(&modulations),
        dropped_samples,
        target_label,
        sampling_interval_ms: (!intervals.is_empty()).then(|| mean(&intervals).round() as u64),
        homogeneous_mode: modes.len() <= 1,
        modes,
        experiment_id,
        epoch_revision,
        lyapunov_delta_total: pulse_metrics.lyapunov_delta_total,
        lyapunov_decrement_mean: pulse_metrics.lyapunov_decrement_mean,
        contraction_confidence: pulse_metrics.contraction_confidence,
        marginal_fraction: pulse_metrics.marginal_fraction,
        trigger_density_per_minute: pulse_metrics.trigger_density_per_minute,
        minimum_inter_event_ms: pulse_metrics.minimum_inter_event_ms,
    }
}

pub fn compare_sessions(
    baseline: crate::model::SessionSummary,
    candidate: crate::model::SessionSummary,
) -> crate::model::ComparisonReport {
    use crate::model::ComparisonReport;

    let baseline_rate = if baseline.average_tokens_per_second > 0.0 {
        baseline.average_tokens_per_second
    } else {
        baseline.throughput_units_per_second
    };
    let candidate_rate = if candidate.average_tokens_per_second > 0.0 {
        candidate.average_tokens_per_second
    } else {
        candidate.throughput_units_per_second
    };
    let mut invalid_reasons = Vec::new();
    if baseline.samples < 30 || candidate.samples < 30 {
        invalid_reasons.push("minimum_sample_count".into());
    }
    if baseline.duration_seconds <= 0.0 || candidate.duration_seconds <= 0.0 {
        invalid_reasons.push("zero_duration_window".into());
    }
    if !baseline.target_label.is_empty()
        && !candidate.target_label.is_empty()
        && baseline.target_label != candidate.target_label
    {
        invalid_reasons.push("target_changed".into());
    }
    if let (Some(left), Some(right)) = (
        baseline.sampling_interval_ms,
        candidate.sampling_interval_ms,
    ) {
        if left.abs_diff(right) > (left.max(right) / 10).max(1) {
            invalid_reasons.push("sampling_frequency_changed".into());
        }
    }
    if (baseline.average_stress - candidate.average_stress).abs() > 0.20 {
        invalid_reasons.push("workload_intensity_not_comparable".into());
    }
    let baseline_drop_ratio = baseline.dropped_samples as f64
        / (baseline.samples + baseline.dropped_samples).max(1) as f64;
    let candidate_drop_ratio = candidate.dropped_samples as f64
        / (candidate.samples + candidate.dropped_samples).max(1) as f64;
    if baseline_drop_ratio > 0.10 || candidate_drop_ratio > 0.10 {
        invalid_reasons.push("too_many_dropped_samples".into());
    }
    let comparable = invalid_reasons.is_empty();
    let sample_quality = (baseline.samples.min(candidate.samples) as f64 / 300.0).clamp(0.0, 1.0);
    let duration_ratio = if baseline.duration_seconds > 0.0 && candidate.duration_seconds > 0.0 {
        baseline.duration_seconds.min(candidate.duration_seconds)
            / baseline.duration_seconds.max(candidate.duration_seconds)
    } else {
        0.0
    };
    let evidence_quality = if comparable {
        (0.65 * sample_quality + 0.35 * duration_ratio).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let stability_gain = candidate.flow_stability - baseline.flow_stability;
    let variance_change = candidate.cpu_variance - baseline.cpu_variance;
    let latency_change = candidate.latency_p95_ms - baseline.latency_p95_ms;
    let score = stability_gain * 2.0
        - (candidate.prediction_rmse - baseline.prediction_rmse)
        - (variance_change / baseline.cpu_variance.max(1.0)).clamp(-1.0, 1.0) * 0.20
        - (latency_change / baseline.latency_p95_ms.max(1.0)).clamp(-1.0, 1.0) * 0.15;
    let verdict = if !comparable || evidence_quality < 0.20 {
        "INCONCLUSIVE"
    } else if score > 0.08 {
        "IMPROVED"
    } else if score < -0.08 {
        "REGRESSED"
    } else {
        "NEUTRAL"
    };

    ComparisonReport {
        intervention_value_tokens_per_second: candidate.average_tokens_per_second
            - baseline.average_tokens_per_second,
        throughput_delta_percent: percent_delta(baseline_rate, candidate_rate),
        flow_stability_delta: candidate.flow_stability - baseline.flow_stability,
        thermal_oscillation_delta_c: match (
            baseline.thermal_oscillation_c,
            candidate.thermal_oscillation_c,
        ) {
            (Some(left), Some(right)) => Some(right - left),
            _ => None,
        },
        energy_per_token_delta_percent: match (
            baseline.energy_per_token_joules,
            candidate.energy_per_token_joules,
        ) {
            (Some(left), Some(right)) => percent_delta(left, right),
            _ => None,
        },
        prediction_rmse_delta: candidate.prediction_rmse - baseline.prediction_rmse,
        comparable,
        evidence_quality,
        verdict: verdict.into(),
        invalid_reasons,
        cpu_mean_delta: candidate.cpu_mean - baseline.cpu_mean,
        cpu_variance_delta: variance_change,
        cpu_peak_delta: candidate.cpu_peak - baseline.cpu_peak,
        latency_p95_delta_ms: latency_change,
        queue_mean_delta: candidate.queue_mean - baseline.queue_mean,
        coherence_delta: match (
            baseline.oscillation_coherence,
            candidate.oscillation_coherence,
        ) {
            (Some(left), Some(right)) => Some(right - left),
            _ => None,
        },
        intervention_cost: candidate.applied_modulation_mean,
        note: if comparable {
            format!(
                "{verdict}: descriptive evidence only; repeat matched windows before making a causal claim."
            )
        } else {
            "Comparison invalid: matched targets, sampling, workload intensity, non-zero duration, and at least 30 finalized frames are required.".into()
        },
        baseline,
        candidate,
    }
}

fn percent_delta(baseline: f64, candidate: f64) -> Option<f64> {
    if baseline.is_finite() && candidate.is_finite() && baseline.abs() > f64::EPSILON {
        Some((candidate - baseline) / baseline.abs() * 100.0)
    } else {
        None
    }
}
