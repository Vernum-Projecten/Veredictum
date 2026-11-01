A workload is the traffic a measured run offers: a versioned document under
`perf/workloads/`, published as `schemas/perf-workload.schema.json`. This
command loads every workload through its gates and prints, per workload, the
expanded per-operation mix, the read:write ratio and the planned run range
after seeding. A defect is reported against file and field, with exit `1`.

- **`--out`:** writes `perf-workloads.json`, the byte-deterministic manifest of
  the workloads this binary pins by sha256, and `perf-workloads.md`, the page
  generated from it (every interaction, operation, query class and payload
  with its share and source). The book's
  [Performance workloads](perf-workloads.md) page is written with
  `--out website/book/src` and held to the pins by a test. A committed
  workload whose bytes match no pin is refused, so a workload's version and
  pin change together. `perf/payloads.yaml` is held to what the workloads
  read: an unread entry or a pinned payload whose bytes moved is refused.
- **`--workload`:** checks one document of your own. One matching no pinned
  digest is labelled `custom`.
