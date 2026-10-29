- The measured instruments read a tree of their own, `perf/` (#746):
  `classes/` holds the four performance cases, `journeys.yaml` the journey
  catalogue, `payloads.yaml` a manifest of every body and operational
  template a journey or the seeder sends, and `payloads/` those files. `perf`,
  `stress`, `aql-probe` and `perf-assets` take this tree as `--root` and read
  nothing from `artifacts/`, so the cut-over can delete the case files,
  bindings and corpus without taking the instruments with them. Loading the
  tree checks what the instruments depend on: every workload expands inside
  the read-heavy band, every stage template and auxiliary payload is in the
  manifest, every manifest file exists, and the manifest carries nothing no
  stage reads. The new manifest is published as
  `schemas/perf-payloads.schema.json`. The files are copies, byte-identical
  to the case-core tree's, which keeps its own until the cut-over, and a test
  holds the two together. Pass `--root perf` where `--root artifacts` was
  passed before.
