// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The compositions the ADL 2 content workflows commit inline, held to the
//! published ITS-JSON schema (`specs/its-json-schemas/openehr_rm_1.1.0_all.json`):
//! a commit the catalogue expects to be refused with 400 carries a body the
//! schema refuses, and every other commit carries a body the schema admits,
//! so a 422 refusal or a 201 acceptance never rests on a syntax error. The
//! composition the ADL 1.4 content cases commit is held to the archetype ids
//! and at-codes of every ADL 1.4 content template it is committed against,
//! and every constraint code those templates use is defined in them.

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
        commits >= 85,
        "every ADL 2 content workflow was read, {commits} found"
    );
    Ok(())
}

/// The `node_id` the first `C_OBJECT` of `rm_type` declares in an ADL 1.4
/// OPT, read from the XML text.
fn node_id_of(opt: &str, rm_type: &str) -> Option<String> {
    let (_, after) = opt.split_once(&format!("<rm_type_name>{rm_type}</rm_type_name>"))?;
    let (_, rest) = after.split_once("<node_id>")?;
    Some(rest.split_once("</node_id>")?.0.to_owned())
}

/// Every `archetype_id` an ADL 1.4 OPT names, in document order.
fn archetype_ids(opt: &str) -> Vec<String> {
    opt.split("<archetype_id><value>")
        .skip(1)
        .filter_map(|s| s.split_once("</value>").map(|(id, _)| id.to_owned()))
        .collect()
}

/// The composition `content_instance` commits for an ADL 1.4 content case
/// carries the archetype ids and at-codes of the template it is committed
/// against (AOM 1.4 `C_OBJECT.node_id`), for every content setup.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a Result-returning test that asserts, the Book ch11 shape"
)]
fn the_content_carrier_names_the_nodes_of_every_adl14_content_template()
-> Result<(), Box<dyn Error>> {
    use veredictum::exec::recipes::content_instance;
    use veredictum::model::case::MatrixCell;

    let setup: Value = serde_saphyr::from_str(&fs::read_to_string(
        repo().join("catalogue/cases/_setup/content_templates.arazzo.yaml"),
    )?)?;
    let workflows = setup
        .get("workflows")
        .and_then(Value::as_array)
        .ok_or("the content setup has no workflows")?;
    assert!(!workflows.is_empty(), "the content setup uploads templates");
    for workflow in workflows {
        let opt = workflow
            .pointer("/steps/0/requestBody/payload")
            .and_then(Value::as_str)
            .ok_or("a setup uploads no OPT")?;
        let template_id = opt
            .split_once("<template_id><value>")
            .and_then(|(_, r)| r.split_once("</value>"))
            .map(|(id, _)| id)
            .ok_or("an OPT names no template_id")?;
        let carrier = content_instance(
            "DV_COUNT",
            template_id,
            &["magnitude".to_owned()],
            &[MatrixCell::Literal(serde_json::json!(15))],
        );
        let observation = carrier
            .pointer("/content/0")
            .ok_or("the carrier holds no OBSERVATION")?;
        let ids = archetype_ids(opt);
        for node in [&carrier, observation] {
            let id = node
                .pointer("/archetype_details/archetype_id/value")
                .and_then(Value::as_str)
                .ok_or("an archetype id is missing")?;
            assert!(
                ids.iter().any(|t| t == id),
                "{template_id}: the template names no archetype {id}"
            );
        }
        for (rm_type, pointer) in [
            ("HISTORY", "/data/archetype_node_id"),
            ("EVENT", "/data/events/0/archetype_node_id"),
            ("ITEM_TREE", "/data/events/0/data/archetype_node_id"),
            ("ELEMENT", "/data/events/0/data/items/0/archetype_node_id"),
        ] {
            assert_eq!(
                observation.pointer(pointer).and_then(Value::as_str),
                node_id_of(opt, rm_type).as_deref(),
                "{template_id}: the carrier's {rm_type} is not the template's"
            );
        }
        assert_eq!(
            carrier
                .pointer("/archetype_details/template_id/value")
                .and_then(Value::as_str),
            Some(template_id),
            "the carrier names the template it is committed against"
        );
    }
    Ok(())
}
/// Every constraint code an ADL 1.4 content template uses in a
/// `CONSTRAINT_REF` is defined in a `constraint_definitions` part of its
/// ontology, as ADL 1.4 `master08-adl.adoc` §Coded Term Validity requires:
/// "Each constraint code used in the archetype definition part must be
/// defined in the `constraint_definitions` part of the ontology."
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a Result-returning test that asserts, the Book ch11 shape"
)]
fn every_constraint_code_of_an_adl14_content_template_is_defined() -> Result<(), Box<dyn Error>> {
    let setup: Value = serde_saphyr::from_str(&fs::read_to_string(
        repo().join("catalogue/cases/_setup/content_templates.arazzo.yaml"),
    )?)?;
    let workflows = setup
        .get("workflows")
        .and_then(Value::as_array)
        .ok_or("the content setup has no workflows")?;
    let mut references = 0_usize;
    for workflow in workflows {
        let opt = workflow
            .pointer("/steps/0/requestBody/payload")
            .and_then(Value::as_str)
            .ok_or("a setup uploads no OPT")?;
        let definitions: Vec<&str> = opt
            .split("<constraint_definitions")
            .skip(1)
            .filter_map(|s| s.split_once("</constraint_definitions>").map(|(d, _)| d))
            .collect();
        for reference in opt
            .split("<reference>")
            .skip(1)
            .filter_map(|s| s.split_once("</reference>").map(|(r, _)| r))
        {
            let item = format!("<items code=\"{reference}\">");
            assert!(
                definitions.iter().any(|d| d.contains(&item)),
                "a content template uses {reference} with no constraint definition"
            );
            references += 1;
        }
    }
    assert!(
        references > 0,
        "a content template uses a constraint reference"
    );
    Ok(())
}
