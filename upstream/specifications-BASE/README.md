# Proposed fixes to openEHR BASE

Each patch here is the fix proposed in one `upstream-report` issue, written
against [openEHR/specifications-BASE](https://github.com/openEHR/specifications-BASE)
at Release-1.2.0, commit `fba868df1fad9fe15ae0483f3e3ac1c3fa94d735`, which is the commit `specs/openehr/BASE/` vendors.

| Patch | Report |
|---|---|
| `0716-interval-limit-matches-its-flag.patch` | #716 |
| `0762-timezone-form-matches-value.patch` | #762 |

A patch is named after its report's number. Apply one in a checkout of that
repository with `git apply <patch>`.

These patches are never applied to `specs/openehr/BASE/`. That tree stays the
released bytes, because every expectation in the catalogue quotes the released
text, defects included, and the report records the defect.
