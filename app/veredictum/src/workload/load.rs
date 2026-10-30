// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Loading a workload document through its gates.
//!
//! Loading never fails fast: every defect is one [`LoadError`] naming the
//! file and the field, so one load reports the whole document. The gates are
//! the shares at every level summing to exactly 100 %, the session think-time
//! limit, the stage inputs fitting their operations, the payloads present in
//! `perf/payloads.yaml`, the query classes parsing with the released AQL
//! grammar and binding their parameters as their scope requires, every
//! `requires` token naming a capability folder or an option arm, the
//! expanded mix inside the read band, the qualification limits no looser
//! than the ruled default, and the planned run within ten minutes.
//!
//! A workload whose bytes match a pinned digest is that pinned workload;
//! every other document is `custom`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use openehr_query::parser::parse_str;
use regex::Regex;
use sha2::{Digest as _, Sha256};

use crate::arazzo::capabilities::{CAPABILITIES_FILE, Capabilities};
use crate::arazzo::catalogue::capability_folders;
use crate::arazzo::gates::aql::position;
use crate::ids::{CorpusKey, QueryName};
use crate::load::{LoadError, compile_schema, load_artifact};
use crate::perf_schedule::{PAYLOADS_DIR, PAYLOADS_FILE, PayloadManifest};
use crate::workload::ops::{PayloadRole, QueryTransport, StageInput};
use crate::workload::plan::{PlannedRange, RUN_LIMIT_S, planned_range};
use crate::workload::wire::RESERVED_PARAMETERS;
use crate::workload::{
    ExpandedMix, InteractionShape, ParamSource, Qualification, QueryClass, QueryScope, READ_BAND,
    Share, Stage, WORKLOADS_DIR, Workload, ppm_percent,
};

/// The longest mean think time of a session, in milliseconds.
pub const THINK_TIME_LIMIT_MS: u32 = 10_000;

/// The smallest sample target: a p99 needs 1,000 samples to have 10 above
/// it.
pub const SAMPLE_TARGET_FLOOR: u32 = 1_000;

/// A workload whose digest the binary pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedWorkload {
    /// The workload id.
    pub id: &'static str,
    /// The workload version.
    pub version: u32,
    /// The file name under `perf/workloads/`.
    pub file: &'static str,
    /// The lowercase hex sha256 of the file's bytes.
    pub sha256: &'static str,
}

/// The workloads this binary pins, by digest.
pub const PINNED: &[PinnedWorkload] = &[PinnedWorkload {
    id: "clinical-mix",
    version: 1,
    file: "clinical-mix.yaml",
    sha256: "d4c0c85e6561ed6e8dc8950c9ef5e2b69a22c29c8e66a0169692ad79a57f2e40",
}];

/// What a workload document is, by its digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadIdentity {
    /// A pinned workload, byte for byte.
    Pinned(&'static PinnedWorkload),
    /// Any other document.
    Custom,
}

impl WorkloadIdentity {
    /// Returns the label a record and a rendering carry: `<id>@<version>` for
    /// a pinned workload, `custom` otherwise.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            WorkloadIdentity::Pinned(pin) => format!("{}@{}", pin.id, pin.version),
            WorkloadIdentity::Custom => "custom".to_owned(),
        }
    }
}

/// Returns the lowercase hex sha256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    digest.iter().fold(String::with_capacity(64), |mut out, b| {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Returns what the document with these bytes is.
#[must_use]
pub fn identify(bytes: &[u8]) -> WorkloadIdentity {
    let digest = sha256_hex(bytes);
    PINNED
        .iter()
        .find(|pin| pin.sha256 == digest)
        .map_or(WorkloadIdentity::Custom, WorkloadIdentity::Pinned)
}

/// The names a `requires` token may take: the capability folders under
/// `catalogue/cases/` and the option arms of `catalogue/capabilities.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Vocabulary {
    /// The capability folders.
    pub folders: BTreeSet<String>,
    /// The option arms.
    pub arms: BTreeSet<String>,
}

