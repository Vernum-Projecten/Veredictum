// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The JSON Schemas this crate publishes for the Arazzo catalogue: the two
//! specification extensions, the condition profile's static constraints, the
//! rows files and the surface-coverage file.
//!
//! No openEHR spec governs these — our own design. Each schema is also the
//! validator the matching gate loads its files through, so the published
//! norm and the gate cannot disagree. Every closed vocabulary is derived from
//! the enum the gate reads it into.

use serde_json::{Value, json};

use crate::arazzo::cite::{CITE_KEY, EXCLUDED_ROOT, ORACLE_ROOTS};
use crate::arazzo::control::CONTROL_KEY;
use crate::arazzo::derive::DERIVE_KEY;
use crate::arazzo::document::CHECK_KEY;
use crate::arazzo::gates::profile::ProfileHeader;
use crate::arazzo::gates::rows::ServerOwnership;
use crate::arazzo::gates::surface::NegotiationHeader;
use crate::arazzo::principal::PRINCIPAL_KEY;
use crate::arazzo::recipe::{ContentClass, SynthesizedClass};
use crate::arazzo::scope::SCOPE_KEY;
use crate::arazzo::structure::{ReportState, SECTION_KEY, Theme, Tier};
use crate::vocab::{FormatName, Iteration, SpecComponent};

/// `MAJOR.MINOR.PATCH`, the form of a release version an `applies` range
/// bounds.
pub const VERSION_PATTERN: &str = "^[0-9]+\\.[0-9]+\\.[0-9]+$";

const DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// The `$id` of one published Arazzo-catalogue schema.
fn urn(name: &str) -> String {
    format!(
        "urn:openehr:cnf:schema:{name}:{}",
        crate::schema::SCHEMA_VERSION
    )
}

/// `[A-Za-z0-9_\-]+`, the `workflowId` form a case id is built from.
pub const WORKFLOW_ID_PATTERN: &str = "^[A-Za-z0-9_\\-]+$";

/// `<document>/<workflowId>`, a case id.
pub const CASE_ID_PATTERN: &str = "^[A-Za-z0-9_\\-]+/[A-Za-z0-9_\\-]+$";

/// The serde tokens of a closed vocabulary.
fn tokens<T: serde::Serialize>(all: &[T]) -> Vec<Value> {
    all.iter()
        .map(|v| serde_json::to_value(v).unwrap_or(Value::Null))
        .collect()
}

/// One `x-veredictum-check` judge: `judge` fixed, `context` required, and
/// exactly the arguments [`crate::arazzo::check::Check`] reads.
fn judge_def(judge: &str, required: &[&str], properties: &Value) -> Value {
    let mut all = json!({
        "judge": { "const": judge },
        "context": {
            "description": "The runtime expression the judged value is read from.",
            "type": "string",
            "pattern": "^\\$"
        }
    });
    if let (Some(all), Some(more)) = (all.as_object_mut(), properties.as_object()) {
        all.extend(more.clone());
    }
    let mut needed = vec!["judge", "context"];
    needed.extend_from_slice(required);
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": needed,
        "properties": all
    })
}

/// The `x-veredictum-check` object: one of the judges. A string value
/// starting with `$` is a runtime expression; any other value is a literal.
fn check_def() -> Value {
    let expression_or =
        |literal: Value| json!({ "anyOf": [{ "type": "string", "pattern": "^\\$" }, literal] });
    json!({
        "oneOf": [
            judge_def("result_set", &["match"], &json!({
                "match": { "enum": tokens(crate::vocab::ResultSetMatch::ALL) },
                "rows": expression_or(json!({ "type": "array", "items": { "type": "array" } })),
                "count": { "type": "integer", "minimum": 0 },
                "columns": { "type": "array", "items": { "type": "string" } },
                "cells": { "enum": tokens(crate::vocab::CellComparison::ALL) }
            })),
            judge_def("equivalent", &["to"], &json!({
                "to": { "description": "The document the context must equal: a literal, or a runtime expression." },
                "ignoring": {
                    "description": "RM paths ignored on both sides; `**` matches any depth.",
                    "type": "array",
                    "items": { "type": "string", "minLength": 1 }
                }
            })),
            judge_def("signature", &[], &json!({
                "present": { "type": "boolean" },
                "verifiable": { "type": "boolean" },
                "equals": { "type": "string" },
                "distinct_from": { "type": "string" }
            })),
            judge_def("json_type", &["type"], &json!({
                "type": {
                    "description": "The JSON type the context's value must be, one of the six of RFC 8259 §3; an absent value is of no type, and a present `null` is of the type `null`.",
                    "enum": tokens(crate::arazzo::check::JsonType::ALL)
                }
            })),
            {
                "description": "The step's response body validates against the schema its operation declares for the served status and media type. It runs only where a criterion asks for it, and that criterion cites the OAS element.",
                "type": "object",
                "additionalProperties": false,
                "required": ["judge", "against"],
                "properties": {
                    "judge": { "const": "schema" },
                    "against": { "enum": tokens(crate::arazzo::check::SchemaSource::ALL) }
                }
            }
        ]
    })
}

