# Badges

<!-- toc -->

A vendor can show the result of its own conformance run in a README with
static badges. [`veredictum badge`](commands.md#badge) draws them from the
result document a run wrote, and the vendor publishes them next to its own
code. No badge service is involved, and the hosted instrument publishes
nothing.

Every badge is the vendor's own run, and says so: the `instrument` badge reads
`self-run`. A badge is never a certificate, and none says certified. No
certificate is issued before the openEHR SEC approves the instrument. Link each
badge to the run it was drawn from, because that run's recorded results are
the evidence behind it.

## What they look like

Each image below is `veredictum badge` run over a fixture result, the same
documents the instrument's badge tests pin, not a real server. Under each group
is the Markdown that drew it on this page. The fixtures name instrument version
0.3.1, so the `instrument` badge reads `v0.3.1`; your own run names the version
that drew it.

A passing result judged against the release, with one BASE warning:

![openEHR BASE · released: 3/3 passed · 1 warning](badges/passing/base.svg)
![openEHR options · released: 1/1 passed](badges/passing/options.svg)
![specification: released](badges/passing/profile.svg)
![veredictum: self-run · v0.3.1](badges/passing/instrument.svg)

```markdown
![openEHR BASE · released: 3/3 passed · 1 warning](badges/passing/base.svg)
![openEHR options · released: 1/1 passed](badges/passing/options.svg)
![specification: released](badges/passing/profile.svg)
![veredictum: self-run · v0.3.1](badges/passing/instrument.svg)
```

A failing result: one BASE row failed against the server, and one BASE row
and one option row errored without a verdict:

![openEHR BASE · released: 2/4 passed · 1 failed · 1 errored](badges/failing/base.svg)
![openEHR options · released: 0/1 passed · 1 errored](badges/failing/options.svg)
![specification: released](badges/failing/profile.svg)
![veredictum: self-run · v0.3.1](badges/failing/instrument.svg)

```markdown
![openEHR BASE · released: 2/4 passed · 1 failed · 1 errored](badges/failing/base.svg)
![openEHR options · released: 0/1 passed · 1 errored](badges/failing/options.svg)
![specification: released](badges/failing/profile.svg)
![veredictum: self-run · v0.3.1](badges/failing/instrument.svg)
```

The passing result judged against the development snapshot:

![openEHR BASE · development: 3/3 passed · 1 warning](badges/development/base.svg)
![openEHR options · development: 1/1 passed](badges/development/options.svg)
![specification: development (master)](badges/development/profile.svg)
![veredictum: self-run · v0.3.1](badges/development/instrument.svg)

```markdown
![openEHR BASE · development: 3/3 passed · 1 warning](badges/development/base.svg)
![openEHR options · development: 1/1 passed](badges/development/options.svg)
![specification: development (master)](badges/development/profile.svg)
![veredictum: self-run · v0.3.1](badges/development/instrument.svg)
```

A performance record whose knee is a bracket, and one whose every plateau
qualified:

![openEHR perf · custom: knee 180.8–210.9/s, self-run](badges/perf/bracket.svg)

![openEHR perf · ehr-mix@2: knee ≥ 1,250.0/s, self-run](badges/perf/lower-bound.svg)

```markdown
![openEHR perf · custom: knee 180.8–210.9/s, self-run](badges/perf/bracket.svg)
![openEHR perf · ehr-mix@2: knee ≥ 1,250.0/s, self-run](badges/perf/lower-bound.svg)
```

In a README each badge is the same Markdown image, pointed at wherever you
publish it ([GitHub](#github), [GitLab](#gitlab)) and linked to the run that
drew it. The first badge above becomes:

```markdown
[![openEHR BASE · released: 3/3 passed · 1 warning](https://raw.githubusercontent.com/<owner>/<repo>/veredictum-badges/base.svg)](https://github.com/<owner>/<repo>/actions/workflows/conformance.yml?query=branch%3Amain)
```

The gallery is redrawn by `scripts/render/book-badges.sh`, and CI fails when
the committed images differ from what the current binary draws
(`scripts/checks/book-badges.sh`). [Reading a result](reading-results.md)
explains what the rows behind these numbers mean for a buyer.

## The four conformance badges

A conformance result (`results.json`) draws four badges, one fact each, so a
README can show the ones it needs.

| Badge | Label | Message |
|---|---|---|
| `base` | `openEHR BASE · <profile>` | passed out of the BASE rows and the rows the outline places nowhere that bear on the verdict (passed, failed and errored), then the failed, errored and warning counts when there are any: `312/318 passed · 2 failed · 1 warning` |
| `options` | `openEHR options · <profile>` | the same count over the option arms the statement selected and the cases resting on TRIAL text, which are read with the options |
| `profile` | `specification` | `released`, or `development (master)` with the branch the snapshot was taken from |
| `instrument` | `veredictum` | `self-run · v<version>` |

A warning row is a SHOULD the server did not follow. It is neither a pass nor
a failure, so it stays out of the ratio and is named after it, on the badge of
the BASE or option row it belongs to. A skipped row is not in the ratio either.

The counts are the ones [`show`](commands.md#show) and the Markdown summary
print, read from the same outline. A badge can be embedded alone, so every
counting badge names its profile in its own label: a tally judged against the
development snapshot never reads as a released one.

The colours follow one rule: a badge is green only when nothing it counts
failed or errored. A warning never changes a colour.

| Colour | `base` and `options` | `profile` |
|---|---|---|
| green | rows passed, none failed or errored | |
| red | at least one row counts against the server: it failed, or errored with the server (or no one) named in its record | |
| orange | none counts against the server, and at least one errored row is inconclusive (the instrument could not judge it) | |
| grey | nothing passed: no rows, or every row skipped | |
| blue | | `released` |
| purple | | `development` |

The `instrument` badge is the brand's deep teal. Every mark is in the text as
well as the colour, so a failed or errored row reads the same in greyscale.

## The performance badge

A `perf-result.json` draws one badge, `perf`, and nothing else.

| Label | Message |
|---|---|
| `openEHR perf · <workload>` | the knee as the record states it, then `self-run` and how many passes measured it |

The workload is the record's label: `<id>@<version>` for a workload the
instrument pins, `custom` for any other document. The knee reads
`knee 180.8–210.9/s` for a bracket, `knee ≥ 1,250.0/s` when every plateau
qualified, `below ladder` when the lowest plateau failed, and `no plateau` when
the run had none. The message closes with `1 pass, noise unmeasured`, or
`1 pass, A/A floor attached` when the record carries an A/A floor. A knee is a
reading of capacity, never a pass or a failure, so the badge is always slate
grey-blue.

## The icon

Each badge carries the Veredictum mark, the seal ring with the V check, in
white at the left of the label. The `.json` file carries the same mark as its
`logoSvg`, so a badge Shields renders shows it too. The openEHR logo is never
used: it is openEHR's trademark, and a vendor's own run is not an endorsement
by openEHR.

## Drawing the badges

```bash
veredictum badge --results conformance/results.json --out conformance/badges
```

writes `base.svg`, `base.json` and the same pair for each other badge. The SVG
is complete in itself and needs no font download. The `.json` file is a
[Shields.io endpoint](https://shields.io/badges/endpoint-badge) document, so a
README can also point Shields at it:
`https://img.shields.io/endpoint?url=<the published URL of base.json>`.

## GitHub

[The GitHub Action](github-action.md) draws the badges after every run into
`badges/` in its `out-dir`. With the `badge-branch` input it also pushes them to
that branch of your repository, as a single commit that holds only the badges
and replaces the branch's content each time. The push happens only on a run on
your default branch that is not a pull request, so a pull request or a feature
branch never changes the badges your README shows. When the badges did not
change, nothing is pushed.

The push uses the job's `GITHUB_TOKEN`, which you set as `env:` on the step,
and the job then needs `contents: write`:

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

In the README, each badge is the raw file on that branch, linking to the
workflow's runs on the default branch:

```markdown
[![openEHR BASE](https://raw.githubusercontent.com/<owner>/<repo>/veredictum-badges/base.svg)](https://github.com/<owner>/<repo>/actions/workflows/conformance.yml?query=branch%3Amain)
[![openEHR options](https://raw.githubusercontent.com/<owner>/<repo>/veredictum-badges/options.svg)](https://github.com/<owner>/<repo>/actions/workflows/conformance.yml?query=branch%3Amain)
[![specification](https://raw.githubusercontent.com/<owner>/<repo>/veredictum-badges/profile.svg)](https://github.com/<owner>/<repo>/actions/workflows/conformance.yml?query=branch%3Amain)
[![veredictum](https://raw.githubusercontent.com/<owner>/<repo>/veredictum-badges/instrument.svg)](https://github.com/<owner>/<repo>/actions/workflows/conformance.yml?query=branch%3Amain)
```

The newest run in that list drew the badges. GitHub serves raw files with a
cache lifetime of five minutes, so a README can show the previous badges for
that long after a push.

## GitLab

[The GitLab CI template](gitlab-ci.md) draws the badges after every run into
`badges/` in its `out-dir` and keeps them as job artifacts. GitLab serves an
artifact by URL from the latest successful pipeline of a branch, so the badge
URL of a public project is

```text
https://gitlab.com/<namespace>/<project>/-/jobs/artifacts/<branch>/raw/veredictum-results/badges/<name>.svg?job=veredictum
```

with the template's default `out-dir` and `job-name`. Use it in the README:

```markdown
[![openEHR BASE](https://gitlab.com/<namespace>/<project>/-/jobs/artifacts/main/raw/veredictum-results/badges/base.svg?job=veredictum)](https://gitlab.com/<namespace>/<project>/-/pipelines?ref=main)
```

or under **Settings → General → Badges**, with the same URL as the badge image
and the project's pipelines as its link, where `%{default_branch}` can stand
for the branch.

The template's verdict job fails the pipeline exactly as the GitHub Action
fails its step, and a failed pipeline leaves that URL on the badges of the
last successful one. To keep the badges on the newest run, set the
template's `allow-failure` input to `true`: a run with a failed row then
leaves the pipeline passed with warnings, and the badges show the failure.

## Any other CI

The command alone is enough. Run `veredictum badge --results <results.json>
--out <dir>` after the run, from the release binary or the image, and publish
the directory wherever your README can read it. Draw the badges whether the run
passed or failed, and make sure a failed run replaces the previous badges.
