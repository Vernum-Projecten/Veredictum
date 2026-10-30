# Authoring the catalogue

<!-- toc -->

The catalogue is data, not code. You can read it, diff it, and add to it without
touching the runner. It is written in the OpenAPI Initiative's
[Arazzo Specification 1.1.0](https://spec.openapis.org/arazzo/v1.1.0.html): a
case is an Arazzo workflow, and its steps call the operations of the released
openEHR REST API OpenAPI documents by `operationId`. Any Arazzo tool reads the
files. This chapter is an overview of their shape and the rules an author
works under.

The formats this instrument adds are published as JSON Schema in
[`schemas/`](https://github.com/Vernum-Projecten/Veredictum/tree/main/schemas). Those
files are emitted by `veredictum emit-schemas` and drift-tested against it, so
the published format and the code that reads it are one thing. The design
record is
[`docs/ARCHITECTURE.md`](https://github.com/Vernum-Projecten/Veredictum/blob/main/docs/ARCHITECTURE.md)
§15; this page will not repeat every field.

## The layout

Everything lives under `catalogue/` in the repository.

| Path | What it holds |
|---|---|
| `cases/<capability>/<topic>.arazzo.yaml` | The workflows, one Arazzo document per topic. The folder is the capability a statement claims |
| `cases/<capability>/<workflowId>.rows.yaml` | A data-driven workflow's rows, beside its document |
| `cases/_setup/` | Shared setups. A setup is not a case: it runs only where a case names it in `dependsOn` |
| `capabilities.yaml` | The options: behaviour the released text leaves to the implementation, each with the arms a statement picks from |
| `sources/` | The routes no openEHR OpenAPI document describes, in one document of our own |
| `keys/` | The test signing material a run can be given: the OpenPGP pair a server signs VERSIONs with, and the SMART test issuer the runner mints tokens with. Public by design and never for real use |
| `id-map.yaml` | Every case id of the former case-core catalogue, mapped to the workflows that carry it |

There are 15 capability folders: `ehr`, `ehr_status`, `composition`,
`content`, `contribution`, `directory`, `item_tags`, `query`, `definition`,
`demographic`, `admin`, `security`, `simplified_formats`, `smart` and `system`.

## A workflow

One workflow, one behaviour. A case id is `<document>/<workflowId>`, for
example `ehr/create_ehr-same_ehr_twice-with_id`, and it is never reused.

- **Steps** call an operation by its qualified
  `$sourceDescriptions.<name>.<operationId>`, with the parameters and request
  body the behaviour needs, and name the outputs a later step reads.
- **`dependsOn`** names the shared setups the workflow needs, such as an EHR
  to commit into. Nothing set up is carried from one workflow to the next.
- **`successCriteria`** are the checks. Status codes and counts use `simple`,
  field values `jsonpath` or `regex`. An `xpath` criterion is refused until a
  vetted XPath engine judges it.
- **`x-veredictum-cite`** is the citation, on a workflow, a step or a
  criterion: the vendored file, the section, and the sentence quoted verbatim.
  `validate` refuses a criterion that resolves to no citation and a quote that
  does not occur in the named file. The file may also be a released ITS-JSON
  or ITS-XML schema, and a JSON Pointer to a number, a boolean or `null` in a
  JSON or YAML file is quoted as that value. A quote that recommends rather
  than requires carries `level: should`, and a row that fails only such
  criteria is a warning: it never counts against a capability. A quote that
  only permits carries `level: may`. A quote that says "should", "can" or
  "may" in lower case and nothing that requires needs its level stated.
- **`x-veredictum-check`** names one of the judges a plain condition cannot
  express: `result_set` (AQL result equality), `equivalent` (two RM documents
  equal apart from a named set of server-assigned fields), `signature`,
  `schema` and `json_type` (the JSON type of a value).
- **`x-veredictum-derive`** takes part of a version identifier, such as the
  versioned object id out of a version uid, which no Arazzo expression can.

## Rows files

A workflow with `inputs` runs once per row of its rows file. The expected
outcome belongs to the workflow, not the row: rows that share an outcome share
a workflow. A rows file may write cells in the decision-table literal grammar,
build request bodies with a named recipe, bind an input to a value the
party's IXIT declares (`declared`), and state a property of the whole run
(`aggregate`). A row whose declared value is missing is skipped as
`not-declared`.

## Options

Where the released text lets a server choose, for example whether it serves
canonical XML, both answers are conformant and neither may be asserted
unconditionally. `capabilities.yaml` names each such option, its arms and the
documents that test each arm, with the sentence that leaves the choice open.
A statement picks one arm of each option, and the documents of every other
arm are skipped as `option-not-selected`.

## The rules an author works under

1. **Cite the released specification.** An expectation without a resolving
   citation is a finding. The openEHR Conformance component's test schedule is a
   guide to which behaviours are worth covering, and it is not authority for what
   the correct answer is.
2. **One behaviour per workflow.** If a failure of your workflow could mean two
   different defects, split it.
3. **Add cases; never remove one to go green.** A removal that makes a run pass
   is a narrowed claim, not a fix.
4. **Never adjust an expectation to match an observed response.** Change it only
   with a new citation that says the old reading was wrong.
5. **Record a specification silence, do not resolve it.** No workflow asserts
   the silent behaviour. The silence is an `upstream-report` issue with a
   proposed patch, or an option where the release leaves the choice to the
   implementation, and a private resolution makes a harness non-conformant.
6. **Keep the invalid twin.** Where a server is right to refuse a shape, the
   invalid body stays as its own refusal workflow beside the accepting one, so
   a lenient server fails it. Removing it narrows the claim without changing
   any visible count, which is why it does not happen.

## Checking your work

```bash
veredictum validate --root catalogue --specs specs/openehr
```

Zero findings is the only passing result, and the gates behind that line are in
[the command reference](commands.md#validate).

Run it before you drive anything against a server. A catalogue defect found by a
server run costs a great deal more to diagnose than the same defect found by
`validate` in a second.
