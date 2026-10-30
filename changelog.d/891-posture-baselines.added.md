- The posture canaries bracket every `perf --workload` run and every side of
  a `perf-compare` run: one bracket after seeding, one after the last
  plateau. `perf --posture <profile>` declares `minimal` (the default),
  `minimal-signed-digest`, `minimal-signed-pgp` or `clinical-default`, the
  record carries the posture block, and a reading that contradicts the
  declaration, or two readings that disagree, refuse the record. The
  validation canary commits the workload's invalid twin, named in
  `perf/payloads.yaml` as `<template>.invalid_twin`.
- `perf-compare --with-baselines <IXIT>` composes each pinned reference CDR
  (EHRbase, FerroEHR) on the same host from its digest-pinned images, runs
  the target against it interleaved, writes both records and the comparison
  under `--out/<reference>/`, and tears the stack down with its volumes.
- The workload manifest `perf-workloads --out` writes is published in the
  book as the "Performance workloads" page, held to the pinned workloads by
  a test.