impl Vocabulary {
    /// Reads the vocabulary from a catalogue root.
    ///
    /// # Errors
    /// [`LoadError`] when the folders do not list or the capabilities file
    /// does not load.
    pub fn from_catalogue(root: &Path) -> Result<Self, LoadError> {
        let folders = capability_folders(root)
            .map_err(|e| LoadError::Model {
                path: root.join("cases"),
                message: e.to_string(),
            })?
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let path = root.join(CAPABILITIES_FILE);
        let capabilities = Capabilities::load(&path).map_err(|e| LoadError::Model {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let arms = capabilities
            .options
            .values()
            .flat_map(|option| option.arms.keys().cloned())
            .collect();
        Ok(Self { folders, arms })
    }

    /// Returns whether `token` is a folder or an arm.
    #[must_use]
    pub fn knows(&self, token: &str) -> bool {
        self.folders.contains(token) || self.arms.contains(token)
    }
}

/// What a workload is checked against.
#[derive(Debug, Clone)]
pub struct LoadContext {
    /// The payload manifest.
    pub payloads: PayloadManifest,
    /// The directory the manifest's sources resolve against.
    pub payloads_dir: PathBuf,
    /// The names `requires` may take.
    pub vocabulary: Vocabulary,
}

static PAYLOADS_SCHEMA: LazyLock<Result<jsonschema::Validator, String>> = LazyLock::new(|| {
    compile_schema(
        &crate::schema::perf_payloads_schema(),
        "perf-payloads.schema.json",
    )
    .map_err(|e| e.to_string())
});

static WORKLOAD_SCHEMA: LazyLock<Result<jsonschema::Validator, String>> = LazyLock::new(|| {
    compile_schema(
        &crate::schema::perf_workload_schema(),
        "perf-workload.schema.json",
    )
    .map_err(|e| e.to_string())
});

fn schema_defect(name: &str, message: &str) -> LoadError {
    LoadError::Schema {
        path: PathBuf::from(name),
        message: message.to_owned(),
    }
}

impl LoadContext {
    /// Reads the context from the performance root and the catalogue root.
    ///
    /// # Errors
    /// [`LoadError`] when the payload manifest or the catalogue vocabulary
    /// does not load.
    pub fn from_roots(perf_root: &Path, catalogue_root: &Path) -> Result<Self, LoadError> {
        let validator = PAYLOADS_SCHEMA
            .as_ref()
            .map_err(|m| schema_defect("perf-payloads.schema.json", m))?;
        let payloads = load_artifact(&perf_root.join(PAYLOADS_FILE), validator)?;
        Ok(Self {
            payloads,
            payloads_dir: perf_root.join(PAYLOADS_DIR),
            vocabulary: Vocabulary::from_catalogue(catalogue_root)?,
        })
    }
}

/// A workload that passed every gate.
#[derive(Debug, Clone)]
pub struct LoadedWorkload {
    /// The file it was read from.
    pub path: PathBuf,
    /// The document.
    pub workload: Workload,
    /// The lowercase hex sha256 of the file's bytes.
    pub sha256: String,
    /// Pinned or custom.
    pub identity: WorkloadIdentity,
    /// The expanded per-operation mix.
    pub mix: ExpandedMix,
    /// The planned run after seeding.
    pub plan: PlannedRange,
}

/// A workload refused by its gates, with every defect.
#[derive(Debug)]
pub struct WorkloadRefused {
    /// The file.
    pub path: PathBuf,
    /// One error per defect, each naming the file and the field.
    pub errors: Vec<LoadError>,
}

impl fmt::Display for WorkloadRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is refused ({} defect(s))",
            self.path.display(),
            self.errors.len()
        )?;
        for error in &self.errors {
            write!(f, "\n  {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for WorkloadRefused {}

/// Loads the workload at `path` through every gate.
///
/// # Errors
/// [`WorkloadRefused`] with every defect found.
pub fn load_file(path: &Path, ctx: &LoadContext) -> Result<LoadedWorkload, WorkloadRefused> {
    let refused = |errors| WorkloadRefused {
        path: path.to_owned(),
        errors,
    };
    let bytes = std::fs::read(path).map_err(|source| {
        refused(vec![LoadError::Io {
            path: path.to_owned(),
            source,
        }])
    })?;
    let validator = WORKLOAD_SCHEMA
        .as_ref()
        .map_err(|m| refused(vec![schema_defect("perf-workload.schema.json", m)]))?;
    let workload: Workload = load_artifact(path, validator).map_err(|e| refused(vec![e]))?;
    let mut errors = Vec::new();
    check(path, &workload, ctx, &mut errors);
    if !errors.is_empty() {
        return Err(refused(errors));
    }
    Ok(LoadedWorkload {
        path: path.to_owned(),
        mix: workload.expansion(),
        plan: planned_range(&workload),
        sha256: sha256_hex(&bytes),
        identity: identify(&bytes),
        workload,
    })
}

/// One payload key a workload document reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadRead {
    /// The key.
    pub key: String,
    /// How it is read.
    pub role: PayloadRole,
    /// The workload that reads it, for a diagnostic.
    pub reader: String,
}

/// Returns every payload key the workload documents under
/// `<perf_root>/workloads/` read: their corpus templates and the payloads of
/// their stages.
///
/// A document that does not load contributes nothing here.
// NOTE: no openEHR spec governs this — our own design; `load_file` reports a defective document by name, so this read only needs the sound ones.
#[must_use]
pub fn payload_reads(perf_root: &Path) -> Vec<PayloadRead> {
    let Ok(validator) = WORKLOAD_SCHEMA.as_ref() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(perf_root.join(WORKLOADS_DIR)) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "yaml"))
        .collect();
    paths.sort();
    let mut reads = Vec::new();
    for path in paths {
        let Ok(workload) = load_artifact::<Workload>(&path, validator) else {
            continue;
        };
        let reader = format!("workload {}", workload.id);
        for key in workload.corpus.templates.keys() {
            reads.push(PayloadRead {
                key: key.to_string(),
                role: PayloadRole::Template,
                reader: reader.clone(),
            });
        }
        for interaction in workload.interactions.values() {
            for stage in interaction.stages() {
                if let (Some(key), StageInput::Payload(role)) = (&stage.payload, stage.op.input()) {
                    reads.push(PayloadRead {
                        key: key.to_string(),
                        role,
                        reader: reader.clone(),
                    });
                }
            }
        }
    }
    reads
}

