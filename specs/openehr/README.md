# Vendored openEHR specifications (normative text)

The **authoritative openEHR spec text**, vendored verbatim from the official
`github.com/openEHR/specifications-*` repos at the pins in
`scripts/vendor/spec-docs.sh`. **This is the conformance authority for all work
in this repo:** when authoring or reviewing an expectation, read the relevant
section *here*. Do not rely on memory, on a vendor's documentation, or on what
a server did.

- Vendored by `scripts/vendor/spec-docs.sh`. The pins live in that script, and
  each component's `PROVENANCE.md` records its ref, commit and license, and is
  written by the script.
- Text formats only (`.adoc`, `.md`, `.txt`, `.csv`, `.json`, `.yaml`, `.yml`,
  `.robot`, `.xml`, `.opt`, `.g4`), **plus every figure the chapters actually
  reference**. The three figure attributes are defined in
  `openEHR/specifications-AA_GLOBAL`, `docs/boilerplate/global_vars.adoc`, all
  resolved relative to a component's `docs/` root, and the vendored mirror
  keeps that layout so the references resolve as published:

  | Attribute | Expands to |
  |---|---|
  | `:uml_diagrams_uri: UML/diagrams` | `docs/UML/diagrams/<name>.svg` |
  | `:diagrams_uri: {doc_name}/diagrams` | `docs/<doc_name>/diagrams/<name>` |
  | `:images_uri: {doc_name}/images` | `docs/<doc_name>/images/<name>` |

  `{doc_name}` is the document directory the referencing chapter lives in
  (`docs/common`, `docs/AOM2`, `docs/bmm`, …), so the per-document sets are
  derived per directory. Only referenced files are vendored, byte-for-byte
  from the same pinned commit as the text, and a reference with no file at the
  pin fails the vendoring run. Each component's `PROVENANCE.md` states how many
  figures it carries. Unreferenced figures, UML `.xmi`/`.mdzip`, XSDs and PDFs
  are excluded.
- The released machine-readable bundles sit beside this tree:
  `specs/rest-oas/` (the ITS-REST OpenAPI documents), `specs/its-xml-schemas/`
  (the ITS-XML XSDs) and `specs/its-json-schemas/` (the ITS-JSON schemas).
- Never hand-edit anything under `specs/openehr/` except this README.

## Map (component → what to read)

Spec documents are AsciiDoc books: `docs/<spec>/master.adoc` includes the
`master*.adoc` chapter files beside it. Grep the chapter files directly.

| Dir | Component (pin) | Key content |
|---|---|---|
| `BASE/` | BASE Release-1.2.0 | `docs/foundation_types/` (primitives, intervals, ISO 8601), `docs/base_types/` (identification: `OBJECT_VERSION_ID`, `ARCHETYPE_ID`, …), `docs/resource/`, `docs/architecture_overview/` |
| `RM/` | RM Release-1.1.0 | `docs/ehr/` (EHR, COMPOSITION, versioning semantics), `docs/common/` (LOCATABLE, VERSION, CONTRIBUTION, audit/attestation), `docs/data_types/`, `docs/data_structures/`, `docs/demographic/`, `docs/ehr_extract/`, `docs/integration/`, `docs/support/` (terminology-service interfaces) |
| `AM/` | AM Release-2.3.0 | `docs/ADL1.4/`, `docs/AOM1.4/`, `docs/ADL2/`, `docs/AOM2/`, `docs/OPT2/`, `docs/Identification/`, `docs/Overview/` |
| `QUERY/` | QUERY Release-1.1.0 | `docs/AQL/` (the AQL spec: grammar, operators, functions, result set), `docs/AQL_examples/` |
| `TERM/` | TERM Release-3.0.0 | `docs/SupportTerminology/` (the openEHR support terminology: code sets, term sets) |
| `LANG/` | LANG Release-1.0.0 | `docs/bmm/`, `docs/bmm_persistence/` (P_BMM), `docs/odin/`, `docs/expression_language/` |
| `CNF/` | CNF (master, frozen SHA; never released) | `docs/platform_test_schedule/` (**the Platform Conformance Test Schedule: a STALLED structural guide, never the correctness authority**), `docs/guide/`, `docs/profiles/`, `docs/certificate/`, `tests/platform/robot/` (the upstream Robot suite and its fixtures: stalled reference material, never an oracle) |
| `ITS-REST/` | ITS-REST Release-1.1.0 | `docs/` (the docs text, THE wire oracle) + `specifications/` (the OAS the released text presents as its computable artifacts; subordinate, it grounds an expectation only where the docs text is silent). All 7 API groups vendored |
| `ITS-XML/` | ITS-XML (master, frozen SHA) | the docs text, the README release directory and the per-release `components/**/README.adoc`; the XSDs themselves are vendored at `specs/its-xml-schemas/` |
| `ITS-JSON/` | ITS-JSON (master, frozen SHA; never released) | the docs text; the JSON schemas themselves are vendored at `specs/its-json-schemas/` |
| `REQUIREMENTS/` | release artifacts | the openEHR ISO 18308 conformance statement (a requirements-level reference, not an oracle) |

PROC (Process / Task Planning) is deliberately NOT vendored: openEHR publishes no releasable text for it.

SM (the Service Model) is deliberately NOT vendored either (#604): it was never released, so it is no part of the oracle, and ITS-REST binds every operation to the wire on its own.

ITS-BMM is deliberately not vendored: no expectation in the catalogue rests on
a BMM file.

## How to use (agents + humans)

1. Identify the owning component (RM behaviour → `RM/`, REST wire → `ITS-REST/`,
   AQL semantics → `QUERY/`, validation/constraints → `AM/` + `RM/`).
2. Grep the class/attribute/endpoint name across that component's `docs/`
   chapter files; read the surrounding section, including invariants and
   inherited-class semantics.
3. For behaviour the CNF schedule covers, the matching
   `CNF/docs/platform_test_schedule/master*.adoc` chapter says WHICH behaviour
   to test. The correct answer is re-derived from the released component,
   never from the schedule's own assertion or the Robot suite.
4. Cite the spec section (file + heading) in the expectation's
   `x-veredictum-cite`, and in a code comment as a `// NOTE:` carrying that
   citation; anything still missing is a `// TODO(#NNNN):` naming its issue.
