# Control model

PulseFlow is a bounded, residual-encoded workload controller. It builds a normalized stress signal from available telemetry, low-pass filters it, calculates a PID-like correction, then subtracts accumulated positive residual pressure.

See the full equations in the README. Controller source: `src/controller.rs`.

## Stability mechanisms

- input normalization and renormalization over available sensors;
- filtered stress state;
- integral clamp;
- authority clamp to `[0,1]`;
- per-sample authority slew limit;
- QoS hysteresis;
- platform-level minimum dwell time;
- thermal latch with distinct guard and release thresholds;
- responsive priority disabled by default;
- monitor-only fallback.

## Important claim boundary

This implementation is an engineering controller inspired by Pulse-Feedback Stability Theory. It does not yet contain a formal proof that every supported workload converges, nor would such a universal proof be realistic without a model of the specific plant. Stability must be evaluated empirically per workload class and actuator surface.
