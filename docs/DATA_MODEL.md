# Data model

## Observation frame

Each persisted line is one `pulseflow.observation.v1` object and corresponds to:

`D_t = (X_t, A_t, R_t, Y_(t+Delta))`.

The frame is opened at the state/action time and finalized at the following telemetry sample. Persisted outcomes therefore use `alignment = next_interval`.

## Raw versus derived fields

- `workload`, `machine`, `controller`, `action`, `residue`, and `outcome` preserve the observation/control record.
- `metrics` contains rolling values known after that frame is finalized.
- Full-session summaries are recomputed from saved frames rather than trusted from the final rolling metric alone.

## Null semantics

A missing sensor is `null`. Zero means a sensor exists and reported zero. Consumers must preserve that distinction.

## Unit semantics

| Field | Unit |
|---|---|
| timestamps | Unix milliseconds |
| CPU/RAM/GPU utilization | percent, 0–100 |
| memory | GB for system RAM; MB for GPU/process fields |
| temperature | degrees Celsius |
| power | watts |
| latency | milliseconds |
| energy | joules |
| authority/stress/residue memory | normalized scalar |
| completed units | interval count |
