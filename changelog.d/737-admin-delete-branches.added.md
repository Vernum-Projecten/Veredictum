- An option in `capabilities.yaml` can carry `decides`, the response statuses
  of an operation that its arms divide as alternative answers, each with the
  released sentence that makes them alternatives. The surface gate counts a
  listed status as covered when one arm asserts it and every arm calls the
  operation. Every other element still needs every arm, and `validate`
  refuses a listed status that no arm asserts or whose operation an arm does
  not call. This covers the Admin API delete branches `202` and `204` and the
  bulk delete's `405`.
- A rows file can carry `server: exclusive`: its workflow runs only when the
  party's IXIT declares `environment.exclusive_server: true`, and is skipped
  as `not-declared` otherwise (#738).
- The unfiltered Admin API bulk delete (`DELETE /admin/ehr/all` with no
  `ehr_id`) has a synchronous and an asynchronous workflow under that
  declaration (#738). The bulk delete's `404` stays a surface gap under
  upstream report #735, because no released sentence names the request it
  answers.
