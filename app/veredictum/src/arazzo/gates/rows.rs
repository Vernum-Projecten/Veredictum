// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The rows gate.
//!
//! Every rows file sits beside the Arazzo document whose workflow it feeds,
//! as `<workflowId>.rows.yaml`, validates against the published rows schema,
//! and binds every input that workflow requires.
//!
//! No openEHR spec governs this — our own design: Arazzo 1.1.0 defines a
//! workflow's `inputs` as a JSON Schema but not how a set of values for them
//! is supplied, so the rows file is this catalogue's own format.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use roas_arazzo::v1_1::Workflow;
use serde::Deserialize;
use serde_json::Value;

use crate::arazzo::gates::published::rows_schema;
use crate::vocab::Iteration;

/// The suffix every rows file carries.
pub const ROWS_SUFFIX: &str = ".rows.yaml";

/// The value a generated `format: uuid` input stands in with when a row is
/// validated; the evaluator draws a fresh one per row.
const GENERATED_UUID: &str = "00000000-0000-4000-8000-000000000000";

/// One rows file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowsFile {
    /// The case id of the workflow the rows feed, `<document>/<workflowId>`.
    pub workflow: String,
    /// Whether every row gets fresh preconditions or all share one state.
    pub iteration: Iteration,
    /// The rows, each binding input names to values.
    pub rows: Vec<BTreeMap<String, Value>>,
}

static VALIDATOR: LazyLock<Result<jsonschema::Validator, String>> =
    LazyLock::new(|| jsonschema::validator_for(&rows_schema()).map_err(|e| e.to_string()));

/// Reads a rows file's parsed YAML through the published rows schema.
///
/// # Errors
/// One message per schema violation, or the typed-parse failure.
pub fn parse(value: &Value) -> Result<RowsFile, Vec<String>> {
    let validator = VALIDATOR
        .as_ref()
        .map_err(|error| vec![format!("the rows schema does not compile: {error}")])?;
    let violations: Vec<String> = validator
        .iter_errors(value)
        .map(|error| format!("#{}: {error}", error.instance_path()))
        .collect();
    if !violations.is_empty() {
        return Err(violations);
    }
    serde_json::from_value(value.clone()).map_err(|error| vec![error.to_string()])
}

/// The input names a workflow declares, the ones it requires, and the ones
/// the evaluator generates when no row supplies them (`format: uuid`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputShape {
    /// Every property `inputs.properties` names.
    pub declared: Vec<String>,
    /// Every name in `inputs.required`.
    pub required: Vec<String>,
    /// Every declared property of `format: uuid`.
    pub generated: Vec<String>,
}

