// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The committed Arazzo documents pass the catalogue gate: every criterion
//! resolves to a citation whose quote occurs in the vendored released text,
//! every operation resolves in its released source, and every condition is
//! inside the profile.

use std::path::{Path, PathBuf};

use veredictum::arazzo::document::ArazzoDocument;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn documents(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            documents(&path, out)?;
        } else if path.to_string_lossy().ends_with(".arazzo.yaml") {
            out.push(path);
        }
    }
    Ok(())
}

#[test]
fn every_committed_document_passes_the_gate() {
    let root = repo_root();
    let mut found = Vec::new();
    documents(&root.join("catalogue/cases"), &mut found).unwrap();
    found.sort();
    assert!(
        !found.is_empty(),
        "no Arazzo document under catalogue/cases"
    );
    for path in found {
        let document = ArazzoDocument::load(&path).unwrap();
        let findings: Vec<String> = document
            .check(&root)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(findings, Vec::<String>::new(), "{}", path.display());
    }
}

/// One entry of `catalogue/id-map.yaml`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MapEntry {
    new: Vec<String>,
    #[serde(default)]
    open: Option<String>,
}

/// `catalogue/id-map.yaml`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct IdMap {
    cases: std::collections::BTreeMap<String, MapEntry>,
}

fn case_ids(dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            case_ids(&path, out)?;
        } else if let Some(id) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".yaml"))
        {
            out.push(id.to_owned());
        }
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 test shape: plumbing propagates with `?`, and the assertions are how the test fails"
)]
fn the_id_map_resolves_on_both_sides_and_covers_each_converted_component()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let map: IdMap = serde_json::from_value(veredictum::load::yaml_file_to_value(
        &root.join("catalogue/id-map.yaml"),
    )?)?;
    let mut documents_found = Vec::new();
    documents(&root.join("catalogue/cases"), &mut documents_found)?;
    let mut workflows = std::collections::BTreeSet::new();
    for path in documents_found {
        let document = ArazzoDocument::load(&path)?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".arazzo.yaml"))
            .ok_or("a document name")?
            .to_owned();
        for workflow in &document.description().workflows {
            workflows.insert(format!("{name}/{}", workflow.workflow_id));
        }
    }
    let mut old = Vec::new();
    case_ids(&root.join("artifacts/schedule"), &mut old)?;
    let components: std::collections::BTreeSet<&str> = map
        .cases
        .keys()
        .filter_map(|id| id.split_once('.').map(|(component, _)| component))
        .collect();
    for id in &old {
        if let Some((component, _)) = id.split_once('.')
            && components.contains(component)
        {
            assert!(
                map.cases.contains_key(id),
                "{id} is missing from the id map"
            );
        }
    }
    for (id, entry) in &map.cases {
        assert!(old.contains(id), "{id} names no case core");
        assert!(
            !entry.new.is_empty() || entry.open.is_some(),
            "{id}: an entry carries new ids or says why it has none"
        );
        for new in &entry.new {
            assert!(
                workflows.contains(new),
                "{id} maps to {new}, which no document holds"
            );
        }
    }
    Ok(())
}

#[test]
fn the_ehr_document_carries_the_worked_example() {
    let root = repo_root();
    let document = ArazzoDocument::load(&root.join("catalogue/cases/ehr/ehr.arazzo.yaml")).unwrap();
    let workflow = document
        .workflow("create_ehr-same_ehr_twice-with_id")
        .unwrap();
    let steps: Vec<&str> = workflow.steps.iter().map(|s| s.step_id.as_str()).collect();
    assert_eq!(steps, ["create", "create-again", "read-back"]);
}
