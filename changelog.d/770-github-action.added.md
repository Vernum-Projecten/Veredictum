- A GitHub Action at the repository root runs the instrument in a vendor's CI
  with one `uses:` step. It runs the release image of its own version, or an
  image the caller names by digest, drives `test` against the server with the
  caller's statement and IXIT, writes `results.json`, `junit.xml`, `ctrf.json`
  and `summary.md` into its output directory, and appends the summary to the
  job summary. The step exits with the run's status: `1` when a row that
  counts against the server failed or errored, `2` when the run could not
  start. Credentials are read from the step's environment under the names the
  engine reads, or from files through their `_FILE` forms, and an empty
  variable counts as unset. The book has a page for it with a complete
  workflow, and the `export` page's example uses it.
