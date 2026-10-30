// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The performer that realizes every call of a bound workload as one
//! released ITS-REST exchange against the seeded corpus.
//!
//! An interaction addresses one EHR of the corpus throughout, drawn from
//! the seed and its arrival index. Updates lease their target from a pool,
//! so no two in-flight updates address one versioned object and no `412`
//! is the instrument's own doing; a delete takes a composition the run
//! committed itself, or one of the seeded reserve. An update or delete that
//! finds no free target is counted as [`ErrorClass::NoTarget`], a defect of
//! the run that still fails the plateau. A refusal of a query class whose
//! `on_refusal` is `count_apart` is [`Outcome::Refused`].
//!
//! No openEHR specification governs the mix: our own design. Each request
//! is the released operation [`crate::workload::ops`] cites for its token.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use reqwest::StatusCode;

use crate::ids::QueryName;
use crate::perf_emulate::binding::Binding;
use crate::perf_emulate::corpus::{
    CREATED, CorpusState, POOL_TARGETS, Payloads, SUBJECT_NAMESPACE, object_of,
};
use crate::perf_emulate::http::HttpClient;
use crate::perf_gen::pool::LeasePool;
use crate::perf_gen::{Call, ErrorClass, FailedExchange, Outcome, Performer};
use crate::perf_run::pack;
use crate::workload::ops::{QueryTransport, WorkloadOp};
use crate::workload::wire::{self, Addressing, FLAT_MEDIA_TYPE, ParamValue, WireBody, WireRequest};
use crate::workload::{OnRefusal, ParamSource, QueryClass, Stage, Workload};

const OK: &[StatusCode] = &[StatusCode::OK];
const UPDATED: &[StatusCode] = &[StatusCode::OK, StatusCode::NO_CONTENT];
const DELETED: &[StatusCode] = &[StatusCode::NO_CONTENT];

/// Separate draw streams under one seed.
const STREAM_EHR: u64 = 0x0065_6872;
const STREAM_PICK: u64 = 0x7069_636b;
const STREAM_TRANSPORT: u64 = 0x7472_616e;
const STREAM_PARAM: u64 = 0x7061_7261;

/// A versioned object an update leases: its EHR, its object uid and its
/// current version uid.
#[derive(Debug)]
pub struct Versioned {
    read: WorkloadOp,
    ehr: String,
    object: String,
    current: Mutex<String>,
    subject: Option<String>,
}

impl Versioned {
    fn new(
        read: WorkloadOp,
        ehr: &str,
        object: &str,
        current: &str,
        subject: Option<&str>,
    ) -> Arc<Self> {
        Arc::new(Self {
            read,
            ehr: ehr.to_owned(),
            object: object.to_owned(),
            current: Mutex::new(current.to_owned()),
            subject: subject.map(str::to_owned),
        })
    }

