# Provenance: the AQL conformance corpus fixtures

`catalogue/cases/_setup/aql_conformance.arazzo.yaml` carries, as inline
request bodies, fixtures of the AQL conformance corpus, and the query documents
`catalogue/cases/query/aqlc_*.arazzo.yaml` carry its queries and expected
answers. `catalogue/id-map.yaml` (`corpora: aql-conformance`) maps each of the
corpus's 495 case ids to the workflows that carry it, or says why it is not
carried.

- **Source:** <https://github.com/Cadasto/aql-conformance>
- **Commit:** `28749a482a2335ca29813911f797bf87fb82d08e`. The first import read
  `d5b8c554fecef73fbeab0620dfd28d4a1149bd63`; none of its 236 case files
  changed between the two commits.
- **Licence of the corpus:** MIT, Copyright (c) 2026 Cadasto B.V.
  (`LICENSES/MIT.txt`).
- **Licence of the cases generated from EHRbase:** the 234 cases under the
  corpus's `corpus/cases/*/ehrbase/` are rebuilt from the AQL Robot suites and
  CSV rows of `ehrbase/integration-tests` at commit
  `27b63a59c6efe9ade662e970f24af9f705dedda8`: Apache-2.0, Copyright (c) 2019
  Vitasystems GmbH and Hannover Medical School (`LICENSES/Apache-2.0.txt`).
  Confirmed from the upstream `LICENSE` the corpus vendors at
  `corpus/sources/ehrbase/upstream/LICENSE` and from each case's own `source:`
  block; the corpus records that upstream ships no `NOTICE` file at that
  commit. The carried queries and expectations of those cases sit in
  `aqlc_order_by`, `aqlc_paging` and `aqlc_where`.

Nothing was imported blind. Every carried expectation was re-derived from the
released QUERY and ITS-REST text and quotes the sentence that assigns it; the
corpus's own levels and comments were leads only.

## The fixtures, by file of the corpus

| Corpus file | Upstream origin | Licence | Changes made here |
|---|---|---|---|
| `corpus/fixtures/templates/vital_signs.opt` | No recorded source; the corpus records its own edits (`LICENSING.md` §3) | Built from CKM archetypes (`openEHR-EHR-COMPOSITION.encounter.v1`, `openEHR-EHR-OBSERVATION.blood_pressure.v1`, `.body_temperature.v1`, `.heart_rate.v1`, `.respiration.v1`); the template's own licence metadata is empty, so it is declared with the CKM archetype licences, CC-BY-SA-3.0 AND CC-BY-SA-4.0 | Trailing whitespace on each line and the final newline normalised |
| `corpus/fixtures/templates/vital_signs_sectioned.opt` | Derived from `vital_signs.opt` by the corpus, adding `openEHR-EHR-SECTION.adhoc.v1` | As `vital_signs.opt` | As `vital_signs.opt` |
| `corpus/fixtures/templates/type_repetition_conformance_ehrbase.org.opt` | `ehrbase/integration-tests`, commit `27b63a59c6efe9ade662e970f24af9f705dedda8` | Apache-2.0, Copyright (c) 2019 Vitasystems GmbH and Hannover Medical School | As `vital_signs.opt` |
| `corpus/fixtures/compositions/vital_signs.json` | The corpus's own, normalised from a generated example | MIT | None; carried as a YAML payload |
| `corpus/fixtures/compositions/vital_signs_sectioned.json` | The corpus's own | MIT | None; carried as a YAML payload |
| `corpus/fixtures/compositions/repeated_types_single.json` | `ehrbase/integration-tests` `type_repetition_conformance_ehrbase.org_one_reptation.json`, renamed by the corpus | Apache-2.0 | None; carried as a YAML payload |
| `corpus/fixtures/compositions/repeated_types_double.json` | `ehrbase/integration-tests` `type_repetition_conformance_ehrbase.org_like.json`, modified by the corpus | Apache-2.0 | None; carried as a YAML payload |
| `corpus/fixtures/compositions/participations_two.json` | Derived by the corpus from `repeated_types_single.json` | Apache-2.0 | None; carried as a YAML payload |
| `corpus/fixtures/templates/conformance_ehrbase.de.v0.opt` | `ehrbase/integration-tests`, commit `27b63a59c6efe9ade662e970f24af9f705dedda8`, corrected by the corpus: each of the 2 ordinal `<symbol>` elements without a `<value>` is given one (RM `DV_TEXT.value` is 1..1) | Apache-2.0, Copyright (c) 2019 Vitasystems GmbH and Hannover Medical School | A final newline added |
| `corpus/fixtures/templates/aql-conformance-ehrbase.org.v0.opt` | As `conformance_ehrbase.de.v0.opt`, for its 6 ordinal `<symbol>` elements | As `conformance_ehrbase.de.v0.opt` | A final newline added |
| `corpus/fixtures/compositions/ehrbase/aql-conformance-ehrbase.org.v0_contains.json` | `ehrbase/integration-tests`, commit `27b63a59c6efe9ade662e970f24af9f705dedda8`, vendored verbatim by the corpus | Apache-2.0 | None; carried as a YAML payload |
| `corpus/fixtures/compositions/ehrbase/type_repetition_conformance_ehrbase.org_like.json` | As `aql-conformance-ehrbase.org.v0_contains.json` | Apache-2.0 | None; carried as a YAML payload |
| `corpus/fixtures/compositions/ehrbase/conformance_ehrbase.de.v0_max.json` | As `aql-conformance-ehrbase.org.v0_contains.json` | Apache-2.0 | None; carried as a YAML payload |

`aql-conformance-ehrbase.org.v0.opt` embeds two CKM archetypes beside EHRbase's
own (`openEHR-EHR-OBSERVATION.blood_pressure.v2` and
`openEHR-EHR-SECTION.adhoc.v1`), and its licence metadata names no licence.
Like `type_repetition_conformance_ehrbase.org.opt`, which embeds
`openEHR-EHR-SECTION.adhoc.v1` too, it is declared under the licence of the
repository it comes from. Whether the CKM archetype terms (CC-BY-SA) also reach
it is an open doubt for the owner.

The corpus's other fixtures, its engine profiles and its runner are not carried.
