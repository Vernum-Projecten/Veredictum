<!-- THE ARCHITECTURE RECORD, and it is living. Moved to this repository's
root at the 2026-08-26 split, from FerroEHR's `docs/conformance/cnf-design.md`,
where the instrument was built as a workspace member. It is the design record
the runner implements — in particular the §8.14 performance machinery: the
traffic emulation that walks a load ladder to the knee, the plateau limits
(p99 <= 1 s), the population-anchored reading of a knee (≈13 peak
arrivals/s per 100k population), and the workload derivation from
OECD/Eurostat/NHS statistics.

PERMANENT (owner rulings 2026-07-22 + 2026-07-23, carried across the split).
Never delete it, and never file it under a plans directory: it is not
delete-on-implementation working material. It is the owner's curated document
and it is still being worked on — adapt names and paths when the tree moves,
never reword the content. One owner-directed exception is recorded: the
2026-08-27 correction pass (below) re-verified the checkable claims and
rewrote what had stalled; the design substance was corrected where wrong.

Spec-facing citations in CODE still point at `specs/openehr/` per the citation
rules. This file is the design record, not a code-citable oracle.

WHAT THE SPLIT CHANGED, and nothing else: `cnf-runner` is `veredictum`; the
paths that were under `tools/cnf-runner/` map to this repository's trees —
after the #55 restructure the code is `app/veredictum/` (a virtual workspace
root; the console is `app/veredictum-console/`) and the data trees stay
root-mapped (`catalogue/`, `schemas/`, `verification-pack/`, with the
case-core `artifacts/` tree retired at the cut-over, #604); the party tree
that used to sit beside them is gone (#465), because a declaration's support
columns belong to the supplier of the implementation and the one filled-in
example that remains is a named test fixture under `fixtures/`;
`specs/openehr/` is `specs/openehr/`. References belonging to the
FerroEHR repository — its issue numbers, its committed conformance baselines —
are qualified as such, because they do not resolve here. FerroEHR named as a
PRODUCT or as a committing party is content rather than a path, and is left
exactly as written. -->

# openEHR conformance & certification — the CNF 2.0 design

*The design record of the CNF 2.0 framework this repository implements:
the machine-readable conformance schedule, the pure verdict pipeline, the
BASE and OPTIONS reading of a result, the certificate, and the measured
performance model. Every
claim was verified 2026-07-21 against the sources in the Appendix (source
register), through repeated independent audit rounds (openEHR spec
conformance, ISO, legal/regulatory, internal consistency, implementability),
and the checkable claims were re-verified 2026-08-27 against this repository
and its tracker. The reference runner is shipped: `veredictum`, published on
crates.io (0.1.0-alpha.4 at the re-verification), with signed releases, the
container image (carrying the web console, #6), the docs site, and a
catalogue of Arazzo workflows over the released ITS-REST operations (§15)
that passes every validate gate. A party's measured artifacts are emitted to the output
directory a run is given.*

---

## 1. Summary

The official CNF component defines the right concepts — Conformance Guide,
Platform Conformance Test Schedule, Profiles, Certificate — and is frozen:
last content amendment March 2022, Release 1.0.0 (planned December 2018)
never cut, the entire assessment layer `TBD`, zero AQL test cases, and one
vendor-specific Robot suite that no longer runs as the only executable
artifact (§3). Procurement names openEHR with nothing verifiable to require —
Catalonia's ~€8.5M CDR tender had to use latency SLAs as the conformance
proxy — and in Europe the EHDS regulation is making self-assessed, CE-marked,
automatically-tested conformity the norm for EHR systems (implementing acts
due March 2027; EHR-system obligations staged 2029/2031 by data category), a
frame openEHR is currently not in (§6.5).

## 2. The three pillars

CNF 2.0 keeps the 2021–2022 community design (§4) and fixes the operating
model that killed it:

1. **Govern and resource it so it cannot stall again** (§12): a chartered
   maintainer group under openEHR International (voted decisions, no
   single-vendor majority), openEHR International owning the repository, the
   hosted instrument with its signing key, and the trademark, recurring
   program funding, ≥2 competing vendors co-authoring
   the schema before ratification.
2. **A machine-readable Test Schedule as the single normative source**
   (§8): one versioned catalogue — in ISO terms the Abstract Test Suite —
   from which the spec prose is generated and against which any harness
   (Robot, Rust, Spock, Postman) proves itself; CI replaces the bottleneck
   maintainer. The same machine-readable philosophy openEHR already applies
   via BMM and OpenAPI.
3. **Certification defined with international vocabulary** (§6, §9), with a
   certificate that signs one run: the functional result reads as BASE and
   OPTIONS, chapter by chapter (§15.9), and the measured instruments report
   their own figures (§8.14). The whole is a conformity-assessment scheme
   per ISO/IEC 17000 — ISO/IEC 17050 supplier's declarations first, witnessed peer verification next,
   delegated ISO/IEC 17025-lab + 17065-certifier assessment (the only rung
   ISO/IEC 17067 governs) last — the architecture IHE and ONC already run
   and the shape EHDS Art 40 mandates.

Nothing conceptual is claimed as new: the four-artifact vocabulary, the
SM-anchors/ITS-executes split, tech profiles, and the global ID scheme are
the 2021–2022 community's work. The deltas are five: one-file-per-case data
with generated prose; CI enforcement of the derivation chain; computable
Statement/results schemas with mechanically computed verdicts; the
governance/resourcing charter; and the ISO/EHDS grounding. The working
implementation is this repository's published instrument (an Arazzo
catalogue over the released ITS-REST operations, both wire formats,
machine-computed verdicts; grown from FerroEHR's ECC draft of 394 cases) — and it
is explicitly not "the standard": the standard is community-owned,
vendor-neutral, and multi-harness by construction.

## 3. Evidence base — state of the official CNF component (2026-07-21)

### 3.1 The four books

Published at
[specifications.openehr.org/releases/CNF/development](https://specifications.openehr.org/releases/CNF/development);
sources in [openEHR/specifications-CNF](https://github.com/openEHR/specifications-CNF);
vendored snapshot `specs/openehr/CNF/` @ `33251d2a`.

| Book | Status | Last substantive amendment | State |
|---|---|---|---|
| Conformance Guide | DEVELOPMENT | 0.6.0, 08-Jan-2022 (`guide/master00-amendment_record.adoc`) | Methodology sound (SUT model, the specs→runnable-tests "square", API-vs-content test split). **Assessment layer all `TBD`**: `guide/master05-assessment.adoc` §Tooling, §Test Execution Report, §Conformance Statement, §Conformance Certification. "Platform Clients" scope: bare `TBD`. |
| Platform Conformance Test Schedule | DEVELOPMENT | 0.8.6, 24-Mar-2022 (`platform_test_schedule/master00-amendment_record.adoc`) | See chapter map below. Minimum RM pinned 1.0.2 (`master03-overview.adoc`), behind RM 1.1.0/1.2.0. |
| Platform Profiles | DEVELOPMENT | 2022 | CORE / STANDARD / OPTIONS capability matrix (`profiles/master03-profiles.adoc`). The instrument adopts none of these tiers, in the statement or on the certificate: its capabilities are the folders under `catalogue/cases/`, the behaviour the release leaves to the implementation is an option in `catalogue/capabilities.yaml`, and a result reads as BASE and OPTIONS, chapter by chapter (§15.9). |
| Conformance Certificate | DEVELOPMENT | 2021 | A **fictional worked example** ("BestEHR 2.4", "ACME EHR systems LLC", dated 2017; `certificate/master03-certificate.adoc`). No issuance procedure, assessor accreditation, validity period, or revocation anywhere. The book advertises BASIC-SEC/BASIC-PRIV ratings for which no defining test cases exist. |

### 3.2 Test Schedule chapter map

From `specs/openehr/CNF/docs/platform_test_schedule/` (`aaaa`/`bbbb`/`xx`/`TBD`
placeholders = stub):

| Chapter | Area | State |
|---|---|---|
| master04 | Definitions ADL 1.4 / ADL 2 | Fleshed for ADL 1.4; ~5 ADL2 mentions only |
| master05 | Definitions: stored queries | **Stub** (all `xx`) |
| master06–09 | EHR / COMPOSITION / CONTRIBUTION / DIRECTORY | **Fully fleshed** — the good core (~120 cases with Description/Pre/Post/Flow + data-set matrices) |
| master10 | Demographic | **Pure stub** (26 TBD markers) |
| master11 | Querying (AQL) | **Stub** — the flagship openEHR capability has zero official test cases |
| master12 | Admin | **Pure stub** |
| master13 | Messaging (EHR Extract / TDD) | **Pure stub**; duplicated `export_ehr()` heading |
| master14 | — | **Missing** (numbering jumps 13→15) |
| master15–16 | Content: COMPOSITION / ENTRY structures | Fleshed (decision tables) |
| master17.1–17.7 | Content: data types | Fleshed except 17.5 (time_specification, stub); 17.3 (quantities, 47 cases) is the exemplar |

Two ID families exist and are kept unchanged by this proposal: functional
`<SERVICE_COMPONENT>.<operation>-<case>` (e.g. `I_EHR_SERVICE.create_ehr-main`)
anchored to SM interface operations, and content `CONT-<TYPE>-<scenario>`
decision tables — the global ID scheme announced in 2022 as spanning "REST
API, content, everything"
([Discourse 2358](https://discourse.openehr.org/t/conformance-schedule-progress-data-types/2358)).

### 3.3 The executable layer

- `CNF/tests/platform/robot/` — 223 `.robot` files, imported wholesale from the
  EHRbase project: every header reads "This file is part of Project EHRbase"
  (© 2019 Vitasystems/HMS); `tests/Taskfile.yml` hard-codes
  `ehrbase/ehrbase:13.3` + `ehrbase/ehrbase-postgres:13.4` and Spring auth
  flags. It is an EHRbase dev harness, not a neutral instrument.
- [specifications-CNF PR #5](https://github.com/openEHR/specifications-CNF/pull/5)
  "Make the tests runnable" — open since **June 2023**, unmerged. The official
  suite does not run against an arbitrary SUT out of the box.
- `scripts/openehr_platform/*.txt` in the upstream CNF repository — 34 abstract
  pseudo-code scripts (© 2017), a third representation not wired to anything.
  Deliberately NOT vendored: nothing here reads them, and each one names
  CC-BY-SA 3.0 while linking the NoDerivatives licence, a contradiction
  reported upstream rather than carried.
- Robot coverage is asymmetric to the schedule: robots exist for stub chapters
  (Query, Admin) and are missing for others (Demographic, Messaging).

### 3.4 Vital signs

Jira SPECCNF: two visible issues (SPECCNF-1 open since 2017; SPECCNF-6 "in
progress" since October 2021, zero comments); Release-1.0.0 dated 2018-12-28,
never released. Repo: last content work 2022; 2024 = link fixes; May 2026 =
Antora toolchain migration only; issues #1/#2 date from 2017.

### 3.5 Authorship of the chapters this catalogue cites

§3.2 says which chapters are fleshed. This section says who wrote them,
because the coverage of the catalogue in §8 descends from that work.

`platform_test_schedule/master00-amendment_record.adoc` names **P Pazos**
first among the raisers of revision 0.8.0 (23 Nov 2021, "Rewrite main
schedule based on EhrBase", with W Wagner and T Beale), 0.8.5 (21 Feb 2022,
with T Beale) and 0.8.6 (24 Mar 2022, "Improved headings based on openEHR
Service Model", his alone). Those three revisions made master06–09 the
fleshed core §8 draws on, and
`guide/master00-amendment_record.adoc` names him with T Beale on the
Conformance Guide's initial writing. Recounted on 2026-08-29 against the
catalogue as it stood before the cut-over (#604): **127 of its 1146 case cores
cited master06, master07, master08 or master09** (22 / 31 / 40 / 34
respectively), and 349 cited some chapter of the Test Schedule.

Pablo Pazos wrote the original EHRbase conformance tests in 2019 at Hannover
Medical School. The Robot battery of §3.3 is that work: 179 of its 223 files
read "Copyright (c) 2019 Wladislaw Wagner (Vitasystems GmbH), Pablo Pazos
(Hannover Medical School)". His
[openEHR conformance verification framework](https://github.com/ppazos/openehr-conformance-verification)
is his own expanded formalization of it, presented at EHRCON23, and it
carries a conformance testing specification of its own.

Three positions this record treats as settled were in print in his work
before this document stated them: anchoring test cases on the Service Model
rather than on the REST ITS (§7, principle 2), the Conformance Statement as
the artifact a supplier publishes (§6.4), and the principle that a test
report is not a certificate (§6.1's attestation rungs). They appear in his
2019 openEHR SEC presentation and in the 2023 framework design
([Discourse 17238](https://discourse.openehr.org/t/17238), 2026-08-29).

Nothing from that framework is imported here: no test case, no data set, no
fixture. The Test Schedule citation is to its chapters as the structural
guide to which behaviours need covering, and every expectation resolves
against a released component (§7). Two facts bound any future reuse. The
framework repository carries no LICENSE file, so reuse of its material needs
its author's permission first. And its demographic tests target his own
demographic-API proposal rather than the demographic API openEHR has since
published, so they do not transfer to master10 unread.

## 4. History distilled — what carries forward, and why it stalled

The 2021–2022 community design era (Discourse threads 1616/1851/2239/2285/
2358/2373, board-funded 2021 — Appendix) settled the foundations this
framework keeps **wholesale**:

- the four-artifact vocabulary: Conformance **Schedule / Profile /
  Statement / Certificate**;
- **SM names the capabilities, an ITS executes the tests**;
- **technology profiles** parameterizing serialization/protocol;
- the **global test-case ID scheme** spanning API + content tests;
- the four-stage **certification maturity ladder**;
- profiles **CORE / STANDARD / OPTIONS** with the capability matrix.

It then stalled, for four operating-model causes this framework must answer
(§12 answers 1–3; §8's machine-readable schedule + CI answers 1 and 4):

1. single-person, spare-time ownership;
2. funding tied to one project, not the program;
3. a two-track scope split (narrow official CNF vs one company's broader
   framework) with no owner for the union;
4. single-harness lock-in — the abstract-spec/any-technology model was
   chosen but never realized; the only implementation stayed
   EHRbase-specific and its generalization (specifications-CNF PR #5,
   open since 2023) had no owner.

The 2017 conformance wiki page (T. Beale — Appendix) contributed five ideas
the 2021–22 era never carried forward, recovered into this design: the
maximal-coverage end-to-end template test and scenario/lifecycle suites
(§11.2–3), the Enterprise dimension — data portability, EHR
merge/split/move, cross-enterprise sync (§11.11) — the performance/
volumetric dimension made measurable (§8.14), and the Security & Privacy BASIC
rung (§8.15). Its functional levels 1/2/3+O were superseded by the Profiles
book's CORE/STANDARD/OPTIONS.

The 2017 spec review ([SPECCNF-1 comment 22500](https://openehr.atlassian.net/browse/SPECCNF-1?focusedCommentId=22500))
remains the oldest open requirements list; its asks are answered in the
design: computable Conformance Statements as the first artifact (§8.10),
certificate governance — who creates/grants/verifies (§9), scope discipline
via ISO/IEC 25010 with no manual testing (§6.3, §7),
no conceptual REST hard-coding (a case's steps call the released ITS-REST
operations by `operationId`, never a hand-written path, §15.2), and
precise archetype-validation conformance points (§8.9 pilot 5, §11).

## 5. Prior art — how other standards run conformance

| Program | Model | What to copy |
|---|---|---|
| **DICOM conformance statements** ([DICOM PS3.2](https://www.dicomstandard.org/current)) | Every product publishes a standardized conformance statement; procurement compares statements; no central certification. | The **statement as the legally load-bearing artifact**, with a normative template. CNF 2.0 upgrade: make it computable. |
| **OpenID Foundation certification** ([openid.net/certification](https://openid.net/certification/)) | **Self-certification**: vendor runs the official open-source suite, submits results + a signed legal attestation, pays a small fee, gets listed on the public certified page. Runs at scale since 2015. | The **cheapest credible rung**: official suite + published results + attestation + public registry. |
| **HL7 FHIR / ONC Inferno** ([inferno.healthit.gov](https://inferno.healthit.gov/), [framework docs](https://inferno-framework.github.io/docs/)) | Open-source test kits per implementation guide; the (g)(10) kit is an approved test method inside a regulatory certification program. Structure: policy (ASTP/ONC, 45 CFR 170) → open-source test method (Inferno) → **ISO/IEC 17025** labs (ONC-ATLs, NVLAP-accredited) → **ISO/IEC 17065** certifiers (ONC-ACBs) → accreditor (ANSI/ANAB), plus surveillance + the public CHPL product list. | **Test kits as maintained open-source products**; machine-readable expectations; and the five-layer separation: the standards body never tests or certifies its own conformity — it owns criteria and approves test methods. |
| **IHE Connectathons + Conformity Assessment Scheme** ([ihe.net/testing](https://www.ihe.net/testing/)) | Annual supervised peer-testing events (results published) plus a formal scheme **explicitly built on ISO/IEC 17025 + 17067**, with certification bodies under ISO/IEC 17065 evaluating accredited-lab results. | The **community verification event** rung (a conformance-thon at EHRCON fits openEHR's culture) and the canonical lab/certifier split for the eventual top rung. |
| **EHDS Article 40** ([Regulation (EU) 2025/327](https://eur-lex.europa.eu/eli/reg/2025/327/oj/eng)) | The Commission develops **open-source digital testing software**, operated as EU and national testing environments, for the harmonised EHR components; manufacturers must use these environments pre-market and file the results; positive results = presumption of conformity. Conformity is **manufacturer self-assessment** + EU declaration + CE marking + public registration — no notified bodies. | Regulatory confirmation of the whole shape: automated open-source suite + self-assessment + declaration + public registry is now *the law's own architecture* for EHR conformity in Europe. |
| **openEHR's own ISO 18308 Conformance Statement** ([PDF](https://specifications.openehr.org/releases/1.0.2/requirements/iso18308_conformance.pdf)) | A requirement-by-requirement statement of openEHR's conformance to ISO 18308, exceptions indexed. | **In-family precedent**: openEHR has already authored a requirement-indexed conformance statement; the computable Statement is its machine-readable evolution. |

Composite lesson: nobody starts with third-party certification — every
working program starts with an official runnable suite + a public registry,
then adds attestation, events, accreditation. The 2021 ladder was right; the
bottom rung was never built.

## 6. The international frame — ISO vocabulary, and the regulatory clock

CNF 2.0 should adopt the international conformity-assessment vocabulary
instead of inventing terms. Everything this proposal describes has a settled
ISO name, which buys procurement- and regulator-legibility at zero design
cost, and openEHR gains the right to say its program is *structured per
ISO/IEC 17067* rather than home-grown.

### 6.1 The conformity-assessment mapping (CASCO toolbox)

| CNF 2.0 concept | International term to adopt | Standard to cite |
|---|---|---|
| The machine-readable Test Schedule | **Abstract Test Suite (ATS)**; per-case **test purposes** | ISO/IEC 9646-1/-2 (ITU-T X.290/X.291) |
| A concrete runner (veredictum, Robot, Spock…) | **Executable Test Suite (ETS)** realized by a **Means of Testing** | ISO/IEC 9646-4/-5 |
| The computable Conformance Statement | **Implementation Conformance Statement (ICS)** from a normative **proforma**; legally a **first-party attestation / supplier's declaration of conformity** | ISO/IEC 9646-7; ISO/IEC 17050-1; ISO/IEC 17000 |
| Evidence linked to a statement | **Supporting documentation** (traceability, availability, retention) | ISO/IEC 17050-2 |
| Deployment parameters to run against a live SUT (base URL, auth, template-id policy…) | **IXIT** (Implementation eXtra Information for Testing) | ISO/IEC 9646-1 (ITU-T X.292) |
| "The Statement selects which schedule cases apply" | **ICS-driven test selection**; checking the Statement's internal legality = **static conformance review** | ISO/IEC 9646-1/-7 |
| The product / the deployed system | **IUT** / **SUT** | ISO/IEC 9646-1 |
| A run's outcome | **verdicts** (pass / fail / inconclusive) + the **conformance test report** | ISO/IEC 9646-1; report shape per ISO/IEC/IEEE 29119-3 |
| Published-statement and self-declaration rungs (§9, rungs 0 and 1) | **First-party attestation** (SDoC) | ISO/IEC 17000; 17050-1/-2 |
| Community verification rung | **Witnessed peer verification** — ISO defines no "second-party attestation"; genuinely second-party only when the witness is a purchaser/user | ISO/IEC 17000 (party definitions) |
| Accredited assessment rung | **Third-party attestation → certification** by an **ISO/IEC 17065** body using an **ISO/IEC 17025** lab | ISO/IEC 17065; 17025 |
| The program itself | A **conformity-assessment scheme** (ISO/IEC 17000 §3), openEHR International as **scheme owner**; only the third-party rung is an ISO/IEC 17067 product-certification scheme (Type 1a initially; **Type 5** — type testing + process assessment + surveillance — if ongoing certification ships) | ISO/IEC 17000; ISO/IEC 17067 (third-party rung only) |
| "Conformance" scope | **Functional suitability** (completeness + correctness, incl. the Security & Privacy behaviours §8.15) **plus performance efficiency** (§8.14) — nothing else | ISO/IEC 25010 (+25023 measures); software-product evaluation per ISO/IEC 25051 |

### 6.2 ISO/IEC 9646 — the 35-year-old blueprint for exactly this design

ISO/IEC 9646 standardized, in 1991, exactly this architecture: a supplier
fills in a published **ICS proforma**; the ICS **selects** which cases from
the **Abstract Test Suite** apply; the supplier provides the **IXIT**
(instance parameters to run the tests); runners realize the ATS as Executable
Test Suites; outcomes are pass/fail/inconclusive verdicts in a standardized
report. ETSI, Bluetooth SIG, and USB-IF still run on this vocabulary — the
machine-readable schedule is settled practice to adopt, not an invention to
evaluate.

### 6.3 Scope discipline via ISO/IEC 25010 (answering the 2017 review)

Conformance under CNF 2.0 attests exactly two ISO/IEC 25010 characteristics
— the two verdict machineries of §8:

- **Functional suitability** (completeness + correctness against the openEHR
  specifications) — the functional + content schedules (§8).
- **Performance efficiency** — the performance & volumetrics schedule
  (§8.14): a measured reading under a declared traffic mix on a recorded
  environment, written to a `perf-result` (the knee, its latency
  percentiles and the population band it could serve), re-derived from its
  histograms and never a pass or fail. NOTE: the current Conformance Guide
  excludes performance ("Non-functional conformance (performance, etc) is
  not addressed by this guide" — `guide/master03-overview.adoc`); CNF 2.0
  deliberately extends the scope here, as the 2017 schedule did, by
  measuring performance beside the conformance result, never inside it — an
  explicit SEC decision item. Measures follow ISO/IEC 25023.

The Security & Privacy family (§8.15) sits **inside** functional
suitability: its cases attest the functional *correctness of security
behaviours* (access rejected, audit written, demographic content separated)
through the assertion machinery — and so does **signature verification**:
where the Signing capability is claimed, the presence, verifiability, and
chain integrity of version signatures are wire-testable behaviours (§8.15).
What stays out of scope is ISO/IEC 25010's **security quality characteristic
itself** — attack resistance, penetration strength, cryptographic *strength*
(algorithm security, key management assurance) — which belongs to security
evaluation schemes, not conformance testing; likewise reliability and
maintainability,
referenced by their ISO names rather than redefined (the 2017 review's
point). ISO/IEC 25051 (conformity evaluation of ready-to-use software
products) and ISO/IEC/IEEE 29119-3 (test documentation shapes) are the
supporting citations for the evaluation procedure and report formats.

### 6.4 Legal weight of self-declaration (the phrasing to adopt)

Under ISO/IEC 17050-1 the supplier's declaration is made on the supplier's
sole responsibility; the standard states, verbatim: *"References to
assessments by first, second or third parties are not to be interpreted as
reducing the responsibility of the supplier in any way."* CNF 2.0's lower
rungs should carry exactly this framing in the Guide:

> *A published Conformance Statement is a first-party attestation
> (ISO/IEC 17000) in the form of a supplier's declaration of conformity
> (ISO/IEC 17050-1). openEHR International registers and publishes it but does
> not verify or endorse it; responsibility for its accuracy rests entirely
> with the declaring supplier.*

That sentence gives self-declaration recognized legal weight without implying
openEHR liability or endorsement — and it is the same legal shape as the EHDS
EU declaration of conformity.

### 6.5 The EHDS clock — honest positioning

Facts, from the verbatim OJ text of
[Regulation (EU) 2025/327](https://eur-lex.europa.eu/eli/reg/2025/327/oj/eng)
(retrieved via the Publications Office machine channel,
`publications.europa.eu/resource/celex/32025R0327` — Art 105 quoted):

- **Entry into force 25 March 2025** — Art 105: *"This Regulation shall
  enter into force on the twentieth day following that of its publication
  in the Official Journal"* (published 5 March 2025).
- **General application** — Art 105: *"This Regulation shall apply from
  26 March 2027."*
- **Staged EHR-system waves** — Art 105 third paragraph: Articles 3–15,
  23(2)–(6), 25–27 and 47–49 apply *"from 26 March 2029 to priority
  categories … points (a), (b) and (c) [patient summaries, ePrescriptions,
  eDispensations], **and to EHR systems intended by the manufacturer to
  process such categories of data**"*, and from 26 March 2031 for points
  (d)–(f) (imaging, lab results, discharge reports) — i.e. the
  harmonised-component obligations (Arts 25–27) reach category-(a)–(c) EHR
  systems already in **2029**.
- **Chapter III as a whole** — Art 105: *"Chapter III shall apply to EHR
  systems put into service in the Union referred to in Article 26(2) from
  26 March 2031."* Chapter IV (secondary use) applies from 26 March 2029.

Every in-scope EHR system must
embed two **harmonised software components** (European interoperability
component; European logging component; Art 25, Annex II), pass an
**open-source digital testing environment** (Art 40 — Commission-developed
open-source software, operated as EU and national environments), and ship
with a manufacturer **self-assessed EU declaration of conformity** —
Art 39(1) verbatim: *"The EU declaration of conformity … shall state that
the manufacturer of an EHR system has demonstrated that the essential
requirements laid down in Annex II have been fulfilled"* — **CE marking**
(Art 41) and public registration (Art 49). The testing obligation is
Art 40 ("European digital testing environment") verbatim: *"The Commission
shall develop a European digital testing environment … The Commission shall
make the software supporting the European digital testing environment
available as open-source"* (40(1)); *"Before placing EHR systems on the
market, manufacturers **shall use** the digital testing environments … The
elements in relation to which the results of the assessment are positive
shall be presumed to be in conformity with this Regulation"* (40(3)). Common
specifications + the EEHRxF exchange format arrive as implementing acts
adopted by **26 March 2027** (Arts 36, 15), applying on the
priority-category clock, pre-drafted by the Xt-EHR joint action —
whose deliverables are **HL7 FHIR logical models** and whose
conformity-assessment scheme (D8.2, May 2026) is **IHE/FHIR-based**. The
regulation itself names no standard at all.

Honest implications:

- **openEHR is not, and should not claim to be, the EHDS conformity route.**
  The exchange layer is FHIR/IHE territory. Overclaiming would be factually
  wrong and reputationally expensive.
- **The credible position**: an openEHR CDR is the system-of-record *behind*
  the EHDS interoperability component. CNF 2.0 certifies the CDR's openEHR
  conformance — and its roadmap includes verifying that a conformant CDR can
  drive the EEHRxF export faithfully (the openEHR→FHIR seam; EHRCON26 already
  programmes "Conformance Testing openEHR with FHIR TestScript"). An openEHR
  conformance result becomes *complementary evidence alongside* an EHDS DoC,
  never a competitor to it.
- **Architectural alignment is free**: EHDS Art 40 mandates precisely the
  shape CNF 2.0 proposes (automated, open-source, self-run testing +
  self-assessed declaration). Building CNF 2.0 in that shape
  makes openEHR conformance culturally and procedurally compatible with what
  every European vendor will be doing anyway from 2027.
- **Timing**: the program needs to exist — visibly, with a running suite
  and vendors publishing their own statements — before the March 2027 implementing acts and the 2029/2031
  application waves define "conformity" habits without openEHR in the room.

## 7. Design principles for CNF 2.0

1. **Machine-readable normative source, generated prose.** Like BMM → RM and
   OpenAPI → REST, the Schedule's normative form is data; the published spec
   pages are rendered from it. A test case that isn't in the catalogue doesn't
   exist; a catalogue entry without spec citations doesn't build — and the
   citation must be to a **RELEASED** spec component (RM / BASE / AM / QUERY /
   TERM / ITS-XML / ITS-REST docs text). The frozen CNF schedule, the OAS, and
   the Robot suites are STALLED structural guides (which behaviours to cover),
   never the correctness oracle — this framework earns its authority from the
   released components, which is precisely why it is the first *enforceable* CNF.
   **The honest state of the vendored oracle (audited 2026-08-27, #78):** only
   QUERY and ITS-REST are pinned to release tags today; RM, BASE, AM, TERM,
   ITS-XML, ITS-JSON and LANG are pinned to their `master` development heads
   (each tree's `PROVENANCE.md` records its ref). That gap between the claim
   and the pins is release-blocking for v0.1.0. Issue #78 makes the oracle
   generation explicit and selectable: a stable set of released tags as the
   DEFAULT, the development set as the deliberate second option, and the
   record naming which generation a verdict was spoken against. An instrument
   whose authority claim and whose oracle bytes disagree is the defect class
   it exists to catch in others, so the record states the gap plainly.
2. **ITS-REST binds the operation to the wire.** Every workflow names the
   released ITS-REST operation it drives, cites the section that assigns each
   expected value, and judges the response with the criteria of §15.6. The
   unreleased SM is no part of the oracle (#604).
3. **Harness independence by construction.** Catalogue + data sets + schemas +
   verdict rules are the contract; every runner is a downstream implementation
   verified against a shared reference pack (§8.12). No harness is normative.
4. **Verdicts are computed, never asserted.** Verdicts, and the BASE and
   OPTIONS reading of a result, derive mechanically from the results file. A certificate row a human typed is a
   defect.
5. **Honesty is structural.** Coverage is a mandate, not a pass rate — every
   wire behaviour the spec defines is its own case, and the bounds + honest
   boundaries are printed, never silent (§15.9, §11). N/A adjudications are
   cited; a spec silence or defect becomes an `upstream-report` issue or a
   declared option (§15.6), never a silent edit, and **every carried
   divergence is reported back upstream** (§13), never absorbed. Both directions are
   published when comparing products; a published result names its
   attestation rung (§9), so self-declaration is never mistaken for
   certification.
6. **Vendor neutrality is testable.** No vendor image names, endpoints, auth
   flows, or behavioural quirks in normative artifacts; fixtures carry spec
   citations, not `EhrBase ref:` markers; reference expectations are
   adjudicated against spec text, never against whichever SUT emitted them.
   (Red-run triage from the SUT side applies the same law: the vendored spec is
   the sole oracle and the application, runner, and catalogue are all suspects,
   none presumed correct — `.claude/rules/cnf-triage.md`.) The wire oracle is
   the ITS-REST **docs text**, never the (stalled) OAS — the OAS is `emit-rest`
   codegen input only, and where it and the docs text disagree, the text wins
   (owner ruling 2026-07-24).
   CI enforces what it can; the maintainer charter (§12) enforces the rest.
7. **Versioned like every other component.** Cases pin spec-version
   applicability ranges; a statement names the schedule release + tech profile
   it was earned against; within-major supersets follow openEHR's release
   strategy.
8. **Scope discipline.** Platform (CDR) profile first. Conformance =
   ISO/IEC 25010 functional suitability (including the Security & Privacy
   behaviours, §8.15) **plus performance efficiency** — the two verdict
   machineries (§6.3, §8.14); reliability, maintainability, and security
   *strength* (vs behaviour) stay out, declared in the Statement where
   relevant.
9. **Adopt international vocabulary** (§6.1) — ATS/ICS/IXIT/attestation
   levels/scheme-owner — instead of coining terms.

## 8. The party documents, the verdicts and the measured instruments

The conformance catalogue is Arazzo 1.1.0 workflows over the released ITS-REST
operations, and its design is §15: the layout, the extensions, the
condition profile, the gates, the results and their exports. This section
holds what sits around it: the documents a party supplies and receives
(§8.10), how a statement selects what runs and how verdicts are computed
(§8.11), how the instrument proves its own judgement (§8.12), measured
performance by traffic emulation (§8.14, which also holds what the universal
benchmark instrument used to describe) and the Security & Privacy schedule
(§8.15).

Two machineries share one discipline. **Conformance by assertion** is the
catalogue: each workflow row ends passed, failed, errored or skipped, every
red row carries its four-part explanation, and a capability folder's rows
count for that capability (§15.8, §15.12). **Measured performance** is the
workload under `perf/`: a declared traffic mix offered on a load ladder,
recorded with the environment it ran on, and re-judged from its histograms
(§8.14). Both are pure functions of recorded data.

The case-core catalogue that §8 used to describe (case cores, operation
bindings per SM operation, the outcome-kind taxonomy, the ambiguity register,
the corpus manifest and the machine gates over them) was retired at the
cut-over (#604). Every old case id resolves to the workflows that carry it
through `catalogue/id-map.yaml`. References elsewhere in this record to
§8.1–§8.9 and §8.13 name sections of that retired design, whose text stays in
the history before #604; the RESULT_SET equivalence rules moved to §15.11 and
the decision-table literal grammar to §15.7.

### 8.10 The statement, results and IXIT documents

Field-level contracts, each published as a JSON Schema under `schemas/`:

**`statement.json` — the ICS + SDoC** — one artifact carrying two standard
roles: the **ISO/IEC 9646 ICS** (the capability proforma that drives test
selection) *and* the **ISO/IEC 17050-1** supplier's-declaration content that
makes it a legal SDoC:

| Field | Semantics |
|---|---|
| `product` ∎ | name, **exact version/build**, vendor, unique product identifier |
| `schedule_release` ∎ | the release the claims are made against |
| `spec_versions` | declared RM/AQL/ITS-REST/TERM versions (the `applies` ranges of §15.12 read them) |
| `claims` ∎ | `capabilities`: the capability folders under `catalogue/cases/` the party claims; every workflow of a document in another folder is skipped `not-claimed` |
| `tech_profiles` | which formats are claimed (e.g. `[its-rest: [canonical-json, canonical-xml, wt-flat]]`) |
| `options` | one arm per option of `catalogue/capabilities.yaml`; the documents of every other arm are skipped `option-not-selected` |
| `non_functional` | declaration-only slots (security posture beyond what the catalogue tests: encryption configuration, pseudonymisation on export) — never verdict inputs |
| `evidence` | hash links to the results backing the claims |
| `attestation` | signatory name/role/date + the §6.4 responsibility sentence |

Version-binding rule: a statement pins the exact product version; a new
version needs a new statement or a signed "conformance-relevant surface
unchanged" attestation referencing the prior evidence.

**`results.json` of a catalogue run — the conformance test report** (9646
PCTR analogue), published as `schemas/arazzo-results.schema.json`: one row per
workflow row, each passed, failed, errored or skipped; a skipped row carries
its reason from the closed list of §15.8 and its citation, and a red row its
four-part explanation and the HAR entry it was judged on. Mapping to the
ISO/IEC 9646 verdicts: passed→pass, failed→fail, errored→**inconclusive**;
skipped is not a 9646 verdict, it records ICS-driven selection. The
recording (`recording.har`) beside it holds every exchange, so a re-judgement
reproduces the document byte for byte.

**`perf-result.json` of a measured run** (`schemas/perf-result.schema.json`):
the target's System OPTIONS identity, the environment block, the generator's
fingerprint and the host prescription it claims, the posture block, the
embedded workload and corpus, and per plateau the encoded HDR histograms, with
every derived number beside them (§8.14). A statement carries no performance
claim; the schema refuses a `performance` member by name.

**`ixit.json`** (9646 IXIT): the SUT **topology**, one or more **named
instances** (each: base URL, auth mode and the names of the variables its
credentials are read from) plus the **environment block** (hardware class,
cores, memory, storage class, deployment topology — mandatory for
performance runs, §8.14) and, for a measured run, the optional `containers`
block naming the server and database containers and the `database` the
server-side readings query. A rows file's `declared` inputs read their values
from it (§15.7). The shipped contract is `schemas/ixit.schema.json`.

### 8.11 ICS-driven selection and verdict computation

A pure function of (statement, IXIT, catalogue, recording):

1. **Static review** of the statement (`validate --statement`): the published
   schema, every claimed capability a capability folder, every selected arm
   an arm of an option in `capabilities.yaml`, one arm per option.
2. **Selection** (§15.8, §15.12): a document outside the claim is skipped
   `not-claimed`, one under an unselected arm `option-not-selected`, one
   outside its `applies` range `version-out-of-range`, and a row whose
   declared value is missing `not-declared`. Shared setups run where a claimed
   document depends on them.
3. **Execution** under the condition profile (§15.6): every exchange recorded
   as HAR 1.2, every criterion judged, and an evaluation error makes the row
   errored, never failed.
4. **Verdicts**: a row passes iff every criterion holds. Only failed counts
   against the server, and a row whose failing criteria are all `should` is
   a warning and never counts against a capability. `veredictum test` exits
   `1` when a row failed (not a warning) or errored.
5. **Measured records** (the second machinery, `veredictum perf-judge`):
   every derived number of a `perf-result` is re-derived from its embedded
   histograms, and a record that does not reproduce byte for byte is named
   by the JSON pointer of each difference. A measured record earns no class
   and enters no conformance verdict (§8.14).

Any two conformant implementations of this pipeline MUST compute identical
verdicts from the same inputs.

### 8.12 Runner verification

The instrument proves its own judgement with the verification pack under
`verification-pack/arazzo/`: recorded HAR exchanges for a passing, a failing,
an errored and an ETag-variant row, and the adjudicated results each must
re-judge into (`expected.json`). The integration suite re-judges every
recording and requires the adjudicated rows; a tampered adjudication must
fail the comparison, which is the pack's own falsifiability requirement. Each
recording is a slice of one or two workflows, so the suite plans every other
workflow of the document as skipped; `test --rejudge` re-judges the recording
of a whole run and refuses a slice, because a replay that does not line up
with its documents is refused rather than judged.
Live-SUT conformance stays a second leg: the catalogue is driven against at
least two independent live SUTs, and every disagreement is attributed under
`.claude/rules/cnf-triage.md` before anything changes.

### 8.14 Measured performance: traffic emulation

No openEHR specification governs this section. The CNF guide says so in as
many words: "Non-functional conformance (performance, etc) is not addressed
by this guide" (`master03-overview.adoc`). Everything below is our own
design, and every method claim names the source it rests on.

The instrument measures a CDR the way a clinical system loads it. It offers a
declared mix of operations, AQL shapes and payloads at Poisson arrival
rates, measures every latency from the instant the request was due, walks a
short ladder of offered rates up to the knee, and writes one record from which
every reported number is re-derived offline. It reports, per operation and
overall, p50, p95, p99, p99.9 and max, the achieved throughput, the error rate
by class, and how latency moves as the offered load rises. The full run after
seeding takes at most ten minutes, and about five when the server is fast.

**The owner's rulings of 2026-10-30 (#879, #880).** They are the frame of
this section, and each is restated where it bites:

1. `bench` folds into `perf`, and `bench`, `bench-compare` and `bench-packs`
   go.
2. `aql-probe` folds into perf's server-side readings, and the command goes.
3. POC, S, L and R stay as names of corpus scales (10k, 100k, 1M and 10M
   EHRs). They are no longer pass or fail classes.
4. The statement's `performance` block is removed.
5. A plateau qualifies when every operation holds p99 ≤ 1 s (≤ 5 s for
   population AQL), errors stay at or below 0.1 %, and the achieved
   throughput is at least 95 % of the offered rate. A workload may set
   stricter limits and never looser ones.
6. #595 stands: the console shows no measured results.
7. Server-side readings come from Docker stats and PostgreSQL only. There is
   no Prometheus scrape.
8. The full run after seeding takes at most 10 minutes, 5 where the evidence
   allows, and there is no second, longer profile.
9. The plateau hold is fixed: a function of the planned rate, never of the
   server's answers.
10. The reference mix is derived and provisional, for SEC ratification.
11. The reference host of FerroEHR#4188, an idle 8-CPU, 8 GB machine, is
    recorded as versioned data.

**Why the hour went.** A class run held one rate for an hour and earned a
letter. FerroEHR's committed record shows what that bought: class POC earned
at 2.04 arrivals/s for an hour, while the stress report on the same corpus
showed the server holding 512/s. An hour at a floor says the server is above
the floor and nothing about where it stops. The knee says that, and finding it
takes minutes, not an hour.

#### The workload document

A workload is a versioned YAML document under `perf/workloads/<id>.yaml`,
published as `schemas/perf-workload.schema.json`, emitted and drift-guarded
like every other schema here. It carries:

| Field | Content |
|---|---|
| `id`, `version`, `status` | the workload's name, an integer version, and `provisional` or `ratified` |
| `seed` | the root of every seeded draw: arrival gaps, think times, the EHR each interaction addresses, query parameters |
| `corpus` | the recipe the run seeds: scale by name (POC, S, L, R), compositions per EHR, and a template mix of payload keys with shares |
| `arrival` | `poisson` (the default) or `uniform`, the latter kept only as a labelled comparison with the old even grid |
| `sample_target` | samples per plateau, 5,000 by default, at least 1,000 |
| `qualification` | the limits of ruling 5, each at most as loose as the ruled default |
| `interactions` | a share each, summing to 100 %: either one operation, or a session of stages on one EHR with an exponential think time of at most 10 s mean |
| `queries` | the AQL query classes: text, parameters, scope, tier, transport mix, `requires` and `on_refusal` |

Every interaction share carries a `derivation` that names its source. The
operation vocabulary is closed, and each token cites the released ITS-REST
operation it is sent as (`specs/openehr/ITS-REST/specifications/operations/`),
so a workload names no wire the release does not define. A query class is
one `aql` operation per class, keyed `aql:<class>` in the record, and its
transport mix splits its arrivals over the four released query executions:
ad-hoc `POST /query/aql`, ad-hoc `GET /query/aql`, and the stored query over
`GET` and `POST /query/{qualified_query_name}`
(`specs/openehr/ITS-REST/specifications/query.openapi.yaml`). A query with an
EHR parameter is single-subject; one without is a population query, and the
release forbids an EHR scope on it ("It MUST NOT be supplied for 'population
queries' and similar multi-patient queries",
`specs/openehr/ITS-REST/specifications/docs/query/Request.md` §About the
ehr_id parameter). `requires` names the capability folders and option arms a
query needs beyond its operation's own folder, and `on_refusal` says whether a
refusal of an optional query counts as an error or is counted apart from
errors.

Loading the document is its gate. Each failure is a load error naming the file
and the field:

- shares, at any level, that do not sum to exactly 100 %;
- a session whose mean think time exceeds 10 s;
- an operation outside the vocabulary, a stage whose input does not fit its
  operation, or a query the document does not declare;
- a payload key missing from `perf/payloads.yaml`, or a template key without
  its `.example` skeleton;
- a query whose text does not parse with the released AQL grammar, whose
  parameters and `$` references disagree, or whose population scope binds the
  EHR;
- a `requires` token that names no capability folder under
  `catalogue/cases/` and no option arm of `catalogue/capabilities.yaml`;
- an expanded per-operation mix outside the 10:1 to 50:1 read band of step 4
  below;
- a qualification limit looser than the ruled default;
- a planned run longer than 10 minutes after seeding.

A statement drops what it does not claim. An interaction requires the
capability folder of every operation it sends, plus the `requires` of every
query it runs; when the statement claims less, the interaction leaves the mix,
the loader reports the drop and the renormalized shares, and the record
carries both. `perf --workload <file>` takes an evaluator's own document. The
embedded workloads are pinned by sha256 in the binary, and a document that
matches none is labelled `custom` in every record and every rendering.
`perf-workloads --out <dir>` writes the byte-deterministic manifest of the
embedded workloads with their digests, the expanded mix, the read:write ratio,
the planned run range, and every payload with its size; the workload page is
generated from it.

The reference workload is `clinical-mix` v2 (v1 #882; v2 publishes every derivation whole, #938), carried over from the 24
journeys of `perf/journeys.yaml` without their minute-scale offsets. Chart
review becomes a session; vitals, laboratory results, medication orders,
corrections, folders, tags, templates and the demographic side become
single-operation interactions; the query set joins the six `aql-mix` classes
with the front-end shapes (patient summary, observation trend, ward board,
problem list with a `TERMINOLOGY()` variant behind the `aql-terminology`
option arm, and cohort `COUNT`). No published per-operation trace of a
production openEHR CDR exists in the sources read, so every share rests on
derivation, says so, and is provisional by ruling 10.

#### Arrivals: open, partly open, and Poisson

A closed-loop generator issues the next request only after the previous
answer, so a slow server is offered less work and its stall disappears from
the samples. Schroeder, Wierman and Harchol-Balter show that open and closed
models of one system answer differently by orders of magnitude, and that real
web traffic sits between them: users arrive open, then run a short closed
session ("Open Versus Closed: A Cautionary Tale", NSDI 2006). The instrument
uses that partly-open model. Interactions arrive open on the planned
schedule; inside a session each stage waits for the previous answer and a
drawn think time, and the session addresses one EHR throughout.

Arrival gaps are exponential, so arrivals are Poisson. An even grid has no
arrival variance and understates queueing: by Kingman's heavy-traffic
approximation the mean wait scales with `(ca² + cs²) / 2`, the squared
coefficients of variation of inter-arrival and service time (Kingman, "The
single server queue in heavy traffic", Proc. Cambridge Phil. Soc. 57(4),
1961). A grid sets `ca² = 0` and hides half the term. The offered rate is
counted in operation arrivals per second, the unit of the population model
below.

Every latency is measured from the intended instant, never from when the
request left: a request due at `t` that leaves at `t + 40 ms` because the
generator or the connection pool was behind carries those 40 ms. This is
wrk2's correction for coordinated omission (Tene, wrk2 README,
<https://github.com/giltene/wrk2>; "How NOT to Measure Latency"). The
generator records its own lag (actual send minus intended) in a histogram,
and a plateau whose lag p99 exceeds 5 ms while the server keeps up is
`generator_bound` and never qualifies. HdrHistogram's post-hoc correction
assumes a fixed interval and is not used. k6 draws the same line between its
`constant-arrival-rate` and `ramping-arrival-rate` executors and its
closed-model ones, and counts the arrivals it could not start as
`dropped_iterations` (k6 documentation, executors); Gatling's open injection
profiles, Vegeta's constant-rate attack and Locust's user model are the
other points of comparison read.

#### The ten-minute run

Seeding comes first and is outside the clock: a closed-loop bulk load of the
corpus recipe, reported apart and labelled closed-loop. Then:

| Phase | Duration | What happens |
|---|---|---|
| settle | 15 s | `ANALYZE` or the declared `settle_maintenance` where containers are declared, then idle (FerroEHR#4318 traced A/A differences to an un-analyzed clone) |
| warm-up | 20 s | the start rate, discarded |
| scout | 60 s | an exponential ramp judged in 2 s windows; the first breaching window gives the knee estimate K̂ (k6 `ramping-arrival-rate`) |
| drain | ≤ 10 s | in-flight work completes |
| ladder | 4 × (5 s ramp-in + hold) | plateaus at 0.5, 0.75, 0.9 and 1.05 × K̂, plus one at 0.25 × K̂ only when the 0.5 plateau fails (Gatling's stairs injection) |
| closing | ≈ 10 s + the longest expected session span | the posture canaries, after the last plateau's sessions finish |

The hold of a plateau at rate `r` is `clamp(sample_target / r, 30 s, 90 s)`
(ruling 9), so two targets offered the same rates receive identical work. The
fixed phases sum to 115 s. Four plateaus of 35 to 95 s give 4.3 minutes when
every plateau runs above 167/s, 8.3 minutes when every plateau runs at or
below 56/s, and 9.8 minutes with the extra plateau; a knee near 200/s takes
about 4.7 minutes. The loader adds the longest expected session span of the
workload (stages less one, times the mean think time) to the closing phase and
refuses a workload whose upper bound exceeds 600 s, so a session-heavy
workload pays for its tail in the gate rather than at run time. There is no
quick profile and no repetition of the ladder (ruling 8).

Rates are operation arrivals per second; the generator draws interactions at
the rate divided by the mix's operations per interaction. The warm-up holds
8/s and the scout ramps from 8/s to 4,096/s, both settable per run. The scout
stops early once the exchanges in flight exceed twice the planned rate times
the largest latency budget, because by Little's law the mean latency is then
past twice the budget and the rest of the ramp would only bury the server.
The 0.5 plateau runs first and alone. When it fails, the extra plateau runs
next, before a higher plateau can leave its backlog behind, and the 0.75, 0.9
and 1.05 plateaus then run as one continuous profile, so sessions carry
across their boundaries. Between these legs the run waits at most the largest
latency budget for outstanding exchanges; an exchange still out by then is
over every budget already.

**The 5,000-sample argument.** The binomial order-statistic interval puts a
distribution-free 95 % confidence interval on a quantile from the ranks
`np ± 1.96 √(np(1 − p))` (Le Boudec, *Performance Evaluation of Computer and
Communication Systems*, 2010, §2.2). At `n = 5,000` and `p = 0.99` that is
ranks 4,936 to 4,964, about p98.7 to p99.3, with 50 samples above the
estimate. At 500 samples only 5 lie above, and the instrument declares a
quantile with fewer than 10 samples above it `not_resolvable` rather than
print a number that is mostly noise.

**Qualification (ruling 5).** A plateau qualifies when its achieved
throughput is at least 95 % of offered, every operation holds its budget at
the quantile it is judged at (p99 ≤ 1 s, ≤ 5 s for population AQL, or the
workload's stricter limit), the error share is at most 0.1 %, and it is not
`generator_bound`. A `429` refuses the record. A refusal of an optional query
whose `on_refusal` is `count_apart` is counted in its own column and never as
an error.

#### What is measured

Per plateau, per operation and overall: p50, p90, p95, p99, p99.9 and max,
each with its 95 % interval and resolvability; the achieved throughput; the
error share by class; the mean in flight, from Little's law `L = λW` (Little,
"A Proof for the Queuing Formula: L = λW", Operations Research 9(3), 1961);
and the half-to-half ratio of p50 and p95, which marks a plateau `unsteady`
outside 0.8 to 1.25. Overall numbers come from summed histograms, never from
an average of percentiles (HdrHistogram `add`, <https://hdrhistogram.org/>).
Each operation names `judged_at`, the highest quantile it resolves: an
operation with a 10 % share at 5,000 samples has 500 samples and is judged at
p95.

Per run: the knee, the highest offered rate whose plateau and every lower one
qualify, with its achieved throughput (Jain, *The Art of Computer Systems
Performance Analysis*, 1991, ch. 3, usable capacity), stated as a bracket up
to the first failing plateau or as a lower bound when 1.05 × K̂ qualifies; the
peak achieved throughput whatever the latency; and a fit of the Universal
Scalability Law `X(N) = γN / (1 + α(N − 1) + βN(N − 1))` of throughput against
mean in flight (Gunther, *Guerrilla Capacity Planning*, 2007;
<https://arxiv.org/abs/0808.1431>), recorded with α, β, N\*, the predicted
maximum and R² as an explanatory model and never a verdict.

Server-side readings sit beside each plateau as context that never bears on
qualification (ruling 7): CPU, RSS, block and network I/O per declared
container from the Docker Engine API, the database and per-schema sizes at the
plateau's start and end, the top statements by total time from
`pg_stat_statements`, and the PostgreSQL settings once per run. Each reading
is `present` or `absent` with its reason, and a sampler failure truncates its
series and never fails the run. The statement table is the attribution
`aql-probe` used to provide (ruling 2). The database the readings query is
named by the IXIT member `containers.database`; without it the database
readings are absent with that reason. The sampler is read-only: it creates no
extension, changes no setting and resets no statistic, so the statement table
is the difference of two `pg_stat_statements` snapshots taken at the hold's
start and end, and a server without the extension gets an absent reading
rather than an instrument that alters it.

#### What one pass gives up, and how the record says so

A single pass gives sampling intervals and nothing else. It does not measure:

- **between-run noise.** FerroEHR#4318 saw two GitHub-hosted runs of the same
  code read up to 156 % apart. The record carries `passes: 1` and
  `between_run_noise: unmeasured` unless an A/A floor from `perf-compare` is
  attached (Kalibera and Jones, "Rigorous Benchmarking in Reasonable Time",
  ISMM 2013, on why repetition at the right level matters more than count);
- **p99 for rare operations.** An operation's samples are its share of the
  plateau's; below 1,000 it is judged at a lower quantile and the record names
  that quantile in `judged_at`;
- **a bisected knee.** The ladder brackets the knee between two plateaus and
  says which; it never bisects further;
- **behaviour beyond ten minutes.** Memory growth, data growth and hours of
  operation are out of reach. Every record carries one scope sentence: a run
  of at most ten minutes measures steady-state capacity and says nothing about
  memory growth, data growth or behaviour over hours.

#### The record and its re-judgement

One family replaces the four measured families of before:
`schemas/perf-result.schema.json` (#884). Its `raw` part holds the target's
System OPTIONS identity, the instrument version, the IXIT environment block,
the generator fingerprint, the host prescription claimed, the posture block,
the embedded workload and its digest, the corpus recipe and its digest, the
seed, the scout's 2 s windows, and per plateau the planned rate and hold, one
encoded HDR histogram per hold half and operation, counters by error class,
the lag histogram, in-flight samples, and the first 16 failed exchanges per
operation. Its `derived` part holds every number above.
`veredictum perf-judge --result <file>` recomputes `derived` from `raw` and
exits `1` on any byte of difference, the measured counterpart of re-judging a
`recording.har` (§8.12). The USL fit uses pinned arithmetic, a fixed solver
and iteration count, so the re-judgement reproduces it. `show`,
`export --markdown`, `perf-assets` and `perf-compare` read the record.

#### Comparability, posture and the reference host

An absolute millisecond describes a system and the machine it ran on
together. `perf-compare` aligns two or more records into one table and names
every mismatch above it: workload digest, corpus digest, posture, host
prescription and fingerprint, network leg; it exits `1` when any applies.
`--ab <A> <B>` keeps both targets resident on one host, seeded from one recipe
and seed, gives each its warm-up and scout, builds the shared ladder from the
larger estimate so both sides are offered the same rates and holds, and
alternates the plateaus per rate in ABBA order. `--aa` runs the same
interleaving with one target on both sides and records the floor, the largest
A/A ratio per metric. A difference counts only when it exceeds the declared
tolerance and the attached floor, and the two sides' 95 % intervals do not
overlap. `--with-baselines` keeps the pinned EHRbase and FerroEHR
compositions, each its own published deployment recipe at an immutable tag,
and the relative index against them, interleaved the same way.

The posture profiles and their bracketing canaries came from the bench
instrument unchanged. A run declares one profile (`minimal`, the bare
spec-conformant surface with validation at `template`, because the release
puts it there: ITS-REST `specifications/responses/422.yaml`; or
`clinical-default`, the same with an audit trail). Each item is probed
black-box where an observable exists and labelled `verified` or
`declared-only`: version signing from the run's own committed versions (RM
`UML/classes/version.adoc` §Attributes, `signature`), commit validation from
the workload's invalid twin (`422` on
`specifications/operations/composition_create.yaml`), authentication from one
uncredentialed request, compression from one `Accept-Encoding` request, and TLS
from the base URL. Audit and tenancy have no released read surface and stay
`declared-only`. The canaries run after seeding and again after the last
plateau; a reading that contradicts the declaration, or two readings that
disagree, refuse the run.

The reference host is versioned data (ruling 11): `perf/hosts/<id>.yaml`
names the machine class, vCPU, memory, storage class, container ceilings and
network leg, the first being the idle 8-CPU, 8 GB host of FerroEHR#4188. A
record names the prescription it claims beside the fingerprint it measured,
and a record on an unmatched prescription says so. The neutral-host model
extends the same prescription to a load generator on hardware no vendor
controls, with the network leg (`same-host`, `same-zone`, `cross-region`) a
record field; a record naming no leg reads as `same-host`.

#### The population reading

The classes are gone (ruling 3); their derivation stays as the reading that
puts a knee in context. The **population-anchored utilization model** turns
a population into peak API arrivals per second:

1. **Clinical documents per capita per year ≈ 46**, from published activity
   rates times documents per event: doctor consultations 6/capita (OECD Health
   at a Glance 2023; Eurostat range 4.4–10.1) × 1 document; inpatient
   discharges 0.128/capita (OECD/Eurostat) × ~10 documents per stay; ED visits
   0.3/capita (OECD WP 83; 40-country median) × ~4; laboratory results
   15/capita (RCPath, flagged low-confidence); imaging 0.82/capita (NHS
   Diagnostic Imaging Dataset 2023/24) × 1; prescription items 21.8/capita
   (NHS BSA PCA 2024/25, an accredited statistic) × 1. Sanity: between the
   Danish and Estonian major-document exchange rates (~8–10/capita/yr) and
   Finland Kanta's all-inclusive ~130/capita/yr (>2M documents stored a day).
2. **Average writes/s** = population × 46 ÷ seconds per year, so 0.15/s per
   100k.
3. **Peak factor ×8** (band 6–10): weekday concentration ×1.4, a busy hour of
   ≈17 % of daily traffic ×~4 (the ITU-T E.500 busy-hour convention), and
   intra-hour burstiness ×1.2–1.5. ED arrival rates vary more than 3× over 24
   h, with ~88 % of arrivals in the 16 non-overnight hours.
4. **Read multiplier 10:1** (the 90/10 read-heavy OLTP convention,
   OLTP-Bench and YCSB) as the floor, with audit-log evidence (~597 EHR
   interactions per encounter, PMC10148376) bounding a read-heavy ceiling at
   ~50:1. The same band is the workload loader's read-band gate.
5. **Result: ≈13 peak API arrivals/s per 100k population**, band 13–75.

The reading divides the knee by that band and prints the population a
deployment of this shape could serve at its peak, as a band, beside the
corpus scale the run seeded (POC 10k, S 100k, L 1M, R 10M EHRs) and the
environment it ran on. The Catalonia 13M-patient openEHR CDR (117M
compositions) published the lesson that per-EHR data volume dominates query
cost, which is why the corpus recipe names compositions per EHR and why a
reading is only ever stated at its scale.

#### Where it lives

```text
perf/
  workloads/<id>.yaml     the workload documents (clinical-mix first)
  hosts/<id>.yaml         the host prescriptions
  payloads.yaml           key -> file, template_id, provenance
  payloads/               the CKM templates and their example skeletons, the
                          scale-corpus OPT, the Simplified-FLAT pair, the
                          demographic bodies
```

The schedule is kept apart from `catalogue/` because the catalogue holds
conformance workflows only (§15.10), and no released openEHR text assigns a
load or a threshold. A workload reads `catalogue/` for one thing, the names
its `requires` tokens must resolve to.

#### What the design removes

With the replacement landed (#891, #892), nothing offers the old surface:

- the commands `stress`, `stress-compare`, `aql-probe`, `bench`,
  `bench-compare` and `bench-packs`, and the measured half of `verdicts` with
  its performance badge;
- the `perf` flags `--class` and `--hours`, the `PT1H` window, the `PT5M`
  warm-up, the diurnal curve and the even arrival grid;
- the schemas `performance-case.schema.json`, `stress.schema.json`,
  `aql-probe.schema.json`, `bench-result.schema.json`,
  `bench-packs.schema.json`, and the measured-campaign
  `results.schema.json`;
- the trees and modules `perf/classes/`, `probe.rs`, the class path of
  `pipeline/measured.rs` with `ClassVerdict`, `class_verdict` and
  `rederive_verdict`, and the console's `ClassVerdict` token;
- the minute-scale offsets and periodic stages of `perf/journeys.yaml`, whose
  content the reference workload carries;
- the statement's `performance` block (ruling 4), which the statement schema
  then refuses with the field named.

The removal breaks FerroEHR's `scripts/conformance.sh` (`CONF_PERF_CLASS`,
`CONF_PERF_HOURS`), which its own tracker handles. `community-vitals` and
`aql-mix` become embedded workloads; the community harness's closed-loop
sweep stays inside `community-vitals` as a labelled closed-loop phase,
because that is the figure that compares with the community's published
number.

### 8.15 The Security & Privacy schedule

The 2017 schedule rated Security & Privacy as its own certificate dimension
(Functional | Enterprise | Performance | Security). Here it is the `security`
capability folder of the functional catalogue (§8's architecture: these are
testable functional behaviours, and this is explicitly NOT a security
evaluation scheme in the Common Criteria sense, §11.9). Its conformance points
come from the 2017 schedule's "BASIC" rung, each a family of ordinary
workflows; deeper points are future SEC work:

| Point | Conformance behaviour | Anchor |
|---|---|---|
| **EHR/demographic separation** | No demographic identifying content is reachable through EHR-side endpoints: `EHR_STATUS.subject` carries only opaque external refs / PARTY_SELF; EHR queries cannot join demographic records without demographic-service authorization. | 2017 schedule Security BASIC (lineage); RM ehr §EHR_STATUS.subject |
| **Authenticated access enforced** | Every platform route rejects unauthenticated requests (`401`) — a negative sweep over the route table. | ITS-REST overview §HTTP status codes |
| **Authorization separation** | Admin-family operations reject non-administrative principals (`403`); read-only principals cannot commit. The premise is DECLARED, never presumed: no released operation discloses which roles a principal holds (upstream report #757), so the run's administrative principal is the one `--admin-auth` presents. | ITS-REST overview §HTTP status codes (the code of a refusal) |
| **Audit accountability** | Every change-controlled commit carries `commit_audit` with committer identity and **server-set** `time_committed` (client value ignored); audit-event emission on writes where an audit log is supported (IHE ATNA-shaped). | RM common §change_control; ITS-REST overview §openehr-audit-details |
| **Anonymous EHRs** | An EHR is creatable and fully operable with no demographic identity attached. | Profiles book §Non-Functional (existing CORE capability) |
| **Version-signature integrity** *(applies when the Signing capability is claimed)* | Committed VERSIONs carry a verifiable `signature`: produced over the canonical form of the version data, verifiable against the declared key material, with the version lineage (`preceding_version_uid` chain) intact — the digital signing chain. Verification behaviour is conformance; algorithm strength and key management are not (§6.3). | RM common §change_control (`ORIGINAL_VERSION.signature`); Profiles book §Non-Functional (Signing) |

Signing thereby gets its concrete conformance point (the signature workflows
under `catalogue/cases/security/`, judged by the `signature` judge of §15.4). The
**statement-declared posture** (never wire verdicts): transport/at-rest
encryption configuration and id-pseudonymisation-on-export — the 2017
D-row's "encryption? id pseudonymisation?" aspects — declared in the
statement's `non_functional` security slots and revisited when the
Enterprise-D chapter (§11.11) makes export regression testable.

**What a result says**: the `security` folder's rows read like every other
folder's, BASE and OPTIONS chapter by chapter (§15.9). No certificate carries
a separate Security rating.

The catalogue's `security` capability folder holds these behaviours; a
party claims it like any other folder (§8.10).

## 9. Certification governance — the ladder as a conformity-assessment scheme

**Scheme owner: openEHR International** (the CIC that operationally runs the
specification program — the body the Conformance Guide already names as the
Platform Specifier). The program is a **conformity-assessment scheme** in the
ISO/IEC 17000 sense. Its self-declaration rungs are governed by
**ISO/IEC 17050** — not by 17067, which is by definition third-party product
certification; only the top rung is an ISO/IEC 17067 scheme: **Type 1a**
initially (type testing of a specific product version, no surveillance),
maturing to **Type 5** (type testing + process/QMS assessment + ongoing
surveillance of both) if surveillance is funded. Rungs are labelled by
attestation level so no rung can masquerade as a higher one:

| Rung | Name | ISO frame | Mechanism | Who grants |
|---|---|---|---|---|
| 0 | **Published statement** | First-party attestation | Vendor publishes `statement.json` + `results.json` itself; Veredictum publishes no results and keeps no registry (§15.10). **Preconditions for the claim**: the results come from a runner that has passed the §8.12 verification pack, and the statement passes static conformance review (`validate --statement`). The results name the runner that produced them. | Nobody — publication only |
| 1 | **Self-declared (signed SDoC)** | First-party attestation with signed SDoC (ISO/IEC 17050-1/-2) — "self-certification" in industry usage (OpenID); ISO reserves "certification" for third-party attestation, which is rung 3 alone | Rung 0 + a signed legal attestation of result accuracy by an authorized officer (+ modest fee funding the program). The §6.4 responsibility sentence appears on the certificate. | openEHR International (administrative + static review only) |
| 2 | **Community-verified** | Witnessed peer verification (genuinely second-party only when the witness is a procurer/user of the product) | Results reproduced at a supervised conformance-thon (EHRCON slot) or by a named community witness re-running the suite from the vendor's `ixit.json` against a vendor-provided deployment. The witness is named beside the re-run results. | Event organizers / named witnesses |
| 3 | **Certified** | Third-party attestation → certification | An **ISO/IEC 17025**-accredited lab runs the suite; an **ISO/IEC 17065**-accredited certification body reviews and certifies, with surveillance obligations. Both roles **delegated to independent accredited bodies** (the IHE/ONC model) — openEHR International remains scheme owner only, because a spec author certifying its own ecosystem fails 17065 impartiality. **This rung is not offered until surveillance is funded**; advertising it earlier would be dishonest. | Accredited certification bodies |

Cross-cutting rules:

- **What a certificate covers**: one run. It carries no tier or family
  rating; its payload names the run and the digests of its `results.json`
  and recording (§15.10), and that result reads as BASE and OPTIONS, chapter
  by chapter, from `catalogue/chapters.yaml` and `catalogue/capabilities.yaml`
  (§15.9). The measured instruments report the knee and its population
  reading per environment in a `perf-result` (§8.14). Every figure is
  computed from recorded data, never hand-asserted.
- **Validity & supersession**: a statement/certificate names the CNF schedule
  release + spec versions + tech profile + exact product version. It never
  expires by clock alone; it is **superseded** when a newer schedule release
  changes the cases it rests on or when the product version moves without a
  new statement/attestation (§8.10). A certificate's payload names the
  product version, the catalogue revision and the engine version (§15.10),
  so any reader judges currency from the certificate itself — answering
  Alkmaar's expiry question without inventing a revocation bureaucracy.
- **Disputes**: when a procurer or competitor contests a published result, the
  named path is a rung-2 witnessed re-run (same schedule release, same
  `ixit.json` shape), whose results stand beside the contested ones. Below
  rung 3, legal veracity remains a commercial-contract matter between vendor
  and procurer (ISO/IEC 17050 framing, §6.4) — stated plainly, as the 2021
  roadmap did.
- **Badges** are static files the instrument draws from one result
  (`veredictum badge`), and the vendor publishes them from its own run, for
  example to a branch of its own repository through the GitHub Action (owner
  rulings 2026-10-31, #919). Each badge says `self-run`, names the
  specification profile its result was judged under, and links to the run
  whose recorded results are its evidence. No badge is a certificate, and no
  server draws one. An "openEHR Conformant" mark is the scheme owner's to
  govern, as follows. **The mark is a licensed ordinary trademark, not a
  registered certification mark**: an EU certification mark legally asserts that the
  proprietor *certifies* (EUTMR (EU) 2017/1001 Art 83) — incompatible with
  self-declared rungs — and its owner may not carry on business involving
  the certified goods (Art 83(2)). The OpenID model applies instead:
  revocable, royalty-free trademark licence with prescribed per-rung usage
  statements, goodwill to openEHR International, mandatory removal on
  supersession or withdrawal; wording implying certification is licensed at
  rung 3 only. A mark on a rung-0/1 result signifies a self-declaration,
  never certification *by* openEHR International.
- **No registry, so no registry terms**: Veredictum lists no product and
  publishes no result (§15.10). The hosted instrument issues and keeps the
  certificate of a vendor's run and gives the vendor a copy; the certificate
  states what that one run recorded. The accuracy of a statement is the
  supplier's own declaration under ISO/IEC 17050-1, carrying the §6.4
  responsibility sentence, and the signatory's name and role appear only in
  the documents the supplier publishes.
- **Access**: schedule, schemas, corpus, and runners are public and free
  (Inferno/OpenID lesson: adoption dies behind paywalls). Rungs 1–3 may carry
  fees; the 2021 members-only idea applies to *services* (attestation
  processing, events, assessor program), never to the artifacts.

## 10. The procurement pack — usable within 12 months

The deliverable a tendering authority can use the moment rung 0 exists:

- **A normative RFP requirement template** (new short section of the Guide,
  answering the framework's "RFI/RFP guides: future" TODO):

  > *The offered product must demonstrate openEHR conformance to [CNF
  > schedule release ≥ R, capabilities C and option arms O, technology
  > profile including canonical JSON], evidenced by a published openEHR Conformance Statement
  > at attestation rung ≥ 1 for the product version offered, **or by equivalent
  > means of proof** (including a manufacturer's technical dossier or an
  > equivalent conformance report) demonstrating conformity to the same test
  > cases. The awarding authority will accept any evidence that objectively
  > establishes equivalent conformance, and reserves the right to require a
  > witnessed re-run (rung 2) of the published or submitted results prior to
  > acceptance.*

  Tender authors fill five parameters (release, capabilities, option arms,
  tech profile, rung).
  **The equivalence clause is not optional**: Directive 2014/24/EU Arts 42–44
  oblige contracting authorities to accept equivalent labels and equivalent
  means of proof (and a technical dossier where the operator demonstrably
  could not obtain the label in time) — a template naming one scheme's
  certificate exclusively is challengeable as discriminatory. The scheme's
  openness (public artifacts, open governance) satisfies the Art 43(1)
  label conditions; the equivalence duty applies regardless. This replaces
  the Catalonia-style behavioural-SLA workaround with a lawful,
  referenceable requirement.
- **Comparability**: statements and results are computable documents with
  published schemas, so an authority compares two vendors' submissions field
  by field, capability by capability and tech profile by tech profile. The
  instrument publishes no comparison itself (§15.10); the authority reads
  the documents the vendors supplied.
- **The dispute path** (§9) gives an authority a defined action when two green
  statements conflict with lived experience — the answer v1 lacked.
- **Version discipline** (§8.10) tells the authority exactly what a statement
  covers when the vendor ships the next release.

## 11. Gap-fill roadmap (content plan for the schedule itself)

Ordered by procurement value; each item is a bounded, assignable chapter task,
landing as Arazzo workflows in a capability folder under `catalogue/cases/`
(§15.3):

1. **Querying / AQL (master11 + master05)** — the flagship gap. The
   result-set equivalence rules are **written, normative, and cited in §15.11**
   (spec-grounded rules marked [spec]; the four points the specs are silent
   on carried as [legislated] proposed defaults) — U5's gate is SEC
   ratification of the [legislated] points, not de-novo design. Seed
   material: the ECC-era 25 QRY + 8 SQR + 4 AQT case designs, grown into
   this repository's query chapter (`catalogue/cases/query/`, each workflow
   carrying AQL 1.1 citations), and EHRbase's AQL conformance corpus ([ehrbase/conformance-testing-documentation](https://github.com/ehrbase/conformance-testing-documentation),
   SELECT/WHERE/ORDER BY/LIMIT/FROM/parameter suites).
2. **The maximal-coverage template round-trip** (the 2017 "template
   injection test"): one template exercising ALL RM types (every DV_* incl.
   generic derivations like DV_INTERVAL<DV_QUANTITY>) and all compositional
   hierarchy shapes, driven end-to-end — inject OPT → commit instance →
   export canonical JSON+XML → regression-compare. Pairs with master04's
   "maximal valid OPT" data set; one case family, enormous coverage per case.
3. **Scenario/lifecycle suites** (the 2017 "EHR API lifecycle test"):
   realistic multi-contribution journeys — admission (admin COMPOSITION) →
   persistent medication list → event vital signs → update both → retrieve
   all versions in both formats — encoded as ordinary workflows (§15.2); these
   catch cross-operation state defects the per-operation cases cannot.
4. **The performance & volumetrics chapter** (§8.14): normative workload
   definitions + the provisional reference mix and plateau limits now tabled in §8.14
   (derivation shown; SEC ratifies or amends), the synthesized scale corpora
   shared with §11.11, and the measurement schema. Ships after the functional pilot
   proves the artifact discipline; the schedule extension of the Guide's
   scope is flagged for SEC in §6.3.
5. **Content chapters refresh** — raise the RM floor statement (1.0.2 → an
   applicability ladder), fill 17.5 or formally adjudicate it out, fix the
   master14 numbering gap and the master13 duplicate heading.
6. **Demographic (master10)** — schedule cases exist in no form today; ECC's
   31 DEM cases + the ITS-REST Demographic API (DEVELOPMENT lifecycle) are the
   seed; profile placement stays OPTIONS.
7. **Admin (master12) + Messaging (master13)** — decide what is
   *wire-testable* (platform API) vs inherently off-wire (dump/load,
   archives); off-wire capabilities move to statement-declared, not
   schedule-tested — the honest boundary.
8. **N/A re-adjudication of donated material (hard gate)** — every donated
   case whose evidence or N/A justification points at FerroEHR internal
   tests is re-adjudicated to spec-text-only evidence **before** entering the
   normative catalogue. No exceptions; this is a scoped workstream, not an
   assumption.
9. **Security & privacy conformance points** — the Certificate book
   advertises BASIC-SEC/BASIC-PRIV with no defining cases; only Signing +
   Anonymous EHRs exist in the Profiles book. **The BASIC rung's points are
   now defined in §8.15** (EHR/demographic separation, authenticated access,
   authorization separation, audit accountability, anonymous EHRs) — this
   roadmap item authors its cases. Explicitly scoped small; not a security
   evaluation scheme.
10. **ADL2 cases (master04)** — OPTIONS-profile depth for the `am24`
   generation.
11. **The Enterprise capability family** (the 2017 schedule's D/M/X
   dimension, absent from every later draft): **D — data portability**
   (full-EHR dump/load in canonical form **between independent instances** —
   single-instance export/archive already exists as Admin capabilities
   (master12 `I_ADMIN_DUMP_LOAD`/`I_ADMIN_ARCHIVE`; Profiles "EHR
   Dump/Load"); the cross-instance portability regression is the new part —
   verified by lossless regression over a random query set, on synthesized
   corpora at declared scales — 1k/10k/100k/1M/10M EHRs, ~100 composition
   versions each, the recipes joining the §8.8 governed corpus);
   **M — EHR management** (merge/split/move of EHRs across instances);
   **X — cross-enterprise synchronisation** (asynchronous update merging —
   specifications-CNF issue #1 is the 2017 seed). Architecturally supported
   already: ixit declares named instances and flow steps carry `on:`
   selectors (§8.3, §8.10), so dual-instance cases are ordinary cases. Its
   own capability folder + a grounding decision; dump/load
   overlaps §11.7's off-wire boundary.
12. **The openEHR→EEHRxF seam (EHDS alignment, later)** — cases verifying that
   priority-category content in a conformant CDR renders faithfully to the
   EEHRxF FHIR models, once the March 2027 implementing acts fix them. Flag:
   this extends conformance scope beyond the platform API; it needs its own
   profile family and SEC decision.

## 12. Governance & resourcing — the section that answers the post-mortem

The 2021–22 effort had board sponsorship and still stalled; this section is
the difference between "nice idea, same risk" and "resourced program".

- **Ownership**: the normative repo (specifications-CNF), the hosted
  instrument and its signing key (§15.10), the "openEHR Conformant"
  wordmark, and the scheme rules are owned by
  **openEHR International** (the CIC already operating the specification
  program). No vendor owns any normative artifact.
- **The CNF maintainer group**: chartered under the SEC; 5–7 seats with **no
  single-vendor majority**; schema and scheme-rule changes by **recorded
  vote** (simple majority, SEC escalation path), never by PR-volume or any
  party's unilateral veto — including ours and including openEHR
  International's own staff. Charter published in the repo before the first
  normative merge.
- **Change control**: RFC process for schema/scheme changes; schedule releases
  cut like spec releases (versioned, changelogged); CI running the §15.9
  gates makes community PRs safe to accept, which is what actually
  de-bottlenecks a volunteer group.
- **IP**: donated cases/schemas/corpus items enter under the spec repo's
  licence with contributor licence hygiene (no retained vendor copyright in
  normative artifacts, no patent encumbrance).
- **Funding**: a recurring program line (hosting the instrument, CI, maintainer
  coordination, event slots) funded from openEHR International's program
  budget + rung-1 attestation fees — explicitly *not* from one vendor's
  project budget, because §4 stall-cause 2 is how that ends. Gap-fill chapters can be
  vendor-sponsored (bounded, reviewable tasks), but the *program* must not
  be — and a sponsoring vendor is never the sole adjudicator of its own
  sponsored cases: sponsored work is scoped to case authorship reviewed
  against spec text by non-sponsor maintainers.
- **Commitments in hand**: the pilot engineering is delivered — this
  repository's published instrument (§14.2). The upstream ask explicitly
  requests matching co-commitments — a second vendor's engineering time and
  2–3 maintainer volunteers — before the SEC agenda item, so the SEC decides
  on a resourced plan, not a hope.
- **Impartiality by structure**: openEHR International is scheme owner and
  rung-1 administrator only. It never tests, never certifies (rung 3 is delegated to
  accredited bodies; rung 1 is administrative). A spec author grading its own
  ecosystem is the 17065 impartiality failure the IHE/ONC split exists to
  avoid.

## 13. Upstream path

1. **Discourse** (Conformance category): the proposal condensed for
   discussion, collecting the §12 co-commitments before any SEC agenda item.
2. **SPECCNF-1/6 comment + a specifications-CNF issue** carrying the §8
   artifact set and the eight encoded pilots.
3. **SEC agenda item**: adopt-the-format decision, the maintainer-group
   charter, the AQL chapter blessed as the pilot.
4. **Execution**: the §14.1 PR series; two products publishing their own
   statements and results (FerroEHR volunteers); an EHRCON26 conformance slot; EHDS liaison per
   §6.5 (track the Art 36/15 implementing acts; revisit the EEHRxF-seam
   profile when they land in 2027).

**Outbound spec-defect reports (→ openEHR).** Building
and running the catalogue against the real spec surfaces where the spec itself
is silent, self-contradictory, or misaligned across RM / ITS / CNF. Every such
finding is written as
a concrete openEHR report on its `upstream-report`-labeled GitHub issue (a
plain summary, what the released spec says, what this implementation does,
the resolution sought) for a maintainer to file, recording the returned
channel key on the issue. This is a first-class deliverable of the
framework, not a side effect: an
instrument that exercises the whole spec is exactly the instrument that finds
the spec's own defects, and reporting them back is how the standard improves.

Success measures: SEC adopts the schedule + charter; ≥2 independent runners
pass the §8.12 verification pack; the AQL chapter ships with normative
equivalence rules; ≥3 products publishing their own statements and results; CNF Release 1.0.0
finally cut — before the March 2027 EHDS implementing acts.

## 14. Production implementation — the plan, and what shipped

Two tracks, both production-grade from day one — no throwaway prototype. The
in-repo track did not wait for upstream adoption: the reference runner
implements the §15 catalogue design as its production format, which is
simultaneously the proof the upstream proposal ships with. **Status
(re-verified 2026-08-27): the in-repo track (§14.2) is DELIVERED and
published; the upstream track (§14.1) has not started and remains the plan.**

### 14.1 Upstream: the specifications-CNF PR series

Sequenced, each PR independently reviewable and CI-green, each with an
acceptance gate:

| PR | Content | Acceptance gate |
|---|---|---|
| U1 | The catalogue format: the Arazzo profile with the `x-veredictum-*` extensions (§15.2, §15.4), the capability options (`catalogue/capabilities.yaml`), the condition profile (§15.6), and the §15.9 gates as the CI workflow; a defect or silence in the cited text is an `upstream-report` issue that the citation names in `report:` (§15.4) | The schemas validate this repository's catalogue at zero `validate` findings; CI runs on the repo |
| U2 | master06 (EHR) converted: all 21 cases as Arazzo workflows in the `ehr` capability folder over the released EHR operations, each case id mapped in `catalogue/id-map.yaml` | Generated prose semantically equivalent to the current chapter (human-reviewed diff); zero information loss against the AsciiDoc tables |
| U3 | master07/08/09 (COMPOSITION/CONTRIBUTION/DIRECTORY) converted the same way, one capability folder each | Same gate; the versioning cases round-trip |
| U4 | Content chapters (master15–17) conversion — decision tables as rows files under the literal grammar (§15.7) + generation recipes | Every existing table row preserved verbatim; grammar parses 100% of existing literals |
| U5 | **master11/AQL — the first new chapter**: the §15.11 equivalence rules as normative text + ~37 cases seeded from ECC QRY/SQR/AQT (25 QRY + 8 SQR + 4 AQT) + the EHRbase AQL corpus | SEC ratifies the §15.11 [legislated] defaults FIRST; every case spec-cited to AQL 1.1 |
| U6 | **Simplified-Formats chapter** (new): the `simplified_formats` capability folder, cases driven from the master04/05/06 spec-example blocks | Every case cites its simplified_formats section; OPTIONS-profile placement |
| U7 | statement/results/ixit schemas + verdict rules + the reference verdict implementation + the runner verification pack (recorded HAR exchanges + adjudications) | Two independent runners (ECC + the rescued Robot suite or another vendor's) compute identical verdicts on the pack |
| U8 | The hosted instrument handed to openEHR International (§15.10): certificate issuance under a key the foundation generates, attestation-rung labels, the dispute path of §9 | Two vendors hold certificates of their own runs |

The performance & volumetrics chapter (§8.14 + §11.4), Demographic
(master10), and Admin/Messaging (master12/13) follow as U9+ per the §11
roadmap once the pattern is proven on U2–U6.

### 14.2 This codebase: the reference runner, built from scratch — DELIVERED

Owner ruling: the conformance + benchmark tooling was **rebuilt from the
ground up** as one runner implementing the §8 architecture natively — not an
incremental adaptation of the ECC, and **not a 1:1 transcription of its
catalogue**: the catalogue was authored from the CNF 2.0 framework itself
(the official schedule cases per the §8.9 pilots, the new chapters, the
framework's own selection/format/option machinery). The ECC and its final
committed baseline (402 case×format executions · 384 passed · 18 N/A)
served as the **comparison reference** and retired FerroEHR-side; the
FerroEHR split's acceptance run drove the extracted instrument
byte-identical to the in-tree runner's record (FerroEHR#2789), and this
repository's catalogue is the baseline that now ratchets. The workstreams
below shipped, in this order, first inside FerroEHR and then here:

| WS | Workstream | Content | Done-gate |
|---|---|---|---|
| W1 | **Artifact schemas in Rust** | The typed model + validator (`app/veredictum/src/`) for the catalogue's documents, with JSON-Schema emission under `schemas/` so the same schemas ship upstream in U1. The catalogue checks run as `validate` (§15.9) and as `cargo nextest` guards. Done; the model is the Arazzo catalogue of §15 since #604. | Validator rejects every seeded-defect artifact fixture; schemas byte-identical to the U1 set |
| W2 | **Catalogue authoring + comparison** | The CNF 2.0 catalogue authored per the framework: official schedule cases first (the §8.9 pilot encodings generalized across master04–09/15–17), then the ECC-original designs that fill genuine gaps — each re-adjudicated to spec-text-only evidence (§11.8) before entry, keeping an `ecc-` namespace pending upstream adoption. Official CNF ids primary; `inventory/ecc-catalog.tsv` retires with the old harness. | **Comparison gate** (not reproduction): committed ECC↔CNF coverage map + comparison report — every difference from the old baseline enumerated and justified against the framework, verdict regressions on equivalent coverage explained, official-schedule coverage ≥ the old harness; cutover, old-harness retirement, and the new baseline follow a reviewed report |
| W3 | **Data-driven executor** | The engine executes the catalogue directly from its files: the Arazzo workflows (§15.2), their rows files (§15.7) and the condition profile (§15.6), with the AQL `result_set` assertions under the §15.11 equivalence rules (U5 ratifies the [legislated] defaults). Hand-written Rust remains only for generation recipes and genuinely non-mechanizable glue. Done; it executes the Arazzo catalogue since #604. | ≥90% of cases run through the interpreter; every exception listed in the report; the W2 comparison report stays green |
| W4 | **Statement / results / ixit emission** | `results.json` is emitted in the `schemas/arazzo-results.schema.json` schema (per row its status: passed, failed, errored or skipped; its inputs and generated values; the skip reason; and, for a failed or errored row, the step, what was checked, the citation, the expected and received values, whose side it is and the recorded exchange it was judged on; beside the rows the catalogue outline, the specification profile, the instrument status, the server's System OPTIONS statement and the static review); `statement.json` (ICS) + `ixit.json` (formalizing `SutDescriptor`) emitted per SUT; the Certificate/Statement/Comparison artifacts render from them; verdict computation moves to the shared pure function. | All of FerroEHR's `docs/conformance/**` artifacts regenerate from the new schemas; the honesty blocks survive; badges derive from the new results |
| W5 | **Simplified-formats deepening** | The §8.7 blueprint's gap categories 2–9 (node-id algorithm, level removal, the 43 suffix tables, `_`-attributes, `\|raw`, full ctx vocabulary, counters, STRUCTURED style) + deepened 1/10 — ~40 new SF cases, all spec-example-driven, all OPTIONS-profile. | Every master04/05/06 spec-example JSON block exercised; ECC baseline ratchets upward only |
| W6 | **Runner verification pack** | Author the U7 transcripts + adjudications; ECC self-verifies against them in CI; publish the pack so the Robot suite (and any vendor runner) can prove itself. | ECC passes both pack parts; the pack rejects a deliberately-broken runner build |
| W7 | **Performance schedule implementation** | The donated benchmark harness's workload generation and knee-finding ladder re-expressed as the §8.14 workloads under `perf/`, the load ladder to the knee, and the measured-run schemas; the host recorded beside each run. Done; the measured run is the ladder to the knee since #892. | A measured run re-judges from its recorded histograms (§8.14) |

Sequencing was W1 → W2 → {W3, W4} → {W5, W6, W7}, all delivered. The
standing gates carried over and grew: the guard tier, the full Rust tier,
`validate` at zero findings over 1689 workflows in 219 documents (recounted
2026-10-31), the fuzz lane, and the ratchet rule (the baseline only moves
upward).

**Shipped since the split, beyond the W-plan** (each on this repository's
tracker): the deliberate library API (`veredictum::pipeline`, one seam per
whole operation; #22); publication on crates.io with the signed release
pipeline, SBOMs and the multi-architecture container image (#5, #12); the
signed run record (a byte-deterministic digest manifest, a detached OpenPGP
signature, and the `verify-record` verb; #62; retired by #593 for the JWS
certificate of §15.10); the machine-readable
`run --progress` stream for drivers (#81); and the web console over the
published crate (#6; the ratified screen record is #61; shipped so far:
#63–#67 — the shell and design system, the read surfaces, the full run
wizard with the live screen, and the results and verdicts surfaces — plus
the catalogue's CNF profile language #87, the E2E harness #69 with its
real-CDR mode #99, and the pasted-statement scope #101; the container image
now carries the console). Exports are the `veredictum export` command, and a
certificate is checked with `veredictum verify` from the CLI alone (§15.10).
The tracker is the plan from here; this section stays as the record.

### 14.3 Runner technology — why the reference runner is Rust

No harness is normative (§8.7 discipline: any technology that passes the
§8.12 verification pack is a compliant runner — the Robot suite, Spock,
Postman, anything). The **reference** runner, however, is a deliberate
technology choice, and it is Rust:

- **There is no Robot-equivalent to inherit.** The Rust ecosystem has strong
  building blocks — [goose](https://github.com/tag1consulting/goose)
  (Locust-class load-testing framework),
  [cucumber-rs](https://github.com/cucumber-rs/cucumber) (native BDD
  runner), [Hurl](https://hurl.dev) (declarative plain-text HTTP testing) —
  but no keyword-driven acceptance framework like Robot. CNF 2.0 makes one
  unnecessary: the machine-readable schedule **is** the keyword layer, so
  the runner is a data-driven interpreter over the artifact files (W3) —
  precisely the thing worth building once, well.
- **Type safety enforces the framework's own laws.** The closed
  vocabularies this design lives by — outcome kinds, body/header selectors,
  reference grammar, capability keys — map to Rust enums and newtypes:
  an outcome kind outside the taxonomy or a malformed `${…}` reference is a
  *compile-time* error in the reference implementation, not a runtime
  surprise. The verdict machineries are pure functions over typed data —
  the property §8.11 demands ("any two conformant implementations MUST
  compute identical verdicts") is easiest to guarantee in a language where
  the types make illegal states unrepresentable.
- **One language serves both machineries.** The measurement machinery
  (§8.14) needs a workload generator that is never itself the bottleneck at
  class-R concurrency; Rust's async runtime handles goose-class load
  generation, and FerroEHR's `tools/benchmark` (knee ladder + sustained
  runs, published against two CDRs) was the donated draft. Assertion
  interpreter + load generator + verdict computation share one toolchain.
- **A single static binary ends the environment rot that killed the last
  suite.** The Robot suite's practical death was environmental
  (specifications-CNF PR #5, unmerged since 2023: Python dependencies,
  hard-coded vendor images). A vendor or procurer runs the reference runner
  as one downloaded binary against an `ixit.json` — no interpreter, no
  package manager, no environment to rot.
- **Memory safety + fearless concurrency** matter for hour-long sustained
  performance runs and parallel case execution against live clinical
  systems.

**The selected stack** (the ground-up rebuild's dependency decisions —
following this workspace's discipline: pinned workspace crates, never
hand-roll what a vetted crate provides, verify versions live at adoption):

| Concern | Selection | Rationale |
|---|---|---|
| Async runtime / HTTP | `tokio` + `reqwest` (rustls) | The runner is an interpreter + HTTP driver; both machineries share the client; already pinned + vetted in this workspace |
| Artifact model | `serde` + hand-written typed model (enums/newtypes for every closed vocabulary) | The §8 laws become compile-time properties; no framework needed or wanted |
| Canonical interchange | `serde_json` | statement/results/ixit are JSON; hash-linked artifacts |
| Schema validation | `jsonschema` | Validates the artifact families + emitted party artifacts against the published schemas |
| YAML authoring front-end | **`serde-saphyr`** (1.0.0-rc.1, 2026-07-18) | The only actively-maintained pure-Rust (`unsafe`-forbidden) serde YAML 1.2 front-end with **no RUSTSEC advisories** — `serde_yaml` is archived and both libyaml-fork successors inherit RUSTSEC-2023-0075; deserializes to `serde_json::Value` (the exact shape `jsonschema` validates) and ships a `Budget` cap on nesting/alias expansion, closing billion-laughs on untrusted test files. Pin the rc or its stable line. |
| Workload engine (measurement machinery) | **own tokio-native engine** — the knee-ladder + sustained-run code of the donated benchmark harness, carried into the rebuild (the `perf` and `stress` instruments under `app/veredictum/src/`) | Already built, published against two CDRs, and methodology-specific; [goose](https://github.com/tag1consulting/goose) evaluated and named as fallback — its Locust user-behaviour model does not fit the ladder |
| Latency statistics | **`hdrhistogram`** (evaluated at 7.6.0, 2026-07-18; pinned 7.5.4 in the tree) | The canonical Rust HdrHistogram port, no RUSTSEC advisories; the `serialization` feature emits the standard V2/compressed encoding so §8.14 thresholds are re-checkable from the results artifact, and `record_correct` provides the coordinated-omission correction the open-loop model requires. |
| CLI / errors / telemetry | `clap` / `thiserror` / `tracing` | Workspace standards |
| Test-definition DSLs | **[cucumber-rs](https://github.com/cucumber-rs/cucumber) and [Hurl](https://hurl.dev): evaluated and declined** | Each would introduce a second test-definition language beside the schedule — the exact three-representations drift CNF 2.0 exists to kill; the schedule is the only DSL |

The ECC (FerroEHR's `tools/conformance` + `tools/benchmark`) was the prior
art and the comparison reference for the rebuild (§14.2); W1–W7 delivered
the new runner as the first production implementation of the artifact set.
The Robot suite remains a first-class *compliant* runner via the
verification pack — rescuing it is inside the upstream proposal (§8.7, §13);
it is simply no longer the thing the framework's credibility depends on.

**The comparison gate, operationally** (the W2 done-gate): (a) a committed
ECC↔CNF **coverage map** relating old cases to the ground the new catalogue
covers; (b) a **comparison report** enumerating every difference from the
old 402-execution baseline — added/dropped/reshaped cases, execution-set
changes under the §8.7 format model, N/A rationale changes under the new
guard/option machinery — each justified against the framework, reviewed
line-by-line; (c) any verdict **regression on genuinely-equivalent
coverage** is explained (a real finding vs a deliberate case change — never
silently absorbed); (d) official-schedule coverage is ≥ the old harness's.
The framework changes what is tested and how it is counted, so the numbers
WILL differ — honesty lives in the enumeration, not in reproduction.
Cutover, old-harness retirement, and the establishment of the new baseline
(which then ratchets) follow a reviewed report.

What this buys strategically: when U1 reaches the SEC, the schemas arrive
with a published production runner already storing, validating, executing,
and reporting through them against real CDRs — the difference between
proposing a format and demonstrating one.

---

## 15. The catalogue design

The catalogue is Arazzo workflows over the released ITS-REST operations
(#590), and it is the one catalogue since the cut-over (#604).

### 15.1 The properties every part of this design holds

1. The released openEHR specifications are the only authority. Every
   expectation quotes the released sentence that assigns it.
2. A server under test never grades its own homework. Verdicts come from this
   instrument's own evaluator, never from a third-party runner.
3. Verdicts are pure functions of recorded data. Re-judging a recording gives
   the same bytes.
4. A red row is attributed before anything changes: the server, the runner or
   the catalogue (`.claude/rules/cnf-triage.md`).
5. Coverage only ratchets up. Every refusal the spec requires is an asserted
   negative case.
6. A defect, contradiction or silence in the released text is an
   `upstream-report` issue in this repository, never a private resolution,
   and no case asserts behaviour the released text does not assign.

### 15.2 Cases are Arazzo workflows over the released ITS-REST operations

The catalogue is written in the OpenAPI Initiative's Arazzo Specification
1.1.0. A case is an Arazzo workflow, and its steps call the operations of the
released openEHR ITS-REST OpenAPI documents by `operationId`.

A future second ITS gets its own Arazzo documents beside the REST ones.

ITS-REST Release-1.1.0 writes its wire contract in two places, both under
`specs/openehr/ITS-REST/specifications/`:

- **The docs text**, `docs/overview/*.md` and `docs/<api>/*.md`, states the
  rules that hold across operations: what each status code means, the
  headers, content negotiation. For example, `Requests_and_responses.md`
  assigns 422 to a request that is "well-formed but was unable to be
  followed due to semantic errors". The ITS-REST docs under `docs/` beside
  `specifications/` (Simplified Formats, SMART) are docs text too.
- **The decomposed OpenAPI**, `operations/*.yaml`, `responses/*.yaml` and
  `parameters/**`, states each operation's own responses, parameters and
  bodies.

An expectation cites the sentence that assigns it, in whichever of the two
says it. The docs text wins any conflict (hard rule 1), and an operation's
OAS response list is not exhaustive: OAS 3.0.3 §Responses Object says "The
documentation is not necessarily expected to cover all possible HTTP response
codes because they may not be known in advance." So a status the docs text
assigns is expected even when the operation's OAS fragment does not declare
it. An `operationId` is an address;
it is never the authority for an expectation.

### 15.3 The catalogue layout

```text
catalogue/
  capabilities.yaml                    options and their arms' documents, each document's version and format
                                       needs, per-folder workflow floors, the extension-only folders
  cases/<capability>/
    <topic>.arazzo.yaml                the workflows
    <workflowId>.rows.yaml             a data-driven workflow's rows, beside it
  datasets/<name>/                     data a server is seeded with (§15.11)
  payloads/                            request bodies, templates, archetypes, MANIFEST.yaml
  recipes/                             code that generates payloads
  keys/<kind>/                         signing material: the OpenPGP version-signing pair and the SMART test issuer
  sources/smart-platform.openapi.yaml  the SMART on openEHR routes under the Platform base URL
  sources/veredictum-probes.openapi.yaml   the deliberately wrong requests no released operation defines
schemas/                               statement, results, ixit, matrix, rows,
                                       dataset, certificate payload, extension schema
perf/                                  the performance workloads, host prescriptions and payloads (§8.14), a sibling
                                       of the catalogue because a measurement is not a conformance verdict
```

A case's files sit together: a capability's folder holds its Arazzo documents
and their rows files. "Payloads" is Arazzo's own word for a request body
(`requestBody.payload`).

Of that layout, `capabilities.yaml`, `cases/`, `keys/` and `sources/` exist
today, beside `id-map.yaml` (every former case-core id mapped to its workflows,
#603). Request bodies are inline in the workflows, and `datasets/`,
`payloads/` and `recipes/` arrive with the first case that needs one.

- **Sources.** Every document lists the released
  `specs/rest-oas/<group>-validation.openapi.yaml` bundles it calls, each with
  an explicit `type: openapi`. The
  `-validation` flavour inlines its schemas, which response validation needs.
  `contribution_create` and `contribution_get` exist in both `ehr` and
  `demographic`, so every step uses the qualified
  `$sourceDescriptions.<name>.<operationId>` form.
- **Extension routes.** Routes no openEHR OAS describes are declared in OAS
  documents of our own under `sources/`, each marked as a non-openEHR source
  by `info.x-veredictum-non-openehr: true`: `smart-platform.openapi.yaml`
  (the SMART routes, with `x-veredictum-base: platform`) and
  `veredictum-probes.openapi.yaml` (the probes). A capability folder verified
  only over marked sources is listed under `extension-only` in
  `capabilities.yaml`, and the `realization-scope` gate holds the list to the
  marks of the sources the folder's steps call (§15.9). No folder is
  extension-only today.
- **Capability by document.** A workflow belongs to the capability of the
  folder its document is in. `capabilities.yaml` holds the options a
  statement answers with the documents of each arm, what a document needs
  before it runs (`documents`), each folder's floor, and the extension-only
  folders.
- **Ids.** A case id is `<document>/<workflowId>`, for example
  `ehr/create_ehr-same_ehr_twice-with_id`. `workflowId` matches
  `[A-Za-z0-9_\-]+`, as Arazzo recommends.

### 15.4 The extensions

Arazzo has no citation object, no judges for openEHR result semantics, no way
to take part of a string, no way to name the credential or token a request is
sent with, and no way to tie a refusal to the request it refuses, so six
specification extensions are defined, with a published JSON Schema
(`schemas/arazzo-extensions.schema.json`): `x-veredictum-cite`,
`x-veredictum-check`, `x-veredictum-derive`, `x-veredictum-principal`,
`x-veredictum-scope` and `x-veredictum-control`. Any further extension needs a
recorded reason on #590 first.

**`x-veredictum-cite`**, on workflows, steps and criteria:

```yaml
x-veredictum-cite:
  source: specs/openehr/ITS-REST/specifications/responses/409_EHR_with_id.yaml
  section: description
  quote: "Can happen when the supplied `ehr_id` is already used by an existing EHR."
```

An optional `level: should` marks a quote that recommends rather than
requires, and `level: may` one that only permits (§15.6 rule 10); `must` is
the default. An optional `report: <n>`
names the `upstream-report` issue in this repository that records a defect or
silence in the quoted text; that issue, with its patch under `upstream/`, is
the whole record of the defect. This catalogue carries no ambiguity register:
a behaviour the release leaves unassigned is either an upstream report that no
workflow asserts, or an option in `capabilities.yaml` that a statement
answers (#617). `section` addresses the
element the quote must occur in. In a YAML or JSON source it
is a top-level key or a JSON Pointer (RFC 6901); in a Markdown or AsciiDoc
source it is a heading title. A pointer to a number, a boolean or `null`
names a leaf with no text, so its quote is that value written as JSON and is
matched by value: `section: /definitions/DV_PARAGRAPH/properties/items/minItems`
with `quote: "1"` cites the released ITS-JSON schema's floor on
DV_PARAGRAPH items. A criterion without its own citation inherits
its step's, and a step inherits its workflow's. `validate` refuses a criterion that resolves to no citation,
and refuses a citation whose `quote` does not occur verbatim (after whitespace
normalization) in the named file. A citation names a file under
`specs/openehr/`, `specs/rest-oas/`, `specs/its-json-schemas/` or
`specs/its-xml-schemas/`, never the SM. A file under
`specs/openehr/ITS-REST/specifications/` or `specs/rest-oas/` is an OAS
citation and ranks below the docs text (hard rule 1).

**`x-veredictum-check`**, on a criterion, for the three judgements no standard
criterion type can express. Arazzo requires every criterion to carry a
`condition`, so the criterion's condition states what must hold for the
judge to run (for example `$statusCode == 200`), and the criterion holds only
when both the condition and the judge hold:

| Judge | What it decides |
|---|---|
| `result_set` | AQL result equality under the RESULT_SET bag, order, cell and NULL rules (§15.11); a row cell may be a runtime expression (§15.11) |
| `equivalent` | Two RM documents are equal, ignoring a named set of server-assigned fields |
| `signature` | `ORIGINAL_VERSION.signature` facts (RM common §Digital Signature) |
| `schema` | The response conforms to the schema its operation declares in the `-validation` OAS for that status |
| `json_type` | The value at a runtime expression is one of the six JSON types of RFC 8259 §3 (§15.6 rule 11) |

Every other check is a standard Arazzo criterion. Field values use `jsonpath`
or `regex`; status codes and counts use `simple`. An XML document's `xpath`
criteria are refused until #637 (§15.6 rule 4).
Response-schema validation runs only where a case asks for it, through the
`schema` judge on a criterion that carries its own citation, like every other
expectation. It never runs on every response by default: an OAS schema can
contradict a higher oracle (the released `Ehr.yaml` types `ehr_status` as a
reference to an EHR_STATUS version, while RM `ehr.adoc` `Ehr_status_valid`
requires the VERSIONED_EHR_STATUS, #639), and an automatic check would put the
OAS above the RM. A case author applies the judge only where no higher text
says otherwise. A postcondition about server
state is a read-back step.

**`x-veredictum-derive`**, on a step, names new step outputs, each one
function of an `OBJECT_VERSION_ID` applied to one of the step's own outputs.
A create answers with a `version_uid`, while the update and the
VERSIONED_COMPOSITION reads address the container by its
`versioned_object_uid`, and no Arazzo runtime expression or selector can take
part of a string. BASE `object_version_id.adoc` defines the derivation:
"Globally unique identifier for one version of a versioned object; lexical
form: `object_id  '::' creating_system_id  '::' version_tree_id`", with the
functions `object_id()`, `creating_system_id()` and `version_tree_id()`.

```yaml
outputs:
  version_uid: $response.body#/uid
x-veredictum-derive:
  versioned_object_uid: { function: object_id, of: version_uid }
```

The three function names are the whole vocabulary. A derived output is read
like any other, as `$steps.<stepId>.outputs.<name>`. `validate` refuses an
unknown function, an `of` the step does not declare and a derived name that
collides with a declared output. A captured value that is not of the lexical
form makes the row errored, never failed, because the workflow's subject is
not the identifier's form; an `of` the response did not carry leaves the
derived output uncaptured, so a later step that reads it is errored too.

**`x-veredictum-principal`**, on a step, names the principal its request is
sent as when that is not the run's ordinary credential; `administrative` is
the one principal. ITS-REST `Requests_and_responses.md` §Authentication and
authorization "does not mandate a specific authentication scheme", so the
party supplies that credential, and a run whose party supplies none skips
every workflow with such a step as `not-declared`.

**`x-veredictum-scope`**, on a step and never beside `x-veredictum-principal`,
names the scope set of the access token the step presents: an OAuth 2.0 scope
value, or `$inputs.<name>` to read it per row. A row whose scope set the party
covers with no token is skipped as `not-declared`.

**`x-veredictum-control`**, on a workflow, names the control of a refusal
workflow (#662). A refusal workflow is one with a criterion
`$statusCode == 400` or `$statusCode == 422`. The released text gives an error
body no shape, so the evaluator cannot read which part of a request a server
refused, and a server that refuses a request for an unrelated reason (a
header it cannot read, say) would pass every refusal of a defective body. The
control is the workflow whose request is the same apart from the one defect,
named the way `dependsOn` names a workflow: a `workflowId` of the same
document, or `$sourceDescriptions.<name>.<workflowId>` of a `type: arazzo`
source.

```yaml
- workflowId: update-paragraph_no_items
  x-veredictum-control: update-other_details
```

A control is accepted in a run when the run holds a row of it and every row
passed or failed only `should` criteria, so the server took the request. A
refusal row whose control was not accepted in the same run is *errored*, with
a reason naming the control, whatever the refusal itself received. A refusal
that runs under either arm of an option names one control per arm as a list,
and is judged when no listed control that ran was refused and at least one
ran. A refusal with no twin in the catalogue yet names the open issue that
tracks one, `x-veredictum-control: { pending: <issue> }`, and is never judged:
every row of it is *errored*, naming the issue (`scripts/checks/pending-refs.sh`
holds each number to an open issue in CI). `validate` refuses a refusal
workflow without a control, a control on any other workflow, a `pending` that
is not a positive integer, and a control that names no workflow, the workflow
itself or another refusal workflow. No openEHR spec governs the extension — our own
design.

### 15.5 A case, worked: re-creating an existing EHR

Creating an EHR twice with the same `ehr_id` has two variants, and they call
different operations: a client-supplied id (`ehr_create_with_id`, a PUT) and a
server-assigned one (`ehr_create`, a POST). Each is its own workflow. This is
the client-supplied one:

```yaml
- workflowId: create_ehr-same_ehr_twice-with_id
  summary: An ehr_id already in use is refused on create
  description: >-
    Creates an EHR with a client-supplied ehr_id, then creates one with the
    same ehr_id again. The second create is refused with 409 Conflict, and
    the first EHR is still retrievable.
  x-veredictum-cite:
    source: specs/openehr/ITS-REST/specifications/responses/409_EHR_with_id.yaml
    section: description
    quote: "Can happen when the supplied `ehr_id` is already used by an existing EHR."
  inputs:
    type: object
    properties:
      ehr_id: { type: string, format: uuid }
  steps:
    - stepId: create
      description: Create the EHR with the client-supplied ehr_id.
      operationId: $sourceDescriptions.ehr.ehr_create_with_id
      parameters:
        - { name: ehr_id, in: path, value: $inputs.ehr_id }
      successCriteria:
        - condition: $statusCode == 201
          x-veredictum-cite:
            source: specs/openehr/ITS-REST/specifications/responses/201_EHR.yaml
            section: description
            quote: "`201 Created` is returned when the EHR has been successfully created,"
    - stepId: create-again
      description: Create an EHR with the same ehr_id again.
      operationId: $sourceDescriptions.ehr.ehr_create_with_id
      parameters:
        - { name: ehr_id, in: path, value: $inputs.ehr_id }
      successCriteria:
        - condition: $statusCode == 409
    - stepId: read-back
      description: The first EHR is still retrievable by its ehr_id.
      operationId: $sourceDescriptions.ehr.ehr_get_by_id
      parameters:
        - { name: ehr_id, in: path, value: $inputs.ehr_id }
      successCriteria:
        - condition: $statusCode == 200
          x-veredictum-cite:
            source: specs/openehr/ITS-REST/specifications/responses/200_EHR.yaml
            section: description
            quote: "`200 OK` is returned when the requested EHR resource is successfully retrieved."
        - context: $response.body#/ehr_id/value
          type: regex
          condition: '^{$inputs.ehr_id}$'
```

The `create-again` criterion inherits the workflow's 409 citation. The `ehr_id`
input has no value in any rows file, so the evaluator generates a fresh UUID
per run (§15.6), and the case needs no empty server.

### 15.6 The condition profile

Arazzo leaves several evaluation rules open (upstream issues #515, #517, #518,
#584). The profile pins each one, is published with the schemas, and is
enforced by `validate` and the evaluator. Every rule stays inside what Arazzo
1.1.0 permits.

1. **`simple` conditions** are used only for status codes, numbers and
   presence. Arazzo makes `simple` string comparison case-insensitive, which
   would let an identifier that differs only in case pass, so `validate`
   refuses a `simple` condition comparing strings. Exact string checks use
   `regex` or `jsonpath` (RFC 9535 comparison is exact).
2. **`regex`** uses the syntax that ECMA-262 (the dialect proposed upstream in
   #516) and the evaluator's engine share: no lookaround and no
   backreferences, which `validate` refuses. A value substituted into a
   pattern through `{$…}` is a literal: its metacharacters are escaped.
3. **`jsonpath`** is RFC 9535.
4. **`xpath`** criteria are refused until a vetted XPath 3.1 engine judges them
   (#637); a document that needs one is a gate finding, never a skipped check.
5. **Headers.** A step may send any header, including `Accept`, `Content-Type`
   and `Authorization`, whether or not the operation declares it. OAS 3.0.3
   tells a tool to ignore those three as declared parameters, and content
   negotiation cases depend on sending them.
6. **Missing values.** A criterion over a value that is absent from a
   readable response does not hold, so the row is *failed*. A body the
   evaluator cannot read, and an output a later step needs that an earlier
   step never captured, make the row *errored*, except when the step that
   should have produced the output failed only `level: should` criteria:
   then the row takes the warning outcome of the first of them, naming it
   (rule 10, #674), because the instrument did judge the exchange. `simple`
   compares against integers, booleans and `null`.
7. **Generated inputs.** An input of `format: uuid` with no supplied value is a
   fresh random UUID per row, recorded in the results so a replay reuses it.
8. **Evaluation errors.** Arazzo says an evaluation error evaluates to *fail*.
   The evaluator records that, and the verdict layer classifies the row as
   *errored*, never *failed*, because the instrument could not judge.
   `validate` catches every statically detectable expression error before a
   run.
9. **A rule that holds only when a header is sent.** A response header is a
   string or absent, and a `regex` criterion whose `context` is a response
   header reads an absent header as the empty string. A pattern that also
   matches the empty string therefore states "if sent, then": `^(W/".+")?$`
   holds when no `ETag` is sent and when a weak one is, while `^W/".+"$`
   also requires the header. Body values keep rule 6.
10. **Requirement level.** Every citation states the level of the sentence it
    quotes: `level: must` (MUST, SHALL, REQUIRED, MUST NOT, SHALL NOT, and a
    definition in the indicative), the default, `level: should` (SHOULD,
    RECOMMENDED, SHOULD NOT), or `level: may` (MAY, OPTIONAL). A row whose
    failing criteria are all `should` is *failed* and marked as a warning: it is
    reported with its explanation and never counts against a capability,
    because RFC 2119 lets an implementation ignore a SHOULD for a valid
    reason. A `may` criterion counts like a `must` one: RFC 2119 §5 obliges an
    implementation to interoperate with one that takes an option, so a
    client's permission binds the server, and a choice the release leaves to
    the server is an option in `capabilities.yaml` whose arm a statement
    selects. The upper-case keywords decide a quote that carries any. The
    openEHR text often writes a requirement in lower case, which RFC 2119
    cannot read, so a quote with no upper-case keyword is read by the same
    words in any case, with "can" as a permission: a citation that leaves the
    default `must` while its quote says "should", "can" or "may" and nothing
    that requires is refused until its author states the level, `should` or
    `may` when the sentence says so, or `must` when it requires in other words
    ("can always be written to", "will be considered").
11. **JSON types.** No `simple`, `regex` or RFC 9535 criterion asserts the
    JSON type of a value: a JSONPath filter selects nodes and does not test
    them, and `regex` and `simple` read scalars. The `json_type` judge of
    `x-veredictum-check` does, with `type` one of the six JSON types of
    RFC 8259 §3 (`object`, `array`, `string`, `number`, `boolean`, `null`),
    and `validate` refuses any other name. An absent value is of no type and
    fails, while a present `null` is of the type `null`. A judge was chosen
    over a function extension of RFC 9535, which Arazzo 1.1.0 does not define
    and no conformant JSONPath engine would evaluate, so the profile stays
    inside both standards.

### 15.7 Data-driven cases

A workflow with `inputs` runs once per row of its rows file,
`<workflowId>.rows.yaml` beside its document. The rows file is a small format
of our own with a published schema, because Arazzo defines inputs but not how a
set of them is supplied:

```yaml
# shape only; the values are illustrative
workflow: content/commit-dv_count_out_of_range
iteration: reset_per_row          # or single_pass, when the rows share state
literals: [magnitude]             # columns written in the decision-table literal grammar
recipe: { name: content_instance, rm_class: DV_COUNT, template_id: count_range, input: payload }
declared: { system_id: ixit.instances.sut.system_id }
aggregate:
  - distinct: ehr_id              # single_pass only: checked once after the last row
    x-veredictum-cite: { source: …, section: …, quote: … }
rows:
  - { magnitude: 0 }
  - { magnitude: 30 }
```

- **The expected outcome belongs to the workflow, not the row.** Rows that
  share an outcome share a workflow, and a variant with another outcome, or
  one that calls a different operation, is its own workflow (§15.5).
- **Literals and recipes.** Columns named in `literals` use the decision-table
  literal grammar: ranges `a..b`, lists `[x, y]`, unit-scoped ranges
  `[cm 5.0..10.0, m]`, terminology codes `openehr::122 (length)` /
  `local::at0005`, ordinal tuples `1|[local::at0005]`, quantity literals
  `100 mg`, with the violation categories `rm_schema`,
  `rm_invariant(<name>)`, `iso8601(<rule>)` and `constraint(<clause>)`
  (no openEHR spec governs this grammar — our own design). `recipe` builds the
  input or request body a row sends from them, deterministically.
- **Declared values.** `declared` binds an input to a value the party's IXIT
  or statement provides, such as the system id a server stamps. A row whose
  declared value is absent is skipped with the reason `not-declared`.
- **Cross-row checks.** `aggregate` states a property of the whole run, such
  as `distinct` over an output every row produces. It needs `single_pass`, is
  judged once after the last row, and carries its own citation.

### 15.8 The evaluator, and what a vendor sees on a red row

The evaluator is Veredictum's own Rust code. It parses Arazzo 1.1.0, resolves
sources, runs setup workflows through `dependsOn`, drives each step, records
every exchange as HAR 1.2, and judges each criterion under the condition
profile. A vendor may run the same documents through Redocly Respect or any
other Arazzo tool as a second opinion; that result is never a verdict.

Every row ends in one of four statuses: *passed*, *failed*, *errored* or
*skipped*. Only *failed* counts against the vendor. A *skipped* row was not
run, and always carries one reason from a closed list, with its citation:

| Reason | When |
|---|---|
| `not-claimed` | the statement does not claim the case's capability |
| `option-not-selected` | the statement selects the other branch of an option in `capabilities.yaml` |
| `version-out-of-range` | the case applies to spec versions the statement does not declare |
| `format-not-claimed` | the case needs a format outside the claimed tech profile |
| `extension-not-offered` | the case drives an extension route the vendor does not declare |
| `not-declared` | a value the case needs from the party's IXIT or statement is not declared |

Every red row in `results.json` and in the rendered report carries four parts,
derived from the case and the recording with nothing extra to author:

```text
FAILED   ehr/create_ehr-same_ehr_twice-with_id   step create-again
What     Create an EHR with the same ehr_id again.
Why      ITS-REST 1.1.0, responses/409_EHR_with_id.yaml:
         "Can happen when the supplied `ehr_id` is already used by an existing EHR."
Expected status 409
Received status 201
Exchange PUT /ehr/3f2a… → 201   (recording entry 2)
Whose    the server: a failed row counts against the claimed capability
```

The last line is one of: the server (*failed*); the instrument could not judge
(*errored*, which never counts against the vendor). Where the expectation rests
on released text with a known defect, the row also names the `upstream-report`
issue its citation carries in `report:`.

**Exports for a vendor's CI.** `results.json` is the record; three views are
derived from it, byte-deterministic like every other output, so a vendor can
run the instrument in their own pipeline and see the result where they work:

| View | Mapping | Shown by |
|---|---|---|
| JUnit XML | a workflow is a `testsuite`, a row a `testcase`; *failed* is `<failure>`, *errored* is `<error>`, *skipped* is `<skipped>` with its reason and citation | GitHub (through a JUnit report action), GitLab, Jenkins and most CI systems |
| CTRF 0.2.0 JSON | *passed*, *failed* and *skipped* map directly; *errored* is `other`; the case id, skip reason, citation and report link go in the test's `extra` object | `ctrf-io/github-test-reporter`: a GitHub job summary and pull request comment |
| Markdown summary | the rendered report, written to `$GITHUB_STEP_SUMMARY` when present | GitHub Actions job summary, with no third-party action |

In every view, the failure message is the four-part explanation above, so the
vendor reads why a row is red without opening `results.json`.

### 15.9 The gates

`validate` reports zero findings or the catalogue does not build:

- **Arazzo schema**: every document validates against the official 1.1 schema
  (`https://spec.openapis.org/arazzo/1.1/schema/2026-04-15`). Where the schema
  and the prose disagree (open upstream bugs #558, #559), the prose wins and the
  gate records the exception. The schema also admits only a string or a
  Selector Object as a Payload Replacement Object `value`, while Arazzo 1.1.0
  §Payload Replacement Object types the field `Any | {expression} | Selector
  Object` and says "The value can be a constant, a Runtime Expression, or
  Selector Objects to be evaluated and passed to the referenced operation or
  workflow." The gate follows the text and admits a literal of every JSON
  type, and the evaluator sends it with its JSON type intact (#663).
- **Source resolution**: every `operationId` resolves in its named source, and
  every parameter, after following the operation's `$ref`s, resolves at that
  operation or is a header the profile allows.
- **Citation**: every criterion resolves to a citation, and every quote occurs
  verbatim in its file.
- **Profile**: no `simple` string comparison, no regex lookaround or
  backreferences, and every expression parses.
- **Level**: a citation's `level` agrees with its quote. A quote carrying
  SHOULD or RECOMMENDED but no MUST, SHALL or REQUIRED needs `level: should`,
  and the reverse is refused; a quote whose only keyword is MAY or OPTIONAL is
  refused outright, because permission is never asserted.
- **Unknown keys**: a key that is neither an Arazzo field nor one of the six
  extensions, in the place §15.4 allows it, is refused. A parser that captures extensions through a catch-all
  map drops a misspelled key such as `succesCriteria` without an error, so the
  gate compares the parsed document with the raw one.
- **Rows**: every rows file validates and every row binds every required input.
- **Surface coverage**: the released operations, their response branches and
  the negotiation headers, enumerated from the decomposed ITS-REST OAS, each
  have a covering step or a recorded gap that names its tracker issue. The
  scope is declared, never inferred: a `surface.yaml` beside the workflows
  lists the families converted so far, each as an API document plus an OAS
  tag, so an unconverted family does not fail the build and a converted one
  cannot silently lose coverage. A gap whose element is already covered is
  refused as stale.
- **Identity and links**: unique case ids, resolving payload references,
  option tags that resolve to options in `capabilities.yaml`, and a control on
  every refusal workflow that resolves to another workflow that is not itself
  a refusal workflow (§15.4). A citation's
  `report:` is a positive issue number; checking that it names an
  `upstream-report` issue needs the tracker, so it runs in CI
  (`scripts/checks/report-refs.sh`, over the workflow documents and
  `capabilities.yaml`) rather than in the offline `validate`.
- **Document needs**: an `applies` range or a `formats` entry in
  `capabilities.yaml` names a document of the catalogue; a range does not
  start after its end, and its citation passes the citation and level gates;
  a document needing a format sends that format's media type in `Accept` or
  `Content-Type` on some step; and a capability document that sends
  `application/json`, itself or through a setup it names in `dependsOn`,
  states `canonical-json`.
- **Floors** (`arazzo-floor`): every capability folder has a workflow-count
  floor in `capabilities.yaml`, and a folder holding fewer workflows than its
  floor is refused, as are a folder without one and a floor naming no folder.
  The test suite re-derives every floor from the catalogue and fails when one
  differs, so a floor rises in the change that adds workflows and coverage
  only ratchets up.
- **Realization scope** (`arazzo-realization-scope`): a folder every one of
  whose steps calls an operation of a source marked
  `info.x-veredictum-non-openehr: true` is listed under `extension-only`, and
  a listed folder has no step calling an unmarked source. Either mismatch is
  refused, so a capability verified only over Veredictum's own routes never
  reads as openEHR coverage.
- **Structure** (`arazzo-structure`): `catalogue/chapters.yaml` places every
  capability folder in exactly one chapter and declares each folder's BASE
  sections. A document no option arm names is BASE and names one declared
  section with `info.x-veredictum-section`; a document an arm names is
  OPTION and names none, so BASE and OPTION are derived from
  `capabilities.yaml` and never declared twice. A section no document uses,
  an option with no theme and tier, a `spec-boundary` option whose citation
  names no `report`, a case of a formalism folder whose setups upload no
  template or templates of both formalisms, and a cited file in no vendored
  component with a manifest are refused. `catalogue/reports.yaml`, the
  snapshot of every cited upstream report's title and state, holds exactly
  the reports the citations name, and `scripts/checks/report-refs.sh` holds
  it to the tracker.
- **Maturity** (`arazzo-citation`): every cited file's release status is
  read from the vendored manifests; a citation into a document the release
  marks RETIRED is refused, as is one whose document states no known status.
  A DEVELOPMENT document inside a release is released text. A case citing
  TRIAL text carries `trial` in the results outline and is read with the
  options.
- **Claims**: `validate --statement` holds a statement to its published
  schema, which requires at least one claimed capability folder (#503), to the
  capability folders and to one arm per option. Claim completeness
  (`claim-completeness`) then refuses a claimed folder in which no document
  runs under the statement's own declarations: its option arms, its
  `spec_versions` against the `applies` ranges, and its `tech_profiles`
  against the `formats`. ISO/IEC 9646-7 makes the ICS the supplier's statement
  of what it implements, and a claim no workflow can run for verifies nothing.
  The arm format gate (`arm-format`) refuses a selected option arm one of
  whose documents needs a format the statement's `tech_profiles` do not
  claim, since the run would skip that document and verify nothing of the
  arm.

No openEHR spec governs the floors, the realization scope or the claim gates;
they are this instrument's own design.

### 15.10 Results, certificates and the hosted instrument (#592, #593)

- `results.json` is the conformance test report (§8.10). The statement, IXIT,
  selection and verdict computation are §8.10 and §8.11, and the verification
  pack is §8.12.
- **The decision (#592).** Veredictum publishes no results itself: no
  registry, no public board, and no record or certificate committed to the
  repository. A vendor certifies through the hosted instrument, which issues
  and keeps the certificate, and the vendor receives a copy. Verification is by
  public-key signature outside the console, with the CLI alone.
- A certificate is a JWS (RFC 7515) signed with Ed25519 (RFC 8037), chosen over
  an OpenPGP detached signature (the retired run record), an in-toto statement
  in a DSSE envelope and X.509, because any JOSE library verifies it with the
  public key alone and the engine already carried a JOSE implementation for the
  SMART lane. Its payload (`schemas/certificate.schema.json`) names the SUT
  with each identity field's source, the base URL, the catalogue revision as a
  digest of the catalogue tree, the engine version and approval status, the
  verdict summary, and the SHA-256 digests of `results.json` and the HAR
  recording. The header carries the `kid`.
- The verifying keys are a JWK Set (RFC 7517, `schemas/jwks.schema.json`)
  committed as `deploy/hosted/jwks.json`, compiled into the console and served
  at `https://console.veredictum.eu/.well-known/jwks.json`; a console test holds
  the served bytes to the committed ones. Two key members are the instrument's
  own: `expires_at` bounds when a key may sign, and `revoked_at` withdraws
  everything it signed. `veredictum verify` checks a certificate offline against
  the set and refuses a changed payload, an unknown `kid`, another algorithm, a
  revoked key and a key that expired before the certificate was issued, each as
  its own error variant. A rotation is a JWKS append.
- Issuance is off by default (trial mode): runs, results and verdicts work, no
  certificate is created, every page's status banner says so, and no private
  key is loaded. Only `VEREDICTUM_CERTIFICATES=issue` turns it on, so a typo
  keeps trial mode. Turning it on is an `.env` setting and a redeploy, once a
  release has shipped the set that publishes the key.
- **The consequences.** The signing key moves onto the hosted machine, which
  until now held no credential because a visitor can reach it: it is a file
  readable by the console user alone, generated on the box and never copied
  off it, and a stolen key signs anything until it is revoked. The machine
  holds durable state for the first time, so it keeps every certified run
  (certificate, results and recording, which a re-issue after a revocation
  reads), is backed up, and gives the vendor the same three files (#824).
  Issuance is switched on in v1.0.0 (#825). The foundation hand-over moves the
  machine, the catalogue governance and the signing, the last by a rotation to a
  key the foundation generates, so no private key changes hands.
  `deploy/hosted/README.md` carries the procedures.
- The console offers connect, scope, live run, results, verdicts and the
  certificate download. The measured instruments (`perf`, `perf-compare`)
  run from the CLI only and read the workloads under `perf/` (§8.14).

### 15.11 AQL conformance

An AQL expectation is a fact about the QUERY rules *and* about the data the
server holds: "this query returns these three rows" is only spec-derived when
the data is known exactly. So AQL cases run against a known dataset.

**Datasets.** `catalogue/datasets/<name>/` holds a named, versioned dataset:
the payloads to commit (compositions against a named template, EHRs, their
statuses) and a manifest describing what the data contains, in the terms the
expected results are derived from. A dataset is seeded once per run by a setup
workflow, and every query workflow that reads it names it in `dependsOn`, so a
run seeds it once and queries it many times (`single_pass`, §15.7).

**Query rows.** A query workflow executes one query and judges the result with
the `result_set` judge (§15.4). Its rows file lists the cases: the query text,
its parameters, the expected rows, and the match mode (`ordered`, `set`,
`count`, `contains`), under the RESULT_SET equivalence rules: bag semantics,
total ordering under ORDER BY, explicit LIMIT, cell equality and NULL
ordering (below). Each row cites the QUERY section it exercises, and its
expected rows are derived from the dataset manifest and that section, never
from a server's answer. Stored queries, query parameters and paging use the
same shape.

**RESULT_SET equivalence rules** (normative; each rule is either **[spec]**
— stated by the vendored specs, cited — or **[legislated]** — a proposed
default the specs are silent on, for SEC ratification):

1. **Comparison scope** — equivalence is over `rows` only; every `meta`
   field is excluded (**[spec]**: all `ResultSetMetadata` fields optional and
   "implementation dependent … useful for debugging" —
   `ITS-REST schemas/query/ResultSetMetadata.yaml`, `docs/query/Response.md`);
   `columns` are compared only when the case asserts them, since the array
   itself is optional (**[spec]**: `ResultSet.yaml` `required: [rows]`).
   Column identity: the `AS` alias, else `#<0-based index>` (**[spec]**:
   `ResultSetColumn.yaml`).
2. **Order** — `match: ordered` is legal only when the query carries an
   ORDER BY that totally orders the expected rows (**[spec]**: absent
   ORDER BY, "default ordering in results is undefined" — QUERY
   `master03-syntax.adoc` §ORDER BY; LIMIT determinism requires unique
   ordering — §LIMIT). Otherwise `match: set`, which despite its name is
   **bag (multiset)** equality — duplicate rows are significant, because AQL
   is bag-semantics unless `DISTINCT` is present (**[spec]**: §DISTINCT).
   `match: count` compares row count only; `match: contains` requires every
   expected row to appear (bag-wise) with extra rows permitted. ORDER BY
   semantics: ASC default, left→right lexicographic tie-break (**[spec]**:
   §ORDER BY).
3. **Cell equality** — an RM-object cell (carries `_type`) compares by
   canonical-JSON structural equality (**[spec]**: cells may be full RM
   objects — QUERY `master04-result_structure.adoc`,
   `ResultSetRow.yaml`); a scalar numeric cell compares by **numeric
   value**, not lexeme — `140` = `140.0` (**[legislated]**: no spec rule on
   projected-scalar number typing); a void cell is encoded as JSON `null`
   and equals only `null` (**[legislated]**: AQL names the value NULL, the
   wire encoding is unpinned).
4. **NULL ordering** — under ORDER BY, null cells sort **last** ascending,
   **first** descending (**[legislated]**: QUERY is silent on NULL sort
   position).
5. **Under-specified orderings are avoided, not legislated** — RM
   `DV_ORDERED` comparison is itself incompletely specified for partial
   dates/timezones/durations, so schedule cases MUST NOT order or
   discriminate on values whose comparison the RM leaves open
   (**[legislated]** avoidance rule).
6. **Counts are always determined** — cases pass `fetch`/`LIMIT` explicitly
   (the default fetch is implementation-defined) and never truncate
   without a total ordering (**[spec]**: §LIMIT + §ORDER BY).

**The parser check.** Every query in the catalogue is checked by `validate`
against the released grammar, `specs/openehr/QUERY/docs/AQL/grammar/AqlParser.g4`
and `AqlLexer.g4`, so a mistyped query can never become a false failure. The
parsing is done with `openehr-query`, a Rust implementation of that grammar
(Apache-2.0, like every dependency `deny.toml` admits). It is the one AQL parser this
instrument uses. Because FerroEHR, a server this instrument judges, is built on
the same parser, the grammar stays the authority over it: the parser check is
tested against the worked queries of the release this repository pins,
`specs/openehr/QUERY/docs/AQL_examples/` (which `openehr-query` vendors at a
different commit), and a query whose validity rests on a reading of the
grammar cites the grammar rule. When that test shows the parser disagreeing
with the released grammar, `openehr-query` is fixed; the instrument never
works around it. Queries that the grammar refuses are kept as their own
refusal cases.

**External query collections.** A query and dataset collection maintained
elsewhere enters as workflows: its fixtures become set-ups in a document under
`catalogue/cases/_setup/`, a provenance file in `catalogue/` names its
source, commit and licences, and its cases become workflows that name those set-ups
(the aql-conformance corpus: `_setup/aql_conformance.arazzo.yaml` and
`cases/query/aqlc_*.arazzo.yaml`). Nothing is imported blind: each query is
re-adjudicated against the QUERY specification before it becomes a workflow,
and a query whose expected result the released text does not determine stays
out, with its reason and its upstream report under `corpora` in
`catalogue/id-map.yaml`, which names every case of the collection by its own id.

**Identifiers in expected rows.** A `result_set` row may name a value the run
learns, such as the version uid a set-up was answered with: a cell that is a
string starting with `$` is a runtime expression, read from the run before the
rows are compared, and `validate` refuses one that does not parse. A query
that needs such a value takes it as a query parameter, or, in a GET `q`, as an
embedded `{$…}` expression, which the parser check reads as a placeholder.

### 15.12 Composing the catalogue

- **Shared setups.** A setup that several documents need (uploading a
  template, seeding a dataset) is written once, in a document under
  `catalogue/cases/_setup/`. A document that needs it lists that document as a
  source of `type: arazzo` and names the workflow in `dependsOn`, as Arazzo
  1.1.0 §Workflow Object prescribes: "If the workflow is defined in a separate
  Arazzo Document then the workflow MUST be defined in the
  `sourceDescriptions` and the `workflowId` MUST be specified using a Runtime
  Expression".
- **Options.** Behaviour the released text leaves to the implementation is an
  option in `capabilities.yaml`: its arms, each naming the documents that test
  it. The statement answers the option, and the documents of every other arm
  are skipped with `option-not-selected`. Surface coverage counts an element as
  covered when every arm covers it.
- **Capabilities.** The folder of a document names the capability its rows
  count for. `capabilities.yaml` may list a document under further
  capabilities it exercises; those count for coverage, never for a verdict.
- **Version ranges.** `capabilities.yaml` may give a document an `applies`
  range under `documents.<id>.applies`, keyed by the specification component
  a statement's `spec_versions` declares (`rm`, `base`, `am`, `aql`,
  `its_rest`, `term`). Each range has an inclusive `min`, `max` or both, in
  the `MAJOR.MINOR.PATCH` form the released components name their releases in
  (ITS-REST `docs/overview/Amendment_record.md`: Release-1.1.0), and an
  `x-veredictum-cite` quoting the released sentence that bounds it. When the
  statement declares a version of the component outside the range, every
  workflow of the document is skipped with `version-out-of-range`, and the
  skip's Why is that citation. A statement that declares no version of the
  component, and a run without a statement, run the document. A rule that
  holds only for some versions is its own workflow in a document with that
  range. No document carries a range today: the catalogue cites one release
  of each component.
- **Formats.** `documents.<id>.formats` lists the wire formats a server must
  serve or process for the document, in the statement's `tech_profiles`
  vocabulary (`canonical-json`, `canonical-xml`, `wt-flat`, `wt-structured`,
  `wt`). ITS-REST `docs/overview/Resources.md` §Data representation makes the
  choice the party's: "Services MUST support at least one of the openEHR
  **XML** or **JSON** canonical formats for resource representation", and the
  Simplified Formats SHOULD be supported. When the statement's its-rest
  profile does not list a format the document needs, every workflow of it is
  skipped with `format-not-claimed`, citing the statement schema's
  `tech_profiles` clause; a statement without an its-rest profile claims every
  format, as the results' `defaulted` profile records. The XML options
  (`ehr-xml-read` and its siblings) answer a different question: which answer
  the service gives when a client asks for XML. Only the documents of the arm
  that serves or accepts a format list it, so a party that does not claim
  `canonical-xml` selects the `-unsupported` or `-refused` arm, whose
  documents pin the 406 or 415 the released text requires and need no
  XML. Every capability document that sends `application/json` lists
  `canonical-json`, so a party claiming only canonical XML runs none of
  them, and a document whose workflows need different formats is split so
  the document-level list holds for each of its workflows: an XML refusal
  that needs no JSON setup sits in a document of its own. A document
  without `formats` is not gated by format.
- **Deliberately wrong requests.** A request the released OpenAPI defines no
  operation for, such as an unsupported method on a resource, is declared in
  `catalogue/sources/veredictum-probes.openapi.yaml`, a source marked as
  Veredictum's own. Its expectations still cite released openEHR text, such as
  the ITS-REST rule that a recognized but not allowed method "SHOULD be `405
  Method Not Allowed`". A rule only another standard states (an HTTP RFC) is
  asserted only where the openEHR text adopts it.
- **One root.** `veredictum validate --root catalogue` checks the whole
  catalogue, and no second catalogue format exists beside it.

## Appendix — source register

**openEHR:**
- Vendored CNF snapshot: `specs/openehr/CNF/` @ `33251d2a`
  (`PROVENANCE.md`); key files cited inline above.
- Published component: <https://specifications.openehr.org/releases/CNF/development>.
- Repo: <https://github.com/openEHR/specifications-CNF> — master last content
  2022; development = Antora migration (May 2026); PR #5 open since
  2023-06-11; issues #1/#2 from 2017.
- Jira: [SPECCNF-1](https://openehr.atlassian.net/browse/SPECCNF-1) (+ the
  [review, comment 22500](https://openehr.atlassian.net/browse/SPECCNF-1?focusedCommentId=22500)),
  [SPECCNF-6](https://openehr.atlassian.net/browse/SPECCNF-6); Release-1.0.0
  unreleased (target 2018-12-28).
- Wiki: [openEHR Conformance (2017)](https://openehr.atlassian.net/wiki/spaces/spec/pages/73367558/openEHR+Conformance),
  [Alkmaar SEC notes (2017)](https://openehr.atlassian.net/wiki/spaces/spec/pages/94181296/Conformance+Notes+-+SEC+meeting+Alkmaar+2017).
- Discourse: threads
  [1335](https://discourse.openehr.org/t/conformance-testing/1335),
  [1616](https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616),
  [1851](https://discourse.openehr.org/t/conformance-roadmap-2021/1851),
  [2239](https://discourse.openehr.org/t/conformance-framework-description/2239),
  [2285](https://discourse.openehr.org/t/openehr-conformance-verification-design-document/2285),
  [2358](https://discourse.openehr.org/t/conformance-schedule-progress-data-types/2358),
  [2373](https://discourse.openehr.org/t/conformance-testing-implementation-alternatives/2373),
  [17238](https://discourse.openehr.org/t/17238) (the 2026-08-29 authorship
  exchange behind §3.5).
- Conformance verification framework (P Pazos / CaboLabs), presented at
  EHRCON23: <https://github.com/ppazos/openehr-conformance-verification> —
  carries no LICENSE file; nothing from it is imported here (§3.5).
- Governance: <https://openehr.org/governance/> (openEHR Foundation + openEHR
  International CIC); HL7–openEHR joint statements (Amsterdam Jun 2025;
  [Dublin "Converge & Collaborate" May 2026](https://discourse.openehr.org/t/converge-collaborate-2026-joint-statement-from-hl7-international-and-openehr-international-press-release/16843));
  [EHRCON26 programme](https://openehr.org/ehrcon26/programme/).
- openEHR's own [ISO 18308 Conformance Statement](https://specifications.openehr.org/releases/1.0.2/requirements/iso18308_conformance.pdf).

**ISO / conformity assessment:**
- ISO/IEC 17000:2020 (vocabulary; attestation levels) —
  <https://www.iso.org/obp/ui/#iso:std:iso-iec:17000:ed-1:en>; CASCO overview
  <https://casco.iso.org/attestations-of-conformity.html>.
- ISO/IEC 17025:2017 (testing labs) — <https://www.iso.org/standard/66912.html>.
- ISO/IEC 17065:2012 (certification bodies) —
  <https://www.iso.org/obp/ui/#iso:std:iso-iec:17065:ed-1:v1:en>.
- ISO/IEC 17067:2013 (scheme types) — <https://www.iso.org/standard/55087.html>.
- ISO/IEC 17050-1:2004 / -2:2004 (supplier's declaration of conformity) —
  <https://www.iso.org/standard/29373.html>,
  <https://www.iso.org/standard/35516.html>.
- ISO/IEC 9646 (conformance testing methodology; PICS/ICS, IXIT, ATS/ETS,
  verdicts) — <https://www.iso.org/standard/17473.html> (part 1),
  overview <https://homes.cs.aau.dk/~kgl/TOV03/iso9646.pdf>.
- ISO/IEC 25010:2023 (quality model; functional suitability) —
  <https://www.iso.org/obp/ui/#iso:std:iso-iec:25010:ed-1:v1:en>;
  ISO/IEC 25051:2014 — <https://www.iso.org/standard/61579.html>;
  ISO/IEC/IEEE 29119-3:2021 — <https://www.iso.org/standard/79429.html>.
- ISO 18308:2011 — <https://www.iso.org/standard/52823.html>.

**Regulatory / programs:**
- Regulation (EU) 2025/327 (EHDS) — OJ text:
  <https://eur-lex.europa.eu/eli/reg/2025/327/oj/eng> (Arts 14–15, 25, 30,
  36–41, 49, 105; Annexes II–IV); verbatim text retrieved via the
  Publications Office machine channel
  <http://publications.europa.eu/resource/celex/32025R0327> (CELEX
  32025R0327; Arts 105/40/39 quoted in §6.5).
- Xt-EHR joint action — <https://www.xt-ehr.eu/> ;
  D8.2 EHR Conformity Assessment Scheme (May 2026)
  <https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D8.2.pdf> ;
  EEHRxF FHIR models <https://www.xt-ehr.eu/fhir/models/index.html>.
- ONC/ASTP Health IT Certification Program — 45 CFR Part 170 Subpart E
  <https://www.ecfr.gov/current/title-45/subtitle-A/subchapter-D/part-170/subpart-E>;
  program structure <https://www.healthit.gov/faq/a1-how-onc-health-it-certification-program-structured>;
  Inferno <https://inferno.healthit.gov/> +
  <https://inferno-framework.github.io/docs/>.
- IHE testing programs — <https://www.ihe.net/testing/>; IHE International
  Conformity Assessment Scheme Part 1 (ISO/IEC 17025 + 17067 basis)
  <https://www.ihe.net/wp-content/uploads/2018/08/IHE_International_Conformity_Assessment_Scheme_Part_1_Rev1-0_2014-06-25.pdf>.
- OpenID certification — <https://openid.net/certification/>.
- DICOM (PS3.2 conformance) — <https://www.dicomstandard.org/current>.

**Performance-model sources (§8.14 derivation):**
- OECD Health at a Glance 2023, consultations —
  <https://www.oecd.org/en/publications/2023/11/health-at-a-glance-2023_e04f8239/full-report/consultations-with-doctors_159193ce.html>;
  Eurostat healthcare activities (consultations; hospital discharges) —
  <https://ec.europa.eu/eurostat/statistics-explained/index.php?title=Healthcare_activities_statistics_-_consultations>,
  <https://ec.europa.eu/eurostat/statistics-explained/index.php?title=Hospital_discharges_and_length_of_stay_statistics>.
- NHS Diagnostic Imaging Dataset 2023/24 (47.2M exams) —
  <https://www.england.nhs.uk/statistics/wp-content/uploads/sites/2/2024/11/DID-Annual-Statistical-Release-2023-24.pdf>;
  NHS BSA Prescription Cost Analysis 2024/25 (1.26B items) —
  <https://www.nhsbsa.nhs.uk/statistical-collections/prescription-cost-analysis-england/prescription-cost-analysis-england-202425>;
  RCPath pathology volumes (flagged estimate) —
  <https://www.rcpath.org/discover-pathology/news/fact-sheets/pathology-facts-and-figures-.html>;
  OECD Emergency Care WP 83 —
  <https://www.oecd.org/content/dam/oecd/en/publications/reports/2015/08/emergency-care-services_g17a26ec/5jrts344crns-en.pdf>.
- EHR interaction intensity (~597 audit events/encounter) —
  <https://pmc.ncbi.nlm.nih.gov/articles/PMC10148376/>; OLTP read/write
  conventions — <https://www.cs.cmu.edu/~pavlo/papers/oltpbench-vldb.pdf>.
- Busy-hour conventions — Cisco VoIP traffic analysis
  <https://www.cisco.com/c/en/us/td/docs/ios/solutions_docs/voip_solutions/TA_ISD.html>;
  ITU-T E.500 <https://www.itu.int/rec/T-REC-E.500>; ED diurnal
  distribution <https://pmc.ncbi.nlm.nih.gov/articles/PMC6656946/>.
- Class-anchoring precedent — TPC-C v5.11 (warehouse-anchored tpmC ceiling)
  <https://www.tpc.org/tpc_documents_current_versions/pdf/tpc-c_v5.11.0.pdf>;
  SAP SAPS <https://www.sap.com/about/benchmark/measuring.html>.
- Real-system envelope — NHS Spine (peak 3,500 msg/s, ~60M)
  <https://digital.nhs.uk/services/spine>; Finland Kanta (>2M docs/day)
  <https://www.kanta.fi/en/statistics>; Catalonia 13M-patient openEHR CDR
  whitepaper (117M compositions; per-EHR volume dominates query cost)
  <https://hip.vitagroup.ag/wp-content/uploads/2026/04/Whitepaper_EN_Towards_large_scale_openEHR_Clinical_Data_Repositories.pdf>.

**Performance-method sources (§8.14):**
- Schroeder, Wierman and Harchol-Balter, "Open Versus Closed: A Cautionary
  Tale", NSDI 2006 —
  <https://www.usenix.org/conference/nsdi-06/open-versus-closed-cautionary-tale>.
- Tene, wrk2 README (coordinated omission) — <https://github.com/giltene/wrk2>;
  HdrHistogram — <https://hdrhistogram.org/>; "How NOT to Measure Latency" —
  <https://www.infoq.com/presentations/latency-response-time/>.
- Little, "A Proof for the Queuing Formula: L = λW", Operations Research 9(3),
  1961 — <https://doi.org/10.1287/opre.9.3.383>.
- Kingman, "The single server queue in heavy traffic", Proc. Cambridge Phil.
  Soc. 57(4), 1961 — <https://doi.org/10.1017/S0305004100036094>.
- Jain, *The Art of Computer Systems Performance Analysis*, Wiley 1991, ch. 3.
- Gunther, *Guerrilla Capacity Planning*, Springer 2007; Gunther, "A General
  Theory of Computational Scalability Based on Rational Functions" —
  <https://arxiv.org/abs/0808.1431>.
- Le Boudec, *Performance Evaluation of Computer and Communication Systems*,
  EPFL Press 2010, §2.2 — <https://perfeval.epfl.ch/>.
- Kalibera and Jones, "Rigorous Benchmarking in Reasonable Time", ISMM 2013 —
  <https://doi.org/10.1145/2464157.2464160>.
- k6 executors — <https://grafana.com/docs/k6/latest/using-k6/scenarios/executors/>;
  Gatling injection — <https://docs.gatling.io/reference/script/core/injection/>;
  Vegeta — <https://github.com/tsenart/vegeta>; Locust —
  <https://docs.locust.io/en/stable/>.

**Procurement evidence:**
- Catalonia CDR award —
  <https://discourse.openehr.org/t/region-of-catalonia-award-of-the-tender-for-the-service-of-cdr-platform/3910>.
- Karolinska/Stockholm framework —
  <https://discourse.openehr.org/t/karolinska-stockholm-procurement-of-digital-health-platform-cdr-tools-services-consultants/4457>.
- Malta NEHR — <https://www.openehr.org/news_events/industry_news/272>.
- Wales DHCW National Data Resource —
  <https://dhcw.nhs.wales/our-programmes/national-data-resource1/>.
- openEHR procurement index —
  <https://openehr.atlassian.net/wiki/spaces/resources/pages/416514052/>.

**Ecosystem:**
- [ehrbase/conformance-testing-documentation](https://github.com/ehrbase/conformance-testing-documentation)
  (AQL suites + fixtures, last push 2025-01-30);
  [CaboLabs openEHR Conformance Framework](https://www.cabolabs.com/blog/article/openehr_conformance_framework-61ef4f513f7c5.html).
- Rust crates (verified live 2026-07-21): serde-saphyr
  <https://crates.io/crates/serde-saphyr> (1.0.0-rc.1; RUSTSEC: none) vs the
  archived serde_yaml and the libyaml forks carrying RUSTSEC-2023-0075
  <https://rustsec.org/advisories/RUSTSEC-2023-0075.html>; hdrhistogram
  <https://crates.io/crates/hdrhistogram> (7.6.0;
  <https://github.com/HdrHistogram/HdrHistogram_rust>).
- AQL/RESULT_SET equivalence grounding: QUERY
  `docs/AQL/master03-syntax.adoc` (§SELECT/§DISTINCT/§TOP/§ORDER BY/§LIMIT),
  `master04-result_structure.adoc`; ITS-REST
  `schemas/query/{ResultSet,ResultSetColumn,ResultSetRow,ResultSetMetadata}.yaml`,
  `docs/query/{Request,Response}.md` (all vendored).
- Our instrument: this repository (`app/veredictum/`), published as
  `veredictum` on crates.io (0.1.0-alpha.4 at the 2026-08-27
  re-verification, when the catalogue held 1146 case cores and 249 operation
  bindings); since the cut-over (#604) the catalogue is Arazzo workflows and
  passes every validate gate. Its predecessor, the ECC
  (FerroEHR's `tools/conformance/`, 394 active catalogue cases; final
  committed baseline `docs/conformance/ferroehr/CONFORMANCE_REPORT.md`, 402
  case×format executions · 384 passed · 0 failed · 18 N/A; CORE PASS /
  STANDARD PASS / OPTIONS OBTAINED), is the retired comparison reference.