/// A citation's `level`.
/// The `development` member of a citation: the snapshot's wording of a
/// reworded section.
fn development_def() -> Value {
    json!({
        "description": "Where the development snapshot states the same requirement in other words. Read under the development specification profile only, in place of `section` and `quote`, and never under the released profile. `validate --profile development` refuses it when its quote does not occur in the snapshot, and when the released quote still occurs there unchanged.",
        "type": "object",
        "additionalProperties": false,
        "required": ["section", "quote"],
        "properties": {
            "source": {
                "description": "The snapshot file, relative to the repository root, when it is not the file `source` maps to.",
                "type": "string",
                "pattern": "^specs/openehr-development/"
            },
            "section": {
                "description": "The element or the section heading in the snapshot, as `section` names one.",
                "type": "string",
                "minLength": 1
            },
            "quote": {
                "description": "The snapshot's sentence, copied verbatim.",
                "type": "string",
                "minLength": 1
            }
        }
    })
}

fn level_def() -> Value {
    json!({
        "description": "The requirement level of the quote: `must` (the default) for MUST, SHALL, REQUIRED or a definition, `should` for SHOULD or RECOMMENDED, `may` for MAY or OPTIONAL. A quote with no upper-case keyword is read by the same words in any other case, with \"can\" as a permission. `validate` refuses a level its quote's upper-case keywords disagree with, a stated `should` or `may` its quote does not say, and a citation that leaves the default `must` while its quote says \"should\", \"can\" or \"may\" and no requiring word; a stated `must` records that such a sentence requires in other words. A row whose failing criteria all cite `should` is failed and a warning, and never counts against a capability; a `may` criterion counts like a `must` one (RFC 2119 §5).",
        "enum": tokens(crate::arazzo::cite::Level::ALL)
    })
}

/// A citation inside the capabilities file: the required members, with the
/// rest of the shape left to the `arazzo-extensions` cite definition the
/// gate checks it by.
fn capabilities_cite_def() -> Value {
    json!({
        "description": "The citation, in the `arazzo-extensions` cite shape.",
        "type": "object",
        "required": ["source", "section", "quote"],
        "properties": { "level": level_def() }
    })
}

