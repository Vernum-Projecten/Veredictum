- The surface gate counts a response status as covered when one arm of a
  capability option asserts it and every arm of that option calls its
  operation, since the arms are the service's alternative answers to that
  operation. This covers the Admin API delete branches `202`, `204` and the
  bulk delete's `405`. Operations and negotiation values still need every arm.
- A rows file can carry `server: exclusive`: its workflow runs only when the
  party's IXIT declares `environment.exclusive_server: true`, and is skipped
  as `not-declared` otherwise (#738).
- The unfiltered Admin API bulk delete (`DELETE /admin/ehr/all` with no
  `ehr_id`) has a synchronous and an asynchronous workflow under that
  declaration (#738). The bulk delete's `404` stays a surface gap under
  upstream report #735, because no released sentence names the request it
  answers.
