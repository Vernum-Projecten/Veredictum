Latency is measured from the planned arrival instant under open-loop Poisson
arrivals, so a stall shows up as latency instead of disappearing into a
slowed-down request rate. The run has one shape, and no flag lengthens it: the
ladder finds the knee in at most ten minutes after seeding, and the record
says what such a run cannot show. [Measured performance](running.md#measured-performance)
walks a run phase by phase and explains how to read the record.

`--root` is `perf/` in this repository: the workloads, the payloads they send
and the host prescriptions. `--workload` names one workload document, an
embedded one or your own, which is labelled `custom`. `--catalogue` is only
read for the capability folders and option arms a workload's `requires`
tokens name, and `--statement` drops the interactions your claims do not
cover, with the drop recorded.

The corpus is seeded before the clock, closed-loop, and its listing is stored
beside the record (`--corpus-state`, by default the record's path with the
extension `corpus.json`). `--corpus reuse` reuses that corpus instead of
seeding again, and is refused before the warm-up when the server's read-back
counts, the stored digest or the recipe moved.

The `environment` block is mandatory because a latency number is a claim
about a deployment, and a claim with no deployment described cannot be checked
or reproduced. Every member of it is mandatory here (`hardware_class`,
`cores`, `memory_gb`, `storage_class`, `topology`), and a run whose IXIT lacks
one stops before it sends a request, naming the member. A conformance run
reads only `exclusive_server`, so a conformance-only IXIT may leave the others
out. The optional `containers` block names the server and database containers
and the `database` the server-side readings query.

`--posture` declares the posture profile the canaries check after seeding and
after the last plateau; a contradiction refuses the run. `--host-prescription`
names a prescription under `perf/hosts/`, which the record embeds and reads
the generator's measured fingerprint against, and `--network-leg` says where
the generator sat.
