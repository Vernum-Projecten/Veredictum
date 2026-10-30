- `veredictum identify` and the `server` member of `results.json` record a
  System OPTIONS answer whose code lies from 600 to 999 as the outcome
  `invalid_status`, since RFC 9110 §15 makes those codes invalid.
  `not_offered` now covers 100 to 599 only, so every document the instrument
  prints validates against `identity.schema.json`.
