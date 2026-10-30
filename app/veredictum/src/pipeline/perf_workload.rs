// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The measured run of a workload document against a live server.
//!
//! It loads the workload through its gates, reads the server's System
//! OPTIONS identity, seeds or reuses the corpus, drives the run after
//! seeding, and writes one `perf-result`. The pieces are shared with the
//! interleaved comparison of `perf-compare`, which drives two targets with
//! the same workload, corpus recipe and seed.
//!
//! The posture canaries bracket every run: one bracket after seeding and
//! one after the last plateau, each checking the declared profile against
//! the server black-box. A reading that contradicts the declaration, or two
//! readings that disagree, refuse the record.
//!
//! A server that answers any exchange of the run `429 Too Many Requests`
//! gets no record: the run would have measured its rate limiter. The
//! refusal reads the run's own record, never a process-wide flag, so two
//! runs in one process never refuse each other's.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use crate::ixit::{Containers, Environment};
use crate::perf_emulate::baselines::ReferencePin;
use crate::perf_emulate::binding::{Binding, bind};
use crate::perf_emulate::canary::CanaryClient;
use crate::perf_emulate::corpus::{
    self, CorpusError, CorpusState, Needs, Payloads, SeedReport, SeedRequest,
};
use crate::perf_emulate::fingerprint::EnvironmentFingerprint;
use crate::perf_emulate::http::HttpClient;
use crate::perf_emulate::performer::{Targets, WirePerformer, read_current};
use crate::perf_emulate::posture::{
    AuthnMode, Bracket, CanaryReading, CanaryTarget, InvalidTwin, PostureDivergence, PostureItem,
    PostureProfile, PostureRecord, TlsMode, VersionSample, bracket, settle, submission_defects,
    tls_of,
};
use crate::perf_emulate::run::{LadderOutcome, RunHooks, RunRequest, RunShape, run_after_seeding};
use crate::perf_emulate::server::{DockerPsql, NO_CONTAINERS, NO_DATABASE, ServerSampler};
use crate::perf_gen::arrivals::Rate;
use crate::perf_gen::drive::GeneratorLimits;
use crate::perf_result::host::{HostPrescription, NetworkLeg, load_host_id};
use crate::perf_result::server::{ServerReadings, SettingsReading};
use crate::perf_result::{
    CorpusOrigin, CorpusRecord, EmbeddedWorkload, Generator, Instrument, LoadModel, PerfResult,
    Raw, RawMix, SeedingRecord, Target, TargetOptions,
};
use crate::pipeline::{Error, load_ixit, load_statement};
use crate::workload::Claimed;
use crate::workload::load::{LoadContext, LoadedWorkload, load_file};
use crate::workload::plan::RUN_LIMIT_S;

/// Where the corpus of a run comes from (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorpusMode {
    /// Seed the workload's recipe before the clock.
    Seed,
    /// Reuse the stored corpus whose read-back counts and digest match.
    Reuse,
}

/// What every measured run of one workload shares: the workload, the
/// statement, the run's shape and the host it claims.
#[derive(Debug, Clone)]
pub struct WorkloadSettings<'a> {
    /// The performance root (`perf/`): `payloads.yaml`, `payloads/` and
    /// `hosts/`.
    pub root: &'a Path,
    /// The catalogue root, whose capability folders and option arms a
    /// workload's `requires` resolve to.
    pub catalogue: &'a Path,
    /// The workload document.
    pub workload: &'a Path,
    /// The statement whose claims keep or drop interactions.
    pub statement: Option<&'a Path>,
    /// Seed or reuse.
    pub corpus: CorpusMode,
    /// The closed-loop seeding workers.
    pub seed_workers: usize,
    /// The warm-up and scout start rate, operations per second.
    pub start_rate: f64,
    /// The scout's end rate, operations per second.
    pub scout_end_rate: f64,
    /// The id of the host prescription under `perf/hosts/` the run claims.
    pub host_prescription: Option<String>,
    /// Where the generator sits relative to the server.
    pub network_leg: NetworkLeg,
    /// The posture profile the run declares, which the canaries check.
    pub posture: &'static PostureProfile,
}

/// One measured run of a workload.
#[derive(Debug, Clone)]
pub struct WorkloadRunRequest<'a> {
    /// The workload and the run's settings.
    pub settings: WorkloadSettings<'a>,
    /// The IXIT topology.
    pub ixit: &'a Path,
    /// Where the record is written.
    pub out: &'a Path,
    /// The stored corpus state, written by a seeding and read by a reuse.
    pub corpus_state: PathBuf,
}

