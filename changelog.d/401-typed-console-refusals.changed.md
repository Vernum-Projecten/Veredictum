- Every console server function answers a refusal as one typed error with its
  own HTTP status: a spent probe or start budget is 429, a refused target, an
  over-long value, a refused statement or an unusable argument is 422, a
  missing connection draft or a run already in flight is 409, and only the
  instance's own failures stay 500. The body is the encoded error, so the
  screens name what was refused and offer the next step that refusal calls
  for, worded in one place; an unreadable call answers 400 or 413 in the same
  encoding. The `veredictum_server_fn_responses_total` 5xx class now counts
  only the instance's own failures.
