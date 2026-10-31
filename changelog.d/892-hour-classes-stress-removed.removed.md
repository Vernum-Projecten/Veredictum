- **Breaking:** the class run, the sustained hour and the stress ladder are
  removed. Removed commands: `stress`, `stress-compare` and `verdicts`.
  Removed flags: `perf --class`, `--hours`, `--results`, `--sut-name` and
  `--sut-version` (`perf` now takes `--workload` and `--out` and nothing
  else selects a run), and `perf-assets --root` and `--stress` (`perf-assets`
  reads a `perf-result` only). Removed with them: the `PT1H` window, the
  `PT5M` warm-up, the diurnal curve and the even arrival grid; `perf/classes/`
  and `perf/journeys.yaml`; the schemas `performance-case.schema.json`,
  `journey-catalogue.schema.json`, `stress.schema.json` and the
  measured-campaign `results.schema.json` (the Arazzo
  `arazzo-results.schema.json` is unchanged); the class verdict and its
  `earned` / `not-earned` token; the committed `results.example.json`; and
  the statement's `performance` block, which `statement.schema.json` now
  refuses with the field named. The corpus scale names POC, S, L and R stay,
  as names of corpus scales. This breaks FerroEHR's `scripts/conformance.sh`,
  which sets `CONF_PERF_CLASS` and `CONF_PERF_HOURS`.
- The payload manifest `perf/payloads.yaml` is gated by the workload loader:
  every source is a file under `payloads/`, a `sha256` pin is checked, and an
  entry no workload reads is refused. The entries only the removed journeys
  read are gone.
