The offline check of a conformance certificate. It reads the two files you
name and no network, so the published JWK Set is enough: the copy at
`https://console.veredictum.eu/.well-known/jwks.json`, or
`deploy/hosted/jwks.json` in this repository. Any JOSE library verifies the
signature; this command also refuses a key the set marks `revoked_at` and a
certificate issued after its key's `expires_at`, which a generic library
ignores.

A verified certificate prints the server with who supplied each identity
field, the base URL, the catalogue digest, the engine version and its approval
status, the verdict summary, and the SHA-256 digests of the run's
`results.json` and HAR recording. Run `sha256sum` over the two files a vendor
supplied to check they are the ones the certificate covers. The payload's
schema is `schemas/certificate.schema.json`; the key set's is
`schemas/jwks.schema.json`.
