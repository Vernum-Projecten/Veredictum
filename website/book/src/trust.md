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

- **`bench`** sends HTTP requests to the base URL you give it, and writes into
  the `--out` directory you give it. Nothing else. The packs are compiled into
  the binary, so it fetches nothing; the credential is read from an
  environment variable and never rides argv; there is no telemetry and no
  other network destination. With `--with-baselines` it additionally drives
  your local `docker` CLI to pull and run the two pinned reference images
  (from ghcr.io and Docker Hub) on loopback ports, and removes them when done.
- **`test`** sends requests to the base URL you give it,
  reads the catalogue and the vendored specs, and writes into `--out`. It
  creates and deletes data on the target, which is why you point it at a test
  system. With `--rejudge` it sends nothing at all.
- **`validate`, `verdicts`, `export`, `verify-record`, `bench-compare`, the
  asset renderer** are offline: they read the paths you pass and write where you
  say. No network at all.
- **The console image** binds loopback by default, has no login, and reaches
  a CDR only through the runs you start in it.

`bench` warns once when a credential would ride plain `http` to a host that is
not loopback.

## The record does not depend on trusting the operator

A conformance run's recording carries the exchanges its results are
re-judged from (`veredictum test --rejudge`), and a bench record embeds the histograms
its summary is re-derived from, so both are checkable after the fact.
