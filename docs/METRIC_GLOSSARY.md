# PulseFlow Metric Glossary

## Control authority

The verified fraction of the permitted process-scoped control envelope.
It is 0 before verification and 1 after verification while the receipt is
valid. It is authorization, not actuation.

## Applied modulation

Actual normalized controller effort in `[0,1]` for the interval:

`0.45*qos_intensity + 0.35*slew_use + 0.15*|correction| + 0.05*|residue_memory|`

The result is zero unless governance is active and the backend confirms a
non-monitor QoS result. QoS intensity is fixed and bounded by level. The value
is deterministic, recorded in every v2 frame, and never inferred from a click.

## Oscillation coherence

Agreement between aligned raw workload stress and the governor-filtered
trajectory over the rolling window (300 samples by default). Both traces are
demeaned. Coherence is `1 - RMSE(demeaned traces)/(sigma_raw + sigma_filtered +
epsilon)`, clamped to `[0,1]`. Fewer than eight samples returns unavailable.
Identical constants return 1; one constant and one varying trace return 0.

Stability can remain high while coherence is moderate when disturbances are
bounded but irregular.

## Futurist forecast

A bounded linear-trend forecast over at most the latest 60 filtered-stress
samples, projected by the configured horizon (five samples by default).
Confidence is the residual fit quality multiplied by sample coverage. The
forecast reports pressure risk but cannot widen authority or bypass gates.

## Controlled turbulence

`CONTROLLED_TURBULENCE` requires at least eight samples, measurable local
variation, pressure below 0.75, mean queue below 48, thermal drift within
0.15 C/sample, residue within 0.25, and bounded modulation. Thresholds live in
`config/pulseflow.json`. Other states are `QUIESCENT`, `PRESSURE_BUILDING`,
`SATURATED`, `UNSTABLE`, and `INSUFFICIENT_DATA`.
