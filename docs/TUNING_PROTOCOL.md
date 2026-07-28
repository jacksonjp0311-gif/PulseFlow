# Tuning protocol

## Baseline

Use a repeatable workload and record at least several hundred finalized samples. Capture model, task type, context size, operating mode, room/ambient assumptions, and whether any other heavy processes were active.

## Candidate generation

Change one parameter family at a time:

- filter: `filter_alpha`;
- response: `kp`, `ki`, `kd`;
- residual memory: `kr`, `residue_decay`;
- actuator smoothness: `slew_per_sample`;
- operating envelope: setpoints and hysteresis thresholds.

## Promotion gates

A candidate should not advance merely because one metric improved. Require:

- no thermal safety regression;
- no material increase in QoS chatter;
- equal or better task success;
- repeatable results across multiple paired runs;
- bounded gains and a known rollback profile;
- shadow disagreement analysis before live agent-policy application.

## A/B method

Alternate baseline and candidate runs when possible to reduce time-order bias. Compare the same workload duration or same completed work. Treat differences smaller than normal run-to-run variance as inconclusive.
