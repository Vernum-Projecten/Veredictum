# Proposed fixes to openEHR ITS-REST

Each patch here is the fix proposed in one `upstream-report` issue, written
against [openEHR/specifications-ITS-REST](https://github.com/openEHR/specifications-ITS-REST)
at Release-1.1.0, commit `24058992d5fa96e8dfbd855d9c133f328387fc09`, which is
the commit `specs/openehr/ITS-REST/` vendors.

| Patch | Report |
|---|---|
| `0352-folder-and-ehr-status-are-canonical-only.patch` | #352 |
| `0352-party-kinds-are-canonical-only.patch` | #352 |
| `0618-ehr-status-body-operations-declare-422.patch` | #618 |
| `0619-ehr-id-parameter-is-a-hier-object-id.patch` | #619 |
| `0620-etag-descriptions-name-the-weak-tag.patch` | #620 |
| `0621-ehr-status-example-carries-archetype-details.patch` | #621 |
| `0639-ehr-references-the-versioned-objects.patch` | #639 |
| `0699-error-bodies-are-problem-details.patch` | #699 |
| `0715-adl14-template-id-version.patch` | #715 |
| `0721-simplified-formats-mapping-tables.patch` | #721 |
| `0732-party-creates-declare-no-404.patch` | #732 |
| `0735-admin-ehr-delete-all-subset-parameter.patch` | #735 |
| `0736-system-options-tag.patch` | #736 |
| `0744-update-version-signature-semantics.patch` | #744 |
| `0750-item-tag-refusals-filters-identity.patch` | #750 |
| `0751-contribution-commit-shapes.patch` | #751 |
| `0752-versioned-content-reads-and-writes.patch` | #752 |
| `0753-directory-branches-and-path.patch` | #753 |
| `0754-query-execution-and-stored-queries.patch` | #754 |
| `0755-template-upload-semantic-refusal.patch` | #755 |
| `0756-http-methods-location.patch` | #756 |
| `0757-admin-delete-effect.patch` | #757 |
| `0758-smart-discovery-and-scopes.patch` | #758 |
| `0759-simplified-other-rejections.patch` | #759 |

A patch is named after its report's number. Apply one in a checkout of that
repository with `git apply <patch>`.

These patches are never applied to `specs/openehr/ITS-REST/`. That tree stays
the released bytes, because every expectation in the catalogue quotes the
released text, defects included, and the report records the defect.
