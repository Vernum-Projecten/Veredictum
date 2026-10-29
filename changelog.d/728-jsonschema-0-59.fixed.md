- Schema checks run on `jsonschema` 0.59.0 (#728). A pattern that hit the
  regular-expression backtrack limit counted as a non-match before, so a
  `not` or `patternProperties` keyword could accept an instance it should
  refuse. A response judged against the released OpenAPI schemas, or an
  artifact checked against the published schemas, now fails in that case.
  The fuzz harnesses move to the same version.
