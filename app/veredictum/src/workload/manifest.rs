// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The workload manifest: every pinned workload described as one emitted
//! document, and the page generated from it.
//!
//! Everything is derived from the loaded documents, the payload manifest and
//! the operation vocabulary, so the description cannot disagree with what a
//! run offers. Every collection is ordered and every share is computed in
//! integers, so the emitted bytes are a function of the committed files
//! alone.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::ids::CorpusKey;
use crate::load::LoadError;
use crate::perf_gen::arrivals::ArrivalProcess;
use crate::workload::load::{
    LoadContext, LoadedWorkload, PINNED, WorkloadIdentity, WorkloadRefused, load_dir, sha256_hex,
};
use crate::workload::ops::{PayloadRole, ReleasedOperation, StageInput};
use crate::workload::plan::RUN_LIMIT_S;
use crate::workload::{
    CorpusScale, InteractionShape, MixStatus, OnRefusal, ParamSource, Qualification, QueryScope,
    QueryTier, Share, WORKLOADS_DIR,
};

/// The file name the manifest is written under.
pub const MANIFEST_FILE: &str = "perf-workloads.json";

/// The file name the generated page is written under.
pub const PAGE_FILE: &str = "perf-workloads.md";

/// Why the manifest cannot be described.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// The context or the directory does not load.
    #[error(transparent)]
    Load(#[from] LoadError),
    /// A committed workload is refused by its gates.
    #[error("{0}")]
    Refused(WorkloadRefused),
    /// A committed workload matches no pinned digest.
    #[error(
        "{path}: sha256 {sha256} matches no pinned workload; a changed workload raises its version and its pin"
    )]
    Unpinned {
        /// The file.
        path: PathBuf,
        /// Its digest.
        sha256: String,
    },
    /// A pinned workload has no committed file.
    #[error("the pinned workload {file} is not under {WORKLOADS_DIR}/")]
    MissingPin {
        /// The pinned file name.
        file: &'static str,
    },
    /// A payload file does not read.
    #[error("{path}: {source}")]
    Payload {
        /// The file.
        path: PathBuf,
        /// The read failure.
        #[source]
        source: std::io::Error,
    },
    /// The manifest does not serialize.
    #[error("the manifest does not serialize: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// The manifest of the pinned workloads.
#[derive(Debug, Clone, Serialize)]
pub struct Manifest {
    /// The workloads, in file order.
    pub workloads: Vec<WorkloadDescription>,
}

/// One workload, described.
#[derive(Debug, Clone, Serialize)]
pub struct WorkloadDescription {
    /// The workload id.
    pub id: String,
    /// The version.
    pub version: u32,
    /// `<id>@<version>` for a pinned workload, `custom` otherwise.
    pub label: String,
    /// Provisional or ratified.
    pub status: MixStatus,
    /// The file under `perf/workloads/`.
    pub file: String,
    /// The lowercase hex sha256 of the file.
    pub sha256: String,
    /// What the workload emulates.
    pub description: String,
    /// The published harness the workload reproduces, when it reproduces
    /// one; the read band then does not gate its mix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reproduces: Option<String>,
    /// The root of every seeded draw.
    pub seed: u64,
    /// The arrival process.
    pub arrival: ArrivalProcess,
    /// Samples per plateau.
    pub sample_target: u32,
    /// The qualification limits.
    pub qualification: Qualification,
    /// The corpus recipe.
    pub corpus: CorpusDescription,
    /// The planned run after seeding.
    pub planned_run: PlanDescription,
    /// Reads per write of the expanded mix, as `r:1`, or `reads only`.
    pub read_write_ratio: String,
    /// Mean operations per interaction arrival, to two decimals.
    pub mean_operations_per_interaction: String,
    /// The interactions.
    pub interactions: Vec<InteractionDescription>,
    /// The expanded per-operation mix.
    pub operations: Vec<OperationDescription>,
    /// The query classes.
    pub queries: Vec<QueryDescription>,
    /// Every payload the workload sends or seeds.
    pub payloads: Vec<PayloadDescription>,
}

/// The corpus recipe, described.
#[derive(Debug, Clone, Serialize)]
pub struct CorpusDescription {
    /// The scale's name.
    pub scale: CorpusScale,
    /// The EHRs it seeds.
    pub ehrs: u64,
    /// Compositions per EHR.
    pub compositions_per_ehr: u32,
    /// The template mix.
    pub templates: Vec<KeyShare>,
}

