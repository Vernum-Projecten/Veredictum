- `veredictum validate` reads the Arazzo catalogue only. Its summary line
  counts what it read (documents, workflows, rows files and capability
  folders) beside the findings, `--statement` holds a declaration to the
  published statement schema and to the catalogue's capability folders and
  option arms, and `--write-report` is gone with the coverage report it wrote.
- `veredictum verdicts` reads the performance cases from the performance
  schedule its `--root` names (`perf/`) and writes `verdicts.json` with the
  re-derived class verdicts and the review of the performance claim. The
  conformance documents and badges it rendered from the case-core judgement
  are gone with it.
- The container image carries `catalogue/` and `specs/` only, and its
  `VEREDICTUM_ROOT` is `/work/catalogue`.
