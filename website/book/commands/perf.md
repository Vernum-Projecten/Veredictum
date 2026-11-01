Latency is measured from the planned arrival instant under open-loop Poisson
arrivals, so a stall shows up as latency. The run has one shape and no flag
lengthens it: the ladder finds the knee in at most ten minutes after seeding,
and the record states what such a run cannot show.
[Measured performance](running.md#measured-performance) walks a run phase by
phase and explains the record.

- **`--root`:** `perf/` in this repository: workloads, the payloads they send,
  and host prescriptions.
- **`--workload`:** one workload document, embedded or your own (labelled
  `custom`).
- **`--catalogue`:** read only for the capability folders and option arms a
  workload's `requires` tokens name.
- **`--statement`:** drops the interactions your claims do not cover, and
  records the drop.
- **`--corpus-state`:** where the seeded corpus listing is stored (default:
  the record's path with extension `corpus.json`). Seeding runs before the
  clock, closed-loop.
- **`--corpus reuse`:** reuses that corpus; refused before the warm-up when the
  server's read-back counts, the stored digest or the recipe moved.
- **`--posture`:** the posture profile the canaries check after seeding and
  after the last plateau. A contradiction refuses the run.
- **`--host-prescription`:** a prescription under `perf/hosts/`, embedded in
  the record and read against the generator's measured fingerprint.
- **`--network-leg`:** where the generator sat.

The IXIT's `environment` block is mandatory, because a latency number is a
claim about a deployment. Every member is required here (`hardware_class`,
`cores`, `memory_gb`, `storage_class`, `topology`), and a missing one stops the
run before any request, naming it. A conformance run reads only
`exclusive_server`. The optional `containers` block names the server and
database containers and the `database` the server-side readings query.
