# HTTP API

The complete machine-readable route list is `schemas/pulseflow-api.contract.json`.

All write endpoints except `/api/session/new` require `Content-Type: application/json`. Responses disable caching. The server is intended for localhost sidecar use.

Example signal:

```json
{
  "source": "perci-runtime",
  "agent": "perci",
  "task_type": "coding",
  "model": "phi-4-mini",
  "context_tokens": 12480,
  "input_queue": 3,
  "output_queue": 1,
  "latency_ms": 816,
  "tokens_per_second": 31.5,
  "completed_units": 1,
  "success": true,
  "busy": true
}
```

Example replay request:

```json
{
  "session_id": "perci-1785023400000",
  "tuning": {
    "quiet_setpoint": 0.50,
    "balanced_setpoint": 0.66,
    "performance_setpoint": 0.78,
    "kp": 0.65,
    "ki": 0.08,
    "kd": 0.10,
    "kr": 0.34,
    "residue_decay": 0.82,
    "filter_alpha": 0.24,
    "slew_per_sample": 0.07
  }
}
```
