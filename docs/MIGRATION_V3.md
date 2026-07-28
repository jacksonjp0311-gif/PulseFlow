# Observation-frame migration v3

`pulseflow.observation.v3` makes the controller contract semantically explicit:

- `capacity_signal` is the bounded estimate of workload headroom.
- `control_authority` is the verified fraction of the process-scoped envelope.
- `controller_effort` is the normalized event-triggered effort for the interval.
- `applied_modulation` and `modulation` remain compatibility aliases for effort.

V3 frames also carry `experiment_id`, `epoch_revision`, `epoch_reason`, and
`tuning_revision`. PulseFlow starts a fresh epoch/session when mode, learning
stage, or tuning changes so one dataset does not silently combine distinct
experimental conditions.

V1 and V2 recordings remain readable. V1 ambiguous modulation is migrated to
zero. V2 effort is retained, while capacity is conservatively marked unknown
as zero because it cannot be reconstructed honestly from the old record.

Rolling and full-session analytics now expose the PFP v2.0 observables:
Lyapunov-style aggregate ΔV, average decrement, contraction confidence,
marginal fraction, applied-QoS trigger density, and minimum inter-event time.
