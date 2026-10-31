# The GitHub Action

<!-- toc -->

This repository is also a GitHub Action. One `uses:` step in a vendor's
workflow runs the instrument against their server, writes the results and the
CI views into a directory, puts the Markdown summary on the job's summary page,
annotates each row that needs attention, and fails the job when a row counts
against the server.

The action is versioned by the same `vX.Y.Z` tags as the binary and the
container image, and it lives in this repository so it cannot drift from the
release it runs. It needs Docker on the runner, which every GitHub-hosted Linux
runner has.

## A complete workflow

The job below starts the vendor's server as a service container, runs the
catalogue against it with the vendor's statement and IXIT, and keeps the three
CI views and the badges as a workflow artifact. Pin every `uses:` to a full
commit SHA, with
the tag in a trailing comment, so the workflow runs the bytes you reviewed.

```yaml
name: openEHR conformance
on: [pull_request]

permissions: {}

jobs:
  conformance:
    runs-on: ubuntu-latest
    permissions:
      contents: read # the checkout below, and nothing else
    services:
      cdr:
        image: registry.example/our-cdr:1.4.2
        ports: ["8080:8080"]
        options: >-
          --health-cmd "curl -fsS http://127.0.0.1:8080/health"
          --health-interval 5s --health-retries 30
    steps:
      - uses: actions/checkout@<commit-sha> # vN
        with:
          persist-credentials: false

      - id: veredictum
        uses: Vernum-Projecten/Veredictum@<commit-sha> # vX.Y.Z
        # Only for a CDR that wants bearer tokens; see "Credentials" below for
        # a CDR that wants a username and password, or no login.
        env:
          VEREDICTUM_CDR_TOKEN: ${{ secrets.CDR_TOKEN }}
          VEREDICTUM_CDR_ADMIN_TOKEN: ${{ secrets.CDR_ADMIN_TOKEN }}
        with:
          base-url: http://127.0.0.1:8080/openehr/v1
          statement: conformance/statement.json
          ixit: conformance/ixit.json
          auth: bearer
          admin-auth: bearer
          out-dir: conformance-results

      - uses: actions/upload-artifact@<commit-sha> # vN
        if: ${{ !cancelled() }}
        with:
          name: conformance-views
          path: |
            conformance-results/results.json
            conformance-results/junit.xml
            conformance-results/ctrf.json
            conformance-results/summary.md
            conformance-results/badges/
```

The step fails the job on a row that counts against the server. The upload
still runs, because `!cancelled()` holds for a failed job, so the views are
kept exactly when someone needs to read them.

`conformance-results/recording.har` is left out of the upload on purpose. It
holds the server's response bodies verbatim, and a CDR under test can hold real
patient data even in a test EHR. Keep it where you keep the run, never in an
artifact a public repository's readers can download.

The CTRF view is the input of reporters such as `ctrf-io/github-test-reporter`,
which can also comment on a pull request; that comment needs the
`pull-requests: write` permission, which the job above does not grant. The
JUnit view is read by most other CI systems.

## What the step does

1. It resolves the image, pulls it, and records the image ID it will run.
2. It runs `test` against `base-url`, with the statement, the IXIT and the
   credentials, writing `results.json` and `recording.har` into `out-dir`.