/// A payload key with its share.
#[derive(Debug, Clone, Serialize)]
pub struct KeyShare {
    /// The payload key.
    pub key: String,
    /// The share.
    pub share: Share,
}

/// The planned run, described.
#[derive(Debug, Clone, Serialize)]
#[expect(
    clippy::struct_field_names,
    reason = "every field is a count of seconds, and the suffix names the unit"
)]
pub struct PlanDescription {
    /// The shortest plan, in seconds.
    pub min_s: u32,
    /// The longest plan, in seconds.
    pub max_s: u32,
    /// The session tail inside both, in seconds.
    pub session_tail_s: u32,
    /// The limit the longest plan is held to.
    pub limit_s: u32,
}

/// One interaction, described.
#[derive(Debug, Clone, Serialize)]
pub struct InteractionDescription {
    /// The name.
    pub name: String,
    /// The share of interaction arrivals.
    pub share: Share,
    /// `single` or `session`.
    pub kind: &'static str,
    /// The mean think time of a session, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub think_time_mean_ms: Option<u32>,
    /// The stages, as operation keys.
    pub stages: Vec<String>,
    /// The folders and arms a statement claims for it to run.
    pub requires: Vec<String>,
    /// What it is.
    pub description: String,
    /// Where its share comes from.
    pub derivation: String,
}

/// One operation of the expanded mix, described.
#[derive(Debug, Clone, Serialize)]
pub struct OperationDescription {
    /// The measurement key.
    pub key: String,
    /// The share of operation arrivals, to two decimals of a percent.
    pub share_of_operations: String,
    /// Whether it changes the server's state.
    pub write: bool,
    /// The released operations it is sent as.
    pub released: Vec<ReleasedDescription>,
}

/// A released operation, described.
#[derive(Debug, Clone, Serialize)]
pub struct ReleasedDescription {
    /// The `operationId`.
    pub operation_id: &'static str,
    /// The method.
    pub method: &'static str,
    /// The path template.
    pub path: &'static str,
    /// The file and route it is cited by.
    pub citation: String,
}

impl From<ReleasedOperation> for ReleasedDescription {
    fn from(op: ReleasedOperation) -> Self {
        Self {
            operation_id: op.operation_id,
            method: op.method,
            path: op.path,
            citation: op.citation(),
        }
    }
}

/// One query class, described.
#[derive(Debug, Clone, Serialize)]
pub struct QueryDescription {
    /// The class name.
    pub name: String,
    /// Its share of operation arrivals.
    pub share_of_operations: String,
    /// One EHR or many.
    pub scope: QueryScope,
    /// Metadata or content.
    pub tier: QueryTier,
    /// The latency budget it is judged against, in milliseconds.
    pub budget_ms: u32,
    /// The AQL text.
    pub text: String,
    /// The parameter sources.
    pub parameters: BTreeMap<String, ParamSource>,
    /// The transport mix.
    pub transport: Vec<TransportDescription>,
    /// What it needs beyond `query`.
    pub requires: Vec<String>,
    /// What a refusal counts as.
    pub on_refusal: OnRefusal,
    /// What it asks.
    pub description: String,
    /// Where its shape comes from.
    pub derivation: String,
}

/// One transport of a query class, described.
#[derive(Debug, Clone, Serialize)]
pub struct TransportDescription {
    /// The transport token.
    pub transport: &'static str,
    /// Its share of the class's arrivals.
    pub share: Share,
    /// The released operation it is sent as.
    pub released: ReleasedDescription,
}

/// One payload, described.
#[derive(Debug, Clone, Serialize)]
pub struct PayloadDescription {
    /// The payload key.
    pub key: String,
    /// The file under `perf/payloads/`.
    pub source: String,
    /// The template's wire identifier, where it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    /// The file's size in bytes.
    pub bytes: u64,
    /// The lowercase hex sha256 of the file.
    pub sha256: String,
    /// Where the bytes come from.
    pub provenance: String,
}

/// Returns `num / den` as a percentage with two decimals, rounded half up.
#[expect(
    clippy::integer_division,
    reason = "fixed-point hundredths, rounded half up"
)]
fn percent(num: u64, den: u64) -> String {
    if den == 0 {
        return "0.00%".to_owned();
    }
    let hundredths = (u128::from(num) * 10_000 + u128::from(den) / 2) / u128::from(den);
    format!("{}.{:02}%", hundredths / 100, hundredths % 100)
}

/// The read:write ratio of a mix that writes nothing.
pub const READS_ONLY: &str = "reads only";

