# GCMT v1.3 × PulseFlow — Analysis & Evolution Plan

**Status:** analysis + UI corner instruments live (client arbiter display)  
**Evidence class:** GCMT-D for theory fixture; PulseFlow remains empirical host lab  
**Product target:** PulseFlow **v0.8** — Governed Regime Continuation  

---

## 1. What GCMT v1.3 is saying (compressed)

GCMT elevates memory from “store more” to **regime control**:

```text
Observe → Estimate condition z → Classify envelope → Arbitrate m*
  → Execute → Verify → Receipt → Update envelope
```

| Regime | Role |
|---|---|
| \(M_L\) | Local continuation (stay in compressed operational state) |
| \(M_R\) | Re-anchor (refresh identity / verification / origin) |
| \(M_E\) | Explicit evidence (receipts, JSONL, graph blobs, compare) |
| \(M_A\) | Abstain / source-check (shadow, do not promote) |
| \(M_Q\) | Quarantine (capability without legitimacy) |
| \(M_B\) | Rollback (integrity fail / corrupted continuation) |

Hard locks (non-negotiable):

- \(p^{int}=0 \Rightarrow M_B\)
- low legitimacy + durable write \(\Rightarrow M_Q\)
- indecisive margin \(\Rightarrow M_A\)
- outside local envelope + valid evidence \(\Rightarrow M_E \succ M_L\)
- \(\tau_{out} < \tau_{in}\) hysteresis against thrash
- unreconciled drift \(\Rightarrow\) **continuation debt** \(D^{cont}\)

**Canonical lock shared with PulseFlow today:** adaptation may be automatic; **promotion remains governed**; receipts matter; synthetic success ≠ production validation.

---

## 2. PulseFlow runtime inspection (live sample)

| Observable | Value | GCMT reading |
|---|---:|---|
| Version | 0.7.0 | Install current |
| Form factor | constrained_desktop | Memory-primary plant |
| Authority | **faulted** | \(q^{leg}\) collapsed → arbiter should show **ROLLBACK / RE-ANCHOR** |
| QoS | monitor_only | Actuator off while faulted (correct demotion) |
| CPU / RAM | ~65% / ~84% | Eco-pressure zone; local envelope strained |
| Stress / phase | ~0.44 / advance | Classical stress not the whole story |
| Coherence | ~0.70 | \(\kappa\) moderate address stress |
| Slack / eco \(P\) | ~0.46 / ~0.53 | Reserve OK-ish; ecosystem still loaded |
| Residue / latent | low | \(\delta\) small now; debt can still sit from prior faults |
| Futurist | hold, risk 0 | Soft margin \(\mu\) |
| Sessions | 16 jsonl + 8 blobs | Evidence plane has material |

**Verdict:** The host is doing what GCMT predicts under fault: **local fluency can look fine while legitimacy/integrity force a non-local regime.** Top-corner metrics now surface that split.

---

## 3. Condition vector map (efficient, implementable)

GCMT \(z_t = (\delta,\kappa,\mu,r,f,\omega,q^{leg},p^{int})\) → PulseFlow:

| GCMT | PulseFlow proxy (v0.7 UI) | Future backend field |
|---|---|---|
| \(\delta\) drift | residue + latent + max(0, momentum) | `condition.drift` |
| \(\kappa\) address | \(1-\)oscillation_coherence | `condition.address_stress` |
| \(\mu\) margin | max(forecast_conf, policy_conf) | `condition.margin` |
| \(r\) rank/capacity | homeostatic_slack | already present |
| \(f\) freshness | verification age / signal fresh | `condition.freshness` |
| \(\omega\) contradiction | \|raw−filtered\| + turbulence | `condition.contradiction` |
| \(q^{leg}\) | authority lattice score | `condition.legitimacy` |
| \(p^{int}\) | receipt success / failed_invariant | `condition.integrity` |
| \(D^{cont}\) | EMA of envelope breaches | `metrics.continuation_debt` |
| \(m^*\) | cost-sensitive rule + hard overrides | `arbiter.regime` + receipt |

Envelope zones for \(M_L\):

- **ADM** — continue locally (observe / govern within authority)  
- **UNC** — re-anchor or broaden evidence  
- **INV** — yield: abstain / quarantine / rollback / explicit evidence  

---

## 4. UI shipped now (top corners)

**Left · Condition (O-plane)**  
Turbulence, phase, \|error\|, drift \(\delta\), slack, ecosystem \(P\), transduction \(T\), recovery, **envelope zone**, **continuation debt**.

**Right · Arbiter (E/C-plane)**  
Regime \(m^*\), legitimacy \(q\), integrity \(p\), freshness \(f\), margin \(\mu\), contradiction \(\omega\), clock, λ lock, switch cost (HOLD/SWITCH/DWELL).

