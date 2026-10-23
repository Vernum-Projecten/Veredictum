# Proposed fixes to openEHR ITS-REST

Each patch here is the fix proposed in one `upstream-report` issue, written
against [openEHR/specifications-ITS-REST](https://github.com/openEHR/specifications-ITS-REST)
at Release-1.1.0, commit `24058992d5fa96e8dfbd855d9c133f328387fc09`, which is
the commit `specs/openehr/ITS-REST/` vendors.

| Patch | Report |
|---|---|
| `0618-ehr-status-body-operations-declare-422.patch` | #618 |
| `0619-ehr-id-parameter-is-a-hier-object-id.patch` | #619 |
| `0620-etag-descriptions-name-the-weak-tag.patch` | #620 |
| `0621-ehr-status-example-carries-archetype-details.patch` | #621 |
| `0639-ehr-references-the-versioned-objects.patch` | #639 |

A patch is named after its report's number. Apply one in a checkout of that
repository with `git apply <patch>`.

These patches are never applied to `specs/openehr/ITS-REST/`. That tree stays
the released bytes, because every expectation in the catalogue quotes the
released text, defects included, and the report records the defect.
