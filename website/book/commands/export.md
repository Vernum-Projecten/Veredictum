`export` reads the `results.json` the Arazzo evaluator writes and renders the
three views a CI system shows. Each is a pure function of that document, so
exporting the same results twice writes the same bytes, and none of them
carries a clock reading the results document does not carry.

| Status | JUnit XML | CTRF 0.2.0 |
|---|---|---|
| passed | a bare `<testcase>` | `passed` |
| failed | `<failure>` | `failed` |
| advisory | `<system-out>`, no `<failure>` | `other`, with `rawStatus: advisory` |
| errored | `<error>` | `other`, with `rawStatus: errored` |
| skipped | `<skipped>` | `skipped` |

An advisory row is a SHOULD the server did not follow, which never counts
against the server, so no view counts it as failed and a CI that fails a job
on a failed test does not fail it on one. Its JUnit `<testcase>` carries the
four-part explanation in `<system-out>`, opened by a line that says it is
advisory, and no `failures` count includes it. In CTRF it is `other`, and
`summary.failed` excludes it.

A workflow is a JUnit `<testsuite>` and a row is a `<testcase>` named
`<case> row <n>`. The text of a `<failure>` or `<error>` is the row's
four-part explanation, its `message` is a one-line summary, and its `type`
says whose problem the row is (`server` or `instrument`). A
`<skipped>` carries the skip reason and the citation that makes the case not
apply. A suite whose case the results place carries `<properties>` naming the
part, chapter, band, capability, section or option and arm, formalism, TRIAL
mark and upstream reports, each property named `eu.veredictum.<name>`. Every
`time` is `0`, because the results document carries no timing.

JUnit XML has no single owner, so the view is held to the two schemas the tools
that read it publish: the document to the Jenkins xUnit plugin's `junit-10.xsd`,
which defines the `<testsuites>` root, and each `<testsuite>` to Apache Maven
Surefire's test report schema. Both are vendored, and a CI gate validates the
pinned views against them with `xmllint`.

In CTRF, every test has `type` `conformance`, a `suite` path of the document
then the part, chapter, band and section or option it is read under, and
`labels` naming where it sits. A red row's `message` is the same four-part
explanation. Everything CTRF does not define sits in `extra` under the one
namespaced key `eu.veredictum`, as CTRF §4.6.1 asks: the case id, the row, the
skip reason, the citation and, where the citation names one, the link to its
upstream report. The CTRF document validates against the schema the CTRF
project publishes for `specVersion` 0.2.0, its latest release.

**What the Markdown summary holds.** It is written for a person reading it on
GitHub, in two parts. A table names the server and the instrument, and a
summary table counts the rows of BASE and OPTIONS. `## BASE` follows chapter by
chapter, each chapter a table of its bands and each band a table of its
sections. `## OPTIONS` follows the same chapters and bands, each band a table
of its options with their kind (Recommended (SHOULD), Optional (MAY), or
`@SpecDiverge` for an option that exists only because the released text is
unclear) and the
arm the statement selected, and a collapsed note on what each option decides.
The cases resting on TRIAL text sit in their band under OPTIONS. Passed rows
are only counted. Each failed, advisory or errored row is a collapsed block
whose summary names the outcome, the case and where it sits; opening it shows
the step, what was checked, expected against received, the exchange, whose
problem it is and the row's inputs as a table, with the cited sentence quoted
below it. The advisory rows follow with their citations, and the skipped rows
counted by reason with the full list collapsed. A case resting on text an
upstream report records as unclear or contradictory is tagged `@SpecDiverge`
where it appears, and a closing note counts those cases and links the reports
they are judged by. JUnit carries the tag as the `eu.veredictum.tag` property
and CTRF in the test's standard `tags`.

**Where the Markdown summary goes.** GitHub Actions sets
`GITHUB_STEP_SUMMARY` in every job step, and `export` appends the summary to
that file, so the job's summary page shows it with no third-party action.
`--markdown <file>` writes it to a file as well. With neither, the summary is
printed to standard output and every other console line goes to standard
error, so the summary can be piped.

**A vendor's GitHub Actions job.** [The GitHub Action](github-action.md) runs
[`test`](#test) and then `export` in one step, with the image of the release it
belongs to:

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
appends the summary to the job summary, and fails the job when a row failed or
errored. Its page has a complete workflow.

The JUnit file is read by GitLab (`artifacts:reports:junit`), Jenkins (the
JUnit plugin) and most other CI systems. `conformance/recording.har` holds the
server's response bodies verbatim, so it belongs where the operator keeps the
run, never in a public artifact.
