# ARIA verification and repair layer

PulseFlow adopts ARIA as a verification doctrine and machine-readable connection surface. It does not claim that a visual glyph grants authority or that a successful handshake authorizes mutation.

## Deterministic connection

Run:

```powershell
.\scripts\ARIA-Handshake.ps1 -Json
```

The handshake reads `aria/ARIA-CONNECT.json`, hashes the declared resources, publishes a deterministic `sha256:` identity, declares the read order and valid commands, and preserves initial authority as `none`.

The lifecycle is:

```text
discover → orient → verify → align → propose
```

## Verification lattice

- source identity: `aria/ARIA-SOURCE.json`;
- connection contract: `aria/ARIA-CONNECT.json`;
- staged declaration: `aria/pulseflow.aria`;
- repair and verification receipts: `aria/receipts/` and `state/aria-receipts/`;
- PowerShell 5.1 semantic contract: `schemas/powershell-runtime.contract.json`;
- executable Windows gates: `scripts/ARIA-Verify.ps1`;
- live HTTP closure: `scripts/ARIA-Smoke.ps1`.

The governing rule is **verify before promotion**. A visible button is not complete merely because it renders. A declared action must exist in the HTML declaration, JavaScript handler map, JSON contract, and Rust route surface. Rust source must format, compile, pass all tests, build in release mode, and survive the live HTTP smoke suite.

The v0.2.4 closure adds a semantic PowerShell gate. The earlier script parsed successfully but contained an expandable-string ambiguity in which `$Baseline?limit` was treated as one variable token. PulseFlow now constructs request paths and queries separately through `New-ApiUri`, percent-encodes dynamic path segments, and rejects that string pattern before compilation or smoke execution.

A failed gate remains a deterministic fracture rather than being hidden or represented as success.