/// Loads every `.yaml` file under `<perf_root>/workloads/`, in path order.
///
/// # Errors
/// [`LoadError::Io`] when the directory does not list. A refused workload is
/// returned beside the loaded ones.
pub fn load_dir(
    perf_root: &Path,
    ctx: &LoadContext,
) -> Result<(Vec<LoadedWorkload>, Vec<WorkloadRefused>), LoadError> {
    let dir = perf_root.join(WORKLOADS_DIR);
    let io = |source| LoadError::Io {
        path: dir.clone(),
        source,
    };
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "yaml") {
            paths.push(path);
        }
    }
    paths.sort();
    let mut loaded = Vec::new();
    let mut refused = Vec::new();
    for path in paths {
        match load_file(&path, ctx) {
            Ok(workload) => loaded.push(workload),
            Err(e) => refused.push(e),
        }
    }
    Ok((loaded, refused))
}

fn defect(errors: &mut Vec<LoadError>, path: &Path, field: &str, mut message: String) {
    message.insert_str(0, &format!("{field}: "));
    errors.push(LoadError::Model {
        path: path.to_owned(),
        message,
    });
}

fn check_sum<'a>(
    errors: &mut Vec<LoadError>,
    path: &Path,
    field: &str,
    shares: impl Iterator<Item = (String, &'a Share)>,
) {
    let mut total: u64 = 0;
    for (name, share) in shares {
        if share.ppm() == 0 {
            defect(
                errors,
                path,
                &format!("{field}.{name}"),
                "a share of 0% offers nothing".to_owned(),
            );
        }
        total += u64::from(share.ppm());
    }
    if total != u64::from(Share::WHOLE.ppm()) {
        let sum = ppm_percent(total);
        defect(
            errors,
            path,
            field,
            format!("the shares sum to {sum}, not 100%"),
        );
    }
}

