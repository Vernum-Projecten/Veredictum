- Conformance certificates: a JWS in the compact serialization, signed with
  Ed25519 (`EdDSA`), whose payload names the server with the source of each
  identity field, the catalogue digest, the engine version and status, the
  verdict summary and the SHA-256 digests of `results.json` and the HAR
  recording. `veredictum verify <certificate> --jwks <file>` checks one offline
  and refuses a changed payload, an unknown `kid`, another algorithm, a revoked
  key and a key that expired before issuance, each as its own error. The
  payload and the key set are published as `schemas/certificate.schema.json`
  and `schemas/jwks.schema.json`.
