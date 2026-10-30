// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The performance schedule: the input tree of the measured instruments.
//!
//! `perf`, `stress` and `perf-assets` read one root laid out as:
//!
//! ```text
//! <root>/classes/*.yaml    the performance cases, one per class
//! <root>/journeys.yaml     the journey catalogue the workloads decompose into
//! <root>/payloads.yaml     the payload manifest: key, file, template id, provenance
//! <root>/payloads/**       the request bodies and operational templates
//! ```
//!
//! The tree is separate from the conformance catalogue and reads nothing from
//! it: the cases carry their own class, corpus and thresholds, every journey
//! operation carries a fixed wire realization ([`crate::perf::PerfOp`]), and
//! every payload a stage sends is a file this tree carries.
//!
//! Loading never fails fast. Every defect, whether in one file or in a
//! relation between files, is one [`LoadError`] naming the file it is found
//! in, so one load reports the whole tree. The relations held here are the
//! ones a measured run depends on: each workload expands inside the
//! read-heavy derivation band, each stage template resolves to an
//! operational template and its example skeleton, each auxiliary payload a
//! stage needs is present, each manifest source is a file under `payloads/`,
//! and the manifest carries nothing that no stage or seeder reads.
//!
//! NOTE: no openEHR spec governs measured performance (CNF guide
//! `master03-overview.adoc` §Product Scope excludes it) — our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use serde::Deserialize;

use crate::ids::CorpusKey;
use crate::load::{LoadError, compile_schema, load_artifact};
use crate::perf::{JourneyCatalogue, PerfClass, PerformanceCase};
use crate::perf_run::pack;

/// The directory under the root that holds the performance cases.
pub const CLASSES_DIR: &str = "classes";
/// The journey catalogue's file name under the root.
pub const JOURNEYS_FILE: &str = "journeys.yaml";
/// The payload manifest's file name under the root.
pub const PAYLOADS_FILE: &str = "payloads.yaml";
/// The directory under the root every manifest `source` resolves against.
pub const PAYLOADS_DIR: &str = "payloads";

/// One payload the measured instruments send.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadEntry {
    /// The file, relative to `payloads/`.
    pub source: String,
    /// The operational template's wire identifier, for an OPT and for a body
    /// constrained by one.
    #[serde(default)]
    pub template_id: Option<String>,
    /// Where the bytes come from.
    pub provenance: String,
}

/// The payload manifest (`payloads.yaml`), keyed by the name a journey stage
/// or the seeder uses.
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

/// The loaded performance schedule. Files that failed to load are absent;
/// their errors travel beside it in [`Loaded::errors`].
#[derive(Debug, Default)]
pub struct PerfSchedule {
    /// The root the schedule was read from.
    pub root: PathBuf,
    /// The performance cases, with the file each came from, in path order.
    pub cases: Vec<(PathBuf, PerformanceCase)>,
    /// The journey catalogue, when the root carries one.
    pub journeys: Option<(PathBuf, JourneyCatalogue)>,
    /// The payload manifest, when the root carries one.
    pub payloads: Option<(PathBuf, PayloadManifest)>,
}

impl PerfSchedule {
    /// Returns the directory every manifest `source` resolves against.
    #[must_use]
    pub fn payloads_dir(&self) -> PathBuf {
        self.root.join(PAYLOADS_DIR)
    }

    /// Returns every case of `class`, with the file it came from.
    pub fn cases_of_class(
        &self,
        class: PerfClass,
    ) -> impl Iterator<Item = (&Path, &PerformanceCase)> {
        self.cases
            .iter()
            .filter(move |(_, case)| case.class == class)
            .map(|(path, case)| (path.as_path(), case))
    }
}

/// One load pass over a performance schedule root.
#[derive(Debug, Default)]
pub struct Loaded {
    /// Everything that loaded and typed successfully.
    pub schedule: PerfSchedule,
    /// One error per defect, in discovery order.
    pub errors: Vec<LoadError>,
}

/// The three schemas the tree is read through, compiled once per process.
struct Schemas {
    case: jsonschema::Validator,
    journeys: jsonschema::Validator,
    payloads: jsonschema::Validator,
}

