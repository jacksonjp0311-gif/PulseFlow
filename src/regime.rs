//! GCMT-inspired condition vector and cost-sensitive regime arbitration.
//!
//! Maps host telemetry + authority into an operating-envelope zone and a
//! memory/control regime. Hard integrity and legitimacy locks override soft
//! cost minimization. Display and policy may consume this; durable promotion
//! remains receipt-gated.

use crate::authority::AuthorityState;
use crate::model::{ControlSnapshot, RuntimeMetrics, Telemetry};
use serde::{Deserialize, Serialize};

/// GCMT memory regimes adapted to PulseFlow host governance.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRegime {
    /// Local continuation inside the admissible envelope.
    #[default]
    Local,
    /// Re-anchor identity / verification.
    Reanchor,
    /// Prefer explicit evidence (learn/compact/compare).
    Evidence,
    /// Abstain from durable promotion (shadow / hold).
    Abstain,
    /// Quarantine: capability without legitimacy.
    Quarantine,
    /// Rollback / demote authority.
    Rollback,
}

impl MemoryRegime {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "M_L",
            Self::Reanchor => "M_R",
            Self::Evidence => "M_E",
            Self::Abstain => "M_A",
            Self::Quarantine => "M_Q",
            Self::Rollback => "M_B",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "LOCAL",
            Self::Reanchor => "RE-ANCHOR",
            Self::Evidence => "EVIDENCE",
            Self::Abstain => "ABSTAIN",
            Self::Quarantine => "QUARANTINE",
            Self::Rollback => "ROLLBACK",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeZone {
    #[default]
    Admissible,
    Uncertain,
    Invalid,
}

impl EnvelopeZone {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admissible => "ADM",
            Self::Uncertain => "UNC",
            Self::Invalid => "INV",
        }
    }
}

/// Condition vector z_t ≈ (δ, κ, μ, r, f, ω, q_leg, p_int).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConditionVector {
    pub schema_version: String,
    /// Origin / prediction drift proxy.
    pub drift: f64,
    /// Address-condition stress (1 - coherence when known).
    pub address_stress: f64,
    /// Decision margin (forecast/policy confidence).
    pub margin: f64,
    /// Effective reserve (homeostatic slack).
    pub slack: f64,
    /// Evidence / verification freshness [0,1].
    pub freshness: f64,
    /// Contradiction density (raw vs filtered + turbulence).
    pub contradiction: f64,
    /// Authority legitimacy [0,1].
    pub legitimacy: f64,
    /// Continuation integrity 0/1.
    pub integrity: f64,
    /// Local expected transition loss estimate.
    pub local_loss: f64,
    pub ecosystem_pressure: f64,
    pub residual_burden: f64,
}

/// Arbiter output for the current sample.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegimeDecision {
    pub schema_version: String,
    pub regime: MemoryRegime,
    pub regime_code: String,
    pub regime_label: String,
    pub zone: EnvelopeZone,
    pub zone_code: String,
    pub continuation_debt: f64,
    pub switch_cost: String,
    pub hard_override: Option<String>,
    pub reason: String,
    pub condition: ConditionVector,
}

#[derive(Debug, Clone)]
pub struct RegimeArbiter {
    previous: MemoryRegime,
    debt: f64,
    dwell: u32,
    enter_loss: f64,
    exit_loss: f64,
    min_dwell: u32,
}

impl Default for RegimeArbiter {
    fn default() -> Self {
        Self {
            previous: MemoryRegime::Local,
            debt: 0.0,
            dwell: 0,
            // Hysteresis: harder to leave local than to return (asymmetric).
            enter_loss: 0.38,
            exit_loss: 0.28,
            min_dwell: 3,
        }
    }
}

