// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The payload manifest, `perf/payloads.yaml`, and its gates.
//!
//! Every request body and operational template a workload sends is a file
//! under `perf/payloads/`, keyed by the name a workload or the seeder reads
//! it by, with its provenance. Loading the manifest checks every source is a
//! relative path naming a file inside `payloads/` and that a file pinned by
//! `sha256` still hashes to its pin. [`check_reads`] then holds the manifest
//! to the workloads: every key a workload reads is present, an operational
//! template carries its `template_id`, and the manifest carries nothing that
//! no workload reads. Every defect is one [`LoadError`] naming the manifest.
//!
//! No openEHR specification governs measured performance (CNF guide
//! `master03-overview.adoc`: "Non-functional conformance (performance, etc)
//! is not addressed by this guide"): our own design.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use serde::Deserialize;

use crate::ids::CorpusKey;
use crate::load::{LoadError, compile_schema, load_artifact};
use crate::workload::load::{PayloadRead, sha256_hex};
use crate::workload::ops::PayloadRole;

/// The payload manifest's file name under the performance root.
pub const PAYLOADS_FILE: &str = "payloads.yaml";
/// The directory under the performance root the manifest's sources resolve
/// against.
pub const PAYLOADS_DIR: &str = "payloads";
/// The manifest suffix of a template's committed example skeleton.
pub const EXAMPLE_SUFFIX: &str = ".example";
/// The manifest suffix that names a corpus template's invalid twin, the
/// composition the commit-validation canary offers.
pub const INVALID_TWIN_SUFFIX: &str = ".invalid_twin";

/// One payload a workload sends.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadEntry {
    /// The file, relative to `payloads/`.
    pub source: String,
    /// The operational template's wire identifier, for an OPT and for a body
    /// constrained by one.
    #[serde(default)]
    pub template_id: Option<String>,
    /// The lowercase hex sha256 the file's bytes are pinned at, for a
    /// payload reproduced from a published source.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Where the bytes come from.
    pub provenance: String,
}

/// The payload manifest, keyed by the name a workload or the seeder reads.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PayloadManifest(pub BTreeMap<CorpusKey, PayloadEntry>);

impl PayloadManifest {
    /// Returns the entry for `key`, or `None` when the manifest has none or
    /// `key` is not a well-formed payload key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&PayloadEntry> {
        CorpusKey::parse(key).ok().and_then(|key| self.0.get(&key))
    }
}

static PAYLOADS_SCHEMA: LazyLock<Result<jsonschema::Validator, String>> = LazyLock::new(|| {
    compile_schema(
        &crate::schema::perf_payloads_schema(),
        "perf-payloads.schema.json",
    )
    .map_err(|e| e.to_string())
});

fn defect(errors: &mut Vec<LoadError>, path: &Path, message: String) {
    errors.push(LoadError::Model {
        path: path.to_owned(),
        message,
    });
}

/// Loads `<perf_root>/payloads.yaml` through its schema, then checks every
/// source and every pin.
///
/// # Errors
/// Every [`LoadError`] found: the schema's, then one per source outside
/// `payloads/`, missing file, or pin the bytes no longer hash to.
pub fn load_manifest(perf_root: &Path) -> Result<PayloadManifest, Vec<LoadError>> {
    let path = perf_root.join(PAYLOADS_FILE);
    let validator = PAYLOADS_SCHEMA.as_ref().map_err(|message| {
        vec![LoadError::Schema {
            path: PathBuf::from("perf-payloads.schema.json"),
            message: message.clone(),
        }]
    })?;
    let manifest: PayloadManifest = load_artifact(&path, validator).map_err(|e| vec![e])?;
    let mut errors = Vec::new();
    check_sources(&perf_root.join(PAYLOADS_DIR), &path, &manifest, &mut errors);
    if errors.is_empty() {
        Ok(manifest)
    } else {
        Err(errors)
    }
}

