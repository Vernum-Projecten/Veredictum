# Proposed fixes to openEHR ITS-XML

Each patch here is the fix proposed in one `upstream-report` issue, written
against the `master` branch of
[openEHR/specifications-ITS-XML](https://github.com/openEHR/specifications-ITS-XML), the newest
openEHR text and the line a fix lands on. The patches target `master` at
commit `e6ff2a4e55c13cc79e15376a46c29f4c1e529f92`, the snapshot vendored under
`specs/openehr-development/specifications-ITS-XML/`. The report itself quotes the released text under
`specs/its-xml-schemas/its-xml-2.0.0-nsv2/`, which stays the oracle.

| Patch | Report |
|---|---|
| `0719-archetype-xsd-carries-aom14-temporal-fields.patch` | #719 |

A patch is named after its report's number.
`scripts/checks/upstream-patches-apply.sh` checks on every pull request that
each one applies to the snapshot with
`git apply --check --directory=specs/openehr-development/specifications-ITS-XML`. Apply one in a checkout of
that repository at the pinned commit with `git apply <patch>`. A defect that
`master` already fixes carries no patch here, and its report records the
fixing commit.

These patches are never applied to `specs/its-xml-schemas/its-xml-2.0.0-nsv2/` or to the snapshot.
The release tree stays the released bytes, because every expectation in the
catalogue quotes the released text, defects included, and the report records
the defect.
