# Proposed fixes to openEHR ITS-JSON

Each patch here is the fix proposed in one `upstream-report` issue, written
against the `master` branch of
[openEHR/specifications-ITS-JSON](https://github.com/openEHR/specifications-ITS-JSON), the newest
openEHR text and the line a fix lands on. The patches target `master` at
commit `14387ce9191f228d768b6bade3308b79aaf87471`, the snapshot vendored under
`specs/openehr-development/specifications-ITS-JSON/`. The report itself quotes the released text under
`specs/its-json-schemas/`, which stays the oracle.

| Patch | Report |
|---|---|
| `0724-uri-value-and-interval-limits.patch` | #724 |
| `0741-party-and-contribution-uid-not-required.patch` | #741 |

A patch is named after its report's number.
`scripts/checks/upstream-patches-apply.sh` checks on every pull request that
each one applies to the snapshot with
`git apply --check --directory=specs/openehr-development/specifications-ITS-JSON`. Apply one in a checkout of
that repository at the pinned commit with `git apply <patch>`. A defect that
`master` already fixes carries no patch here, and its report records the
fixing commit.

These patches are never applied to `specs/its-json-schemas/` or to the snapshot.
The release tree stays the released bytes, because every expectation in the
catalogue quotes the released text, defects included, and the report records
the defect.