impl RegimeArbiter {
    pub fn decide(
        &mut self,
        telemetry: &Telemetry,
        control: &ControlSnapshot,
        metrics: &RuntimeMetrics,
        authority: AuthorityState,
        verification_fresh: bool,
        verification_ok: bool,
        failed_invariant: bool,
        session_samples: u64,
    ) -> RegimeDecision {
        let condition = estimate_condition(
            telemetry,
            control,
            metrics,
            authority,
            verification_fresh,
            verification_ok,
            failed_invariant,
        );

        // Continuation debt accumulation / repayment (GCMT-style).
        let add = (condition.drift - 0.18).max(0.0) * 0.35
            + (condition.ecosystem_pressure - 0.70).max(0.0) * 0.25
            + (0.35 - condition.margin).max(0.0) * 0.15
            + (0.45 - condition.freshness).max(0.0) * 0.10
            + if condition.local_loss > 0.62 {
                0.12
            } else if condition.local_loss > 0.38 {
                0.05
            } else {
                0.0
            };
        self.debt = (0.92 * self.debt + add).clamp(0.0, 4.0);
        if condition.local_loss < 0.28 && condition.drift < 0.12 {
            self.debt *= 0.96;
        }

        let zone = if condition.integrity < 0.5
            || condition.local_loss > 0.62
            || condition.legitimacy < 0.35
        {
            EnvelopeZone::Invalid
        } else if condition.local_loss > self.enter_loss
            || matches!(authority, AuthorityState::Faulted | AuthorityState::Paused)
        {
            EnvelopeZone::Uncertain
        } else if condition.local_loss < self.exit_loss {
            EnvelopeZone::Admissible
        } else {
            // Between exit and enter: keep prior zone bias via dwell.
            if matches!(self.previous, MemoryRegime::Local) {
                EnvelopeZone::Admissible
            } else {
                EnvelopeZone::Uncertain
            }
        };

        let mut hard_override = None;
        let (mut regime, mut reason) = if condition.integrity < 0.5
            || failed_invariant
            || authority == AuthorityState::Faulted
        {
            hard_override = Some("integrity_or_fault".into());
            (
                MemoryRegime::Rollback,
                "Integrity failure or faulted authority forces rollback regime.".to_string(),
            )
        } else if condition.legitimacy < 0.35
            && matches!(
                authority,
                AuthorityState::Active | AuthorityState::Verified | AuthorityState::Paused
            )
        {
            hard_override = Some("low_legitimacy".into());
            (
                MemoryRegime::Quarantine,
                "Low legitimacy with elevated authority claim; quarantine durable actions."
                    .to_string(),
            )
        } else if condition.margin < 0.12
            && !matches!(zone, EnvelopeZone::Admissible)
            && metrics.samples >= 8
        {
            hard_override = Some("indecisive_margin".into());
            (
                MemoryRegime::Abstain,
                "Margin below threshold outside ADM; abstain from promotion.".to_string(),
            )
        } else if matches!(zone, EnvelopeZone::Invalid) {
            if self.debt > 0.8 || session_samples > 200 {
                (
                    MemoryRegime::Evidence,
                    "Invalid local envelope with continuation debt; prefer explicit evidence."
                        .to_string(),
                )
            } else {
                (
                    MemoryRegime::Reanchor,
                    "Invalid local envelope; re-anchor verification and identity.".to_string(),
                )
            }
        } else if matches!(zone, EnvelopeZone::Uncertain) {
            (
                MemoryRegime::Reanchor,
                "Uncertain envelope; escalate to re-anchor / source-check.".to_string(),
            )
        } else {
            (
                MemoryRegime::Local,
                "Admissible local envelope; continue operational loop.".to_string(),
            )
        };

        // Soft cost: switching penalty + dwell before return to Local.
        let switch_cost = if regime != self.previous {
            if self.previous != MemoryRegime::Local
                && regime == MemoryRegime::Local
                && (self.dwell < self.min_dwell || self.debt > 0.35)
            {
                regime = self.previous;
                reason = format!(
                    "Hysteresis hold on {}; dwell {}/{} or debt {:.2} blocks return to LOCAL.",
                    self.previous.label(),
                    self.dwell,
                    self.min_dwell,
                    self.debt
                );
                "HOLD".to_string()
            } else {
                self.dwell = 0;
                self.previous = regime;
                "SWITCH".to_string()
            }
        } else {
            self.dwell = self.dwell.saturating_add(1);
            if self.dwell >= self.min_dwell {
                "DWELL".to_string()
            } else {
                "HOLD".to_string()
            }
        };

        RegimeDecision {
            schema_version: "pulseflow.regime.v1".into(),
            regime,
            regime_code: regime.as_str().into(),
            regime_label: regime.label().into(),
            zone,
            zone_code: zone.as_str().into(),
            continuation_debt: self.debt,
            switch_cost,
            hard_override,
            reason,
            condition,
        }
    }

    pub fn debt(&self) -> f64 {
        self.debt
    }
}