/// The `x-veredictum-cite`, `x-veredictum-check`, `x-veredictum-derive`,
/// `x-veredictum-principal`, `x-veredictum-scope` and `x-veredictum-control`
/// extension objects.
#[must_use]
pub fn extensions_schema() -> Value {
    let roots: Vec<String> = ORACLE_ROOTS
        .iter()
        .map(|root| regex_escape_prefix(root))
        .collect();
    json!({
        "$schema": DRAFT,
        "$id": urn("arazzo-extensions"),
        "title": "The Arazzo catalogue's seven specification extensions",
        "description": format!("`{CITE_KEY}` sits on a workflow, a step or a criterion; a criterion without one inherits its step's, and a step its workflow's. `{CHECK_KEY}` sits on a criterion only, for the judgements no standard Arazzo criterion can express. `{DERIVE_KEY}` sits on a step only, and names new step outputs, each one function of BASE OBJECT_VERSION_ID applied to one of the step's own outputs; `validate` also refuses an `of` the step does not declare and a derived name that collides with a declared output. `{PRINCIPAL_KEY}` sits on a step only, and names the principal the step is sent as instead of the run's ordinary credential; a run whose party supplies no credential for that principal skips every workflow with such a step with the reason `not-declared`. `{SCOPE_KEY}` sits on a step only, never beside `{PRINCIPAL_KEY}`, and names the scope set of the access token the step presents; a row whose scope set the party's IXIT covers with no token is skipped with the reason `not-declared`. `{CONTROL_KEY}` sits on a workflow only, and every refusal workflow, one with a criterion `$statusCode == 400` or `$statusCode == 422`, carries one: it names the workflow whose request is the same apart from the one defect, and a refusal row whose control was not accepted in the same run is errored; `validate` refuses a control on any other workflow and one that names no workflow, the workflow itself or another refusal workflow. `{SECTION_KEY}` sits on a document's `info` only, and names the BASE section of its capability folder that `catalogue/chapters.yaml` declares. `validate` also checks that a citation's quote occurs, after whitespace normalization, inside the named section of the named vendored file, and that the file belongs to no document the release marks RETIRED, which no schema can state."),
        "$defs": {
            "cite": {
                "type": "object",
                "additionalProperties": false,
                "required": ["source", "section", "quote"],
                "properties": {
                    "source": {
                        "description": "The vendored released file, relative to the repository root.",
                        "type": "string",
                        "pattern": format!("^({})", roots.join("|")),
                        "not": { "pattern": format!("^{}", regex_escape_prefix(EXCLUDED_ROOT)) }
                    },
                    "section": {
                        "description": "A top-level key or a JSON Pointer in a YAML or JSON source; when the element is a number, a boolean or `null`, the quote is that value as JSON text and is matched by value. In a Markdown or AsciiDoc source, the title of exactly one heading, or a path of titles joined by ` > ` in which each heading sits directly under the one before it; a title that matches more than one heading is refused.",
                        "type": "string",
                        "minLength": 1
                    },
                    "quote": {
                        "description": "The sentence that assigns the expectation, copied verbatim.",
                        "type": "string",
                        "minLength": 1
                    },
                    "report": {
                        "description": "The upstream-report issue in this repository that records a defect, contradiction or silence in the cited text.",
                        "type": "integer",
                        "minimum": 1
                    },
                    "level": level_def(),
                    "development": development_def()
                }
            },
            "section": {
                "description": "The BASE section of its capability folder a document belongs to, by the key `catalogue/chapters.yaml` declares.",
                "type": "string",
                "pattern": KEY_PATTERN
            },
            "check": check_def(),
            "control": {
                "description": "The control of a refusal workflow: a `workflowId` of the same document, or `$sourceDescriptions.<name>.<workflowId>` of a `type: arazzo` source, as Arazzo 1.1.0 `dependsOn` names a workflow; a list names one control per arm of an option the refusal runs under. A control is accepted when every row of it passed or failed only criteria that yield a warning, and a refusal row is judged when no listed control that ran was refused and at least one ran.",
                "oneOf": [
                    { "type": "string", "minLength": 1 },
                    {
                        "type": "array",
                        "minItems": 1,
                        "items": { "type": "string", "minLength": 1 }
                    },
                    {
                        "description": "A refusal with no twin in the catalogue yet: the open issue in this repository that tracks authoring one. Such a refusal is never judged; every row of it is errored, naming the issue.",
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["pending"],
                        "properties": { "pending": { "type": "integer", "minimum": 1 } }
                    }
                ]
            },
            "principal": {
                "description": "The principal a step is sent as: `administrative` is a principal the service authorizes for its administrative operations.",
                "enum": tokens(crate::arazzo::principal::Principal::ALL)
            },
            "scope": {
                "description": "The scope set of the access token a step presents: an OAuth 2.0 scope value, space-delimited scope tokens in any order (RFC 6749 §3.3), or `$inputs.<name>` to read it from each row. The empty string is a token with no scope.",
                "type": "string",
                "pattern": "^(\\$inputs\\.[A-Za-z0-9_-]+|[\\x20\\x21\\x23-\\x5B\\x5D-\\x7E]*)$"
            },
            "derive": {
                "description": "New step outputs by name. Each applies one function of BASE `object_version_id.adoc` to an output the same step declares, whose value has the lexical form `object_id::creating_system_id::version_tree_id`; a value of another form makes the row errored.",
                "type": "object",
                "minProperties": 1,
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["function", "of"],
                    "properties": {
                        "function": { "enum": tokens(crate::arazzo::derive::DeriveFunction::ALL) },
                        "of": { "type": "string", "minLength": 1 }
                    }
                }
            }
        }
    })
}

