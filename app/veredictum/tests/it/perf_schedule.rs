// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The performance schedule (`perf/`) against the case-core catalogue it was
//! copied from.
//!
//! The case-core tree still carries its own copy of the performance cases,
//! the journey catalogue and the payloads, because `validate --root
//! artifacts` and the verdict pipeline read them there. Until that tree is
//! deleted, both copies must say the same thing: a measured run and a
//! verdict over its record would otherwise read two different schedules.

use std::path::{Path, PathBuf};

use veredictum::load::yaml_file_to_value;
use veredictum::perf_schedule::{CLASSES_DIR, JOURNEYS_FILE, PAYLOADS_DIR, PAYLOADS_FILE};

type Fallible = Result<(), Box<dyn std::error::Error>>;

fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn perf() -> PathBuf {
    repo_root().join("perf")
}

fn artifacts() -> PathBuf {
    repo_root().join("artifacts")
}

/// The file names directly in `dir`, sorted.
fn names_in(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

/// The four performance cases and the journey catalogue carry the same data
/// in both trees; only their comments differ.
// TODO(#604): delete this test with the case-core tree it compares against.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_cases_and_journeys_match_the_case_core_copies() -> Fallible {
    let old_cases = artifacts().join("schedule/performance");
    let new_cases = perf().join(CLASSES_DIR);
    let names = names_in(&new_cases)?;
    assert_eq!(
        names,
        names_in(&old_cases)?,
        "the two trees carry different cases"
    );
    assert_eq!(names.len(), 4, "{names:?}");
    for name in &names {
        assert_eq!(
            yaml_file_to_value(&new_cases.join(name))?,
            yaml_file_to_value(&old_cases.join(name))?,
            "{name} differs between the two trees"
        );
    }
    assert_eq!(
        yaml_file_to_value(&perf().join(JOURNEYS_FILE))?,
        yaml_file_to_value(&artifacts().join("vocab/journey_catalogue.yaml"))?,
        "the journey catalogue differs between the two trees"
    );
    Ok(())
}

/// Every payload is byte-identical to the corpus entry of the same key, with
/// the same template id, so the bytes a measured run sends are the ones the
/// corpus records.
// TODO(#604): delete this test with the case-core tree it compares against.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn every_payload_matches_its_corpus_entry() -> Fallible {
    let manifest = yaml_file_to_value(&perf().join(PAYLOADS_FILE))?;
    let corpus = yaml_file_to_value(&artifacts().join("corpus/MANIFEST.yaml"))?;
    let entries = manifest
        .as_object()
        .ok_or("the payload manifest is a mapping")?;
    assert!(!entries.is_empty());
    for (key, entry) in entries {
        let old = corpus
            .get(key)
            .ok_or_else(|| format!("the corpus manifest has no {key}"))?;
        assert_eq!(
            entry.get("template_id"),
            old.get("template_id"),
            "{key}: template_id"
        );
        let source = |value: &serde_json::Value| {
            value
                .get("source")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("{key} carries no source"))
        };
        let new_bytes = std::fs::read(perf().join(PAYLOADS_DIR).join(source(entry)?))?;
        let old_bytes = std::fs::read(artifacts().join("corpus").join(source(old)?))?;
        assert!(new_bytes == old_bytes, "{key}: the payload bytes differ");
    }
    Ok(())
}
