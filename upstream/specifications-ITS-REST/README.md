# Proposed fixes to openEHR ITS-REST

Each patch here is the fix proposed in one `upstream-report` issue, written
against the `master` branch of
[openEHR/specifications-ITS-REST](https://github.com/openEHR/specifications-ITS-REST), the newest
openEHR text and the line a fix lands on. The patches target `master` at
commit `9d25ea783f790512e05f908ef743626b4f7e8d2c`, the snapshot vendored under
`specs/openehr-development/specifications-ITS-REST/`. The report itself quotes the released text under
`specs/openehr/ITS-REST/`, which stays the oracle.

| Patch | Report |
|---|---|
| `0221-client-supplied-body-uid-on-content-commits.patch` | #221 |
| `0351-405-declares-allow.patch` | #351 |
| `0352-canonical-only-operations.patch` | #352 |
| `0355-directory-commits-declare-422.patch` | #355 |
| `0618-ehr-status-body-operations-declare-422.patch` | #618 |
| `0619-ehr-id-parameter-is-a-hier-object-id.patch` | #619 |
| `0620-etag-descriptions-name-the-weak-tag.patch` | #620 |
| `0621-ehr-status-example-carries-archetype-details.patch` | #621 |
| `0639-ehr-references-the-versioned-objects.patch` | #639 |
| `0649-ehr-create-with-id-subject-conflict.patch` | #649 |
| `0650-type-required-on-polymorphic-values.patch` | #650 |
| `0673-return-identifier-never-204.patch` | #673 |
| `0687-body-less-reads-declare-no-content-type.patch` | #687 |
| `0699-error-bodies-are-problem-details.patch` | #699 |
| `0709-default-ehr-status-name-and-archetype.patch` | #709 |
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
| `0759-xml-lineage.patch` | #759 |
| `0791-simplified-formats-tables-and-examples.patch` | #791 |
| `0899-query-store-operation-ids.patch` | #899 |
| `1008-direct-delete-lifecycle-deleted.patch` | #1008 |

A patch is named after its report's number.
`scripts/checks/upstream-patches-apply.sh` checks on every pull request that
each one applies to the snapshot with
`git apply --check --directory=specs/openehr-development/specifications-ITS-REST`. Apply one in a checkout of
that repository at the pinned commit with `git apply <patch>`. A defect that
`master` already fixes carries no patch here, and its report records the
fixing commit.

These patches are never applied to `specs/openehr/ITS-REST/` or to the snapshot.
The release tree stays the released bytes, because every expectation in the
catalogue quotes the released text, defects included, and the report records
the defect.
