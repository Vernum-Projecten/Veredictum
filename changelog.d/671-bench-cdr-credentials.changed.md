- `veredictum bench` reads its credential from the variables `test` and
  `identify` read: `VEREDICTUM_CDR_PASSWORD` and `VEREDICTUM_CDR_TOKEN`, each
  with its `_FILE` form, and `VEREDICTUM_CDR_USER` when `--user` is absent.
  `VEREDICTUM_BENCH_PASSWORD` and `VEREDICTUM_BENCH_TOKEN` are no longer read.
