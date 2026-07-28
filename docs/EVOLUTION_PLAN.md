# PulseFlow Evolution Plan

**Status:** Phase A–C implemented in **v0.7.0** (adaptive system, Futurist, learn→blob)  
**Baseline product:** PulseFlow Governor **v0.7.0**  
**Handoff date:** 2026-07-28  
**Prior operator:** Codex (authority lattice → icon/install → recording analysis)  
**Current operator:** Grok  

### Shipped in this handoff (v0.7.0)

- `src/system.rs` — know-thy-system form factors + adaptive weights/guards  
- `src/futurist.rs` — multi-horizon Futurist Governor + skill scoring  
- Eco RAM assist + ability metrics on summaries  
- `POST /api/session/learn` + CLI `pulseflow-governor learn`  
- Futurist UI tab + Memory graph blobs after raw delete  


This document is the canonical next-step plan. It is evidence-led: every track
cites retained recordings, recycled-session receipts, or verification gates.
It does **not** authorize automatic promotion of controller gains, expanded
process priority, or agent live mode without the gates below.

---

## 1. Where we are (truthful baseline)

### 1.1 What Codex closed (keep)

| Layer | Outcome |
|---|---|
| Authority | Discover → connect → verify → enable → govern; receipt-linked; fault demotion |
| Semantics | Control authority ≠ applied modulation; forecast never widens authority |
| Analytics | Oscillation coherence, controlled-turbulence class, baseline/governed compare |
| Install UX | Icon pack, embedded exe icon, desktop/Start shortcuts, single-instance launch |
| Safety | v1→v2 migration, invalid-frame rejection, agent isolation from process interlink |
| Theory | Computational homeostasis + vector pressure + compact iteration memory |
| Policy | Memory guard (75/85/95% RAM) promoted after governed Edge evidence |
| Verification | ARIA lattice green through `verification-20260728-064140.json` |

### 1.2 What the recordings actually prove

#### A. Recycled governed Edge experiment (strongest closed loop so far)

Receipt: `state/aria-receipts/recording-analysis-20260728-0440.json`  
Raw JSONL recycled after analysis; aggregates preserved.

| Window | Samples | RAM mean | Stress / applied | Notes |
|---|---:|---:|---|---|
| Governed Edge | 176 / ~183 s | **89.1%** | stress 0.315, applied mod **0.136** | thermally safe (~40 °C), RMSE **0.029**, stability **0.92** |
| System observe | 162 | 89.1% | monitor | same host pressure without Edge QoS |
| Pre-enable | 13 | — | transition only | **not a baseline** |
| Post disconnect | 66 | 88.9% | monitor | host RAM unchanged by disconnect |

**Promotion made:** agent memory guard only.  
**Promotion refused:** controller gains (correct).

#### B. Retained sessions (still on disk)

| Session | Role | n | Governor | Headline |
|---|---|---:|---|---|
| `msedgeexe-1785206021315` | Long governed Edge (v1) | 1379 | **active, QoS always `normal`** | 0 QoS transitions; stress mean 0.287; RAM mean 83.3% (peak 93.6%); proc CPU mean 36% |
| `msedgeexe-1785205676350` | Mixed Edge | 227 | 128 active / 99 monitor | low stress; stable |
| `msedgeexe-1785207646232` | Short Edge | 48 | mostly normal | higher host CPU mean ~48% |
| `system-monitor-1785205912913` | 86-frame homeostasis anchor | 86 | monitor | RAM ~82–85%; forced vector/homeostasis model |
| `system-monitor-1785207698005` | Long host observe | **16105** (~4.6 h) | monitor | RAM climbed ~74→84%; legacy capacity field saturated near 1.0 |
| `system-monitor-1785226000686` | v2 host observe | 705 | monitor | RAM mean **88%**, 41 frames CPU≥90; forecast skill usable |
| `system-monitor-1785227305213` | v2 host observe | 229 | monitor | RAM mean **89%**, coherence ~0.66, controlled_turbulence |

#### C. Governor ability — critical finding

On the longest retained governed session (`msedgeexe-1785206021315`):

- `governor_active` for all 1379 frames  
- **requested/applied QoS = `normal` for every frame**  
- **zero QoS transitions**  
- phase always `advance` (stress ≪ balanced setpoint 0.66)  
- legacy `modulation` (old capacity) mean ≈ **0.999**

Interpretation:

1. The **authority machine works** (enable holds; process stays linked).  
2. The **actuator almost never fires** under desktop Edge load because stress is built mainly from CPU/GPU and sits below setpoint while **RAM is the real constraint**.  
3. Memory weight in `config/pulseflow.json` is only **0.14**; ecosystem RAM at 85–92% does not drive Eco.  
4. Therefore “governor ability” today is mostly **observe + light Normal priority hold + agent advice**, not active Eco/ThermalProtect pulsing.

