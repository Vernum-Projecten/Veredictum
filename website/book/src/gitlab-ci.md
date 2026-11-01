# GitLab CI

<!-- toc -->

`ci/gitlab/veredictum.yml` is a GitLab CI template, versioned by the same
`vX.Y.Z` tags as the binary and image. It needs a runner that can run
Docker-in-Docker, as GitLab.com's hosted Linux runners can.

## Including the template

```yaml
include:
  - remote: https://raw.githubusercontent.com/Vernum-Projecten/Veredictum/vX.Y.Z/ci/gitlab/veredictum.yml
    inputs:
      base-url: https://cdr-test.example/openehr/v1
      statement: conformance/statement.json
      ixit: conformance/ixit.json
      auth: bearer
      profile: released
```

Name a release tag, never a branch
([includes](https://docs.gitlab.com/ci/yaml/includes/),
[inputs](https://docs.gitlab.com/ci/inputs/)).

## The two jobs

1. **`<job-name>`** (default `veredictum`) runs `test`, `export` and `badge`
   with `ghcr.io/vernum-projecten/veredictum:X.Y.Z`, keeping `results.json`,
   `junit.xml` (also the merge request's test report), `ctrf.json`,
   `summary.md` and `badges/` as artifacts. It always succeeds, so its badges
   stay servable.
2. **`<job-name>-verdict`** fails on the saved exit status:

| Exit status | Meaning | The verdict job |
|---|---|---|
| `0` | no row counts against the server and none is inconclusive | passes |
| `1` | a row counts against the server | fails |
| `2` | the run could not start or finish | fails |
| `3` | no row counts against the server, and at least one is inconclusive: the instrument could not judge it | fails, or passes with `fail-on-inconclusive: false` |

The run uses a digest-pinned Docker CLI image and Docker-in-Docker service.
The instrument image has no shell, so files move in and out with `docker cp`.
`base-url` must be reachable from that service's network (a review
environment works; a server started as another service of the job does not).

## Inputs

| Input | Default | Meaning |
|---|---|---|
| `base-url` | required | the server's base URL, up to and including the openEHR REST base |
| `statement` | none | path to the party's statement in the repository, which decides what runs ([`test`](commands.md#test)) |
| `ixit` | none | path to the party's IXIT in the repository, which answers the declared inputs |
| `auth` | `none` | `none`, `basic` or `bearer` |
| `admin-auth` | `none` | the same, for the administrative principal; with `none` the administrative workflows are skipped |
| `documents` | none | document ids separated by spaces; a narrowed run is a working tool, never the record you submit |
| `profile` | `development` | `development` (the pinned `master` of each openEHR repository) or `released` (the frozen release); only a `released` run backs a released claim or a certificate |
| `platform-url` | none | the SMART Platform base URL; without it every workflow sent to the Platform is skipped as `not-declared` |
| `out-dir` | `veredictum-results` | where the files and `badges/` are written, relative to the project directory |
| `keep-recording` | `false` | `true` keeps `recording.har` as an artifact |
| `fail-on-inconclusive` | `true` | `false` lets exit status `3` pass the verdict job |
| `allow-failure` | `false` | `true` leaves the pipeline passed with warnings when the verdict job fails, so the [badges](#badges) keep showing the newest run |
| `stage` | `test` | the stage both jobs run in |
| `job-name` | `veredictum` | the run job's name; the verdict job is this name with `-verdict` appended |

Leave `keep-recording` off for a public project: the recording holds response
bodies verbatim.

## CI/CD variables

Credentials are masked CI/CD variables, never inputs:

| Your CDR wants | Input | CI/CD variables |
|---|---|---|
| no login | `auth: none` (the default) | nothing |
| a username and password | `auth: basic` | `VEREDICTUM_CDR_USER`, `VEREDICTUM_CDR_PASSWORD` |
| a bearer token | `auth: bearer` | `VEREDICTUM_CDR_TOKEN` |

`admin-auth` reads `VEREDICTUM_CDR_ADMIN_USER` and
`VEREDICTUM_CDR_ADMIN_PASSWORD`, or `VEREDICTUM_CDR_ADMIN_TOKEN`. SMART reads
`VEREDICTUM_SCOPED_TOKEN_KEY` for a `mint` issuer and each
`VEREDICTUM_SCOPED_TOKEN_<NAME>` an `issued` entry names. For a File-type
variable append `_FILE` (for example `VEREDICTUM_SCOPED_TOKEN_KEY_FILE`). An
empty variable stops the run with exit `2`.

## Badges

See [Badges](badges.md#gitlab) for the artifact URL and README snippet.

## The template's own checks

`scripts/release/check-gitlab-template.sh` pins the template's image tag to
the engine version and refuses undeclared, unread or credential-named inputs.
The test suite validates the template against GitLab's CI schema.
