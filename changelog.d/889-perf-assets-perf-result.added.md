- `veredictum perf-assets --results <perf-result.json> --out <dir>` draws
  three charts from a `perf-result`: `perf-saturation.svg`, p50, p95 and p99
  against achieved throughput over the scout windows (hollow circles on thin
  lines) and the plateaus (squares on thick lines), with the knee bracket
  shaded and the Universal Scalability Law fit as a dashed curve of mean
  latency; `perf-throughput.svg`, achieved against offered throughput with
  the identity line; and `perf-operations.svg`, each operation's latency at
  the knee at the quantile it is judged at, beside its budget. A quantile
  that does not resolve is marked below the plot and never joined to its
  neighbours, every chart restyles for a dark colour scheme, and the same
  record always draws the same bytes. `--summary` writes the
  `export --markdown` view of the record. `--root` is now needed only for a
  class `results.json`, and a `perf-result` refuses `--root` and `--stress`.
