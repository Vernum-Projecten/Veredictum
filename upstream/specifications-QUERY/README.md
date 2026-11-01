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
| `0159-containment-follows-versioned-object-references.patch` | #159 |
| `0160-containment-at-any-depth.patch` | #160 |
| `0161-containment-pair-without-rm-relationship.patch` | #161 |
| `0195-where-over-a-multi-valued-path.patch` | #195 |
| `0701-select-rows-over-a-multi-valued-path.patch` | #701 |
| `0702-class-expression-matches-descendants.patch` | #702 |
| `0703-class-expression-variable-and-predicate-optional.patch` | #703 |
| `0704-or-and-not-contains-rows.patch` | #704 |
| `0705-grammatical-queries-the-rm-does-not-define.patch` | #705 |
| `0706-class-expressions-name-locatable-classes.patch` | #706 |
| `0707-aggregate-function-group.patch` | #707 |
| `0708-logically-deleted-object-not-matched.patch` | #708 |
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
