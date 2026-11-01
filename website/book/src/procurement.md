# Procurement

<!-- toc -->

For a tendering authority requiring openEHR conformance in a call for
tenders: a clause to paste, its five parameters, and the equivalence duty that
comes with naming any conformance scheme. The clause comes from the design
record (`docs/ARCHITECTURE.md` §10).

## The clause

> The offered product must demonstrate openEHR conformance to [CNF schedule
> release ≥ R, capabilities C and option arms O, technology profile including
> canonical JSON], evidenced by a published openEHR Conformance Statement at
> attestation rung ≥ 1 for the product version offered, **or by equivalent
> means of proof** (including a manufacturer's technical dossier or an
> equivalent conformance report) demonstrating conformity to the same test
> cases. The awarding authority will accept any evidence that objectively
> establishes equivalent conformance, and reserves the right to require a
> witnessed re-run (rung 2) of the published or submitted results prior to
> acceptance.

## The five parameters

Each maps to a field of the vendor's statement (`schemas/statement.schema.json`),
so a submission can be checked field by field.

| Parameter | What you choose | Where the vendor answers it |
|---|---|---|
| **R, the release** | the catalogue release to judge against, and the openEHR specification versions you require | `schedule_release`, and `spec_versions` (for example `"its_rest": "1.1.0"`) |
| **C, the capabilities** | the parts of the REST API the product must cover, by folder under `catalogue/cases/`: `ehr`, `ehr_status`, `composition`, `contribution`, `directory`, `query`, `definition`, `demographic` and the others | `claims.capabilities` |
| **O, the option arms** | for each option in `catalogue/capabilities.yaml` that matters to you, the arm you need; for example `composition-xml-supported` for canonical XML | `options` |
| **The technology profile** | the ITS and wire formats the product must serve; canonical JSON at the least | `tech_profiles`, for example `its-rest` with `canonical-json` |
| **The rung** | how much independent checking stands behind the claim | the attestation the vendor supplies |

The rungs (`docs/ARCHITECTURE.md` §9):

| Rung | Name | What stands behind the claim |
|---|---|---|
| 0 | Published statement | the vendor publishes its statement and results itself |
| 1 | Self-declared | rung 0 plus a signed declaration of conformity by an authorised officer, under ISO/IEC 17050-1 |
| 2 | Community-verified | a named witness reproduces the results by re-running the instrument against the vendor's deployment |
| 3 | Certified | an accredited laboratory runs the instrument and an accredited body certifies; not offered until surveillance is funded |

Only a result whose `specification` member reads `released` counts toward a
released claim ([the method](methodology.md#two-specification-profiles)). Ask
for the statement and the `results.json` it was run with, and check them with
`veredictum validate --statement` and `veredictum show`
([Command reference](commands.md)). No certificate is issued before the
openEHR SEC approves the instrument, and every 0.x result is marked
`pre-approval`.

## An example requirement

For a hospital that needs EHRs, compositions, contributions, folders,
templates and AQL over canonical JSON, and no XML:

> The offered product must demonstrate openEHR conformance to the Veredictum
> catalogue release named in its statement, judged against the released
> openEHR specifications with openEHR REST API 1.1.0, capabilities `ehr`,
> `ehr_status`, `composition`, `contribution`, `directory`, `definition` and
> `query`, and a technology profile `its-rest` including `canonical-json`,
> evidenced by a published openEHR Conformance Statement at attestation rung
> ≥ 0 for the product version offered, together with the `results.json` of
> the run behind it, **or by equivalent means of proof** (including a
> manufacturer's technical dossier or an equivalent conformance report)
> demonstrating conformity to the same test cases. The awarding authority will
> accept any evidence that objectively establishes equivalent conformance, and
> reserves the right to require a witnessed re-run (rung 2) of the published or
> submitted results prior to acceptance.

It names no option arm, so either answer to every option is acceptable. To
require XML, add `composition-xml-supported` and the other `*-xml-supported`
arms you need. It asks for rung 0, which is what a vendor can supply today;
raise it once higher rungs are operated.

## The equivalence duty

An authority bound by Directive 2014/24/EU on public procurement must keep
"or by equivalent means of proof". Naming one scheme's statement or
certificate as the only acceptable proof is open to challenge as
discriminatory:

- **Technical specifications** (Article 42(3)(b)): a specification set by
  reference to a standard or technical reference system "shall be accompanied
  by the words 'or equivalent'".
- **Labels** (Article 43(1)): "Contracting authorities requiring a specific
  label shall accept all labels that confirm that the works, supplies or
  services meet equivalent label requirements." Where an operator
  demonstrably could not obtain the label in time for reasons not its own,
  the authority "shall accept other appropriate means of proof, which may
  include a technical dossier from the manufacturer".
- **Test reports and certificates** (Article 44): certificates from
  "equivalent other conformity assessment bodies shall also be accepted", and
  the authority "shall accept other appropriate means of proof than those
  referred to in paragraph 1, such as a technical dossier of the
  manufacturer" where the operator had no access to them.

The requirement stays objective: the instrument, catalogue and schemas are
public and free under Apache-2.0, so any vendor can produce the evidence and
any authority can check it, and the witnessed re-run tests a doubtful
submission.

The Directive is quoted from
<https://www.legislation.gov.uk/eudr/2014/24/contents> as it stood on
31 December 2020; the authoritative version is on EUR-Lex
(<https://eur-lex.europa.eu/eli/dir/2014/24/oj>). This page is not legal
advice: check the clause with your procurement counsel against the national
law that transposes the Directive.

## Comparing submissions

Statements and results have published schemas, so two submissions compare
field by field: capability, option and chapter. The instrument publishes no
comparison and keeps no registry (`docs/ARCHITECTURE.md` §15.10).
[Reading a result](reading-results.md) explains each row for the decision.
