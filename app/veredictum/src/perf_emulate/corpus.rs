// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The corpus a run measures against.
//!
//! This module holds its payloads, the closed-loop seeding that builds it
//! before the clock starts, the stored state a later run reuses it from, and
//! the read-back that refuses a corpus that changed.
//!
//! Seeding is a bulk load, never a measurement: each worker sends its next
//! request after the previous answer, so the server sets the pace, and the
//! record reports it apart and labels it closed-loop. A reused corpus is
//! accepted only when the stored listing still hashes to its stored digest,
//! its recipe is the workload's, and the server's read-back counts equal the
//! stored ones.

#![expect(
    clippy::disallowed_types,
    reason = "the payload fixtures and the ResultSet of the read-back are wire JSON whose shape \
              belongs to the payload files and the server"
)]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::time::Instant;

use crate::perf_emulate::http::{HttpClient, Reply, request_line};
use crate::perf_emulate::posture::InvalidTwin;
use crate::perf_run::pack::{self, FlatPayload, PackTemplate};
use crate::perf_schedule::PayloadManifest;
use crate::workload::load::sha256_hex;
use crate::workload::ops::{PayloadRole, WorkloadOp};
use crate::workload::wire::{
    self, Addressing, FLAT_MEDIA_TYPE, WireBody, WireRequest, stored_query_store,
};
use crate::workload::{CorpusRecipe, Workload};

/// The namespace every seeded subject is registered under.
pub const SUBJECT_NAMESPACE: &str = "org.veredictum.perf";

/// The most targets an update, read or delete pool holds.
pub const POOL_TARGETS: usize = 1_024;

/// The compositions seeded apart for deletion, beyond the corpus.
pub const DELETE_RESERVE: usize = 64;

/// The persons seeded when the workload reads or updates demographics.
pub const PARTIES: usize = 256;

/// The AQL that reads back the EHR count (QUERY AQL `master03-syntax.adoc`
/// §COUNT).
pub const COUNT_EHRS: &str = "SELECT COUNT(e/ehr_id/value) AS ehrs FROM EHR e";

/// The AQL that reads back the composition count.
pub const COUNT_COMPOSITIONS: &str =
    "SELECT COUNT(c/uid/value) AS compositions FROM EHR e CONTAINS COMPOSITION c";

/// The `ETag` of the created family: `201`, or `204` for a minimal create
/// (ITS-REST `docs/overview/Requests_and_responses.md` §Prefer).
pub const CREATED: &[StatusCode] = &[StatusCode::CREATED, StatusCode::NO_CONTENT];

/// A seeding or a reuse that cannot go on.
#[derive(Debug, Error)]
pub enum CorpusError {
    /// A payload does not load.
    #[error("payload: {0}")]
    Payload(String),
    /// A seeding request was refused or failed.
    #[error("seeding: {0}")]
    Seeding(String),
    /// The stored corpus does not read.
    #[error("corpus state {path}: {message}")]
    State {
        /// The file.
        path: String,
        /// What went wrong.
        message: String,
    },
    /// The stored corpus is not the one the server holds or the workload
    /// names.
    #[error("the stored corpus is refused: {0}")]
    Refused(String),
}

/// Every payload a workload sends, loaded once.
#[derive(Debug, Clone, Default)]
pub struct Payloads {
    /// Operational templates with their example skeletons, by payload key.
    pub templates: BTreeMap<String, PackTemplate>,
    /// Simplified Flat bodies with the OPT they are constrained by.
    pub flats: BTreeMap<String, FlatPayload>,
    /// Bodies sent as their bytes, by payload key.
    pub bodies: BTreeMap<String, Value>,
    /// The invalid twin the validation canary commits: the first corpus
    /// template, in key order, whose manifest carries a `<key>.invalid_twin`
    /// entry.
    pub invalid_twin: Option<InvalidTwin>,
}

/// The manifest suffix that names a corpus template's invalid twin.
pub const INVALID_TWIN_SUFFIX: &str = ".invalid_twin";

