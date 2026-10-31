# GitLab CI

<!-- toc -->

This repository ships a GitLab CI template, `ci/gitlab/veredictum.yml`. One
`include:` in a vendor's `.gitlab-ci.yml` runs the instrument against their
server, keeps the results, the CI views and the badges as job artifacts, shows
the rows in the merge request's test report, and fails the pipeline exactly
when [the GitHub Action](github-action.md) fails its step.

The template is versioned by the same `vX.Y.Z` tags as the binary and the
container image, and it runs the release image of the tag you include it
from. It needs a runner that can run Docker-in-Docker, which GitLab.com's
hosted Linux runners can.

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

Name a release tag, never a branch, so the pipeline runs the bytes you
reviewed. GitLab reads a remote include from any public URL and gives it the
typed inputs its header declares
([includes](https://docs.gitlab.com/ci/yaml/includes/),
[inputs](https://docs.gitlab.com/ci/inputs/)).

## The two jobs

The include adds two jobs to the `stage` you name.

1. **`<job-name>`** (default `veredictum`) pulls
   `ghcr.io/vernum-projecten/veredictum:X.Y.Z` and runs `test` against
   `base-url`, then `export` and `badge` over the result. It writes
   `results.json`, `junit.xml`, `ctrf.json`, `summary.md` and `badges/` into
   `out-dir`, with `recording.har` when `keep-recording` is `true`, and keeps
   them as artifacts, `junit.xml` also as the JUnit report the merge request
   shows. It saves the run's exit status beside them and always succeeds, so
   its artifacts belong to a pipeline GitLab can serve the badges from.
2. **`<job-name>-verdict`** reads that exit status and fails as the GitHub
   Action's step does:

| Exit status | Meaning | The verdict job |
|---|---|---|
| `0` | no row counts against the server and none is inconclusive | passes |
| `1` | a row counts against the server | fails |
| `2` | the run could not start or finish | fails |
| `3` | no row counts against the server, and at least one is inconclusive: the instrument could not judge it | fails, or passes with `fail-on-inconclusive: false` |

The image has no shell, so the run job drives it from the Docker CLI image
through a Docker-in-Docker service, both pinned by digest, and moves every
file in and out with `docker cp`. The run joins the Docker-in-Docker
service's network, so `base-url` must be reachable from there: a test
deployment of your server, such as a review environment, works. A server
started as another service of the same job is not something the template
sets up.

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

`recording.har` holds the server's response bodies verbatim, and a CDR under
test can hold real patient data even in a test EHR. It is written on every
run and left out of the artifacts unless you set `keep-recording`, because a
public project's artifacts can be downloaded by anyone.

## CI/CD variables

A credential is never an input. Set it as a masked CI/CD variable under the
name the engine reads, and the run job hands it to the container by name:

| Your CDR wants | Input | CI/CD variables |
|---|---|---|
| no login | `auth: none` (the default) | nothing |
| a username and password | `auth: basic` | `VEREDICTUM_CDR_USER`, `VEREDICTUM_CDR_PASSWORD` |
| a bearer token | `auth: bearer` | `VEREDICTUM_CDR_TOKEN` |

`admin-auth` reads `VEREDICTUM_CDR_ADMIN_USER` and
`VEREDICTUM_CDR_ADMIN_PASSWORD`, or `VEREDICTUM_CDR_ADMIN_TOKEN`. The SMART on
openEHR documents read the scoped tokens the IXIT's `scoped_tokens` member
names: `VEREDICTUM_SCOPED_TOKEN_KEY` for a `mint` test issuer, and each
`VEREDICTUM_SCOPED_TOKEN_<NAME>` an `issued` entry names.

A variable of the File type holds a path rather than the value. Give it the
name with `_FILE` appended, `VEREDICTUM_SCOPED_TOKEN_KEY_FILE` for example,
and the run job copies the file into the container. An empty variable counts
as unset, so a missing one stops the run with exit status `2` instead of
presenting an empty credential to your server.

## Badges

The run job draws the five [badges](badges.md) into `badges/` in `out-dir`.
GitLab serves a job artifact by URL from the latest successful pipeline of a
branch, and [Badges](badges.md#gitlab) gives the URL and the README snippet.
A verdict job that fails the pipeline leaves that URL on the last successful
pipeline's badges; `allow-failure: true` keeps it on the newest run.

## The template's own checks

`scripts/release/check-gitlab-template.sh` holds the template's image tag to
the engine version on every pull request and to the tag being released in the
release pipeline, and refuses an interpolation that names an undeclared input,
an input no job reads, or an input named like a credential. The test suite
validates the template, with its inputs substituted, against GitLab's own CI
configuration schema.