    fn current(&self) -> String {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn advance(&self, next: Option<String>) {
        let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        *current = next.unwrap_or_else(|| next_version(&current));
    }
}

/// The version uid after `version_uid` in its trunk: the version tree id's
/// last number raised by one (RM `common` §`OBJECT_VERSION_ID` and
/// §`VERSION_TREE_ID`), for an update answered without an `ETag`.
#[must_use]
pub fn next_version(version_uid: &str) -> String {
    match version_uid.rsplit_once("::") {
        Some((head, tree)) => match tree.parse::<u64>() {
            Ok(n) => format!("{head}::{}", n.saturating_add(1)),
            Err(_) => version_uid.to_owned(),
        },
        None => version_uid.to_owned(),
    }
}

/// The targets a run addresses.
#[derive(Debug)]
pub struct Targets {
    state: CorpusState,
    status: LeasePool<Arc<Versioned>>,
    directories: LeasePool<Arc<Versioned>>,
    compositions: BTreeMap<String, LeasePool<Arc<Versioned>>>,
    parties: LeasePool<Arc<Versioned>>,
    deletable: Mutex<VecDeque<(String, String)>>,
    without_directory: Mutex<Vec<String>>,
}

impl Targets {
    /// Builds the target pools over a corpus whose versions are current:
    /// the first EHRs' statuses and directories, the first compositions of
    /// each template, the persons, and the deletion reserve.
    ///
    /// `current` maps an object uid (or an `ehr_id` for a status or a
    /// directory, keyed `status:<id>` and `directory:<id>`) to the version
    /// uid read before the clock.
    #[must_use]
    pub fn new(state: CorpusState, current: &BTreeMap<String, String>) -> Self {
        let first = state.listing.ehrs.iter().take(POOL_TARGETS);
        let status = first
            .clone()
            .filter_map(|e| {
                current.get(&format!("status:{}", e.ehr_id)).map(|v| {
                    Versioned::new(
                        WorkloadOp::EhrStatusRead,
                        &e.ehr_id,
                        object_of(v),
                        v,
                        Some(&e.subject_id),
                    )
                })
            })
            .collect();
        let directories = first
            .clone()
            .filter(|e| e.directory)
            .filter_map(|e| {
                current.get(&format!("directory:{}", e.ehr_id)).map(|v| {
                    Versioned::new(WorkloadOp::DirectoryRead, &e.ehr_id, object_of(v), v, None)
                })
            })
            .collect();
        let mut by_template: BTreeMap<String, Vec<Arc<Versioned>>> = BTreeMap::new();
        for ehr in &state.listing.ehrs {
            for composition in &ehr.compositions {
                let pool = by_template.entry(composition.template.clone()).or_default();
                if pool.len() < POOL_TARGETS {
                    let object = composition.object_uid();
                    let version = current
                        .get(object)
                        .map_or(composition.version_uid.as_str(), String::as_str);
                    pool.push(Versioned::new(
                        WorkloadOp::CompositionReadCurrent,
                        &ehr.ehr_id,
                        object,
                        version,
                        None,
                    ));
                }
            }
        }
        let parties = state
            .listing
            .parties
            .iter()
            .filter_map(|uid| {
                current
                    .get(uid)
                    .map(|v| Versioned::new(WorkloadOp::PartyRead, "", uid, v, None))
            })
            .collect();
        let deletable = state
            .listing
            .reserve
            .iter()
            .map(|r| (r.ehr_id.clone(), r.version_uid.clone()))
            .collect();
        let without_directory = state
            .listing
            .ehrs
            .iter()
            .filter(|e| !e.directory)
            .map(|e| e.ehr_id.clone())
            .collect();
        Self {
            status: LeasePool::new(status),
            directories: LeasePool::new(directories),
            compositions: by_template
                .into_iter()
                .map(|(k, v)| (k, LeasePool::new(v)))
                .collect(),
            parties: LeasePool::new(parties),
            deletable: Mutex::new(deletable),
            without_directory: Mutex::new(without_directory),
            state,
        }
    }