/// Every gate over a typed document.
fn check(path: &Path, workload: &Workload, ctx: &LoadContext, errors: &mut Vec<LoadError>) {
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        && stem != workload.id.as_str()
    {
        defect(
            errors,
            path,
            "id",
            format!("{} is not the file stem {stem:?}", workload.id),
        );
    }
    check_sum(
        errors,
        path,
        "interactions",
        workload
            .interactions
            .iter()
            .map(|(n, i)| (n.to_string(), &i.share)),
    );
    check_sum(
        errors,
        path,
        "corpus.templates",
        workload
            .corpus
            .templates
            .iter()
            .map(|(k, s)| (k.to_string(), s)),
    );
    for key in workload.corpus.templates.keys() {
        check_payload(
            errors,
            path,
            "corpus.templates",
            key,
            PayloadRole::Template,
            ctx,
        );
    }
    if workload.corpus.compositions_per_ehr == 0 {
        defect(
            errors,
            path,
            "corpus.compositions_per_ehr",
            "must be at least 1".to_owned(),
        );
    }
    if workload.sample_target.0 < SAMPLE_TARGET_FLOOR {
        defect(
            errors,
            path,
            "sample_target",
            format!(
                "{} is below {SAMPLE_TARGET_FLOOR}, so no plateau could resolve its p99",
                workload.sample_target.0
            ),
        );
    }
    check_qualification(errors, path, &workload.qualification);
    let used = check_interactions(errors, path, workload, ctx);
    for (name, class) in &workload.queries {
        check_query(errors, path, &format!("queries.{name}"), class, ctx);
        if !used.contains(name) {
            defect(
                errors,
                path,
                &format!("queries.{name}"),
                "no interaction runs this query".to_owned(),
            );
        }
    }
    let mix = workload.expansion();
    if !mix.inside_read_band() {
        let (reads, writes) = mix.reads_writes();
        defect(
            errors,
            path,
            "interactions",
            format!(
                "the expanded mix reads {reads} per {writes} writes, outside the {}:1 to {}:1 read band",
                READ_BAND.0, READ_BAND.1
            ),
        );
    }
    let plan = planned_range(workload);
    if !plan.within_limit() {
        defect(
            errors,
            path,
            "interactions",
            format!(
                "the planned run takes up to {} s after seeding ({} s of session tail), over the {RUN_LIMIT_S} s limit",
                plan.max_s, plan.session_tail_s
            ),
        );
    }
}

/// The interactions' gates; returns the query classes their stages run.
fn check_interactions(
    errors: &mut Vec<LoadError>,
    path: &Path,
    workload: &Workload,
    ctx: &LoadContext,
) -> BTreeSet<QueryName> {
    let mut used = BTreeSet::new();
    for (name, interaction) in &workload.interactions {
        let field = format!("interactions.{name}");
        if let InteractionShape::Session(session) = &interaction.shape {
            if session.think_time_mean_ms > THINK_TIME_LIMIT_MS {
                defect(
                    errors,
                    path,
                    &format!("{field}.session.think_time_mean_ms"),
                    format!(
                        "{} ms is over the {THINK_TIME_LIMIT_MS} ms limit of a session",
                        session.think_time_mean_ms
                    ),
                );
            }
            for (i, stage) in session.stages.iter().enumerate() {
                check_stage(
                    errors,
                    path,
                    &format!("{field}.session.stages[{i}]"),
                    stage,
                    workload,
                    ctx,
                    &mut used,
                );
            }
        } else if let InteractionShape::Single(stage) = &interaction.shape {
            check_stage(errors, path, &field, stage, workload, ctx, &mut used);
        }
    }
    used
}