/// The readings of one canary bracket, one per posture item.
type Readings = Vec<(PostureItem, CanaryReading)>;

/// The posture a side declares, and every item on which the deployment's
/// own configuration departs from the named profile.
#[derive(Debug, Clone)]
pub(crate) struct Declared {
    pub(crate) profile: PostureProfile,
    pub(crate) divergences: Vec<PostureDivergence>,
}

impl Declared {
    /// A profile declared as the instrument defines it.
    pub(crate) fn plain(profile: &PostureProfile) -> Self {
        Self {
            profile: *profile,
            divergences: Vec::new(),
        }
    }
}

/// The posture canaries of one side: the declared profile, the three client
/// views, the workload's invalid twin and versions the seeding committed.
pub(crate) struct PostureCheck {
    profile: PostureProfile,
    divergences: Vec<PostureDivergence>,
    authn: AuthnMode,
    tls: TlsMode,
    client: CanaryClient,
    raw: CanaryClient,
    anonymous: CanaryClient,
    twin: Option<InvalidTwin>,
    samples: Vec<VersionSample>,
}

/// How many seeded versions the signing canary samples from.
const POSTURE_SAMPLES: usize = 3;

impl PostureCheck {
    /// Builds the canaries of a side whose corpus is ready.
    pub(crate) fn new(
        declared: Declared,
        target: &OpenTarget,
        twin: Option<InvalidTwin>,
        state: &CorpusState,
    ) -> Result<Self, Error> {
        let principal = target.client.principal();
        let authorization = principal.authorization().map_err(instrument)?;
        let client = CanaryClient::new(
            principal.base_url(),
            authorization,
            principal.extra_headers().to_vec(),
        )
        .map_err(instrument)?;
        let samples = state
            .listing
            .ehrs
            .iter()
            .flat_map(|ehr| {
                ehr.compositions.iter().map(|c| VersionSample {
                    ehr_id: ehr.ehr_id.clone(),
                    object_uid: c.object_uid().to_owned(),
                    version_uid: c.version_uid.clone(),
                })
            })
            .take(POSTURE_SAMPLES)
            .collect();
        Ok(Self {
            profile: declared.profile,
            divergences: declared.divergences,
            authn: target.authn,
            tls: tls_of(&target.base_url),
            raw: client.without_decompression().map_err(instrument)?,
            anonymous: client.without_credential().map_err(instrument)?,
            client,
            twin,
            samples,
        })
    }

    /// Takes one bracket of readings; blocking.
    fn take(&self, which: Bracket) -> Readings {
        let target = CanaryTarget {
            client: &self.client,
            raw: &self.raw,
            anonymous: &self.anonymous,
            profile: &self.profile,
            authn: self.authn,
            tls: self.tls,
            invalid_twin: self.twin.clone(),
            samples: &self.samples,
        };
        bracket(&target, which)
    }

    /// Takes one bracket on a blocking thread.
    pub(crate) async fn bracket(self: &Arc<Self>, which: Bracket) -> Result<Readings, Error> {
        let check = Arc::clone(self);
        tokio::task::spawn_blocking(move || check.take(which))
            .await
            .map_err(|e| instrument(format!("posture canaries: {e}")))
    }

    /// Settles the two brackets into the record's posture block.
    pub(crate) fn settle(
        &self,
        before: &Readings,
        after: &Readings,
    ) -> Result<PostureRecord, Error> {
        let mut record = settle(&self.profile, self.authn, self.tls, before, after)
            .map_err(|e| instrument(format!("the run is refused: {e}")))?;
        record.comparability.clone_from(&self.divergences);
        Ok(record)
    }
}

/// The hooks a live run gives the run after seeding.
pub(crate) struct LiveHooks<'a> {
    progress: &'a (dyn Fn(String) + Sync),
    label: &'static str,
    db_container: Option<String>,
    server: ServerSampler<DockerPsql>,
    posture: Arc<PostureCheck>,
    after: Mutex<Option<Result<Readings, Error>>>,
}

impl LiveHooks<'_> {
    fn say(&self, message: &str) {
        (self.progress)(format!("{}{message}", self.label));
    }
}

