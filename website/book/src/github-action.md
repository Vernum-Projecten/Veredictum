# The GitHub Action

<!-- toc -->

This repository is also a GitHub Action. One `uses:` step in a vendor's
workflow runs the instrument against their server, writes the results and the
CI views into a directory, puts the Markdown summary on the job's summary page,
and fails the job when a row that counts against the server failed or errored.

The action is versioned by the same `vX.Y.Z` tags as the binary and the
container image, and it lives in this repository so it cannot drift from the
release it runs. It needs Docker on the runner, which every GitHub-hosted Linux
runner has.

## A complete workflow

The job below starts the vendor's server as a service container, runs the
catalogue against it with the vendor's statement and IXIT, and keeps the three
CI views as a workflow artifact. Pin every `uses:` to a full commit SHA, with
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
        env:
          VEREDICTUM_SUT_TOKEN: ${{ secrets.CDR_TOKEN }}
          VEREDICTUM_ADMIN_TOKEN: ${{ secrets.CDR_ADMIN_TOKEN }}
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
```

The step fails the job on a failed or errored row. The upload still runs,
because `!cancelled()` holds for a failed job, so the views are kept exactly
when someone needs to read them.

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
   and `summary.md` beside it, and appends `summary.md` to the job summary
   together with the image it ran.
4. It exits with the run's status.

| Exit status | Meaning |
|---|---|
| `0` | every row passed or was skipped, or failed only as advisory |
| `1` | a row that counts against the server failed or errored |
| `2` | the run could not start or finish: a usage error, a catalogue gate finding, an unreadable file, a missing credential |

A run that could not start writes no results, so there is nothing to export;
the job summary says so, and the step log names the reason. The action
removes the five files it writes from `out-dir` before the run, so a results
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
| `out-dir` | `veredictum-results` | where the five files are written |
| `image` | the release image | the image to run ([below](#which-image-runs)) |
| `pull` | `true` | `false` runs an image already on the runner, such as one built earlier in the job |
| `network` | `host` | the container network the run joins |

With `network: host` the run shares the runner's network, so a service
container is reached at `127.0.0.1` on its published port, as above, and a
server elsewhere at its own address.

The step's outputs are `exit-code`, the paths `results`, `junit`, `ctrf` and
`summary`, and `image-id` and `image-digests`, which name the image the run
used.

## Credentials

A credential is never an input. Set it as `env:` on the step, under the name
the engine reads:

| Variable | Read when |
|---|---|
| `VEREDICTUM_SUT_USER`, `VEREDICTUM_SUT_PASSWORD` | `auth: basic` |
| `VEREDICTUM_SUT_TOKEN` | `auth: bearer` |
| `VEREDICTUM_ADMIN_USER`, `VEREDICTUM_ADMIN_PASSWORD` | `admin-auth: basic` |
| `VEREDICTUM_ADMIN_TOKEN` | `admin-auth: bearer` |

Each may instead be given as a file: append `_FILE` to the name and set it to
the file's path, and the action mounts that file into the container read-only.
The action hands each variable to the container by name, so its value never
appears on a command line, and the engine records none of them.

An empty variable counts as unset. A secret that does not exist in your
repository expands to an empty string, and the run then stops with exit status
`2` on the missing credential instead of presenting an empty one to your
server.

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

## Permissions

The action calls no GitHub API and reads no token. The job needs
`contents: read` for its own checkout and nothing more; set `permissions: {}`
at the workflow level and grant that one scope on the job, as above.
