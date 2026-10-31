- `veredictum show` and `veredictum export --markdown` read a `perf-result`
  as well as an Arazzo results document, told apart by the document's
  `family`. The view names the target, environment, host prescription and
  generator, the workload with its digest (a `custom` one named so), the
  corpus scale, the knee estimate, the between-run noise and the record's
  scope; prints one line per plateau with offered and achieved rates, p50 to
  p99.9 and max, the error share, a ✓ or ✕ qualification mark and `unsteady`
  where recorded; tables the operations at the knee and at the lowest
  plateau with the quantile each is judged at (`n/r` where a quantile does
  not resolve) and names every operation judged below p99; states the knee,
  peak, population and scalability-fit sentences; draws a sparkline of p99
  against the offered rate; and lists the server-side readings as context.
  The terminal view fits 80 columns. `export --junit` and `--ctrf` refuse a
  `perf-result` with exit status 2, and a document naming any other
  `family` is refused by name.
- `app/veredictum/examples/perf-result.example.json` is a committed example
  record from `cargo run --example make_example_perf_result`, held to its
  generator by `scripts/checks/example-results-drift.sh`.
