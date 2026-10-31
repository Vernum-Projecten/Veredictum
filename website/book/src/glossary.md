# Glossary

<!-- toc -->

The conformance vocabulary comes from two places: the ISO conformance-testing
tradition (ICS, IXIT, SDoC) and the openEHR specifications themselves (RM,
AQL, the component names). This page defines every term the instrument and
the console put in front of you.

## The parties and the claim

- **Party:** the vendor whose product is being graded and who makes the
  conformance claim. The two documents below are theirs to write and travel
  with the submission they describe; this repository commits neither.
- **ICS (Implementation Conformance Statement):** the party's claim document,
  `statement.json`. It declares which capability folders the product claims
  and which arm of each option it sits on, and it decides what a run
  selects.
- **ICS proforma:** the blank form an ICS fills in: the capability folders
  under `catalogue/cases/` and the options of `catalogue/capabilities.yaml`.
  ISO/IEC 9646-7 assigns every cell of it to the test specification's author
  except the support and supported-values columns, which belong to the
  supplier of the implementation. So the proforma is published here and the
  answers never are. The term comes from the ISO/IEC 9646 conformance-testing
  methodology.
- **IXIT (Implementation eXtra Information for Testing):** the deployment
  facts needed to drive the claim, `ixit.json`: the values a rows file's
  `declared` inputs read, and for a measured run the environment the numbers
  were produced on. Same ISO/IEC 9646 origin.
- **SDoC (Supplier's Declaration of Conformity):** the self-declaration half
  of the statement, in the sense of ISO/IEC 17050: the supplier declares,
  the instrument checks, and nobody grades their own homework.
- **SUT (System Under Test):** the running server a campaign drives, here
  always a CDR reached over its own REST wire.
- **CDR (Clinical Data Repository):** a server that stores and serves
  openEHR clinical data, such as EHRbase or FerroEHR.

## The catalogue and the run

- **Catalogue:** the machine-readable test schedule under `catalogue/`:
  Arazzo workflows over the released openEHR REST API OpenAPI documents,
  grouped in capability folders, with `capabilities.yaml` naming the options.
  `validate` holds it to zero findings before any server is composed.
- **Case:** one spec-cited behaviour, one Arazzo workflow. One behaviour per
  case is a design rule: a red row then names exactly one defect.
- **Case id:** the stable identifier of a case, `<document>/<workflowId>`,
  such as `ehr/create_ehr-same_ehr_twice-with_id`.
- **Rows file:** the data a data-driven workflow runs once per row of, beside
  its document.
- **Option:** a behaviour the released text leaves to the implementation,
  with its arms. A statement picks one arm of each option, and the documents
  of every other arm are skipped.
- **Recording:** `recording.har`, every exchange a run made as HAR 1.2. A
  re-judgement answers every request from it and writes the same results.
- **Upstream report:** an `upstream-report` issue in this repository that
  records a silence or a defect in the released text, with a proposed patch.
  No case asserts the silent behaviour.

## The verdict

- **Row status:** every workflow row ends `passed`, `failed`, `errored` or
  `skipped`. A row passes when every criterion holds.
- **Errored:** a row whose exchange could not be judged, such as a transport
  fault or a criterion that could not be evaluated. By the attribution law
  this is never a server failure; it is a defect in the runner or the
  catalogue until adjudicated.
- **Skipped:** a row the selection did not run, with its reason:
  `not-claimed` (a folder the statement does not claim),
  `option-not-selected` (an arm the statement did not pick) or `not-declared`
  (a value or principal the party did not supply). Skips are printed, never
  silent.
- **Warning:** a failed row whose failing criteria all quote a SHOULD. It is
  reported and never counts against a capability.
- **Capability:** a folder under `catalogue/cases/`, the unit a statement
  claims.

## Measured performance

- **Workload:** a versioned document under `perf/workloads/` declaring the
  traffic a performance run offers: a corpus recipe, interactions with their
  shares, and AQL query classes. A document that matches no pinned digest is
  labelled `custom`.
- **Open loop:** arrivals fire at their planned instants whatever the server
  does, and latency is measured from the planned instant, so a stall shows up
  as latency. Seeding and a reproduced harness's read walk are closed loop:
  each request waits for the previous answer, and the record labels them so.
- **Scout:** the 60 s exponential ramp that finds the knee estimate K̂, the
  rate of the first 2 s window that breaches the qualification.
- **Plateau:** one rung of the load ladder, a fixed offered rate held long
  enough for about 5,000 samples. It qualifies when every operation holds its
  latency budget, errors stay at or below 0.1 % and the achieved throughput
  is at least 95 % of the offered rate.
- **Knee:** the highest offered rate whose plateau and every lower one
  qualify, stated as a bracket up to the first failing plateau, or as a lower
  bound when every plateau qualifies.
- **`judged_at`:** the quantile an operation is judged at, the highest of
  p99, p95, p90 and p50 that has at least 10 samples above it. A quantile
  with fewer is `not_resolvable` and prints as `n/r`.
- **Population reading:** the knee divided by 13 to 75 peak API arrivals per
  second per 100,000 people, printed as the band of population a deployment
  of that shape could serve, at the run's corpus scale (POC, S, L or R).
- **A/A floor:** the largest ratio per metric between two sides of one
  target run interleaved against itself. A difference between two servers
  counts only above it.
- **Posture canary:** a black-box probe that checks the declared posture
  profile (signing, commit validation, authentication, compression, TLS)
  after seeding and after the last plateau, and refuses a run whose server
  contradicts it.
- **Host prescription:** a versioned description of the machine a record
  claims to have run on, under `perf/hosts/`, read against the generator's
  measured fingerprint.

## The authorities

- **Oracle:** the released openEHR specification text vendored under
  `specs/`. It is the only authority an expectation may cite, and it is
  never a suspect when a run goes red.
- **CNF:** openEHR's conformance specification component. Its Platform
  Conformance Test Schedule names which behaviours to cover; the released
  components say what the correct answer is.
- **Spec components:** **RM** (the Reference Model: the data structures),
  **AQL** (the Archetype Query Language), **AM** (the Archetype Model),
  **BASE** (foundation types and identifiers), **TERM** (terminology),
  **ITS** (the
  Implementation Technology Specifications for REST, JSON and XML: how the
  operations land on a wire).
- **Attribution law:** the discipline applied to every red row before
  anything changes. The failure belongs to exactly one of three suspects: the
  server, the runner, or the catalogue. Which one is decided by comparing
  spec-required against catalogue-expected against observed, with the cited
  text as the reference.