3. It runs `export` over that `results.json`, writing `junit.xml`, `ctrf.json`
   and `summary.md` beside it, appends `summary.md` to the job summary
   together with the image it ran, and [annotates](#annotations) the rows that
   need attention.
4. It runs `badge`, writing the [badges](#badges) into `badges/` in
   `out-dir`, and pushes them to `badge-branch` when that input is set and the
   run may push.
5. It exits with the run's status.

Every failed or errored row records whose problem it is, and the exit status
follows that record:

| Exit status | Meaning | The step |
|---|---|---|
| `0` | no row counts against the server and none is inconclusive: every row passed or was skipped, or failed only as a warning | passes |
| `1` | a row counts against the server: it failed (a warning row excepted), or errored with the server named, whatever else the run holds | fails |
| `2` | the run could not start or finish: a usage error, a catalogue gate finding, an unreadable file, a missing credential | fails |
| `3` | no row counts against the server, and at least one is inconclusive: the instrument could not judge it, so it does not count against the vendor | fails, or passes with `fail-on-inconclusive: false` |

With `fail-on-inconclusive: false`, a run that exits `3` passes the step, its
`exit-code` output still reads `3`, and the job summary lists each
inconclusive row with what the instrument received. A vendor whose required
check must not go red on rows the instrument cannot judge yet sets it;
`1` and `2` fail the step either way.

A run that could not start writes no results, so there is nothing to export;
the job summary says so, and the step log names the reason. The action
removes the files it writes from `out-dir` before the run, the badges among
them, so a results
document left by an earlier run is never exported as this one.

## Inputs

| Input | Default | Meaning |
|---|---|---|
| `base-url` | required | the server's base URL, up to and including the openEHR REST base |
| `statement` | none | path to the party's statement, which decides what runs ([`test`](commands.md#test)) |
| `ixit` | none | path to the party's IXIT, which answers the declared inputs |
| `auth` | `none` | `none`, `basic` or `bearer` |
| `admin-auth` | `none` | the same, for the administrative principal; with `none` the administrative workflows are skipped |
| `documents` | none | document ids separated by white space; a narrowed run is a working tool, never the record you submit |
| `profile` | `development` | the specification text the citations are read against: `development` (the pinned `master` of each openEHR repository) or `released` (the frozen release); only a `released` run backs a released claim or a certificate |
| `platform-url` | none | the SMART Platform base URL ([below](#smart-on-openehr)); without it every workflow sent to the Platform is skipped as `not-declared` |
| `out-dir` | `veredictum-results` | where the five files and the `badges/` directory are written |
| `image` | the release image | the image to run ([below](#which-image-runs)) |
| `pull` | `true` | `false` runs an image already on the runner, such as one built earlier in the job |
| `network` | `host` | the container network the run joins |
| `badge-branch` | none | a branch of your repository to push the badges to ([below](#badges)); with none, nothing is pushed |
| `fail-on-inconclusive` | `true` | `false` lets exit status `3`, a run whose only red rows are inconclusive, pass the step ([above](#what-the-step-does)) |
| `annotations` | `true` | `false` writes no [annotations](#annotations) |

With `network: host` the run shares the runner's network, so a service
container is reached at `127.0.0.1` on its published port, as above, and a
server elsewhere at its own address.

The step's outputs are `exit-code`, the paths `results`, `junit`, `ctrf`,
`summary` and `badges`, and `image-id` and `image-digests`, which name the image the run
used.

## Credentials

A credential is never an input. `auth` says how your CDR wants the client to
log in, and the credential is set as `env:` on the step, under the name the
engine reads:

| Your CDR wants | Input | `env:` on the step |
|---|---|---|
| no login | `auth: none` (the default) | nothing |
| a username and password | `auth: basic` | `VEREDICTUM_CDR_USER`, `VEREDICTUM_CDR_PASSWORD` |
| a bearer token | `auth: bearer` | `VEREDICTUM_CDR_TOKEN` |

`admin-auth` does the same for the administrative principal, with
`VEREDICTUM_CDR_ADMIN_USER` and `VEREDICTUM_CDR_ADMIN_PASSWORD` for `basic` and
`VEREDICTUM_CDR_ADMIN_TOKEN` for `bearer`. [Logging in to the
CDR](authentication.md) covers the same table for the command line.

A CDR that wants a username and password:

```yaml
      - uses: Vernum-Projecten/Veredictum@<commit-sha> # vX.Y.Z
        env:
          VEREDICTUM_CDR_USER: ${{ secrets.CDR_USER }}
          VEREDICTUM_CDR_PASSWORD: ${{ secrets.CDR_PASSWORD }}
        with:
          base-url: http://127.0.0.1:8080/openehr/v1
          statement: conformance/statement.json
          auth: basic
```

A CDR that needs no login takes no `env:` and no `auth`:

```yaml
      - uses: Vernum-Projecten/Veredictum@<commit-sha> # vX.Y.Z
        with:
          base-url: http://127.0.0.1:8080/openehr/v1
          statement: conformance/statement.json
```

Each may instead be given as a file: append `_FILE` to the name and set it to
the file's path, and the action mounts that file into the container read-only.
The action hands each variable to the container by name, so its value never
appears on a command line, and the engine records none of them.

An empty variable counts as unset. A secret that does not exist in your
repository expands to an empty string, and the run then stops with exit status
`2` on the missing credential instead of presenting an empty one to your
server.

## SMART on openEHR

A server that claims the `smart` folder runs its documents in the same step.
Two things are added. The `platform-url` input names the SMART Platform base
URL, which SMART on openEHR serves its discovery document relative to and
which can differ from `base-url`. The scoped tokens come from the IXIT's
`scoped_tokens` member ([`test`](commands.md#test) describes it), and every
secret that member names is set as `env:` on the step, like the CDR
credentials:

| The IXIT declares | `env:` on the step |
|---|---|
| a test issuer under `mint` | `VEREDICTUM_SCOPED_TOKEN_KEY`, the RSA private key in PEM |
| a token under `issued` | the `VEREDICTUM_SCOPED_TOKEN_<NAME>` variable it names |

```yaml
      - uses: Vernum-Projecten/Veredictum@<commit-sha> # vX.Y.Z
        env:
          VEREDICTUM_CDR_TOKEN: ${{ secrets.CDR_TOKEN }}
          VEREDICTUM_SCOPED_TOKEN_KEY: ${{ secrets.SMART_TEST_ISSUER_KEY }}
        with:
          base-url: http://127.0.0.1:8080/openehr/v1
          platform-url: http://127.0.0.1:8080
          statement: conformance/statement.json
          ixit: conformance/ixit.json
          auth: bearer
```

Here the statement claims `smart` and the IXIT's `scoped_tokens` declares a
`mint` issuer, so the instrument signs a token for each scope set a row
presents. Give it only key material a test deployment trusts, never one a
production deployment trusts. Each variable may instead be given as a file by
appending `_FILE`, and an empty one counts as unset, exactly as for the CDR
credentials. The action hands every variable named
`VEREDICTUM_SCOPED_TOKEN_<NAME>` to the container by name and no other.

## Which image runs

Left empty, `image` is `ghcr.io/vernum-projecten/veredictum:X.Y.Z`, where
`X.Y.Z` is the version the action's own tree carries. For the action at tag
`vX.Y.Z` that is the image the same release published, and the step refuses
an image whose engine reports a different version.

A tag can be moved; a digest cannot. The action at a tag cannot name the
digest of its own release, because the release builds the image after the tag
exists. To fix the bytes, pass the digest yourself:

```yaml
        with:
          image: ghcr.io/vernum-projecten/veredictum@sha256:<digest>
```

The release run's summary names the digest it published, and

```bash
docker buildx imagetools inspect ghcr.io/vernum-projecten/veredictum:X.Y.Z
```

prints it for any release. Whichever reference you pass, the step runs both
subcommands by the image ID it resolved, and writes that ID to the job summary
and to its `image-id` output, so the run names the image it judged with.

## Annotations

The step puts each row that needs attention on the run as a GitHub
annotation, which the pull request's checks and the workflow run's summary
page show without opening the job summary: an error for each failed or
errored row, and a warning for each warning row. Each is titled with the case
id and its row, and says what was checked, the cited sentence with its source
and section, what was expected and what was received:

```text
Error: ehr/create_ehr-same_ehr_twice-with_id row 1
Failed: counts against the server.
Checked: Create an EHR with the same ehr_id again.
Cited: "Can happen when the supplied `ehr_id` is already used by an existing EHR." (specs/openehr/ITS-REST/specifications/responses/409_EHR_with_id.yaml, description)
Expected: status 409
Received: status 201
```

An errored row says whether it counts against the server or is inconclusive.
GitHub shows at most 10 error and 10 warning annotations per step
([GitHub's limits](https://docs.github.com/en/rest/checks/runs)), so the step
writes the first 10 of each kind in row order and then one notice naming how
many more rows the job summary lists. No annotation points at a file: the
catalogue documents the rows come from ship in the image, not in your
repository. Set `annotations: false` to write none.

## Badges

The step draws five badges from every run into `badges/` in `out-dir`. With
`badge-branch: <branch>` it also pushes them to that branch of your
repository, but only from a run on your default branch that is not a pull
request, so your README can show them. [Badges](badges.md) covers what each
badge says, the colour rules, and the README snippet.

## Permissions

Without `badge-branch`, the action calls no GitHub API and reads no token. The
job needs `contents: read` for its own checkout and nothing more; set
`permissions: {}` at the workflow level and grant that one scope on the job,
as above.

With `badge-branch`, the step pushes the badges with the job's token, which
you set as `GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}` in the step's `env:`.
The job then needs `contents: write` instead.
