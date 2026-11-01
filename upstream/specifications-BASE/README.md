# Proposed fixes to openEHR BASE

Each patch here is the fix proposed in one `upstream-report` issue, written
against the `master` branch of
[openEHR/specifications-BASE](https://github.com/openEHR/specifications-BASE), the newest
openEHR text and the line a fix lands on. The patches target `master` at
commit `93abaf9f656b7bc87019458303c9d88f3f688ef8`, the snapshot vendored under
`specs/openehr-development/specifications-BASE/`. The report itself quotes the released text under
`specs/openehr/BASE/`, which stays the oracle.

| Patch | Report |
|---|---|
| `0249-utc-timezone-representable.patch` | #249 |
| `0716-interval-limit-matches-its-flag.patch` | #716 |
| `0762-timezone-form-matches-value.patch` | #762 |

A patch is named after its report's number.
`scripts/checks/upstream-patches-apply.sh` checks on every pull request that
each one applies to the snapshot with
`git apply --check --directory=specs/openehr-development/specifications-BASE`. Apply one in a checkout of
that repository at the pinned commit with `git apply <patch>`. A defect that
`master` already fixes carries no patch here, and its report records the
fixing commit.

These patches are never applied to `specs/openehr/BASE/` or to the snapshot.
The release tree stays the released bytes, because every expectation in the
catalogue quotes the released text, defects included, and the report records
the defect.
