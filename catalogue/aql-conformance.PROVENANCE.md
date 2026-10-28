# Provenance: the AQL conformance corpus fixtures

`aql_conformance.arazzo.yaml` beside this file carries, as inline request
bodies, fixtures of the AQL conformance corpus, and the query documents
`catalogue/cases/query/aqlc_*.arazzo.yaml` carry its queries and expected
answers. `catalogue/id-map.yaml` (`corpora: aql-conformance`) maps each of the
corpus's 236 case ids to the workflows that carry it, or says why it is not
carried.

- **Source:** <https://github.com/Cadasto/aql-conformance>
- **Commit:** `d5b8c554fecef73fbeab0620dfd28d4a1149bd63`
- **Licence of the corpus:** MIT, Copyright (c) 2026 Cadasto B.V.
  (`LICENSES/MIT.txt`).

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

The corpus's other fixtures, its engine profiles and its runner are not carried.
