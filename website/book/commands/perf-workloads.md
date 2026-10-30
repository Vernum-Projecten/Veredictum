A workload is the traffic a measured run offers: a versioned document under
`perf/workloads/`, published as `schemas/perf-workload.schema.json`. This
command loads every workload through its gates and prints, per workload, the
expanded per-operation mix, the read:write ratio and the planned run range
after seeding. A defect is reported against the file and the field, and the
command exits `1`.

With `--out` it writes `perf-workloads.json`, the byte-deterministic manifest
of the workloads this binary pins by sha256, and `perf-workloads.md`, the page
generated from it: every interaction, operation, query class and payload with
its share and its source. A committed workload whose bytes match no pin is
refused there, so a changed workload raises its version and its pin together.

With `--workload` it checks one document of your own instead. A document that
matches no pinned digest is labelled `custom`.
