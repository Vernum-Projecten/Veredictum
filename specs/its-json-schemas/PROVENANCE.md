# Vendored ITS-JSON schemas (provenance)

JSON Schemas from `openEHR/specifications-ITS-JSON`, vendored **verbatim** (the
contents of the upstream `components/` directory, at this directory's root).

Source repo: https://github.com/openEHR/specifications-ITS-JSON
Pinned commit: `5acae056248e917a4b4c56f7e712f4fcfeb616a6` (master; ITS-JSON is
DEVELOPMENT status with no numbered release — this is the latest available)
License: Apache-2.0. The upstream repo's `LICENSE` is vendored verbatim beside
this file, from the same pinned commit.
Fetched: 2026-07-04.

## Layout (the upstream `components/` contents)

- `RM/Release-1.0.3/`, `RM/Release-1.0.4/`, `RM/Release-1.1.0/`
- `BASE/Release-1.1.0/`
- `AM/Release-1.4/`, `AM/Release-2.1.0/`, `AM/Release-2.2.0/`
- Per package: individual `<TYPE>.json` files + a package `main.json`.
- Per RM version: a consolidated `openehr_rm_<version>_all.json` at this
  directory's root (e.g. `openehr_rm_1.1.0_all.json`).

784 schema files besides this one and the `LICENSE`. Schemas use `if…then` for `_type` polymorphism and
organize by package (draft-07).

## Role

Reference material for the canonical-JSON wire form the catalogue exercises.
The instrument reads none of these files at run time. The container image
carries the tree under `/work/specs/its-json-schemas/`.

ITS-JSON has no numbered release, so this tree is a development pin. It tops
out at RM 1.1.0, the RM release vendored under `specs/openehr/RM/`, and the
consolidated schemas are closed (`additionalProperties: false` per class), so
a document carrying an attribute a later RM release adds fails them.

`scripts/vendor/its-json-schemas.sh` refreshes the tree and its `LICENSE` from
the commit above, whose pin it carries; this file is hand-written, so a refresh
that moves the pin updates it in the same change.