/// The condition profile's static constraints: a criterion `validate`
/// accepts, and the headers a step may send undeclared.
#[must_use]
pub fn profile_schema() -> Value {
    let headers: Vec<&str> = ProfileHeader::ALL.iter().map(|h| h.name()).collect();
    json!({
        "$schema": DRAFT,
        "$id": urn("arazzo-profile"),
        "title": "The Arazzo catalogue's condition profile",
        "description": "What the profile pins where Arazzo 1.1.0 leaves evaluation open. A `simple` condition compares a runtime expression with an integer, a boolean or `null`, never a string, because Arazzo makes `simple` string comparison case-insensitive. A `regex` condition uses no lookaround and no backreference, and one whose context is a response header reads an absent header as the empty string, so a pattern that matches the empty string holds when the header is not sent. Every runtime expression parses. A step may send a header its operation does not declare only when the header is in `profile_header`. Header names compare case-insensitively.",
        "$defs": {
            "criterion": {
                "type": "object",
                "required": ["condition"],
                "properties": {
                    "context": { "type": "string", "pattern": "^\\$" },
                    "condition": { "type": "string", "minLength": 1 },
                    "type": {}
                },
                "allOf": [
                    {
                        "if": {
                            "anyOf": [
                                { "not": { "required": ["type"] } },
                                { "properties": { "type": { "const": "simple" } } }
                            ]
                        },
                        "then": {
                            "properties": { "condition": { "not": { "pattern": "['\"]" } } }
                        }
                    },
                    {
                        "if": {
                            "required": ["type"],
                            "properties": { "type": { "const": "regex" } }
                        },
                        "then": {
                            "required": ["context"],
                            "properties": {
                                "condition": {
                                    "not": { "pattern": "\\(\\?(=|!|<=|<!)|\\\\[1-9]|\\\\k<" }
                                }
                            }
                        }
                    }
                ]
            },
            "profile_header": { "enum": headers }
        }
    })
}

/// A rows file: the input rows a data-driven workflow runs once each.
#[must_use]
pub fn rows_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("arazzo-rows"),
        "title": "Input rows for one data-driven Arazzo workflow",
        "description": "Sits beside the Arazzo document whose workflow it feeds, named `<workflowId>.rows.yaml`; `workflow` repeats that workflow's case id. Each row binds the workflow's `inputs` by property name: a key the inputs schema does not name is refused, every `required` input is bound in every row, and each row validates against the inputs schema. An input of `format: uuid` with no value is generated per row. A value under a column `literals` names is a decision-table literal, and `validate` refuses one that does not parse against the literal grammar, naming its row, its column and the character offset of the production that fails. A `recipe` binds one or two more inputs per row from the row's literals, and the row it completes is what the inputs schema validates. A file with `server` may omit `rows`: the workflow then runs once, with only its generated and declared inputs, and `validate` refuses a file whose workflow requires any other input. A workflow that declares no inputs takes such a file and no rows.",
        "type": "object",
        "additionalProperties": false,
        "required": ["workflow", "iteration"],
        "anyOf": [{ "required": ["rows"] }, { "required": ["server"] }],
        "dependentRequired": { "recipe": ["literals"] },
        "properties": {
            "workflow": {
                "description": "The case id of the workflow, `<document>/<workflowId>`.",
                "type": "string",
                "pattern": CASE_ID_PATTERN
            },
            "iteration": {
                "description": "`reset_per_row` runs each row against fresh preconditions; `single_pass` runs every row against one shared server state.",
                "enum": [iteration_token(Iteration::ResetPerRow), iteration_token(Iteration::SinglePass)]
            },
            "literals": {
                "description": "The columns whose values are decision-table literals: a range `a..b`, a list `[x, y]`, a unit-scoped range `cm 5.0..10.0`, a terminology code `openehr::122 (length)`, an ordinal tuple `1|[local::at0005]`, a quantity `100 mg`, or a plain scalar. Each names an input the workflow declares.",
                "type": "array",
                "minItems": 1,
                "uniqueItems": true,
                "items": { "type": "string", "minLength": 1 }
            },
            "recipe": recipe_def(),
            "server": {
                "description": "What the workflow needs the server under test to be. `exclusive`: the workflow changes state it did not create, such as a delete of every EHR, so it runs only when the party's IXIT declares `environment.exclusive_server: true`; any other IXIT skips every row with the reason `not-declared`.",
                "enum": ServerOwnership::ALL.iter().map(|s| s.token()).collect::<Vec<_>>()
            },
            "declared": {
                "description": "Inputs bound from a value the party declares: each maps an input the workflow declares to `ixit.<member>…` or `statement.<member>…`, a dotted path into the party's IXIT or statement. No row supplies a declared input. A run whose party does not declare the value skips every row with the reason `not-declared`.",
                "type": "object",
                "additionalProperties": {
                    "type": "string",
                    "pattern": "^(ixit|statement)(\\.[^.]+)+$"
                }
            },
            "aggregate": {
                "description": "Properties of the whole run, judged once after the last row and only under `single_pass`. `distinct` names a workflow output no two rows may share; each entry carries the citation that assigns the property.",
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["distinct", CITE_KEY],
                    "properties": {
                        "distinct": { "type": "string", "minLength": 1 },
                        CITE_KEY: {
                            "description": "The citation, in the `arazzo-extensions` cite shape.",
                            "type": "object",
                            "required": ["source", "section", "quote"]
                        }
                    }
                }
            },
            "rows": {
                "type": "array",
                "minItems": 1,
                "items": { "type": "object" }
            }
        }
    })
}

