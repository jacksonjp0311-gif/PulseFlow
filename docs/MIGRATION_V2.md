# Observation v1 to v2 Migration

`pulseflow.observation.v2` separates `control_authority` and
`applied_modulation`, adds the futurist forecast, coherence, and controlled
turbulence metrics, and records applied modulation in the action frame.

The reader accepts v1 and v2. A v1 frame's ambiguous `modulation` value is never
treated as proof of intervention: migration sets authority and applied
modulation to zero, then marks the in-memory frame v2. Existing JSONL files are
not rewritten.

Unsupported versions, unsafe session IDs, zero sequences, non-finite values,
or bounded metrics outside their contracts are rejected with the file and line
number. They are not silently admitted to analytics.

Generic `system-monitor-*` and target-specific segments remain separate.
Starting a target connection, baseline, governed window, or disconnect creates
a new segment rather than merging subjects.
