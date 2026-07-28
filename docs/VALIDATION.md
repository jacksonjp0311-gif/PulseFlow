# Validation

## Static and compiler gates

Run:

```powershell
.\scripts\PulseFlow-All-One.ps1 -Mode Verify
```

This invokes the ARIA verifier and smoke test.

## Test inventory

| File | Coverage |
|---|---|
| `tests/controller_tests.rs` | bounds, thermal latch/release, monitor-only behavior |
| `tests/analytics_tests.rs` | statistics, session energy/tokens, comparison evidence gate |
| `tests/adaptive_tests.rs` | evidence gate, shadow-only guarantee, bounded update limit |
| `tests/storage_tests.rs` | JSONL append/read and metadata round trip |
| `tests/replay_tests.rs` | defined empty-session behavior |
| `tests/ui_contract_tests.rs` | action parity, handlers, routes, tabs, schemas, causal markers |
| `scripts/ARIA-Smoke.ps1` | live server and every HTTP-backed operator action |

## Local build-status file

`BUILD-STATUS.md` records which checks were executed in the packaging environment and which require the Windows Rust toolchain. This distinction is deliberate: static source validation is not reported as a successful Rust compilation.