/// The `documents` member of the capabilities file: what a document needs
/// from the statement before it runs.
fn documents_def() -> Value {
    json!({
        "description": "What a document needs from the statement before it runs, keyed by document id. `validate` refuses a key that names no document of the catalogue.",
        "type": "object",
        "propertyNames": { "pattern": WORKFLOW_ID_PATTERN },
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "minProperties": 1,
            "properties": {
                "applies": {
                    "description": "The specification versions the document applies to, keyed by the component a statement's `spec_versions` declares a version of. Every workflow of the document is skipped version-out-of-range when the statement declares a version of a component outside its range; a component the statement does not declare decides nothing.",
                    "type": "object",
                    "minProperties": 1,
                    "propertyNames": { "enum": tokens(SpecComponent::ALL) },
                    "additionalProperties": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": [CITE_KEY],
                        "anyOf": [{ "required": ["min"] }, { "required": ["max"] }],
                        "properties": {
                            "min": {
                                "description": "The first release the document applies to, inclusive.",
                                "type": "string",
                                "pattern": VERSION_PATTERN
                            },
                            "max": {
                                "description": "The last release the document applies to, inclusive.",
                                "type": "string",
                                "pattern": VERSION_PATTERN
                            },
                            CITE_KEY: {
                                "description": "The released sentence that bounds the range, in the `arazzo-extensions` cite shape.",
                                "type": "object",
                                "required": ["source", "section", "quote"]
                            }
                        }
                    }
                },
                "formats": {
                    "description": "The wire formats the server must serve or process for the document to run, in the statement's `tech_profiles` vocabulary. Every workflow of the document is skipped format-not-claimed when the statement's its-rest profile does not list one of them. A document that tests the refusal of a format needs nothing: it runs for a party that does not serve the format.",
                    "type": "array",
                    "minItems": 1,
                    "uniqueItems": true,
                    "items": { "enum": tokens(FormatName::ALL) }
                }
            }
        }
    })
}

