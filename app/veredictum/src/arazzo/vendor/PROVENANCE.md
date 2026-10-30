# Vendored JSON Schemas

Two schemas, each fetched by its own script under `scripts/vendor/`.

## Arazzo 1.1 JSON Schema

Source: <https://spec.openapis.org/arazzo/1.1/schema/2026-04-15>, the dated
schema revision the OpenAPI Initiative publishes for Arazzo 1.1.x documents
(its `$id` is that URL).
Upstream repository: <https://github.com/OAI/Arazzo-Specification>.
SHA-256: `37be908409bdb2f7bffe61fa23685c7e84cbeebfafac475a1d01dbc50ff7ab9e`.
Fetched: 2026-10-23 by `scripts/vendor/arazzo-schema.sh`, which refuses bytes
with any other digest.
License: Apache-2.0, the upstream repository's licence (`REUSE.toml` records
it for this file). The repository's `LICENSE` at commit
`e9ee1c49414780441fef0c5196c52f8222357a0a` is vendored beside the schema as
`LICENSE-Arazzo-Specification`, by the same script and pinned by its own
SHA-256, because the dated schema URL names no commit.

The file is byte-for-byte what the URL served. It is compiled into the engine,
because the crate publishes to crates.io and may only embed files it ships.

### Where the schema and the Arazzo 1.1.0 text disagree

The specification text (<https://spec.openapis.org/arazzo/v1.1.0.html>) is
normative and the schema is not. The engine amends the compiled schema where
the two disagree, never the vendored bytes, and each amendment checks that the
vendored value it replaces is still what it expects:

- **OAI/Arazzo-Specification#559.** The schema drops `object` from Parameter
  Object `value`, while §Parameter Object types the field
  `Any | {expression} | Selector Object`. The engine restores the plain-object
  branch.
- **Payload Replacement Object `value`.** The schema admits only a string or
  a Selector Object, while §Payload Replacement Object types the field
  `Any | {expression} | Selector Object` ("The value can be a constant, a
  Runtime Expression, or Selector Objects") and gives `value: 10` as its
  literal example. The engine admits a literal of every JSON type there.
- **OAI/Arazzo-Specification#558** needs no amendment. The schema requires
  Expression Type Object `version` against its own description, and the
  specification text agrees with the `required` list: §Expression Type Object,
  Fixed Fields, marks `version` REQUIRED.

## CTRF 0.2.0 JSON Schema

Source: <https://raw.githubusercontent.com/ctrf-io/ctrf/e325eb33c7662a208bfa4d093a41cc55439d6e6a/schema/ctrf.schema.json>,
the file `schema/ctrf.schema.json` at the commit the `v0.2.0` tag of the
Common Test Report Format repository names.
Upstream repository: <https://github.com/ctrf-io/ctrf>.
SHA-256: `cb9bd075beb0e046076046793f96bb1ba72bd1b1d331b1fbd42ac0ea36776cf3`.
Fetched: 2026-10-28 by `scripts/vendor/ctrf-schema.sh`, which refuses bytes
with any other digest.
License: MIT, the upstream repository's licence (`REUSE.toml` records it for
this file, and `LICENSES/MIT.txt` carries the text). The repository's `LICENSE`
at the same commit is vendored beside the schema as `LICENSE-ctrf`, by the
same script and pinned by its own SHA-256.

The file is byte-for-byte what the URL served. The CTRF export pins
`specVersion` 0.2.0, and its tests validate every emitted document against
this schema.

## JUnit XML schemas

JUnit XML has no single owner. The export is held to the two schemas the tools
that read it publish, both fetched by `scripts/vendor/junit-schemas.sh`, which
refuses bytes with any other digest.

`surefire-test-report-3.6.1.xsd` is Apache Maven Surefire's test report
schema, which defines one `<testsuite>`.
Source: <https://raw.githubusercontent.com/apache/maven-surefire/5bf0dcf034365f13dc7fdc7a2776e936e636bc52/maven-surefire-plugin/src/site/resources/xsd/surefire-test-report.xsd>,
the commit the `surefire-3.6.1` tag names; the same bytes Apache publishes at
<https://maven.apache.org/surefire/maven-surefire-plugin/xsd/surefire-test-report.xsd>.
SHA-256: `f612ebfbd7cb285ac1af2fa9c103f2b83080feffffc6b3dfcc5ab6673d14f052`.
License: Apache-2.0. The repository's `LICENSE` at the same commit is vendored
as `LICENSE-maven-surefire`, pinned by its own SHA-256.

`junit-10-xunit-3.1.7.xsd` is the Jenkins xUnit plugin's `junit-10.xsd`, which
defines the `<testsuites>` root that wraps several suites.
Source: <https://raw.githubusercontent.com/jenkinsci/xunit-plugin/5fbba4002e02a66032804c5cace4cf51b20491cf/src/main/resources/org/jenkinsci/plugins/xunit/types/model/xsd/junit-10.xsd>,
the commit the `xunit-3.1.7` release tag names.
SHA-256: `a1a816f58d1bf95ebabf371994df0b9246dee66ea9572fbec4f9296f1b2c0ff6`.
License: MIT. The upstream repository has no licence file; the schema's own
header comment carries the MIT text, and the vendor script copies it out of the
fetched bytes into `LICENSE-xunit-plugin`.

Both files are byte-for-byte what the URLs served. `scripts/checks/junit-schema.sh`
validates every pinned JUnit view against them: the whole document against the
xUnit schema, and each `<testsuite>` against the Surefire schema.
