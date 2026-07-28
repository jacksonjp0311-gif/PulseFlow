# Architecture

## Runtime data path

```text
TelemetryCollector ── CPU/RAM/process/NVIDIA ──┐
                                               ├─> PulseController ─> ProcessGovernor
POST /api/signal ── agent queues/outcomes ─────┘          │                 │
                                                          │                 └─ Windows target QoS
                                                          ├─> AgentDirective
                                                          └─> pending ObservationFrame
                                                                    │ next sample
                                                                    v
                                                          finalized D_t frame
                                                          ├─> JSONL recorder
                                                          ├─> rolling analytics
                                                          ├─> dashboard history
                                                          └─> replay/comparison lab
```

## Thread model

- Main thread binds the localhost server.
- One control-loop thread samples and finalizes frames.
- Each HTTP connection is handled in a short-lived thread.
- Shared runtime state uses `Arc<RwLock<RuntimeState>>`.
- JSONL persistence occurs on the control loop; event persistence occurs on state transitions.

## Trust boundaries

- Network bind defaults to `127.0.0.1`.
- The agent signal endpoint clamps numeric values and bounds text lengths.
- Session identifiers are restricted to ASCII alphanumeric, dash, and underscore.
- Dynamic file reads resolve only safe session IDs inside the configured session directory.
- Adaptive recommendations are shadow-only until the evidence gate is met.
- Hardware clocks, voltage, power limits, firmware, and fan controls are outside the design.