/// Returns `num / den` with two decimals, rounded half up.
#[expect(
    clippy::integer_division,
    reason = "fixed-point hundredths, rounded half up"
)]
fn decimal(num: u64, den: u64) -> String {
    if den == 0 {
        return "n/a".to_owned();
    }
    let hundredths = (u128::from(num) * 100 + u128::from(den) / 2) / u128::from(den);
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

/// Describes one loaded workload.
///
/// # Errors
/// [`ManifestError::Payload`] when a payload file does not read.
#[expect(
    clippy::too_many_lines,
    reason = "one description, built field by field"
)]
pub fn describe(
    loaded: &LoadedWorkload,
    ctx: &LoadContext,
) -> Result<WorkloadDescription, ManifestError> {
    let workload = &loaded.workload;
    let mix = &loaded.mix;
    let total = mix.total();
    let (reads, writes) = mix.reads_writes();
    let qualification = workload.qualification;

    let interactions = workload
        .interactions
        .iter()
        .map(|(name, interaction)| {
            let (kind, think) = match &interaction.shape {
                InteractionShape::Single(_) => ("single", None),
                InteractionShape::Session(s) => ("session", Some(s.think_time_mean_ms)),
            };
            InteractionDescription {
                name: name.to_string(),
                share: interaction.share,
                kind,
                think_time_mean_ms: think,
                stages: interaction
                    .stages()
                    .iter()
                    .map(|s| s.key().to_string())
                    .collect(),
                requires: workload.requirements(interaction).into_iter().collect(),
                description: interaction.description.clone(),
                derivation: interaction.derivation.clone(),
            }
        })
        .collect();

    let mut released_by_key: BTreeMap<String, Vec<ReleasedDescription>> = BTreeMap::new();
    for interaction in workload.interactions.values() {
        for stage in interaction.stages() {
            let key = stage.key().to_string();
            if released_by_key.contains_key(&key) {
                continue;
            }
            let released = match (stage.op.released(), &stage.query) {
                (Some(op), _) => vec![op.into()],
                (None, Some(query)) => workload.queries.get(query).map_or_else(Vec::new, |c| {
                    c.transport.keys().map(|t| t.released().into()).collect()
                }),
                (None, None) => Vec::new(),
            };
            released_by_key.insert(key, released);
        }
    }
    let operations = mix
        .operations
        .iter()
        .map(|(key, entry)| OperationDescription {
            key: key.to_string(),
            share_of_operations: percent(entry.weight, total),
            write: entry.write,
            released: released_by_key.remove(key.as_str()).unwrap_or_default(),
        })
        .collect();

    let queries = workload
        .queries
        .iter()
        .map(|(name, class)| QueryDescription {
            name: name.to_string(),
            share_of_operations: percent(workload.query_weight(name), total),
            scope: class.scope,
            tier: class.tier,
            budget_ms: match class.scope {
                QueryScope::SingleSubject => qualification.operation_p99_ms,
                QueryScope::Population => qualification.population_aql_p99_ms,
            },
            text: class.text.clone(),
            parameters: class.parameters.clone(),
            transport: class
                .transport
                .iter()
                .map(|(t, share)| TransportDescription {
                    transport: t.token(),
                    share: *share,
                    released: t.released().into(),
                })
                .collect(),
            requires: class.requires.clone(),
            on_refusal: class.on_refusal,
            description: class.description.clone(),
            derivation: class.derivation.clone(),
        })
        .collect();

    Ok(WorkloadDescription {
        id: workload.id.to_string(),
        version: workload.version,
        label: loaded.identity.label(),
        status: workload.status,
        file: loaded
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        sha256: loaded.sha256.clone(),
        description: workload.description.clone(),
        reproduces: workload.reproduces.clone(),
        seed: workload.seed,
        arrival: workload.arrival,
        sample_target: workload.sample_target.0,
        qualification,
        corpus: CorpusDescription {
            scale: workload.corpus.scale,
            ehrs: workload.corpus.ehr_count(),
            compositions_per_ehr: workload.corpus.compositions_per_ehr,
            templates: workload
                .corpus
                .templates
                .iter()
                .map(|(key, share)| KeyShare {
                    key: key.to_string(),
                    share: *share,
                })
                .collect(),
        },
        planned_run: PlanDescription {
            min_s: loaded.plan.min_s,
            max_s: loaded.plan.max_s,
            session_tail_s: loaded.plan.session_tail_s,
            limit_s: RUN_LIMIT_S,
        },
        read_write_ratio: if writes == 0 {
            READS_ONLY.to_owned()
        } else {
            format!("{}:1", decimal(reads, writes))
        },
        mean_operations_per_interaction: decimal(total, u64::from(Share::WHOLE.ppm())),
        interactions,
        operations,
        queries,
        payloads: payloads(loaded, ctx)?,
    })
}

