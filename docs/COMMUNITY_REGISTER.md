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