fn check_qualification(errors: &mut Vec<LoadError>, path: &Path, q: &Qualification) {
    let ruled = Qualification::default();
    let field = "qualification";
    if q.operation_p99_ms > ruled.operation_p99_ms {
        defect(
            errors,
            path,
            &format!("{field}.operation_p99_ms"),
            format!(
                "{} ms is looser than the ruled {} ms",
                q.operation_p99_ms, ruled.operation_p99_ms
            ),
        );
    }
    if q.population_aql_p99_ms > ruled.population_aql_p99_ms {
        defect(
            errors,
            path,
            &format!("{field}.population_aql_p99_ms"),
            format!(
                "{} ms is looser than the ruled {} ms",
                q.population_aql_p99_ms, ruled.population_aql_p99_ms
            ),
        );
    }
    if q.max_error_share > ruled.max_error_share {
        defect(
            errors,
            path,
            &format!("{field}.max_error_share"),
            format!(
                "{} is looser than the ruled {}",
                q.max_error_share, ruled.max_error_share
            ),
        );
    }
    if q.min_achieved_share < ruled.min_achieved_share {
        defect(
            errors,
            path,
            &format!("{field}.min_achieved_share"),
            format!(
                "{} is looser than the ruled {}",
                q.min_achieved_share, ruled.min_achieved_share
            ),
        );
    }
}

fn check_stage(
    errors: &mut Vec<LoadError>,
    path: &Path,
    field: &str,
    stage: &Stage,
    workload: &Workload,
    ctx: &LoadContext,
    used: &mut BTreeSet<QueryName>,
) {
    let op = stage.op;
    match op.input() {
        StageInput::None => {
            if stage.payload.is_some() {
                defect(
                    errors,
                    path,
                    &format!("{field}.payload"),
                    format!("{op} sends no payload"),
                );
            }
            if stage.query.is_some() {
                defect(
                    errors,
                    path,
                    &format!("{field}.query"),
                    format!("{op} runs no query"),
                );
            }
        }
        StageInput::Payload(role) => {
            match &stage.payload {
                Some(key) => {
                    check_payload(errors, path, &format!("{field}.payload"), key, role, ctx);
                }
                None => defect(
                    errors,
                    path,
                    &format!("{field}.payload"),
                    format!("{op} sends a payload, and none is named"),
                ),
            }
            if stage.query.is_some() {
                defect(
                    errors,
                    path,
                    &format!("{field}.query"),
                    format!("{op} runs no query"),
                );
            }
        }
        StageInput::Query => {
            if stage.payload.is_some() {
                defect(
                    errors,
                    path,
                    &format!("{field}.payload"),
                    format!("{op} sends no payload"),
                );
            }
            match &stage.query {
                Some(query) if workload.queries.contains_key(query) => {
                    used.insert(query.clone());
                }
                Some(query) => defect(
                    errors,
                    path,
                    &format!("{field}.query"),
                    format!("{query} is not a declared query class"),
                ),
                None => defect(
                    errors,
                    path,
                    &format!("{field}.query"),
                    format!("{op} names the query class it runs"),
                ),
            }
        }
    }
}

fn check_payload(
    errors: &mut Vec<LoadError>,
    path: &Path,
    field: &str,
    key: &CorpusKey,
    role: PayloadRole,
    ctx: &LoadContext,
) {
    let Some(entry) = ctx.payloads.get(key.as_str()) else {
        defect(
            errors,
            path,
            field,
            format!("{key} is not in {PAYLOADS_FILE}"),
        );
        return;
    };
    if !ctx.payloads_dir.join(&entry.source).is_file() {
        defect(
            errors,
            path,
            field,
            format!("{key}: {PAYLOADS_DIR}/{} is not a file", entry.source),
        );
    }
    match role {
        PayloadRole::Template | PayloadRole::Flat if entry.template_id.is_none() => {
            defect(errors, path, field, format!("{key} carries no template_id"));
        }
        _ => {}
    }
    if role == PayloadRole::Template && ctx.payloads.get(&format!("{key}.example")).is_none() {
        defect(
            errors,
            path,
            field,
            format!("{key} has no {key}.example skeleton in {PAYLOADS_FILE}"),
        );
    }
}