/// Every payload key the workload sends or seeds, with each template's
/// `.example` skeleton.
fn payload_keys(loaded: &LoadedWorkload) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let mut template = |key: &CorpusKey| {
        keys.insert(key.to_string());
        keys.insert(format!("{key}.example"));
    };
    for key in loaded.workload.corpus.templates.keys() {
        template(key);
    }
    let mut plain = BTreeSet::new();
    for interaction in loaded.workload.interactions.values() {
        for stage in interaction.stages() {
            if let (StageInput::Payload(role), Some(key)) = (stage.op.input(), &stage.payload) {
                if role == PayloadRole::Template {
                    template(key);
                } else {
                    plain.insert(key.to_string());
                }
            }
        }
    }
    keys.extend(plain);
    keys
}

fn payloads(
    loaded: &LoadedWorkload,
    ctx: &LoadContext,
) -> Result<Vec<PayloadDescription>, ManifestError> {
    let mut out = Vec::new();
    for key in payload_keys(loaded) {
        let Some(entry) = ctx.payloads.get(&key) else {
            continue;
        };
        let path = ctx.payloads_dir.join(&entry.source);
        let bytes = std::fs::read(&path).map_err(|source| ManifestError::Payload {
            path: path.clone(),
            source,
        })?;
        out.push(PayloadDescription {
            key,
            source: entry.source.clone(),
            template_id: entry.template_id.clone(),
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: sha256_hex(&bytes),
            provenance: entry.provenance.clone(),
        });
    }
    Ok(out)
}

/// Describes every committed workload under `<perf_root>/workloads/`,
/// requiring each to match its pin and each pin to be committed.
///
/// # Errors
/// [`ManifestError`] on the first refused, unpinned or missing workload.
pub fn describe_pinned(perf_root: &Path, ctx: &LoadContext) -> Result<Manifest, ManifestError> {
    let (loaded, mut refused) = load_dir(perf_root, ctx)?;
    if !refused.is_empty() {
        return Err(ManifestError::Refused(refused.remove(0)));
    }
    for pin in PINNED {
        if !loaded
            .iter()
            .any(|l| l.path.file_name().is_some_and(|n| n == pin.file))
        {
            return Err(ManifestError::MissingPin { file: pin.file });
        }
    }
    let mut workloads = Vec::new();
    for workload in &loaded {
        if workload.identity == WorkloadIdentity::Custom {
            return Err(ManifestError::Unpinned {
                path: workload.path.clone(),
                sha256: workload.sha256.clone(),
            });
        }
        workloads.push(describe(workload, ctx)?);
    }
    Ok(Manifest { workloads })
}

/// Renders the manifest as its published JSON text.
///
/// # Errors
/// [`ManifestError::Serialize`] when it does not serialize.
pub fn render_json(manifest: &Manifest) -> Result<String, ManifestError> {
    let mut text = serde_json::to_string_pretty(manifest)?;
    text.push('\n');
    Ok(text)
}

/// Returns a one-line summary of a loaded workload: its label, the planned
/// run range, and the read:write ratio, followed by one line per operation
/// of the expanded mix.
#[must_use]
pub fn summary(loaded: &LoadedWorkload) -> String {
    let mix = &loaded.mix;
    let total = mix.total();
    let (reads, writes) = mix.reads_writes();
    let mut out = format!(
        "{} ({}): planned run {} to {} s after seeding (limit {RUN_LIMIT_S} s), reads:writes {}, {} operations per interaction\n",
        loaded.path.display(),
        loaded.identity.label(),
        loaded.plan.min_s,
        loaded.plan.max_s,
        if writes == 0 {
            READS_ONLY.to_owned()
        } else {
            format!("{}:1", decimal(reads, writes))
        },
        decimal(total, u64::from(Share::WHOLE.ppm())),
    );
    if loaded.workload.reproduces.is_some() {
        out.push_str(
            "  reproduces a published harness, so the population read band does not gate it\n",
        );
    }
    for (key, entry) in &mix.operations {
        let _ = writeln!(
            out,
            "  {:<40} {:>8} {}",
            key.as_str(),
            percent(entry.weight, total),
            if entry.write { "write" } else { "read" }
        );
    }
    out
}

