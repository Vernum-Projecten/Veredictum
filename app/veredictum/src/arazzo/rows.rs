// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Rows files at run time: where the evaluator finds the rows of a workflow,
//! and how it loads them.
//!
//! The format, its published schema and its gate are
//! [`crate::arazzo::gates::rows`]; a rows file sits beside the document whose
//! workflow it feeds, as `<workflowId>.rows.yaml`. No openEHR spec governs
//! this — our own design.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use roas_arazzo::v1_1::Workflow;
use serde_json::Value;

use crate::arazzo::gates::rows::{ROWS_SUFFIX, RowsFile, bind, check_rows, parse};

/// One row: the input values of one run of the workflow.
pub type Row = BTreeMap<String, Value>;

/// Why a rows file cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum RowsError {
    /// The file does not read or parse as YAML.
    #[error(transparent)]
    Load(#[from] crate::load::LoadError),
    /// The file does not match the published rows schema.
    #[error("`{path}` is not a rows file: {}", reasons.join("; "))]
    Schema {
        /// The file.
        path: PathBuf,
        /// Every violation.
        reasons: Vec<String>,
    },
    /// The file names another workflow than the one it is found for.
    #[error("`{path}` names `{named}`, and is found for `{expected}`")]
    WrongWorkflow {
        /// The file.
        path: PathBuf,
        /// The case id it names.
        named: String,
        /// The case id it is found for.
        expected: String,
    },
    /// A row does not bind the workflow's inputs.
    #[error("`{path}`: {}", reasons.join("; "))]
    Binding {
        /// The file.
        path: PathBuf,
        /// One message per problem, each naming its row.
        reasons: Vec<String>,
    },
}

/// The rows file path for `workflow_id` of the document at `document`: beside
/// the document, in the same folder.
#[must_use]
pub fn rows_path(document: &Path, workflow_id: &str) -> PathBuf {
    document.with_file_name(format!("{workflow_id}{ROWS_SUFFIX}"))
}

/// Loads the rows file at `path` for `workflow`, whose case id is `case_id`,
/// through the published schema and the rows gate's binding check.
///
/// The rows returned are the rows the workflow receives: each authored row
/// plus the input the file's recipe binds, if it names one.
///
/// # Errors
/// [`RowsError`] when the file does not load, does not validate, names
/// another workflow, or does not bind the workflow's inputs.
pub fn load(path: &Path, case_id: &str, workflow: &Workflow) -> Result<RowsFile, RowsError> {
    let value = crate::load::yaml_file_to_value(path)?;
    let mut rows = parse(&value).map_err(|reasons| RowsError::Schema {
        path: path.to_owned(),
        reasons,
    })?;
    if rows.workflow != case_id {
        return Err(RowsError::WrongWorkflow {
            path: path.to_owned(),
            named: rows.workflow,
            expected: case_id.to_owned(),
        });
    }
    let reasons = check_rows(&rows, workflow);
    if !reasons.is_empty() {
        return Err(RowsError::Binding {
            path: path.to_owned(),
            reasons,
        });
    }
    rows.rows = bind(&rows).map_err(|reasons| RowsError::Binding {
        path: path.to_owned(),
        reasons,
    })?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::Iteration;
    use assert_fs::prelude::*;
    use serde_json::json;

    fn workflow() -> Workflow {
        serde_json::from_value(json!({
            "workflowId": "w",
            "inputs": { "type": "object", "required": ["magnitude", "id"],
                "properties": { "magnitude": { "type": "number" }, "units": { "type": "string" },
                                "id": { "type": "string", "format": "uuid" } } },
            "steps": []
        }))
        .unwrap()
    }

    fn file(text: &str) -> (assert_fs::TempDir, PathBuf) {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("cases/content/w.rows.yaml")
            .write_str(text)
            .unwrap();
        let path = dir.path().join("cases/content/w.rows.yaml");
        (dir, path)
    }

    #[test]
    fn a_rows_file_loads_and_binds_its_inputs() {
        let (_dir, path) = file(
            "workflow: content/w\niteration: reset_per_row\nrows:\n  - { magnitude: 5.0, units: cm }\n  - { magnitude: 11.0, units: cm }\n",
        );
        let rows = load(&path, "content/w", &workflow()).unwrap();
        assert_eq!(rows.iteration, Iteration::ResetPerRow);
        assert_eq!(rows.rows.len(), 2);
    }

    #[test]
    fn a_malformed_misnamed_or_unbound_rows_file_is_refused() {
        let (_dir, path) = file("workflow: content/w\niteration: sometimes\nrows: [{}]\n");
        assert!(matches!(
            load(&path, "content/w", &workflow()),
            Err(RowsError::Schema { .. })
        ));
        let (_dir, path) =
            file("workflow: content/v\niteration: single_pass\nrows: [{ magnitude: 1 }]\n");
        assert!(matches!(
            load(&path, "content/w", &workflow()),
            Err(RowsError::WrongWorkflow { .. })
        ));
        let (_dir, path) =
            file("workflow: content/w\niteration: single_pass\nrows: [{ units: cm }]\n");
        assert!(matches!(
            load(&path, "content/w", &workflow()),
            Err(RowsError::Binding { .. })
        ));
    }

    #[test]
    fn the_rows_file_sits_beside_its_document() {
        assert_eq!(
            rows_path(
                Path::new("catalogue/cases/ehr/ehr.arazzo.yaml"),
                "get_ehr-by_id"
            ),
            PathBuf::from("catalogue/cases/ehr/get_ehr-by_id.rows.yaml")
        );
    }
}
