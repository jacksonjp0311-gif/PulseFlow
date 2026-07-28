# Agent integration

## Producer obligation

The agent should POST `/api/signal` at least once per PulseFlow sampling interval while busy. `completed_units` should be the number completed since the prior update, not a lifetime counter.

## Consumer obligation

The agent may poll `/api/directive`. It must:

1. ignore live application when `shadow_only=true`;
2. clamp values again to local limits;
3. map only to supported local actuators;
4. log the applied or rejected decision;
5. preserve correctness and permission constraints over throughput goals.

## Recommended correlation fields

`agent`, `task_type`, `model`, and `context_tokens` should be stable and descriptive enough to group comparable sessions. Do not put secrets, prompt bodies, credentials, or user content into these labels.

## Integration pattern

Run PulseFlow beside the agent as a localhost sidecar. Keep the agent responsible for internal concurrency and model routing; keep PulseFlow responsible for telemetry fusion, residue tracking, evidence capture, and bounded recommendations.
