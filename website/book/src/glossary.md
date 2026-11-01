# Glossary

<!-- toc -->

Terms from ISO conformance testing (ICS, IXIT, SDoC) and the openEHR
specifications (RM, AQL, the component names).

## The parties and the claim

- **Party:** the vendor whose product is graded and who makes the claim. Its
  ICS and IXIT travel with its submission; this repository commits neither.
- **ICS (Implementation Conformance Statement):** the claim, `statement.json`:
  claimed capability folders and the arm of each option. It decides what a run
  selects.
- **ICS proforma:** the blank form an ICS fills in: the folders under
  `catalogue/cases/` and the options of `catalogue/capabilities.yaml`.
  ISO/IEC 9646-7 gives the support and supported-values columns to the
  supplier and every other cell to the test author, so the proforma is
  published here and the answers never are.
- **IXIT (Implementation eXtra Information for Testing):** the deployment facts
  that drive the claim, `ixit.json`: values for `declared` inputs and, for a
  measured run, the environment. From ISO/IEC 9646.
- **SDoC (Supplier's Declaration of Conformity):** the statement's
  self-declaration half, per ISO/IEC 17050: the supplier declares, the
  instrument checks.
- **SUT (System Under Test):** the running server a campaign drives, always a
  CDR over its own REST wire.
- **CDR (Clinical Data Repository):** a server storing and serving openEHR
  clinical data, such as EHRbase or FerroEHR.

## The catalogue and the run

- **Catalogue:** the machine-readable test schedule under `catalogue/`: Arazzo
  workflows over the released openEHR REST API OpenAPI documents, in capability
  folders, with options in `capabilities.yaml`. `validate` holds it to zero
  findings.
- **Case:** one spec-cited behaviour as one Arazzo workflow, so a red row names
  one defect.
- **Case id:** `<document>/<workflowId>`, such as
  `ehr/create_ehr-same_ehr_twice-with_id`. Stable.
- **Rows file:** the rows a data-driven workflow runs once each, beside its
  document.
- **Option:** a behaviour the release leaves to the implementation, with arms.
  A statement picks one arm; the others' documents are skipped.
- **Recording:** `recording.har`, every exchange of a run as HAR 1.2. A
  re-judgement answers from it and writes the same results.
- **Upstream report:** an `upstream-report` issue here recording a silence or
  defect in the released text, with a proposed patch. No case asserts the
  silent behaviour.

## The verdict

- **Row status:** `passed`, `failed`, `errored` or `skipped`. A row passes when
  every criterion holds.
- **Errored:** could not be judged (a transport fault, an unevaluable
  criterion). Never a server failure; a runner or catalogue defect until
  adjudicated.
- **Skipped:** not run, with a printed reason such as `not-claimed`,
  `option-not-selected` or `not-declared`
  ([all reasons](reading-results.md#a-skipped-row)).
- **Warning:** a failed row whose failing criteria all quote a SHOULD. Never
  counted against a capability.
- **Capability:** a folder under `catalogue/cases/`, the unit a statement
  claims.

## Measured performance

- **Workload:** a versioned document under `perf/workloads/`: corpus recipe,
  interactions with shares, AQL query classes. Labelled `custom` when it
  matches no pinned digest.
- **Open loop:** arrivals fire at planned instants regardless of the server,
  latency counts from the planned instant, so a stall shows as latency.
  Seeding and a reproduced harness's read walk are closed loop, and labelled
  so.
- **Scout:** the 60 s exponential ramp estimating the knee K̂: the rate of the
  first 2 s window that breaches the qualification.
- **Plateau:** one ladder rung, a fixed rate held for about 5,000 samples. It
  qualifies when every operation holds its latency budget, errors stay at or
  below 0.1 % and achieved throughput is at least 95 % of offered.
- **Knee:** the highest offered rate whose plateau and all lower ones qualify;
  a bracket up to the first failing plateau, or a lower bound when all qualify.
- **`judged_at`:** the highest of p99, p95, p90 and p50 with at least 10
  samples above it. With fewer, `not_resolvable`, printed `n/r`.
- **Population reading:** the knee divided by 13 to 75 peak API arrivals per
  second per 100,000 people, as a population band at the run's corpus scale
  (POC, S, L or R).
- **A/A floor:** the largest per-metric ratio between two sides of one target
  interleaved against itself. A two-server difference counts only above it.
- **Posture canary:** a black-box probe of the declared posture (signing,
  commit validation, authentication, compression, TLS) after seeding and after
  the last plateau; a contradiction refuses the run.
- **Host prescription:** a versioned machine description under `perf/hosts/`,
  read against the generator's measured fingerprint.

## The authorities

- **Oracle:** the released openEHR specification text under `specs/`. The only
  authority an expectation may cite, never a suspect.
- **CNF:** openEHR's conformance component. Its Platform Conformance Test
  Schedule names what to cover; the released components give the answer.
- **Spec components:** **RM** (Reference Model, the data structures), **AQL**
  (Archetype Query Language), **AM** (Archetype Model), **BASE** (foundation
  types and identifiers), **TERM** (terminology), **ITS** (Implementation
  Technology Specifications for REST, JSON and XML: the wire).
- **Attribution law:** every red row belongs to exactly one suspect (server,
  runner or catalogue), decided before anything changes by comparing
  spec-required, catalogue-expected and observed.