/// The capabilities file: the options a statement answers, with the
/// documents that test each arm.
#[must_use]
pub fn capabilities_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("arazzo-capabilities"),
        "title": "Options and their arm documents for the Arazzo catalogue",
        "description": "Lives at `catalogue/capabilities.yaml`. Each option is behaviour the released text leaves to the implementation, citing the sentence that leaves it. Its arms are keyed by the token a statement's `options` list selects one with, and each names the documents, by document id, that test it. The documents of every arm the statement does not select are skipped with `option-not-selected`, and a surface element counts as covered when every arm of its option covers it, or when the option's `decides` lists it, one arm asserts it, and every arm calls its operation. `validate` refuses an arm naming a document the catalogue does not hold, a document under two arms of one option, and a `decides` status no arm asserts or whose operation an arm does not call. `documents` states what a document needs before it runs, `floors` the workflow count each capability folder keeps, and `extension-only` the capability folders verified only over Veredictum's own extension routes.",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "documents": documents_def(),
            "floors": {
                "description": "The workflow count each capability folder keeps, keyed by folder name. `validate` refuses a folder without a floor, a floor naming no folder, and a folder holding fewer workflows than its floor; a test re-derives every floor from the catalogue, so a floor moves with the folder it counts.",
                "type": "object",
                "propertyNames": { "pattern": "^[A-Za-z_][A-Za-z0-9_]*$" },
                "additionalProperties": { "type": "integer", "minimum": 1 }
            },
            "extension-only": {
                "description": "The capability folders verified only over the routes of Veredictum's own sources, the OpenAPI documents whose `info` carries `x-veredictum-non-openehr: true` and which no openEHR OAS describes. `validate` refuses a listed folder with a step calling an unmarked source, and an unlisted folder whose every step calls a marked one.",
                "type": "array",
                "uniqueItems": true,
                "items": { "type": "string", "pattern": "^[A-Za-z_][A-Za-z0-9_]*$" }
            },
            "options": {
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["description", "arms", CITE_KEY],
                    "properties": {
                        "description": { "type": "string", "minLength": 1 },
                        "arms": {
                            "type": "object",
                            "minProperties": 2,
                            "additionalProperties": {
                                "type": "object",
                                "additionalProperties": false,
                                "properties": {
                                    "documents": {
                                        "type": "array",
                                        "uniqueItems": true,
                                        "items": { "type": "string", "pattern": WORKFLOW_ID_PATTERN }
                                    }
                                }
                            }
                        },
                        CITE_KEY: capabilities_cite_def(),
                        "decides": {
                            "description": "The response branches the arms are the mutually exclusive answers to: for each operation, the statuses a service answers with one arm or another, and the released sentence that makes them alternatives. A listed status counts as covered when one arm asserts it and every arm calls the operation; every other element needs every arm. No openEHR spec governs this, our own design.",
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["api", "operation", "statuses", CITE_KEY],
                                "properties": {
                                    "api": { "type": "string", "minLength": 1 },
                                    "operation": { "type": "string", "minLength": 1 },
                                    "statuses": {
                                        "type": "array",
                                        "minItems": 2,
                                        "uniqueItems": true,
                                        "items": { "type": "string", "pattern": "^[1-5][0-9]{2}$" }
                                    },
                                    CITE_KEY: capabilities_cite_def()
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

/// The kebab-case key a chapter, a section or an option carries.
const KEY_PATTERN: &str = "^[a-z0-9]+(-[a-z0-9]+)*$";

/// The structure file: the chapters, their capability folders, each folder's
/// BASE sections, and the theme and tier of every option.
#[must_use]
pub fn chapters_schema() -> Value {
    let title = || json!({ "type": "string", "minLength": 1 });
    json!({
        "$schema": DRAFT,
        "$id": urn("arazzo-chapters"),
        "title": "The reading structure of the Arazzo catalogue",
        "description": format!("Lives at `catalogue/chapters.yaml`. Every view of a result groups its rows by it: the chapters in reading order, the capability folders of each chapter as its bands, and each folder's BASE sections in reading order. A document named by no arm of `capabilities.yaml` is BASE and names one section of its folder with `{SECTION_KEY}` on its `info`; a document an arm names is OPTION and names none. `options` gives every option of `capabilities.yaml` its theme and tier. `validate` refuses a folder in no chapter or in two, a BASE document with no section or an unknown one, a section no document uses, an OPTION document with a section, an option with no placement, and a `spec-boundary` option whose citation names no `report`. No openEHR spec governs this; it is the instrument's own design."),
        "type": "object",
        "additionalProperties": false,
        "required": ["chapters", "options"],
        "properties": {
            "chapters": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["chapter", "title", "folders"],
                    "properties": {
                        "chapter": { "type": "string", "pattern": KEY_PATTERN },
                        "title": title(),
                        "folders": {
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["folder", "title", "sections"],
                                "properties": {
                                    "folder": { "type": "string", "pattern": "^[A-Za-z_][A-Za-z0-9_]*$" },
                                    "title": title(),
                                    "formalism": {
                                        "type": "boolean",
                                        "description": "Whether the views badge each case of the folder with the template formalism (ADL 1.4 or ADL 2) its setups upload. `validate` refuses a case of such a folder that uploads no template, or templates of both formalisms."
                                    },
                                    "sections": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "additionalProperties": false,
                                            "required": ["section", "title"],
                                            "properties": {
                                                "section": { "type": "string", "pattern": KEY_PATTERN },
                                                "title": title()
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "options": {
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["theme", "tier"],
                    "properties": {
                        "theme": { "enum": tokens(Theme::ALL) },
                        "tier": {
                            "enum": tokens(Tier::ALL),
                            "description": "`recommended`: support is a SHOULD, so declining it is recorded as a warning. `options`: a MAY, or behaviour the release leaves to the implementation. `spec-boundary`: an option only because the released text is defective, recorded as an upstream report its citation names."
                        }
                    }
                }
            }
        }
    })
}

/// The report snapshot: the title and state of every upstream report a
/// citation of the catalogue names.
#[must_use]
pub fn reports_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("arazzo-reports"),
        "title": "The upstream reports the Arazzo catalogue cites",
        "description": "Lives at `catalogue/reports.yaml`: the committed snapshot of every `upstream-report` issue a `report` of the catalogue's citations names, keyed by issue number, so a view of a result names a report's title and state without asking the tracker. `validate` refuses a cited report the snapshot lacks and an entry no citation names; `scripts/checks/report-refs.sh` holds each entry to the tracker. No openEHR spec governs this; it is the instrument's own design.",
        "type": "object",
        "additionalProperties": false,
        "required": ["reports"],
        "properties": {
            "reports": {
                "type": "object",
                "propertyNames": { "pattern": "^[1-9][0-9]*$" },
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["title", "state"],
                    "properties": {
                        "title": { "type": "string", "minLength": 1 },
                        "state": {
                            "enum": tokens(ReportState::ALL),
                            "description": "`open`: not yet re-verified. `confirmed`: open and carrying `upstream-confirmed`. `closed`: closed as the standing outbound record."
                        }
                    }
                }
            }
        }
    })
}

/// The surface-coverage file: the converted operation families, and the
/// tracked gaps inside them.
#[must_use]
pub fn surface_schema() -> Value {
    let headers: Vec<&str> = NegotiationHeader::ALL.iter().map(|h| h.name()).collect();
    json!({
        "$schema": DRAFT,
        "$id": urn("arazzo-surface"),
        "title": "Surface coverage of the Arazzo catalogue",
        "description": "Lives at `cases/surface.yaml`. `scope` names the operation families the catalogue has converted, each an ITS-REST API document and an OAS tag. Inside that scope every released operation, every response it declares and every value of its negotiation headers is covered by a step, or is listed in `gaps` with the issue that tracks it. A gap whose element is covered is refused, so the list only shrinks.",
        "type": "object",
        "additionalProperties": false,
        "required": ["scope"],
        "properties": {
            "scope": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["api", "tag"],
                    "properties": {
                        "api": { "type": "string", "pattern": "^[a-z]+$" },
                        "tag": { "type": "string", "minLength": 1 }
                    }
                }
            },
            "gaps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["api", "operation", "issue"],
                    "oneOf": [
                        { "not": { "anyOf": [{ "required": ["status"] }, { "required": ["header"] }, { "required": ["value"] }] } },
                        { "required": ["status"], "not": { "anyOf": [{ "required": ["header"] }, { "required": ["value"] }] } },
                        { "required": ["header", "value"], "not": { "required": ["status"] } }
                    ],
                    "properties": {
                        "api": { "type": "string", "pattern": "^[a-z]+$" },
                        "operation": { "type": "string", "minLength": 1 },
                        "status": { "type": "string", "pattern": "^[1-5][0-9][0-9]$" },
                        "header": { "enum": headers },
                        "value": { "type": "string", "minLength": 1 },
                        "issue": { "type": "integer", "minimum": 1 },
                        "note": { "type": "string", "minLength": 1 }
                    }
                }
            }
        }
    })
}