static SCHEMAS: LazyLock<Result<Schemas, (PathBuf, String)>> = LazyLock::new(|| {
    let one = |schema, name: &str| {
        compile_schema(&schema, name).map_err(|e| (e.path().to_owned(), e.detail()))
    };
    Ok(Schemas {
        case: one(
            crate::schema::performance_case_schema(),
            "performance-case.schema.json",
        )?,
        journeys: one(
            crate::schema::journey_catalogue_schema(),
            "journey-catalogue.schema.json",
        )?,
        payloads: one(
            crate::schema::perf_payloads_schema(),
            "perf-payloads.schema.json",
        )?,
    })
});

/// Loads the performance schedule under `root`.
///
/// An absent journey catalogue or payload manifest is not an error here: the
/// seam that needs it names it when it is missing.
///
/// # Errors
/// [`LoadError::Schema`] only on a schema-compilation defect in
/// [`crate::schema`] itself, a bug in this crate. Defects in the tree come
/// back as [`Loaded::errors`].
pub fn load(root: &Path) -> Result<Loaded, LoadError> {
    let schemas = SCHEMAS
        .as_ref()
        .map_err(|(path, message)| LoadError::Schema {
            path: path.clone(),
            message: message.clone(),
        })?;
    let mut loaded = Loaded {
        schedule: PerfSchedule {
            root: root.to_owned(),
            ..PerfSchedule::default()
        },
        errors: Vec::new(),
    };

    match yaml_files_in(&root.join(CLASSES_DIR)) {
        Ok(paths) => {
            for path in paths {
                match load_case(&path, &schemas.case) {
                    Ok(case) => loaded.schedule.cases.push((path, case)),
                    Err(e) => loaded.errors.push(e),
                }
            }
        }
        Err(e) => loaded.errors.push(e),
    }
    let journeys = root.join(JOURNEYS_FILE);
    if journeys.exists() {
        match load_artifact::<JourneyCatalogue>(&journeys, &schemas.journeys) {
            Ok(catalogue) => loaded.schedule.journeys = Some((journeys, catalogue)),
            Err(e) => loaded.errors.push(e),
        }
    }
    let payloads = root.join(PAYLOADS_FILE);
    if payloads.exists() {
        match load_artifact::<PayloadManifest>(&payloads, &schemas.payloads) {
            Ok(manifest) => loaded.schedule.payloads = Some((payloads, manifest)),
            Err(e) => loaded.errors.push(e),
        }
    }
    check_relations(&loaded.schedule, &mut loaded.errors);
    Ok(loaded)
}

fn load_case(path: &Path, schema: &jsonschema::Validator) -> Result<PerformanceCase, LoadError> {
    let case: PerformanceCase = load_artifact(path, schema)?;
    case.check_invariants()
        .map_err(|message| LoadError::Model {
            path: path.to_owned(),
            message,
        })?;
    Ok(case)
}

/// The `.yaml` files directly in `dir`, sorted.
///
/// An absent directory carries no case, which the class selection then
/// reports by name; a directory that exists and cannot be read is a defect.
fn yaml_files_in(dir: &Path) -> Result<Vec<PathBuf>, LoadError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let io = |source| LoadError::Io {
        path: dir.to_owned(),
        source,
    };
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "yaml") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn defect(errors: &mut Vec<LoadError>, path: &Path, message: String) {
    errors.push(LoadError::Model {
        path: path.to_owned(),
        message,
    });
}

/// The relations between the files of one schedule.
fn check_relations(schedule: &PerfSchedule, errors: &mut Vec<LoadError>) {
    let mut ids = BTreeSet::new();
    for (path, case) in &schedule.cases {
        if !ids.insert(case.id.as_str()) {
            defect(
                errors,
                path,
                format!("duplicate performance case id {}", case.id),
            );
        }
    }
    if let Some((path, catalogue)) = &schedule.journeys {
        if let Err(message) = catalogue.check_invariants() {
            defect(errors, path, message);
            return;
        }
        for (case_path, case) in &schedule.cases {
            if let Err(message) = catalogue.expansion(&case.workload.journeys) {
                defect(errors, case_path, message);
            }
        }
    }
    if let Some((path, manifest)) = &schedule.payloads {
        check_sources(&schedule.payloads_dir(), path, manifest, errors);
        let catalogue = schedule.journeys.as_ref().map(|(_, c)| c);
        let workload_reads = crate::workload::load::payload_reads(&schedule.root);
        check_payload_keys(
            catalogue,
            !schedule.cases.is_empty(),
            &workload_reads,
            path,
            manifest,
            errors,
        );
    }
}

