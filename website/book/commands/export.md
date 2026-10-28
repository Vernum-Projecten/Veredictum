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

**A vendor's GitHub Actions job.** The workflow below drives the EHR workflows
against the vendor's CDR through the repository's `arazzo_spike` example,
which is how the Arazzo evaluator is run, then exports the results. The
Markdown summary lands on the job's summary page, and the CTRF report is
handed to `ctrf-io/github-test-reporter`, which can also comment on a pull
request. In a real workflow, pin every `uses:` to a full commit SHA.

```yaml
name: openEHR conformance
on: [pull_request]

jobs:
  conformance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          repository: Vernum-Projecten/Veredictum
          path: veredictum
      - name: Drive the EHR workflows against the CDR
        working-directory: veredictum
        env:
          CDR_BASE_URL: ${{ vars.CDR_BASE_URL }}
          CDR_USER: ${{ secrets.CDR_USER }}
          CDR_PASSWORD: ${{ secrets.CDR_PASSWORD }}
        run: |
          cargo run --locked --example arazzo_spike -- \
            --base-url "$CDR_BASE_URL" --user "$CDR_USER" --password "$CDR_PASSWORD" \
            --document catalogue/cases/ehr/ehr.arazzo.yaml --out ../conformance
      - name: Export the results to the job summary, JUnit and CTRF
        if: ${{ !cancelled() }}
        working-directory: veredictum
        run: |
          cargo run --locked -- export --results ../conformance/results.json \
            --junit ../conformance/junit.xml --ctrf ../conformance/ctrf.json
      - uses: ctrf-io/github-test-reporter@v1
        if: ${{ !cancelled() }}
        with:
          report-path: conformance/ctrf.json
```

The JUnit file is read by GitLab (`artifacts:reports:junit`), Jenkins (the
JUnit plugin) and most other CI systems. `conformance/recording.har` holds the
server's response bodies verbatim, so it belongs where the operator keeps the
run, never in a public artifact.
