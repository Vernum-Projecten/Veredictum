- A citation may carry `level: may` for a sentence that only permits, in the
  `arazzo-extensions` and `arazzo-capabilities` schemas. A `may` criterion
  counts against the server like a `must` one (RFC 2119 §5). `validate` now
  refuses a citation that leaves the default `must` while its quote says
  "should", "can" or "may" in lower case and nothing that requires, until the
  citation states its level; 78 catalogue citations now state one.