impl RunHooks for LiveHooks<'_> {
    fn progress(&self, message: &str) {
        self.say(message);
    }

    async fn settle(&self) {
        let Some(db) = self.db_container.clone() else {
            self.say("settle: no containers declared, idle only");
            return;
        };
        let settled = tokio::task::spawn_blocking(move || {
            crate::perf_run::resources::settle_maintenance(&db)
        })
        .await;
        match settled {
            Ok(Ok(())) => self.say("settle: maintenance settled"),
            Ok(Err(e)) => self.say(&format!("settle: maintenance not settled: {e}")),
            Err(e) => self.say(&format!("settle: maintenance not settled: {e}")),
        }
    }

    async fn plateau_began(&self, _index: usize) {
        self.server.begin().await;
    }

    async fn plateau_ended(&self, index: usize) -> ServerReadings {
        let readings = self.server.end().await;
        for (what, reason) in readings.absences() {
            if reason != NO_CONTAINERS && reason != NO_DATABASE {
                self.say(&format!("plateau {index}: no {what} reading: {reason}"));
            }
        }
        readings
    }

    async fn closing(&self) {
        self.say(&format!(
            "closing: posture canaries against the declared profile `{}`",
            self.posture.profile.name
        ));
        let readings = self.posture.bracket(Bracket::After).await;
        *self.after.lock().unwrap_or_else(PoisonError::into_inner) = Some(readings);
    }
}

impl LiveHooks<'_> {
    /// Settles the closing bracket against `before` into the posture block,
    /// refusing the run on a contradiction or a flip.
    pub(crate) fn posture_record(&self, before: &Readings) -> Result<PostureRecord, Error> {
        let after = self
            .after
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or_else(|| instrument("the closing posture bracket never ran"))??;
        let record = self.posture.settle(before, &after)?;
        for defect in submission_defects(&record) {
            self.say(&format!("posture: {defect}"));
        }
        Ok(record)
    }
}

pub(crate) fn instrument(message: impl std::fmt::Display) -> Error {
    Error::Instrument(format!("perf: {message}"))
}

fn corpus_error(e: CorpusError) -> Error {
    instrument(e)
}

/// The workload bound, with everything a side needs that does not depend
/// on the target.
pub(crate) struct Prepared {
    pub(crate) loaded: LoadedWorkload,
    pub(crate) document: String,
    pub(crate) binding: Arc<Binding>,
    pub(crate) payloads: Arc<Payloads>,
    pub(crate) shape: RunShape,
    pub(crate) limits: GeneratorLimits,
    pub(crate) host: Option<HostPrescription>,
    pub(crate) workers: usize,
    pub(crate) twin: Option<InvalidTwin>,
}

/// Loads the workload through its gates, applies the statement's claims,
/// loads the payloads and the claimed host prescription.
pub(crate) fn prepare(
    settings: &WorkloadSettings<'_>,
    progress: &(dyn Fn(String) + Sync),
) -> Result<Prepared, Error> {
    let ctx =
        LoadContext::from_roots(settings.root, settings.catalogue).map_err(Error::Catalogue)?;
    let loaded =
        load_file(settings.workload, &ctx).map_err(|refused| Error::Party(refused.to_string()))?;
    let workload = &loaded.workload;
    progress(format!(
        "workload {} ({}), sha256 {}",
        workload.id,
        loaded.identity.label(),
        loaded.sha256
    ));
    let claimed = match settings.statement {
        Some(path) => Some(Claimed::from_statement(&load_statement(path)?.0)),
        None => None,
    };
    let binding: Binding = bind(workload, claimed.as_ref()).map_err(instrument)?;
    for dropped in &binding.dropped {
        progress(format!(
            "dropped {} ({}): the statement does not claim {}",
            dropped.interaction,
            dropped.share,
            dropped.missing.join(", ")
        ));
    }
    let payloads =
        Arc::new(Payloads::load(workload, &ctx.payloads_dir, &ctx.payloads).map_err(corpus_error)?);
    let host = match &settings.host_prescription {
        Some(id) => Some(load_host_id(settings.root, id).map_err(Error::Catalogue)?),
        None => None,
    };
    let shape = RunShape::standard(
        workload.sample_target.0,
        Rate::new(settings.start_rate).map_err(instrument)?,
        Rate::new(settings.scout_end_rate).map_err(instrument)?,
    );
    let document = std::fs::read_to_string(&loaded.path).map_err(|source| Error::Read {
        path: loaded.path.clone(),
        source,
    })?;
    let twin = payloads.invalid_twin.clone();
    Ok(Prepared {
        twin,
        loaded,
        document,
        binding: Arc::new(binding),
        payloads,
        shape,
        limits: GeneratorLimits::default(),
        host,
        workers: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
    })
}

