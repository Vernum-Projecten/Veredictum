`export` renders the Arazzo `results.json` as the views a CI system shows.
Each is a pure function of that document, so the same results write the same
bytes, with no clock reading the results do not carry.

| Status | JUnit XML | CTRF 0.2.0 |
|---|---|---|
| passed | a bare `<testcase>` | `passed` |
| failed | `<failure>` | `failed` |
| warning | `<system-out>`, no `<failure>` | `other`, with `rawStatus: warning` |
| errored | `<error>` | `other`, with `rawStatus: errored` |
| skipped | `<skipped>` | `skipped` |

A warning is a SHOULD the server did not follow and never counts against it,
so no view counts it as failed and a CI job does not fail on it. In JUnit it
is the four-part explanation in `<system-out>`, opened by a line naming it a
warning, outside every `failures` count. In CTRF it is `other`, outside
`summary.failed`.

**JUnit XML.** A workflow is a `<testsuite>`; a row is a `<testcase>` named
`<case> row <n>`.

- **`<failure>` and `<error>`:** text is the four-part explanation, `message`
  a one-line summary, `type` whose problem it is (`server` or `instrument`).
- **`<skipped>`:** the skip reason and the citation that makes the case not
  apply.
- **`<properties>`:** on a suite the results place, named
  `eu.veredictum.<name>`: part, chapter, band, capability, section or option
  and arm, formalism, TRIAL mark and upstream reports.
- **`time`:** always `0`, since the results carry no timing.

JUnit XML has no single owner, so the view is held to two published schemas:
the document to the Jenkins xUnit plugin's `junit-10.xsd` (which defines the
`<testsuites>` root), each `<testsuite>` to Apache Maven Surefire's test
report schema. Both are vendored, and a CI gate validates the pinned views
against them with `xmllint`.

**CTRF.** Every test has `type` `conformance`, a `suite` path (the document,
then part, chapter, band and section or option) and `labels` naming where it
sits. A red row's `message` is the four-part explanation. Everything CTRF does
not define sits in `extra` under the key `eu.veredictum`, as CTRF §4.6.1
asks: case id, row, skip reason, citation and, where the citation names one,
the upstream report link. The document validates against the CTRF project's
schema for `specVersion` 0.2.0, its latest release.

**The Markdown summary**, for a person reading it on GitHub:

- A table naming the server and the instrument, and a summary table counting
  BASE and OPTIONS rows.
- `## BASE`, chapter by chapter: each chapter a table of its bands, each band
  a table of its sections.
- `## OPTIONS`, by the same chapters and bands: each band a table of its
  options with their kind (Recommended (SHOULD), Optional (MAY), or
  `@SpecDiverge` for an option that exists only because the released text is
  unclear), the selected arm, and a collapsed note on what the option decides.
  Cases resting on TRIAL text sit in their band here.
- Passed rows are only counted. Each failed, warning or errored row is a
  collapsed block naming outcome, case and place; open, it shows the step, what
  was checked, expected against received, the exchange, whose problem it is,
  the row's inputs as a table, and the cited sentence.
- Warning rows with their citations, then skipped rows counted by reason, with
  the full list collapsed.
- A case resting on text an upstream report records as unclear or
  contradictory is tagged `@SpecDiverge`, and a closing note counts those cases
  and links their reports. JUnit carries the tag as the `eu.veredictum.tag`
  property, CTRF in the test's standard `tags`.

**Where the summary goes.** `export` appends it to `GITHUB_STEP_SUMMARY` when
GitHub Actions sets it, so the job summary shows it with no third-party
action. `--markdown <file>` writes it to a file too. With neither, it goes to
standard output and every other line to standard error, so it can be piped.

**A vendor's GitHub Actions job.** [The GitHub Action](github-action.md) runs
[`test`](#test) and then `export` in one step, with its release's image:

```yaml
      - uses: Vernum-Projecten/Veredictum@<commit-sha> # vX.Y.Z
        env:
          VEREDICTUM_CDR_USER: ${{ secrets.CDR_USER }}
          VEREDICTUM_CDR_PASSWORD: ${{ secrets.CDR_PASSWORD }}
        with:
          base-url: ${{ vars.CDR_BASE_URL }}
          statement: conformance/statement.json
          auth: basic
          out-dir: conformance
```

The step writes `junit.xml`, `ctrf.json` and `summary.md` into `conformance/`,
appends the summary to the job summary, annotates the rows that need
attention, and fails the job when a row counts against the server. Its page
has a complete workflow, and [GitLab CI](gitlab-ci.md) has a template.

`--annotations <file>` writes those annotations as workflow commands: one
`::error` per failed or errored row and one `::warning` per warning row, at
most 10 of each, then a `::notice` counting the rows the summary lists beyond
them.

GitLab (`artifacts:reports:junit`), Jenkins (the JUnit plugin) and most other
CI systems read the JUnit file. `conformance/recording.har` holds response
bodies verbatim, so it stays with the operator, never in a public artifact.

A `perf-result.json` has one view, the Markdown summary, holding what `show`
prints as tables. `--junit`, `--ctrf` and `--annotations` refuse it with exit
status 2, since a performance record has no test cases.
