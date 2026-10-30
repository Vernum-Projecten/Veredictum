- `community-vitals` and `aql-mix` are embedded workloads under
  `perf/workloads/`, pinned by digest. `community-vitals` reproduces the
  openEHR community's vital-signs harness: 100 EHRs of 1,000 commits of the
  harness's own composition on one worker, then a closed-loop walk of its
  seven reads over every composition (recorded as `raw.walk` and
  `derived.walk`, labelled closed-loop), then the same reads on the load
  ladder. `aql-mix` offers the six query classes over the same population.
  The template and composition are pinned by `sha256` in
  `perf/payloads.yaml`.
- A workload document may name the harness it `reproduces`, which exempts its
  mix from the population read band and is printed wherever it is rendered;
  its corpus may pin `ehrs`, `seed_workers`, a `verbatim` commit form and a
  closed-loop `walk`. The operation vocabulary gains
  `composition_read_at_time`, `versioned_composition_read`,
  `composition_version_read_latest` and `composition_version_read_at_time`.
- The population reading states the EHRs the server held when the clock
  started, as read back, rather than the scale's nominal count.
