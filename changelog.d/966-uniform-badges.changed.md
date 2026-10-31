- Every badge `veredictum badge` draws carries one fact: the label names the
  subject and the message gives one short value, with no `·` separator in
  either. A counting badge reads `312/318 passed`, plus `, 1 warning` or
  `, N warnings`, and its SVG title and `aria-label` carry the full breakdown
  with the failed and errored counts. A counting badge drawn from the
  development profile says `dev` in its label (`openEHR BASE dev`), and a
  released one carries no marker. The `instrument` badge reads
  `self-run v<version>`. A `perf-result` draws three badges: `perf`
  (`perf <workload>`, the knee in whole requests per second, rounded outward
  so a bracket only widens), `noise` (`A/A floor`: `attached` or
  `not measured`) and `instrument` (the version that wrote the record). The
  library's `perf_badge` is replaced by `perf_badges`, and `Badge` gains a
  `detail` field holding the full text.
