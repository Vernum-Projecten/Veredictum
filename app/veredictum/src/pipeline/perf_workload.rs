// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The measured run of a workload document against a live server.
//!
//! It loads the workload through its gates, reads the server's System
//! OPTIONS identity, seeds or reuses the corpus, drives the run after
//! seeding, and writes one `perf-result`.
//!
//! A server that answers any exchange of the run `429 Too Many Requests`
//! gets no record: the run would have measured its rate limiter. The
//! refusal reads the run's own record, never a process-wide flag, so two
//! runs in one process never refuse each other's.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::bench::fingerprint::EnvironmentFingerprint;
use crate::perf_emulate::binding::{Binding, bind};
use crate::perf_emulate::corpus::{
    self, CorpusError, CorpusState, Needs, Payloads, SeedReport, SeedRequest,
};
use crate::perf_emulate::http::HttpClient;
use crate::perf_emulate::performer::{Targets, WirePerformer, read_current};
use crate::perf_emulate::run::{RunHooks, RunRequest, RunShape, run_after_seeding};
use crate::perf_emulate::server::{DockerPsql, NO_CONTAINERS, NO_DATABASE, ServerSampler};
use crate::perf_gen::arrivals::Rate;
use crate::perf_gen::drive::GeneratorLimits;
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

/// One measured run of a workload.
#[derive(Debug, Clone)]
pub struct WorkloadRunRequest<'a> {
    /// The performance root (`perf/`): `payloads.yaml` and `payloads/`.
    pub root: &'a Path,
    /// The catalogue root, whose capability folders and option arms a
    /// workload's `requires` resolve to.
    pub catalogue: &'a Path,
    /// The workload document.
    pub workload: &'a Path,
    /// The IXIT topology.
    pub ixit: &'a Path,
    /// The statement whose claims keep or drop interactions.
    pub statement: Option<&'a Path>,
    /// Where the record is written.
    pub out: &'a Path,
    /// Seed or reuse.
    pub corpus: CorpusMode,
    /// The stored corpus state, written by a seeding and read by a reuse.
    pub corpus_state: PathBuf,
    /// The closed-loop seeding workers.
    pub seed_workers: usize,
    /// The warm-up and scout start rate, operations per second.
    pub start_rate: f64,
    /// The scout's end rate, operations per second.
    pub scout_end_rate: f64,
    /// The host prescription the run claims.
    pub host_prescription: Option<String>,
}

/// The hooks a live run gives the run after seeding.
struct LiveHooks<'a> {
    progress: &'a (dyn Fn(String) + Sync),
    client: HttpClient,
    db_container: Option<String>,
    server: ServerSampler<DockerPsql>,
}

impl RunHooks for LiveHooks<'_> {
    fn progress(&self, message: &str) {
        (self.progress)(message.to_owned());
    }

    async fn settle(&self) {
        let Some(db) = self.db_container.clone() else {
            (self.progress)("settle: no containers declared, idle only".to_owned());
            return;
        };
        let settled = tokio::task::spawn_blocking(move || {
            crate::perf_run::resources::settle_maintenance(&db)
        })
        .await;
        match settled {
            Ok(Ok(())) => (self.progress)("settle: maintenance settled".to_owned()),
            Ok(Err(e)) => (self.progress)(format!("settle: maintenance not settled: {e}")),
            Err(e) => (self.progress)(format!("settle: maintenance not settled: {e}")),
        }
    }

    async fn plateau_began(&self, _index: usize) {
        self.server.begin().await;
    }

    async fn plateau_ended(&self, index: usize) -> ServerReadings {
        let readings = self.server.end().await;
        for (what, reason) in readings.absences() {
            if reason != NO_CONTAINERS && reason != NO_DATABASE {
                (self.progress)(format!("plateau {index}: no {what} reading: {reason}"));
            }
        }
        readings
    }

    async fn closing(&self) {
        // TODO(#891): the posture canaries bracket the run here.
        let request = crate::workload::wire::operation_request(
            crate::workload::ops::WorkloadOp::SystemOptions,
            &crate::workload::wire::Addressing::default(),
            None,
        );
        let line = match request {
            Ok(request) => match self.client.send(&request).await {
                Ok(reply) => format!("closing: System OPTIONS answered {}", reply.status.as_u16()),
                Err(e) => format!("closing: System OPTIONS failed: {e}"),
            },
            Err(e) => format!("closing: {e}"),
        };
        (self.progress)(line);
    }
}

