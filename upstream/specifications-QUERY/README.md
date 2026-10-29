# Proposed fixes to openEHR QUERY

Each patch here is the fix proposed in one `upstream-report` issue, written
against [openEHR/specifications-QUERY](https://github.com/openEHR/specifications-QUERY)
at commit `a87bb51fa1c515b863c9610a9444a2d5570dc05a`, which is the commit
`specs/openehr/QUERY/` vendors.

| Patch | Report |
|---|---|
| `0761-archetype-predicate-and-top.patch` | #761 |
| `0771-precedence-regex-like-dates-comparisons-paths.patch` | #771 |

A patch is named after its report's number. Apply one in a checkout of that
repository with `git apply <patch>`.

These patches are never applied to `specs/openehr/QUERY/`. That tree stays the
released bytes, because every expectation in the catalogue quotes the released
text, defects included, and the report records the defect.
