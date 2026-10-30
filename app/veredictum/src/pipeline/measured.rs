// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The two instruments that drive load at a live system under test: the
//! measured class run and the step-load stress ladder.
//!
//! Both share one preamble — load the performance schedule
//! ([`crate::perf_schedule`]), read the topology, select the class's case,
//! seed a fresh corpus — and differ only in what
//! they do with the seeded system afterwards. Only the measured run produces
//! a conformance record; the stress ladder is an exploration instrument and says
//! so in its own report.
//!
//! Each seam reports its own progress through an observer rather than
//! writing to a console, so a caller renders the run as it happens.

use std::path::Path;

use crate::identity::OperatorIdentity;
use crate::ids::CaseId;
use crate::ixit::Ixit;
use crate::party::{
    InstrumentStatus, RecordedTechProfile, Results, Runner, SCHEDULE_RELEASE, SelectionBasis, Sut,
    TechProfileSource, VerificationPackStatus, ixit_digest,
};
use crate::perf::{JourneyCatalogue, Measurement, PerfClass, PerformanceCase};
use crate::perf_run::client::PerfClient;
use crate::perf_run::corpus::SeededCorpus;
use crate::perf_run::pack::{self, JourneyPack};
use crate::perf_schedule::{JOURNEYS_FILE, PAYLOADS_FILE, PerfSchedule};
use crate::pipeline::{Error, load_ixit, load_party_json, to_json_document};
use crate::schema::results_schema;
use crate::stress::StressReport;
use crate::vocab::{FormatName, ItsName};

/// The sustained-window ladder a measured run may hold its offered load for.
///
/// A longer window is a stricter demonstration of the same class and
/// persists like any measured run; nothing shorter than the case's own
/// normative window exists.
#[derive(Debug, Clone, Copy)]
pub struct SustainedWindow(u64);

impl SustainedWindow {
    /// Every window on the ladder, in hours.
    pub const LADDER: &[u64] = &[1, 2, 4, 6, 8, 12];

    /// Returns the window of `hours`, or `None` when that is not a rung of
    /// the ladder.
    #[must_use]
    pub fn hours(hours: u64) -> Option<Self> {
        Self::LADDER.contains(&hours).then_some(Self(hours))
    }

    /// Returns the window in seconds.
    #[must_use]
    pub fn seconds(self) -> u64 {
        self.0.saturating_mul(3600)
    }
}

impl Default for SustainedWindow {
    fn default() -> Self {
        Self(1)
    }
}

/// The seeding milestones the disk anchors are probed at.
#[derive(Debug, Clone, Copy)]
pub enum SeedStage {
    /// Before anything is written, the empty baseline.
    BeforeScale,
    /// After the scale ladder is seeded.
    AfterScale,
    /// After the standing ward is seeded on top of it.
    AfterWard,
}

/// What a measured run reports as it happens.
#[derive(Debug)]
pub enum MeasuredEvent<'a> {
    /// A progress message from the seeding or the window itself.
    Progress(String),
    /// A case of the selected class is starting.
    CaseStarted {
        /// The case being measured.
        case: &'a PerformanceCase,
        /// The file it was loaded from.
        source: &'a Path,
    },
    /// The window closed and produced this record.
    Measured(&'a Measurement),
    /// A record for a case the performance schedule no longer carries was
    /// dropped from the results.
    PrunedOrphan(&'a CaseId),
    /// The record was merged into the results document at this path.
    Merged(&'a Path),
    /// No results document existed at this path, so the record was written
    /// into a new one.
    Created(&'a Path),
}

/// One completed measured run.
#[derive(Debug)]
pub struct MeasuredRun {
    /// The record produced per case of the selected class.
    pub measurements: Vec<Measurement>,
    /// Whether every case of the class earned its verdict.
    pub earned_all: bool,
}

/// Which class to measure, against which topology, into which results
/// record.
#[derive(Debug)]
pub struct MeasuredRequest<'a> {
    /// The performance schedule root ([`crate::perf_schedule`]).
    pub root: &'a Path,
    /// The ixit topology document; its environment block is mandatory for a
    /// measured run.
    pub ixit: &'a Path,
    /// The results document the measurement records are merged into. When no
    /// file exists at this path, the run creates a new measured record there
    /// ([`new_measured_record`]).
    pub results: &'a Path,
    /// The SUT name and version a new measured record carries where the
    /// server's System OPTIONS answer states none. Unread when the results
    /// document already exists.
    pub sut: OperatorIdentity<'a>,
    /// The class token selecting the performance case(s) to measure.
    pub class: &'a str,
    /// Parallel seeding workers.
    pub seed_workers: usize,
    /// How long to hold the offered load.
    pub window: SustainedWindow,
}

/// Which class-scale corpus to stress, and how hard to climb.
#[derive(Debug)]
pub struct StressRequest<'a> {
    /// The performance schedule root ([`crate::perf_schedule`]).
    pub root: &'a Path,
    /// The ixit topology document; its environment block is mandatory,
    /// because a throughput number without the deployment described is
    /// meaningless.
    pub ixit: &'a Path,
    /// The class token selecting the corpus scale and workload mix. No
    /// class floor enters a stress report.
    pub corpus_class: &'a str,
    /// Parallel seeding workers.
    pub seed_workers: usize,
    /// Each load step's recorded hold, in seconds.
    pub step_secs: u64,
    /// Post-breach bisection refinements.
    pub bisections: u32,
    /// The climb cap, in arrivals per second.
    pub max_rate: f64,
}

/// Loads the performance schedule under `root`, refusing a tree with any
/// defect.
///
/// # Errors
/// [`Error::Catalogue`] on a schema-compilation defect in this crate, or
/// [`Error::Artifacts`] carrying one diagnostic per defect in the tree.
pub fn load_clean_schedule(root: &Path) -> Result<PerfSchedule, Error> {
    let loaded = crate::perf_schedule::load(root).map_err(Error::Catalogue)?;
    if loaded.errors.is_empty() {
        Ok(loaded.schedule)
    } else {
        Err(Error::Artifacts(loaded.errors))
    }
}

/// Returns the performance case of `class`, with the file it came from.
///
/// # Errors
/// [`Error::Missing`] when the schedule carries no case of that class.
pub fn performance_case_of_class<'a>(
    schedule: &'a PerfSchedule,
    class: PerfClass,
    token: &str,
) -> Result<(&'a Path, &'a PerformanceCase), Error> {
    schedule.cases_of_class(class).next().ok_or_else(|| {
        Error::Missing(format!(
            "no performance case of class {token} in the performance schedule"
        ))
    })
}

