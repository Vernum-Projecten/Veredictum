# What the instrument touches

## Verify the binary first

```bash
gh attestation verify veredictum-<tag>-<target>.tar.gz \
    -R Vernum-Projecten/Veredictum \
    --signer-workflow Vernum-Projecten/Veredictum/.github/workflows/release-build.yml
```

If it fails, do not run the file.

## What each command reaches

- **`test`:** the base URL you give; it creates and deletes data there, so use
  a test system. With `--rejudge` it sends nothing.
- **`perf`:** the IXIT's base URL, where it creates its corpus. With a
  `containers` block, also the Docker Engine API (container statistics), plus
  read-only queries and one `vacuumdb --analyze` in the database container.
- **`perf-compare --ab` / `--aa`:** as `perf`, per side. `--with-baselines`
  also pulls and runs two pinned reference images (ghcr.io, Docker Hub) on
  loopback through your `docker` CLI, then removes them and their volumes.
- **`validate`, `export`, `show`, `verify`, `perf-judge`,
  `perf-compare --result`, `perf-workloads`, `perf-assets`:** offline.
- **The console image:** loopback by default, no login; reaches a CDR only
  through runs you start.

No command sends telemetry. Credentials come only from environment variables
([Logging in to the CDR](authentication.md)). Any record can be re-checked
without trusting the operator: `test --rejudge` for a recording, `perf-judge`
for a performance record.

## Verifying a certificate

A certificate is an Ed25519-signed JWS (RFC 7515) naming the server, catalogue
revision, engine version, verdict summary, and the SHA-256 digests of the
run's `results.json` and HAR recording. The JWK Set is at
`https://console.veredictum.eu/.well-known/jwks.json` and in
`deploy/hosted/jwks.json`:

```bash
veredictum verify certificate.jws --jwks jwks.json
sha256sum results.json recording.har
```

`verify` exits `0` and prints the certificate's statements, or `1` naming the
refusal: payload or header changed, unknown key, algorithm other than `EdDSA`,
revoked key, or key expired before issue. Its two digests must match
`sha256sum`'s.

A JOSE library verifies the same signature but ignores the set's
`expires_at` and `revoked_at` members; check `revoked_at` yourself.

The hosted instrument runs in trial mode and issues no certificate yet, so the
set holds no key.