/// One target: its IXIT, its environment, its client and its identity.
pub(crate) struct OpenTarget {
    pub(crate) authn: AuthnMode,
    pub(crate) containers: Option<Containers>,
    pub(crate) environment: Environment,
    pub(crate) client: HttpClient,
    pub(crate) base_url: String,
    pub(crate) options: TargetOptions,
}

/// Reads the IXIT and the target's System OPTIONS identity before anything
/// is written.
pub(crate) fn open_target(ixit_path: &Path) -> Result<OpenTarget, Error> {
    use crate::identity;
    let (ixit, _text) = load_ixit(ixit_path)?;
    let (principals, environment) =
        crate::perf_run::window::measured_run_context(&ixit).map_err(Error::Instrument)?;
    let instance = ixit.default_instance().map_err(Error::Instrument)?;
    let authorization = identity::authorization(&ixit, &instance.auth)?;
    let options = match crate::exec::transport::HttpTransport::new() {
        Ok(mut transport) => {
            match identity::read(&mut transport, &instance.base_url, authorization.as_deref()) {
                Ok(answered) => TargetOptions::Answered {
                    answer: answered.read,
                },
                Err(e) => TargetOptions::Unanswered {
                    error: e.to_string(),
                },
            }
        }
        Err(e) => TargetOptions::Unanswered { error: e },
    };
    let base_url = instance.base_url.clone();
    let authn = match &instance.auth {
        crate::ixit::AuthMode::None => AuthnMode::None,
        crate::ixit::AuthMode::Basic { .. } => AuthnMode::Basic,
        crate::ixit::AuthMode::Bearer { .. } | crate::ixit::AuthMode::BearerMint { .. } => {
            AuthnMode::Bearer
        }
    };
    let client = HttpClient::new(principals.primary().clone()).map_err(Error::Instrument)?;
    Ok(OpenTarget {
        authn,
        containers: ixit.containers.clone(),
        environment,
        client,
        base_url,
        options,
    })
}

impl OpenTarget {
    /// A target composed on this host from a reference pin: its base URL,
    /// its clinical user's credential, and the environment of the target it
    /// is compared with.
    pub(crate) fn baseline(pin: &ReferencePin, environment: Environment) -> Result<Self, Error> {
        let base_url = pin.base_url();
        let authorization = pin.authorization();
        let options = match crate::exec::transport::HttpTransport::new() {
            Ok(mut transport) => {
                match crate::identity::read(&mut transport, &base_url, Some(&authorization)) {
                    Ok(answered) => TargetOptions::Answered {
                        answer: answered.read,
                    },
                    Err(e) => TargetOptions::Unanswered {
                        error: e.to_string(),
                    },
                }
            }
            Err(e) => TargetOptions::Unanswered { error: e },
        };
        let principal = crate::perf_run::client::PerfClient::fixed(&base_url, Some(authorization))
            .map_err(Error::Instrument)?;
        Ok(Self {
            authn: AuthnMode::Basic,
            containers: None,
            environment,
            client: HttpClient::new(principal).map_err(Error::Instrument)?,
            base_url,
            options,
        })
    }
}

/// A target with its corpus ready and its performer built.
pub(crate) struct ReadyTarget {
    pub(crate) state: CorpusState,
    pub(crate) seeding: SeedReport,
    pub(crate) origin: CorpusOrigin,
    pub(crate) performer: Arc<WirePerformer>,
}

