// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The official Arazzo 1.1 JSON Schema, embedded, amended where its own
//! specification text disagrees with it, and compiled once per process.
//!
//! The schema is published at
//! <https://spec.openapis.org/arazzo/1.1/schema/2026-04-15> and vendored
//! byte-for-byte by `scripts/vendor/arazzo-schema.sh`. The Arazzo 1.1.0
//! specification text (<https://spec.openapis.org/arazzo/v1.1.0.html>) is
//! normative, so where the two disagree the text wins: each amendment below
//! replaces one schema value and refuses to apply unless the vendored value
//! is exactly the one it was written against.

use std::sync::LazyLock;

use serde_json::{Value, json};

/// The vendored schema's own `$id`.
pub const SCHEMA_ID: &str = "https://spec.openapis.org/arazzo/1.1/schema/2026-04-15";

const VENDORED: &str = include_str!("../vendor/arazzo-1.1-schema-2026-04-15.json");

/// Why the embedded schema cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchemaError {
    /// The vendored bytes are not JSON.
    #[error("the vendored Arazzo schema is not JSON: {0}")]
    NotJson(String),
    /// An amendment's pre-image is not what the vendored schema holds, so the
    /// amendment was written against a different revision.
    #[error("the Arazzo schema amendment at `{0}` no longer matches the vendored schema")]
    StaleAmendment(&'static str),
    /// The amended schema does not compile.
    #[error("the Arazzo schema does not compile: {0}")]
    Compile(String),
}

/// One replacement of a schema value, guarded by the value it replaces.
struct Amendment {
    /// The JSON Pointer of the replaced value.
    pointer: &'static str,
    /// The vendored value the amendment was written against.
    before: fn() -> Value,
    /// The value the specification text requires.
    after: fn() -> Value,
}

// NOTE: OAI/Arazzo-Specification#559: Arazzo 1.1.0 §Parameter Object types `value`
// `Any | {expression} | Selector Object`, so a plain object is a valid value.
const PARAMETER_VALUE: Amendment = Amendment {
    pointer: "/$defs/parameter-object/properties/value",
    before: || {
        json!({
            "description": "The value to pass in the parameter",
            "oneOf": [
                { "type": ["string", "boolean", "array", "number", "null"] },
                { "$ref": "#/$defs/selector-object" }
            ]
        })
    },
    after: || {
        json!({
            "description": "The value to pass in the parameter",
            "anyOf": [
                { "type": ["string", "boolean", "object", "array", "number", "null"] },
                { "$ref": "#/$defs/selector-object" }
            ]
        })
    },
};

// NOTE: Arazzo 1.1.0 §Payload Replacement Object types `value` `Any | {expression} |
// Selector Object`, "a constant, a Runtime Expression, or Selector Objects", so any literal is.
const REPLACEMENT_VALUE: Amendment = Amendment {
    pointer: "/$defs/payload-replacement-object/properties/value",
    before: || {
        json!({
            "description": "The value to set at the location defined by the target. May be a literal, a runtime expression string, or a selector object.\n",
            "oneOf": [
                { "type": "string" },
                { "$ref": "#/$defs/selector-object" }
            ]
        })
    },
    after: || {
        json!({
            "description": "The value to set at the location defined by the target. May be a literal, a runtime expression string, or a selector object.\n",
            "anyOf": [
                { "type": ["string", "boolean", "object", "array", "number", "null"] },
                { "$ref": "#/$defs/selector-object" }
            ]
        })
    },
};

const AMENDMENTS: &[Amendment] = &[PARAMETER_VALUE, REPLACEMENT_VALUE];
// NOTE: OAI/Arazzo-Specification#558 needs no amendment: Arazzo 1.1.0 §Expression Type
// Object, Fixed Fields, marks `version` REQUIRED, as the schema's `required` list does.

/// The vendored schema with every amendment applied.
///
/// # Errors
/// [`SchemaError::NotJson`] or [`SchemaError::StaleAmendment`].
pub fn amended_schema() -> Result<Value, SchemaError> {
    let schema: Value =
        serde_json::from_str(VENDORED).map_err(|error| SchemaError::NotJson(error.to_string()))?;
    amend(schema)
}

/// Applies every amendment to `schema`, refusing one whose pre-image differs.
fn amend(mut schema: Value) -> Result<Value, SchemaError> {
    for amendment in AMENDMENTS {
        let slot = schema
            .pointer_mut(amendment.pointer)
            .filter(|slot| **slot == (amendment.before)())
            .ok_or(SchemaError::StaleAmendment(amendment.pointer))?;
        *slot = (amendment.after)();
    }
    Ok(schema)
}

static VALIDATOR: LazyLock<Result<jsonschema::Validator, SchemaError>> = LazyLock::new(|| {
    let schema = amended_schema()?;
    jsonschema::validator_for(&schema).map_err(|error| SchemaError::Compile(error.to_string()))
});

