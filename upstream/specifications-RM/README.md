# Proposed fixes to openEHR RM

Each patch here is the fix proposed in one `upstream-report` issue, written
against the `master` branch of
[openEHR/specifications-RM](https://github.com/openEHR/specifications-RM), the newest
openEHR text and the line a fix lands on. The patches target `master` at
commit `654296bb027b03b4b076101e7171f3359f44cebc`, the snapshot vendored under
`specs/openehr-development/specifications-RM/`. The report itself quotes the released text under
`specs/openehr/RM/`, which stays the oracle.

| Patch | Report |
|---|---|
| `0215-is-modifiable-in-a-contribution.patch` | #215 |
| `0240-revision-history-most-recent-last.patch` | #240 |
| `0353-archetype-node-id-root-point.patch` | #353 |
| `0717-partial-temporal-magnitude.patch` | #717 |
| `0718-coded-text-value-is-rubric.patch` | #718 |
| `0760-rm-slips.patch` | #760 |
| `1053-target-valid-negation.patch` | #1053 |
| `1048-reverse-relationships-invariant.patch` | #1048 |

A patch is named after its report's number.
`scripts/checks/upstream-patches-apply.sh` checks on every pull request that
each one applies to the snapshot with
`git apply --check --directory=specs/openehr-development/specifications-RM`. Apply one in a checkout of
that repository at the pinned commit with `git apply <patch>`. A defect that
`master` already fixes carries no patch here, and its report records the
fixing commit.

These patches are never applied to `specs/openehr/RM/` or to the snapshot.
The release tree stays the released bytes, because every expectation in the
catalogue quotes the released text, defects included, and the report records
the defect.
