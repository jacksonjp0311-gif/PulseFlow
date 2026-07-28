use crate::{
    config::{ControlConfig, StressWeights},
    model::{ControlSnapshot, OperatingMode, QosLevel, Telemetry},
};

pub struct PulseController {
    config: ControlConfig,
    weights: StressWeights,
    /// Secondary Eco trigger: host RAM percent at which Eco is requested
    /// even when classical stress remains below the setpoint (adaptive).
    eco_ram_enter_percent: f64,
    filtered_stress: f64,
    stress_velocity: f64,
    previous_error: f64,
    integral: f64,
    residue_memory: f64,
    authority: f64,
    qos_level: QosLevel,
    transition_count: u64,
    thermal_latched: bool,
}

impl PulseController {
    pub fn new(config: ControlConfig, weights: StressWeights, monitor_only: bool) -> Self {
        Self {
            config,
            weights,
            eco_ram_enter_percent: 88.0,
            filtered_stress: 0.0,
            stress_velocity: 0.0,
            previous_error: 0.0,
            integral: 0.0,
            residue_memory: 0.0,
            authority: 0.70,
            qos_level: if monitor_only {
                QosLevel::MonitorOnly
            } else {
                QosLevel::Normal
            },
            transition_count: 0,
            thermal_latched: false,
        }
    }

    pub fn update_config(&mut self, config: ControlConfig) {
        self.config = config;
        self.integral = self.integral.clamp(-2.0, 2.0);
        self.authority = self.authority.clamp(0.0, 1.0);
    }

    pub fn update_weights(&mut self, weights: StressWeights) {
        self.weights = weights;
    }

    pub fn set_eco_ram_enter_percent(&mut self, percent: f64) {
        self.eco_ram_enter_percent = percent.clamp(50.0, 99.0);
    }

