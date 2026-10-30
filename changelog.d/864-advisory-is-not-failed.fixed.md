- An advisory row, a SHOULD the server did not follow, no longer counts as
  failed in any view of a result. The Markdown summary shows an Advisory count
  and lists those rows in their own table with their citations. In JUnit an
  advisory row is a passing `testcase` with its explanation in `<system-out>`,
  and no `failures` count includes it; in CTRF it is `other` with
  `rawStatus: advisory`, and `summary.failed` excludes it. A CI that fails a
  job on a failed test no longer fails it on a SHOULD. The line `test` prints
  after a run counts advisory rows apart as well.