/// Every source is a relative path inside `payloads/` naming a file, and a
/// pinned file still hashes to its pin.
fn check_sources(
    dir: &Path,
    manifest_path: &Path,
    manifest: &PayloadManifest,
    errors: &mut Vec<LoadError>,
) {
    for (key, entry) in &manifest.0 {
        let relative = Path::new(&entry.source);
        if !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        {
            defect(
                errors,
                manifest_path,
                format!(
                    "{key}: source {:?} must be a relative path inside {PAYLOADS_DIR}/",
                    entry.source
                ),
            );
            continue;
        }
        match std::fs::read(dir.join(relative)) {
            Err(_) => defect(
                errors,
                manifest_path,
                format!(
                    "{key}: source {PAYLOADS_DIR}/{} is not a file",
                    entry.source
                ),
            ),
            Ok(bytes) => {
                if let Some(pin) = &entry.sha256 {
                    let actual = sha256_hex(&bytes);
                    if &actual != pin {
                        defect(
                            errors,
                            manifest_path,
                            format!(
                                "{key}: sha256: {PAYLOADS_DIR}/{} is pinned at {pin} but hashes \
                                 to {actual}",
                                entry.source
                            ),
                        );
                    }
                }
            }
        }
    }
}

/// What a payload key is read as, which decides whether it needs a
/// `template_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// An operational template a workload commits against.
    Template,
    /// A body or a skeleton read for its bytes alone.
    Bytes,
}