/// Returns the blood-pressure OPT the scale corpora commit against.
///
/// # Errors
/// [`Error::Missing`] when the schedule carries no payload manifest or no
/// entry for the OPT, [`Error::Instrument`] when its file cannot be read.
pub fn scale_opt_xml(schedule: &PerfSchedule) -> Result<String, Error> {
    let (_, manifest) = schedule.payloads.as_ref().ok_or_else(|| {
        Error::Missing(format!(
            "the performance schedule carries no {PAYLOADS_FILE}"
        ))
    })?;
    let entry = manifest.get(pack::SCALE_OPT_KEY).ok_or_else(|| {
        Error::Missing(format!(
            "{PAYLOADS_FILE} has no {} entry",
            pack::SCALE_OPT_KEY
        ))
    })?;
    let path = schedule.payloads_dir().join(&entry.source);
    std::fs::read_to_string(&path)
        .map_err(|e| Error::Instrument(format!("cannot read OPT {}: {e}", path.display())))
}

/// Returns the journey context every measured run needs: the catalogue the
/// workload decomposes into, and the template pack its stages name.
///
/// # Errors
/// [`Error::Missing`] when the schedule carries no journey catalogue or no
/// payload manifest, [`Error::Instrument`] when the pack itself will not
/// load.
pub fn journey_context(schedule: &PerfSchedule) -> Result<(JourneyCatalogue, JourneyPack), Error> {
    let catalogue = schedule
        .journeys
        .as_ref()
        .map(|(_, catalogue)| catalogue.clone())
        .ok_or_else(|| {
            Error::Missing(format!(
                "the performance schedule carries no {JOURNEYS_FILE}"
            ))
        })?;
    let (_, manifest) = schedule.payloads.as_ref().ok_or_else(|| {
        Error::Missing(format!(
            "the performance schedule carries no {PAYLOADS_FILE}"
        ))
    })?;
    let pack = JourneyPack::load(&schedule.payloads_dir(), manifest, &catalogue)
        .map_err(Error::Instrument)?;
    Ok((catalogue, pack))
}

/// Seeds the scale corpus and the standing ward on a freshly composed,
/// empty SUT.
///
/// The workflow always seeds a fresh system and tears the stack down
/// afterwards, so there is no seed reuse. `stage` observes the seeding
/// milestones, which is where the disk anchors are probed.
///
/// # Errors
/// [`Error::Instrument`] naming the stage that failed.
pub fn seed_corpus(
    client: &PerfClient,
    corpus_key: &str,
    opt_xml: &str,
    journey_pack: &JourneyPack,
    seed_workers: usize,
    progress: &(dyn Fn(String) + Sync),
    stage: &mut dyn FnMut(SeedStage),
) -> Result<SeededCorpus, Error> {
    use crate::perf_run::corpus;
    let (ehrs, versions) = corpus::scale_shape(corpus_key).map_err(Error::Instrument)?;
    stage(SeedStage::BeforeScale);
    let mut seeded = corpus::seed_scale_ladder(
        client,
        corpus_key,
        opt_xml,
        ehrs,
        versions,
        seed_workers,
        progress,
    )
    .map_err(|e| Error::Instrument(format!("seeding failed: {e}")))?;
    stage(SeedStage::AfterScale);
    corpus::seed_ward(client, &mut seeded, journey_pack, seed_workers, progress)
        .map_err(|e| Error::Instrument(format!("ward seeding failed: {e}")))?;
    stage(SeedStage::AfterWard);
    Ok(seeded)
}

