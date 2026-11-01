# Authoring the catalogue

<!-- toc -->

The catalogue is data in
[Arazzo 1.1.0](https://spec.openapis.org/arazzo/v1.1.0.html): you can read,
diff and extend it without touching the runner, and any Arazzo tool reads it.
A case is a workflow whose steps call operations of the released openEHR REST
API OpenAPI documents by `operationId`. The formats Veredictum adds are JSON
Schemas in
[`schemas/`](https://github.com/Vernum-Projecten/Veredictum/tree/main/schemas),
emitted by `veredictum emit-schemas` and drift-tested. The full design is
[`docs/ARCHITECTURE.md`](https://github.com/Vernum-Projecten/Veredictum/blob/main/docs/ARCHITECTURE.md)
§15.

## The layout

Everything lives under `catalogue/`.

| Path | What it holds |
|---|---|
| `cases/<capability>/<topic>.arazzo.yaml` | The workflows, one Arazzo document per topic. The folder is the capability a statement claims |
| `cases/<capability>/<workflowId>.rows.yaml` | A data-driven workflow's rows, beside its document |
| `cases/_setup/` | Shared setups. A setup is not a case: it runs only where a case names it in `dependsOn` |
| `capabilities.yaml` | The options: behaviour the released text leaves to the implementation, each with the arms a statement picks from |
| `sources/` | The routes no openEHR OpenAPI document describes, in one document of our own |
| `keys/` | Test signing material: the OpenPGP pair a server signs VERSIONs with, and the SMART test issuer the runner mints tokens with. Public by design, never for real use |
| `id-map.yaml` | Every case id of the former case-core catalogue, mapped to the workflows that carry it |

There are 15 capability folders: `ehr`, `ehr_status`, `composition`,
`content`, `contribution`, `directory`, `item_tags`, `query`, `definition`,
`demographic`, `admin`, `security`, `simplified_formats`, `smart` and `system`.

## A workflow

One workflow, one behaviour. A case id is `<document>/<workflowId>`, for
example `ehr/create_ehr-same_ehr_twice-with_id`, and is never reused.

- **Steps:** call an operation by its qualified
  `$sourceDescriptions.<name>.<operationId>` with the parameters and body the
  behaviour needs, and name the outputs a later step reads.
- **`dependsOn`:** the shared setups the workflow needs, such as an EHR to
  commit into. Nothing set up carries over between workflows.
- **`successCriteria`:** the checks. Status codes and counts use `simple`,
  field values `jsonpath` or `regex`. `xpath` is refused until a vetted XPath
  engine judges it.
- **`x-veredictum-cite`:** the citation, on a workflow, step or criterion: the
  vendored file, the section, and the sentence quoted verbatim. `validate`
  refuses a criterion with no resolving citation and a quote absent from the
  named file. The file may be a released ITS-JSON or ITS-XML schema; a JSON
  Pointer to a number, boolean or `null` in JSON or YAML is quoted as that
  value. Levels:
  - `level: should` for a recommending quote. A row failing only such
    criteria is a warning and never counts against a capability.
  - `level: may` for a quote that only permits.
  - A quote with a lower-case "should", "can" or "may" and nothing that
    requires must state its level.
- **`x-veredictum-check`:** a judge a plain condition cannot express:
  `result_set` (AQL result equality), `equivalent` (two RM documents equal
  apart from named server-assigned fields), `signature`, `schema` and
  `json_type` (the JSON type of a value).
- **`x-veredictum-derive`:** takes part of a version identifier, such as the
  versioned object id out of a version uid, which no Arazzo expression can.

## Rows files

A workflow with `inputs` runs once per row of its rows file. The expected
outcome belongs to the workflow, so rows sharing an outcome share a workflow.
A rows file may write cells in the decision-table literal grammar, build
request bodies with a named recipe, bind an input to a value the party's IXIT
declares (`declared`), and state a property of the whole run (`aggregate`). A
row whose declared value is missing is skipped as `not-declared`.

## Options

Where the released text lets a server choose, such as serving canonical XML,
both answers are conformant and neither may be asserted unconditionally.
`capabilities.yaml` names each option, its arms, the documents testing each
arm, and the sentence that leaves the choice open. A statement picks one arm
per option; the other arms' documents are skipped as `option-not-selected`.

## The rules an author works under

1. **Cite the released specification.** An expectation without a resolving
   citation is a finding. The Conformance component's test schedule guides
   coverage, never the correct answer.
2. **One behaviour per workflow.** If a failure could mean two defects, split.
3. **Add cases; never remove one to go green.**
4. **Never fit an expectation to an observed response.** Change it only with a
   new citation showing the old reading was wrong.
5. **Record a silence, never resolve it.** No workflow asserts it: it becomes
   an `upstream-report` issue with a proposed patch, or an option where the
   release leaves the choice open. A private resolution makes a harness
   non-conformant.
6. **Keep the invalid twin.** A shape a server is right to refuse stays as its
   own refusal workflow beside the accepting one, so a lenient server fails.
   Removing it narrows the claim without changing any visible count.

## Checking your work

```bash
veredictum validate --root catalogue --specs specs/openehr
```

Zero findings is the only passing result; the gates are in
[the command reference](commands.md#validate). Run it before driving any
server: it finds a catalogue defect in a second that a server run makes
expensive to diagnose.