    pub fn reset(&mut self, monitor_only: bool) {
        self.filtered_stress = 0.0;
        self.stress_velocity = 0.0;
        self.previous_error = 0.0;
        self.integral = 0.0;
        self.residue_memory = 0.0;
        self.authority = 0.70;
        self.qos_level = if monitor_only {
            QosLevel::MonitorOnly
        } else {
            QosLevel::Normal
        };
        self.transition_count = 0;
        self.thermal_latched = false;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        telemetry: &Telemetry,
        mode: OperatingMode,
        dt_seconds: f64,
        governor_active: bool,
        thermal_guard_c: f64,
        thermal_release_c: f64,
    ) -> ControlSnapshot {
        let raw_stress = self.compute_stress(telemetry);
        let alpha = self.config.filter_alpha.clamp(0.01, 1.0);
        let prior_filtered = self.filtered_stress;
        if self.filtered_stress == 0.0 {
            self.filtered_stress = raw_stress;
        } else {
            self.filtered_stress = alpha * raw_stress + (1.0 - alpha) * self.filtered_stress;
        }
        let observed_velocity = self.filtered_stress - prior_filtered;
        self.stress_velocity = if prior_filtered == 0.0 {
            0.0
        } else {
            (0.35 * observed_velocity + 0.65 * self.stress_velocity).clamp(-0.20, 0.20)
        };

        let setpoint = match mode {
            OperatingMode::Quiet => self.config.quiet_setpoint,
            OperatingMode::Balanced => self.config.balanced_setpoint,
            OperatingMode::Performance => self.config.performance_setpoint,
        }
        .clamp(0.1, 0.98);

        let dt = dt_seconds.clamp(0.05, 10.0);
        let error = setpoint - self.filtered_stress;
        self.integral = (self.integral + error * dt).clamp(-2.0, 2.0);
        let derivative = (error - self.previous_error) / dt;

        // The old predictor added a fixed authority bias. Captured sessions showed
        // that this created a persistent ~-0.05 residue when capacity saturated.
        // The futurist estimator now extrapolates only measured filtered motion.
        let forecast_confidence =
            (1.0 - (raw_stress - self.filtered_stress).abs() * 4.0).clamp(0.0, 1.0);
        let predicted_stress = (self.filtered_stress + self.stress_velocity).clamp(0.0, 1.0);
        let residue = (raw_stress - predicted_stress).clamp(-1.0, 1.0);
        let decay = self.config.residue_decay.clamp(0.0, 0.999);
        self.residue_memory = decay * self.residue_memory + (1.0 - decay) * residue;

        let correction =
            self.config.kp * error + self.config.ki * self.integral + self.config.kd * derivative
                - self.config.kr * self.residue_memory;
        let prior_authority = self.authority;
        let proposed = (self.authority + 0.22 * correction).clamp(0.0, 1.0);
        let slew = self.config.slew_per_sample.clamp(0.001, 0.25);
        self.authority += (proposed - self.authority).clamp(-slew, slew);
        self.authority = self.authority.clamp(0.0, 1.0);
        self.previous_error = error;

        let gpu_temp = telemetry.gpu.as_ref().and_then(|gpu| gpu.temperature_c);
        if gpu_temp.is_some_and(|temperature| temperature >= thermal_guard_c) {
            self.thermal_latched = true;
        }
        if gpu_temp.is_some_and(|temperature| temperature <= thermal_release_c) {
            self.thermal_latched = false;
        }

        let mut requested_qos = if !governor_active {
            QosLevel::MonitorOnly
        } else if self.thermal_latched {
            QosLevel::ThermalProtect
        } else {
            self.select_qos(mode)
        };
        // Adaptive secondary actuator: on memory-starved hosts, classical stress
        // can stay low while RAM saturates. Enter Eco without waiting for PID lag.
        if governor_active
            && !self.thermal_latched
            && telemetry.memory_percent >= self.eco_ram_enter_percent
            && !matches!(requested_qos, QosLevel::Eco | QosLevel::ThermalProtect)
        {
            requested_qos = QosLevel::Eco;
        }
        if requested_qos != self.qos_level {
            self.qos_level = requested_qos;
            self.transition_count = self.transition_count.saturating_add(1);
        }

        let jitter = (raw_stress - self.filtered_stress).abs();
        let qos_effort: f64 = match requested_qos {
            QosLevel::MonitorOnly => 0.0,
            QosLevel::Normal => 0.08,
            QosLevel::Eco => 0.38,
            QosLevel::Responsive => 0.55,
            QosLevel::ThermalProtect => 0.85,
        };
        let slew_effort = ((self.authority - prior_authority).abs() / slew).clamp(0.0, 1.0);
        let correction_effort = correction.abs().clamp(0.0, 1.0);
        let applied_modulation = if governor_active {
            (0.45 * qos_effort
                + 0.35 * slew_effort
                + 0.15 * correction_effort
                + 0.05 * self.residue_memory.abs())
            .clamp(0.0, 1.0)
        } else {
            0.0
        };
        let phase = if self.thermal_latched {
            "protect"
        } else if correction < -0.04 {
            "contract"
        } else if correction > 0.04 {
            "advance"
        } else {
            "hold"
        };

        ControlSnapshot {
            raw_stress,
            filtered_stress: self.filtered_stress,
            setpoint,
            error,
            integral: self.integral,
            derivative,
            predicted_stress,
            residue,
            residue_memory: self.residue_memory,
            modulation: applied_modulation,
            capacity_signal: self.authority.clamp(0.0, 1.0),
            control_authority: if governor_active { 1.0 } else { 0.0 },
            controller_effort: applied_modulation,
            applied_modulation,
            forecast_stress: predicted_stress,
            forecast_confidence,
            jitter,
            phase: phase.into(),
            reason: self.reason(telemetry, error, residue),
            requested_qos,
            applied_qos: requested_qos,
            transition_count: self.transition_count,
        }
    }

