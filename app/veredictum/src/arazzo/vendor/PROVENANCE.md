# Vendored Arazzo 1.1 JSON Schema

Source: <https://spec.openapis.org/arazzo/1.1/schema/2026-04-15>, the dated
schema revision the OpenAPI Initiative publishes for Arazzo 1.1.x documents
(its `$id` is that URL).
Upstream repository: <https://github.com/OAI/Arazzo-Specification>.
SHA-256: `37be908409bdb2f7bffe61fa23685c7e84cbeebfafac475a1d01dbc50ff7ab9e`.
Fetched: 2026-10-23 by `scripts/vendor/arazzo-schema.sh`, which refuses bytes
with any other digest.
License: Apache-2.0, the upstream repository's licence (`REUSE.toml` records
it for this file).

The file is byte-for-byte what the URL served. It is compiled into the engine,
because the crate publishes to crates.io and may only embed files it ships.

## Where the schema and the Arazzo 1.1.0 text disagree

The specification text (<https://spec.openapis.org/arazzo/v1.1.0.html>) is
normative and the schema is not. The engine amends the compiled schema where
the two disagree, never the vendored bytes, and each amendment checks that the
vendored value it replaces is still what it expects:

- **OAI/Arazzo-Specification#559.** The schema drops `object` from Parameter
  Object `value`, while §Parameter Object types the field
  `Any | {expression} | Selector Object`. The engine restores the plain-object
  branch.
- **OAI/Arazzo-Specification#558** needs no amendment. The schema requires
  Expression Type Object `version` against its own description, and the
  specification text agrees with the `required` list: §Expression Type Object,
  Fixed Fields, marks `version` REQUIRED.
