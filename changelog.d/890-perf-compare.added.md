- `veredictum perf-compare` reads `perf-result` records. `--result` (twice
  or more) aligns committed records into one table and names every mismatch
  above it: the workload digest, the corpus recipe, the posture, the host
  prescription, the generator's fingerprint and the network leg; any
  mismatch exits `1`. `--ab <IXIT_A> <IXIT_B>` seeds both targets from the
  same recipe and seed, gives each its settle, warm-up and scout, builds the
  shared ladder from the larger knee estimate, and runs the plateaus ABBA per
  rung; `--aa <IXIT>` runs one target on both sides and attaches its floor,
  the largest A/A ratio per metric, to both records. A ratio counts as a
  difference only past the declared `--tolerance` and the attached floor
  (`--floor`), with the two 95 % intervals apart. Each interleaved run writes
  both records, `perf-compare.json` (`schemas/perf-compare.schema.json`) and
  `perf-compare.md`.
- Host prescriptions are versioned data under `perf/hosts/<id>.yaml`,
  published as `schemas/perf-host.schema.json` and loaded through gates; the
  first is `reference-8cpu-8gb`, the idle 8-CPU, 8 GB reference host.
  `perf --host-prescription <id>` embeds the prescription in the record, and
  `perf --network-leg` names where the generator sat.
- A `perf-result` gains `raw.network_leg`, `raw.interleaving`, `raw.aa_floor`
  and `derived.host`, which reads the measured fingerprint against the
  claimed prescription as `matched`, `unmatched` (each contradiction named),
  `unverifiable` or `unclaimed`. `derived.between_run_noise` is
  `unmeasured`, or the attached A/A floor. `raw.host_prescription` is now the
  embedded prescription rather than its name.
