# Proposed fixes to openEHR ITS-XML

Each patch here is the fix proposed in one `upstream-report` issue, written
against [openEHR/specifications-ITS-XML](https://github.com/openEHR/specifications-ITS-XML)
at Release-2.0.0v2, commit `de8b37ba6c9a5e126623a063cafba3b58ebf1107`, which is the commit `specs/its-xml-schemas/its-xml-2.0.0-nsv2/` vendors.

| Patch | Report |
|---|---|
| `0719-archetype-xsd-carries-aom14-temporal-fields.patch` | #719 |

A patch is named after its report's number. Apply one in a checkout of that
repository with `git apply <patch>`.

These patches are never applied to `specs/its-xml-schemas/its-xml-2.0.0-nsv2/`. That tree stays the
released bytes, because every expectation in the catalogue quotes the released
text, defects included, and the report records the defect.
