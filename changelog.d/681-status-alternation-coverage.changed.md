- The `arazzo-surface` gate counts a `regex` criterion on `$statusCode` whose
  pattern is a plain alternation of status codes, such as `^(400|409)$`, as
  covering each declared response it names. Any other pattern on
  `$statusCode` still covers nothing. The surface gap for the 400 of a
  COMPOSITION delete is closed.
