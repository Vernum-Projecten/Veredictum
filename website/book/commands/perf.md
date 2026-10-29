Conformance by measurement. Latency is measured from the planned arrival instant
under open-loop offered load, so a stall shows up as latency instead of
disappearing into a slowed-down request rate. A longer `--hours` window is a
stricter demonstration and persists like any measured run; nothing shorter than
the case exists.

`--root` is the performance schedule, `perf/` in this repository: the class
cases, the journey catalogue and the payloads the journeys send. `stress`,
`aql-probe` and `perf-assets` read the same tree, and none of them reads
`catalogue/`.

The `environment` block is mandatory rather than optional because a latency
number is a claim about a deployment, and a claim with no deployment described
cannot be checked or reproduced.