/// Seeds or reuses the target's corpus and builds its performer.
pub(crate) async fn ready_target(
    prepared: &Prepared,
    target: &OpenTarget,
    settings: &WorkloadSettings<'_>,
    corpus_state: &Path,
    progress: &(dyn Fn(String) + Sync),
) -> Result<ReadyTarget, Error> {
    let workload = &prepared.loaded.workload;
    let client = &target.client;
    let (state, report, origin) = match settings.corpus {
        CorpusMode::Seed => {
            progress(format!(
                "seeding {} (closed-loop, {} workers): {} EHRs x {} compositions",
                target.base_url,
                settings.seed_workers,
                workload.corpus.scale.ehrs(),
                workload.corpus.compositions_per_ehr
            ));
            let seed = SeedRequest {
                recipe: &workload.corpus,
                ehrs: workload.corpus.scale.ehrs(),
                workers: settings.seed_workers,
                seed: workload.seed,
                token: format!("perf-{}", uuid::Uuid::new_v4().simple()),
                needs: Needs::of(&prepared.binding.operations()),
            };
            let (state, report) =
                corpus::seed(client, Arc::clone(&prepared.payloads), workload, &seed)
                    .await
                    .map_err(corpus_error)?;
            state.write(corpus_state).map_err(corpus_error)?;
            (state, report, CorpusOrigin::Seeded)
        }
        CorpusMode::Reuse => {
            let state = CorpusState::read(corpus_state).map_err(corpus_error)?;
            let report = corpus::reuse(client, workload, &state)
                .await
                .map_err(corpus_error)?;
            (state, report, CorpusOrigin::Reused)
        }
    };
    progress(format!(
        "corpus {origin:?}: {} EHRs, {} compositions read back; {} requests in {} ms",
        state.ehrs_read_back, state.compositions_read_back, report.requests, report.wall_ms
    ));
    let mut requests = report.requests;
    let current = read_current(client, &state, &prepared.binding, &mut requests)
        .await
        .map_err(instrument)?;
    let seeding = SeedReport {
        requests,
        wall_ms: report.wall_ms,
    };
    let targets = Targets::new(state.clone(), &current);
    let performer = Arc::new(WirePerformer::new(
        client.clone(),
        Arc::clone(&prepared.binding),
        workload,
        Arc::clone(&prepared.payloads),
        targets,
    ));
    Ok(ReadyTarget {
        state,
        seeding,
        origin,
        performer,
    })
}

/// The hooks of one live side, with the database settings read once.
pub(crate) async fn live_hooks<'p>(
    target: &OpenTarget,
    posture: Arc<PostureCheck>,
    label: &'static str,
    progress: &'p (dyn Fn(String) + Sync),
) -> (LiveHooks<'p>, SettingsReading) {
    let hooks = LiveHooks {
        progress,
        label,
        db_container: target.containers.as_ref().map(|c| c.db.clone()),
        server: ServerSampler::from_ixit(target.containers.as_ref()),
        posture,
        after: Mutex::new(None),
    };
    let settings = hooks.server.settings().await;
    if let SettingsReading::Absent { reason } = &settings {
        hooks.say(&format!(
            "server-side readings: no database settings, sizes or statements ({reason})"
        ));
    }
    if target.containers.is_none() {
        hooks.say(&format!(
            "server-side readings: no container series ({NO_CONTAINERS})"
        ));
    }
    (hooks, settings)
}

/// Reads the corpus back after the run and stores the counts, so a later
/// reuse is checked against what the server holds now.
pub(crate) async fn store_read_back(
    target: &OpenTarget,
    state: &CorpusState,
    corpus_state: &Path,
    progress: &(dyn Fn(String) + Sync),
) -> Result<(), Error> {
    let mut after = state.clone();
    let mut ignored = 0;
    match corpus::read_back(&target.client, &mut ignored).await {
        Ok((ehrs, compositions)) => {
            after.ehrs_read_back = ehrs;
            after.compositions_read_back = compositions;
            after.write(corpus_state).map_err(corpus_error)?;
        }
        Err(e) => progress(format!(
            "the read-back after the run failed ({e}); the stored corpus keeps its counts \
             and a reuse will be refused"
        )),
    }
    Ok(())
}

/// Builds one side's raw record from what the run produced.
pub(crate) fn raw_record(
    prepared: &Prepared,
    settings: &WorkloadSettings<'_>,
    target: &OpenTarget,
    ready: &ReadyTarget,
    server_settings: SettingsReading,
    outcome: LadderOutcome,
    posture: PostureRecord,
) -> Raw {
    let workload = &prepared.loaded.workload;
    let state = &ready.state;
    Raw {
        instrument: Instrument::this_build(),
        target: Target {
            base_url: target.base_url.clone(),
            options: target.options.clone(),
        },
        environment: target.environment.clone(),
        generator: Generator {
            host: EnvironmentFingerprint::detect(),
            in_flight_budget: prepared.limits.in_flight_budget,
            lag_limit_us: u64::try_from(prepared.limits.lag_limit.as_micros()).unwrap_or(u64::MAX),
            workers: u32::try_from(prepared.workers).unwrap_or(u32::MAX),
        },
        host_prescription: prepared.host.clone(),
        network_leg: Some(settings.network_leg),
        interleaving: None,
        aa_floor: None,
        posture: Some(posture),
        workload: EmbeddedWorkload {
            id: workload.id.to_string(),
            label: prepared.loaded.identity.label(),
            version: workload.version.to_string(),
            sha256: format!("sha256:{}", prepared.loaded.sha256),
            document: prepared.document.clone(),
        },
        corpus: CorpusRecord {
            scale: workload.corpus.scale,
            recipe_sha256: state.recipe_sha256.clone(),
            digest: state.digest.clone(),
            origin: ready.origin,
            ehrs: state.ehrs_read_back,
            compositions: state.compositions_read_back,
        },
        seeding: SeedingRecord {
            model: LoadModel::ClosedLoop,
            workers: u32::try_from(settings.seed_workers.max(1)).unwrap_or(u32::MAX),
            requests: ready.seeding.requests,
            wall_ms: ready.seeding.wall_ms,
        },
        mix: RawMix {
            operations_per_interaction: prepared.binding.operations_per_interaction,
            dropped: prepared.binding.dropped.clone(),
        },
        qualification: prepared.binding.qualification.clone(),
        seed: workload.seed,
        server_settings,
        phases: outcome.phases,
        scout: Some(outcome.scout),
        plateaus: outcome.plateaus,
    }
}

