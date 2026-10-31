# Introduction

Veredictum is a conformance instrument for openEHR clinical data repositories.
You point it at a running CDR and it tells you, with citations, which parts of
the openEHR specification that server implements.

> **Status:** Veredictum is on 0.x and is not yet approved by the openEHR SEC.
> Version 1.0.0 will mark that approval.

It executes a machine-readable catalogue of 1793 spec-cited test cases against
the server's own wire, records every exchange, and computes verdicts as pure
functions over what it recorded. Functional conformance and measured
performance, a single run or two servers compared on one host, come from the
one tool.
This repository publishes the instrument and no results about anybody's
product.

[console.veredictum.eu](https://console.veredictum.eu) is the hosted
conformance instrument. It serves the catalogue, and a run driven there against
a publicly reachable CDR shows the full results and verdicts for that server.
It runs functional conformance only; the measured instruments are run locally
with the command line. The instance holds no key and nothing durable.
[The hosted instrument](hosted.md) says where it runs and what a hosted run
establishes. Running the instrument yourself works against any deployment you
can reach ([Installation](installation.md)).

The released openEHR specifications are the only authority the instrument
accepts. Every expectation in the catalogue names the section it comes from, so
it can be refuted by a better reading of the specification and by nothing else.

## Two products over one engine

- **The instrument CLI:** the `veredictum` command, installed from
  [crates.io](https://crates.io/crates/veredictum) or taken as a signed
  release binary. Every verdict this repository speaks is a run of it.
- **The web console:** the container image at
  [ghcr.io/vernum-projecten/veredictum][ghcr],
  a browser frontend that drives the same pinned CLI underneath: connect a
  CDR, paste the vendor's claim, watch the run live, read the results and the
  verdicts. The image also runs any CLI subcommand given as its first
  argument, so `docker run <image> test …` drives the catalogue it ships.

Both are pre-1.0. The 0.x line publishes working releases and makes no
API-stability claim yet; every claim a release does make is checked, signed
and reproducible, which is the stability that matters for a verdict.

## Two questions, one command surface

| Question | Commands |
|---|---|
| **Does it conform?** | `validate` checks the catalogue itself before any server is involved; zero findings is the only passing result. `test` drives the cases your statement selects against the server over its own REST wire, records every exchange as HAR, judges each row and writes `results.json`; `--rejudge` reproduces those results from the recording alone. `export` renders the results for a CI system. |
| **How does it hold up under clinical traffic?** | `perf` offers a declared workload as Poisson arrivals, walks a load ladder to the knee in at most ten minutes after seeding, and writes a `perf-result` whose every number `perf-judge` re-derives. `perf-compare` aligns records, and runs two CDRs, or one against itself, interleaved on one host. `perf-workloads` checks a workload document without a server. A performance record earns no class and is never a conformance verdict. |

More subcommands serve the records themselves: `show` prints a results
document or a `perf-result` in the terminal, `verify` checks a conformance
certificate, `emit-schemas` writes the published JSON Schemas, `perf-assets`
draws the charts of a `perf-result`, and `identify` asks a server who it is. `veredictum --help` is the authoritative list, and the
[command reference](commands.md) is generated from it.

A certificate is a JWS (RFC 7515) signed with Ed25519, and its payload names
the server, the catalogue revision, the engine version, the verdict summary
and the SHA-256 digests of the run's `results.json` and HAR recording.
`veredictum verify <certificate> --jwks <file>` checks it offline against the
published JWK Set, so anyone holding the set can check a certificate without
the console, and any JOSE library can do the same. The hosted instrument
issues none yet: it runs in trial mode, where runs, results and verdicts work
and no certificate is created ([checking a certificate](trust.md#verifying-a-certificate)).

## Who this is for

- **Evaluators and procurement teams.** You need a claim about a CDR that
  someone other than its vendor can check. The instrument produces a results
  document and the recording it was judged from, so anyone holding the
  recording can re-judge it and get the same bytes.
- **CDR vendors and maintainers.** You want to know where your server diverges
  from the released text before a customer finds out. A failing row arrives
  with the exchange that produced it and the specification section it violates.
- **The openEHR community.** The catalogue is an openEHR conformance test suite
  in machine-readable form. Where the specification is silent or contradicts
  itself, no case asserts the behaviour: the divergence is an upstream report
  with a proposed patch, never resolved privately.

## What is in the box

The instrument is one binary plus a data tree, and the data tree lives in the
repository. The counts on these pages are the line `veredictum validate`
prints over `catalogue/`, and a CI guard fails the build when a count here
disagrees with the catalogue.

| | |
|---|---|
| **The catalogue** | `catalogue/cases/`: 1793 spec-cited cases as [Arazzo 1.1.0](https://spec.openapis.org/arazzo/v1.1.0.html) workflows over the released openEHR REST API OpenAPI documents, in 246 Arazzo documents. One small isolated case per behaviour, so a red row names one defect. Every invalid shape is kept as its own refusal case, so a lenient server fails it. Every criterion carries the released sentence it enforces |
| **15 capability folders** | EHR, EHR status, composition, content, contribution, directory, item tags, query, definition, demographic, admin, security, simplified formats, SMART, system. A statement claims folders |
| **The options** | `catalogue/capabilities.yaml`: the behaviours the release leaves to the implementation, each an option with its arms; a party's statement picks one arm of each, and the documents of every other arm are skipped |
| **The id map** | `catalogue/id-map.yaml`: every case id of the former case-core catalogue, mapped to the workflows that carry it or to the reason none does |
| **The performance workloads** | `perf/`: the workload documents `perf` offers, the host prescriptions a record may claim and the payloads it sends, kept apart from the conformance catalogue |
| **The published schemas** | `schemas/`: JSON Schema for every artifact family, emitted by the instrument and drift-tested, so you can author against the format |
| **The verification pack** | `verification-pack/arazzo/`: recorded HAR exchanges with their adjudicated results. The evaluator re-judges every recording and must reproduce every row, so no harness, this one included, is trusted on its word |
| **The specification oracle** | `specs/openehr/`: the released specification text, vendored verbatim, plus the released XSD, JSON Schema and OpenAPI bundles a citation resolves against |

## Two commands, and a re-judgement

1. `veredictum validate` checks the catalogue itself. Zero findings is the only
   passing result, and it runs before any server is involved.
2. `veredictum test` drives the catalogue against your server, records every
   exchange as HAR and writes `results.json`, judged row by row.
3. `veredictum test --rejudge` judges the recording again with no server and
   writes the same bytes, so the judgement can be checked by anyone holding
   the recording.

[Installation](installation.md) covers getting the command. [Running the
instrument](running.md) walks the steps against a live server.

## Coverage is machine-checked

A green run over a thin catalogue proves nothing, so a gate enumerates the wire
surface from the released OpenAPI documents alone and fails on any operation or
response branch that has neither a covering case nor a recorded gap. Cases are added. They are never
removed to make a run go green.

[The conformance method](methodology.md) says what the gate reads and what
happens to a gap it finds.

> [!NOTE]
> openEHR® is the registered trademark of the openEHR Foundation. Veredictum
> is an independent, community-driven conformance instrument: it names
> openEHR descriptively, to say what is being tested, and it is not an
> official openEHR Foundation product, not the Foundation's CNF program, and
> not endorsed by or affiliated with the Foundation. The released openEHR
> specifications are this instrument's oracle by its own choice, and every
> expectation cites them. That fidelity is a design discipline here, never a
> claim of official status.

[ghcr]: https://github.com/Vernum-Projecten/Veredictum/pkgs/container/veredictum
