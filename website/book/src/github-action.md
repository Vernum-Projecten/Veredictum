# The GitHub Action

<!-- toc -->

This repository is a GitHub Action, versioned by the same `vX.Y.Z` tags as the
binary and image. It needs Docker on the runner (every GitHub-hosted Linux
runner has it).

## A complete workflow

Pin every `uses:` to a full commit SHA with the tag in a trailing comment.

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

Leave `recording.har` out of uploads: it holds response bodies verbatim, which
can include patient data. The CTRF view feeds reporters such as
`ctrf-io/github-test-reporter` (a pull request comment needs
`pull-requests: write`).

## What the step does

1. Pulls the image and records its ID.
2. Runs `test`, writing `results.json` and `recording.har` into `out-dir`.
3. Runs `export` (`junit.xml`, `ctrf.json`, `summary.md`), appends the summary
   and image to the job summary, and writes [annotations](#annotations).
4. Runs `badge` into `out-dir/badges/` and pushes to `badge-branch` if set
   ([badges](#badges)).
5. Exits with the run's status.

| Exit status | Meaning | The step |
|---|---|---|
| `0` | no row counts against the server and none is inconclusive: every row passed or was skipped, or failed only as a warning | passes |
| `1` | a row counts against the server: it failed (a warning row excepted), or errored with the server named, whatever else the run holds | fails |
| `2` | the run could not start or finish: a usage error, a catalogue gate finding, an unreadable file, a missing credential | fails |
| `3` | no row counts against the server, and at least one is inconclusive: the instrument could not judge it, so it does not count against the vendor | fails, or passes with `fail-on-inconclusive: false` |

With exit `3` and `fail-on-inconclusive: false`, `exit-code` still reads `3`
and the job summary lists each inconclusive row. The action clears its own
files from `out-dir` first, so stale results are never exported.

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

With `network: host`, a service container is at `127.0.0.1` on its published
port.

Outputs: `exit-code`, `results`, `junit`, `ctrf`, `summary`, `badges`,
`image-id` and `image-digests`.

## Credentials

Never an input; set them as `env:` on the step.

| Your CDR wants | Input | `env:` on the step |
|---|---|---|
| no login | `auth: none` (the default) | nothing |
| a username and password | `auth: basic` | `VEREDICTUM_CDR_USER`, `VEREDICTUM_CDR_PASSWORD` |
| a bearer token | `auth: bearer` | `VEREDICTUM_CDR_TOKEN` |

`admin-auth` uses `VEREDICTUM_CDR_ADMIN_USER` and
`VEREDICTUM_CDR_ADMIN_PASSWORD`, or `VEREDICTUM_CDR_ADMIN_TOKEN`.
A `_FILE` variable is mounted read-only into the container; an empty variable
counts as unset and stops the run with exit `2`
([Logging in to the CDR](authentication.md)).

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

With no login, omit `env:` and `auth`:

```yaml
      - uses: Vernum-Projecten/Veredictum@<commit-sha> # vX.Y.Z
        with:
          base-url: http://127.0.0.1:8080/openehr/v1
          statement: conformance/statement.json
```

## SMART on openEHR

A server claiming `smart` adds `platform-url` (the SMART Platform base URL,
which can differ from `base-url`) and the IXIT's `scoped_tokens`
([`test`](commands.md#test)):

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

A `mint` issuer signs a token per scope set; use only a key a test deployment
trusts. Only `VEREDICTUM_SCOPED_TOKEN_<NAME>` variables reach the container.
Working example: `deploy/docker/e2e-ferroehr-smart.yml` and
`deploy/docker/smart/`.

## Which image runs

By default `ghcr.io/vernum-projecten/veredictum:X.Y.Z`, the action's own
version; an image reporting another version is refused. To pin bytes, pass a
digest:

```yaml
        with:
          image: ghcr.io/vernum-projecten/veredictum@sha256:<digest>
```

```bash
docker buildx imagetools inspect ghcr.io/vernum-projecten/veredictum:X.Y.Z
```

The step runs by resolved image ID and reports it as `image-id`.

## Annotations

An error per failed or errored row, a warning per warning row:

```text
Error: ehr/create_ehr-same_ehr_twice-with_id row 1
Failed: counts against the server.
Checked: Create an EHR with the same ehr_id again.
Cited: "Can happen when the supplied `ehr_id` is already used by an existing EHR." (specs/openehr/ITS-REST/specifications/responses/409_EHR_with_id.yaml, description)
Expected: status 409
Received: status 201
```

GitHub shows at most 10 of each kind per step
([limits](https://docs.github.com/en/rest/checks/runs)); the step writes the
first 10 in row order, then one notice counting the rest.

## Badges

Four badges per run go to `out-dir/badges/`. With `badge-branch: <branch>`
they are pushed there, only from a non-pull-request run on the default branch.
See [Badges](badges.md#github).

## Permissions

Without `badge-branch`: no GitHub API and no token; the job needs only
`contents: read`. With it: set `GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}` in
the step's `env:` and grant `contents: write`.