Platform actuator (`src/governor.rs`): priority class + optional power throttling, 8 s dwell, Responsive disabled by default. That is an appropriate safety envelope — but it must be **exercised and measured** before claiming process governance value.

#### D. Futurist skill — encouraging

On `system-monitor-1785226000686` (v2), absolute error of rolling forecast vs stress **~5 samples ahead**:

- mean abs error ≈ **0.030**  
- ~p95 ≈ **0.085**  
- mean forecast confidence still modest (~0.20) because of bursty desktop CPU  

Futurist is already a **useful predictor of stress motion**. It is **not** yet a full multi-channel pressure governor UI, and it still cannot/must not escalate authority.

### 1.3 Machine profile (lab constraint)

- Host RAM total ≈ **7.9 GB**  
- Typical operating point **82–92% RAM**  
- Agent policy will almost always be memory-constrained on this box  
- Treat **chronic high-RAM desktop** as Profile A for all experiments

---

## 2. North star for the next evolution

> **PulseFlow becomes a two-surface governor:**  
> 1) **Process Governor** — bounded Windows QoS with measurable actuation and honest effect sizes.  
> 2) **Futurist Governor** — multi-horizon, multi-channel pressure foresight that *advises* process and agent envelopes without ever self-promoting authority.

Both surfaces share one observation frame, one authority lattice, one evidence ledger.

Success is **not** “looks futuristic.” Success is:

1. Forecasts beat naive baselines on held-out recordings.  
2. When enabled, the process governor produces **observable QoS transitions** under designed stress and does not worsen tails.  
3. Agent directives under memory pressure reduce background/token pressure **with bound outcomes**.  
4. Every promotion leaves a receipt and a failing counterfactual if the gate is not met.

---

## 3. Product surfaces

### 3.1 Process Governor (existing, must mature)

**Job:** apply dwell-limited Eco / Normal / ThermalProtect (Responsive opt-in) to a verified PID.

**Ability metrics (new, required):**

| Metric | Definition |
|---|---|
| Actuation rate | QoS transitions / minute while Active |
| Eco duty cycle | fraction of Active samples in Eco or ThermalProtect |
| Intent→apply latency | samples between requested≠applied and apply |
| Dwell integrity | fraction of deferred transitions explained by dwell |
| Target share | process memory / used host memory |
| Effect size | Δ stress, Δ proc CPU, Δ RAM slope: baseline vs governed (matched) |

**Gate to claim “governor works”:** ≥1 matched baseline/governed pair with n≥60 each, evidence quality valid, and either (a) Eco duty cycle > 0 under designed pressure **or** (b) explicit NEUTRAL verdict with high confidence that Normal-only was correct.

### 3.2 Futurist Governor (elevate to first-class)

**Job:** publish bounded foresight the human and agent policy can use.

**v0.6 today:**

- 1-step controller velocity forecast  
- rolling linear trajectory (≤60 samples, horizon config, default 5)  
- confidence + pressure risk  
- UI display fields  

**Target Futurist Governor (v0.7 track):**

```text
channels: stress | ecosystem_pressure | ram | cpu | gpu_thermal | queue
horizons: H=1, H=5, H=15 samples (~1s / 5s / 15s at 1 Hz)
outputs:  forecast, confidence, pressure_risk, recommended_envelope
envelope: hold | contract_agent | suggest_eco | thermal_watch
authority: NEVER auto-enable, NEVER widen QoS beyond verified Active envelope
```

**Skill gates (must pass before UI badge “calibrated”):**

| Gate | Criterion on held-out sessions |
|---|---|
| F1 | MAE(H=5 stress) < MAE(persist-last) by ≥10% relative |
| F2 | MAE(H=5 RAM%) beats persist-last when RAM variance > 1 pp |
| F3 | High-risk alarms (pressure_risk > threshold) have precision ≥ 0.6 on next 15 s |
| F4 | Forecast path never mutates authority_state in tests |

---

## 4. Evolution tracks (ordered)

### Track 0 — Continuity & lab hygiene (do first, 0.5 day)

**Why:** recordings prove 49 MB JSONL and missing governed raw frames after recycle.

| Work item | Done when |
|---|---|
| 0.1 Document handoff in this file (this PR) | file landed |
| 0.2 Ensure install + source tree same version label | `Cargo.toml` version matches installed binary banner / health |
| 0.3 Compaction-default after Analyze | compact → learning dataset → optional recycle; receipt always kept |
| 0.4 Session catalog in dashboard | list role, n, duration, governor_active duty, schema |
| 0.5 Cold-start health probe | desktop shortcut already fixed; add `/api/health` contract test if route differs |

**No control-law changes.**

### Track 1 — Futurist Governor v1 (product + science)

**Goal:** make Futurist a named, testable subsystem users can trust as foresight.