    /// The stored corpus the targets were built over.
    #[must_use]
    pub fn state(&self) -> &CorpusState {
        &self.state
    }
}

/// The version uids a run's pools start from, read before the clock: the
/// status and directory of the first EHRs, the first compositions of each
/// template the workload updates, and the persons.
///
/// # Errors
/// A message naming the read the server refused.
pub async fn read_current(
    client: &HttpClient,
    state: &CorpusState,
    binding: &Binding,
    requests: &mut u64,
) -> Result<BTreeMap<String, String>, String> {
    let ops = binding.operations();
    let mut current = BTreeMap::new();
    let read = |request: WireRequest, key: String| async move {
        let reply = client
            .send(&request)
            .await
            .map_err(|f| format!("{}: {f}", wire_line(&request)))?;
        if reply.status != StatusCode::OK {
            return Err(format!(
                "{} answered {}",
                wire_line(&request),
                reply.status.as_u16()
            ));
        }
        reply
            .etag
            .map(|etag| (key, etag))
            .ok_or_else(|| format!("{} answered without an ETag", wire_line(&request)))
    };
    let mut count: u64 = 0;
    let mut wanted = Vec::new();
    for ehr in state.listing.ehrs.iter().take(POOL_TARGETS) {
        let at = Addressing {
            ehr: Some(&ehr.ehr_id),
            ..Addressing::default()
        };
        if ops.contains(&WorkloadOp::EhrStatusUpdate) {
            wanted.push((
                wire::operation_request(WorkloadOp::EhrStatusRead, &at, None),
                format!("status:{}", ehr.ehr_id),
            ));
        }
        if ehr.directory && ops.contains(&WorkloadOp::DirectoryUpdate) {
            wanted.push((
                wire::operation_request(WorkloadOp::DirectoryRead, &at, None),
                format!("directory:{}", ehr.ehr_id),
            ));
        }
    }
    let updated: Vec<&str> = binding
        .stages
        .iter()
        .flatten()
        .filter(|s| s.op == WorkloadOp::CompositionUpdate)
        .filter_map(|s| s.payload.as_ref().map(crate::ids::CorpusKey::as_str))
        .collect();
    let mut per_template: BTreeMap<&str, usize> = BTreeMap::new();
    for ehr in &state.listing.ehrs {
        for composition in &ehr.compositions {
            let seen = per_template
                .entry(composition.template.as_str())
                .or_insert(0);
            if updated.contains(&composition.template.as_str()) && *seen < POOL_TARGETS {
                *seen += 1;
                let at = Addressing {
                    ehr: Some(&ehr.ehr_id),
                    uid: Some(composition.object_uid()),
                    ..Addressing::default()
                };
                wanted.push((
                    wire::operation_request(WorkloadOp::CompositionReadCurrent, &at, None),
                    composition.object_uid().to_owned(),
                ));
            }
        }
    }
    for uid in &state.listing.parties {
        let at = Addressing {
            uid: Some(uid),
            ..Addressing::default()
        };
        wanted.push((
            wire::operation_request(WorkloadOp::PartyRead, &at, None),
            uid.clone(),
        ));
    }
    for (request, key) in wanted {
        let request = request.map_err(|e| e.to_string())?;
        count = count.saturating_add(1);
        let (key, etag) = read(request, key).await?;
        let _previous = current.insert(key, etag);
    }
    *requests = requests.saturating_add(count);
    Ok(current)
}

fn wire_line(request: &WireRequest) -> String {
    crate::perf_emulate::http::request_line(request)
}

/// The performer of a bound workload.
#[derive(Debug)]
pub struct WirePerformer {
    client: HttpClient,
    binding: Arc<Binding>,
    queries: BTreeMap<QueryName, QueryClass>,
    payloads: Arc<Payloads>,
    targets: Targets,
    seed: u64,
}

impl WirePerformer {
    /// Builds the performer.
    #[must_use]
    pub fn new(
        client: HttpClient,
        binding: Arc<Binding>,
        workload: &Workload,
        payloads: Arc<Payloads>,
        targets: Targets,
    ) -> Self {
        Self {
            client,
            binding,
            queries: workload.queries.clone(),
            payloads,
            targets,
            seed: workload.seed,
        }
    }

    /// The targets, with the stored corpus.
    #[must_use]
    pub fn targets(&self) -> &Targets {
        &self.targets
    }

    fn draw(&self, stream: u64, parts: &[u64]) -> u64 {
        let mut keyed = vec![stream];
        keyed.extend_from_slice(parts);
        crate::perf_run::fnv1a(self.seed, &keyed)
    }

