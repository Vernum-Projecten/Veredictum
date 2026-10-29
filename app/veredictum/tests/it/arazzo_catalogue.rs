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
    /// The cases of a query collection maintained elsewhere, by its name.
    #[serde(default)]
    corpora: std::collections::BTreeMap<String, Corpus>,
}

/// One collection under `corpora` in `catalogue/id-map.yaml`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    source: String,
    commit: String,
    cases: std::collections::BTreeMap<String, MapEntry>,
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 test shape: plumbing propagates with `?`, and the assertions are how the test fails"
)]
fn every_new_id_of_the_id_map_names_a_workflow() -> Result<(), Box<dyn std::error::Error>> {
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
    assert!(!map.cases.is_empty(), "the id map carries the old case ids");
    for (id, entry) in &map.cases {
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
    for (name, corpus) in &map.corpora {
        assert!(
            corpus.source.starts_with("https://") && corpus.commit.len() == 40,
            "{name} names its source repository and full commit"
        );
        assert!(!corpus.cases.is_empty(), "{name} maps no case");
        for (id, entry) in &corpus.cases {
            assert!(
                !entry.new.is_empty() || entry.open.is_some(),
                "{name} {id}: an entry carries new ids or says why it has none"
            );
            for new in &entry.new {
                assert!(
                    workflows.contains(new),
                    "{name} {id} maps to {new}, which no document holds"
                );
            }
        }
    }
    Ok(())
}

/// Every relative `$ref` of the decomposed ITS-REST OAS under `dir`, as the
/// referring file and the reference without its fragment.
fn oas_references(dir: &Path, out: &mut Vec<(PathBuf, String)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            oas_references(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "yaml") {
            for line in std::fs::read_to_string(&path)?.lines() {
                let line = line.trim_start().trim_start_matches("- ");
                let Some(value) = line.strip_prefix("$ref:") else {
                    continue;
                };
                let value = value.trim().trim_matches(['\'', '"']);
                let file = value.split('#').next().unwrap_or_default();
                if !file.is_empty() && !file.contains("://") {
                    out.push((path.clone(), file.to_owned()));
                }
            }
        }
    }
    Ok(())
}

/// The surface gate enumerates the released operations by following the
/// decomposed OAS's `$ref`s, and a reference whose case differs from the
/// vendored file name resolves on a case-insensitive file system only.
#[test]
fn every_decomposed_oas_reference_names_its_file_in_the_same_case() {
    let root = repo_root().join("specs/openehr/ITS-REST/specifications");
    let mut references = Vec::new();
    oas_references(&root, &mut references).unwrap();
    assert!(!references.is_empty(), "no $ref under {}", root.display());
    for (from, reference) in references {
        let mut dir = from.parent().unwrap().to_path_buf();
        for part in reference.split('/') {
            match part {
                "." | "" => {}
                ".." => {
                    dir.pop();
                }
                name => {
                    let names: Vec<String> = std::fs::read_dir(&dir)
                        .unwrap()
                        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                        .collect();
                    assert!(
                        names.iter().any(|n| n == name),
                        "{}: `{reference}` names `{name}`, which `{}` does not hold in that case",
                        from.display(),
                        dir.display()
                    );
                    dir.push(name);
                }
            }
        }
    }
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

/// The committed floors are the catalogue's own workflow counts, re-derived
/// here: a change that adds workflows to a folder raises its floor in the
/// same change, so the floor `validate` holds the folder to never lags it.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn every_committed_floor_is_the_workflow_count_of_its_folder()
-> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root().join("catalogue");
    let mut loaded = Vec::new();
    for found in veredictum::arazzo::catalogue::document_paths(&root)? {
        loaded.push(ArazzoDocument::load(&found.path)?);
    }
    let counts = veredictum::arazzo::gates::folders::workflow_counts(&loaded);
    let capabilities = veredictum::arazzo::capabilities::Capabilities::load(
        &root.join(veredictum::arazzo::capabilities::CAPABILITIES_FILE),
    )?;
    assert_eq!(
        capabilities.floors, counts,
        "re-derive the floors in capabilities.yaml from the catalogue"
    );
    Ok(())
}
