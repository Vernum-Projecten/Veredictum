The offline check of a conformance certificate. It reads the two files you
name and reaches no network, so a certificate verifies with the published JWK
Set alone: the copy at `https://console.veredictum.eu/.well-known/jwks.json`,
or the one committed in this repository as `deploy/hosted/jwks.json`. Any JOSE
library verifies the same signature; what this command adds is the refusal of
a key the set marks `revoked_at`, and of a certificate issued after its key's
`expires_at`, which a generic library ignores.

A verified certificate prints what it states: the server with who supplied
each identity field, the base URL, the catalogue digest, the engine version and
its approval status, the verdict summary, and the SHA-256 digests of the run's
`results.json` and HAR recording. Run `sha256sum` over the two files a vendor
received to check they are the exchanges the certificate covers. The payload's
JSON Schema is `schemas/certificate.schema.json`, and the key set's is
`schemas/jwks.schema.json`.
