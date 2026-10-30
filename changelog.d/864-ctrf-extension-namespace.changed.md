- **Breaking for CTRF readers:** every field Veredictum adds to a CTRF
  document now sits in `extra` under the one namespaced key `eu.veredictum`,
  as CTRF 0.2.0 §4.6.1 asks. A reader of `extra.case`, `extra.row`,
  `extra.skipReason`, `extra.citation`, `extra.report` or `extra.whose` on a
  test, `extra.instrumentStatus` on the tool or `extra.baseUrl` on the
  environment reads them under `extra["eu.veredictum"]` instead. Each test
  also carries its place in the result in the standard `suite` path and
  `labels`, and its `type` is `conformance`.
