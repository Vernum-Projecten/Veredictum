# Vendored openEHR ITS-REST OpenAPI (OAS)

Source repo: https://github.com/openEHR/specifications-ITS-REST
Pinned commit: `24058992d5fa96e8dfbd855d9c133f328387fc09` (tag Release-1.1.0, released 19-Jul-2026)
Upstream path: `computable/OAS/` · Fetched: 2026-07-20.
License: Apache-2.0. The upstream repo's `LICENSE` is vendored verbatim beside
this file, from the same pinned commit. Each bundle's own `info.license`
stanza contradicts that grant: all 21 declare CC-BY-ND-3.0 (name and URL
agreeing). We take the files under the repository `LICENSE`, which is what
`REUSE.toml` records.

`scripts/vendor/rest-oas.sh` refreshes the tree and its `LICENSE` from the
commit above. It is the ITS-REST pin of `scripts/vendor/spec-docs.sh`, so the
OAS and the docs text under `specs/openehr/ITS-REST/` are one release. This
file is hand-written, so a refresh that moves the pin updates it in the same
change.

All **21** OpenAPI 3.0 bundles are vendored verbatim: **7 API groups** ×
**3 variants**.

- **Groups:** `admin`, `definition`, `demographic`, `ehr`, `overview`,
  `query`, `system`.
- **Variants:**
  - `-codegen.openapi.yaml` — authored upstream for code generation.
  - `-html.openapi.yaml` — for documentation rendering (Redocly).
  - `-validation.openapi.yaml` — for request/response validation. The
    catalogue's Arazzo documents name these as their source descriptions,
    and their citations quote them.

These are hand-authored, self-contained bundles (openEHR authors the OAS by
hand and publishes these variants). The instrument reads them; it generates no
code from them and emits no OAS of its own.
