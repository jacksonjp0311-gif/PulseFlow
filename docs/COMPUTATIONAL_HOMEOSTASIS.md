# Computational Homeostasis Theory

## Status

This document defines a provisional, falsifiable model implemented by
PulseFlow Governor v0.5.0. It is an engineering hypothesis, not a validated
law, medical analogy, or claim of exclusive novelty.

## 1. Observation that forced the model

In a normal desktop-activity session, system memory pressure increased while
the selected browser PID represented only a small fraction of used memory.
CPU bursts were only partly attributable to that PID. At the same time,
controller effort was low and the controller's capacity signal was high.

Those facts can coexist because an application process is not an isolated
plant. It participates in a larger state containing process families,
schedulers, caches, shared services, device queues, memory residency, human
input, and delayed work.

The first correction is therefore ontological:

> The selected PID is an authority boundary. It is not assumed to be the
> causal boundary of the observed system.

The first v0.5.0 evaluation of that session measured ecosystem pressure
`0.578`, latent pressure `0.044`, homeostatic slack `0.378`, positive pressure
momentum of `0.0219/min`, recovery balance `-0.055`, resource coupling `0.228`,
and a detected pulse-recovery half-life of `2.10 s`. The selected Edge PID
represented only `3.13%` of used host memory. These values motivate the model;
they do not validate it.

## 2. Relationship to existing work

The model is informed by established fields:

- Linux Pressure Stall Information measures time in which work is delayed by
  CPU, memory, or I/O scarcity and supports event-triggered pressure monitors.
- Autonomic-computing research has described self-configuring,
  self-optimizing, self-healing systems and has explicitly studied
  homeostasis, attractors, and basins in computational state spaces.
- Queueing and congestion-control systems regulate admitted work using delay,
  loss, queue state, feedback clocks, pacing, and recovery.
- Feedback control contributes bounded actions, state estimation, stability
  criteria, hysteresis, and falsification.

PulseFlow does not rename any one of these. Its hypothesis is that a useful
desktop governor needs their intersection: whole-machine resource motion,
natural workload pulses, recovery dynamics, target-scoped authority, causal
outcome alignment, and explicit experimental epochs.

Primary references:

- [Linux PSI documentation](https://docs.kernel.org/accounting/psi.html)
- [IBM: Meta dynamic states for self-healing autonomic computing systems](https://research.ibm.com/publications/meta-dynamic-states-for-self-healing-autonomic-computing-systems)
- [IBM: Unity autonomic computing system](https://research.ibm.com/publications/unity-experiences-with-a-prototype-autonomic-computing-system)
- [RFC 9937: Proportional Rate Reduction](https://www.rfc-editor.org/rfc/rfc9937.html)

## 3. State and pressure

Let the observable normalized pressure vector be

\[
\mathbf{z}_t =
[z_{\mathrm{cpu}},z_{\mathrm{ram}},z_{\mathrm{gpu}},
z_{\mathrm{thermal}},z_{\mathrm{queue}},z_{\mathrm{latency}}]_t
\]

Only available channels participate. Each channel is bounded to \([0,1]\).
The ecosystem-pressure estimator combines average pressure with the dominant
bottleneck:

\[
P_t =
\frac{1}{2}\max_i z_{i,t}
+ \frac{1}{2N}\sum_{i=1}^{N}z_{i,t}
\]

This prevents abundant GPU or queue headroom from concealing a nearly
exhausted memory channel while remaining less brittle than a pure maximum.

## 4. Latent pressure and homeostatic slack

Instantaneous utilization cannot show whether the system is recovering or
still accumulating delayed effects. For a window \(W\):

\[
A_W = \max(0,P_{\mathrm{last}}-P_{\mathrm{first}})
\]

\[
L_W = \frac{1}{|W|}\sum_{t\in W}|r_t|
\]

where \(r_t\) is bounded controller residue memory. Provisional latent
pressure is:

\[
\Lambda_W = \operatorname{clip}(A_W+L_W,0,1)
\]

Homeostatic slack is then:

\[
S_H(W)=\operatorname{clip}
(1-\overline{P}_W-\Lambda_W,0,1)
\]

\(S_H\) is not free CPU. It is a conservative estimate of remaining reserve
after current ecosystem pressure, net accumulation, and unresolved residue
are accounted for.

## 5. Recovery dynamics

For adjacent observations separated by \(\Delta t\):

\[
v_t = \frac{P_t-P_{t-1}}{\Delta t}
\]

Accumulation and recovery rates are calculated separately:

\[
\rho_+ = \operatorname{mean}(v_t\mid v_t>0), \qquad
\rho_- = \operatorname{mean}(-v_t\mid v_t<0)
\]

Recovery balance is:

\[
B_R = \frac{\rho_- - \rho_+}{\rho_-+\rho_+ + \epsilon}
\]

\(B_R\in[-1,1]\). Positive values mean observed recovery intervals are
stronger than accumulation intervals; negative values mean accumulation is
winning.

Pressure momentum reports the signed net change per minute:

\[
M_P = 60\frac{P_{\mathrm{last}}-P_{\mathrm{first}}}
{t_{\mathrm{last}}-t_{\mathrm{first}}}
\]

For a detected pulse peak at \(P_k\), PulseFlow reports a recovery half-life
when pressure subsequently reaches halfway from the peak toward its local
pre-pulse baseline. No value is emitted when the pulse never demonstrably
recovers inside the recording.

## 6. Resource coupling and attribution

Resource coupling is the mean absolute Pearson correlation among the
first-difference traces of CPU, RAM, and GPU when sufficient variance and
samples exist:

\[
C_R = \operatorname{mean}_{i<j}
\left|\operatorname{corr}(\Delta z_i,\Delta z_j)\right|
\]

It measures coordinated motion, not causation.

Target memory share is:

\[
Q_{\mathrm{target}} =
\frac{\text{selected-process memory}}
{\text{total used host memory}}
\]

A small value explicitly warns that target-scoped authority cannot explain
most system memory state. Future process-family aggregation may improve
coverage, but it still will not turn correlation into causal attribution.

## 7. Control interpretation

PulseFlow maintains three different quantities:

1. `capacity_signal`: controller-estimated workload headroom.
2. `control_authority`: verified permission to act on a bounded target.
3. `controller_effort`: event-triggered effort applied in the interval.

Homeostatic slack is a fourth quantity: estimated system recovery reserve.
None is a synonym for another.

The agent policy uses RAM tiers and homeostatic slack only to contract
recommendations. These measurements cannot expand process authority. Above
85% RAM, concurrency and batch size receive absolute caps; the caps tighten
again above 90% and 95%.

## 8. Falsification program

The model must be rejected or revised if any of the following persists across
controlled repeated experiments:

1. \(S_H\) does not predict pulse recovery time or pressure accumulation on
   held-out sessions better than simpler utilization-only baselines.
2. Estimated recovery balance has no stable relationship with observed
   post-pulse recovery.
3. Resource coupling is dominated by sampling artifacts or disappears under
   higher-rate measurement.
4. Similar workloads under similar conditions produce materially
   irreproducible estimates.
5. Slack-driven contraction worsens tail latency, throughput, thermal
   pressure, stall time, or recovery half-life.
6. Process-family aggregation fails to improve attribution coverage.

## 9. Evidence still required

- Longer normal-activity recordings with deliberate quiet recovery tails.
- Repeated identical pulse workloads at multiple starting memory pressures.
- Windows PSI comparison on a comparable Linux machine, or equivalent Windows
  stall/delay instrumentation where available.
- Complete process-family observation without broadening control authority.
- Outcome adapters that report workload latency, throughput, and completion.
- Pre-registered baseline/candidate thresholds for predictive usefulness.

Until those experiments succeed, homeostatic slack remains a transparent
engineering estimator—not a performance claim.