impl Payloads {
    /// Loads the corpus templates and every stage payload of `workload`.
    ///
    /// A Simplified Flat body is constrained by the operational template of
    /// the manifest entry with the same `template_id` and an `.opt` source.
    ///
    /// # Errors
    /// [`CorpusError::Payload`] naming the key that does not load.
    pub fn load(
        workload: &Workload,
        payloads_dir: &Path,
        manifest: &PayloadManifest,
    ) -> Result<Self, CorpusError> {
        let mut payloads = Self::default();
        let mut wanted: BTreeMap<String, PayloadRole> = workload
            .corpus
            .templates
            .keys()
            .map(|k| (k.to_string(), PayloadRole::Template))
            .collect();
        for interaction in workload.interactions.values() {
            for stage in interaction.stages() {
                if let (Some(key), crate::workload::ops::StageInput::Payload(role)) =
                    (&stage.payload, stage.op.input())
                {
                    let _previous = wanted.insert(key.to_string(), role);
                }
            }
        }
        let read = |source: &str| {
            std::fs::read_to_string(payloads_dir.join(source))
                .map_err(|e| CorpusError::Payload(format!("cannot read {source}: {e}")))
        };
        for key in workload.corpus.templates.keys() {
            let twin_key = format!("{key}{INVALID_TWIN_SUFFIX}");
            if let Some(entry) = manifest.get(&twin_key) {
                payloads.invalid_twin = Some(InvalidTwin {
                    key: twin_key,
                    bytes: read(&entry.source)?,
                });
                break;
            }
        }
        for (key, role) in wanted {
            match role {
                PayloadRole::Template => {
                    let template = PackTemplate::load(payloads_dir, manifest, &key)
                        .map_err(CorpusError::Payload)?;
                    let _previous = payloads.templates.insert(key, template);
                }
                PayloadRole::Flat => {
                    let entry = manifest
                        .get(&key)
                        .ok_or_else(|| CorpusError::Payload(format!("no manifest entry {key}")))?;
                    let template_id = entry.template_id.clone().ok_or_else(|| {
                        CorpusError::Payload(format!("{key} carries no template_id"))
                    })?;
                    let opt = manifest
                        .0
                        .values()
                        .find(|e| {
                            Path::new(&e.source)
                                .extension()
                                .is_some_and(|x| x.eq_ignore_ascii_case("opt"))
                                && e.template_id.as_deref() == Some(template_id.as_str())
                        })
                        .ok_or_else(|| {
                            CorpusError::Payload(format!(
                                "no operational template in the manifest has the template_id \
                                 {template_id} that {key} names"
                            ))
                        })?;
                    let body = serde_json::from_str(&read(&entry.source)?)
                        .map_err(|e| CorpusError::Payload(format!("{key}: {e}")))?;
                    let _previous = payloads.flats.insert(
                        key,
                        FlatPayload {
                            template_id,
                            opt_xml: read(&opt.source)?,
                            body,
                        },
                    );
                }
                PayloadRole::Body => {
                    let entry = manifest
                        .get(&key)
                        .ok_or_else(|| CorpusError::Payload(format!("no manifest entry {key}")))?;
                    let body = serde_json::from_str(&read(&entry.source)?)
                        .map_err(|e| CorpusError::Payload(format!("{key}: {e}")))?;
                    let _previous = payloads.bodies.insert(key, body);
                }
            }
        }
        Ok(payloads)
    }

    /// The operational templates the server must hold, by template id:
    /// every template and every Simplified Flat body's OPT.
    #[must_use]
    pub fn opts(&self) -> BTreeMap<String, String> {
        let mut opts = BTreeMap::new();
        for template in self.templates.values() {
            let _previous = opts.insert(template.template_id.clone(), template.opt_xml.clone());
        }
        for flat in self.flats.values() {
            let _previous = opts.insert(flat.template_id.clone(), flat.opt_xml.clone());
        }
        opts
    }
}

/// One seeded composition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeededComposition {
    /// The payload key of its template.
    pub template: String,
    /// Its version uid at seeding.
    pub version_uid: String,
}

