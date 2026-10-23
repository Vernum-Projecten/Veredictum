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
    documents(&root.join("artifacts/workflows"), &mut found).unwrap();
    found.sort();
    assert!(
        !found.is_empty(),
        "no Arazzo document under artifacts/workflows"
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

#[test]
fn the_ehr_document_carries_the_worked_example() {
    let root = repo_root();
    let document =
        ArazzoDocument::load(&root.join("artifacts/workflows/ehr/ehr.arazzo.yaml")).unwrap();
    let workflow = document
        .workflow("create_ehr-same_ehr_twice-with_id")
        .unwrap();
    let steps: Vec<&str> = workflow.steps.iter().map(|s| s.step_id.as_str()).collect();
    assert_eq!(steps, ["create", "create-again", "read-back"]);
}
