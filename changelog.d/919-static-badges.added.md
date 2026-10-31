- `veredictum badge --results <file> --out <dir>` draws static badges for a
  README, each as an SVG and a Shields.io endpoint JSON document, with the
  Veredictum mark as its icon. A conformance result draws five: `base`,
  `options` and `warnings`, each naming the specification profile in its
  label, `profile`, and `instrument`, which says `self-run`. A counting badge
  is green only when no row failed or errored. A `perf-result` draws one,
  `perf`, with the workload and the knee as the record states it, in a
  neutral colour. Every badge is byte-deterministic, and none is a
  certificate.
- The GitHub Action draws the badges into `badges/` in `out-dir` after every
  run, and with the new `badge-branch` input pushes them to that branch of the
  calling repository with the job's `GITHUB_TOKEN`, only from a run on the
  default branch that is not a pull request. The new `badges` output names the
  directory.
- The book has a Badges page: what each badge says, the colour rules, and how
  to publish the badges from GitHub, GitLab or any other CI.