impl InputShape {
    /// Reads the shape of `workflow.inputs`.
    #[must_use]
    pub fn of(workflow: &Workflow) -> Self {
        let Some(inputs) = &workflow.inputs else {
            return Self::default();
        };
        let properties = inputs.get("properties").and_then(Value::as_object);
        let declared = properties
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default();
        let generated = properties
            .map(|map| {
                map.iter()
                    .filter(|(_, schema)| {
                        schema.get("format").and_then(Value::as_str) == Some("uuid")
                    })
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default();
        // NOTE: a non-string `required` entry is not a JSON Schema, which the Arazzo schema gate
        // already refuses through §Workflow Object `inputs`; here it names no input.
        let required = inputs
            .get("required")
            .and_then(Value::as_array)
            .map(|names| {
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            declared,
            required,
            generated,
        }
    }

    /// The required inputs no row-less run can bind: required and not
    /// generated.
    #[must_use]
    pub fn unbound_without_rows(&self) -> Vec<&str> {
        self.required
            .iter()
            .filter(|name| !self.generated.contains(name))
            .map(String::as_str)
            .collect()
    }
}

/// Checks every row of `rows` against `workflow`'s inputs; returns one
/// message per problem, each naming its row.
#[must_use]
pub fn check_rows(rows: &RowsFile, workflow: &Workflow) -> Vec<String> {
    let mut out = Vec::new();
    let Some(inputs) = &workflow.inputs else {
        out.push(format!(
            "workflow `{}` declares no inputs, so a row has nothing to bind",
            workflow.workflow_id
        ));
        return out;
    };
    let shape = InputShape::of(workflow);
    // TODO(#638): check row values against the decision-table literal grammar once the
    // evaluator reads rows through it.
    let validator = match jsonschema::validator_for(inputs) {
        Ok(validator) => validator,
        Err(error) => {
            out.push(format!(
                "the workflow's inputs schema does not compile: {error}"
            ));
            return out;
        }
    };
    for (index, row) in rows.rows.iter().enumerate() {
        for key in row.keys() {
            if !shape.declared.contains(key) {
                out.push(format!(
                    "row {index}: `{key}` is not an input the workflow declares"
                ));
            }
        }
        let mut instance: serde_json::Map<String, Value> =
            row.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        for name in &shape.generated {
            instance
                .entry(name.clone())
                .or_insert_with(|| Value::String(GENERATED_UUID.to_owned()));
        }
        let instance = Value::Object(instance);
        for error in validator.iter_errors(&instance) {
            out.push(format!("row {index}: #{}: {error}", error.instance_path()));
        }
    }
    out
}

/// The `workflowId` a rows file names by its file name,
/// `<workflowId>.rows.yaml`; `None` for any other file.
#[must_use]
pub fn workflow_id_of(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()?.strip_suffix(ROWS_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn workflow(inputs: &Value) -> Workflow {
        serde_json::from_value(json!({
            "workflowId": "w",
            "inputs": inputs,
            "steps": [{ "stepId": "s", "operationId": "o" }]
        }))
        .unwrap()
    }

    fn inputs() -> Value {
        json!({
            "type": "object",
            "required": ["magnitude", "ehr_id"],
            "properties": {
                "magnitude": { "type": "number" },
                "units": { "type": "string" },
                "ehr_id": { "type": "string", "format": "uuid" }
            }
        })
    }

    #[test]
    fn a_well_formed_rows_file_parses_and_binds() {
        let rows = parse(&json!({
            "workflow": "content/w",
            "iteration": "reset_per_row",
            "rows": [{ "magnitude": 5.0, "units": "cm" }, { "magnitude": 11 }]
        }))
        .unwrap();
        assert_eq!(rows.iteration, Iteration::ResetPerRow);
        assert_eq!(
            check_rows(&rows, &workflow(&inputs())),
            Vec::<String>::new()
        );
    }

    #[test]
    fn seeded_schema_defects_are_refused() {
        for seeded in [
            json!({ "workflow": "w", "iteration": "reset_per_row", "rows": [{}] }),
            json!({ "workflow": "c/w", "iteration": "per_row", "rows": [{}] }),
            json!({ "workflow": "c/w", "rows": [{}] }),
            json!({ "workflow": "c/w", "iteration": "single_pass", "rows": [] }),
            json!({ "workflow": "c/w", "iteration": "single_pass", "rows": [{}], "extra": 1 }),
        ] {
            assert!(parse(&seeded).is_err(), "{seeded}");
        }
    }

    #[test]
    fn seeded_binding_defects_are_refused() {
        let rows = |row: Value| RowsFile {
            workflow: "c/w".to_owned(),
            iteration: Iteration::SinglePass,
            rows: vec![serde_json::from_value(row).unwrap()],
        };
        let w = workflow(&inputs());
        let missing = check_rows(&rows(json!({ "units": "cm" })), &w);
        assert!(
            missing.iter().any(|m| m.contains("magnitude")),
            "{missing:?}"
        );
        let unknown = check_rows(&rows(json!({ "magnitude": 1, "unit": "cm" })), &w);
        assert!(
            unknown.iter().any(|m| m.contains("`unit` is not an input")),
            "{unknown:?}"
        );
        let mistyped = check_rows(&rows(json!({ "magnitude": "five" })), &w);
        assert_eq!(mistyped.len(), 1, "{mistyped:?}");
        let no_inputs: Workflow = serde_json::from_value(
            json!({ "workflowId": "w", "steps": [{ "stepId": "s", "operationId": "o" }] }),
        )
        .unwrap();
        assert_eq!(check_rows(&rows(json!({})), &no_inputs).len(), 1);
    }

    #[test]
    fn generated_inputs_bind_themselves_and_others_need_rows() {
        let shape = InputShape::of(&workflow(&inputs()));
        assert_eq!(shape.unbound_without_rows(), vec!["magnitude"]);
    }

    #[test]
    fn a_rows_file_name_names_its_workflow() {
        assert_eq!(
            workflow_id_of(Path::new("/a/workflows/ehr/w-1.rows.yaml")),
            Some("w-1")
        );
        assert_eq!(workflow_id_of(Path::new("/a/workflows/ehr/w.yaml")), None);
        assert_eq!(
            workflow_id_of(Path::new("/a/workflows/ehr/ehr.arazzo.yaml")),
            None
        );
    }
}
