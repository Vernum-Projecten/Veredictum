- An IXIT's `environment` block may declare `exclusive_server` alone
  (`schemas/ixit.schema.json`). `hardware_class`, `cores`, `memory_gb`,
  `storage_class` and `topology` are required only by `perf`, `stress` and
  `aql-probe`, which refuse an IXIT that lacks one and name the member.
