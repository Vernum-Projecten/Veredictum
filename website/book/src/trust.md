# What the instrument touches

You should not have to read the source to decide whether this tool is safe to
run. This page says what each command reaches, and how to verify the binary
before running anything.

## Verify the binary first

Every release binary carries a Sigstore attestation. One command proves the
exact bytes came out of this repository's build workflow:

```bash
gh attestation verify veredictum-<tag>-<target>.tar.gz \
    -R Vernum-Projecten/Veredictum \
    --signer-workflow Vernum-Projecten/Veredictum/.github/workflows/release-build.yml
```

If that check fails, do not run the file.

## What each command reaches

- **`perf`** sends requests to the base URL your IXIT names, creates the
  corpus it measures on that server, and writes the record where `--out`
  says. With a `containers` block in the IXIT it also reads the named
  containers' statistics from the Docker Engine API, and runs read-only
  queries and one `vacuumdb --analyze` in the database container. The
  credential is read from the environment variables the IXIT names, and
  there is no telemetry and no other network destination.
- **`perf-compare`** with `--ab` or `--aa` does what `perf` does for each
  side. With `--with-baselines` it also drives your local `docker` CLI to pull
  and run the two pinned reference images (from ghcr.io and Docker Hub) on
  loopback ports, and removes them with their volumes when done.
- **`test`** sends requests to the base URL you give it,
  reads the catalogue and the vendored specs, and writes into `--out`. It
  creates and deletes data on the target, which is why you point it at a test
  system. With `--rejudge` it sends nothing at all.
- **`validate`, `export`, `show`, `verify`, `perf-judge`, `perf-compare
  --result`, `perf-workloads`, `perf-assets`** are offline: they read the paths you pass and write where you
  say. No network at all.
- **The console image** binds loopback by default, has no login, and reaches
  a CDR only through the runs you start in it.

## The record does not depend on trusting the operator

A conformance run's recording carries the exchanges its results are
re-judged from (`veredictum test --rejudge`), and a performance record embeds
the histograms every number is re-derived from (`veredictum perf-judge`), so
both are checkable after the fact.

## Verifying a certificate

A certificate from the hosted instrument is a JWS (RFC 7515) signed with
Ed25519. Check it offline against the published JWK Set, which is served at
`https://console.veredictum.eu/.well-known/jwks.json` and committed in the
repository as `deploy/hosted/jwks.json`:

```bash
veredictum verify certificate.jws --jwks jwks.json
sha256sum results.json recording.har
```

`verify` exits `0` and prints what the certificate states, or exits `1` and
names the refusal: a payload or header changed after signing, a key the set
does not hold, an algorithm other than `EdDSA`, a key the set marks revoked,
or a key that expired before the certificate was issued. The two digests it
prints are the ones `sha256sum` should print for the run's files. A valid
signature proves the key's holder signed that payload unchanged; it claims
nothing the payload does not name.

Any JOSE library verifies the same signature. Two members of a key in the set
are this instrument's own, `expires_at` and `revoked_at`, and a generic library
ignores them, so check `revoked_at` yourself if you verify that way.

The hosted instrument runs in trial mode today: runs, results and verdicts are
complete, every page says it is a trial, and no certificate is issued, so the
published set holds no key yet.
