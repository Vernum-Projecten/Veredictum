# Badges

<!-- toc -->

[`veredictum badge`](commands.md#badge) draws static badges from a run's
result, for the vendor to publish; no badge service is involved. Every badge
is the vendor's own run (`self-run`) and never a certificate. Link each badge
to the run that drew it.

## What they look like

Drawn from test fixtures (instrument version 0.3.1), each with its Markdown.

A passing result judged against the release, with one BASE warning:

![openEHR BASE: 3/3 passed, 1 warning](badges/passing/base.svg)
![openEHR options: 1/1 passed](badges/passing/options.svg)
![specification: released](badges/passing/profile.svg)
![veredictum: self-run v0.3.1](badges/passing/instrument.svg)

```markdown
![openEHR BASE: 3/3 passed, 1 warning](badges/passing/base.svg)
![openEHR options: 1/1 passed](badges/passing/options.svg)
![specification: released](badges/passing/profile.svg)
![veredictum: self-run v0.3.1](badges/passing/instrument.svg)
```

A failing result: one BASE row failed, one BASE and one option row errored:

![openEHR BASE: 2/4 passed, 1 failed, 1 errored](badges/failing/base.svg)
![openEHR options: 0/1 passed, 1 errored](badges/failing/options.svg)
![specification: released](badges/failing/profile.svg)
![veredictum: self-run v0.3.1](badges/failing/instrument.svg)

```markdown
![openEHR BASE: 2/4 passed, 1 failed, 1 errored](badges/failing/base.svg)
![openEHR options: 0/1 passed, 1 errored](badges/failing/options.svg)
![specification: released](badges/failing/profile.svg)
![veredictum: self-run v0.3.1](badges/failing/instrument.svg)
```

The passing result judged against the development snapshot:

![openEHR BASE dev: 3/3 passed, 1 warning](badges/development/base.svg)
![openEHR options dev: 1/1 passed](badges/development/options.svg)
![specification: development (master)](badges/development/profile.svg)
![veredictum: self-run v0.3.1](badges/development/instrument.svg)

```markdown
![openEHR BASE dev: 3/3 passed, 1 warning](badges/development/base.svg)
![openEHR options dev: 1/1 passed](badges/development/options.svg)
![specification: development (master)](badges/development/profile.svg)
![veredictum: self-run v0.3.1](badges/development/instrument.svg)
```

The committed example performance record, with a bracketed knee and no A/A
floor:

![perf custom: knee 180.8–210.9/s](badges/perf/bracket/perf.svg)
![A/A floor: not measured](badges/perf/bracket/noise.svg)
![veredictum: self-run v0.3.1](badges/perf/bracket/instrument.svg)

```markdown
![perf custom: knee 180.8–210.9/s](badges/perf/bracket/perf.svg)
![A/A floor: not measured](badges/perf/bracket/noise.svg)
![veredictum: self-run v0.3.1](badges/perf/bracket/instrument.svg)
```

A lower-bound knee on a pinned workload, every plateau qualified, with an A/A
floor attached:

![perf ehr-mix@2: knee ≥ 1,250.0/s](badges/perf/lower-bound/perf.svg)
![A/A floor: attached](badges/perf/lower-bound/noise.svg)
![veredictum: self-run v0.3.1](badges/perf/lower-bound/instrument.svg)

```markdown
![perf ehr-mix@2: knee ≥ 1,250.0/s](badges/perf/lower-bound/perf.svg)
![A/A floor: attached](badges/perf/lower-bound/noise.svg)
![veredictum: self-run v0.3.1](badges/perf/lower-bound/instrument.svg)
```

Published ([GitHub](#github), [GitLab](#gitlab)) and linked to its run, the
first badge becomes:

```markdown
[![openEHR BASE: 3/3 passed, 1 warning](https://raw.githubusercontent.com/<owner>/<repo>/veredictum-badges/base.svg)](https://github.com/<owner>/<repo>/actions/workflows/conformance.yml?query=branch%3Amain)
```

`scripts/render/book-badges.sh` redraws this gallery;
`scripts/checks/book-badges.sh` fails CI on drift.

## One design for every badge

One fact per badge: a label of at most three words, one short value. The
SVG title and `aria-label` carry the full text, such as
`openEHR BASE: 2/4 passed, 1 failed, 1 errored`.

## The four conformance badges

| Badge | Label | Message |
|---|---|---|
| `base` | `openEHR BASE`, or `openEHR BASE dev` | passed out of the BASE rows and the rows the outline places nowhere that bear on the verdict (passed, failed and errored), then the warning count when there is one: `312/318 passed`, `312/318 passed, 1 warning`, `312/318 passed, 2 warnings` |
| `options` | `openEHR options`, or `openEHR options dev` | the same count over the option arms the statement selected and the cases resting on TRIAL text, which are read with the options |
| `profile` | `specification` | `released`, or `development (master)` with the branch the snapshot was taken from |
| `instrument` | `veredictum` | `self-run v<version>` |

Warnings and skipped rows stay out of the ratio. Without an outline, both
counting badges read `no outline`. Counts match [`show`](commands.md#show).

| Colour | `base` and `options` | `profile` |
|---|---|---|
| green | rows passed, none failed or errored | |
| red | at least one row counts against the server: it failed, or errored with the server (or no one) named in its record | |
| orange | none counts against the server, and at least one errored row is inconclusive (the instrument could not judge it) | |
| grey | nothing passed: no rows, or every row skipped | |
| blue | | `released` |
| purple | | `development` |

The `instrument` badge is deep teal. A warning never changes a colour.

## The performance badges

| Badge | Label | Message |
|---|---|---|
| `perf` | `perf <workload>` | `knee 180–211/s` for a bracket, `knee ≥ 1,250/s` when every plateau qualified, `below ladder` when the lowest plateau failed, `no plateau` when the run had none |
| `noise` | `A/A floor` | `attached` when the record carries an A/A floor, `not measured` when it does not |
| `instrument` | `veredictum` | `self-run v<version>`, the version that wrote the record |

`<workload>` is `<id>@<version>` when pinned, else `custom`. Rates round
outward to whole numbers; the title keeps one decimal. `perf` is always slate
grey-blue; `noise` is slate when attached, grey when not.

## The icon

The Veredictum mark sits at the left (also `logoSvg` in the `.json`). The
openEHR logo is never used: it is openEHR's trademark.

## Drawing the badges

```bash
veredictum badge --results conformance/results.json --out conformance/badges
```

Writes an `.svg` (self-contained) and a `.json`
[Shields.io endpoint](https://shields.io/badges/endpoint-badge) per badge:
`https://img.shields.io/endpoint?url=<the published URL of base.json>`.

## GitHub

With `badge-branch`, [the GitHub Action](github-action.md) pushes the badges
as a single replacing commit, only from a non-pull-request run on the default
branch and only when they changed. It needs `GITHUB_TOKEN` and
`contents: write`:

```yaml
name: openEHR conformance
on:
  push:
    branches: [main]
  pull_request:

permissions: {}

jobs:
  conformance:
    runs-on: ubuntu-latest
    permissions:
      contents: write # the badge push; contents: read without badge-branch
    steps:
      - uses: actions/checkout@<commit-sha> # vN
        with:
          persist-credentials: false

      - uses: Vernum-Projecten/Veredictum@<commit-sha> # vX.Y.Z
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          VEREDICTUM_CDR_TOKEN: ${{ secrets.CDR_TOKEN }}
        with:
          base-url: ${{ vars.CDR_BASE_URL }}
          statement: conformance/statement.json
          auth: bearer
          profile: released
          badge-branch: veredictum-badges
```

In the README:

```markdown
[![openEHR BASE](https://raw.githubusercontent.com/<owner>/<repo>/veredictum-badges/base.svg)](https://github.com/<owner>/<repo>/actions/workflows/conformance.yml?query=branch%3Amain)
[![openEHR options](https://raw.githubusercontent.com/<owner>/<repo>/veredictum-badges/options.svg)](https://github.com/<owner>/<repo>/actions/workflows/conformance.yml?query=branch%3Amain)
[![specification](https://raw.githubusercontent.com/<owner>/<repo>/veredictum-badges/profile.svg)](https://github.com/<owner>/<repo>/actions/workflows/conformance.yml?query=branch%3Amain)
[![veredictum](https://raw.githubusercontent.com/<owner>/<repo>/veredictum-badges/instrument.svg)](https://github.com/<owner>/<repo>/actions/workflows/conformance.yml?query=branch%3Amain)
```

GitHub caches raw files for five minutes.

## GitLab

[The GitLab CI template](gitlab-ci.md) keeps the badges as artifacts, served
from a branch's latest successful pipeline. With the default `out-dir` and
`job-name`:

```text
https://gitlab.com/<namespace>/<project>/-/jobs/artifacts/<branch>/raw/veredictum-results/badges/<name>.svg?job=veredictum
```

In the README:

```markdown
[![openEHR BASE](https://gitlab.com/<namespace>/<project>/-/jobs/artifacts/main/raw/veredictum-results/badges/base.svg?job=veredictum)](https://gitlab.com/<namespace>/<project>/-/pipelines?ref=main)
```

Or under **Settings → General → Badges**, with `%{default_branch}` for the
branch. A failed pipeline leaves the URL on the last successful run; set
`allow-failure: true` to keep the badges on the newest run.

## Any other CI

Run `veredictum badge --results <results.json> --out <dir>` after every run,
passed or failed, and publish the directory.
