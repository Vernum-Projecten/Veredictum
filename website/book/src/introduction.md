# Introduction

Veredictum is a conformance instrument for openEHR clinical data repositories.
You point it at a running CDR and it tells you, with citations, which parts of
the openEHR specification that server implements.

> **Status:** Veredictum is on 0.x and is not yet approved by the openEHR SEC.
> Version 1.0.0 will mark that approval.

It executes a machine-readable catalogue of 1649 spec-cited test cases against
the server's own wire, records every exchange, and computes verdicts as pure
functions over what it recorded. Functional conformance, measured performance,
step-load stress and the comparative-speed benchmark come from the one tool.
This repository publishes the instrument and no results about anybody's
product.

[console.veredictum.eu](https://console.veredictum.eu) is the hosted
conformance instrument. It serves the catalogue, and a run driven there against
a publicly reachable CDR shows the full results and verdicts for that server.
It runs functional conformance only; the measured instruments are run locally
with the command line. The instance holds no key and nothing durable. Running
the instrument yourself works against any deployment you can reach
([Installation](installation.md)).

The released openEHR specifications are the only authority the instrument
accepts. Every expectation in the catalogue names the section it comes from, so
it can be refuted by a better reading of the specification and by nothing else.

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

| | |
|---|---|
| **1649 spec-cited cases** | Arazzo workflows over the released openEHR REST API OpenAPI documents, in 203 Arazzo documents. One small isolated case per behaviour, so a red row names one defect. Every invalid shape is kept as its own refusal case, so a lenient server fails it |
| **14 capability folders** | EHR, EHR status, composition, content, contribution, directory, item tags, query, definition, demographic, admin, security, simplified formats, system. A statement claims folders |
| **The options** | The behaviours the release leaves to the implementation, each with the arms a statement picks one of |
| **The performance schedule** | The four measured-workload class cases, the journey catalogue and the payloads the measured instruments send |
| **The published schemas** | JSON Schema for every artifact family, emitted and drift-tested, so you can author against the format |
| **The specification oracle** | The released specification text, vendored verbatim, plus the released XSD, JSON Schema and OpenAPI bundles a citation resolves against |

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
> not endorsed by or affiliated with the Foundation.
