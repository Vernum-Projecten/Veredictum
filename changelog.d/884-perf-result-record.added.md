- `schemas/perf-result.schema.json` publishes the `perf-result` record: a
  `raw` part holding the run's evidence, with one encoded HDR histogram per
  plateau half and operation, and a `derived` part computed from it. Each
  percentile from p50 to p99.9 comes from summed histograms and carries its
  95 % rank interval, or reads `not_resolvable` with fewer than 10 samples
  above it, and each operation names the quantile it is judged at. Every
  record states `passes: 1`, `between_run_noise: unmeasured` and what a run
  of at most ten minutes cannot show.
- `veredictum perf-judge --result <file>` re-derives every derived number of
  a `perf-result` from its recorded histograms and exits `1` on any byte of
  difference, naming each differing field by its JSON pointer.
