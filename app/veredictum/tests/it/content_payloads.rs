// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The compositions the ADL 2 content workflows commit inline, held to the
//! published ITS-JSON schema (`specs/its-json-schemas/openehr_rm_1.1.0_all.json`):
//! a commit the catalogue expects to be refused with 400 carries a body the
//! schema refuses, and every other commit carries a body the schema admits,
//! so a 422 refusal or a 201 acceptance never rests on a syntax error.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

fn repo() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The status a commit step's criteria require, read from its
/// `$statusCode == N` condition.
fn required_status(step: &Value) -> Option<u16> {
    step.get("successCriteria")?
        .as_array()?
        .iter()
        .filter_map(|c| c.get("condition")?.as_str())
        .find_map(|c| c.strip_prefix("$statusCode == ")?.trim().parse().ok())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a Result-returning test that asserts, the Book ch11 shape"
)]
fn every_adl2_content_composition_meets_the_its_json_schema_as_its_outcome_requires()
-> Result<(), Box<dyn Error>> {
    let schema: Value = serde_json::from_str(&fs::read_to_string(
        repo().join("specs/its-json-schemas/openehr_rm_1.1.0_all.json"),
    )?)?;
    let validator = jsonschema::validator_for(&schema)?;
    let content = repo().join("catalogue/cases/content");
    let mut paths: Vec<PathBuf> = fs::read_dir(&content)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    paths.retain(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("adl2_") && n.ends_with(".arazzo.yaml"))
    });
    paths.sort();
    assert!(
        !paths.is_empty(),
        "the content folder holds ADL 2 documents"
    );
    let mut commits = 0_usize;
    for path in &paths {
        let doc: Value = serde_saphyr::from_str(&fs::read_to_string(path)?)?;
        let workflows = doc
            .get("workflows")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{} has no workflows", path.display()))?;
        for workflow in workflows {
            let id = workflow
                .get("workflowId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let steps = workflow
                .get("steps")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{id} has no steps"))?;
            let commit = steps
                .iter()
                .find(|s| {
                    s.get("operationId")
                        .and_then(Value::as_str)
                        .is_some_and(|o| o.ends_with(".composition_create"))
                })
                .ok_or_else(|| format!("{id} commits no composition"))?;
            let payload = commit
                .pointer("/requestBody/payload")
                .filter(|p| p.is_object())
                .ok_or_else(|| format!("{id} commits no inline composition"))?;
            let status = required_status(commit).ok_or_else(|| format!("{id} names no status"))?;
            assert_eq!(
                validator.is_valid(payload),
                status != 400,
                "{id}: a commit expected to answer {status} carries a body the ITS-JSON schema {} it",
                if status == 400 {
                    "admits, yet 400 needs one it refuses"
                } else {
                    "refuses, yet only 400 may rest on"
                }
            );
            commits += 1;
        }
    }
    assert!(
        commits >= 81,
        "every ADL 2 content workflow was read, {commits} found"
    );
    Ok(())
}