/// A rows file's `recipe`: one of the registered recipes, with exactly its
/// arguments.
fn recipe_def() -> Value {
    json!({
        "description": "The registered recipe that turns each row's literals into the values of more inputs. `content_instance` builds the composition a content decision-table row commits: the `rm_class` value from the literal columns, in the minimal carrier composition stamped with `template_id`. `content_synthesized` serves a table whose rows vary the template: per row it builds the operational template (OPT 1.4 XML text) from the literal columns, under the id `template_id` followed by a digest of the template the row's constraint columns build, binds it to `template_input`, and binds the composition stamped with that id to `input`.",
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "rm_class", "template_id", "input"],
                "properties": {
                    "name": { "const": "content_instance" },
                    "rm_class": { "enum": tokens(ContentClass::ALL) },
                    "template_id": { "type": "string", "minLength": 1 },
                    "input": {
                        "description": "The workflow input the recipe binds; no row supplies it.",
                        "type": "string",
                        "minLength": 1
                    }
                }
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "rm_class", "template_id", "input", "template_input"],
                "properties": {
                    "name": { "const": "content_synthesized" },
                    "rm_class": { "enum": tokens(SynthesizedClass::ALL) },
                    "template_id": {
                        "description": "The template family: each row's template id is this id, a dot and eight hex digits.",
                        "type": "string",
                        "minLength": 1
                    },
                    "input": {
                        "description": "The workflow input the composition is bound to; no row supplies it.",
                        "type": "string",
                        "minLength": 1
                    },
                    "template_input": {
                        "description": "The workflow input the row's operational template is bound to; no row supplies it.",
                        "type": "string",
                        "minLength": 1
                    }
                }
            }
        ]
    })
}

