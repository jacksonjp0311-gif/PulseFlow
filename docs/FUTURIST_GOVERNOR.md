# Futurist Governor

The futurist governor is a predictive observation layer, not an autonomous
authority escalation mechanism.

## What it is

PulseFlow v0.7 elevates Futurist to a first-class surface:

- multi-horizon forecasts at **H = 1, 5, 15** samples;
- channels: **stress**, **RAM**, **ecosystem pressure**;
- bounded pressure risk and an **envelope suggestion**:
  `hold | suggest_eco | contract_agent | thermal_watch`;
- skill scoring vs persist-last when a session is learned into a graph blob.

Study of captured JSONL sessions found that legacy controller capacity
commonly saturated near 1.0 while raw and filtered stress remained close and
residue held near -0.05. The cause was a fixed `+0.10*(capacity-0.5)` prediction
bias. The current controller forecasts from measured filtered-stress velocity
instead. Rolling analytics adds a longer linear trend forecast and fit
confidence.

## Non-goals (hard)

The forecast:

- requires eight aligned samples for multi-horizon mode;
- uses at most 60 recent samples per channel;
- is clamped to `[0,1]`;
- never changes agent binding by itself;
- never skips discovery, connection, verification, or enable;
- never expands the configured control envelope.

Envelope suggestions **may** influence agent-policy capacity (shadow or live
stage only as configured). They **never** call `Enable` or raise process
priority above the verified Active envelope.

## Know thy system

Futurist rides on an adaptive host profile (`GET /api/system`):

| Form factor | Typical host | Bias |
|---|---|---|
| `mobile_class` | &lt;6 GB, few cores | Memory-first weights, early guards |
| `constrained_desktop` | &lt;12 GB | Elevated RAM weight + Eco RAM assist |
| `desktop` | interactive workstation | Balanced ecosystem weights |
| `server` | many cores / high RAM | CPU/IO-forward weights |

Override with `PULSEFLOW_FORM_FACTOR=server|desktop|constrained_desktop|mobile`.

Process QoS remains Windows-gated. Observation, agent signals, Futurist, and
graph memory work on any host that can run the binary.

## Learn → graph blob → delete heavy raw

After each recording:

1. Start a new session (active session is protected).
2. **Learn All → Graph Blobs** (`POST /api/session/learn`) or compact one session.
3. PulseFlow writes `state/sessions/learning-datasets/{id}.dataset.json` plus a
   lightweight `.blob.json` index, calibrates Futurist skill, then **deletes**
   the raw JSONL.
4. Graph the blob in the **Memory** tab.

## Memory-pressure evidence

Agent-policy capacity is memory-aware and **profile-adaptive**. Soft / hard /
critical thresholds move with host class (for example earlier on 8 GB machines).
At hard RAM, PulseFlow suspends background memory work and contracts token,
retrieval, concurrency, and batch pressure. At critical RAM, it serializes agent
work. Secondary Eco assist also requests process Eco QoS when host RAM crosses
the adaptive Eco RAM enter threshold—even if classical stress is still low.

## API

| Method | Path | Role |
|---|---|---|
| GET | `/api/futurist` | Live multi-horizon snapshot |
| GET | `/api/system` | Adaptive host profile |
| POST | `/api/system/refresh` | Re-probe host |
| POST | `/api/session/learn` | Batch compact → blob → delete |

## UI

Open the **Futurist** tab for envelope, risk, skill, ability duty cycle, and
host identity. Open **Memory** to graph retained blobs after raw deletion.