fn instrument(message: impl std::fmt::Display) -> Error {
    Error::Instrument(format!("perf: {message}"))
}

fn corpus_error(e: CorpusError) -> Error {
    instrument(e)
}

/// The workload, loaded through its gates.
fn load_workload(request: &WorkloadRunRequest<'_>) -> Result<LoadedWorkload, Error> {
    let ctx = LoadContext::from_roots(request.root, request.catalogue).map_err(Error::Catalogue)?;
    load_file(request.workload, &ctx).map_err(|refused| Error::Party(refused.to_string()))
}

/// Reads the System OPTIONS identity before anything is written.
fn read_identity(ixit: &crate::ixit::Ixit) -> Result<(String, TargetOptions), Error> {
    use crate::identity;
    let instance = ixit.default_instance().map_err(Error::Instrument)?;
    let authorization = identity::authorization(ixit, &instance.auth)?;
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
    Ok((instance.base_url.clone(), options))
}

/// Runs one workload against the IXIT's server and writes its record.
///
/// # Errors
/// [`Error::Party`] when the workload or the statement is refused,
/// [`Error::Read`] or [`Error::Parse`] for the IXIT, and
/// [`Error::Instrument`] when the corpus cannot be seeded or reused, the
/// run fails, the server rate-limited the run, or the record cannot be
/// written.
#[expect(
    clippy::too_many_lines,
    reason = "the run is one sequence: load, identify, seed, drive, record"
)]
pub fn run_workload(
    request: &WorkloadRunRequest<'_>,
    progress: &(dyn Fn(String) + Sync),
) -> Result<PerfResult, Error> {
    let loaded = load_workload(request)?;
    let workload = &loaded.workload;
    progress(format!(
        "workload {} ({}), sha256 {}",
        workload.id,
        loaded.identity.label(),
        loaded.sha256
    ));
    let claimed = match request.statement {
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
    let (ixit, _text) = load_ixit(request.ixit)?;
    let (principals, environment) =
        crate::perf_run::window::measured_run_context(&ixit).map_err(Error::Instrument)?;
    let (base_url, options) = read_identity(&ixit)?;
    let ctx = LoadContext::from_roots(request.root, request.catalogue).map_err(Error::Catalogue)?;
    let payloads =
        Arc::new(Payloads::load(workload, &ctx.payloads_dir, &ctx.payloads).map_err(corpus_error)?);
    let client = HttpClient::new(principals.primary().clone()).map_err(Error::Instrument)?;
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
        .map_err(|e| instrument(format!("runtime: {e}")))?;
    let binding = Arc::new(binding);
    let shape = RunShape::standard(
        workload.sample_target.0,
        Rate::new(request.start_rate).map_err(instrument)?,
        Rate::new(request.scout_end_rate).map_err(instrument)?,
    );
    let limits = GeneratorLimits::default();

    let (state, seeding, origin, outcome, server_settings) = runtime.block_on(async {
        let (state, report, origin) = match request.corpus {
            CorpusMode::Seed => {
                progress(format!(
                    "seeding (closed-loop, {} workers): {} EHRs x {} compositions",
                    request.seed_workers,
                    workload.corpus.scale.ehrs(),
                    workload.corpus.compositions_per_ehr
                ));
                let seed = SeedRequest {
                    recipe: &workload.corpus,
                    ehrs: workload.corpus.scale.ehrs(),
                    workers: request.seed_workers,
                    seed: workload.seed,
                    token: format!("perf-{}", uuid::Uuid::new_v4().simple()),
                    needs: Needs::of(&binding.operations()),
                };
                let (state, report) = corpus::seed(&client, Arc::clone(&payloads), workload, &seed)
                    .await
                    .map_err(corpus_error)?;
                state.write(&request.corpus_state).map_err(corpus_error)?;
                (state, report, CorpusOrigin::Seeded)
            }
            CorpusMode::Reuse => {
                let state = CorpusState::read(&request.corpus_state).map_err(corpus_error)?;
                let report = corpus::reuse(&client, workload, &state)
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
        let current = read_current(&client, &state, &binding, &mut requests)
            .await
            .map_err(instrument)?;
        let seeding = SeedReport {
            requests,
            wall_ms: report.wall_ms,
        };
        let targets = Targets::new(state.clone(), &current);
        let performer = Arc::new(WirePerformer::new(
            client.clone(),
            Arc::clone(&binding),
            workload,
            Arc::clone(&payloads),
            targets,
        ));
        progress(format!(
            "planned run after seeding: {} to {} s (at most {RUN_LIMIT_S} s); the first measured \
             request follows the settle",
            loaded.plan.min_s, loaded.plan.max_s
        ));
        let hooks = LiveHooks {
            progress,
            client: client.clone(),
            db_container: ixit.containers.as_ref().map(|c| c.db.clone()),
            server: ServerSampler::from_ixit(ixit.containers.as_ref()),
        };
        let settings = hooks.server.settings().await;
        if let SettingsReading::Absent { reason } = &settings {
            progress(format!(
                "server-side readings: no database settings, sizes or statements ({reason})"
            ));
        }
        if ixit.containers.is_none() {
            progress(format!(
                "server-side readings: no container series ({NO_CONTAINERS})"
            ));
        }
        let run = RunRequest {
            seed: workload.seed,
            process: workload.arrival,
            binding: &binding,
            limits: limits.clone(),
            shape: shape.clone(),
        };
        let outcome = run_after_seeding(&run, Arc::clone(&performer), &hooks)
            .await
            .map_err(instrument)?;
        let mut after = state.clone();
        let mut ignored = 0;
        match corpus::read_back(&client, &mut ignored).await {
            Ok((ehrs, compositions)) => {
                after.ehrs_read_back = ehrs;
                after.compositions_read_back = compositions;
                after.write(&request.corpus_state).map_err(corpus_error)?;
            }
            Err(e) => progress(format!(
                "the read-back after the run failed ({e}); the stored corpus keeps its counts \
                 and a reuse will be refused"
            )),
        }
        Ok::<_, Error>((state, seeding, origin, outcome, settings))
    })?;

    let document = std::fs::read_to_string(&loaded.path).map_err(|source| Error::Read {
        path: loaded.path.clone(),
        source,
    })?;
    let raw = Raw {
        instrument: Instrument::this_build(),
        target: Target { base_url, options },
        environment,
        generator: Generator {
            host: EnvironmentFingerprint::detect(),
            in_flight_budget: limits.in_flight_budget,
            lag_limit_us: u64::try_from(limits.lag_limit.as_micros()).unwrap_or(u64::MAX),
            workers: u32::try_from(workers).unwrap_or(u32::MAX),
        },
        host_prescription: request.host_prescription.clone(),
        posture: None,
        workload: EmbeddedWorkload {
            id: workload.id.to_string(),
            label: loaded.identity.label(),
            version: workload.version.to_string(),
            sha256: format!("sha256:{}", loaded.sha256),
            document,
        },
        corpus: CorpusRecord {
            scale: workload.corpus.scale,
            recipe_sha256: state.recipe_sha256.clone(),
            digest: state.digest.clone(),
            origin,
            ehrs: state.ehrs_read_back,
            compositions: state.compositions_read_back,
        },
        seeding: SeedingRecord {
            model: LoadModel::ClosedLoop,
            workers: u32::try_from(request.seed_workers.max(1)).unwrap_or(u32::MAX),
            requests: seeding.requests,
            wall_ms: seeding.wall_ms,
        },
        mix: RawMix {
            operations_per_interaction: binding.operations_per_interaction,
            dropped: binding.dropped.clone(),
        },
        qualification: binding.qualification.clone(),
        seed: workload.seed,
        server_settings,
        phases: outcome.phases,
        scout: Some(outcome.scout),
        plateaus: outcome.plateaus,
    };
    refuse_rate_limited(&raw)?;
    let record = PerfResult::from_raw(raw).map_err(instrument)?;
    let text = record.render().map_err(instrument)?;
    std::fs::write(request.out, text).map_err(|source| Error::Write {
        path: request.out.to_owned(),
        source,
    })?;
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
