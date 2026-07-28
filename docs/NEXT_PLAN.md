# PulseFlow Next Plan — v0.7.1 → v0.8

**Date:** 2026-07-28  
**Runtime inspected:** live `0.7.0` on constrained desktop (~7.9 GB)  
**UI just shipped:** corner **Oscillation** + **Reverberation** scopes with shared λ-lock and heat-softened circular gradients  
**GCMT v1.3 bridge:** top-corner condition/arbiter chips + full plan in [`GCMT_PULSEFLOW_EVOLUTION.md`](GCMT_PULSEFLOW_EVOLUTION.md) (v0.8 north star)  


---

## 1. Runtime inspection (live sample)

| Signal | Observed | Read |
|---|---:|---|
| Version | 0.7.0 | Install path current |
| Form factor | `constrained_desktop` | RAM-first profile correct |
| Samples (session) | ~129 | Short window; good for UI, thin for causal claims |
| CPU / RAM | ~61% / ~83% | Host still memory-bound |
| Filtered stress | ~0.54 | Mid-band, phase `advance` |
| Oscillation coherence | ~0.74 | Usable lock; not glassy-smooth |
| Turbulence | `controlled_turbulence` | Bounded motion |
| Residue memory | ~0.01–0.02 | Small echo — reverb scope will look quiet until load pulses |
| Forecast confidence | ~0.09 | Linear H=5 still weak on desktop bursts |
| Eco duty | 1.0 | Eco assist engaged under high RAM (good) |
| Actuation rate | ~0.18 / min | Governor is moving, lightly |
| Homeostatic slack | ~0.34 | Moderate reserve |
| Ecosystem pressure | ~0.58 | Dominant plant pressure |
| Futurist envelope | `hold` | Advisory calm despite Eco process state |
| Installed learning blobs | 8+ | Graph memory path in use |

### Interpretation

1. **Plant is memory-primary.** Oscillation will often be RAM-led, not CPU-led.  
2. **Eco is firing; forecasts are still soft.** Process actuation ≠ predictive skill.  
3. **Residue is low in quiet stretches.** Reverberation chart needs deliberate load pulses to show “after-ring.”  
4. **Coherence ~0.74** is the right regime for λ-lock demos: two waves related but not identical.  
5. **Heat metaphor fits thermal + smoothness:** GPU ~cool now → low heat field; under load, softness should rise and smooth sharp wave edges (annealing).

---

## 2. UI just landed (this change)

On the main **Modulation** view:

| Corner instrument | Source | Shared lock |
|---|---|---|
| **Oscillation** (bottom-left) | Raw vs filtered stress | Dominant λ from filtered zero-crossings / autocorr |
| **Reverberation** (bottom-right) | \|residue\| echo + recovery envelope | Same λ grid marks |
| **Heat-soft field** (core) | GPU temp → heat; stability/activity → softness | Circular radial gradient “anneals” the surface |
| **λ sync pill** (bottom center) | Period in samples + heat % | Operator-readable lock |

Drawing rules:

- Shared wavelength marks (blue dashed phase lines).  
- Multi-pass smoothing strength scales with **softness** (heat + stability − chaos).  
- Heat paint = warm radial fill + soft elliptical rings (tube / roll surface language).

No authority change. Pure observation / operator vision.

---

## 3. Next engineering tracks

### Track A — Wavelength science (v0.7.1)

| ID | Work | Gate |
|---|---|---|
| A1 | Persist `dominant_period_samples` + `phase_offset` on `RuntimeMetrics` | unit test on synthetic sine |
| A2 | Backend residual impulse response (reverb half-life) | matches UI reverb decay within 20% |
| A3 | Dual-channel λ: stress vs RAM; report lock ratio | UI shows “λ stress / λ ram” |
| A4 | When \|λ_stress − λ_ram\| / λ &lt; 0.15, raise “resonance” flag | receipt field |

### Track B — Heat as control metaphor (shadow only)

| ID | Work | Gate |
|---|---|---|
| B1 | Define `surface_softness = f(thermal, residual_activity, stability)` | documented equation |
| B2 | Shadow policy: high softness → prefer Eco dwell extension (not gain change) | replay only |
| B3 | Live promote only after matched baseline/governed with Eco duty evidence | methodology doc |

**Do not** treat “heat” as hardware thermal control. It is a **smoothing / dwell** metaphor only.

### Track C — Forecast skill (Futurist 0.8)

| ID | Work | Gate |
|---|---|---|
| C1 | Mean-reverting + velocity hybrid (beats persist on ≥2 blobs) | F1 from evolution plan |
| C2 | Train light weights offline on learning-datasets | skill receipt |
| C3 | Show forecast ghost wave on Oscillation scope (H=5) | UI only after F1 |

### Track D — Operator loop

| ID | Work |
|---|---|
| D1 | After every recording: Learn → Memory graph → note λ and reverb half-life |
| D2 | Design one “pulse train” workload (tab storm / compile / agent batch) to excite reverb |
| D3 | Reinstall desktop icon after each release (`Install-PulseFlow.ps1`) |

### Track E — Hardening

| ID | Work |
|---|---|
| E1 | ARIA smoke asserts `/` HTML contains oscillation-scope + reverberation-scope |
| E2 | Version bump 0.7.1 with chart-only UI + metric fields |
| E3 | Compact installed session JSONL when &gt; 2 MB |

---

## 4. Recommended experiment sequence (your tube-watch discipline)

1. **Observe** 3–5 min monitor-only with λ lock visible.  
2. **Pulse** the plant (deliberate load) — watch Oscillation wavelength and Reverberation after-ring.  
3. **Heat note:** if GPU/temp rises, softness should blur sharp edges; if chaos rises, edges stay hard.  
4. **Govern** matched window; compare Eco duty vs reverb settling time.  
5. **Learn** session → blob → delete raw → Memory graph of stress vs RAM.  
6. Promote only what the blob + ability ledger support.

---

## 5. Success criteria for “wavelength sync”

Ship claim **“λ-locked dual scopes”** when:

- [ ] Both corner charts always share the same period marks  
- [ ] Period estimate stable (±20%) over 60 s steady load  
- [ ] Operator can see raw lag/lead vs filtered without reading JSON  
- [ ] Reverb chart shows visible decay after a real load pulse  
- [ ] Heat field moves with thermal/softness, never with authority  

---

## 6. Immediate next code slice (when you say go)

1. A1/A2 metric fields + tests  
2. Ghost Futurist H=5 on oscillation scope (display only)  
3. Reinstall package so desktop always serves latest embedded UI  
4. One designed pulse-train recording + analysis receipt  

---

## 7. Non-goals

- Auto-enable from wavelength lock  
- Real metal/thermal process control  
- Hiding Eco assist when RAM is high  
- Keeping multi-10 MB JSONL after analysis  
