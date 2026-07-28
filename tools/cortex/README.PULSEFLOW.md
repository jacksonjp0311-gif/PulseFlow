# Cortex inside PulseFlow

This directory vendors Cortex v3 as PulseFlow's repository-memory engine.
PulseFlow retains ownership of telemetry, compact graph datasets, runtime
analysis receipts, and process authority. Cortex provides repository
assimilation, evidence retrieval, task episodes, and Discovery Cards.

The source provenance is recorded in `PULSEFLOW_VENDOR.json`.

Initialize or refresh the integration from the PulseFlow repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\Initialize-PulseFlow-Cortex.ps1
```

Generated databases and runtime packets remain local and are not committed.
