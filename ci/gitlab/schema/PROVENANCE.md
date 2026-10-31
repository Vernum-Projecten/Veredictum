# Vendored GitLab CI/CD configuration schema

Source: <https://gitlab.com/gitlab-org/gitlab/-/raw/28aa2bf90771a11ecc2f86525bc8a70a49c4d810/app/assets/javascripts/editor/schema/ci.json>,
the JSON Schema GitLab's pipeline editor validates a `.gitlab-ci.yml` with, at
the newest commit that touched it when it was fetched.
Upstream repository: <https://gitlab.com/gitlab-org/gitlab>.
SHA-256: `1c6eeb7d3a34c39996afbfe1ade220480eb05bbeae674e86d3c745dfeeded7fd`.
Fetched: 2026-10-31 by `scripts/vendor/gitlab-ci-schema.sh`, which refuses
bytes with any other digest.

License: MIT Expat. The repository's `LICENSE` at the same commit is vendored
beside the schema as `LICENSE`, pinned by its own SHA-256; it licenses every
path outside `doc/`, `ee/` and `jh/` under the MIT Expat license, and this
file sits outside all three. `REUSE.toml` records it.

The file is byte-for-byte what the URL served. It is test material only: the
`gitlab_template` integration test validates `ci/gitlab/veredictum.yml`
against it, once with its default inputs and once with every input set, after
substituting each `$[[ inputs.* ]]` interpolation the way GitLab does. Nothing
ships it.
