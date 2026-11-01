# Introduction

Veredictum is a conformance instrument for openEHR clinical data repositories.
Point it at a running CDR and it reports, with citations, which parts of the
openEHR specification that server implements.

> **Status:** Veredictum is on 0.x and is not yet approved by the openEHR SEC.
> Version 1.0.0 will mark that approval.

It drives a catalogue of 1902 spec-cited test cases over the server's own
wire, records every exchange, and computes verdicts as pure functions over the
recording. It also measures performance, for one server or two on one host.
The released openEHR specifications are its only authority: every expectation
names the section it comes from. The repository publishes no results about
anybody's product.

[console.veredictum.eu](https://console.veredictum.eu) runs functional
conformance against a publicly reachable CDR ([The hosted
instrument](hosted.md)). For performance or private deployments, see
[Installation](installation.md).

## Two products over one engine

- **The instrument CLI:** `veredictum`, from
  [crates.io](https://crates.io/crates/veredictum) or a signed release binary.
- **The web console:** the image [ghcr.io/vernum-projecten/veredictum][ghcr],
  a browser frontend over the same pinned CLI. `docker run <image> test …`
  runs any CLI subcommand given as the first argument.

## The command surface

| Question | Commands |
|---|---|
| **Does it conform?** | `validate` checks the catalogue before any server is involved; zero findings is the only pass. `test` drives the cases your statement selects, records every exchange as HAR, and writes `results.json`; `--rejudge` reproduces the results from the recording alone. `export` renders results for a CI system. |
| **How does it hold up under clinical traffic?** | `perf` offers a declared workload as Poisson arrivals and walks a load ladder to the knee in at most ten minutes after seeding, writing a `perf-result` that `perf-judge` re-derives. `perf-compare` aligns records, and runs two CDRs, or one against itself, interleaved on one host. `perf-workloads` checks a workload document offline. A performance record earns no class and is never a conformance verdict. |

Also: `show`, `verify` ([certificates](trust.md#verifying-a-certificate)),
`emit-schemas`, `perf-assets` and `identify`. The
[command reference](commands.md) is generated from `veredictum --help`.
[Running the instrument](running.md) walks a live run.

## Who this is for

- **Evaluators and procurement teams:** a claim about a CDR that anyone
  holding the recording can re-judge to the same bytes.
- **CDR vendors and maintainers:** each failing row carries its exchange and
  the section it violates.
- **The openEHR community:** a machine-readable conformance test suite. Where
  the specification is silent or contradicts itself, no case asserts the
  behaviour; it becomes an upstream report with a proposed patch.

## What is in the box

The counts are what `veredictum validate` prints; a CI guard fails the build
if they drift.

| | |
|---|---|
| **The catalogue** | `catalogue/cases/`: 1902 spec-cited cases as [Arazzo 1.1.0](https://spec.openapis.org/arazzo/v1.1.0.html) workflows over the released openEHR REST API OpenAPI documents, in 274 Arazzo documents. One case per behaviour; each invalid shape is its own refusal case; every criterion carries the sentence it enforces |
| **15 capability folders** | EHR, EHR status, composition, content, contribution, directory, item tags, query, definition, demographic, admin, security, simplified formats, SMART, system. A statement claims folders |
| **The options** | `catalogue/capabilities.yaml`: behaviours the release leaves to the implementation. A statement picks one arm of each; other arms are skipped |
| **The id map** | `catalogue/id-map.yaml`: every former case-core id, mapped to its workflows or the reason none exists |
| **The performance workloads** | `perf/`: workloads, host prescriptions and payloads |
| **The published schemas** | `schemas/`: JSON Schema for every artifact family, drift-tested |
| **The verification pack** | `verification-pack/arazzo/`: recorded HAR exchanges the evaluator must re-judge into their adjudicated results |
| **The specification oracle** | `specs/openehr/`: the released text, vendored verbatim, with the released XSD, JSON Schema and OpenAPI bundles |

## Coverage is machine-checked

A gate enumerates the wire surface from the released OpenAPI documents and
fails on any operation or response branch with neither a case nor a recorded
gap. Cases are never removed to go green. See
[the conformance method](methodology.md).

> [!NOTE]
> openEHR® is the registered trademark of the openEHR Foundation. Veredictum
> is an independent, community-driven instrument that names openEHR to say
> what it tests. It is not an official openEHR Foundation product, not the
> Foundation's CNF program, and not endorsed by or affiliated with the
> Foundation.

[ghcr]: https://github.com/Vernum-Projecten/Veredictum/pkgs/container/veredictum
