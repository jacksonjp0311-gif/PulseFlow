# Baseline-to-Governed Method

1. Discover, connect, and verify the same target executable/PID.
2. Select **Capture Baseline**. This opens a target-specific v2 segment with
   applied modulation fixed at zero.
3. Observe at least 30 finalized samples under a representative workload.
4. Enable PulseFlow. A new governed segment preserves the subject lineage.
5. Run a comparable workload window, then suspend the governor.
6. Compare the two saved sessions in Analytics.

The summary records CPU mean/variance/peak/bursts, GPU mean/variance/peak when
available, RAM mean/slope, thermal mean/slope/peak, latency p95, queue activity,
residue memory, coherence, prediction error, flow stability, process CPU,
applied modulation, samples, and detected sequence gaps.

Comparison is invalid when sample counts are below 30, duration is zero, target
identity differs, sample interval differs by more than 10%, average stress
differs by more than 0.20, or dropped samples exceed 10%. Evidence quality
combines sample coverage and duration similarity. Verdicts are `IMPROVED`,
`NEUTRAL`, `REGRESSED`, or `INCONCLUSIVE`; a single transient frame can never
produce a conclusive verdict.

Repeat matched trials and control ambient temperature and workload inputs
before interpreting a descriptive improvement as causal.
