# Proposed fixes to openEHR AM

Each patch here is the fix proposed in one `upstream-report` issue, written
against the `master` branch of
[openEHR/specifications-AM](https://github.com/openEHR/specifications-AM), the newest
openEHR text and the line a fix lands on. The patches target `master` at
commit `15d7a3589d52e33aebd5efb97c321b2498407d6b`, the snapshot vendored under
`specs/openehr-development/specifications-AM/`. The report itself quotes the released text under
`specs/openehr/AM/`, which stays the oracle.

| Patch | Report |
|---|---|
| `0927-opt-specialisation-statement.patch` | #927 |

A patch is named after its report's number.
`scripts/checks/upstream-patches-apply.sh` checks on every pull request that
each one applies to the snapshot with
`git apply --check --directory=specs/openehr-development/specifications-AM`. Apply one in a checkout of
that repository at the pinned commit with `git apply <patch>`. A defect that
`master` already fixes carries no patch here, and its report records the
fixing commit.

These patches are never applied to `specs/openehr/AM/` or to the snapshot.
The release tree stays the released bytes, because every expectation in the
catalogue quotes the released text, defects included, and the report records
the defect.