/// Validates one Arazzo document, as read from YAML, against the amended
/// schema. Each violation is one line: the instance location, then the
/// message.
///
/// # Errors
/// A [`SchemaError`] when the embedded schema itself cannot be used, which is
/// a defect in this crate and never in the document.
pub fn violations(document: &Value) -> Result<Vec<String>, SchemaError> {
    let validator = VALIDATOR.as_ref().map_err(Clone::clone)?;
    let mut out: Vec<String> = validator
        .iter_errors(document)
        .map(|error| {
            let at = error.instance_path().to_string();
            let at = if at.is_empty() {
                "#".to_owned()
            } else {
                format!("#{at}")
            };
            format!("{at}: {error}")
        })
        .collect();
    out.sort();
    out.dedup();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(parameter_value: &Value) -> Value {
        json!({
            "arazzo": "1.1.0",
            "info": { "title": "t", "version": "1" },
            "sourceDescriptions": [{ "name": "ehr", "url": "ehr.yaml", "type": "openapi" }],
            "workflows": [{
                "workflowId": "w",
                "steps": [{
                    "stepId": "s",
                    "operationId": "$sourceDescriptions.ehr.op",
                    "parameters": [{ "name": "p", "in": "query", "value": parameter_value }],
                    "successCriteria": [{ "condition": "$statusCode == 200" }]
                }]
            }]
        })
    }

    #[test]
    fn the_vendored_schema_is_the_published_revision() {
        let schema: Value = serde_json::from_str(VENDORED).unwrap();
        assert_eq!(schema.get("$id"), Some(&json!(SCHEMA_ID)));
    }

    #[test]
    fn a_valid_document_passes() {
        assert_eq!(
            violations(&document(&json!("$inputs.x"))).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn the_559_amendment_admits_a_plain_object_value_and_still_reads_selectors() {
        assert_eq!(
            violations(&document(&json!({ "a": 1 }))).unwrap(),
            Vec::<String>::new()
        );
        let selector =
            json!({ "context": "$response.body", "selector": "$.a", "type": "jsonpath" });
        assert_eq!(
            violations(&document(&selector)).unwrap(),
            Vec::<String>::new()
        );
    }

    fn with_replacement(value: Option<Value>) -> Value {
        let mut doc = document(&json!("x"));
        let mut replacement = json!({ "target": "/a" });
        if let Some(value) = value {
            replacement["value"] = value;
        }
        doc["workflows"][0]["steps"][0]["requestBody"] = json!({
            "contentType": "application/json",
            "payload": { "a": 0 },
            "replacements": [replacement]
        });
        doc
    }

    #[test]
    fn the_replacement_amendment_admits_a_literal_of_every_json_type() {
        let literals = [
            json!("$inputs.x"),
            json!("text"),
            json!(true),
            json!(10),
            json!(1.5),
            json!(null),
            json!([1, "two"]),
            json!({ "a": 1 }),
            json!({ "context": "$response.body", "selector": "$.a", "type": "jsonpath" }),
        ];
        for literal in literals {
            assert_eq!(
                violations(&with_replacement(Some(literal.clone()))).unwrap(),
                Vec::<String>::new(),
                "{literal}"
            );
        }
    }

    #[test]
    fn a_replacement_without_a_value_is_still_refused() {
        assert!(!violations(&with_replacement(None)).unwrap().is_empty());
    }

    #[test]
    fn the_558_rule_stands_an_expression_type_needs_its_version() {
        let mut doc = document(&json!("x"));
        let criterion = doc
            .pointer_mut("/workflows/0/steps/0/successCriteria/0")
            .unwrap();
        *criterion = json!({
            "context": "$response.body",
            "condition": "$.a",
            "type": { "type": "jsonpath" }
        });
        assert!(!violations(&doc).unwrap().is_empty());
    }

    #[test]
    fn seeded_schema_defects_are_refused() {
        let mut missing_info = document(&json!("x"));
        missing_info.as_object_mut().unwrap().remove("info");
        let mut bad_version = document(&json!("x"));
        bad_version["arazzo"] = json!("1.0.1");
        let mut no_steps = document(&json!("x"));
        no_steps["workflows"][0]["steps"] = json!([]);
        for seeded in [missing_info, bad_version, no_steps] {
            assert!(!violations(&seeded).unwrap().is_empty(), "{seeded}");
        }
    }

    #[test]
    fn a_stale_amendment_refuses_to_apply() {
        for amendment in AMENDMENTS {
            let mut schema: Value = serde_json::from_str(VENDORED).unwrap();
            *schema.pointer_mut(amendment.pointer).unwrap() = json!({ "type": "string" });
            assert_eq!(
                amend(schema),
                Err(SchemaError::StaleAmendment(amendment.pointer))
            );
        }
        assert!(amended_schema().is_ok());
    }
}
