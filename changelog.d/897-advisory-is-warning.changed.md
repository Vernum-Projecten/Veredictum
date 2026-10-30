- **Breaking for readers of `results.json`, the certificate, CTRF and JUnit:**
  the outcome of a row whose SHOULD the server did not meet is now called
  `warning`, where it was `advisory`. A results row carries
  `"whose": "warning"`, the certificate summary counts it as `warning`, CTRF
  reports it as `other` with `rawStatus: warning`, and the JUnit text opens
  with `WARNING:`. `show`, `export --markdown`, the console and the book label
  it Warning. The meaning is unchanged: a warning row is never counted as
  failed, is listed apart from failed rows and keeps its own count. Ten case
  ids that ended in `_advisory` now end in `_warning`.
