# Vendored ITS-XML schemas (provenance)

XSD schemas from `openEHR/specifications-ITS-XML`, vendored **verbatim** (the
upstream `components/` tree of each tag, one directory per lineage). They are
released machine-readable artifacts of ITS-XML, kept beside the docs text in
`specs/openehr/ITS-XML/`.

Source repo: https://github.com/openEHR/specifications-ITS-XML
License: Apache-2.0 (the upstream repo's `LICENSE`; root reference copy
`LICENSE-APACHE-2.0`)

## Two lineages are vendored, and they differ by NAMESPACE and by RM generation

The ITS-XML repo's `Release-2.0.0` restructure changed the XML target namespace
from `http://schemas.openehr.org/v1` to `http://schemas.openehr.org/v2`
repo-wide and re-stamped every historical release folder to `v2`. So the
*namespace* is an axis independent of the RM model version. Each lineage is
vendored as the upstream `components/` tree, verbatim:

| Vendored dir | Repo ref | Namespace | What it is |
|---|---|---|---|
| `its-xml-1.0.2-nsv1/` | tag `Release-1.0.2v2` @ `f7a937778bf9ea43b01b0f9d8a616e47f35017c1` | `http://schemas.openehr.org/v1` | The STABLE pre-2.0.0 bundle: flat `ALL/` (11 XSDs) + `AOM2/` (6 XSDs + examples). |
| `its-xml-2.0.0-nsv2/` | tag `Release-2.0.0v2` @ `de8b37ba6c9a5e126623a063cafba3b58ebf1107` (also repo HEAD at vendoring) | `http://schemas.openehr.org/v2` | The 2.0.0 release (TRIAL upstream): full `components/`, with RM (1.0.2/1.0.3/1.0.4/1.1.0/latest), BASE (1.1.0/1.2.0/latest), AM (1.4/latest), OET (1.0.1/latest), QUERY/latest (70 XSDs). |

Fetched: 2026-07-04. (The `openEHR/v1/Template` namespace in the OET/Template
schemas is the separate template-document namespace, not the RM namespace.)

**The two bundles are NOT the same schema in two namespaces.** The 2.0.0
restructure also re-packaged the schemas per component and per RM release, and
the flat `Release-1.0.2v2` bundle was never re-issued against a newer RM:

- The nsv1 bundle publishes only `ALL/` (11 XSDs) and `AOM2/`, with no
  `Ehr.xsd` and no `Demographic.xsd`, so EHR, EHR_STATUS and the demographic
  PARTY types have no `xs:complexType` there at all.
- The nsv2 lineage cannot be compiled by a conformant XSD processor: its
  `archetypeNodeId` `pattern` facet (`BaseTypes.xsd`) uses Perl `(?:…)`
  groups, which XML Schema Part 2 Appendix F
  (<https://www.w3.org/TR/xmlschema-2/#regexs>) does not define.

Upstream marks 2.0.0 **TRIAL** and directs stable consumers to
`Release-1.0.2` (`specs/openehr/ITS-XML/README.adoc` §Releases and IM
Versions), so the v1 namespace is the released-stable lineage.

## Role

Reference material for the canonical-XML wire forms the catalogue exercises.
The instrument reads none of these files at run time. The container image
carries the tree under `/work/specs/its-xml-schemas/`.

`scripts/vendor/its-xml-schemas.sh` refreshes both lineage directories from
the commits in the table above, whose pins it carries; this file is
hand-written, so a refresh that moves a pin updates it in the same change.