impl SeededComposition {
    /// The versioned object uid: the version uid before its first `::`.
    #[must_use]
    pub fn object_uid(&self) -> &str {
        object_of(&self.version_uid)
    }
}

/// The versioned object part of a version uid.
#[must_use]
pub fn object_of(version_uid: &str) -> &str {
    version_uid.split("::").next().unwrap_or(version_uid)
}

/// One seeded EHR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeededEhr {
    /// The `ehr_id`.
    pub ehr_id: String,
    /// The subject id its `EHR_STATUS` names in [`SUBJECT_NAMESPACE`].
    pub subject_id: String,
    /// Its compositions.
    pub compositions: Vec<SeededComposition>,
    /// Whether a directory was seeded in it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub directory: bool,
    /// A contribution seeded in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contribution: Option<String>,
}

/// A composition seeded for deletion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReserveComposition {
    /// Its EHR.
    pub ehr_id: String,
    /// Its version uid.
    pub version_uid: String,
}

/// The listing a corpus digest covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listing {
    /// The EHRs.
    pub ehrs: Vec<SeededEhr>,
    /// The compositions reserved for deletion.
    #[serde(default)]
    pub reserve: Vec<ReserveComposition>,
    /// The seeded persons by versioned object uid.
    #[serde(default)]
    pub parties: Vec<String>,
    /// The operational templates by template id.
    #[serde(default)]
    pub templates: Vec<String>,
}

impl Listing {
    /// The `sha256:` digest of the listing's canonical JSON.
    ///
    /// # Errors
    /// The serializer's message.
    pub fn digest(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        Ok(format!("sha256:{}", sha256_hex(&bytes)))
    }

    /// The compositions the listing names, the reserve included.
    #[must_use]
    pub fn compositions(&self) -> u64 {
        let corpus: usize = self.ehrs.iter().map(|e| e.compositions.len()).sum();
        u64::try_from(corpus.saturating_add(self.reserve.len())).unwrap_or(u64::MAX)
    }
}

/// The stored state of a seeded corpus, the file `--corpus reuse` reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusState {
    /// The `sha256:` digest of the recipe it was seeded from.
    pub recipe_sha256: String,
    /// The `sha256:` digest of [`CorpusState::listing`].
    pub digest: String,
    /// The EHRs the server held at the last read-back.
    pub ehrs_read_back: u64,
    /// The compositions the server held at the last read-back.
    pub compositions_read_back: u64,
    /// What was seeded.
    pub listing: Listing,
}

impl CorpusState {
    /// Reads a stored state and checks its digest.
    ///
    /// # Errors
    /// [`CorpusError::State`] when the file does not read or parse, and
    /// [`CorpusError::Refused`] when its listing no longer hashes to its
    /// stored digest.
    pub fn read(path: &Path) -> Result<Self, CorpusError> {
        let state_error = |message: String| CorpusError::State {
            path: path.display().to_string(),
            message,
        };
        let text = std::fs::read_to_string(path).map_err(|e| state_error(e.to_string()))?;
        let state: Self = serde_json::from_str(&text).map_err(|e| state_error(e.to_string()))?;
        let digest = state.listing.digest().map_err(state_error)?;
        if digest != state.digest {
            return Err(CorpusError::Refused(format!(
                "its listing hashes to {digest}, not to the stored {}",
                state.digest
            )));
        }
        Ok(state)
    }

    /// Writes the state as pretty JSON.
    ///
    /// # Errors
    /// [`CorpusError::State`] when the file cannot be written.
    pub fn write(&self, path: &Path) -> Result<(), CorpusError> {
        let state_error = |message: String| CorpusError::State {
            path: path.display().to_string(),
            message,
        };
        let mut text =
            serde_json::to_string_pretty(self).map_err(|e| state_error(e.to_string()))?;
        text.push('\n');
        std::fs::write(path, text).map_err(|e| state_error(e.to_string()))
    }
}

