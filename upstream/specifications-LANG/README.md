# Proposed fixes to openEHR LANG

Each patch here is the fix proposed in one `upstream-report` issue, written
against the `master` branch of
[openEHR/specifications-LANG](https://github.com/openEHR/specifications-LANG), the newest
openEHR text and the line a fix lands on. The patches target `master` at
commit `46406fe57c606aba7c0c7a9fd60a31bc7749c175`, the snapshot vendored under
`specs/openehr-development/specifications-LANG/`. The report itself quotes the released text under
`specs/openehr/LANG/`, which stays the oracle.

| Patch | Report |
|---|---|
| `0211-non-generic-descendant-conforms-to-generic-ancestor.patch` | #211 |

A patch is named after its report's number.
`scripts/checks/upstream-patches-apply.sh` checks on every pull request that
each one applies to the snapshot with
`git apply --check --directory=specs/openehr-development/specifications-LANG`. Apply one in a checkout of
that repository at the pinned commit with `git apply <patch>`. A defect that
`master` already fixes carries no patch here, and its report records the
fixing commit.

These patches are never applied to `specs/openehr/LANG/` or to the snapshot.
The release tree stays the released bytes, because every expectation in the
catalogue quotes the released text, defects included, and the report records
the defect.