// NOTE: no openEHR spec governs this — our own design: derivations quote AQL such as
// `COUNT([DISTINCT] …)`, which Markdown would read as a link, and cite bare URLs.
fn escape_cell(text: &str) -> String {
    text.replace('\n', " ")
        .split(' ')
        .map(|word| {
            let escaped = word
                .replace('|', "\\|")
                .replace('[', "\\[")
                .replace(']', "\\]");
            let Some(at) = escaped.find("https://") else {
                return escaped;
            };
            let (lead, rest) = escaped.split_at(at);
            let end = rest.find([')', ',', ';']).unwrap_or(rest.len());
            let (url, after) = rest.split_at(end);
            let trimmed = url.trim_end_matches('.');
            let (url, dots) = url.split_at(trimmed.len());
            format!("{lead}<{url}>{dots}{after}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Renders the workload page: per workload, its header facts, the
/// interactions, the expanded operation mix, the query classes and the
/// payloads, each with its share and its source.
#[must_use]
pub fn render_page(manifest: &Manifest) -> String {
    let mut out = String::from(
        "# Performance workloads\n\nGenerated by `veredictum perf-workloads` from the pinned workload \
         documents; do not edit by hand. No openEHR specification governs measured \
         performance: the shares are our own derivation.\n",
    );
    for w in &manifest.workloads {
        let _ = write!(
            out,
            "\n## {} v{} ({:?})\n\n{}\n\n- File: `{}` (sha256 `{}`)\n- Seed: {}; arrivals: {:?}; samples per plateau: {}\n- Corpus: {} ({} EHRs), {} compositions per EHR\n- Planned run after seeding: {} to {} s (limit {} s)\n- Reads per write: {}; operations per interaction: {}\n",
            w.id,
            w.version,
            w.status,
            escape_cell(&w.description),
            w.file,
            w.sha256,
            w.seed,
            w.arrival,
            w.sample_target,
            w.corpus.scale.token(),
            w.corpus.ehrs,
            w.corpus.compositions_per_ehr,
            w.planned_run.min_s,
            w.planned_run.max_s,
            w.planned_run.limit_s,
            w.read_write_ratio,
            w.mean_operations_per_interaction,
        );
        if let Some(source) = &w.reproduces {
            let _ = writeln!(
                out,
                "- Reproduces: {}. Its mix is the harness's, so the population read band \
                 does not gate it.",
                escape_cell(source.trim_end_matches('.'))
            );
        }
        render_tables(&mut out, w);
    }
    out
}

/// Appends one workload's interaction, operation, query and payload tables,
/// each headed with the workload id so every heading on the page is unique.
fn render_tables(out: &mut String, w: &WorkloadDescription) {
    let _ = write!(
        out,
        "\n### Interactions of {}\n\n| Interaction | Share | Stages | Source |\n|---|---|---|---|\n",
        w.id
    );
    for i in &w.interactions {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            i.name,
            i.share,
            escape_cell(&i.stages.join(", ")),
            escape_cell(&i.derivation)
        );
    }
    let _ = write!(
        out,
        "\n### Operations of {}\n\n| Operation | Share of operations | Kind | Released operation |\n|---|---|---|---|\n",
        w.id
    );
    for o in &w.operations {
        let released = o
            .released
            .iter()
            .map(|r| format!("`{}` ({} {})", r.operation_id, r.method, r.path))
            .collect::<Vec<_>>()
            .join("; ");
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            o.key,
            o.share_of_operations,
            if o.write { "write" } else { "read" },
            escape_cell(&released)
        );
    }
    let _ = write!(
        out,
        "\n### Queries of {}\n\n| Query | Share of operations | Scope | Transport | Source |\n|---|---|---|---|---|\n",
        w.id
    );
    for q in &w.queries {
        let transport = q
            .transport
            .iter()
            .map(|t| format!("{} {}", t.transport, t.share))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "| {} | {} | {:?} | {} | {} |",
            q.name,
            q.share_of_operations,
            q.scope,
            escape_cell(&transport),
            escape_cell(&q.derivation)
        );
    }
    let _ = write!(
        out,
        "\n### Payloads of {}\n\n| Payload | Bytes | Source |\n|---|---|---|\n",
        w.id
    );
    for p in &w.payloads {
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            p.key,
            p.bytes,
            escape_cell(&p.provenance)
        );
    }
}
