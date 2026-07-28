# PulseFlow Authority State Model

`src/authority.rs` is the canonical contract. The runtime serializes
`authority_state` and its derived `authority_contract`; the UI does not promote
itself from clicks.

| State | Allowed promotion/action | Backend evidence | Safe rollback |
|---|---|---|---|
| Observation | Discover | Fresh host telemetry | Observation |
| Discovered | Connect | PID returned by discovery | Observation |
| Connected | Verify, disconnect | PID and executable identity captured | Observation |
| Verified | Enable, disconnect | Live PID, identity match, governor support receipt | Connected |
| Active | Pause, disconnect | Fresh verification and confirmed non-monitor QoS | Verified |
| Paused | Resume, disconnect | Prior verification is still fresh | Verified |
| Faulted | Recover, disconnect | Failed invariant and last valid state | Observation |
| Disconnected | Discover | Disconnect receipt | Observation |

The only promotion chain is:

`OBSERVE -> DISCOVER -> CONNECT -> VERIFY -> ENABLE -> GOVERN`

Verification expires after five minutes. Target exit, PID/executable mismatch,
verification expiry, or inability to confirm QoS stops new modulation and
removes the ACTIVE claim. Agent binding is a separate field and is never
changed by process discovery, connection, verification, or activation.

Every authority transition emits `pulseflow.evidence-receipt.v1`. Receipts
include a deterministic configuration checksum and FNV-1a chain link. The link
is an integrity/provenance checksum, not a cryptographic signature.