/// The `sha256:` digest of a corpus recipe.
///
/// # Errors
/// The serializer's message.
pub fn recipe_digest(recipe: &CorpusRecipe) -> Result<String, String> {
    let bytes = serde_json::to_vec(recipe).map_err(|e| e.to_string())?;
    Ok(format!("sha256:{}", sha256_hex(&bytes)))
}

/// What the seeding and the read-back cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SeedReport {
    /// Requests sent.
    pub requests: u64,
    /// Wall time, milliseconds.
    pub wall_ms: u64,
}

/// What the workload's operations need beyond the compositions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per optional seeding step, each read once"
)]
pub struct Needs {
    /// Directories in the first EHRs.
    pub directories: bool,
    /// A contribution in the first EHRs.
    pub contributions: bool,
    /// Compositions reserved for deletion.
    pub delete_reserve: bool,
    /// Seeded persons.
    pub parties: bool,
}

impl Needs {
    /// What `ops` read or update.
    #[must_use]
    pub fn of(ops: &[WorkloadOp]) -> Self {
        let has = |op| ops.contains(&op);
        Self {
            directories: has(WorkloadOp::DirectoryRead) || has(WorkloadOp::DirectoryUpdate),
            contributions: has(WorkloadOp::ContributionRead),
            delete_reserve: has(WorkloadOp::CompositionDelete),
            parties: has(WorkloadOp::PartyRead) || has(WorkloadOp::PartyUpdate),
        }
    }
}

/// Counts the requests a seeding sends.
#[derive(Debug, Default)]
struct Counter(AtomicU64);

