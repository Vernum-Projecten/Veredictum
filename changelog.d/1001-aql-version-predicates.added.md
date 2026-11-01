- Development-only cases for the AQL version predicates: a query with no
  `VERSION` class expression, `VERSION v` and `VERSION v[LATEST_VERSION]` each
  read the latest version only, `VERSION v[ALL_VERSIONS]` reads every version,
  and a version predicate on a `COMPOSITION` class expression is refused with
  400. A released run skips them as `development-only`.
