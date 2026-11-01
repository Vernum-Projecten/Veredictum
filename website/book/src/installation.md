# Installation

To install nothing, use the hosted instrument at
[console.veredictum.eu](https://console.veredictum.eu): it browses the
catalogue and runs functional conformance against a publicly reachable CDR.
Install locally to reach private deployments, run the measured instruments, or
work offline.

<!-- toc -->

## What you need, and why a clone is part of it

The instrument reads three paths at run time: the catalogue, the vendored
specification oracle, and your declaration of the deployment under test. The
catalogue and oracle are over 300 MB, too large for a package registry, so the
crate and the image carry the code and a clone carries the data:

```bash
git clone https://github.com/Vernum-Projecten/Veredictum
cd Veredictum
```

Every install path below runs against that clone.

## With cargo

```bash
cargo install veredictum --locked
veredictum validate --root catalogue --specs specs/openehr
```

`--locked` builds against the release's tested `Cargo.lock`; without it cargo
resolves fresh dependency versions. The crate also publishes a library target
with the typed artifact model and the JSON Schemas.

## With Docker: the web console

The image is the web console. The CLI ships as release binaries, not an image.
The operator compose file pins the image version and binds the console to
loopback on port 3210:

```bash
curl -LO https://raw.githubusercontent.com/Vernum-Projecten/Veredictum/main/deploy/docker/docker-compose.yml
docker compose up
```

Open <http://127.0.0.1:3210>. Releases after v0.1.1 attach the same file,
pinned to that release's image, to the
[release page](https://github.com/Vernum-Projecten/Veredictum/releases).
Without compose:

```bash
docker run --rm -p 127.0.0.1:3210:3000 -v "$PWD:/work" \
    ghcr.io/vernum-projecten/veredictum:<tag>
```

Take `<tag>` from the
[package page](https://github.com/Vernum-Projecten/Veredictum/pkgs/container/veredictum).
The image is multi-architecture and pushed by digest; tags apply only after a
smoke run and a vulnerability scan of that digest pass.

The image carries the release's catalogue and oracle at `/work/catalogue` and
`/work/specs`, so it works in an empty directory. Bind-mount your own checkout
over those paths to grade against it. The console has no login, so both
commands bind loopback; exposing it is the operator's decision.
[The console chapter](console.md) shows what it does.

## From a release binary

Every [release](https://github.com/Vernum-Projecten/Veredictum/releases)
attaches `x86_64` and `aarch64` Linux binaries, each tarball with a
`sha256sum`, a CycloneDX SBOM and a Sigstore bundle. Verify before running;
`--signer-workflow` asserts the file was built by this repository's workflow:

```bash
gh attestation verify veredictum-<tag>-<target>.tar.gz \
    -R Vernum-Projecten/Veredictum \
    --signer-workflow Vernum-Projecten/Veredictum/.github/workflows/release-build.yml
```

A release stays a draft until every asset is attached. Tags are signed and a
repository rule refuses their deletion, so a tag never moves; a bad cut is
fixed by the next version.

## From source

`rust-toolchain.toml` pins the toolchain:

```bash
cargo run -- validate --root catalogue --specs specs/openehr
```

The minimum supported Rust version is 1.98, checked in CI. `cargo-nextest` is
needed only to run the project's own tests.

## Checking that the install works

`validate` needs no server:

```bash
veredictum validate --root catalogue --specs specs/openehr
```

An intact clone prints one line and exits zero:

```text
253 document(s), 1822 workflow(s), 376 rows file(s), 15 capability folder(s), 0 finding(s)
```

A finding count above zero is a catalogue failure, not a setup problem; the
findings printed above the line name the document at fault.