fn estimate_condition(
    telemetry: &Telemetry,
    control: &ControlSnapshot,
    metrics: &RuntimeMetrics,
    authority: AuthorityState,
    verification_fresh: bool,
    verification_ok: bool,
    failed_invariant: bool,
) -> ConditionVector {
    let residual = control.residue_memory.abs().clamp(0.0, 1.0);
    let latent = metrics.latent_pressure.clamp(0.0, 1.0);
    let momentum = metrics.pressure_momentum_per_minute.max(0.0);
    let drift = (0.55 * residual + 0.35 * latent + 0.10 * (momentum / 0.05).clamp(0.0, 1.0))
        .clamp(0.0, 1.0);

    let coh = if metrics.oscillation_coherence.is_some() {
        metrics.oscillation_coherence.unwrap_or(0.5).clamp(0.0, 1.0)
    } else {
        0.5
    };
    let address_stress = (1.0 - coh).clamp(0.0, 1.0);

    let margin = metrics
        .forecast_confidence
        .max(metrics.policy_confidence)
        .clamp(0.0, 1.0);
    let slack = metrics.homeostatic_slack.clamp(0.0, 1.0);
    let eco = metrics.ecosystem_pressure.clamp(0.0, 1.0);

    let mut freshness = if verification_fresh && verification_ok {
        0.9
    } else if verification_ok {
        0.55
    } else {
        match authority {
            AuthorityState::Observation | AuthorityState::Discovered => 0.5,
            AuthorityState::Connected => 0.45,
            AuthorityState::Verified | AuthorityState::Active | AuthorityState::Paused => 0.3,
            AuthorityState::Faulted => 0.1,
            AuthorityState::Disconnected => 0.35,
        }
    };
    if telemetry.io_signal_fresh {
        freshness = (freshness + 0.1_f64).clamp(0.0, 1.0);
    }

    let contra = ((control.raw_stress - control.filtered_stress).abs() * 2.2).clamp(0.0, 1.0);
    let omega = if metrics.turbulence_state.contains("unstable") {
        (contra + 0.25).clamp(0.0, 1.0)
    } else {
        contra
    };

    let legitimacy = legitimacy_score(authority, failed_invariant);
    let integrity =
        if failed_invariant || !verification_ok && matches!(authority, AuthorityState::Faulted) {
            0.0
        } else if failed_invariant {
            0.0
        } else {
            1.0
        };
    let integrity = if authority == AuthorityState::Faulted {
        0.0
    } else {
        integrity
    };

    let local_loss =
        (0.45 * drift + 0.25 * eco + 0.15 * omega + 0.15 * (1.0 - slack)).clamp(0.0, 1.0);

    ConditionVector {
        schema_version: "pulseflow.condition.v1".into(),
        drift,
        address_stress,
        margin,
        slack,
        freshness,
        contradiction: omega,
        legitimacy,
        integrity,
        local_loss,
        ecosystem_pressure: eco,
        residual_burden: residual,
    }
}

fn legitimacy_score(authority: AuthorityState, failed_invariant: bool) -> f64 {
    if failed_invariant {
        return 0.2;
    }
    match authority {
        AuthorityState::Observation => 0.55,
        AuthorityState::Discovered => 0.62,
        AuthorityState::Connected => 0.72,
        AuthorityState::Verified => 0.88,
        AuthorityState::Active => 0.95,
        AuthorityState::Paused => 0.80,
        AuthorityState::Faulted => 0.20,
        AuthorityState::Disconnected => 0.40,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ControlSnapshot, RuntimeMetrics, Telemetry};

    #[test]
    fn fault_forces_rollback() {
        let mut arbiter = RegimeArbiter::default();
        let decision = arbiter.decide(
            &Telemetry::default(),
            &ControlSnapshot::default(),
            &RuntimeMetrics {
                samples: 20,
                homeostatic_slack: 0.8,
                ecosystem_pressure: 0.3,
                forecast_confidence: 0.5,
                policy_confidence: 0.5,
                oscillation_coherence: Some(0.8),
                ..RuntimeMetrics::default()
            },
            AuthorityState::Faulted,
            false,
            false,
            true,
            50,
        );
        assert_eq!(decision.regime, MemoryRegime::Rollback);
        assert_eq!(decision.zone, EnvelopeZone::Invalid);
    }

    #[test]
    fn admissible_stays_local() {
        let mut arbiter = RegimeArbiter::default();
        let decision = arbiter.decide(
            &Telemetry {
                memory_percent: 60.0,
                ..Telemetry::default()
            },
            &ControlSnapshot {
                residue_memory: 0.01,
                raw_stress: 0.3,
                filtered_stress: 0.3,
                ..ControlSnapshot::default()
            },
            &RuntimeMetrics {
                samples: 40,
                homeostatic_slack: 0.7,
                ecosystem_pressure: 0.35,
                latent_pressure: 0.02,
                forecast_confidence: 0.5,
                policy_confidence: 0.6,
                oscillation_coherence: Some(0.85),
                turbulence_state: "controlled_turbulence".into(),
                ..RuntimeMetrics::default()
            },
            AuthorityState::Observation,
            false,
            true,
            false,
            40,
        );
        assert_eq!(decision.regime, MemoryRegime::Local);
        assert_eq!(decision.zone, EnvelopeZone::Admissible);
    }
}