/// The serde token of an [`Iteration`].
fn iteration_token(iteration: Iteration) -> Value {
    serde_json::to_value(iteration).unwrap_or(Value::Null)
}

/// `prefix` with the regex metacharacters it can hold escaped.
fn regex_escape_prefix(prefix: &str) -> String {
    prefix.replace('.', "\\.").replace('-', "\\-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compiled(schema: &Value) -> jsonschema::Validator {
        jsonschema::validator_for(schema).unwrap()
    }

    #[test]
    fn every_published_schema_compiles_and_names_no_null_token() {
        for schema in [
            extensions_schema(),
            profile_schema(),
            rows_schema(),
            surface_schema(),
        ] {
            compiled(&schema);
            assert!(!has_null(&schema), "{schema}");
        }
    }

    fn has_null(value: &Value) -> bool {
        match value {
            Value::Null => true,
            Value::Array(items) => items.iter().any(has_null),
            Value::Object(map) => map.values().any(has_null),
            Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
        }
    }

    #[test]
    fn the_extension_schema_refuses_seeded_citations() {
        let schema = extensions_schema();
        let cite = compiled(&json!({ "$ref": "#/$defs/cite", "$defs": schema["$defs"].clone() }));
        let good = json!({ "source": "specs/rest-oas/ehr-validation.openapi.yaml", "section": "/info", "quote": "q", "report": 618 });
        assert!(cite.is_valid(&good));
        for seeded in [
            json!({ "source": "specs/openehr/SM/x.adoc", "section": "s", "quote": "q" }),
            json!({ "source": "README.md", "section": "s", "quote": "q" }),
            json!({ "source": "specs/openehr/RM/x.adoc", "section": "s", "quote": "q", "report": 0 }),
            json!({ "source": "specs/openehr/RM/x.adoc", "section": "s" }),
            json!({ "source": "specs/openehr/RM/x.adoc", "section": "s", "quote": "q", "page": 1 }),
            json!({ "source": "specs/openehr/RM/x.adoc", "section": "s", "quote": "q", "development": { "quote": "q" } }),
            json!({ "source": "specs/openehr/RM/x.adoc", "section": "s", "quote": "q", "development": { "source": "specs/openehr/RM/y.adoc", "section": "s", "quote": "q" } }),
        ] {
            assert!(!cite.is_valid(&seeded), "{seeded}");
        }
        let reworded = json!({ "source": "specs/openehr/RM/x.adoc", "section": "s", "quote": "q",
            "development": { "source": "specs/openehr-development/specifications-RM/docs/y.adoc", "section": "t", "quote": "r" } });
        assert!(cite.is_valid(&reworded));
    }

    #[test]
    fn the_profile_schema_refuses_seeded_criteria() {
        let schema = profile_schema();
        let criterion =
            compiled(&json!({ "$ref": "#/$defs/criterion", "$defs": schema["$defs"].clone() }));
        for good in [
            json!({ "condition": "$statusCode == 201" }),
            json!({ "context": "$response.body#/a", "type": "regex", "condition": "^{$inputs.a}$" }),
        ] {
            assert!(criterion.is_valid(&good), "{good}");
        }
        for seeded in [
            json!({ "condition": "$response.body#/a == 'x'" }),
            json!({ "type": "simple", "condition": "$response.header.Location == \"x\"" }),
            json!({ "context": "$response.body#/a", "type": "regex", "condition": "^(?=a)" }),
            json!({ "context": "$response.body#/a", "type": "regex", "condition": "(a)\\1" }),
            json!({ "type": "regex", "condition": "^a$" }),
        ] {
            assert!(!criterion.is_valid(&seeded), "{seeded}");
        }
    }
}