    fn pick<'a, T>(&self, items: &'a [T], parts: &[u64]) -> Option<&'a T> {
        let len = u64::try_from(items.len()).ok().filter(|n| *n > 0)?;
        let index = usize::try_from(self.draw(STREAM_PICK, parts) % len).ok()?;
        items.get(index)
    }

    fn no_target(op: WorkloadOp) -> Outcome {
        Outcome::Failure {
            class: ErrorClass::NoTarget,
            exchange: Some(FailedExchange::new(
                None,
                op.token(),
                "the run had no free target to address",
            )),
        }
    }

    fn defect(op: WorkloadOp, message: &str) -> Outcome {
        Outcome::Failure {
            class: ErrorClass::NoTarget,
            exchange: Some(FailedExchange::new(None, op.token(), message)),
        }
    }

    async fn send(
        &self,
        request: Result<WireRequest, wire::WireError>,
        op: WorkloadOp,
        expected: &[StatusCode],
    ) -> (Outcome, Option<crate::perf_emulate::http::Reply>) {
        match request {
            Ok(request) => self.client.judged(&request, expected).await,
            Err(e) => (Self::defect(op, &e.to_string()), None),
        }
    }

    async fn update(
        &self,
        op: WorkloadOp,
        pool: Option<&LeasePool<Arc<Versioned>>>,
        body: impl FnOnce(&Versioned) -> Result<Vec<u8>, String>,
    ) -> Outcome {
        let Some(lease) = pool.and_then(LeasePool::lease) else {
            return Self::no_target(op);
        };
        let Some(target) = lease.target() else {
            return Self::no_target(op);
        };
        let body = match body(target) {
            Ok(body) => body,
            Err(e) => return Self::defect(op, &e),
        };
        let preceding = target.current();
        let at = Addressing {
            ehr: (!target.ehr.is_empty()).then_some(target.ehr.as_str()),
            uid: Some(&target.object),
            if_match: Some(&preceding),
            ..Addressing::default()
        };
        let request = wire::operation_request(op, &at, json(body));
        let (outcome, reply) = self.send(request, op, UPDATED).await;
        if outcome == Outcome::Success {
            target.advance(reply.and_then(|r| r.etag));
        } else if let Some(fresh) = self.refresh(target).await {
            target.advance(Some(fresh));
        }
        outcome
    }

    /// Re-reads a target's current version after a failed update, so one
    /// refused update does not fail every later one.
    async fn refresh(&self, target: &Versioned) -> Option<String> {
        let at = Addressing {
            ehr: (!target.ehr.is_empty()).then_some(target.ehr.as_str()),
            uid: Some(&target.object),
            ..Addressing::default()
        };
        let request = wire::operation_request(target.read, &at, None).ok()?;
        self.client.send(&request).await.ok()?.etag
    }

    async fn query(
        &self,
        call: &Call,
        stage: &Stage,
        ehr: &crate::perf_emulate::corpus::SeededEhr,
    ) -> Outcome {
        let Some(name) = &stage.query else {
            return Self::defect(WorkloadOp::Aql, "an aql stage names no query");
        };
        let Some(class) = self.queries.get(name) else {
            return Self::defect(WorkloadOp::Aql, "the query class is not declared");
        };
        let transport = self.transport(class, call);
        let mut values = BTreeMap::new();
        let mut others: u64 = 0;
        for (index, (param, source)) in class.parameters.iter().enumerate() {
            let index = u64::try_from(index).unwrap_or(0);
            let value = match source {
                ParamSource::Ehr => ParamValue::Text(ehr.ehr_id.clone()),
                ParamSource::OtherEhr => {
                    others = others.saturating_add(1);
                    let ehrs = &self.targets.state.listing.ehrs;
                    let other = self
                        .pick(ehrs, &[call.arrival, others, STREAM_PARAM])
                        .filter(|o| o.ehr_id != ehr.ehr_id)
                        .or_else(|| ehrs.iter().find(|o| o.ehr_id != ehr.ehr_id))
                        .map_or_else(|| ehr.ehr_id.clone(), |o| o.ehr_id.clone());
                    ParamValue::Text(other)
                }
                ParamSource::Composition => {
                    match self.pick(&ehr.compositions, &[call.arrival, index]) {
                        Some(c) => ParamValue::Text(c.version_uid.clone()),
                        None => return Self::no_target(WorkloadOp::Aql),
                    }
                }
                ParamSource::IntRange([low, high]) => {
                    let span = high.saturating_sub(*low).saturating_add(1).max(1);
                    let span = u64::try_from(span).unwrap_or(1);
                    let offset =
                        i64::try_from(self.draw(STREAM_PARAM, &[call.arrival, index]) % span)
                            .unwrap_or(0);
                    ParamValue::Integer(low.saturating_add(offset))
                }
            };
            let _previous = values.insert(param.clone(), value);
        }
        let request = wire::query_execute(name, class, transport, &values, Some(&ehr.ehr_id));
        let (outcome, reply) = self.send(request, WorkloadOp::Aql, OK).await;
        match (&outcome, reply, class.on_refusal) {
            (
                Outcome::Failure {
                    class: ErrorClass::ClientError,
                    ..
                },
                Some(reply),
                OnRefusal::CountApart,
            ) if reply.status.is_client_error() => Outcome::Refused,
            _ => outcome,
        }
    }

    fn transport(&self, class: &QueryClass, call: &Call) -> QueryTransport {
        let total: u64 = class.transport.values().map(|s| u64::from(s.ppm())).sum();
        let mut draw =
            self.draw(STREAM_TRANSPORT, &[call.arrival, u64::from(call.stage)]) % total.max(1);
        let mut chosen = QueryTransport::AdhocPost;
        for (transport, share) in &class.transport {
            chosen = *transport;
            let weight = u64::from(share.ppm());
            if draw < weight {
                break;
            }
            draw -= weight;
        }
        chosen
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per operation of the closed vocabulary"
    )]
    async fn realize(&self, call: &Call) -> Outcome {
        let Some(stage) = self.binding.stage(call.interaction, call.stage) else {
            return Self::defect(WorkloadOp::Aql, "the call names no bound stage");
        };
        let op = stage.op;
        let ehrs = &self.targets.state.listing.ehrs;
        let Some(ehr) = self.pick(ehrs, &[STREAM_EHR, call.arrival]) else {
            return Self::no_target(op);
        };
        let addressed = Addressing {
            ehr: Some(&ehr.ehr_id),
            ..Addressing::default()
        };
        let composition = self.pick(&ehr.compositions, &[call.arrival, u64::from(call.stage)]);
        let template = stage
            .payload
            .as_ref()
            .and_then(|k| self.payloads.templates.get(k.as_str()));
        match op {
            WorkloadOp::EhrCreate => {
                let subject = format!("run-{}-{}", self.seed, call.arrival);
                let body = pack::ehr_status_with_subject(
                    call.arrival,
                    Some((&subject, SUBJECT_NAMESPACE)),
                );
                let request = wire::operation_request(op, &Addressing::default(), json(body));
                let (outcome, reply) = self.send(request, op, CREATED).await;
                if let Some(id) = reply.and_then(|r| r.location_id()) {
                    self.targets
                        .without_directory
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(id);
                }
                outcome
            }
            WorkloadOp::EhrReadBySubject => {
                match wire::ehr_read_by_subject(&ehr.subject_id, SUBJECT_NAMESPACE) {
                    Some(request) => self.client.judged(&request, OK).await.0,
                    None => Self::defect(op, "the subject lookup is not a released operation"),
                }
            }
            WorkloadOp::EhrRead
            | WorkloadOp::EhrStatusRead
            | WorkloadOp::ContributionRead
            | WorkloadOp::TemplateList
            | WorkloadOp::TemplateAdl2List
            | WorkloadOp::SystemOptions => {
                let contributions: Vec<(&str, &str)> = ehrs
                    .iter()
                    .filter_map(|e| e.contribution.as_deref().map(|c| (e.ehr_id.as_str(), c)))
                    .collect();
                let at = if op == WorkloadOp::ContributionRead {
                    let Some((ehr_id, uid)) = self.pick(&contributions, &[call.arrival]) else {
                        return Self::no_target(op);
                    };
                    Addressing {
                        ehr: Some(ehr_id),
                        uid: Some(uid),
                        ..Addressing::default()
                    }
                } else {
                    addressed
                };
                self.send(wire::operation_request(op, &at, None), op, OK)
                    .await
                    .0
            }
            WorkloadOp::EhrStatusUpdate => {
                self.update(op, Some(&self.targets.status), |t| {
                    Ok(pack::ehr_status_with_subject(
                        call.arrival,
                        t.subject.as_deref().map(|s| (s, SUBJECT_NAMESPACE)),
                    ))
                })
                .await
            }
            WorkloadOp::CompositionCommit | WorkloadOp::ContributionCommit => {
                let Some(template) = template else {
                    return Self::defect(op, "the stage's template is not loaded");
                };
                let body = if op == WorkloadOp::CompositionCommit {
                    pack::composition_body(template, call.arrival, call.arrival)
                } else {
                    pack::contribution_body(template, call.arrival, call.arrival)
                };
                let body = match body {
                    Ok(body) => body,
                    Err(e) => return Self::defect(op, &e),
                };
                let request = wire::operation_request(op, &addressed, json(body));
                let (outcome, reply) = self.send(request, op, CREATED).await;
                if op == WorkloadOp::CompositionCommit
                    && let Some(version) = reply.and_then(|r| r.etag)
                    && outcome == Outcome::Success
                {
                    self.targets
                        .deletable
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push_back((ehr.ehr_id.clone(), version));
                }
                outcome
            }
            WorkloadOp::CompositionCommitFlat => {
                let Some(flat) = stage
                    .payload
                    .as_ref()
                    .and_then(|k| self.payloads.flats.get(k.as_str()))
                else {
                    return Self::defect(op, "the stage's Simplified Flat body is not loaded");
                };
                let body = match pack::flat_body(flat) {
                    Ok(body) => body,
                    Err(e) => return Self::defect(op, &e),
                };
                let at = Addressing {
                    ehr: Some(&ehr.ehr_id),
                    template_id: Some(&flat.template_id),
                    ..Addressing::default()
                };
                let body = Some(WireBody {
                    content_type: FLAT_MEDIA_TYPE,
                    bytes: body,
                });
                self.send(wire::operation_request(op, &at, body), op, CREATED)
                    .await
                    .0
            }
            WorkloadOp::CompositionRead
            | WorkloadOp::CompositionReadCurrent
            | WorkloadOp::CompositionRevisionHistory
            | WorkloadOp::CompositionVersionRead
            | WorkloadOp::TagsRead
            | WorkloadOp::TagsPut => {
                let Some(composition) = composition else {
                    return Self::no_target(op);
                };
                let uid = match op {
                    WorkloadOp::CompositionRead | WorkloadOp::CompositionVersionRead => {
                        composition.version_uid.as_str()
                    }
                    _ => composition.object_uid(),
                };
                let at = Addressing {
                    ehr: Some(&ehr.ehr_id),
                    uid: Some(uid),
                    object: Some(composition.object_uid()),
                    ..Addressing::default()
                };
                let (body, expected) = if op == WorkloadOp::TagsPut {
                    (json(pack::tags_body(call.arrival)), UPDATED)
                } else {
                    (None, OK)
                };
                self.send(wire::operation_request(op, &at, body), op, expected)
                    .await
                    .0
            }
            WorkloadOp::CompositionUpdate => {
                let pool = stage
                    .payload
                    .as_ref()
                    .and_then(|k| self.targets.compositions.get(k.as_str()));
                let Some(template) = template else {
                    return Self::defect(op, "the stage's template is not loaded");
                };
                self.update(op, pool, |_| {
                    pack::composition_body(template, call.arrival, call.arrival)
                })
                .await
            }
            WorkloadOp::CompositionDelete => {
                let next = self
                    .targets
                    .deletable
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .pop_front();
                let Some((ehr_id, version)) = next else {
                    return Self::no_target(op);
                };
                let at = Addressing {
                    ehr: Some(&ehr_id),
                    uid: Some(&version),
                    ..Addressing::default()
                };
                self.send(wire::operation_request(op, &at, None), op, DELETED)
                    .await
                    .0
            }
            WorkloadOp::DirectoryCreate => {
                let next = self
                    .targets
                    .without_directory
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .pop();
                let Some(ehr_id) = next else {
                    return Self::no_target(op);
                };
                let at = Addressing {
                    ehr: Some(&ehr_id),
                    ..Addressing::default()
                };
                let body = json(pack::folder_body(false));
                self.send(wire::operation_request(op, &at, body), op, CREATED)
                    .await
                    .0
            }
            WorkloadOp::DirectoryRead => {
                let with: Vec<&crate::perf_emulate::corpus::SeededEhr> = ehrs
                    .iter()
                    .take(POOL_TARGETS)
                    .filter(|e| e.directory)
                    .collect();
                let Some(target) = self.pick(&with, &[call.arrival]) else {
                    return Self::no_target(op);
                };
                let at = Addressing {
                    ehr: Some(&target.ehr_id),
                    ..Addressing::default()
                };
                self.send(wire::operation_request(op, &at, None), op, OK)
                    .await
                    .0
            }
            WorkloadOp::DirectoryUpdate => {
                self.update(op, Some(&self.targets.directories), |_| {
                    Ok(pack::folder_body(true))
                })
                .await
            }
            WorkloadOp::TemplateGet | WorkloadOp::TemplateExample => {
                let Some(template_id) =
                    self.pick(&self.targets.state.listing.templates, &[call.arrival])
                else {
                    return Self::no_target(op);
                };
                let at = Addressing {
                    uid: Some(template_id),
                    ..Addressing::default()
                };
                self.send(wire::operation_request(op, &at, None), op, OK)
                    .await
                    .0
            }
            WorkloadOp::PartyCreate => {
                let Some(person) = stage
                    .payload
                    .as_ref()
                    .and_then(|k| self.payloads.bodies.get(k.as_str()))
                else {
                    return Self::defect(op, "the stage's body is not loaded");
                };
                let body = match pack::person_body(person, call.arrival) {
                    Ok(body) => body,
                    Err(e) => return Self::defect(op, &e),
                };
                self.send(
                    wire::operation_request(op, &Addressing::default(), json(body)),
                    op,
                    CREATED,
                )
                .await
                .0
            }
            WorkloadOp::PartyRead => {
                let Some(uid) = self.pick(&self.targets.state.listing.parties, &[call.arrival])
                else {
                    return Self::no_target(op);
                };
                let at = Addressing {
                    uid: Some(uid),
                    ..Addressing::default()
                };
                self.send(wire::operation_request(op, &at, None), op, OK)
                    .await
                    .0
            }
            WorkloadOp::PartyUpdate => {
                let Some(person) = stage
                    .payload
                    .as_ref()
                    .and_then(|k| self.payloads.bodies.get(k.as_str()))
                else {
                    return Self::defect(op, "the stage's body is not loaded");
                };
                self.update(op, Some(&self.targets.parties), |_| {
                    pack::person_body(person, call.arrival)
                })
                .await
            }
            WorkloadOp::Aql => self.query(call, stage, ehr).await,
        }
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

impl Performer for WirePerformer {
    async fn perform(&self, call: &Call) -> Outcome {
        self.realize(call).await
    }
}

#[cfg(test)]
mod tests {
    use super::next_version;

    #[test]
    fn the_next_version_raises_the_trunk_number() {
        assert_eq!(
            next_version("8849182c::cdr.example::1"),
            "8849182c::cdr.example::2"
        );
        assert_eq!(
            next_version("8849182c::cdr.example::9"),
            "8849182c::cdr.example::10"
        );
        assert_eq!(next_version("opaque"), "opaque");
    }
}