| ID | Work | Evidence |
|---|---|---|
| FG-1 | Extract `src/futurist.rs` (or module) from analytics forecast helpers | unit tests for slope clamp, confidence, empty windows |
| FG-2 | Multi-horizon forecasts H∈{1,5,15} for stress + ecosystem pressure + RAM | schema + API `GET /api/futurist` |
| FG-3 | Skill scorecard: persist-last vs linear vs (optional) residue-aware | computed on compact iterations; stored in receipt |
| FG-4 | Envelope suggestion only: `hold / contract_agent / suggest_eco / thermal_watch` | mapping table tested; authority unchanged |
| FG-5 | Dashboard **Futurist** panel: horizons, risk strip, skill last-N | UI contract actions |
| FG-6 | Offline calibration job over retained JSONL | script + receipt `futurist-calibration-*.json` |
| FG-7 | Docs: expand `docs/FUTURIST_GOVERNOR.md` with equations, gates, non-goals | reviewable |

**Promotion rule:** FG-1..FG-5 + F4 always; F1 on ≥2 retained sessions before “calibrated” badge.

### Track 2 — Process Governor ability (make actuation real & measurable)

**Goal:** when the plant is pressured, Eco/ThermalProtect actually appear and effects are quantified.

| ID | Work | Evidence |
|---|---|---|
| PG-1 | **Ability ledger** per Active session: transitions, duty cycles, dwell deferrals, apply failures | fields on SessionSummary + UI |
| PG-2 | Stress rebalance **experiment** (shadow metric first): report `stress_v2` with higher RAM/ecosystem weight without changing live control | dual-stress on frames; compare |
| PG-3 | Replay lab: force recorded high-RAM windows through candidate eco thresholds; count would-be transitions | replay tests |
| PG-4 | Only if PG-3 shows Eco would fire **and** does not thrash: promote eco trigger to include `homeostatic_slack` or RAM guard as *secondary* enter condition | policy + controller tests; one live matched pair |
| PG-5 | Matched experiment protocol in UI: Baseline capture → Governed → Compare with ability metrics | methodology already in docs; wire defaults (min 60 samples) |
| PG-6 | Failure taxonomy: OpenProcess fail, dwell defer, identity mismatch, verify expiry | events ledger queryable |

**Hard constraints:**

- Do **not** enable Responsive by default.  
- Do **not** raise process priority above Normal without explicit config + test.  
- Do **not** retune kp/ki/kd/kr until two matched pairs exist with agent or process outcome fields.

### Track 3 — Homeostasis closes the loop (policy + display)

Already partially live in `policy.rs` (slack gates + memory guard).

| ID | Work | Evidence |
|---|---|---|
| HS-1 | Persist full homeostasis fields on every v2 frame metrics | fix schema lag vs older installs |
| HS-2 | Vector pressure + slack on Futurist panel and Memory graphs | UI |
| HS-3 | Profile A preset: “8GB chronic RAM” (earlier soft guard, weights displayed) | config profile, not silent gain change |
| HS-4 | Falsification suite: does \(S_H\) predict recovery after next pulse better than mean RAM? | offline on long monitor session |

### Track 4 — Agent outcomes (only path to agent claims)

| ID | Work | Evidence |
|---|---|---|
| AG-1 | Bind a real agent (or synthetic load via `examples/Send-PulseSignal.ps1`) for ≥300 samples | session with signal_fresh |
| AG-2 | A/B shadow vs live directive under memory guard | comparison receipt |
| AG-3 | Outcome fields: latency, tokens/s, success, queue | non-null in ≥50% samples |

Without AG-1..AG-3, agent “improvement” claims remain forbidden.

### Track 5 — Memory & Cortex operations

| ID | Work |
|---|---|
| CX-1 | After every analysis: compact → `Sync-PulseFlow-Cortex.ps1` → recycle raw if receipt ok |
| CX-2 | Index iteration discoveries (transduction, slack, actuation duty) as Cortex cards |
| CX-3 | Keep Cortex no-authority invariant tested |

### Track 6 — Hardening & release (v0.7.0)

| ID | Work |
|---|---|
| RL-1 | Bump version only after FG skill gates + at least one PG matched experiment |
| RL-2 | ARIA full lattice + install smoke + icon launch |
| RL-3 | README: Process Governor vs Futurist Governor two-panel story |
| RL-4 | Reseal MANIFEST; promotion receipt |

---

## 5. Suggested implementation order (concrete)

### Phase A — Plan lock & measurement (now → short)

1. Land this evolution plan.  
2. Implement **PG-1 ability ledger** (no behavior change).  
3. Implement **FG-1/FG-2 multi-horizon API** (advisory only).  
4. Run **FG-6 calibration** on:  
   - `system-monitor-1785226000686`  
   - `system-monitor-1785227305213`  
   - `system-monitor-1785207698005` (sampled)  
   - `msedgeexe-1785206021315` (governed, v1 migrated)  
