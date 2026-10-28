# Proposed fixes to openEHR ITS-JSON

Each patch here is the fix proposed in one `upstream-report` issue, written
against [openEHR/specifications-ITS-JSON](https://github.com/openEHR/specifications-ITS-JSON)
at master, commit `5acae056248e917a4b4c56f7e712f4fcfeb616a6`, which is the commit `specs/its-json-schemas/` vendors.

| Patch | Report |
|---|---|
| `0724-uri-value-and-interval-limits.patch` | #724 |
| `0741-party-and-contribution-uid-not-required.patch` | #741 |

A patch is named after its report's number. Apply one in a checkout of that
repository with `git apply <patch>`.

These patches are never applied to `specs/its-json-schemas/`. That tree stays the
released bytes, because every expectation in the catalogue quotes the released
text, defects included, and the report records the defect.
