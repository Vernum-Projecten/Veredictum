# The community conformance register

The openEHR community debated conformance on its Discourse forum between 2021
and 2022, and again in 2026 once this instrument was published. Before
Veredictum can be put forward as an openEHR recommendation it has to meet the
requirements that community stated and answer the objections it raised. This
record lists every requirement, design position, question and objection from
those threads, attributed and linked to its post, and then assesses the
instrument against each one as it stands on `main`.

The register was first posted on issue #338 on 2026-08-30. This file is its
durable home. Issue #341 recovered the thread #338 could not read, and the
assessment was re-evidenced on 2026-10-31 against the Arazzo catalogue that
replaced the case-core catalogue (#604). Every claim below names a file on
`main` or a closed issue. A count is stated as of commit `eaee9ac1` (main,
2026-10-31) and is a reading of the tree at that commit.

## Sources

| Thread | Read | Posts |
|---|---|---|
| [openEHR Conformance / Conformance Levels / Conformance Scopes][t1616] (t/1616) | 2026-08-30, re-read 2026-10-31 | all 16 |
| [Conformance Testing: implementation alternatives][t2373] (t/2373) | 2026-08-30, re-read 2026-10-31 | all 3 |
| Conformance: roadmap from here? (t/2359), recovered as [Moved: Conformance - roadmap from here?][t17275] (t/17275) | 2026-10-31 | all 12 |
| [Conformance roadmap 2021][t1851] (t/1851) | 2026-10-31 | all 5 |
| [Conformance Framework description][t2239] (t/2239) | 2026-10-31 | all 3 |
| [Conformance Guide - initial draft][t2241] (t/2241) | 2026-10-31 | all 3 |
| [openEHR conformance verification design document][t2285] (t/2285) | 2026-10-31 | all 7 |
| [Conformance Schedule progress - data types][t2358] (t/2358) | 2026-10-31 | all 1 |

Every topic was read through Discourse's public `/t/<id>.json` endpoint
without an account. Nothing was posted anywhere.

### How t/2359 was recovered

On 2026-10-31 every direct route to t/2359 still failed:

| Request | Answer |
|---|---|
| `https://discourse.openehr.org/t/2359.json` | 403 |
| `https://discourse.openehr.org/t/conformance-roadmap-from-here/2359` | 403 |
| `https://discourse.openehr.org/t/conformance-roadmap-from-here/2359.json` | 403 |
| `https://discourse.openehr.org/raw/2359` | 403 |
| `https://discourse.openehr.org/t/2359/print` | 404 |
| `https://discourse.openehr.org/t/2359.rss` | 404 |
| Wayback CDX, `url=discourse.openehr.org/t/*2359*` | 200 with no captures |
| Wayback CDX, `url=discourse.openehr.org/t/conformance-roadmap-from-here*` | 503, then the archive reported itself temporarily offline |
| Wayback availability API for the topic URL | 429 |
| `https://web.archive.org/web/2/https://discourse.openehr.org/t/conformance-roadmap-from-here/2359` | 404 |

Discourse's own search (`/search.json?q=roadmap from here`, 200) found the
thread's posts in a new public topic, t/17275 "Moved: Conformance - roadmap
from here?". Its first post, by Erik Sundvall on 2026-09-07, explains the
move: "There was a topic with this title in an internal SEC discussion that
@borut.jures pointed would be interesting to share wider […] so I start this
thread and will move the old posts here." Posts 2 to 10 of t/17275 are the
SEC thread's posts, dated 2022-02-14 to 2022-02-21, and posts 11 and 12 are
replies from September 2026. t/2359 answers 403 because it sits in a
restricted SEC category, not because it was deleted. The register reads the
moved posts, so no row rests on an unreadable source. The adjacent public
topics named as the fallback (1851, 2239, 2241, 2285, 2358) were read and
extracted as well, since they belong to the same 2021–2022 programme.

## The register

Each row is one item, attributed and linked: a REQUIREMENT the instrument has
to meet, a POSITION on how conformance should be designed, a QUESTION, or an
OBJECTION. Rows R1 to R54 are the issue #338 register, unchanged in substance.
R55, which recorded t/2359 as unreadable, is replaced by the t/2359 rows.

### t/1616: openEHR Conformance / Conformance Levels / Conformance Scopes

- **R1** Pablo Pazos, [post 1][1616-1]. POSITION: the suite design is technology agnostic, "it doesn’t mention any specific implementation or even a REST API".
- **R2** Pablo Pazos, [post 1][1616-1]. POSITION: format independence; the SUT need only "should at least support one openEHR exchange/serialization format".
- **R3** Pablo Pazos, [post 1][1616-1]. REQUIREMENT: conformance is defined against the Service Model, with cases over "each interface and function there".
- **R4** Pablo Pazos, [post 1][1616-1]. QUESTION: do serialization formats belong in a conformance suite at all?
- **R5** Pablo Pazos, [post 1][1616-1]. QUESTION: should conformance have levels (functionality versus exchange formats)?
- **R6** Pablo Pazos, [post 1][1616-1]. QUESTION: should conformance have scopes, formats being ITS-level specs?
- **R7** Pablo Pazos, [post 1][1616-1]. REQUIREMENT: data-validation conformance is its own area: which errors a receiver checks and "how errors are reported back to the source", syntactic (schemas) and semantic (templates), over FOLDER, EHR_STATUS, EHR, CONTRIBUTION and PARTY as well as COMPOSITION.
- **R8** Pablo Pazos, [post 1][1616-1]. OBJECTION: semantic validation of non-COMPOSITION classes is blocked by "the challenge of having templates for classes that are not COMPOSITION".
- **R9** Pablo Pazos, [post 1][1616-1]. QUESTION: is technical conformance the same level as functional, or does conformance split by evaluator (developer, procurement)?
- **R10** Pablo Pazos, [post 1][1616-1]. POSITION: for procurement, functional conformance is the key claim.
- **R11** Pablo Pazos, [post 1][1616-1]. POSITION: "IMO it’s better to have an abstract / platform independent conformance test suite specification, so the implementation can vary."
- **R12** Pablo Pazos, [post 1][1616-1]. OBJECTION: the SM as published is not usable as the formal definition conformance needs.
- **R13** Pablo Pazos, [post 1][1616-1]. POSITION: a second, separate suite for technical conformance (formats, data validation, schemas against BMM).
- **R14** Pablo Pazos, [post 1][1616-1]. POSITION: each specification component may need its own test suite.
- **R15** Pablo Pazos, [post 1][1616-1]. POSITION: the end state says "how compliant" a system is, formalizes procurement, leads to official certification, and improves the SM and CNF.
- **R16** Nataliya Fliusman, [post 2][1616-2]. POSITION: CORE / STANDARD / OPTIONS tiers, with effort on CORE first.
- **R17** Nataliya Fliusman, [post 2][1616-2]. POSITION: new suites must be addable.
- **R18** Thomas Beale, [post 3][1616-3]. POSITION: executing a test implicates a concrete serial format, so formats are one dimension of a "technology profile".
- **R19** Thomas Beale, [post 3][1616-3]. POSITION: a procurement technology profile spans OS, DB, virtualisation, "and arguably, serial formats".
- **R20** Pablo Pazos, [post 4][1616-4]. OBJECTION: one universal CORE "is a little risky"; classify system kinds first (CDR, broker, analytics, recording app, visualization app), then define CORE per kind.
- **R21** Pablo Pazos, [post 4][1616-4]. POSITION: "CONFORMANCE validation is more complicated than just creating a set of tests."
- **R22** Pablo Pazos, [post 4][1616-4]. REQUIREMENT: no "hard rules on areas not all systems should implement"; the suite requires only "at least OPTs, 1.4 or 2" and one of XML or JSON.
- **R23** Pablo Pazos, [post 4][1616-4]. POSITION: per conformance item one success case, one failure case and the border cases, deliberately not every possibility; "verify a system complies with openEHR, not that a system is free of bugs".
- **R24** Pablo Pazos, [post 5][1616-5]. POSITION: the definition is profile independent, the implementation is parameterized by a technology profile per run, and the output is "comparable between vendors that have different tech profiles".
- **R25** Pablo Pazos, [post 5][1616-5]. POSITION: distinguish the procurement request (the CNF spec) from the vendor proposal (executed results under a named technology profile).
- **R26** Nataliya Fliusman, [post 6][1616-6]. QUESTION: where is the line between conformance testing and bug detection (the malformed-uid example)?
- **R27** Nataliya Fliusman, [post 6][1616-6]. QUESTION: can one standard format (canonical JSON) test all vendors?
- **R28** Nataliya Fliusman, [post 6][1616-6]. QUESTION: what is the actual open problem, given that a CONFORMANCE spec exists?
- **R29** Pablo Pazos, [post 7][1616-7]. POSITION: separate data from behaviour and internal from external; conformance tests external behaviour only, against the SM.
- **R30** Pablo Pazos, [post 7][1616-7]. REQUIREMENT: the suite definition contains no ITS-specific cases and is platform independent.
- **R31** Pablo Pazos, [post 7][1616-7]. REQUIREMENT: the definition has cases "for each conformance item defined in the SM, no more, no less".
- **R32** Pablo Pazos, [post 7][1616-7]. POSITION: technology profiles live in the implementation; one supported format suffices for external-behaviour conformance.
- **R33** Pablo Pazos, [post 7][1616-7]. POSITION: the minimal verdict is pass or fail with a general reason; a malformed-UUID specific is unit-test territory.
- **R34** Pablo Pazos, [post 7][1616-7]. POSITION: never add cases the SM does not state; a UUID-format case would bias against a legitimate implementation using OIDs.
- **R35** Pablo Pazos, [post 7][1616-7]. POSITION: extend coverage in the SM and CNF specs themselves, not in the suite definition.
- **R36** Pablo Pazos, [post 7][1616-7]. POSITION: one format cannot test all vendors; that is what technology profiles are for.
- **R37** Pablo Pazos, [post 7][1616-7]. OBJECTION: "The CNF spec is work in progress, incomplete, not viable for current use"; tying cases to the REST ITS "is not correct. CNF should be tied to SM."
- **R38** Birger Haarbrandt, [post 8][1616-8]. POSITION: functionality is testable through any format; formats can be tested separately.
- **R39** Birger Haarbrandt, [post 8][1616-8]. OBJECTION: per-format case counts skew the completeness score against a system that legitimately supports a different format.
- **R40** Pablo Pazos, [post 9][1616-9]. POSITION: data-validation conformance gets its own test-case specification (the CONSTRAINTS.md pattern: one constraint per case, instance-combination tables).
- **R41** Nataliya Fliusman, [post 10][1616-10]. QUESTION: structure clarifications on the validation specification (content and context tables, EVENT versus HISTORY case isolation).
- **R42** Pablo Pazos, [post 11][1616-11]. POSITION: the title names the constraints under test, each table row is one committed instance combination, and archetypable attributes vary independently.
- **R43** Aleksey Molkov, [post 12][1616-12]. OBJECTION: Solit-Clouds "encountered difficulties and technical limitations" and reworked the validation tests their own way.
- **R44** Pablo Pazos, [post 13][1616-13]. OBJECTION: that rework conflates RM-level with template-level validation; the two layers must stay distinct.
- **R45** Pablo Pazos, [post 13][1616-13]. POSITION: preconditions specify the state of the SUT, not actions; each case creates its own setup from that state.
- **R46** Pablo Pazos, [posts 14][1616-14] and [15][1616-15]. POSITION: hand-authored test data does not scale; generate constraint alternatives from OPTs, generate deliberately erroneous compositions, and eventually generate whole OPTs.

### t/2373: Conformance Testing: implementation alternatives

- **R47** Pablo Pazos, [post 1][2373-1]. POSITION: the tooling choice is a community decision on maintainability and implementer familiarity.
- **R48** Pablo Pazos, [post 1][2373-1]. POSITION: several parallel implementations of one test-case specification are legitimate.
- **R49** Pablo Pazos, [post 1][2373-1]. OBJECTION: Robot Framework "got a little messy very fast" as complexity grew.
- **R50** Pablo Pazos, [post 1][2373-1]. POSITION: developer familiarity matters for maintenance.
- **R51** Pablo Pazos, [post 1][2373-1]. OBJECTION: the sunk Robot suite makes any migration costly.
- **R52** Joseph Kane, [post 2][2373-2]. OBJECTION: the shared suite contained "ehrbase-specific parts"; making it generic is real work.
- **R53** Joseph Kane, [post 2][2373-2]. POSITION: a rewrite from scratch may be faster than repairing the incumbent.
- **R54** Pablo Pazos, [post 3][2373-3]. OBJECTION: the tooling effort runs on unfunded free time: "I don’t have a timeline for this since I’m doing it in my free time. Help is welcome."

### t/2359: Conformance: roadmap from here? (read as t/17275)

Post numbers are those of t/17275. Post 1 is the move notice quoted above
and carries no requirement.

- **R55** Thomas Beale, [post 2][17275-2] (2022-02-14). REQUIREMENT: "finalise what a test report looks like".
- **R56** Thomas Beale, [post 2][17275-2]. REQUIREMENT: "finalise a conformance certificate".
- **R57** Thomas Beale, [post 2][17275-2]. REQUIREMENT: make the schedule runnable: "Most parties are going to say: how do I download and run this?", ideally runnable in a CI environment.
- **R58** Thomas Beale, [post 2][17275-2]. POSITION: implementers, assumed to be the industry partners, work on this together, with participation from the procurement side.
- **R59** Thomas Beale, [post 2][17275-2]. OBJECTION: openEHR and EhrBase hold "an almost identical doc+code base" that is already diverging, "the standard danger of maintaining two copies of logically the same thing".
- **R60** Thomas Beale, [post 2][17275-2]. POSITION: the openEHR tests carry "systematic unique identifiers", and content specific to one product is not needed in openEHR's copy.
- **R61** Pablo Pazos, [post 3][17275-3]. REQUIREMENT: the report documents "the result for each test case + data set combination in an individual way".
- **R62** Pablo Pazos, [post 3][17275-3]. REQUIREMENT: the report has "a processable expression" (a JSON document).
- **R63** Pablo Pazos, [post 3][17275-3]. REQUIREMENT: the report carries product identification, the developer or provider and the per-case results, in a summarized view with metrics and a detailed view, after the FHIR Crucible and Inferno scorecards.
- **R64** Pablo Pazos, [post 4][17275-4]. POSITION: "A more ideal context would be to maintain everything at the openEHR repo and EHRBASE pulls the conformance test cases from there."
- **R65** Pablo Pazos, [post 4][17275-4]. REQUIREMENT: functional conformance names its data sets, "which are required to any test cases related with providing content/payload to the SUT".
- **R66** Pablo Pazos, [post 4][17275-4]. POSITION: content conformance is "data validation conformance"; archetypes and templates need a conformance area of their own.
- **R67** Pablo Pazos, [post 4][17275-4]. POSITION: non-functional attributes belong in the Conformance Statement, "more a declaration from the provider than something that could be tested", which becomes part of the contract.
- **R68** Thomas Beale, [post 5][17275-5]. POSITION: non-functional ("critical") attributes cover performance, reliability, security, scalability and others; "At some point we might want to specify criteria for security, maintainability and scalability."
- **R69** Pablo Pazos, [post 7][17275-7]. POSITION: use the ISO/IEC 25010 quality vocabulary, "IMHO we should stick with international standard terminology."
- **R70** Pablo Pazos, [post 8][17275-8]. REQUIREMENT: test data sets are identified, so results are reported "by test case and data set".
- **R71** Pablo Pazos, [post 8][17275-8]. QUESTION: an implementer's pipeline moves in days; can openEHR release conformance cases that fast?
- **R72** Pablo Pazos, [post 9][17275-9]. POSITION: "for each test case there may be multiple test data sets"; suites, cases and data sets are each identified.
- **R73** Pablo Pazos, [post 9][17275-9]. POSITION: three kinds of validation: RM or schema (structural), invariants (non-structural), and archetype or OPT (metamodel).
- **R74** Pablo Pazos, [post 9][17275-9]. POSITION: the conformance specification depends on the Service Model, which depends on the RM and AOM.
- **R75** Thomas Beale, [post 10][17275-10]. POSITION: "A 'test' is therefore the execution of a particular test case with a particular data set"; RM invariants count as structural, distinct from archetype rules "a whole level up"; a data-set identification system is needed.
- **R76** Ian McNicoll, [post 11][17275-11] (2026-09-07). REQUIREMENT: a business reading of a result: "What does it mean to me as a potential CDR consumer if a CDR fails a particular test? […] Is this even a proper openEHR CDR or just a sham".
- **R77** Ian McNicoll, [post 11][17275-11]. POSITION: severity levels, after the Postman tests: "Level 0 fails as being so critical as telling the consumer that this fails to perform even basic CDR functionality", Level 1 for technical failures such as response codes, headers or a missing `archetype_details` on EHR_STATUS.
- **R78** Ian McNicoll, [post 11][17275-11]. REQUIREMENT: the reading accounts for "different levels of support for different parts of the spec", naming Demographics, and for the maturity of parts of the spec "especially for AQL".
- **R79** Pablo Pazos, [post 12][17275-12] (2026-09-09). POSITION: his conformance verification framework covers testing, reporting and certification, and "Maybe a second read with new eyes might resurrect my work into something that can be peer-checked and incorporated into the specs."

### t/1851: Conformance roadmap 2021

- **R80** Thomas Beale, [post 1][1851-1]. REQUIREMENT: procuring organisations receive "'standard' (comprehensible) conformance statements" from tendering vendors.
- **R81** Thomas Beale, [post 1][1851-1]. REQUIREMENT: procurers can test products themselves to check tender claims, including after integration.
- **R82** Thomas Beale, [post 1][1851-1]. REQUIREMENT: vendors and implementers self-test "and initially self-certify".
- **R83** Thomas Beale, [post 1][1851-1]. REQUIREMENT: "(eventually) enabling reliable 3rd party testing with certification".
- **R84** Thomas Beale, [post 1][1851-1]. POSITION: "conformance is concretely tested against ITS specifications"; a product claims conformance to "openEHR REST API v1.1.0, with e.g. JSON, XML etc payloads", and the SM names the capabilities.
- **R85** Thomas Beale, [post 1][1851-1]. POSITION: the legal veracity of a claim is a matter for commercial contracts; the framework provides a technical basis, not a guarantee, until third-party certification exists.
- **R86** Thomas Beale, [post 1][1851-1]. POSITION: four artefacts: Conformance Schedule, Profiles, Statement and Certificate.
- **R87** Thomas Beale, [post 1][1851-1]. REQUIREMENT: "a CI-based tool environment that is either hosted & publicly usable and/or locally instantiable".
- **R88** Thomas Beale, [post 1][1851-1]. QUESTION: are the testing materials public and free, or free only to paying openEHR members?
- **R89** Birger Haarbrandt, [post 2][1851-2]. POSITION: free access for member companies only, "as we need further incentives for the members".
- **R90** Pablo Pazos, [post 3][1851-3]. POSITION: the Conformance Statement is a legal document in a tender and then in the contract, on the DICOM model, distinct from testing.
- **R91** Pablo Pazos, [post 3][1851-3]. REQUIREMENT: guidance for procurers on what to ask and how, and a standard statement form vendors answer in comparable terms.
- **R92** Pablo Pazos, [post 3][1851-3]. POSITION: conformance levels belong in the statement, because minor technical differences make levels hard to pull from test execution.
- **R93** Pablo Pazos, [post 3][1851-3]. REQUIREMENT: the statement names the versions of every specification implemented (RM, AOM, OPT, schemas), so a procurer can require a minimum.
- **R94** Pablo Pazos, [post 3][1851-3]. POSITION: a computable statement compared automatically and published "on a vendor-neutral site".
- **R95** Thomas Beale, [post 4][1851-4]. POSITION: the SM is the basis of the capability list per service (the Conformance Schedule), and the four terms are agreed first.

### t/2239: Conformance Framework description

- **R96** Thomas Beale, [post 1][2239-1]. POSITION: the minimum artefact set is a Conformance Guide, a test framework, a generic test schedule, an executable technology-specific schedule, and templates for the Test Execution Report, Statement and Certificate, "the minimum we can get away with to satisfy contractual as well as engineering needs".

### t/2241: Conformance Guide - initial draft

- **R97** Heather Grain, [post 2][2241-2]. REQUIREMENT: archetype specifications state SHALL, SHOULD and MAY plainly so content can be assessed for conformance; "It is vital that conformance is not only considered from the technical use perspective but also from the content".
- **R98** Thomas Beale, [post 3][2241-3]. POSITION: the content test cases "all operate on a 'MUST' level of conformance, i.e. the idea is that the server cannot permit invalid data (data not conforming to its archetypes) to be committed".

### t/2285: openEHR conformance verification design document

- **R99** Birger Haarbrandt, [post 5][2285-5]. POSITION: the need behind the 2019–2022 work was ensuring conformance of openEHR servers inside a platform, plus automated integration tests for one CDR.
- **R100** Thomas Beale, [post 6][2285-6]. POSITION: "we need to get industry involved and make the openEHR conformance specs operational, and eventually web-executable."
- **R101** Pablo Pazos, [post 7][2285-7]. OBJECTION: the CNF draft is "mainly focused on testing CDRs only"; implementers of other components need conformance specifications too.

### t/2358: Conformance Schedule progress - data types

- **R102** Thomas Beale, [post 1][2358-1]. POSITION: every test case, REST and content alike, carries a formal identifier "within a global id scheme".

## The assessment

Each theme groups register rows and carries one verdict: MET (the mechanism
is named), PARTLY (the honest delta is stated), or NOT MET. Themes 1 to 10
are #338's, re-evidenced; themes 11 to 16 cover the rows t/2359 and the
fallback topics added.

### 1. Abstract definition versus profiled implementation: PARTLY

Rows R1, R2, R11, R24, R30, R32, R36, R48.

Format independence and harness independence are met. Format independence:
a statement declares `tech_profiles`, and a case needing an unclaimed format
is skipped `format-not-claimed` (`docs/ARCHITECTURE.md` §8.10, §15.8). The
ITS-REST rule that "Services MUST support at least one of the openEHR **XML**
or **JSON** canonical formats" is an option with two arms in
`catalogue/capabilities.yaml` (`ehr-xml-read` and its siblings), so a
JSON-only server and an XML-capable one are both judged on what they claim.
Harness independence: the catalogue is Arazzo 1.1.0, an OpenAPI Initiative
standard, so any Arazzo tool can drive the same documents (§15.8), and the
verification pack (`verification-pack/arazzo/`, §8.12) is the recorded
exchange plus adjudicated verdicts a second runner has to reproduce.

The platform-independent definition (R11, R30) is not met, by decision. The
catalogue's cases are workflows over the released ITS-REST operations
(§15.2); there is no protocol-neutral layer above them any more. That layer
was the case-core catalogue, removed at the cut-over (#604) together with its
anchor, the Service Model. A second wire protocol would need its own
catalogue chapter, not just new bindings. The reasoning is in theme 2.

### 2. The Service Model as the anchor: NOT MET, deliberately

Rows R3, R29, R31, R37, R74, R95.

The SM was never released, so it is no part of the oracle (`CLAUDE.md` hard
rule 1, #604), and no case is anchored on it. Every expectation cites the
released ITS-REST docs text, or the released OAS where the docs text is
silent. This is the opposite of R37's "CNF should be tied to SM". The
position the instrument takes is the one Thomas Beale argued in
[t/1851 post 1][1851-1] (R84), and that Pablo Pazos accepted in the reply
([post 3][1851-3]): "ITS is the only way of doing that, we can’t test
abstract specifications". Grounding cases on unreleased text would let an
instrument assert what no released sentence requires, which is the bias R34
warns against.

R31's "no more, no less" is met against the released ITS-REST surface.
No less: the `surface-coverage` gate enumerates the released operations, their
response branches and the negotiation headers from the OAS, and fails on any
that has neither a covering step nor a gap naming its tracker issue
(`catalogue/cases/surface.yaml`, §15.9). No more: the citation gate refuses a
criterion whose quote does not occur verbatim in a vendored released file,
and the realization-scope gate keeps a folder driven only over the
instrument's own routes from reading as openEHR coverage (§15.9).

### 3. Conformance versus bug hunting: MET

Rows R23, R26, R33, R34.

An expectation exists only with a released citation, checked by `validate`:
the citation gate, plus the level gate, which requires `level: should` for a
SHOULD quote and refuses a MAY quote outright, "because permission is never
asserted" (§15.9). A failing `should` criterion makes the row a warning that
never counts against the server (§8.11; the `warnings` badge in
`website/book/src/badges.md`). A spec silence is never asserted: it becomes
an `upstream-report` issue with its patch under `upstream/`, and behaviour the
release leaves open becomes an option a statement answers (`CLAUDE.md` hard
rule 1, `.claude/rules/cnf-triage.md`). The ambiguity register #338 cited is
gone (#617); its entries became those reports and options.

The instrument covers more than R23's one success, one failure and the
borders: every status-code branch and header rule the released text defines
is its own case, with per-folder floors that only rise (`arazzo-floor`,
§15.9). That is not bug hunting by R33's test, because each case cites the
sentence that defines the behaviour. A red row carries what was checked, why,
what was expected and what was received, with the recorded exchange (§15.8),
which is the "general reason" R33 asked for plus its evidence.

### 4. Levels, scopes, tiers and profiles: PARTLY

Rows R4–R6, R9, R10, R13, R14, R16–R20, R38, R92.

The CORE / STANDARD / OPTIONS tiers of R16 are not adopted. #856 (closed,
owner ruling 2026-10-31) removed the statement's profile tiers and the
certificate's tier ratings, because nothing in a released text says which
behaviour is CORE. What replaces them:

- a statement claims capability folders under `catalogue/cases/`, and every
  document outside the claim is skipped `not-claimed` (§8.10, §8.11);
- a result reads as BASE and OPTIONS, chapter by chapter
  (`catalogue/chapters.yaml`, the `arazzo-structure` gate in §15.9): a
  document no option arm names is BASE, and one an arm names is OPTIONS;
- each option carries a tier: `recommended` where support is a SHOULD,
  `options` where the release leaves the behaviour to the implementation, and
  `spec-boundary` where the option exists only because the released text is
  defective (43 options at `eaee9ac1`: 10, 25 and 8);
- a run is judged under a specification profile, `released` or `development`
  (#875, `schemas/arazzo-results.schema.json` `profile`), and a case resting
  on TRIAL text is read with the OPTIONS (#872).

Formats as a technology profile (R18, R19, R38) are the statement's
`tech_profiles`; format-specific behaviour has its own cases (the
`*_xml*` and `*_not_acceptable` documents across the folders), which answers
R4. The two audiences of R9 get distinct artifacts: `results.json` and the
rendered report for a developer, the certificate and the badges for a
procurer.

R20 asked for CORE per kind of system. The capability claim lets a server
claim only what it implements, so a server without AQL is not a failing CDR.
What the instrument does not cover is any kind of system other than a
platform CDR reached over ITS-REST: a broker, an analytics engine or a
clinical application has no catalogue here (R14, R20, R101). That boundary
is not written down anywhere a reader would find it (candidate C3 below).

### 5. Procurement and certification: PARTLY

Rows R10, R15, R25, R28.

R25's request and proposal are the statement and the results. The statement
(`schemas/statement.schema.json`) is the ISO/IEC 9646 ICS and the
ISO/IEC 17050-1 supplier's declaration in one document, and `validate
--statement` reviews it statically (§8.10, §8.11). The results are
`results.json` with its HAR recording. The hosted instrument issues a
certificate, a JWS over the digests of the results and the recording,
verifiable offline with `veredictum verify` against the published key set
(§15.10, #592). Veredictum publishes no results and keeps no registry
(#592). §9 of `docs/ARCHITECTURE.md` maps the rungs from a published
statement to accredited certification.

The delta: certificate issuance is off. The hosted instrument runs in trial
mode until #825 (open) publishes the production key, and the book says no
certificate is issued before the openEHR SEC approves the instrument
(`website/book/src/badges.md`). Rungs 1 to 3 are design, not operation. A
procurer today has rung 0: a vendor's own published statement and results,
re-runnable by anyone.

### 6. Fair scoring across legitimate variation: MET

Rows R22, R39.

Birger Haarbrandt's skew cannot occur: a case on an unclaimed format is
skipped `format-not-claimed`, never failed, and only *failed* counts against
the server (§15.8). R22's "no hard rules on areas not all systems should
implement" is the selection machinery: `not-claimed`,
`option-not-selected`, `version-out-of-range`, `format-not-claimed`,
`extension-not-offered` and `not-declared`, each a closed reason printed with
its citation (§15.8). The claim gates refuse a claimed folder in which no
document can run under the statement's own declarations, so a claim cannot
look complete while verifying nothing (`claim-completeness` and
`arm-format`, §15.9).

### 7. Data-validation conformance: MET for COMPOSITION, PARTLY overall

Rows R7, R8, R40–R45, R65, R66, R73, R75, R98.

The content folder (`catalogue/cases/content/`) carries the CONSTRAINTS.md
pattern R40 and R42 describe: one workflow per constraint context, and a
rows file per outcome whose rows are the committed instance combinations
(378 rows files across the catalogue at `eaee9ac1`). The rows files are named
by what they test, so R44's two layers stay visible: an `accepted` file, a
refusal named by the AOM constraint violated (`c_multiple_attribute_cardinality`,
`c_attribute_existence`, `c_string_pattern`, …), or a refusal named by the RM
attribute missing or invalid (`value_missing`, `magnitude_missing`,
`timezone_validity`, …). R45's preconditions-as-state are setup workflows a
case names in `dependsOn` (`catalogue/cases/_setup/`, §15.8). R98's MUST
level holds: every content refusal expects the server to refuse the commit.
R65, R70 and R72's data sets are rows files, and each row is judged and
reported as its own result row (the `row` member of
`schemas/arazzo-results.schema.json`).

Outside COMPOSITION, #339 (closed) settled what the released text allows:
root-identity refusals on EHR_STATUS (`ehr_status/ehr_status_update.arazzo.yaml`),
FOLDER (`directory/directory_create_refused.arazzo.yaml`,
`directory_update.arazzo.yaml`) and the PARTY classes
(`demographic/*_refused.arazzo.yaml`), with template-level validation of
those classes recorded as a boundary: no released sentence binds a template
check on them to the wire. R7's error-reporting half went through #340
(closed), which asserts the error response's transport headers; the shape of
an error body has no released standard, and that silence is the open
`upstream-report` #699, with its patch
`upstream/specifications-ITS-REST/0699-error-bodies-are-problem-details.patch`.
R66's archetype and template conformance area is out of scope (theme 16).

### 8. Authoring economics: MET

Row R46.

A rows file names a recipe and its literals, and the recipe builds each row's
composition and operational template at drive time
(`recipe: content_synthesized` in the content rows files, §15.7). No
template or composition is hand-authored per row, and every refusal rows file
has an accepted twin built by the same recipe.

### 9. Tooling and maintenance economics: MET technically; R54 NOT MET

Rows R47, R49–R54.

R49 and R53: the instrument is a typed implementation written from scratch,
and the Robot suites are never an oracle (`CLAUDE.md` hard rule 1).
`catalogue/id-map.yaml` maps every former case id to the workflows that
carry it. R52: no case names a vendor image, endpoint or behaviour, and a
party's deployment reaches the run only through its IXIT (§8.10). R47 and R50:
the maintained surface is YAML in a published standard (Arazzo 1.1.0), with
JSON Schemas under `schemas/`, run by one binary on crates.io, the container
image, the GitHub Action (`action.yml`,
`website/book/src/github-action.md`) and a GitLab job
(`website/book/src/badges.md` §GitLab).

R54 still stands. `docs/MAINTAINERS.md` lists one maintainer and one area
maintainer (the AQL workflows), and no organisation funds the work. The
funding question is a conversation with the openEHR Foundation, not a change
this repository can make, and it has no issue here.

### 10. The specifications as the blocker: MET on the instrument's side

Rows R12, R14, R21, R28, R35, R37, R71.

R37's demand that CNF not be treated as an authority is hard rule 1: the CNF
schedule says which behaviours to cover and never what the answer is. R35's
"extend the specs, not the suite" is the outbound pipeline: each silence or
contradiction is an `upstream-report` issue with a patch under `upstream/`
written against the pinned openEHR `master`, and `scripts/checks/upstream-patches-apply.sh`
refuses a patch that does not apply there (#906). At `eaee9ac1` there are 69
such issues (67 open, 2 closed) and 35 patches across six openEHR
repositories. The `development` profile shows what a run would say against
the newest text (#875). R12's inadequate SM is answered by dropping it from
the oracle (theme 2). R71 is answered by release cadence: the catalogue ships
with the instrument on every tag, independent of openEHR's release cycle.

The delta outside the instrument's control: this repository files nothing on
an openEHR channel (`.claude/rules/cnf-triage.md`), so a report and its patch
reach openEHR only when a maintainer submits them there.

### 11. The test report: MET

Rows R55, R61–R63, R70, R72, R75.

`results.json` (`schemas/arazzo-results.schema.json`) is the processable
report R62 asked for: one row per workflow row, so each result is one test
case with one data set (R61, R75), passed, failed, errored or skipped, a
skipped row with its reason and citation and a red row with its four-part
explanation and the HAR entry it was judged on (§8.10). The views R63 asked
for exist: a summary (the Markdown summary and the BASE, OPTIONS and
warnings counts of `veredictum show`, and the badges), and the detailed
report with each red row's exchange. JUnit XML and CTRF exports carry the
same rows into a vendor's CI (§15.8). The certificate payload names the SUT
with the source of each identity field (§15.10), and the statement names the
product and vendor (§8.10).

### 12. A business reading of a result: PARTLY

Rows R76, R77, R78.

What exists: a result reads chapter by chapter, BASE and OPTIONS apart, so a
buyer sees which areas (EHR, COMPOSITION, Query, Demographic, …) hold
failures (`catalogue/chapters.yaml`); a SHOULD the server did not follow is a
warning, never a failure; a capability the vendor does not claim is skipped,
not failed (R78's "different levels of support"); and a case on TRIAL text is
read with the OPTIONS (R78's maturity).

What does not: Ian McNicoll's severity split (R77). The released texts grade
requirements as MUST and SHOULD and nothing finer, so a "Level 0, not a CDR
at all" verdict would rest on a judgement no citation carries, which the
instrument refuses to make (hard rule 1). R76's question, what one failed row
means to a buyer, has no answer written for that buyer: the book explains the
mechanics of a result, not how a procurer should weigh it (candidate C1).

### 13. Runnable, downloadable, web-executable: MET

Rows R57, R81, R82, R87, R100.

The instrument installs with `cargo install veredictum`, runs from the
container image or the quickstart compose file (`deploy/docker/`), runs in a
vendor's CI through the GitHub Action or a GitLab job, and runs in the
browser at the hosted console (`deploy/hosted/`). A procurer can run it
against a deployment without the vendor, given the IXIT (R81). A vendor
self-tests and publishes its own badges (#919). Every one of these is a
self-run (R82).

### 14. One shared source, maintained together: PARTLY

Rows R58–R60, R64, R79, R88, R89, R102.

R59's double maintenance does not occur: there is one catalogue, in this
repository, and no copy of another product's suite. R60 and R102's global
identifiers are the workflow ids, unique across the catalogue (the identity
gate, §15.9). R88: everything is public and free under Apache-2.0, against
R89's members-only proposal, and §9 of `docs/ARCHITECTURE.md` keeps fees for
services, never artifacts. Reuse by others has started: Erik Sundvall's
TypeScript library vendored the content subset to test its validators
([t/17238 post 7][17238-7]).

The delta: R64 wanted the source in openEHR's repository with implementers
pulling from it. The catalogue lives in this repository, outside openEHR,
with a hand-over to openEHR International designed but not done (§12, §15.10).
R58's joint work by industry partners has not happened at scale: one
maintainer and one area maintainer. R79's framework (Pablo Pazos's
`openehr-conformance-verification` repository) is credited in
`website/book/src/about.md`, but no record here maps its requirements against
the instrument the way this register maps the forum (candidate C2).

### 15. The statement as a procurement instrument: PARTLY

Rows R56, R80, R83–R86, R90, R91, R93, R94, R96.

Met: the statement is computable and schema'd (R80, R94), carries the
attestation block and the ISO/IEC 17050-1 responsibility sentence (R90),
declares `spec_versions` so a procurer can require a minimum (R93), and pins
the exact product version (§8.10). R85's position that legal veracity is a
contract matter is stated in §6.4 and §9. R84's ITS-level claim is the
statement's capability claim against the released ITS-REST. R86 and R96's
four artefacts map to the catalogue (Schedule), the capability claim with its
option arms (in place of Profiles), the statement, and the certificate.

Partly: R94's vendor-neutral comparison site does not exist, by decision
(#592); comparison is left to whoever holds two vendors' documents. R91's
procurement guidance exists only as the RFP clause in §10 of
`docs/ARCHITECTURE.md`, not as a guide a procurer would find in the book.
R83's third-party certification is rung 3 of §9, not offered. R56's
certificate exists, and issuance waits on #825 (theme 5).

### 16. Non-functional attributes and content conformance: PARTLY

Rows R66–R69, R97, R101.

Met: performance is measured (R68) by the `perf` instrument, which offers a
declared traffic mix on a load ladder and writes a `perf-result` whose every
number re-derives from its histograms (§8.14, `schemas/perf-result.schema.json`,
#879). It is a reading, never a rating, and it enters no conformance verdict
(§8.11). The statement's `non_functional` block holds declaration-only slots
that are never verdict inputs, which is R67 exactly (§8.10). Security
behaviours are functional cases (`catalogue/cases/security/`, §8.15).
ISO/IEC 25010 is the scope vocabulary (§6.3), as R69 asked.

Not met, and out of scope: conformance of archetypes and templates as
artifacts (R66, R97) and of non-CDR components (R101). This instrument tests
platform CDRs over their wire. The boundary is real, but it is not stated
where a reader would look for it (candidate C3).

## Candidate issues

Nothing below was filed from this record. Each is a gap the assessment found,
with a proposed title and acceptance criteria.

- **C1. A buyer's reading of a conformance result** (R76, R77, R78). A book
  page for a procurer that explains what a failed BASE row, a failed option
  row, a warning and a skipped row each mean, and what a chapter with
  failures says about that area. Criteria: the page exists and is linked from
  the results and badges pages; it states that no severity beyond MUST and
  SHOULD is applied, and why; it answers the "is this a real CDR" question
  with what the result can and cannot show.
- **C2. Map the CaboLabs conformance verification framework against the
  instrument** (R79, R14, R21). Criteria: the framework's design document is
  read in full, every requirement is listed with attribution and assessed
  MET, PARTLY or NOT MET with evidence, in the form of this register; each
  gap gets its own issue.
- **C3. State the instrument's scope boundary** (R14, R20, R66, R97, R101).
  Criteria: the book's methodology page says the instrument tests platform
  CDRs over ITS-REST, and names what it does not cover (brokers, analytics,
  clinical applications, CKM and archetype or template artifact conformance)
  with the reason.
- **C4. A procurement guide in the book** (R91, R80). Criteria: the RFP clause
  of `docs/ARCHITECTURE.md` §10 and the equivalence duty it carries are
  published as a book page a tendering authority can use, with the five
  parameters explained and an example statement requirement.

[t1616]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616
[t2373]: https://discourse.openehr.org/t/conformance-testing-implementation-alternatives/2373
[t17275]: https://discourse.openehr.org/t/moved-conformance-roadmap-from-here/17275
[t1851]: https://discourse.openehr.org/t/conformance-roadmap-2021/1851
[t2239]: https://discourse.openehr.org/t/conformance-framework-description/2239
[t2241]: https://discourse.openehr.org/t/conformance-guide-initial-draft/2241
[t2285]: https://discourse.openehr.org/t/openehr-conformance-verification-design-document/2285
[t2358]: https://discourse.openehr.org/t/conformance-schedule-progress-data-types/2358
[1616-1]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/1
[1616-2]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/2
[1616-3]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/3
[1616-4]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/4
[1616-5]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/5
[1616-6]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/6
[1616-7]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/7
[1616-8]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/8
[1616-9]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/9
[1616-10]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/10
[1616-11]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/11
[1616-12]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/12
[1616-13]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/13
[1616-14]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/14
[1616-15]: https://discourse.openehr.org/t/openehr-conformance-conformance-levels-conformance-scopes/1616/15
[1851-1]: https://discourse.openehr.org/t/conformance-roadmap-2021/1851/1
[1851-2]: https://discourse.openehr.org/t/conformance-roadmap-2021/1851/2
[1851-3]: https://discourse.openehr.org/t/conformance-roadmap-2021/1851/3
[1851-4]: https://discourse.openehr.org/t/conformance-roadmap-2021/1851/4
[2239-1]: https://discourse.openehr.org/t/conformance-framework-description/2239/1
[2241-2]: https://discourse.openehr.org/t/conformance-guide-initial-draft/2241/2
[2241-3]: https://discourse.openehr.org/t/conformance-guide-initial-draft/2241/3
[2285-5]: https://discourse.openehr.org/t/openehr-conformance-verification-design-document/2285/5
[2285-6]: https://discourse.openehr.org/t/openehr-conformance-verification-design-document/2285/6
[2285-7]: https://discourse.openehr.org/t/openehr-conformance-verification-design-document/2285/7
[2358-1]: https://discourse.openehr.org/t/conformance-schedule-progress-data-types/2358/1
[2373-1]: https://discourse.openehr.org/t/conformance-testing-implementation-alternatives/2373/1
[2373-2]: https://discourse.openehr.org/t/conformance-testing-implementation-alternatives/2373/2
[2373-3]: https://discourse.openehr.org/t/conformance-testing-implementation-alternatives/2373/3
[17238-7]: https://discourse.openehr.org/t/veredictum-a-machine-readable-conformance-framework-for-openehr/17238/7
[17275-2]: https://discourse.openehr.org/t/moved-conformance-roadmap-from-here/17275/2
[17275-3]: https://discourse.openehr.org/t/moved-conformance-roadmap-from-here/17275/3
[17275-4]: https://discourse.openehr.org/t/moved-conformance-roadmap-from-here/17275/4
[17275-5]: https://discourse.openehr.org/t/moved-conformance-roadmap-from-here/17275/5
[17275-7]: https://discourse.openehr.org/t/moved-conformance-roadmap-from-here/17275/7
[17275-8]: https://discourse.openehr.org/t/moved-conformance-roadmap-from-here/17275/8
[17275-9]: https://discourse.openehr.org/t/moved-conformance-roadmap-from-here/17275/9
[17275-10]: https://discourse.openehr.org/t/moved-conformance-roadmap-from-here/17275/10
[17275-11]: https://discourse.openehr.org/t/moved-conformance-roadmap-from-here/17275/11
[17275-12]: https://discourse.openehr.org/t/moved-conformance-roadmap-from-here/17275/12