These are **display + operator guidance**, not auto-writers. That preserves GCMT’s promotion boundary.

Bottom corners remain Oscillation / Reverberation with λ-sync (prior change).

---

## 5. Next evolution plan (v0.8 “Governed Regime”)

### Phase 0 — Integrity of current run (immediate ops)

1. Clear **faulted** authority: Disconnect → Discover → Connect → Verify (or restart session).  
2. Note why faulted (`failed_invariant` in status/events).  
3. Learn heavy JSONL → blobs if any session &gt; 2 MB.

### Phase 1 — Backend condition vector (v0.8.0-alpha)

| ID | Work | Gate |
|---|---|---|
| G1 | `ConditionVector` + `RegimeDecision` in `model.rs` | serde + default |
| G2 | Compute in analytics/control loop each sample | unit tests synthetic z |
| G3 | Persist on `RuntimeMetrics` + observation frames | schema v3 fields optional |
| G4 | Emit `pulseflow.regime-receipt.v1` on regime **switch** only | chain hash like authority |

### Phase 2 — Hysteretic arbiter (code, not just UI)

| ID | Work | Gate |
|---|---|---|
| G5 | Asymmetric \(\tau_{in}/\tau_{out}\) for drift, margin, legitimacy | no thrash under ±noise |
| G6 | Dwell \(d_{min}\) before return to \(M_L\) | matches GCMT law |
| G7 | Hard overrides: integrity→\(M_B\), legit→\(M_Q\), margin→\(M_A\) | property tests |
| G8 | Soft objective: \(\hat L_m + \lambda_c C_m + \lambda_s 1_{switch}\) | documented weights |

**PulseFlow costs \(C_m\) (pragmatic):**

| Regime | Cost meaning |
|---|---|
| \(M_L\) | cheap — stay in loop |
| \(M_R\) | verify/rediscover latency |
| \(M_E\) | disk/CPU summarize+compact |
| \(M_A\) | opportunity cost (shadow) |
| \(M_Q\) | blocked durable action |
| \(M_B\) | session reset / demotion |

### Phase 3 — Wire regimes to real surfaces

| Regime | PulseFlow action (bounded) |
|---|---|
| \(M_L\) | Normal observe/govern path |
| \(M_R\) | UI prompt + optional auto **Verify** when identity soft-stale |
| \(M_E\) | Suggest/trigger **Learn → graph blob** when debt high & session inactive |
| \(M_A\) | Force directive `shadow_only`; block adaptive apply |
| \(M_Q\) | Refuse enable/tuning writes; banner |
| \(M_B\) | Demote authority; open new epoch; keep host observation |

Never: silent promotion of gains, never widen QoS from \(M_E\).

### Phase 4 — Evidence precedence

| ID | Work |
|---|---|
| G9 | When zone INV and learning blob exists, Analytics defaults to last blob vs live compressed metrics |
| G10 | Compare baseline/governed invalid if freshness or integrity fail |
| G11 | Cortex remember only on \(M_E\) receipts / learn events |

### Phase 5 — Futurist + wavelength as condition inputs

| ID | Work |
|---|---|
| G12 | λ stability over 60s → \(\kappa\) component |
| G13 | Forecast MAE vs persist → \(\mu\) component |
| G14 | Ghost H=5 on oscillation scope when \(\mu\) high |

### Phase 6 — Validation (honest GCMT-D → approach GCMT-C)

| Gate | Criterion |
|---|---|
| V1 | Regime thrash rate &lt; 0.05/min under idle |
| V2 | Fault inject → \(M_B\) within 1 sample |
| V3 | Low legit enable attempt → \(M_Q\) block |
| V4 | High debt idle session → \(M_E\) suggestion within N samples |
| V5 | ≥3 real host recordings with receipts (not fixture-only) |

---

## 6. Non-goals

- Claiming GCMT-C or production proof from synthetic fixtures  
- Autonomous durable promotion from debt alone  
- Replacing Windows QoS with abstract regimes without process identity  
- Cloud exfil of condition vectors  

---

## 7. Suggested build order

1. **Now:** use top-corner chips while operating (already in UI).  
2. **Next coding session:** G1–G4 backend condition + regime receipt.  
3. **Then:** G5–G8 hysteresis + hard locks.  
4. **Then:** G9–G11 evidence precedence + learn automation suggestions.  
5. **Release v0.8** after V1–V4 on this constrained_desktop host.

---

## 8. One-line synthesis

> **PulseFlow becomes the host-side plant controller; GCMT is the memory-regime controller above it.**  
> Stress, Eco, and Futurist answer *what the machine is doing*.  
> Envelope, debt, and \(m^*\) answer *which continuation mode is admissible*.

That is the v0.8 north star.
