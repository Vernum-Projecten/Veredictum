# Proposed fixes to openEHR QUERY

Each patch here is the fix proposed in one `upstream-report` issue, written
against the `master` branch of
[openEHR/specifications-QUERY](https://github.com/openEHR/specifications-QUERY), the newest
openEHR text and the line a fix lands on. The patches target `master` at
commit `28575a39d6cda11b588892cc8d3376724e645b2e`, the snapshot vendored under
`specs/openehr-development/specifications-QUERY/`. The report itself quotes the released text under
`specs/openehr/QUERY/`, which stays the oracle.

| Patch | Report |
|---|---|
| `0761-archetype-predicate-and-top.patch` | #761 |
| `0771-precedence-regex-like-dates-comparisons-paths.patch` | #771 |

A patch is named after its report's number.
`scripts/checks/upstream-patches-apply.sh` checks on every pull request that
each one applies to the snapshot with
`git apply --check --directory=specs/openehr-development/specifications-QUERY`. Apply one in a checkout of
that repository at the pinned commit with `git apply <patch>`. A defect that
`master` already fixes carries no patch here, and its report records the
fixing commit.

These patches are never applied to `specs/openehr/QUERY/` or to the snapshot.
The release tree stays the released bytes, because every expectation in the
catalogue quotes the released text, defects included, and the report records
the defect.