static PARAMETER: LazyLock<Result<Regex, regex::Error>> =
    LazyLock::new(|| Regex::new(r"\$([A-Za-z0-9_]+)"));

/// Returns the `$name` parameters of an AQL text.
///
/// # Errors
/// The pattern's compile error, a defect in this module.
pub fn aql_parameters(text: &str) -> Result<BTreeSet<String>, String> {
    let pattern = PARAMETER.as_ref().map_err(ToString::to_string)?;
    Ok(pattern
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_owned()))
        .collect())
}

fn check_query(
    errors: &mut Vec<LoadError>,
    path: &Path,
    field: &str,
    class: &QueryClass,
    ctx: &LoadContext,
) {
    if let Err(error) = parse_str(&class.text) {
        defect(
            errors,
            path,
            &format!("{field}.text"),
            format!(
                "the query does not parse with the released AQL grammar ({})",
                position(&error)
            ),
        );
    }
    match aql_parameters(&class.text) {
        Ok(named) => {
            let declared: BTreeSet<String> = class.parameters.keys().cloned().collect();
            for missing in named.difference(&declared) {
                defect(
                    errors,
                    path,
                    &format!("{field}.parameters"),
                    format!("${missing} has no declared source"),
                );
            }
            for unused in declared.difference(&named) {
                defect(
                    errors,
                    path,
                    &format!("{field}.parameters.{unused}"),
                    "the text names no such parameter".to_owned(),
                );
            }
        }
        Err(message) => defect(errors, path, &format!("{field}.text"), message),
    }
    let binds_ehr = class.parameters.values().any(|s| *s == ParamSource::Ehr);
    match class.scope {
        QueryScope::Population if binds_ehr => defect(
            errors,
            path,
            &format!("{field}.parameters"),
            "a population query binds no EHR scope (ITS-REST docs/query/Request.md, About the ehr_id parameter)".to_owned(),
        ),
        QueryScope::SingleSubject if !binds_ehr => defect(
            errors,
            path,
            &format!("{field}.parameters"),
            "a single-subject query binds the interaction's EHR".to_owned(),
        ),
        _ => {}
    }
    for (name, source) in &class.parameters {
        if let ParamSource::IntRange([min, max]) = source
            && min > max
        {
            defect(
                errors,
                path,
                &format!("{field}.parameters.{name}"),
                format!("[{min}, {max}] is empty"),
            );
        }
    }
    check_sum(
        errors,
        path,
        &format!("{field}.transport"),
        class
            .transport
            .iter()
            .map(|(t, s)| (t.token().to_owned(), s)),
    );
    if class.transport.contains_key(&QueryTransport::StoredPost) && class.fetch.is_none() {
        defect(
            errors,
            path,
            &format!("{field}.fetch"),
            "a stored POST sends fetch, which the Query body requires (ITS-REST specifications/schemas/query/Query.yaml)".to_owned(),
        );
    }
    for name in class.parameters.keys() {
        if RESERVED_PARAMETERS.contains(&name.as_str()) {
            defect(
                errors,
                path,
                &format!("{field}.parameters.{name}"),
                "the query executions use this name for themselves (ITS-REST docs/query/Request.md)".to_owned(),
            );
        }
    }
    for token in &class.requires {
        if !ctx.vocabulary.knows(token) {
            defect(
                errors,
                path,
                &format!("{field}.requires"),
                format!(
                    "{token} is neither a capability folder under catalogue/cases/ nor an option arm of {CAPABILITIES_FILE}"
                ),
            );
        }
    }
}

/// Returns, per query class, the share of operation arrivals it receives.
#[must_use]
pub fn query_weights(workload: &Workload) -> BTreeMap<QueryName, u64> {
    workload
        .queries
        .keys()
        .map(|name| (name.clone(), workload.query_weight(name)))
        .collect()
}
