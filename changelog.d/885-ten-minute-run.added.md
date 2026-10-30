- `veredictum perf --workload <file> --out <record>` runs a workload document
  against a live server: a closed-loop seeding reported apart (or `--corpus
  reuse` of a stored corpus, refused before the warm-up when its read-back
  counts or digest differ), then settle, warm-up, a 60 s scout ramp, four
  plateaus at 0.5, 0.75, 0.9 and 1.05 of the knee estimate with an extra
  plateau at 0.25 when the lowest fails, and the closing canary, at most ten
  minutes after seeding. The planned duration is printed before the first
  measured request, every phase's wall time is in the record, each plateau
  carries its qualification, a refused optional query is counted apart from
  errors, and a `429` anywhere in the run refuses that run's record. The
  class run (`--class`, `--hours`) stays beside it and the two are mutually
  exclusive.
