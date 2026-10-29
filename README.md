<div align="center">

<img src="https://raw.githubusercontent.com/Vernum-Projecten/Veredictum/main/assets/brand/veredictum-icon.svg" width="112" alt="The Veredictum seal">

# Veredictum

**The independent conformance instrument for openEHR clinical data repositories.**

[Documentation](https://veredictum.eu/docs/) ·
[veredictum.eu](https://veredictum.eu) ·
[Quick start](#quick-start) ·
[GitHub Action](https://veredictum.eu/docs/github-action.html)

**Project**<br>
[![Release](https://img.shields.io/github/v/release/Vernum-Projecten/Veredictum?logo=github&label=release)](https://github.com/Vernum-Projecten/Veredictum/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/Vernum-Projecten/Veredictum/ci.yml?branch=main&logo=githubactions&logoColor=white&label=CI)](https://github.com/Vernum-Projecten/Veredictum/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/veredictum?logo=rust&label=crates.io)](https://crates.io/crates/veredictum)
[![crate downloads](https://img.shields.io/crates/d/veredictum?logo=rust&label=crate%20downloads)](https://crates.io/crates/veredictum)
[![docs.rs](https://img.shields.io/docsrs/veredictum?logo=docsdotrs&label=docs.rs)](https://docs.rs/veredictum)
[![Docs](https://img.shields.io/badge/docs-veredictum.eu-blue?logo=mdbook)](https://veredictum.eu/docs/)
[![GHCR](https://img.shields.io/badge/ghcr.io-veredictum-2496ED?logo=docker&logoColor=white)](https://github.com/Vernum-Projecten/Veredictum/pkgs/container/veredictum)
[![Image pulls](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fghcr-badge.elias.eu.org%2Fapi%2FVernum-Projecten%2FVeredictum%2Fveredictum&query=downloadCount&label=image%20pulls&logo=github)](https://github.com/Vernum-Projecten/Veredictum/pkgs/container/veredictum)
[![DOI](https://zenodo.org/badge/1347360549.svg)](https://doi.org/10.5281/zenodo.22113258)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-46215C?logo=apache)](https://github.com/Vernum-Projecten/Veredictum/blob/main/LICENSE)
[![Rust 1.98](https://img.shields.io/badge/rust-1.98-B7431B?logo=rust&logoColor=white)](https://github.com/Vernum-Projecten/Veredictum/blob/main/rust-toolchain.toml)

**Quality and security**<br>
[![CodeQL](https://img.shields.io/github/actions/workflow/status/Vernum-Projecten/Veredictum/codeql.yml?branch=main&logo=github&label=CodeQL)](https://github.com/Vernum-Projecten/Veredictum/actions/workflows/codeql.yml)
[![Quality gate](https://img.shields.io/sonar/quality_gate/rubentalstra_Veredictum?server=https%3A%2F%2Fsonarcloud.io&logo=sonarqubecloud&label=quality%20gate)](https://sonarcloud.io/summary/new_code?id=rubentalstra_Veredictum)
[![Coverage](https://img.shields.io/sonar/coverage/rubentalstra_Veredictum?server=https%3A%2F%2Fsonarcloud.io&logo=sonarqubecloud&label=coverage)](https://sonarcloud.io/component_measures?id=rubentalstra_Veredictum&metric=coverage)
[![Reliability rating](https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_Veredictum&metric=reliability_rating)](https://sonarcloud.io/component_measures?id=rubentalstra_Veredictum&metric=reliability_rating)
[![Security rating](https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_Veredictum&metric=security_rating)](https://sonarcloud.io/component_measures?id=rubentalstra_Veredictum&metric=security_rating)
[![Maintainability rating](https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_Veredictum&metric=sqale_rating)](https://sonarcloud.io/component_measures?id=rubentalstra_Veredictum&metric=sqale_rating)
[![Duplicated lines](https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_Veredictum&metric=duplicated_lines_density)](https://sonarcloud.io/component_measures?id=rubentalstra_Veredictum&metric=duplicated_lines_density)
<br>
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/Vernum-Projecten/Veredictum/badge)](https://scorecard.dev/viewer/?uri=github.com/Vernum-Projecten/Veredictum)
[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/14252/badge)](https://www.bestpractices.dev/projects/14252)
[![SLSA Build L3](https://slsa.dev/images/gh-badge-level3.svg)](https://veredictum.eu/docs/installation.html)

**The catalogue**<br>
the released openEHR specifications, version-aware per case ·
1651 spec-cited cases · 205 Arazzo documents

</div>

<!--
Every badge above is a live reading, not a claim, and the SLSA badge's claim is
substantiated by the attestation check the installation page shows. The book's
"About the project" chapter (website/book/src/about.md) says what each one reads.
-->

> **Status:** Veredictum is on 0.x and is not yet approved by the openEHR SEC.
> Version 1.0.0 will mark that approval.

## What Veredictum does

Veredictum grades openEHR servers. Point it at a running clinical data
repository (CDR) and it tells you which parts of the released openEHR
specifications that server implements, what load it sustains, and how fast it
answers. Every finding carries the specification section it comes from.

It drives a machine-readable catalogue of
[Arazzo 1.1.0](https://spec.openapis.org/arazzo/v1.1.0.html) workflows over
the server's own REST API, records every exchange, and computes the verdicts
as pure functions over that recording. Anyone who holds the recording can
judge it again and get the same bytes. One binary answers four questions:
does it conform (`test`), what class does it sustain (`perf`), where does it
break (`stress`), and how fast is it (`bench`).

### Who it is for

- **Evaluators and procurement teams** that need a claim about a CDR someone
  other than its vendor can check.
- **CDR vendors and maintainers** that want to know where their server departs
  from the released text before a customer finds out.
- **The openEHR community**, which gets a conformance test suite in
  machine-readable form.

### Why you can trust its verdicts

The released openEHR specifications are the only authority it accepts. Every
expectation names the section it comes from, so it can be refuted by a better
reading of that text and by nothing else, never by what a server did. Where
the text is silent, no case asserts the behaviour. When a row goes red, the
instrument is a suspect ahead of the server: the first live triage attributed
7 of 7 diagnosed defects to the instrument and none to the server under test.
Releases are signed and carry SLSA Build L3 provenance and an SBOM, and the
repository publishes the instrument and no results about anybody's product.
[The conformance method](https://veredictum.eu/docs/methodology.html) sets
this out in full.

## Quick start

The fastest path installs nothing:
[console.veredictum.eu](https://console.veredictum.eu) is the hosted
instrument, for functional conformance runs against a server the internet can
reach. To run it yourself, pick one of the three below.

### The container image

The image ships the catalogue and the specification oracle, and runs any
subcommand given as its first argument:

```bash
mkdir -p out
export VEREDICTUM_CDR_TOKEN=…   # read from the environment, never from argv
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD/out:/work/out" -e VEREDICTUM_CDR_TOKEN \
  ghcr.io/vernum-projecten/veredictum:<tag> \
  test --base-url https://cdr.example/openehr/v1 --auth bearer --out /work/out
```

The run writes `out/results.json` and `out/recording.har`, and exits `1` when
a row failed or errored. Run with no arguments, the image serves the web
console instead. The example is for a CDR that wants a bearer token;
[Logging in to the CDR](https://veredictum.eu/docs/authentication.html) covers
Basic and no login. [Running the instrument](https://veredictum.eu/docs/running.html)
covers the statement that selects what runs, and
[Installation](https://veredictum.eu/docs/installation.html#with-docker-the-web-console)
the console.

### cargo install

```bash
cargo install veredictum --locked
git clone https://github.com/Vernum-Projecten/Veredictum && cd Veredictum
veredictum validate --root catalogue --specs specs/openehr
veredictum test --base-url https://cdr.example/openehr/v1 --auth bearer --out out/
```

The crate carries the code; the catalogue and the specification oracle are
over 300 MB of data and live in the clone.
[Installation](https://veredictum.eu/docs/installation.html) also covers the
signed release binaries and how to verify them.

### The GitHub Action

One step in a vendor's workflow runs the catalogue against their server and
fails the job on a failed row:

```yaml
- uses: Vernum-Projecten/Veredictum@<commit-sha> # vX.Y.Z
  env:
    VEREDICTUM_CDR_TOKEN: ${{ secrets.CDR_TOKEN }}
  with:
    base-url: http://127.0.0.1:8080/openehr/v1
    statement: conformance/statement.json
    ixit: conformance/ixit.json
    auth: bearer
```

[The GitHub Action](https://veredictum.eu/docs/github-action.html) has a
complete workflow, every input, and what to keep out of an uploaded artifact.

## What ships today

### Conformance

- [The Arazzo catalogue](https://veredictum.eu/docs/introduction.html#what-is-in-the-box): 1651 spec-cited cases in 15 capability folders, one behaviour per case, with a refusal case for every invalid shape
- [`validate`](https://veredictum.eu/docs/commands.html#validate): every gate over the catalogue, plus the static review of a party's statement
- [`test`](https://veredictum.eu/docs/running.html#4-drive-the-catalogue): drives the catalogue over the server's own wire and records every exchange as HAR
- [`test --rejudge`](https://veredictum.eu/docs/running.html#re-judging-a-recording): judges a recording again with no server, byte for byte
- [Options and upstream reports](https://veredictum.eu/docs/methodology.html#where-the-specification-does-not-say) where the release leaves a behaviour open or says nothing
- [SMART on openEHR](https://veredictum.eu/docs/commands.html#test), with a Platform base URL and scoped tokens

### Measured instruments

- [`perf`](https://veredictum.eu/docs/running.html#measured-performance-stress-and-speed): earns a population-anchored performance class (POC, S, L, R) under open-loop load
- [`stress`](https://veredictum.eu/docs/commands.html#stress) and [`aql-probe`](https://veredictum.eu/docs/commands.html#aql-probe): find where a deployment breaks, as exploration that never yields a conformance record
- [`bench`](https://veredictum.eu/docs/running.html#benchmark-a-cdr-in-one-command): comparative speed from embedded packs, with EHRbase and FerroEHR as pinned baselines

### Results and CI exports

- [`export`](https://veredictum.eu/docs/commands.html#export): JUnit XML, CTRF and a Markdown summary of a results document
- [`verdicts` and `verify-record`](https://veredictum.eu/docs/trust.html#the-record-does-not-depend-on-trusting-the-operator): re-derived class verdicts, sealed with a digest manifest and an OpenPGP signature
- [Published JSON Schemas](https://veredictum.eu/docs/commands.html#emit-schemas) for every artifact family, drift-tested
- [Signed releases](https://veredictum.eu/docs/installation.html#from-a-release-binary) for `x86_64` and `aarch64` Linux, with SLSA provenance and a CycloneDX SBOM

### The console

- [The web console](https://veredictum.eu/docs/console.html): connect a CDR, paste the vendor's claim, watch the run live, read the results and the verdicts
- [The hosted instrument](https://veredictum.eu/docs/hosted.html) at console.veredictum.eu, with its machine, its price and who can change it stated in public

## Learn more

- [Documentation](https://veredictum.eu/docs/), starting with the [introduction](https://veredictum.eu/docs/introduction.html)
- [What the instrument touches](https://veredictum.eu/docs/trust.html), before you run it
- [The conformance method](https://veredictum.eu/docs/methodology.html): the oracle, the attribution law, how a verdict is computed
- [The hosted instrument](https://veredictum.eu/docs/hosted.html): where it runs, results and certificates
- [Command reference](https://veredictum.eu/docs/commands.html) and [glossary](https://veredictum.eu/docs/glossary.html)
- [About the project](https://veredictum.eu/docs/about.html): lineage, the origin of the name, the design record, credits
- [Changelog](https://github.com/Vernum-Projecten/Veredictum/blob/main/CHANGELOG.md) and the [public roadmap board](https://github.com/orgs/Vernum-Projecten/projects/1)

## Contributing

[`CONTRIBUTING.md`](https://github.com/Vernum-Projecten/Veredictum/blob/main/CONTRIBUTING.md)
has the gates and the review bar, and
[`CLAUDE.md`](https://github.com/Vernum-Projecten/Veredictum/blob/main/CLAUDE.md)
is the working discipline the project holds itself to. Questions go through
[`SUPPORT.md`](https://github.com/Vernum-Projecten/Veredictum/blob/main/SUPPORT.md).
If you maintain a CDR and want it graded, open an issue. A defect this
instrument finds in your server arrives with the reproduced exchange and the
citation, and a defect you find in this instrument is a first-class bug here.

## Security

Report a vulnerability as
[`SECURITY.md`](https://github.com/Vernum-Projecten/Veredictum/blob/main/SECURITY.md)
describes, never in a public issue.

## Licence

Apache-2.0 ([LICENSE](https://github.com/Vernum-Projecten/Veredictum/blob/main/LICENSE)).
The vendored specification text and clinical models keep their upstream
terms, declared in `REUSE.toml`;
[About the project](https://veredictum.eu/docs/about.html#licence) states them.

openEHR® is the registered trademark of the openEHR Foundation. Veredictum is
an independent, community-driven instrument that names openEHR to say what it
tests against. It is not an official openEHR Foundation product and is not
endorsed by or affiliated with the Foundation.