    fn select_qos(&self, mode: OperatingMode) -> QosLevel {
        match self.qos_level {
            QosLevel::Eco | QosLevel::ThermalProtect => {
                if self.authority < self.config.eco_exit {
                    QosLevel::Eco
                } else {
                    QosLevel::Normal
                }
            }
            QosLevel::Responsive => {
                if self.authority > self.config.responsive_exit
                    && mode == OperatingMode::Performance
                {
                    QosLevel::Responsive
                } else {
                    QosLevel::Normal
                }
            }
            _ => {
                if self.authority <= self.config.eco_enter {
                    QosLevel::Eco
                } else if self.authority >= self.config.responsive_enter
                    && mode == OperatingMode::Performance
                {
                    QosLevel::Responsive
                } else {
                    QosLevel::Normal
                }
            }
        }
    }

    fn compute_stress(&self, telemetry: &Telemetry) -> f64 {
        let mut weighted = 0.0;
        let mut available_weight = 0.0;

        add_component(
            &mut weighted,
            &mut available_weight,
            telemetry.cpu_percent / 100.0,
            self.weights.cpu,
        );
        add_component(
            &mut weighted,
            &mut available_weight,
            telemetry.memory_percent / 100.0,
            self.weights.memory,
        );

        if let Some(gpu) = &telemetry.gpu {
            add_component(
                &mut weighted,
                &mut available_weight,
                gpu.utilization_percent / 100.0,
                self.weights.gpu_utilization,
            );
            if let Some(temp) = gpu.temperature_c {
                let normalized = ((temp - 45.0) / 40.0).clamp(0.0, 1.0);
                add_component(
                    &mut weighted,
                    &mut available_weight,
                    normalized,
                    self.weights.gpu_temperature,
                );
            }
        }

        if telemetry.io_signal_fresh {
            let queue = (telemetry.io.input_queue + telemetry.io.output_queue) as f64;
            let queue_pressure = (queue / 64.0).clamp(0.0, 1.0);
            let latency_pressure = (telemetry.io.latency_ms / 2_000.0).clamp(0.0, 1.0);
            let busy_pressure = if telemetry.io.busy { 0.20 } else { 0.0 };
            let io_pressure =
                (0.60 * queue_pressure + 0.20 * latency_pressure + busy_pressure).clamp(0.0, 1.0);
            add_component(
                &mut weighted,
                &mut available_weight,
                io_pressure,
                self.weights.io_pressure,
            );
            add_component(
                &mut weighted,
                &mut available_weight,
                latency_pressure,
                self.weights.latency,
            );
        }

        if available_weight <= f64::EPSILON {
            0.0
        } else {
            (weighted / available_weight).clamp(0.0, 1.0)
        }
    }

    fn reason(&self, telemetry: &Telemetry, error: f64, residue: f64) -> String {
        if self.thermal_latched {
            return "GPU thermal guard is latched; efficiency QoS is requested until the release threshold is reached.".into();
        }
        if telemetry.memory_percent >= self.eco_ram_enter_percent {
            return format!(
                "Host RAM {:.1}% crossed the adaptive Eco assist threshold ({:.0}%); efficiency QoS is requested.",
                telemetry.memory_percent, self.eco_ram_enter_percent
            );
        }
        if telemetry
            .process
            .as_ref()
            .is_some_and(|process| !process.alive)
        {
            return "The target process is no longer visible; observation continues but QoS cannot be applied.".into();
        }
        if residue > 0.08 {
            return "Observed stress exceeded prediction; residual memory is reducing the next pulse.".into();
        }
        if residue < -0.08 {
            return "Observed stress was below prediction; modulation authority can recover gradually.".into();
        }
        if error < -0.08 {
            return "Filtered stress is above the selected operating setpoint.".into();
        }
        if error > 0.08 {
            return "Measured headroom is available; authority is increasing within the slew limit.".into();
        }
        "Stress is inside the pulse-feedback deadband; the current QoS request is held.".into()
    }
}

fn add_component(weighted: &mut f64, available: &mut f64, value: f64, weight: f64) {
    if weight > 0.0 && value.is_finite() {
        *weighted += value.clamp(0.0, 1.0) * weight;
        *available += weight;
    }
}
