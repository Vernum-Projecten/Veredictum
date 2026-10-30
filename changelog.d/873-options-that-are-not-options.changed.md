- The `aql-top` option is gone. The released AQL deprecates TOP but keeps it
  in the grammar, and no sentence lets a service refuse it, so `aqlc_top`,
  `aql_refused_top` and `aql_stored_top` now run for every claimant of
  `query`. A statement that still selects `aql-top-supported` or
  `aql-top-declined` names an arm that no longer exists.
- A service that declines a format the release recommends now gets one
  advisory row per declining arm. The unsupported arms of the four COMPOSITION
  and four CONTRIBUTION Simplified Formats options and of
  `template-web-template` each repeat one refused request and expect it
  served, citing "the **Simplified Formats** format SHOULD be supported". The
  406 and 415 refusals still gate. The `authentication-not-required` arm runs
  the new `security/authentication_not_required` document, which expects a
  401 on a request with an invalid credential. Each such row is failed and
  advisory, and never counts against the capability.