/// Every source is a relative path inside `payloads/` naming a file.
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
        } else if !dir.join(relative).is_file() {
            defect(
                errors,
                manifest_path,
                format!(
                    "{key}: source {PAYLOADS_DIR}/{} is not a file",
                    entry.source
                ),
            );
        }
    }
}

/// What a payload key is read as, which decides whether it needs a
/// `template_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// An operational template a journey stage or the FLAT stage commits
    /// against, uploaded and named on the wire by its `template_id`.
    Template,
    /// A body or an OPT read for its bytes alone.
    Bytes,
}

/// Every key a stage or the seeder reads is present, an operational template
/// carries its `template_id`, and the manifest carries nothing else.
fn check_payload_keys(
    catalogue: Option<&JourneyCatalogue>,
    has_cases: bool,
    workload_reads: &[crate::workload::load::PayloadRead],
    manifest_path: &Path,
    manifest: &PayloadManifest,
    errors: &mut Vec<LoadError>,
) {
    let mut needed: BTreeMap<String, (Role, String)> = BTreeMap::new();
    if has_cases {
        needed.insert(
            pack::SCALE_OPT_KEY.to_owned(),
            (Role::Bytes, "the scale corpus seeding".to_owned()),
        );
    }
    if let Some(catalogue) = catalogue {
        for (name, journey) in &catalogue.0 {
            for template in journey.stages.iter().filter_map(|s| s.template.as_ref()) {
                needed.insert(
                    template.clone(),
                    (Role::Template, format!("journey {name}")),
                );
                needed.insert(
                    format!("{template}.example"),
                    (Role::Bytes, format!("journey {name}")),
                );
            }
        }
        for kind in pack::needed_aux_payloads(catalogue) {
            for key in pack::aux_payload_keys(kind) {
                let role = if *key == pack::FLAT_OPT_KEY {
                    Role::Template
                } else {
                    Role::Bytes
                };
                needed.insert((*key).to_owned(), (role, format!("the {kind:?} stage")));
            }
        }
    }
    // TODO(#891): this check moves to the workload loader when `journeys.yaml` is retired.
    for read in workload_reads {
        use crate::workload::ops::PayloadRole;
        match read.role {
            PayloadRole::Template => {
                needed.insert(read.key.clone(), (Role::Template, read.reader.clone()));
                needed.insert(
                    format!("{}.example", read.key),
                    (Role::Bytes, read.reader.clone()),
                );
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
    for (key, (role, reader)) in &needed {
        match manifest.get(key) {
            None => defect(
                errors,
                manifest_path,
                format!("{reader} reads {key}, which the manifest does not carry"),
            ),
            Some(entry) if *role == Role::Template && entry.template_id.is_none() => defect(
                errors,
                manifest_path,
                format!(
                    "{key} is an operational template {reader} commits against, but it carries no template_id"
                ),
            ),
            Some(_) => {}
        }
    }
    if catalogue.is_some() {
        for key in manifest.0.keys() {
            if !needed.contains_key(key.as_str()) {
                defect(
                    errors,
                    manifest_path,
                    format!("{key} is a payload no journey stage and no seeder reads"),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed schedule, addressed from this package.
    fn committed() -> PathBuf {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../perf")).to_owned()
    }

    /// Writes `body` at `root/rel`, creating the directories it needs.
    fn put(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("a relative file path has a parent"))
            .expect("creating the parent");
        std::fs::write(path, body).expect("writing the file");
    }

    /// Copies the committed schedule into a temporary root.
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

    fn messages(loaded: &Loaded) -> Vec<String> {
        loaded.errors.iter().map(ToString::to_string).collect()
    }

    /// The committed schedule loads with no defect and carries one case per
    /// class of the ladder.
    #[test]
    fn the_committed_schedule_loads_clean() {
        let loaded = load(&committed()).expect("the schemas compile");
        assert!(loaded.errors.is_empty(), "{:?}", messages(&loaded));
        for class in PerfClass::ALL {
            assert_eq!(
                loaded.schedule.cases_of_class(*class).count(),
                1,
                "class {}",
                class.token()
            );
        }
        assert!(loaded.schedule.journeys.is_some());
        assert!(loaded.schedule.payloads.is_some());
    }

    /// An empty root loads clean and empty, so the seam that needs a piece
    /// names it instead of a load error naming nothing.
    #[test]
    fn an_empty_root_loads_clean_and_empty() {
        let dir = assert_fs::TempDir::new().expect("temp dir");
        let loaded = load(dir.path()).expect("the schemas compile");
        assert!(loaded.errors.is_empty(), "{:?}", messages(&loaded));
        assert!(loaded.schedule.cases.is_empty());
        assert!(loaded.schedule.journeys.is_none());
        assert!(loaded.schedule.payloads.is_none());
    }

    /// A workload outside the read-heavy band is refused against its own case
    /// file, the relation the instruments depend on.
    #[test]
    fn a_workload_outside_the_derivation_band_is_refused() {
        let dir = staged();
        let path = dir.path().join("classes/PERF-hospital_sim-class_POC.yaml");
        let text = std::fs::read_to_string(&path).expect("the POC case");
        let mut body = String::new();
        let mut in_journeys = false;
        for line in text.lines() {
            if line == "  journeys:" {
                in_journeys = true;
                body.push_str("  journeys:\n    admission: 100%\n");
                continue;
            }
            if in_journeys && line.starts_with("    ") {
                continue;
            }
            in_journeys = false;
            body.push_str(line);
            body.push('\n');
        }
        std::fs::write(&path, body).expect("rewriting the case");
        let loaded = load(dir.path()).expect("the schemas compile");
        let errors = messages(&loaded);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].contains("class_POC.yaml") && errors[0].contains("write share"),
            "{errors:?}"
        );
    }

    /// A stage template the manifest does not carry, a source outside
    /// `payloads/`, a missing file and an entry nothing reads are each one
    /// defect against the manifest.
    #[test]
    fn manifest_defects_are_one_error_each() {
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
        body.push_str(
            "cnf.unread.escape:\n  source: ../journeys.yaml\n  provenance: p\n\
             cnf.unread.absent:\n  source: nothing/here.json\n  provenance: p\n",
        );
        std::fs::write(&manifest, body).expect("rewriting the manifest");
        let loaded = load(dir.path()).expect("the schemas compile");
        let errors = messages(&loaded);
        for expected in [
            "reads cnf.ckm.vital_signs, which the manifest does not carry",
            "cnf.unread.escape: source \"../journeys.yaml\" must be a relative path",
            "cnf.unread.absent: source payloads/nothing/here.json is not a file",
            "cnf.unread.escape is a payload no journey stage",
            "cnf.unread.absent is a payload no journey stage",
        ] {
            assert!(
                errors.iter().any(|e| e.contains(expected)),
                "{expected} missing from {errors:?}"
            );
        }
        assert_eq!(errors.len(), 5, "{errors:?}");
    }

    /// A payload only a workload document reads is read, and the same entry
    /// is refused once no workload reads it.
    #[test]
    fn a_workload_only_payload_is_not_refused() {
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
        let document = document.replace(
            "payload: cnf.flat.vitals.minimal_ctx",
            "payload: cnf.flat.workload_only",
        );
        std::fs::write(&workload, document).expect("rewriting the workload");
        let loaded = load(dir.path()).expect("the schemas compile");
        assert_eq!(messages(&loaded), Vec::<String>::new());

        std::fs::remove_file(&workload).expect("removing the workload");
        let loaded = load(dir.path()).expect("the schemas compile");
        let errors = messages(&loaded);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("cnf.flat.workload_only is a payload no journey stage")),
            "{errors:?}"
        );
    }

    /// Two cases with one id are refused against the second file, and a case
    /// outside its schema is refused against itself.
    #[test]
    fn a_duplicate_id_and_a_schema_defect_are_refused() {
        let dir = assert_fs::TempDir::new().expect("temp dir");
        let poc =
            std::fs::read_to_string(committed().join("classes/PERF-hospital_sim-class_POC.yaml"))
                .expect("the POC case");
        put(dir.path(), "classes/a.yaml", &poc);
        put(dir.path(), "classes/b.yaml", &poc);
        put(
            dir.path(),
            "classes/c.yaml",
            "id: PERF-x\nkind: functional\n",
        );
        let loaded = load(dir.path()).expect("the schemas compile");
        let errors = messages(&loaded);
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("b.yaml") && e.contains("duplicate")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("c.yaml") && e.contains("schema")),
            "{errors:?}"
        );
    }
}
