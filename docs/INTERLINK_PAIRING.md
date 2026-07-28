# PulseFlow Interlink Pairing v1

PulseFlow 0.3.1 adds a local pairing flow that behaves like a deliberate device connection without pretending that opening the dashboard grants control.

```text
discover -> select -> connect -> verify -> enable
```

## Channels

| Channel | Scope | Proof |
|---|---|---|
| Host telemetry | Whole-machine CPU, RAM, NVIDIA GPU and thermal observation | Fresh finalized observation frame |
| Process QoS | One operator-selected Windows process | Living PID, Windows governor support, armed state and non-monitor-only applied QoS |
| Agent adapter | A cooperating AI runtime or workload | Fresh `POST /api/signal` input and `GET /api/directive` consumption |

Process QoS can affect Windows priority and EcoQoS for the selected process. It does not directly rewrite GPU clocks or force an application to change batch size. GPU-level smoothing requires the workload to consume PulseFlow agent directives such as concurrency, batch, routing and background-work recommendations.

## Local pairing API

```text
GET  /api/interlink/handshake
GET  /api/processes
POST /api/interlink/connect      { "pid": 1234 }
POST /api/interlink/baseline     {}
POST /api/interlink/enable       {}
POST /api/interlink/disconnect   {}
GET  /api/interlink/verify
```

The server remains bound to `127.0.0.1` by default. Remote or fleet pairing requires a future authenticated transport; PulseFlow does not expose an unauthenticated control port to the network.

## A/B sequence

1. Discover and select the workload process.
2. Connect System. This binds identity but leaves authority paused.
3. Capture Baseline and run the workload for a fixed duration.
4. Enable PulseFlow and repeat the same workload.
5. Verify Link until `VERIFIED ACTIVE` appears.
6. Compare the two sessions in Analytics.

The interlink receipt proves the control path was present. Performance claims still require the measured session comparison.
