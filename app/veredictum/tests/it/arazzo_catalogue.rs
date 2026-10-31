// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The committed Arazzo documents pass the catalogue gate: every criterion
//! resolves to a citation whose quote occurs in the vendored released text,
//! every operation resolves in its released source, and every condition is
//! inside the profile.

use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{Value, json};
use veredictum::arazzo::cite::citation_of;
use veredictum::arazzo::document::ArazzoDocument;
use veredictum::arazzo::expr::{JsonPathCriterion, RequestView, ResponseView, Scope};

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
    cases: BTreeMap<String, MapEntry>,
    /// The cases of a query collection maintained elsewhere, by its name.
    #[serde(default)]
    corpora: BTreeMap<String, Corpus>,
}

/// One collection under `corpora` in `catalogue/id-map.yaml`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    source: String,
    commit: String,
    cases: BTreeMap<String, MapEntry>,
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
/// The seven workflows whose criterion judges the served shape of
/// `ITEM_TAG.target`, by document under `catalogue/cases`.
const TARGET_SHAPE: [(&str, &str); 7] = [
    (
        "demographic/person_tags.arazzo.yaml",
        "person_tags_get-version_target_shape",
    ),
    (
        "demographic/person_tags.arazzo.yaml",
        "person_tags_get-container_target_shape",
    ),
    (
        "item_tags/composition_tags.arazzo.yaml",
        "composition_tags_get-version_target_shape",
    ),
    (
        "item_tags/composition_tags.arazzo.yaml",
        "composition_tags_get-container_target_shape",
    ),
    (
        "item_tags/ehr_status_tags.arazzo.yaml",
        "ehr_status_tags_get-version_target_shape",
    ),
    (
        "item_tags/ehr_status_tags.arazzo.yaml",
        "ehr_status_tags_get-container_target_shape",
    ),
    (
        "demographic/demographic_tags.arazzo.yaml",
        "demographic_tags_get-filter_key",
    ),
];

/// The identifier every step output reads as in [`target_shape_holds`].
const TARGET_UID: &str = "8f0d61a4-3c1e-4b9f-9a57-0c2f4d7e5b11::veredictum.test::1";

/// Judges the target criterion against one served tag list, with every
/// `{$steps.…}` output it embeds read as [`TARGET_UID`].
fn target_shape_holds(
    criterion: &JsonPathCriterion,
    outputs: &BTreeMap<String, BTreeMap<String, Value>>,
    body: Value,
) -> Result<bool, Box<dyn std::error::Error>> {
    let request = RequestView {
        method: "GET".to_owned(),
        url: "http://sut.invalid/".to_owned(),
        headers: BTreeMap::new(),
        query: BTreeMap::new(),
        path: BTreeMap::new(),
    };
    let response = ResponseView {
        status: 200,
        headers: BTreeMap::new(),
        body: Ok(Some(body)),
    };
    let inputs = BTreeMap::new();
    let workflows = BTreeMap::new();
    let scope = Scope {
        inputs: &inputs,
        outputs,
        workflows: &workflows,
        exchange: Some((&request, &response)),
    };
    Ok(criterion.judge(&scope)?.holds)
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_item_tag_target_criterion_holds_for_both_released_shapes()
-> Result<(), Box<dyn std::error::Error>> {
    // NOTE: RM item_tag.adoc types `target` as UID_BASED_ID; ITS-REST ItemTag.yaml wraps it
    // in UObjectRefOfUidBasedId; upstream report #750 leaves the shape to the SEC.
    let root = repo_root();
    let output = Regex::new(r"\{\$steps\.([A-Za-z0-9_-]+)\.outputs\.([A-Za-z0-9_]+)\}")?;
    for (file, id) in TARGET_SHAPE {
        let document = ArazzoDocument::load(&root.join("catalogue/cases").join(file))?;
        let workflow = document
            .description()
            .workflows
            .iter()
            .find(|w| w.workflow_id == id)
            .ok_or_else(|| format!("{file}: no workflow {id}"))?;
        let inherited = citation_of(workflow.extensions.as_ref())?;
        let mut found = Vec::new();
        for step in &workflow.steps {
            for criterion in &step.success_criteria {
                if criterion.condition.contains("@.target") {
                    let cite = citation_of(criterion.extensions.as_ref())?
                        .or_else(|| citation_of(step.extensions.as_ref()).ok().flatten())
                        .or_else(|| inherited.clone())
                        .ok_or_else(|| format!("{id}: the target criterion has no citation"))?;
                    found.push((criterion.clone(), cite));
                }
            }
        }
        let [(criterion, cite)] = found.as_slice() else {
            return Err(format!("{id}: {} target criteria, expected one", found.len()).into());
        };
        assert_eq!(cite.report.map(NonZeroU64::get), Some(750), "{id}");
        let context = criterion.context.as_deref().ok_or("no context")?;
        let judged = JsonPathCriterion::parse(context, &criterion.condition)?;
        let mut outputs: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
        for capture in output.captures_iter(&criterion.condition) {
            let (Some(step), Some(name)) = (capture.get(1), capture.get(2)) else {
                continue;
            };
            outputs
                .entry(step.as_str().to_owned())
                .or_default()
                .insert(name.as_str().to_owned(), json!(TARGET_UID));
        }
        assert!(
            !outputs.is_empty(),
            "{id}: the criterion embeds no step output"
        );
        let bare = |uid: &str| json!([{ "key": "k", "target": { "_type": "OBJECT_VERSION_ID", "value": uid } }]);
        let wrapped = |uid: &str| {
            json!([{ "key": "k", "target": {
                "id": { "_type": "OBJECT_VERSION_ID", "value": uid },
                "namespace": "local",
                "type": "VERSION"
            } }])
        };
        let other = "1d6b0a77-9e0e-4c58-8a0b-2f0e3b6c9d42::veredictum.test::1";
        assert!(
            target_shape_holds(&judged, &outputs, bare(TARGET_UID))?,
            "{id}: the RM shape"
        );
        assert!(
            target_shape_holds(&judged, &outputs, wrapped(TARGET_UID))?,
            "{id}: the OAS shape"
        );
        assert!(
            !target_shape_holds(&judged, &outputs, bare(other))?,
            "{id}: another uid, RM shape"
        );
        assert!(
            !target_shape_holds(&judged, &outputs, wrapped(other))?,
            "{id}: another uid, OAS shape"
        );
    }
    Ok(())
}
