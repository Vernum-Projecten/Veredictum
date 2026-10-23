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
use crate::arazzo::document::CHECK_KEY;
use crate::arazzo::gates::profile::ProfileHeader;
use crate::arazzo::gates::surface::NegotiationHeader;
use crate::vocab::Iteration;

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

/// The `x-veredictum-check` object: one of the three judges. A string value
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

/// The `x-veredictum-cite` and `x-veredictum-check` extension objects.
#[must_use]
pub fn extensions_schema() -> Value {
    let roots: Vec<String> = ORACLE_ROOTS
        .iter()
        .map(|root| regex_escape_prefix(root))
        .collect();
    json!({
        "$schema": DRAFT,
        "$id": urn("arazzo-extensions"),
        "title": "The Arazzo catalogue's two specification extensions",
        "description": format!("`{CITE_KEY}` sits on a workflow, a step or a criterion; a criterion without one inherits its step's, and a step its workflow's. `{CHECK_KEY}` sits on a criterion only, for the judgements no standard Arazzo criterion can express. `validate` also checks that a citation's quote occurs, after whitespace normalization, inside the named section of the named vendored file, which no schema can state."),
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
                        "description": "A top-level key or a JSON Pointer in a YAML source. In a Markdown or AsciiDoc source, the title of exactly one heading, or a path of titles joined by ` > ` in which each heading sits directly under the one before it; a title that matches more than one heading is refused.",
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
                    }
                }
            },
            "check": check_def()
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
        "description": "What the profile pins where Arazzo 1.1.0 leaves evaluation open. A `simple` condition compares a runtime expression with an integer, a boolean or `null`, never a string, because Arazzo makes `simple` string comparison case-insensitive. A `regex` condition uses no lookaround and no backreference. Every runtime expression parses. A step may send a header its operation does not declare only when the header is in `profile_header`. Header names compare case-insensitively.",
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
        "description": "Sits beside the Arazzo document whose workflow it feeds, named `<workflowId>.rows.yaml`; `workflow` repeats that workflow's case id. Each row binds the workflow's `inputs` by property name: a key the inputs schema does not name is refused, every `required` input is bound in every row, and each row validates against the inputs schema. An input of `format: uuid` with no value is generated per row.",
        "type": "object",
        "additionalProperties": false,
        "required": ["workflow", "iteration", "rows"],
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
            "rows": {
                "type": "array",
                "minItems": 1,
                "items": { "type": "object" }
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
        "description": "Lives at `workflows/surface.yaml`. `scope` names the operation families the catalogue has converted, each an ITS-REST API document and an OAS tag. Inside that scope every released operation, every response it declares and every value of its negotiation headers is covered by a step, or is listed in `gaps` with the issue that tracks it. A gap whose element is covered is refused, so the list only shrinks.",
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
        ] {
            assert!(!cite.is_valid(&seeded), "{seeded}");
        }
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