impl Counter {
    fn one(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

async fn expect(
    client: &HttpClient,
    counter: &Counter,
    request: &WireRequest,
    expected: &[StatusCode],
) -> Result<Reply, CorpusError> {
    counter.one();
    let reply = client
        .send(request)
        .await
        .map_err(|f| CorpusError::Seeding(format!("{}: {f}", request_line(request))))?;
    if expected.contains(&reply.status) {
        Ok(reply)
    } else {
        Err(CorpusError::Seeding(format!(
            "{} answered {}",
            request_line(request),
            reply.status.as_u16()
        )))
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "a request's body is optional, and every caller passes this one as that option"
)]
fn json(bytes: Vec<u8>) -> Option<WireBody> {
    Some(WireBody {
        content_type: "application/json",
        bytes,
    })
}

/// The template of composition `index` of EHR `ehr`: a deterministic draw
/// over the recipe's shares.
fn template_of(recipe: &CorpusRecipe, seed: u64, ehr: u64, index: u64) -> Option<&str> {
    let draw = crate::perf_run::fnv1a(seed, &[ehr, index]) % 1_000_000;
    let mut cumulative: u64 = 0;
    let mut last = None;
    for (key, share) in &recipe.templates {
        cumulative = cumulative.saturating_add(u64::from(share.ppm()));
        last = Some(key.as_str());
        if draw < cumulative {
            return last;
        }
    }
    last
}

/// What one seeding builds.
#[derive(Debug, Clone)]
pub struct SeedRequest<'a> {
    /// The workload's corpus recipe.
    pub recipe: &'a CorpusRecipe,
    /// The EHRs to create: the recipe scale's.
    pub ehrs: u64,
    /// The concurrent closed-loop workers.
    pub workers: usize,
    /// The root of the template draws.
    pub seed: u64,
    /// A token that makes this seeding's subject ids unique on the server.
    pub token: String,
    /// What the operations need beyond the compositions.
    pub needs: Needs,
}

/// Creates one EHR with its subject and returns its id.
async fn create_ehr(
    client: &HttpClient,
    counter: &Counter,
    subject: &str,
) -> Result<String, CorpusError> {
    let body = pack::ehr_status_with_subject(0, Some((subject, SUBJECT_NAMESPACE)));
    let request =
        wire::operation_request(WorkloadOp::EhrCreate, &Addressing::default(), json(body))
            .map_err(|e| CorpusError::Seeding(e.to_string()))?;
    let reply = expect(client, counter, &request, CREATED).await?;
    reply
        .location_id()
        .ok_or_else(|| CorpusError::Seeding("POST /ehr answered without a Location".to_owned()))
}

/// Commits one composition of `template` into `ehr` and returns its
/// version uid.
async fn commit(
    client: &HttpClient,
    counter: &Counter,
    ehr: &str,
    template: &PackTemplate,
    arrival: u64,
) -> Result<String, CorpusError> {
    let body = pack::composition_body(template, arrival, arrival).map_err(CorpusError::Seeding)?;
    let at = Addressing {
        ehr: Some(ehr),
        ..Addressing::default()
    };
    let request = wire::operation_request(WorkloadOp::CompositionCommit, &at, json(body))
        .map_err(|e| CorpusError::Seeding(e.to_string()))?;
    let reply = expect(client, counter, &request, CREATED).await?;
    reply.etag.ok_or_else(|| {
        CorpusError::Seeding(format!(
            "a composition commit into {ehr} answered without an ETag"
        ))
    })
}

/// Seeds the EHRs `worker`, `worker + workers`, … of the corpus.
async fn seed_worker(
    client: HttpClient,
    payloads: Arc<Payloads>,
    request: SeedShape,
    worker: u64,
    counter: Arc<Counter>,
) -> Result<Vec<(u64, SeededEhr)>, CorpusError> {
    let mut seeded = Vec::new();
    let mut index = worker;
    while index < request.ehrs {
        let subject_id = format!("{}-{index}", request.token);
        let ehr_id = create_ehr(&client, &counter, &subject_id).await?;
        let mut compositions = Vec::new();
        for n in 0..u64::from(request.recipe.compositions_per_ehr) {
            let Some(key) = template_of(&request.recipe, request.seed, index, n) else {
                break;
            };
            let template = payloads
                .templates
                .get(key)
                .ok_or_else(|| CorpusError::Payload(format!("{key} is not loaded")))?;
            let version_uid = commit(&client, &counter, &ehr_id, template, n).await?;
            compositions.push(SeededComposition {
                template: key.to_owned(),
                version_uid,
            });
        }
        let first = usize::try_from(index).is_ok_and(|i| i < POOL_TARGETS);
        let mut directory = false;
        if first && request.needs.directories {
            let at = Addressing {
                ehr: Some(&ehr_id),
                ..Addressing::default()
            };
            let req = wire::operation_request(
                WorkloadOp::DirectoryCreate,
                &at,
                json(pack::folder_body(false)),
            )
            .map_err(|e| CorpusError::Seeding(e.to_string()))?;
            let _reply = expect(&client, &counter, &req, CREATED).await?;
            directory = true;
        }
        let mut contribution = None;
        if first
            && request.needs.contributions
            && let Some(template) = payloads.templates.values().next()
        {
            let body = pack::contribution_body(template, 0, index).map_err(CorpusError::Seeding)?;
            let at = Addressing {
                ehr: Some(&ehr_id),
                ..Addressing::default()
            };
            let req = wire::operation_request(WorkloadOp::ContributionCommit, &at, json(body))
                .map_err(|e| CorpusError::Seeding(e.to_string()))?;
            let reply = expect(&client, &counter, &req, CREATED).await?;
            contribution = reply.location_id().or(reply.etag);
        }
        seeded.push((
            index,
            SeededEhr {
                ehr_id,
                subject_id,
                compositions,
                directory,
                contribution,
            },
        ));
        index = index.saturating_add(u64::try_from(request.workers).unwrap_or(1).max(1));
    }
    Ok(seeded)
}

/// The owned part of a [`SeedRequest`] each worker carries.
#[derive(Debug, Clone)]
struct SeedShape {
    recipe: CorpusRecipe,
    ehrs: u64,
    workers: usize,
    seed: u64,
    token: String,
    needs: Needs,
}

/// Uploads every operational template and commits each template's example
/// once into a scratch EHR, so a payload ground the server rejects fails
/// the seeding before any volume is written.
async fn prepare(
    client: &HttpClient,
    payloads: &Payloads,
    counter: &Counter,
    token: &str,
) -> Result<Vec<String>, CorpusError> {
    let mut ids = Vec::new();
    for (template_id, opt) in payloads.opts() {
        let request = wire::template_upload(&opt);
        let _reply = expect(
            client,
            counter,
            &request,
            &[StatusCode::CREATED, StatusCode::CONFLICT],
        )
        .await?;
        ids.push(template_id);
    }
    let scratch = create_ehr(client, counter, &format!("{token}-scratch")).await?;
    for template in payloads.templates.values() {
        let _uid = commit(client, counter, &scratch, template, 0).await?;
    }
    for flat in payloads.flats.values() {
        let at = Addressing {
            ehr: Some(&scratch),
            template_id: Some(&flat.template_id),
            ..Addressing::default()
        };
        let body = pack::flat_body(flat).map_err(CorpusError::Seeding)?;
        let request = wire::operation_request(
            WorkloadOp::CompositionCommitFlat,
            &at,
            Some(WireBody {
                content_type: FLAT_MEDIA_TYPE,
                bytes: body,
            }),
        )
        .map_err(|e| CorpusError::Seeding(e.to_string()))?;
        let _reply = expect(client, counter, &request, CREATED).await?;
    }
    Ok(ids)
}

/// Registers every stored query class the workload runs over a stored
/// transport. A version already registered answers `409`, which counts as
/// registered.
///
/// # Errors
/// [`CorpusError::Seeding`] for any other answer.
pub async fn register_queries(
    client: &HttpClient,
    workload: &Workload,
    requests: &mut u64,
) -> Result<(), CorpusError> {
    let counter = Counter::default();
    for (name, class) in &workload.queries {
        if class.transport.keys().any(|t| t.is_stored()) {
            let request = stored_query_store(name, class);
            let _reply = expect(
                client,
                &counter,
                &request,
                &[
                    StatusCode::OK,
                    StatusCode::CREATED,
                    StatusCode::NO_CONTENT,
                    StatusCode::CONFLICT,
                ],
            )
            .await?;
        }
    }
    *requests = requests.saturating_add(counter.get());
    Ok(())
}

/// Reads back the EHR and composition counts the server holds.
///
/// # Errors
/// [`CorpusError::Seeding`] when a count is not answered as a one-row
/// `ResultSet` holding an integer.
pub async fn read_back(client: &HttpClient, requests: &mut u64) -> Result<(u64, u64), CorpusError> {
    let counter = Counter::default();
    let mut counts = [0_u64; 2];
    for (slot, text) in counts.iter_mut().zip([COUNT_EHRS, COUNT_COMPOSITIONS]) {
        let request = wire::adhoc_text(text).map_err(|e| CorpusError::Seeding(e.to_string()))?;
        let reply = expect(client, &counter, &request, &[StatusCode::OK]).await?;
        let value: Value = serde_json::from_str(&reply.body)
            .map_err(|e| CorpusError::Seeding(format!("the count {text:?} answered: {e}")))?;
        *slot = value
            .get("rows")
            .and_then(|rows| rows.get(0))
            .and_then(|row| row.get(0))
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                CorpusError::Seeding(format!("the count {text:?} answered no integer row"))
            })?;
    }
    *requests = requests.saturating_add(counter.get());
    let [ehrs, compositions] = counts;
    Ok((ehrs, compositions))
}