/// Runs the step-load stress ladder to the maximum sustainable throughput.
///
/// This is exploration only: the report it returns is never a conformance
/// record, and it carries no class floor.
///
/// # Errors
/// [`Error::Selector`] for an unknown class token, [`Error::Catalogue`] or
/// [`Error::Artifacts`] when the performance schedule does not load,
/// [`Error::Read`] or [`Error::Parse`] for the topology, and
/// [`Error::Instrument`] for a seeding or run failure — including a window the SUT rate-limited, which
/// would record the limiter's ceiling rather than the server's.
pub fn run_stress(
    request: &StressRequest<'_>,
    progress: &(dyn Fn(String) + Sync),
) -> Result<StressReport, Error> {
    use crate::perf_run;

    let class = PerfClass::parse(request.corpus_class).map_err(Error::Selector)?;
    let schedule = load_clean_schedule(request.root)?;
    let (ixit, _) = load_ixit(request.ixit)?;
    let (principals, environment) =
        perf_run::window::measured_run_context(&ixit).map_err(Error::Instrument)?;
    let client = principals.primary().clone();
    let (_, case) = performance_case_of_class(&schedule, class, request.corpus_class)?;
    let opt_xml = scale_opt_xml(&schedule)?;
    let (catalogue, journey_pack) = journey_context(&schedule)?;
    let corpus = seed_corpus(
        &client,
        case.corpus.as_str(),
        &opt_xml,
        &journey_pack,
        request.seed_workers,
        progress,
        // The stress instrument records no disk anchors (exploration only).
        &mut |_| {},
    )?;
    let options = crate::stress::StressOptions {
        step_hold_s: request.step_secs.max(10),
        bisections: request.bisections,
        max_rate: request.max_rate,
        ..crate::stress::StressOptions::default()
    };
    let workload = perf_run::schedule::JourneyWorkload {
        catalogue: &catalogue,
        shares: &case.workload.journeys,
        pack: &journey_pack,
        // Stress steps are short — the day curve has no meaning there.
        curve: crate::perf::ArrivalCurve::Uniform,
        principals: &principals,
    };
    let report = crate::stress::run_stress(
        &corpus,
        &workload,
        &environment,
        ixit.containers.as_ref(),
        &options,
        progress,
    )
    .map_err(|e| Error::Instrument(format!("stress run failed: {e}")))?;
    perf_run::refuse_rate_limited_record("stress").map_err(Error::Instrument)?;
    Ok(report)
}

/// Returns the results record a measured run creates when none exists yet:
/// the SUT identity and the run's own facts, with no outcome and no
/// measurement.
///
/// Every member is a fact of this run. No party statement selected it, so
/// `selection_basis` is `statement_blind` with no statement digest, and the
/// technology profile is the defaulted one a campaign nothing selected
/// records. The verification pack is `not_run`, the schedule release is the
/// instrument's own [`SCHEDULE_RELEASE`], and the server's
/// `restapi_specs_version` is recorded once, in `sut`, only when it stated
/// one.
#[must_use]
pub fn new_measured_record(sut: Sut, ixit_text: &str) -> Results {
    Results {
        sut,
        runner: Runner {
            name: env!("CARGO_PKG_NAME").to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            verification_pack_status: VerificationPackStatus::NotRun,
        },
        instrument_status: InstrumentStatus::CURRENT,
        schedule_release: SCHEDULE_RELEASE.to_owned(),
        tech_profile: RecordedTechProfile {
            its: ItsName::ItsRest,
            formats: FormatName::ALL.to_vec(),
            source: Some(TechProfileSource::Defaulted),
        },
        ixit_digest: ixit_digest(ixit_text),
        statement_digest: None,
        selection_basis: Some(SelectionBasis::StatementBlind),
        outcomes: Vec::new(),
        measurements: Vec::new(),
    }
}

/// Reads the SUT identity a new measured record carries: what the server
/// states through System OPTIONS, and the operator's values for the rest.
///
/// # Errors
/// [`Error::Instrument`] when the topology declares no `sut` instance, and
/// [`Error::Identity`] when no credential resolves, the request is not
/// answered, or neither side supplies the name or the version.
pub fn measured_identity(
    ixit: &Ixit,
    operator: OperatorIdentity<'_>,
    progress: &dyn Fn(String),
) -> Result<Sut, Error> {
    use crate::identity::{self, IdentityError};

    let instance = ixit.default_instance().map_err(Error::Instrument)?;
    let authorization = identity::authorization(ixit, &instance.auth)?;
    let mut transport =
        crate::exec::transport::HttpTransport::new().map_err(IdentityError::Transport)?;
    let answered = identity::read(&mut transport, &instance.base_url, authorization.as_deref())?;
    progress(format!("identity: {}", answered.read.summary()));
    let (sut, overridden) = identity::merge(answered.read.options(), operator)?;
    for notice in &overridden {
        progress(format!("identity: {}", notice.notice()));
    }
    Ok(sut)
}

