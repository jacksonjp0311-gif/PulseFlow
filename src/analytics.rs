use crate::{
    config::AnalyticsConfig,
    model::{LearningGraphPoint, ObservationFrame, QosLevel, RuntimeMetrics},
};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone)]
struct MetricSample {
    timestamp_ms: u128,
    cpu_percent: f64,
    ram_percent: f64,
    gpu_percent: Option<f64>,
    ram_used_gb: f64,
    process_memory_mb: Option<f64>,
    latency_ms: f64,
    tokens_per_second: f64,
    queue: f64,
    temperature_c: Option<f64>,
    raw_stress: f64,
    filtered_stress: f64,
    setpoint: f64,
    residue: f64,
    residue_memory: f64,
    applied_modulation: f64,
    applied_qos: QosLevel,
    governor_active: bool,
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
            timestamp_ms: frame.timestamp_ms,
            cpu_percent: frame.machine.cpu_percent,
            ram_percent: frame.machine.ram_percent,
            gpu_percent: frame.machine.gpu_percent,
            ram_used_gb: frame.machine.ram_used_gb,
            process_memory_mb: frame.machine.process_memory_mb,
            latency_ms: frame.outcome.latency_ms.max(0.0),
            tokens_per_second: frame.outcome.tokens_per_second.max(0.0),
            queue: (frame.workload.input_queue + frame.workload.output_queue) as f64,
            temperature_c: frame.machine.gpu_temperature_c,
            raw_stress: frame.controller.raw_stress,
            filtered_stress: frame.controller.filtered_stress,
            setpoint: frame.controller.setpoint,
            residue: frame.controller.residue,
            residue_memory: frame.controller.residue_memory,
            applied_modulation: frame.controller.applied_modulation,
            applied_qos: frame.controller.applied_qos,
            governor_active: frame.action.governor_active,
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
        let ability = governor_ability_metrics(&self.window, self.elapsed_seconds);
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
        let homeostasis = homeostasis_metrics(
            &self
                .window
                .iter()
                .map(|sample| HomeostasisSample {
                    timestamp_ms: sample.timestamp_ms,
                    cpu_percent: sample.cpu_percent,
                    ram_percent: sample.ram_percent,
                    gpu_percent: sample.gpu_percent,
                    temperature_c: sample.temperature_c,
                    queue: sample.queue,
                    latency_ms: sample.latency_ms,
                    residue_memory: sample.residue_memory,
                    ram_used_gb: sample.ram_used_gb,
                    process_memory_mb: sample.process_memory_mb,
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
            ecosystem_pressure: homeostasis.ecosystem_pressure,
            latent_pressure: homeostasis.latent_pressure,
            homeostatic_slack: homeostasis.homeostatic_slack,
            pressure_momentum_per_minute: homeostasis.pressure_momentum_per_minute,
            recovery_rate_per_second: homeostasis.recovery_rate_per_second,
            accumulation_rate_per_second: homeostasis.accumulation_rate_per_second,
            recovery_balance: homeostasis.recovery_balance,
            resource_coupling: homeostasis.resource_coupling,
            recovery_half_life_seconds: homeostasis.recovery_half_life_seconds,
            target_memory_share: homeostasis.target_memory_share,
            resource_momentum_per_minute: homeostasis.resource_momentum_per_minute,
            resource_recovery_half_life_seconds: homeostasis.resource_recovery_half_life_seconds,
            vector_accumulation: homeostasis.vector_accumulation,
            vector_dissipation: homeostasis.vector_dissipation,
            pressure_transduction: homeostasis.pressure_transduction,
            net_vector_pressure: homeostasis.net_vector_pressure,
            governor_active_duty: ability.governor_active_duty,
            eco_duty_cycle: ability.eco_duty_cycle,
            qos_transition_count: ability.qos_transition_count,
            actuation_rate_per_minute: ability.actuation_rate_per_minute,
            futurist_envelope: String::new(),
            futurist_stress_h5: forecast_stress,
            futurist_ram_h5: None,
            system_form_factor: String::new(),
            envelope_zone: String::new(),
            memory_regime: String::new(),
            memory_regime_label: String::new(),
            continuation_debt: 0.0,
            condition_drift: 0.0,
            condition_legitimacy: 0.0,
            condition_integrity: 1.0,
            condition_freshness: 0.0,
            condition_margin: 0.0,
            regime_reason: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct GovernorAbilityMetrics {
    governor_active_duty: f64,
    eco_duty_cycle: f64,
    qos_transition_count: u64,
    actuation_rate_per_minute: f64,
}

fn governor_ability_metrics(
    window: &VecDeque<MetricSample>,
    elapsed_seconds: f64,
) -> GovernorAbilityMetrics {
    if window.is_empty() {
        return GovernorAbilityMetrics::default();
    }
    let total = window.len() as f64;
    let active = window
        .iter()
        .filter(|sample| sample.governor_active)
        .count() as f64;
    let eco = window
        .iter()
        .filter(|sample| {
            sample.governor_active
                && matches!(sample.applied_qos, QosLevel::Eco | QosLevel::ThermalProtect)
        })
        .count() as f64;
    let mut transitions = 0u64;
    for pair in window.iter().collect::<Vec<_>>().windows(2) {
        if pair[0].applied_qos != pair[1].applied_qos {
            transitions = transitions.saturating_add(1);
        }
    }
    let minutes = (elapsed_seconds / 60.0).max(1e-9);
    GovernorAbilityMetrics {
        governor_active_duty: (active / total).clamp(0.0, 1.0),
        eco_duty_cycle: if active > 0.0 {
            (eco / active).clamp(0.0, 1.0)
        } else {
            0.0
        },
        qos_transition_count: transitions,
        actuation_rate_per_minute: transitions as f64 / minutes,
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

#[derive(Debug, Clone, Copy)]
struct HomeostasisSample {
    timestamp_ms: u128,
    cpu_percent: f64,
    ram_percent: f64,
    gpu_percent: Option<f64>,
    temperature_c: Option<f64>,
    queue: f64,
    latency_ms: f64,
    residue_memory: f64,
    ram_used_gb: f64,
    process_memory_mb: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct HomeostasisMetrics {
    ecosystem_pressure: f64,
    latent_pressure: f64,
    homeostatic_slack: f64,
    pressure_momentum_per_minute: f64,
    recovery_rate_per_second: f64,
    accumulation_rate_per_second: f64,
    recovery_balance: f64,
    resource_coupling: Option<f64>,
    recovery_half_life_seconds: Option<f64>,
    target_memory_share: Option<f64>,
    resource_momentum_per_minute: BTreeMap<String, f64>,
    resource_recovery_half_life_seconds: BTreeMap<String, f64>,
    vector_accumulation: f64,
    vector_dissipation: f64,
    pressure_transduction: f64,
    net_vector_pressure: f64,
}

fn homeostasis_metrics(samples: &[HomeostasisSample]) -> HomeostasisMetrics {
    if samples.is_empty() {
        return HomeostasisMetrics::default();
    }
    let pressures: Vec<f64> = samples.iter().map(ecosystem_pressure).collect();
    let mean_pressure = mean(&pressures);
    let duration_seconds = samples
        .last()
        .map(|last| last.timestamp_ms.saturating_sub(samples[0].timestamp_ms) as f64 / 1_000.0)
        .unwrap_or(0.0);
    let net_change = pressures.last().copied().unwrap_or(0.0) - pressures[0];
    let pressure_momentum_per_minute = if duration_seconds > 0.0 {
        net_change / duration_seconds * 60.0
    } else {
        0.0
    };
    let channels = resource_channels(samples);
    let mut resource_momentum_per_minute = BTreeMap::new();
    let mut resource_recovery_half_life_seconds = BTreeMap::new();
    let mut accumulation = 0.0;
    let mut dissipation = 0.0;
    for (name, values) in &channels {
        if values.len() < 2 {
            continue;
        }
        let displacement = values.last().copied().unwrap_or(0.0) - values[0];
        accumulation += displacement.max(0.0);
        dissipation += (-displacement).max(0.0);
        let momentum = if duration_seconds > 0.0 {
            displacement / duration_seconds * 60.0
        } else {
            0.0
        };
        resource_momentum_per_minute.insert(name.clone(), momentum);
        if let Some(half_life) = recovery_half_life_series(samples, values) {
            resource_recovery_half_life_seconds.insert(name.clone(), half_life);
        }
    }
    let channel_count = channels.len().max(1) as f64;
    let vector_accumulation = (accumulation / channel_count).clamp(0.0, 1.0);
    let vector_dissipation = (dissipation / channel_count).clamp(0.0, 1.0);
    let pressure_transduction = vector_accumulation.min(vector_dissipation);
    let net_vector_pressure = vector_accumulation - vector_dissipation;
    let residue_burden = mean(
        &samples
            .iter()
            .map(|sample| sample.residue_memory.abs())
            .collect::<Vec<_>>(),
    );
    let latent_pressure = (vector_accumulation + residue_burden).clamp(0.0, 1.0);
    let homeostatic_slack = (1.0 - mean_pressure - latent_pressure).clamp(0.0, 1.0);

    let mut recovery_rates = Vec::new();
    let mut accumulation_rates = Vec::new();
    for (sample_window, pressure_window) in samples.windows(2).zip(pressures.windows(2)) {
        let dt = sample_window[1]
            .timestamp_ms
            .saturating_sub(sample_window[0].timestamp_ms) as f64
            / 1_000.0;
        if dt <= 0.0 {
            continue;
        }
        let velocity = (pressure_window[1] - pressure_window[0]) / dt;
        if velocity > 0.0 {
            accumulation_rates.push(velocity);
        } else if velocity < 0.0 {
            recovery_rates.push(-velocity);
        }
    }
    let recovery_rate = mean(&recovery_rates);
    let accumulation_rate = mean(&accumulation_rates);
    let recovery_balance = if recovery_rate + accumulation_rate > 1e-12 {
        ((recovery_rate - accumulation_rate) / (recovery_rate + accumulation_rate)).clamp(-1.0, 1.0)
    } else {
        0.0
    };

    let channel_deltas = [
        samples
            .windows(2)
            .map(|window| window[1].cpu_percent - window[0].cpu_percent)
            .collect::<Vec<_>>(),
        samples
            .windows(2)
            .map(|window| window[1].ram_percent - window[0].ram_percent)
            .collect::<Vec<_>>(),
        samples
            .windows(2)
            .filter_map(|window| Some(window[1].gpu_percent? - window[0].gpu_percent?))
            .collect::<Vec<_>>(),
    ];
    let mut correlations = Vec::new();
    for left in 0..channel_deltas.len() {
        for right in (left + 1)..channel_deltas.len() {
            if let Some(correlation) =
                pearson_correlation(&channel_deltas[left], &channel_deltas[right])
            {
                correlations.push(correlation.abs());
            }
        }
    }
    let resource_coupling = (!correlations.is_empty()).then(|| mean(&correlations));
    let recovery_half_life_seconds = recovery_half_life(samples, &pressures);
    let target_memory_share = {
        let shares: Vec<f64> = samples
            .iter()
            .filter_map(|sample| {
                let used_mb = sample.ram_used_gb * 1_024.0;
                sample
                    .process_memory_mb
                    .filter(|_| used_mb > 0.0)
                    .map(|process_mb| (process_mb / used_mb).clamp(0.0, 1.0))
            })
            .collect();
        (!shares.is_empty()).then(|| mean(&shares))
    };

    HomeostasisMetrics {
        ecosystem_pressure: mean_pressure,
        latent_pressure,
        homeostatic_slack,
        pressure_momentum_per_minute,
        recovery_rate_per_second: recovery_rate,
        accumulation_rate_per_second: accumulation_rate,
        recovery_balance,
        resource_coupling,
        recovery_half_life_seconds,
        target_memory_share,
        resource_momentum_per_minute,
        resource_recovery_half_life_seconds,
        vector_accumulation,
        vector_dissipation,
        pressure_transduction,
        net_vector_pressure,
    }
}

fn resource_channels(samples: &[HomeostasisSample]) -> BTreeMap<String, Vec<f64>> {
    let mut channels = BTreeMap::new();
    channels.insert(
        "cpu".into(),
        samples
            .iter()
            .map(|sample| (sample.cpu_percent / 100.0).clamp(0.0, 1.0))
            .collect(),
    );
    channels.insert(
        "ram".into(),
        samples
            .iter()
            .map(|sample| (sample.ram_percent / 100.0).clamp(0.0, 1.0))
            .collect(),
    );
    let optional = [
        (
            "gpu",
            samples
                .iter()
                .map(|sample| {
                    sample
                        .gpu_percent
                        .map(|value| (value / 100.0).clamp(0.0, 1.0))
                })
                .collect::<Option<Vec<_>>>(),
        ),
        (
            "thermal",
            samples
                .iter()
                .map(|sample| {
                    sample
                        .temperature_c
                        .map(|value| ((value - 40.0) / 45.0).clamp(0.0, 1.0))
                })
                .collect::<Option<Vec<_>>>(),
        ),
    ];
    for (name, values) in optional {
        if let Some(values) = values {
            channels.insert(name.into(), values);
        }
    }
    channels.insert(
        "queue".into(),
        samples
            .iter()
            .map(|sample| (sample.queue / 64.0).clamp(0.0, 1.0))
            .collect(),
    );
    channels.insert(
        "latency".into(),
        samples
            .iter()
            .map(|sample| (sample.latency_ms / 2_000.0).clamp(0.0, 1.0))
            .collect(),
    );
    channels
}

fn ecosystem_pressure(sample: &HomeostasisSample) -> f64 {
    let mut channels = vec![
        (sample.cpu_percent / 100.0).clamp(0.0, 1.0),
        (sample.ram_percent / 100.0).clamp(0.0, 1.0),
        (sample.queue / 64.0).clamp(0.0, 1.0),
        (sample.latency_ms / 2_000.0).clamp(0.0, 1.0),
    ];
    if let Some(gpu) = sample.gpu_percent {
        channels.push((gpu / 100.0).clamp(0.0, 1.0));
    }
    if let Some(temperature) = sample.temperature_c {
        channels.push(((temperature - 40.0) / 45.0).clamp(0.0, 1.0));
    }
    let bottleneck = channels.iter().copied().fold(0.0, f64::max);
    (0.5 * bottleneck + 0.5 * mean(&channels)).clamp(0.0, 1.0)
}

fn pearson_correlation(left: &[f64], right: &[f64]) -> Option<f64> {
    let length = left.len().min(right.len());
    if length < 8 {
        return None;
    }
    let left = &left[left.len() - length..];
    let right = &right[right.len() - length..];
    let left_mean = mean(left);
    let right_mean = mean(right);
    let mut numerator = 0.0;
    let mut left_energy = 0.0;
    let mut right_energy = 0.0;
    for (left_value, right_value) in left.iter().zip(right) {
        let left_delta = left_value - left_mean;
        let right_delta = right_value - right_mean;
        numerator += left_delta * right_delta;
        left_energy += left_delta * left_delta;
        right_energy += right_delta * right_delta;
    }
    let denominator = (left_energy * right_energy).sqrt();
    (denominator > 1e-12).then(|| (numerator / denominator).clamp(-1.0, 1.0))
}

fn recovery_half_life(samples: &[HomeostasisSample], pressures: &[f64]) -> Option<f64> {
    recovery_half_life_series(samples, pressures)
}

fn recovery_half_life_series(samples: &[HomeostasisSample], pressures: &[f64]) -> Option<f64> {
    if samples.len() < 8 || samples.len() != pressures.len() {
        return None;
    }
    let mut half_lives = Vec::new();
    for peak in 2..pressures.len().saturating_sub(1) {
        if pressures[peak] < pressures[peak - 1] || pressures[peak] <= pressures[peak + 1] {
            continue;
        }
        let baseline_start = peak.saturating_sub(5);
        let baseline = pressures[baseline_start..peak]
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let amplitude = pressures[peak] - baseline;
        if amplitude < 0.05 {
            continue;
        }
        let halfway = baseline + amplitude * 0.5;
        if let Some(recovered) =
            ((peak + 1)..pressures.len()).find(|index| pressures[*index] <= halfway)
        {
            let seconds = samples[recovered]
                .timestamp_ms
                .saturating_sub(samples[peak].timestamp_ms) as f64
                / 1_000.0;
            if seconds > 0.0 {
                half_lives.push(seconds);
            }
        }
    }
    if half_lives.is_empty() {
        None
    } else {
        Some(percentile(&half_lives, 0.5))
    }
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
    let homeostasis = homeostasis_metrics(
        &frames
            .iter()
            .map(|frame| HomeostasisSample {
                timestamp_ms: frame.timestamp_ms,
                cpu_percent: frame.machine.cpu_percent,
                ram_percent: frame.machine.ram_percent,
                gpu_percent: frame.machine.gpu_percent,
                temperature_c: frame.machine.gpu_temperature_c,
                queue: (frame.workload.input_queue + frame.workload.output_queue) as f64,
                latency_ms: frame.outcome.latency_ms,
                residue_memory: frame.controller.residue_memory,
                ram_used_gb: frame.machine.ram_used_gb,
                process_memory_mb: frame.machine.process_memory_mb,
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
        ecosystem_pressure: homeostasis.ecosystem_pressure,
        latent_pressure: homeostasis.latent_pressure,
        homeostatic_slack: homeostasis.homeostatic_slack,
        pressure_momentum_per_minute: homeostasis.pressure_momentum_per_minute,
        recovery_rate_per_second: homeostasis.recovery_rate_per_second,
        accumulation_rate_per_second: homeostasis.accumulation_rate_per_second,
        recovery_balance: homeostasis.recovery_balance,
        resource_coupling: homeostasis.resource_coupling,
        recovery_half_life_seconds: homeostasis.recovery_half_life_seconds,
        target_memory_share: homeostasis.target_memory_share,
        resource_momentum_per_minute: homeostasis.resource_momentum_per_minute,
        resource_recovery_half_life_seconds: homeostasis.resource_recovery_half_life_seconds,
        vector_accumulation: homeostasis.vector_accumulation,
        vector_dissipation: homeostasis.vector_dissipation,
        pressure_transduction: homeostasis.pressure_transduction,
        net_vector_pressure: homeostasis.net_vector_pressure,
        governor_active_duty: {
            let total = frames.len().max(1) as f64;
            frames
                .iter()
                .filter(|frame| frame.action.governor_active)
                .count() as f64
                / total
        },
        eco_duty_cycle: {
            let active = frames
                .iter()
                .filter(|frame| frame.action.governor_active)
                .count()
                .max(1) as f64;
            frames
                .iter()
                .filter(|frame| {
                    frame.action.governor_active
                        && matches!(
                            frame.controller.applied_qos,
                            QosLevel::Eco | QosLevel::ThermalProtect
                        )
                })
                .count() as f64
                / active
        },
        qos_transition_count: frames
            .windows(2)
            .filter(|window| window[0].controller.applied_qos != window[1].controller.applied_qos)
            .count() as u64,
        actuation_rate_per_minute: {
            let transitions = frames
                .windows(2)
                .filter(|window| {
                    window[0].controller.applied_qos != window[1].controller.applied_qos
                })
                .count() as f64;
            transitions / (duration_seconds / 60.0).max(1e-9)
        },
        futurist_envelope: String::new(),
        futurist_skill_improvement: 0.0,
        system_form_factor: String::new(),
    }
}

pub fn learning_graph_points(
    frames: &[ObservationFrame],
    maximum_points: usize,
) -> Vec<LearningGraphPoint> {
    if frames.is_empty() || maximum_points == 0 {
        return Vec::new();
    }
    let stride = frames.len().div_ceil(maximum_points).max(1);
    let start_ms = frames[0].timestamp_ms;
    frames
        .iter()
        .enumerate()
        .filter(|(index, _)| index % stride == 0 || *index + 1 == frames.len())
        .map(|(index, frame)| {
            let window_start = index.saturating_sub(119);
            let samples: Vec<_> = frames[window_start..=index]
                .iter()
                .map(|item| HomeostasisSample {
                    timestamp_ms: item.timestamp_ms,
                    cpu_percent: item.machine.cpu_percent,
                    ram_percent: item.machine.ram_percent,
                    gpu_percent: item.machine.gpu_percent,
                    temperature_c: item.machine.gpu_temperature_c,
                    queue: (item.workload.input_queue + item.workload.output_queue) as f64,
                    latency_ms: item.outcome.latency_ms,
                    residue_memory: item.controller.residue_memory,
                    ram_used_gb: item.machine.ram_used_gb,
                    process_memory_mb: item.machine.process_memory_mb,
                })
                .collect();
            let homeostasis = homeostasis_metrics(&samples);
            LearningGraphPoint {
                offset_ms: frame.timestamp_ms.saturating_sub(start_ms) as u64,
                cpu: (frame.machine.cpu_percent / 100.0).clamp(0.0, 1.0),
                ram: (frame.machine.ram_percent / 100.0).clamp(0.0, 1.0),
                gpu: frame
                    .machine
                    .gpu_percent
                    .map(|value| (value / 100.0).clamp(0.0, 1.0)),
                thermal: frame
                    .machine
                    .gpu_temperature_c
                    .map(|value| ((value - 40.0) / 45.0).clamp(0.0, 1.0)),
                stress: frame.controller.filtered_stress.clamp(0.0, 1.0),
                ecosystem_pressure: homeostasis.ecosystem_pressure,
                latent_pressure: homeostasis.latent_pressure,
                homeostatic_slack: homeostasis.homeostatic_slack,
                recovery_balance: homeostasis.recovery_balance,
            }
        })
        .collect()
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
