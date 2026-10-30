Conformance by measurement. Latency is measured from the planned arrival instant
under open-loop offered load, so a stall shows up as latency instead of
disappearing into a slowed-down request rate. A longer `--hours` window is a
stricter demonstration and persists like any measured run; nothing shorter than
the case exists.

`--root` is the performance schedule, `perf/` in this repository: the class
cases, the journey catalogue and the payloads the journeys send. `stress`
and `perf-assets` read the same tree, and none of them reads `catalogue/`.

`--results` names the measured record. When no file exists there, `perf`
creates it: before it seeds anything, it reads the server's identity through
System OPTIONS, and `--sut-name` and `--sut-version` supply the name and
version the server does not state. The new record holds the measurements and
no conformance outcome. No party statement selected the run, so it is
`statement_blind` with the defaulted technology profile, and its verification
pack is `not_run`. When the file exists, `perf` keeps its header, replaces any
earlier record for the same case and adds the new one. `verdicts` and
`perf-assets` read either kind of record.

The `environment` block is mandatory rather than optional because a latency
number is a claim about a deployment, and a claim with no deployment described
cannot be checked or reproduced. Every member of it is mandatory here
(`hardware_class`, `cores`, `memory_gb`, `storage_class`, `topology`), and a
run whose IXIT lacks one stops before it sends a request, naming the member.
A conformance run reads only `exclusive_server`, so a conformance-only IXIT
may leave the others out.
