`export` reads the `results.json` the Arazzo evaluator writes and renders the
three views a CI system shows. Each is a pure function of that document, so
exporting the same results twice writes the same bytes, and none of them
carries a clock reading the results document does not carry.

| Status | JUnit XML | CTRF 0.2.0 |
|---|---|---|
| passed | a bare `<testcase>` | `passed` |
| failed | `<failure>` | `failed` |
| errored | `<error>` | `other`, with `rawStatus: errored` |
| skipped | `<skipped>` | `skipped` |

A workflow is a JUnit `<testsuite>` and a row is a `<testcase>` named
`<case> row <n>`. The text of a `<failure>` or `<error>` is the row's
four-part explanation, its `message` is a one-line summary, and its `type`
says whose problem the row is (`server`, `advisory` or `instrument`). A
`<skipped>` carries the skip reason and the citation that makes the case not
apply. In CTRF, a red row's `message` is the same four-part explanation, and
each test's `extra` carries the case id, the row, the skip reason, the
citation and, where the citation names one, the link to its upstream report.
The CTRF document validates against the schema the CTRF project publishes for
`specVersion` 0.2.0.

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
          VEREDICTUM_SUT_USER: ${{ secrets.CDR_USER }}
          VEREDICTUM_SUT_PASSWORD: ${{ secrets.CDR_PASSWORD }}
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
