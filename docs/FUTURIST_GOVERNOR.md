# Futurist Governor

The futurist governor is a predictive observation layer, not an autonomous
authority escalation mechanism.

Study of the captured JSONL sessions found that legacy controller capacity
commonly saturated near 1.0 while raw and filtered stress remained close and
residue held near -0.05. The cause was a fixed `+0.10*(capacity-0.5)` prediction
bias. The current controller forecasts from measured filtered-stress velocity
instead. Rolling analytics adds a longer linear trend forecast and fit
confidence.

The forecast:

- requires eight aligned samples;
- uses at most 60 recent samples;
- is clamped to `[0,1]`;
- exposes trend, confidence, and configured-horizon pressure risk;
- is persisted with rolling metrics;
- never changes agent binding;
- never skips discovery, connection, verification, or enable;
- never expands the configured control envelope.

## Memory-pressure evidence

Agent-policy capacity is memory-aware. Above 85% host RAM use, PulseFlow
suspends background memory work and contracts token, retrieval, concurrency,
and batch pressure. At 95% RAM use, it serializes agent work until pressure
falls. These boundaries were promoted after a governed Edge recording showed
stable thermals alongside sustained 89% memory pressure.

This keeps “futurist” meaningful: PulseFlow anticipates bounded pressure while
remaining evidence-gated and fail-safe.
