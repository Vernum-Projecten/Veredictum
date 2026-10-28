# Proposed fixes to openEHR RM

Each patch here is the fix proposed in one `upstream-report` issue, written
against [openEHR/specifications-RM](https://github.com/openEHR/specifications-RM)
at Release-1.1.0, commit `355eb63e201c6b805c44c8809c70d63f79d54f92`, which is the commit `specs/openehr/RM/` vendors.

| Patch | Report |
|---|---|
| `0717-partial-temporal-magnitude.patch` | #717 |
| `0718-coded-text-value-is-rubric.patch` | #718 |
| `0760-rm-slips.patch` | #760 |

A patch is named after its report's number. Apply one in a checkout of that
repository with `git apply <patch>`.

These patches are never applied to `specs/openehr/RM/`. That tree stays the
released bytes, because every expectation in the catalogue quotes the released
text, defects included, and the report records the defect.
