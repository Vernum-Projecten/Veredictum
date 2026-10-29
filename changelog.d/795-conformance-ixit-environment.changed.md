- An IXIT's `environment` block may declare `exclusive_server` alone
  (`schemas/ixit.schema.json`). `hardware_class`, `cores`, `memory_gb`,
  `storage_class` and `topology` are required only by `perf`, `stress` and
  `aql-probe`, which refuse an IXIT that lacks one and name the member.
- `instances` is optional in the published IXIT schema, because `test` reads
  no instance; every instrument that addresses one still refuses an IXIT
  without the `sut` instance.
- `test` checks its IXIT against the published IXIT schema and the typed
  model before anything runs, and stops with exit status 2 naming the file
  and the violation; it used to read the IXIT unchecked, so a misspelled
  member read as an undeclared one.