/// Seeds a corpus: templates and their preflight, the EHRs and their
/// compositions on `workers` closed-loop workers, the targets the
/// operations need, the stored queries, and the read-back.
///
/// # Errors
/// [`CorpusError`] for any refused request or a read-back that fails.
pub async fn seed(
    client: &HttpClient,
    payloads: Arc<Payloads>,
    workload: &Workload,
    request: &SeedRequest<'_>,
) -> Result<(CorpusState, SeedReport), CorpusError> {
    let began = Instant::now();
    let counter = Arc::new(Counter::default());
    let templates = prepare(client, &payloads, &counter, &request.token).await?;
    let shape = SeedShape {
        recipe: request.recipe.clone(),
        ehrs: request.ehrs,
        workers: request.workers.max(1),
        seed: request.seed,
        token: request.token.clone(),
        needs: request.needs,
    };
    let mut tasks = tokio::task::JoinSet::new();
    for worker in 0..u64::try_from(shape.workers).unwrap_or(1) {
        tasks.spawn(seed_worker(
            client.clone(),
            Arc::clone(&payloads),
            shape.clone(),
            worker,
            Arc::clone(&counter),
        ));
    }
    let mut seeded = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        let part = joined.map_err(|e| CorpusError::Seeding(e.to_string()))??;
        seeded.extend(part);
    }
    seeded.sort_by_key(|(index, _)| *index);
    let ehrs: Vec<SeededEhr> = seeded.into_iter().map(|(_, ehr)| ehr).collect();

    let mut reserve = Vec::new();
    if request.needs.delete_reserve
        && let (Some(first), Some(template)) = (ehrs.first(), payloads.templates.values().next())
    {
        for n in 0..u64::try_from(DELETE_RESERVE).unwrap_or(0) {
            let version_uid = commit(client, &counter, &first.ehr_id, template, n).await?;
            reserve.push(ReserveComposition {
                ehr_id: first.ehr_id.clone(),
                version_uid,
            });
        }
    }
    let mut parties = Vec::new();
    if request.needs.parties
        && let Some(person) = payloads.bodies.get(pack::PERSON_KEY)
    {
        for n in 0..u64::try_from(PARTIES).unwrap_or(0) {
            let body = pack::person_body(person, n).map_err(CorpusError::Seeding)?;
            let req = wire::operation_request(
                WorkloadOp::PartyCreate,
                &Addressing::default(),
                json(body),
            )
            .map_err(|e| CorpusError::Seeding(e.to_string()))?;
            let reply = expect(client, &counter, &req, CREATED).await?;
            let uid = reply
                .etag
                .as_deref()
                .map(object_of)
                .map(str::to_owned)
                .or_else(|| reply.location_id())
                .ok_or_else(|| CorpusError::Seeding("a person create named no uid".to_owned()))?;
            parties.push(uid);
        }
    }
    let mut requests = counter.get();
    register_queries(client, workload, &mut requests).await?;
    let (ehrs_read_back, compositions_read_back) = read_back(client, &mut requests).await?;
    let listing = Listing {
        ehrs,
        reserve,
        parties,
        templates,
    };
    let state = CorpusState {
        recipe_sha256: recipe_digest(request.recipe).map_err(CorpusError::Seeding)?,
        digest: listing.digest().map_err(CorpusError::Seeding)?,
        ehrs_read_back,
        compositions_read_back,
        listing,
    };
    let wall_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok((state, SeedReport { requests, wall_ms }))
}

