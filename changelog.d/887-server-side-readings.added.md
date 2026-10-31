- A `perf --workload` record carries server-side readings beside every
  plateau, as context that never bears on its qualification: per declared
  container the cumulative CPU, memory, block and network counters from the
  Docker Engine API stats endpoint every 2 s of the hold, the database and
  per-schema sizes at the hold's start and end, and the ten statements
  `pg_stat_statements` counted most time for over the hold, plus the
  PostgreSQL settings once per run. Each reading is `present` or `absent`
  with its reason, a failed container reading ends that series with the
  reason, and no failed reading fails the run. `derived` summarizes each
  series as CPU mean and maximum, peak resident memory and I/O, and the
  database growth, so `perf-judge` reproduces them. The readings never write
  to the database: without the `pg_stat_statements` extension the statement
  table is absent with the server's reason.
- The IXIT `containers` block takes an optional `database`, the PostgreSQL
  database in the `db` container whose sizes and statements a run reads;
  without it those readings are recorded absent.
