# PulseFlow System Interlink

The System Link is an ARIA-style attestation projected from backend facts. The display never claims active control merely because the dashboard is open.

## Pairing lifecycle

```text
discover -> select -> connect -> verify -> enable
```

- **Observation Link**: whole-machine telemetry and recording are live; no process authority exists.
- **Verified Connected**: a living process is selected and the Windows governor is available; modulation remains paused.
- **Verified Active**: the process is alive, authority is armed and a non-monitor-only QoS state has been applied.
- **Link Fracture**: the process died, the backend is unavailable, telemetry is stale or the platform governor is unsupported.

## Verification endpoint

`GET /api/interlink/verify` reports host telemetry, target identity, target liveness, governor support, armed state, requested/applied QoS, process-control status and agent-adapter freshness. Its `verification_id` binds the report to the current session and live sequence.

## Control scope

PulseFlow 0.5.0 controls one selected Windows process at a time through process priority and EcoQoS. It observes the whole machine. It does not alter clocks, voltage, fan curves, firmware or GPU power limits.

A cooperating AI runtime can additionally use:

```text
POST /api/signal
GET  /api/directive
```

That adapter is the route for workload-level changes such as concurrency, batching, model routing and background-work timing.

## Oscillation scope

The scope plots `controller.raw_stress` and `controller.filtered_stress`. The descriptive damping estimate is:

```text
100 * (1 - std(filtered_stress) / std(raw_stress))
```

This measures signal smoothing, not causal performance improvement. Matched baseline and governed sessions are required for claims about load, temperature, latency or throughput.
