- `openehr-query`, the AQL parser `validate` checks every catalogue query with,
  is now Apache-2.0 from 0.0.116, and the instrument moves to that version.
  `deny.toml` no longer carries the BUSL-1.1 licence exception, `NOTICE` names
  no non-permissive dependency, and every dependency of the binary and the
  container image is under a licence the allow-list admits. The parser's new
  depth refusal is reported with its position like any other parse failure.