5. Publish skill receipt; if F1 fails, improve model before UI chrome.

### Phase B — Governor ability experiment

1. PG-2 dual stress (shadow).  
2. PG-3 replay Eco candidacy on high-RAM windows.  
3. Design one **synthetic pressure** run (e.g. controlled memory + Edge) with Baseline≥60 then Governed≥60.  
4. Promote PG-4 only if replay + live NEUTRAL/IMPROVED and thrash-free.

### Phase C — Futurist productization

1. FG-4 envelope suggestions wired into agent policy reasons (still shadow until AG gates).  
2. FG-5 dashboard panel.  
3. HS-1 metrics on all new frames.  
4. Compaction default (0.3).

### Phase D — Agent proof or explicit defer

1. AG-1 synthetic signals if no external agent.  
2. AG-2 A/B.  
3. Otherwise document “agent claims deferred” in README.

---

## 6. Explicit non-goals (v0.7 window)

- Autonomous authority escalation from forecasts  
- Fan, power-limit, undervolt, or firmware control  
- Cloud telemetry  
- Claiming Falcon/SpaceX affiliation or proprietary methods  
- Gain auto-tuning from a single recording  
- Deleting receipts or rollback trees  

---

## 7. Recording study protocol (operator checklist)

When the user says “analyze the recording”:

1. Pause or stop capture; list new `state/sessions/*` since last receipt.  
2. Classify each session: monitor / pre-enable / governed / post / agent-bound.  
3. Compute: CPU/RAM/GPU/thermal, stress, applied modulation, **QoS duty**, transitions, forecast error if v2, homeostasis if present.  
4. Decide promote / hold / need more data with written gates.  
5. Compact → receipt → recycle only with authorization.  
6. Never invent improvement without comparison validity rules in `docs/EXPERIMENTAL_METHODOLOGY.md`.

### Immediate backlog from *current* disk

| Priority | Action |
|---|---|
| P0 | Compact or archive `system-monitor-1785207698005` (46+ MB) after skill calibration |
| P0 | Re-run governed Edge with **Baseline ≥ 60** then **Governed ≥ 60** under similar tabs |
| P1 | Calibrate Futurist on v2 monitors (forecast already ~0.03 MAE@H5) |
| P1 | Investigate why Active Edge never left `normal` — dual-stress experiment |
| P2 | Bind agent signals for memory-guard proof |

---

## 8. Definition of done for “Futurist Governor shipping”

Ship label **Futurist Governor** in UI/README when all true:

- [ ] Multi-horizon forecasts for stress + RAM + ecosystem pressure  
- [ ] Skill receipt with F1 pass on ≥2 sessions  
- [ ] Envelope suggestions advisory-only; authority tests green  
- [ ] Dashboard panel with confidence and risk  
- [ ] Docs + ARIA + manifest resealed  
- [ ] Process Governor ability ledger visible so foresight is not confused with actuation  

Ship label **Process Governor proven (desktop Edge class)** when:

- [ ] ≥2 valid baseline/governed comparisons  
- [ ] Ability ledger shows intentional actuation **or** principled Normal-only with NEUTRAL high-quality evidence  
- [ ] No increase in thermal peak or RAM slope attributable to enable  

---

## 9. Version sketch

| Version | Theme |
|---|---|
| 0.6.x | Hotfix only: install, compaction, ability metrics, docs |
| **0.7.0** | Futurist Governor first-class + ability ledger + dual-stress shadow |
| 0.7.x | Optional Eco secondary triggers if evidence passes |
| 0.8.0 | Agent outcome loop + Cortex default path |

---

## 10. First commands for the next coding session

```powershell
cd C:\Users\jacks\OneDrive\Desktop\pulseflow-governor
# 1) Confirm tree builds
cargo test
# 2) Optional: start lab
cargo run --release -- serve
# 3) After any promote
powershell -ExecutionPolicy Bypass -File .\scripts\ARIA-Verify.ps1
```

Recommended first code slice (smallest valuable delta):

1. `SessionSummary` ability fields + analytics population (PG-1)  
2. `forecast_trajectory` multi-horizon wrapper + tests (FG-1/FG-2)  
3. Offline calibration script over `state/sessions` (FG-6)  

---

## 11. Handoff note

Codex delivered a serious authority-and-evidence foundation through ~0.3.1→0.6.0 features (authority, icon, install, memory guard, homeostasis).  
The open scientific gap is **not** more chrome: it is **(1) measurable process actuation under the pressures this machine actually has, and (2) elevating Futurist from a single linear forecast into a calibrated multi-horizon governor surface that still cannot promote itself.**

This plan freezes that gap as the work.