/// Builds the multi-threaded runtime a live run drives on.
pub(crate) fn runtime(workers: usize) -> Result<tokio::runtime::Runtime, Error> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
        .map_err(|e| instrument(format!("runtime: {e}")))
}

/// Writes `record` to `out` in its canonical form.
pub(crate) fn write_record(record: &PerfResult, out: &Path) -> Result<(), Error> {
    let text = record.render().map_err(instrument)?;
    std::fs::write(out, text).map_err(|source| Error::Write {
        path: out.to_owned(),
        source,
    })
}

/// Runs one workload against the IXIT's server and writes its record.
///
/// # Errors
/// [`Error::Party`] when the workload or the statement is refused,
/// [`Error::Catalogue`] when the host prescription is refused,
/// [`Error::Read`] or [`Error::Parse`] for the IXIT, and
/// [`Error::Instrument`] when the corpus cannot be seeded or reused, the
/// run fails, the server rate-limited the run, or the record cannot be
/// written.
pub fn run_workload(
    request: &WorkloadRunRequest<'_>,
    progress: &(dyn Fn(String) + Sync),
) -> Result<PerfResult, Error> {
    let settings = &request.settings;
    let prepared = prepare(settings, progress)?;
    let target = open_target(request.ixit)?;
    let runtime = runtime(prepared.workers)?;
    let (ready, outcome, server_settings, posture) = runtime.block_on(async {
        let ready = ready_target(
            &prepared,
            &target,
            settings,
            &request.corpus_state,
            progress,
        )
        .await?;
        progress(format!(
            "planned run after seeding: {} to {} s (at most {RUN_LIMIT_S} s); the first measured \
             request follows the settle",
            prepared.loaded.plan.min_s, prepared.loaded.plan.max_s
        ));
        let check = Arc::new(PostureCheck::new(
            Declared::plain(settings.posture),
            &target,
            prepared.twin.clone(),
            &ready.state,
        )?);
        progress(format!(
            "posture canaries against the declared profile `{}`",
            settings.posture.name
        ));
        let before = check.bracket(Bracket::Before).await?;
        let (hooks, server_settings) = live_hooks(&target, check, "", progress).await;
        let run = RunRequest {
            seed: prepared.loaded.workload.seed,
            process: prepared.loaded.workload.arrival,
            binding: &prepared.binding,
            limits: prepared.limits.clone(),
            shape: prepared.shape.clone(),
        };
        let outcome = run_after_seeding(&run, Arc::clone(&ready.performer), &hooks)
            .await
            .map_err(instrument)?;
        let posture = hooks.posture_record(&before)?;
        store_read_back(&target, &ready.state, &request.corpus_state, progress).await?;
        Ok::<_, Error>((ready, outcome, server_settings, posture))
    })?;
    let raw = raw_record(
        &prepared,
        settings,
        &target,
        &ready,
        server_settings,
        outcome,
        posture,
    );
    refuse_rate_limited(&raw)?;
    let record = PerfResult::from_raw(raw).map_err(instrument)?;
    write_record(&record, request.out)?;
    Ok(record)
}

/// Refuses a record in which the server answered any exchange `429`.
///
/// # Errors
/// [`Error::Instrument`] naming the count and what clears it.
pub fn refuse_rate_limited(raw: &Raw) -> Result<(), Error> {
    let limited = raw.rate_limited();
    if limited == 0 {
        return Ok(());
    }
    Err(Error::Instrument(format!(
        "{} ({limited} exchange(s) answered 429)",
        crate::perf_run::rate_limited_refusal("perf")
    )))
}
