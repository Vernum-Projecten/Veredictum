- `veredictum test` records the server's System OPTIONS identity once in
  `results.json`, under `server`: from the run's own exchange when a workflow
  sent one, otherwise from one read after the catalogue, which the recording
  keeps so `--rejudge` writes the same bytes without asking the server. The
  stated `restapi_specs_version` is absent when the server states none. When
  it differs from the statement's `spec_versions.its_rest`, `results.json`
  carries a `review` note of kind `restapi-version-differs` and the run prints
  it; the note changes no row and no exit status.
