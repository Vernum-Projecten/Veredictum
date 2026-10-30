- **Breaking:** the `bench`, `bench-compare` and `bench-packs` commands, the
  `bench` library module, `schemas/bench-result.schema.json` and
  `schemas/bench-packs.schema.json` are removed, with the landing site's
  generated benchmark methodology page and its `bench-packs.json`. `bench`'s
  three repetitions and its "at least three repetitions" submittability rule
  give way to one pass per record and the interleaved A/A comparison of
  `perf-compare`. A script that runs any of the three commands stops
  working.
