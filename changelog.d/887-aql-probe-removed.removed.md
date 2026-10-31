- The `aql-probe` command and `schemas/aql-probe.schema.json` are removed, a
  breaking change for any script that runs the probe. The per-plateau
  `pg_stat_statements` table of a `perf --workload` record is the attribution
  the probe gave, read over the plateaus a run holds rather than a separate
  burst.