/// Runs the measured class window and merges its record into the results
/// document, creating that document when none exists.
///
/// The merge is part of the run rather than a rendering step: a record
/// replaces any prior one for the same case, records for cases the
/// performance schedule no longer carries are pruned, and the set is written
/// back sorted. When no file exists at the results path, the SUT identity is
/// read before any seeding ([`measured_identity`]) and the record lands in a
/// new document ([`new_measured_record`]).
///
/// # Errors
/// [`Error::Selector`] for an unknown class token, [`Error::Catalogue`] or
/// [`Error::Artifacts`] when the performance schedule does not load,
/// [`Error::Read`] or [`Error::Parse`] for the topology, [`Error::Read`] when
/// the results path cannot be inspected, [`Error::Identity`] when a new
/// record's SUT identity cannot be established, [`Error::Party`] for the
/// results document, and [`Error::Instrument`] for a seeding, window or write
/// failure — including a window the SUT rate-limited, which is not a
/// measurement of that server and never reaches the results.
#[expect(
    clippy::too_many_lines,
    reason = "the measured window is one sequence: seed, settle, drive, attach, merge"
)]
pub fn run_measured(
    request: &MeasuredRequest<'_>,
    observe: &(dyn Fn(MeasuredEvent<'_>) + Sync),
) -> Result<MeasuredRun, Error> {
    use crate::perf_run;

    let class = PerfClass::parse(request.class).map_err(Error::Selector)?;
    let schedule = load_clean_schedule(request.root)?;
    let (ixit, ixit_text) = load_ixit(request.ixit)?;
    let (principals, environment) =
        perf_run::window::measured_run_context(&ixit).map_err(Error::Instrument)?;
    let client = principals.primary().clone();
    let selected: Vec<_> = schedule.cases_of_class(class).collect();
    if selected.is_empty() {
        return Err(Error::Missing(format!(
            "no performance case of class {} in the performance schedule",
            request.class
        )));
    }
    let opt_xml = scale_opt_xml(&schedule)?;
    let (catalogue, journey_pack) = journey_context(&schedule)?;
    let progress = |message: String| observe(MeasuredEvent::Progress(message));
    // Resource sampling is optional by capability: no ixit `containers`
    // block → no `resources` record, never a failed run.
    let containers = ixit.containers.clone();
    if containers.is_none() {
        progress("resources: not sampled (ixit declares no `containers` block)".to_owned());
    }
    // The identity is read before the seeding, so a record that cannot carry
    // one fails before an hour of offered load rather than after it.
    let new_record = if record_exists(request.results)? {
        None
    } else {
        let sut = measured_identity(&ixit, request.sut, &progress)?;
        Some(new_measured_record(sut, &ixit_text))
    };

    let mut run = MeasuredRun {
        measurements: Vec::new(),
        earned_all: true,
    };
    for (path, case) in selected {
        observe(MeasuredEvent::CaseStarted { case, source: path });
        // The disk anchors bracket the seeding milestones; every probe
        // failure degrades to an absent anchor with the reason logged.
        let mut disk = crate::perf::DiskAnchors {
            before_scale_seed_bytes: None,
            after_scale_seed_bytes: None,
            after_ward_seed_bytes: None,
            after_window_bytes: None,
            seed_compositions: perf_run::corpus::scale_shape(case.corpus.as_str())
                .ok()
                .and_then(|(ehrs, versions)| u64::try_from(ehrs.saturating_mul(versions)).ok()),
        };
        let probe_volume = |label: &str| -> Option<u64> {
            let db = &containers.as_ref()?.db;
            match perf_run::resources::db_volume_bytes(db) {
                Ok(bytes) => {
                    progress(format!("disk anchor {label}: {bytes} bytes"));
                    Some(bytes)
                }
                Err(e) => {
                    progress(format!("disk anchor {label} unavailable: {e}"));
                    None
                }
            }
        };
        let corpus = seed_corpus(
            &client,
            case.corpus.as_str(),
            &opt_xml,
            &journey_pack,
            request.seed_workers,
            &progress,
            &mut |milestone| match milestone {
                SeedStage::BeforeScale => {
                    disk.before_scale_seed_bytes = probe_volume("before scale seed");
                }
                SeedStage::AfterScale => {
                    disk.after_scale_seed_bytes = probe_volume("after scale seed");
                }
                SeedStage::AfterWard => {
                    disk.after_ward_seed_bytes = probe_volume("after preflight + ward seed");
                }
            },
        )?;
        // Settle the seeding's maintenance debt before the window: a
        // mid-window autovacuum/analyze of the freshly seeded tables would
        // saturate the engine inside the measurement.
        if let Some(c) = &containers {
            progress(
                "settling maintenance before the measured window (vacuumdb --analyze)".to_owned(),
            );
            if let Err(e) = perf_run::resources::settle_maintenance(&c.db) {
                progress(format!("maintenance not settled: {e}"));
            }
        }
        // The case's normative warmup; the sustained window extends by the
        // hours ladder (a longer hold of the same offered load is a stricter
        // demonstration of the same class).
        let warmup_s = case.workload.warmup.0;
        let duration_s = case.workload.duration.0.max(request.window.seconds());
        // The sampler brackets the whole window (warmup + sustained + the
        // completion drain) and stops after the dispatcher's last
        // completion lands — drive_case returns only then.
        let sampler = containers
            .as_ref()
            .map(|c| perf_run::resources::ResourceSampler::start(c, warmup_s, duration_s));
        let mut measurement = perf_run::window::drive_case(
            case,
            &principals,
            &corpus,
            &journey_pack,
            &catalogue,
            &environment,
            warmup_s,
            duration_s,
            &progress,
        )
        .map_err(|e| Error::Instrument(format!("measured run failed: {e}")))?;
        if let Some(sampler) = sampler {
            let (series, notes) = sampler.stop();
            for note in notes {
                progress(note);
            }
            disk.after_window_bytes = probe_volume("after measured window");
            let sampled_any = series.iter().any(|s| !s.samples.is_empty());
            let anchored_any = disk.before_scale_seed_bytes.is_some()
                || disk.after_scale_seed_bytes.is_some()
                || disk.after_ward_seed_bytes.is_some()
                || disk.after_window_bytes.is_some();
            if sampled_any || anchored_any {
                measurement.resources = Some(crate::perf::ResourcesRecord {
                    sample_interval_s: perf_run::resources::SAMPLE_INTERVAL.as_secs(),
                    containers: series,
                    disk: Some(disk),
                });
            } else {
                progress(
                    "resources: not sampled (container runtime unreachable for the whole run)"
                        .to_owned(),
                );
            }
        }
        observe(MeasuredEvent::Measured(&measurement));
        if measurement.verdict != crate::perf::ClassVerdict::Earned {
            run.earned_all = false;
        }
        // A limiter-shaped window is not a measurement of this server, so it
        // never reaches the results record.
        perf_run::refuse_rate_limited_record("perf").map_err(Error::Instrument)?;
        merge_measurement(
            request,
            new_record.as_ref(),
            &schedule,
            measurement.clone(),
            observe,
        )?;
        run.measurements.push(measurement);
    }
    Ok(run)
}

/// Whether a file exists at `path`.
///
/// # Errors
/// [`Error::Read`] when the path cannot be inspected at all.
fn record_exists(path: &Path) -> Result<bool, Error> {
    std::fs::exists(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })
}

/// Merges one record into the results document at the request's path, or
/// into `new_record` when no file exists there.
fn merge_measurement(
    request: &MeasuredRequest<'_>,
    new_record: Option<&Results>,
    schedule: &PerfSchedule,
    measurement: Measurement,
    observe: &(dyn Fn(MeasuredEvent<'_>) + Sync),
) -> Result<(), Error> {
    let (mut results, created) = if record_exists(request.results)? {
        let results: Results =
            load_party_json(request.results, &results_schema(), "results.schema.json")?;
        (results, false)
    } else {
        // NOTE: no openEHR spec governs this — our own design; a record is
        // created only at a path that held no file when the run began.
        let fresh = new_record.cloned().ok_or_else(|| {
            Error::Party(format!(
                "{}: the results document disappeared during the measured run",
                request.results.display()
            ))
        })?;
        (fresh, true)
    };
    results.measurements.retain(|m| m.case != measurement.case);
    // A measurement whose case is no longer in the performance schedule (a
    // renamed or retired case) is an orphan the verdict review would flag —
    // prune it here, visibly.
    results.measurements.retain(|m| {
        let known = schedule.cases.iter().any(|(_, c)| c.id == m.case);
        if !known {
            observe(MeasuredEvent::PrunedOrphan(&m.case));
        }
        known
    });
    results.measurements.push(measurement);
    results
        .measurements
        .sort_by(|a, b| a.case.as_str().cmp(b.case.as_str()));
    let document = to_json_document(&results, "serialize")?;
    crate::pipeline::write_file(request.results, &document)?;
    observe(if created {
        MeasuredEvent::Created(request.results)
    } else {
        MeasuredEvent::Merged(request.results)
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// The committed example results document — the one real record in the
    /// tree, carrying one measurement of the POC class.
    fn example_results() -> Results {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/results.example.json"
        ))
        .expect("the committed example results document");
        serde_json::from_str(&text).expect("the example document parses as results")
    }

    fn committed_schedule() -> PerfSchedule {
        load_clean_schedule(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../perf"
        )))
        .expect("the committed performance schedule loads")
    }

    /// Writes `results` into a temp directory and returns the path plus the
    /// directory guard, which must outlive the path.
    fn staged(results: &Results) -> (assert_fs::TempDir, std::path::PathBuf) {
        let dir = assert_fs::TempDir::new().expect("temp dir");
        let path = dir.path().join("results.json");
        let text = to_json_document(results, "serialize").expect("the record serializes");
        std::fs::write(&path, text).expect("staging the record");
        (dir, path)
    }

    /// The sustained-window ladder: only its rungs exist, nothing shorter
    /// than the case's own normative window, and the default is the first
    /// rung.
    #[test]
    fn only_the_ladders_rungs_are_sustainable_windows() {
        for hours in SustainedWindow::LADDER {
            let window = SustainedWindow::hours(*hours).expect("a rung of the ladder");
            assert_eq!(window.seconds(), hours * 3600);
        }
        assert!(SustainedWindow::hours(0).is_none());
        assert!(SustainedWindow::hours(3).is_none(), "3h is not a rung");
        assert!(SustainedWindow::hours(24).is_none());
        assert_eq!(SustainedWindow::default().seconds(), 3600);
    }

    /// Class selection over the committed schedule, and the refusal when
    /// the tree carries no case of the asked-for class.
    #[test]
    fn class_selection_finds_its_case_or_names_the_class_it_could_not() {
        let schedule = committed_schedule();
        let (path, case) = performance_case_of_class(&schedule, PerfClass::Poc, "POC")
            .expect("the catalogue carries the POC class");
        assert_eq!(case.class, PerfClass::Poc);
        assert!(path.to_string_lossy().ends_with(".yaml"), "{path:?}");

        let error = performance_case_of_class(&PerfSchedule::default(), PerfClass::Poc, "POC")
            .expect_err("an empty tree carries no performance case");
        assert!(
            matches!(&error, Error::Missing(m) if m.contains("class POC")),
            "{error}"
        );
    }

    /// The two run-context readers over the committed tree, and their
    /// refusals over an empty one. Each names the artifact it wanted, so a
    /// half-built root cannot look like an empty catalogue.
    #[test]
    fn the_run_context_readers_load_or_name_the_missing_artifact() {
        let schedule = committed_schedule();
        assert!(
            scale_opt_xml(&schedule)
                .expect("the committed corpus carries the blood-pressure OPT")
                .contains("template"),
            "the OPT fixture is not an operational template"
        );
        let (catalogue, pack) =
            journey_context(&schedule).expect("the committed tree carries the journey context");
        assert!(catalogue.check_invariants().is_ok());
        assert!(
            !pack.templates.is_empty(),
            "the loaded pack carries no template"
        );

        let empty = PerfSchedule::default();
        let error = scale_opt_xml(&empty).expect_err("an empty tree has no payload manifest");
        assert!(
            matches!(&error, Error::Missing(m) if m.contains(PAYLOADS_FILE)),
            "{error}"
        );
        let error = journey_context(&empty).expect_err("an empty tree has no journey catalogue");
        assert!(
            matches!(&error, Error::Missing(m) if m.contains(JOURNEYS_FILE)),
            "{error}"
        );
    }

    /// Seeding refuses an unknown scale key before it opens a single
    /// connection: a corpus key nobody defined has no volumetric shape, and
    /// guessing one would publish a measurement of an unnamed population.
    #[test]
    fn seeding_refuses_an_unknown_corpus_key_before_any_wire_call() {
        let ixit: Ixit = serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://127.0.0.1:1", "auth": { "mode": "none" } } }
        }))
        .expect("the single-instance topology parses");
        let client = PerfClient::from_instance(
            ixit.default_instance().expect("the default instance"),
            &ixit,
        )
        .expect("a credential-less client builds");
        let pack = JourneyPack {
            templates: Vec::new(),
            aux: pack::AuxPayloads::default(),
        };
        let error = seed_corpus(
            &client,
            "cnf.scale.7k",
            "<opt/>",
            &pack,
            1,
            &|_| {},
            &mut |_| {},
        )
        .expect_err("no such rung of the scale ladder");
        assert!(
            matches!(&error, Error::Instrument(m) if m.contains("cnf.scale.7k")),
            "{error}"
        );
    }

    /// Every instrument reads its class token through the same closed
    /// vocabulary, and an unknown one is refused before the catalogue is
    /// read, let alone a SUT contacted.
    #[test]
    fn an_unknown_class_token_is_refused_by_both_instruments() {
        let root = Path::new("perf");
        let ixit = Path::new("ixit.json");
        let measured = run_measured(
            &MeasuredRequest {
                root,
                ixit,
                results: Path::new("results.json"),
                class: "PLATINUM",
                seed_workers: 1,
                window: SustainedWindow::default(),
                sut: OperatorIdentity::default(),
            },
            &|_| {},
        )
        .expect_err("PLATINUM is not a performance class");
        assert!(matches!(measured, Error::Selector(_)), "{measured}");

        let stress = run_stress(
            &StressRequest {
                root,
                ixit,
                corpus_class: "PLATINUM",
                seed_workers: 1,
                step_secs: 10,
                bisections: 0,
                max_rate: 8.0,
            },
            &|_| {},
        )
        .expect_err("PLATINUM is not a performance class");
        assert!(matches!(stress, Error::Selector(_)), "{stress}");
    }

    /// The seeding milestones the disk anchors are probed at are a closed
    /// set, and each renders distinctly in a progress line.
    #[test]
    fn every_seed_stage_renders_distinctly() {
        let rendered: Vec<String> = [
            SeedStage::BeforeScale,
            SeedStage::AfterScale,
            SeedStage::AfterWard,
        ]
        .iter()
        .map(|stage| format!("{stage:?}"))
        .collect();
        assert_eq!(rendered.len(), 3);
        let mut unique = rendered.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique.len(),
            3,
            "two milestones render the same: {rendered:?}"
        );
    }

    /// A merge over the committed catalogue: one record per case, no orphans,
    /// the set written back sorted.
    #[test]
    fn a_merge_replaces_the_record_of_its_own_case() {
        let schedule = committed_schedule();
        let prior = example_results();
        let (_dir, path) = staged(&prior);
        let mut measurement = prior
            .measurements
            .first()
            .expect("the example is measured")
            .clone();
        // A second window over the same case, holding a longer sustained load.
        measurement.duration_s = measurement.duration_s.saturating_mul(2);

        let events = Mutex::new(Vec::new());
        let request = MeasuredRequest {
            root: Path::new("perf"),
            ixit: Path::new("ixit.json"),
            results: &path,
            class: "POC",
            seed_workers: 1,
            window: SustainedWindow::default(),
            sut: OperatorIdentity::default(),
        };
        merge_measurement(&request, None, &schedule, measurement.clone(), &|event| {
            events
                .lock()
                .expect("the observer lock is uncontended")
                .push(format!("{event:?}"));
        })
        .expect("the merge writes the document");

        let merged: Results = serde_json::from_str(
            &std::fs::read_to_string(&path).expect("the merged document is readable"),
        )
        .expect("the merged document parses");
        assert_eq!(
            merged.measurements.len(),
            1,
            "the new record replaces the prior one for the same case"
        );
        assert_eq!(
            merged.measurements[0].duration_s, measurement.duration_s,
            "the merged record is the new one"
        );
        let seen = events.lock().expect("the observer lock is uncontended");
        assert_eq!(seen.len(), 1, "one event: the merge itself ({seen:?})");
        assert!(seen[0].starts_with("Merged("), "{seen:?}");
    }

    /// A record whose case the catalogue no longer carries is pruned, and the
    /// pruning is reported rather than done silently — an orphan the verdict
    /// review would otherwise flag.
    #[test]
    fn a_record_of_a_retired_case_is_pruned_visibly() {
        let schedule = committed_schedule();
        let mut prior = example_results();
        let mut orphan = prior
            .measurements
            .first()
            .expect("the example is measured")
            .clone();
        orphan.case =
            CaseId::parse("PERF-hospital_sim-class_RETIRED").expect("a well-formed case id");
        prior.measurements.push(orphan.clone());
        let (_dir, path) = staged(&prior);
        let measurement = prior.measurements[0].clone();

        let pruned = Mutex::new(Vec::new());
        let request = MeasuredRequest {
            root: Path::new("perf"),
            ixit: Path::new("ixit.json"),
            results: &path,
            class: "POC",
            seed_workers: 1,
            window: SustainedWindow::default(),
            sut: OperatorIdentity::default(),
        };
        merge_measurement(&request, None, &schedule, measurement, &|event| {
            if let MeasuredEvent::PrunedOrphan(case) = event {
                pruned
                    .lock()
                    .expect("the observer lock is uncontended")
                    .push(case.to_string());
            }
        })
        .expect("the merge writes the document");

        assert_eq!(
            *pruned.lock().expect("the observer lock is uncontended"),
            vec![orphan.case.to_string()],
            "the orphan is named as it is dropped"
        );
        let merged: Results = serde_json::from_str(
            &std::fs::read_to_string(&path).expect("the merged document is readable"),
        )
        .expect("the merged document parses");
        assert!(
            merged.measurements.iter().all(|m| m.case != orphan.case),
            "the orphan record does not survive the merge"
        );
    }

    /// The merge reads the results document through its published schema, so
    /// a document that is not a results record is refused before anything is
    /// written.
    #[test]
    fn a_merge_refuses_a_document_that_is_not_a_results_record() {
        let schedule = PerfSchedule::default();
        let dir = assert_fs::TempDir::new().expect("temp dir");
        let path = dir.path().join("results.json");
        std::fs::write(&path, "{}").expect("staging the wrong document");
        let measurement = example_results()
            .measurements
            .first()
            .expect("the example is measured")
            .clone();
        let request = MeasuredRequest {
            root: Path::new("perf"),
            ixit: Path::new("ixit.json"),
            results: &path,
            class: "POC",
            seed_workers: 1,
            window: SustainedWindow::default(),
            sut: OperatorIdentity::default(),
        };
        let error = merge_measurement(&request, None, &schedule, measurement, &|_| {})
            .expect_err("an empty object is not a results record");
        assert!(matches!(error, Error::Party(_)), "{error}");
        assert_eq!(
            std::fs::read_to_string(&path).expect("the staged document is readable"),
            "{}",
            "a refused merge writes nothing"
        );
    }

    /// The first measurement at a path that holds no file lands in a new
    /// record: the document is created, reads back through the published
    /// results schema, and carries exactly that measurement and no outcome.
    #[test]
    fn the_first_measurement_creates_the_record_it_lands_in() {
        let schedule = committed_schedule();
        let dir = assert_fs::TempDir::new().expect("temp dir");
        let path = dir.path().join("results.json");
        let measurement = example_results()
            .measurements
            .first()
            .expect("the example is measured")
            .clone();
        let header = new_measured_record(Sut::operator("cdr", "1.0"), "{}");

        let events = Mutex::new(Vec::new());
        let request = MeasuredRequest {
            root: Path::new("perf"),
            ixit: Path::new("ixit.json"),
            results: &path,
            class: "POC",
            seed_workers: 1,
            window: SustainedWindow::default(),
            sut: OperatorIdentity::default(),
        };
        merge_measurement(
            &request,
            Some(&header),
            &schedule,
            measurement.clone(),
            &|event| {
                events
                    .lock()
                    .expect("the observer lock is uncontended")
                    .push(format!("{event:?}"));
            },
        )
        .expect("the first measurement creates the record");

        let created: Results = load_party_json(&path, &results_schema(), "results.schema.json")
            .expect("the created record validates against the published results schema");
        assert_eq!(created.measurements.len(), 1);
        assert_eq!(created.measurements[0].case, measurement.case);
        assert!(created.outcomes.is_empty(), "perf records no outcome");
        assert_eq!(created.sut, Sut::operator("cdr", "1.0"));
        assert_eq!(
            created.selection_basis,
            Some(SelectionBasis::StatementBlind)
        );
        assert_eq!(created.statement_digest, None);
        assert_eq!(created.ixit_digest, ixit_digest("{}"));
        let seen = events.lock().expect("the observer lock is uncontended");
        assert_eq!(seen.len(), 1, "one event: the creation itself ({seen:?})");
        assert!(seen[0].starts_with("Created("), "{seen:?}");
    }

    /// A second case of the same class merges into the record the first one
    /// created rather than replacing it with a second new record.
    #[test]
    fn a_created_record_is_merged_into_by_the_next_case() {
        let schedule = committed_schedule();
        let dir = assert_fs::TempDir::new().expect("temp dir");
        let path = dir.path().join("results.json");
        let measurement = example_results()
            .measurements
            .first()
            .expect("the example is measured")
            .clone();
        let first_header = new_measured_record(Sut::operator("first", "1"), "{}");
        let later_header = new_measured_record(Sut::operator("later", "2"), "{}");
        let request = MeasuredRequest {
            root: Path::new("perf"),
            ixit: Path::new("ixit.json"),
            results: &path,
            class: "POC",
            seed_workers: 1,
            window: SustainedWindow::default(),
            sut: OperatorIdentity::default(),
        };
        merge_measurement(
            &request,
            Some(&first_header),
            &schedule,
            measurement.clone(),
            &|_| {},
        )
        .expect("the first measurement creates the record");
        merge_measurement(
            &request,
            Some(&later_header),
            &schedule,
            measurement,
            &|_| {},
        )
        .expect("the next measurement merges");

        let merged: Results = load_party_json(&path, &results_schema(), "results.schema.json")
            .expect("the record still validates");
        assert_eq!(
            merged.sut,
            Sut::operator("first", "1"),
            "an existing record keeps its own header"
        );
        assert_eq!(merged.measurements.len(), 1);
    }

    /// A path that holds no file and no header to create a record from is
    /// refused, and nothing is written there.
    #[test]
    fn a_missing_record_without_a_header_is_refused() {
        let schedule = committed_schedule();
        let dir = assert_fs::TempDir::new().expect("temp dir");
        let path = dir.path().join("results.json");
        let measurement = example_results()
            .measurements
            .first()
            .expect("the example is measured")
            .clone();
        let request = MeasuredRequest {
            root: Path::new("perf"),
            ixit: Path::new("ixit.json"),
            results: &path,
            class: "POC",
            seed_workers: 1,
            window: SustainedWindow::default(),
            sut: OperatorIdentity::default(),
        };
        let error = merge_measurement(&request, None, &schedule, measurement, &|_| {})
            .expect_err("no file and no header is no record");
        assert!(
            matches!(&error, Error::Party(m) if m.contains("disappeared")),
            "{error}"
        );
        assert!(!path.exists(), "a refused merge writes nothing");
    }

    /// The new record's header is the run's own facts: the instrument's
    /// version and schedule release, the defaulted profile a campaign no
    /// statement selected records, and the server's ITS-REST release only
    /// when the server stated one.
    #[test]
    fn a_new_record_states_only_facts_of_its_run() {
        let mut sut = Sut::operator("cdr", "1.0");
        let blind = new_measured_record(sut.clone(), "{}");
        assert_eq!(blind.runner.name, "veredictum");
        assert_eq!(blind.runner.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(
            blind.runner.verification_pack_status,
            VerificationPackStatus::NotRun
        );
        assert_eq!(blind.instrument_status, InstrumentStatus::CURRENT);
        assert_eq!(blind.schedule_release, SCHEDULE_RELEASE);
        assert_eq!(blind.tech_profile.its, ItsName::ItsRest);
        assert_eq!(blind.tech_profile.formats, FormatName::ALL.to_vec());
        assert_eq!(
            blind.tech_profile.source,
            Some(TechProfileSource::Defaulted)
        );
        assert_eq!(blind.sut.restapi_specs_version, None);
        assert_eq!(blind.provenance_contradiction(), None);

        sut.restapi_specs_version = Some(crate::party::IdentityField::server("1.0.3"));
        let stated = new_measured_record(sut, "{}");
        assert_eq!(
            stated.sut.restapi_specs_version,
            Some(crate::party::IdentityField::server("1.0.3"))
        );
        let text = serde_json::to_string(&stated).unwrap();
        assert_eq!(text.matches("restapi_specs_version").count(), 1, "{text}");
    }
}