/// Holds the manifest to what the workloads read: every key read is
/// present, an operational template carries its `template_id`, and the
/// manifest carries nothing no workload reads.
///
/// A template's read includes its `.example` skeleton and, where the
/// manifest has one, its `.invalid_twin`; a Simplified Flat body's read
/// includes every operational template with its `template_id`.
#[must_use]
pub fn check_reads(
    manifest_path: &Path,
    manifest: &PayloadManifest,
    reads: &[PayloadRead],
) -> Vec<LoadError> {
    let mut needed: BTreeMap<String, (Role, String)> = BTreeMap::new();
    for read in reads {
        match read.role {
            PayloadRole::Template => {
                needed.insert(read.key.clone(), (Role::Template, read.reader.clone()));
                needed.insert(
                    format!("{}{EXAMPLE_SUFFIX}", read.key),
                    (Role::Bytes, read.reader.clone()),
                );
                let twin = format!("{}{INVALID_TWIN_SUFFIX}", read.key);
                if manifest.get(&twin).is_some() {
                    needed.insert(
                        twin,
                        (Role::Bytes, "the commit-validation canary".to_owned()),
                    );
                }
            }
            PayloadRole::Flat => {
                needed.insert(read.key.clone(), (Role::Template, read.reader.clone()));
                let template_id = manifest.get(&read.key).and_then(|e| e.template_id.clone());
                for (key, entry) in &manifest.0 {
                    if Path::new(&entry.source)
                        .extension()
                        .is_some_and(|x| x.eq_ignore_ascii_case("opt"))
                        && entry.template_id == template_id
                    {
                        needed.insert(key.to_string(), (Role::Template, read.reader.clone()));
                    }
                }
            }
            PayloadRole::Body => {
                needed.insert(read.key.clone(), (Role::Bytes, read.reader.clone()));
            }
        }
    }
    let mut errors = Vec::new();
    for (key, (role, reader)) in &needed {
        match manifest.get(key) {
            None => defect(
                &mut errors,
                manifest_path,
                format!("{reader} reads {key}, which the manifest does not carry"),
            ),
            Some(entry) if *role == Role::Template && entry.template_id.is_none() => defect(
                &mut errors,
                manifest_path,
                format!(
                    "{key} is an operational template {reader} commits against, but it carries no \
                     template_id"
                ),
            ),
            Some(_) => {}
        }
    }
    for key in manifest.0.keys() {
        if !needed.contains_key(key.as_str()) {
            defect(
                &mut errors,
                manifest_path,
                format!("{key} is a payload no workload reads"),
            );
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workload::load::payload_reads;

    fn committed() -> PathBuf {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../perf")).to_owned()
    }

    /// Copies the committed performance root into a temporary one.
    fn staged() -> assert_fs::TempDir {
        let dir = assert_fs::TempDir::new().expect("temp dir");
        copy_tree(&committed(), dir.path());
        dir
    }

    fn copy_tree(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).expect("creating the target");
        for entry in std::fs::read_dir(from)
            .expect("reading the source")
            .flatten()
        {
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).expect("copying a file");
            }
        }
    }

    fn messages(root: &Path) -> Vec<String> {
        match load_manifest(root) {
            Err(errors) => errors.iter().map(ToString::to_string).collect(),
            Ok(manifest) => check_reads(&root.join(PAYLOADS_FILE), &manifest, &payload_reads(root))
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }

    /// The committed manifest loads, every pin holds, and the workloads read
    /// every entry it carries.
    #[test]
    fn the_committed_manifest_loads_clean_and_is_read_whole() {
        assert_eq!(messages(&committed()), Vec::<String>::new());
    }

    /// A source outside `payloads/` and a missing file are each one defect.
    #[test]
    fn a_source_outside_the_tree_and_a_missing_file_are_refused() {
        let dir = staged();
        let manifest = dir.path().join(PAYLOADS_FILE);
        let mut text = std::fs::read_to_string(&manifest).expect("the manifest");
        text.push_str(
            "cnf.unread.escape:\n  source: ../workloads/clinical-mix.yaml\n  provenance: p\n\
             cnf.unread.absent:\n  source: nothing/here.json\n  provenance: p\n",
        );
        std::fs::write(&manifest, text).expect("rewriting the manifest");
        let errors = messages(dir.path());
        for expected in [
            "cnf.unread.escape: source \"../workloads/clinical-mix.yaml\" must be a relative path",
            "cnf.unread.absent: source payloads/nothing/here.json is not a file",
        ] {
            assert!(
                errors.iter().any(|e| e.contains(expected)),
                "{expected} missing from {errors:?}"
            );
        }
        assert_eq!(errors.len(), 2, "{errors:?}");
    }

    /// A pinned file whose bytes moved is refused, naming the key and both
    /// digests.
    #[test]
    fn a_pinned_payload_whose_bytes_moved_is_refused() {
        let dir = staged();
        let manifest = dir.path().join(PAYLOADS_FILE);
        let mut text = std::fs::read_to_string(&manifest).expect("the manifest");
        let pin = "a".repeat(64);
        text.push_str("cnf.pinned.moved:\n  source: ckm/vital-signs.opt\n  sha256: \"");
        text.push_str(&pin);
        text.push_str("\"\n  provenance: p\n");
        std::fs::write(&manifest, text).expect("rewriting the manifest");
        let errors = messages(dir.path());
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].contains("cnf.pinned.moved: sha256") && errors[0].contains("hashes to"),
            "{errors:?}"
        );
    }

    /// A key a workload reads that the manifest lacks, and an entry no
    /// workload reads, are each one defect.
    #[test]
    fn a_missing_read_and_an_unread_entry_are_refused() {
        let dir = staged();
        let manifest = dir.path().join(PAYLOADS_FILE);
        let text = std::fs::read_to_string(&manifest).expect("the manifest");
        let mut body = String::new();
        let mut skip = false;
        for line in text.lines() {
            if line == "cnf.ckm.vital_signs:" {
                skip = true;
                continue;
            }
            if skip && line.starts_with("  ") {
                continue;
            }
            skip = false;
            body.push_str(line);
            body.push('\n');
        }
        body.push_str("cnf.unread.extra:\n  source: ckm/vital-signs.opt\n  provenance: p\n");
        std::fs::write(&manifest, body).expect("rewriting the manifest");
        let errors = messages(dir.path());
        assert!(
            errors
                .iter()
                .any(|e| e.contains("reads cnf.ckm.vital_signs, which the manifest does not carry")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("cnf.unread.extra is a payload no workload reads")),
            "{errors:?}"
        );
    }

    /// A payload only one workload reads is read, and the same entry is
    /// refused once no workload reads it.
    #[test]
    fn a_payload_is_read_while_a_workload_reads_it() {
        let dir = staged();
        let manifest = dir.path().join(PAYLOADS_FILE);
        let mut text = std::fs::read_to_string(&manifest).expect("the manifest");
        text.push_str(
            "cnf.flat.workload_only:\n  source: flat/vitals.minimal_ctx.json\n  \
             template_id: \"minimal_action.en.v1\"\n  provenance: p\n",
        );
        std::fs::write(&manifest, text).expect("rewriting the manifest");
        let workload = dir.path().join("workloads/clinical-mix.yaml");
        let document = std::fs::read_to_string(&workload).expect("the workload");
        let changed = document.replace(
            "payload: cnf.flat.vitals.minimal_ctx",
            "payload: cnf.flat.workload_only",
        );
        assert_ne!(changed, document, "the workload names the Flat payload");
        std::fs::write(&workload, changed).expect("rewriting the workload");
        let errors = messages(dir.path());
        assert!(
            !errors.iter().any(|e| e.contains("cnf.flat.workload_only")),
            "{errors:?}"
        );
        std::fs::write(&workload, document).expect("restoring the workload");
        let errors = messages(dir.path());
        assert!(
            errors
                .iter()
                .any(|e| e.contains("cnf.flat.workload_only is a payload no workload reads")),
            "{errors:?}"
        );
    }
}