/// Accepts a stored corpus for reuse: its recipe must be the workload's and
/// the server's read-back counts must equal the stored ones; the digest was
/// checked when the state was read.
///
/// # Errors
/// [`CorpusError::Refused`] naming what differs, and every error of
/// [`read_back`] and [`register_queries`].
pub async fn reuse(
    client: &HttpClient,
    workload: &Workload,
    state: &CorpusState,
) -> Result<SeedReport, CorpusError> {
    let began = Instant::now();
    let recipe = recipe_digest(&workload.corpus).map_err(CorpusError::Seeding)?;
    if recipe != state.recipe_sha256 {
        return Err(CorpusError::Refused(format!(
            "it was seeded from recipe {}, and the workload's recipe is {recipe}",
            state.recipe_sha256
        )));
    }
    let mut requests = 0;
    let (ehrs, compositions) = read_back(client, &mut requests).await?;
    if (ehrs, compositions) != (state.ehrs_read_back, state.compositions_read_back) {
        return Err(CorpusError::Refused(format!(
            "the server reads back {ehrs} EHRs and {compositions} compositions, and the stored \
             corpus {} and {}",
            state.ehrs_read_back, state.compositions_read_back
        )));
    }
    register_queries(client, workload, &mut requests).await?;
    let wall_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(SeedReport { requests, wall_ms })
}
