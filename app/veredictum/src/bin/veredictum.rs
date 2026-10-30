// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The Veredictum command line.
//!
//! Every instrument is a subcommand. `veredictum --help` lists them, and
//! `veredictum <subcommand> --help` documents one; the book's command
//! reference is that output rendered per subcommand, with the notes behind
//! each flag beside it.
//!
//! Every subcommand is a clap front end over one seam of
//! [`veredictum::pipeline`]: this file parses arguments, renders the seam's
//! typed result to the console, and maps it to an exit code. Nothing here
//! decides anything a second consumer of the library would have to decide
//! again.
//!
//! Exit codes: `0` clean · `1` findings · `2` runner error.

// Verification CLI: progress/diagnostics on the console ARE this tool's user
// interface, so stdio is the right channel here; only library crates are
// restricted to `tracing`.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "this IS the CLI: stdout carries the run report and stderr the \
              diagnostics; only library crates are restricted to `tracing`"
)]

use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::LazyLock;

use clap::{Parser, Subcommand};

use veredictum::arazzo::credential::{CredentialSource, Role};
use veredictum::arazzo::terminal::{ColorChoice, ShowOptions, render, wants_color};
use veredictum::bench::client::AuthKind;
use veredictum::certificate::{HONESTY_LINE, Jwks, Outcome, verify};
use veredictum::identity::OperatorIdentity;
use veredictum::party::version_line;
use veredictum::pipeline::arazzo_run::{ArazzoRunRequest, Drive, run_catalogue};
use veredictum::pipeline::assets::{performance_assets, schema_files, stress_overlay};
use veredictum::pipeline::bench::{BenchRequest, compare_bench, describe_packs, run_bench};
use veredictum::pipeline::export::{export_results, show_results};
use veredictum::pipeline::judgement::{JudgementRequest, judge};
use veredictum::pipeline::measured::{
    MeasuredEvent, MeasuredRequest, StressRequest, SustainedWindow, run_measured, run_stress,
};
use veredictum::pipeline::perf_judge::judge_perf_result;
use veredictum::pipeline::perf_workload::{CorpusMode, WorkloadRunRequest, run_workload};
use veredictum::pipeline::{RenderedFile, ensure_parent_dir, to_json_document, write_file};

/// What `--version` prints after the name: the version and the instrument's
/// approval status, `0.1.7 (pre-approval: not yet approved by the openEHR SEC)`
/// for a 0.x build.
static VERSION_LINE: LazyLock<String> = LazyLock::new(|| version_line(env!("CARGO_PKG_VERSION")));

#[derive(Parser)]
#[command(
    name = "veredictum",
    about = "The independent conformance instrument for openEHR clinical data repositories",
    version = VERSION_LINE.as_str()
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write the published JSON-Schema set (byte-deterministic).
    EmitSchemas {
        /// Output directory (created if missing).
        #[arg(long)]
        out: PathBuf,
    },
    /// Drive the Arazzo catalogue against a server under test, or re-judge
    /// the recording of an earlier run, and write `results.json` and the HAR
    /// recording.
    ///
    /// The whole catalogue is gated first, as `validate` gates it, and nothing
    /// runs while the gate has a finding. Every document under `cases/` runs,
    /// in path order, except the shared setups under `cases/_setup/`, which
    /// run only where a case names them in `dependsOn`. A workflow whose rows
    /// file says `server: exclusive` runs in a final phase, after every other
    /// workflow, and a re-judgement replays in the same order. Every row is
    /// printed, and a failed or errored row with its four-part explanation.
    /// The run ends with the view `show` prints, coloured only when standard
    /// output is a terminal and `NO_COLOR` is unset or empty.
    ///
    /// The results record, under `server`, what the server stated through
    /// ITS-REST System OPTIONS: the run's own exchange when a workflow sent
    /// one, otherwise one read after the catalogue ran, which the recording
    /// keeps, so a re-judgement never asks the server. A member the server did
    /// not state stays absent. When the statement declares
    /// `spec_versions.its_rest` and the server states a different
    /// `restapi_specs_version`, the results carry a `review` note and the run
    /// prints it; the note changes no row and no exit status.
    ///
    /// A credential is read from the environment, never from an argument:
    /// from the variables `--auth` and `--admin-auth` name, or from the file
    /// named by the same variable with `_FILE` appended. Neither credential is
    /// recorded. The recording holds the server's response bodies verbatim,
    /// so it belongs where the operator keeps the run.
    ///
    /// Exit status: `0` when every row passed or was skipped, or failed only
    /// as a warning (a SHOULD, which never counts against a capability); `1`
    /// when any other row failed or errored; `2` when the run could not start
    /// or finish: a usage error, a gate finding, an unreadable file, a missing
    /// credential, or a recording that does not replay.
    Test {
        /// The catalogue root, holding `capabilities.yaml` and `cases/`. Its
        /// parent holds the vendored `specs/` tree its citations and OAS
        /// sources resolve against. The container image sets it to the
        /// catalogue it ships.
        #[arg(
            long,
            env = "VEREDICTUM_CATALOGUE",
            hide_env_values = true,
            default_value = "catalogue"
        )]
        root: PathBuf,
        /// The server's base URL, up to and including the openEHR REST base
        /// (for example `https://cdr.example/openehr/v1`).
        #[arg(long, required_unless_present = "rejudge")]
        base_url: Option<String>,
        /// The SMART Platform base URL (for example
        /// `https://platform.example`), which SMART on openEHR serves its
        /// discovery document relative to and which can differ from the
        /// openEHR REST base. Without it, every workflow with a step sent to it
        /// is skipped as `not-declared`.
        #[arg(long)]
        platform_url: Option<String>,
        /// How the CDR wants the client to log in: `none`, `basic` or
        /// `bearer`.
        ///
        /// `none`: the CDR needs no login.
        ///
        /// `basic`: a username and password, from `VEREDICTUM_CDR_USER` and
        /// `VEREDICTUM_CDR_PASSWORD`.
        ///
        /// `bearer`: a token, from `VEREDICTUM_CDR_TOKEN`.
        #[arg(long, default_value = "none")]
        auth: String,
        /// How the CDR wants the administrative principal to log in: `none`,
        /// `basic` or `bearer`. With `none`, every workflow with an
        /// administrative step is skipped as `not-declared`.
        ///
        /// `basic`: a username and password, from `VEREDICTUM_CDR_ADMIN_USER`
        /// and `VEREDICTUM_CDR_ADMIN_PASSWORD`.
        ///
        /// `bearer`: a token, from `VEREDICTUM_CDR_ADMIN_TOKEN`.
        #[arg(long, default_value = "none")]
        admin_auth: String,
        /// The party's statement (JSON), which decides what runs.
        ///
        /// Its `claims.capabilities` names the capability folders under
        /// `cases/` it claims, at least one; every workflow of a document in
        /// another folder is skipped as `not-claimed`, while a setup a claimed
        /// document depends on still runs. Its `options` list selects one arm
        /// of each option in `capabilities.yaml`, and the documents of every
        /// other arm are skipped as `option-not-selected`. A document whose
        /// `applies` range in `capabilities.yaml` excludes a version its
        /// `spec_versions` declares is skipped as `version-out-of-range`, and
        /// one needing a wire format its its-rest `tech_profiles` entry does
        /// not list as `format-not-claimed`. A name that is not a folder or an
        /// arm stops the run. Without a statement, every capability runs, in
        /// every version and format, and no arm is selected.
        #[arg(long)]
        statement: Option<PathBuf>,
        /// The party's IXIT (JSON), which the `declared` inputs of a rows
        /// file read. A row whose declared value is missing is skipped as
        /// `not-declared`.
        ///
        /// Its `scoped_tokens` member declares the tokens a step that names a
        /// SMART scope set presents: `issued` maps a scope set to the variable
        /// (`VEREDICTUM_SCOPED_TOKEN_<NAME>`, or the file its `_FILE` names)
        /// holding a token issued for it, and `mint` declares a test issuer the
        /// deployment trusts, whose RSA key is read from
        /// `VEREDICTUM_SCOPED_TOKEN_KEY`. A row whose scope set neither covers is
        /// skipped as `not-declared`; a declared variable that is not set stops
        /// the run before any request.
        #[arg(long)]
        ixit: Option<PathBuf>,
        /// Run only this document, by its id (`ehr` for
        /// `cases/ehr/ehr.arazzo.yaml`). Repeat the flag once per document;
        /// without it, every document runs.
        #[arg(long = "document", value_name = "ID")]
        documents: Vec<String>,
        /// Where `results.json` and `recording.har` are written. Required for
        /// a live run; a re-judgement writes only `results.json`, and only
        /// when this is given.
        #[arg(long, required_unless_present = "rejudge")]
        out: Option<PathBuf>,
        /// Re-judge this HAR recording instead of driving a server: every
        /// request is answered from the recording, so the results are the
        /// bytes the recorded run wrote. Pass the root, statement, IXIT and
        /// documents the recorded run was given.
        #[arg(long, value_name = "RECORDING", conflicts_with_all = ["base_url", "platform_url", "auth", "admin_auth"])]
        rejudge: Option<PathBuf>,
    },
    /// Read a server's identity from the ITS-REST System OPTIONS operation and
    /// print it as one JSON document.
    ///
    /// Sends `OPTIONS <base>/` with `Accept: application/json`. The document's
    /// `outcome` is `offered` (a 200 `Options` document), `not_offered` (any
    /// other status from 100 to 599 outside 2xx, which the operation's SHOULD
    /// permits), `not_options_document` (a 2xx that is not that document) or
    /// `invalid_status` (a code from 600 to 999, which RFC 9110 §15 makes
    /// invalid). Nothing is judged and nothing counts toward conformance.
    Identify {
        /// The server's base URL, up to and including the openEHR REST base.
        #[arg(long)]
        base_url: String,
        /// How the CDR wants the client to log in: `none`, `basic` or
        /// `bearer`. The secret never rides argv: each variable can instead
        /// name a file through its `_FILE` form.
        ///
        /// `none`: the CDR needs no login.
        ///
        /// `basic`: a username and password, from `VEREDICTUM_CDR_USER` and
        /// `VEREDICTUM_CDR_PASSWORD`.
        ///
        /// `bearer`: a token, from `VEREDICTUM_CDR_TOKEN`.
        #[arg(long, default_value = "none")]
        auth: String,
    },
    /// Validate the Arazzo catalogue through every gate, and review one
    /// supplied statement against it.
    Validate {
        /// The catalogue root, holding `capabilities.yaml` and `cases/`.
        #[arg(long)]
        root: PathBuf,
        /// The vendored openEHR spec tree; enables citation resolution and
        /// the surface gate.
        #[arg(long)]
        specs: Option<PathBuf>,
        /// A submitted declaration (`statement.json`) to hold to the static
        /// review: its published schema, the capability folders it claims, the
        /// option arms it selects, and that every claimed folder holds a
        /// workflow its declarations let run.
        ///
        /// ISO/IEC 9646-7 assigns an ICS proforma's support and
        /// supported-values columns to the supplier of the implementation, so
        /// no declaration is committed here and none is swept from the tree.
        #[arg(long)]
        statement: Option<PathBuf>,
    },
    /// Run a workload document against a live SUT and write one
    /// `perf-result` (`--workload`), or execute the performance schedule's
    /// class run and merge its measurement records into a results.json
    /// (`--class`).
    ///
    /// With `--workload`, the corpus is seeded (or reused with `--corpus
    /// reuse`) before the clock, then the run takes at most ten minutes:
    /// settle, warm-up, a scout ramp, a ladder of four plateaus at multiples
    /// of the knee estimate, and the closing canaries. The planned duration
    /// is printed before the first measured request. A server that answers
    /// any exchange `429` gets no record.
    ///
    /// With `--class`:
    /// A new record carries the SUT identity the server states through System
    /// OPTIONS, read before any seeding, with `--sut-name` and `--sut-version`
    /// filling what it does not state. It records no conformance outcome:
    /// nothing selected a statement, so it is `statement_blind`, and the
    /// verification pack is `not_run`. An existing file keeps its header and
    /// gains or replaces the measured case's record.
    Perf {
        /// The performance schedule root, `perf/` in this repository
        /// (workloads/, payloads.yaml, payloads/, and for `--class` the
        /// classes/ and journeys.yaml)
        #[arg(long)]
        root: PathBuf,
        /// The ixit topology file (JSON) — its environment block is
        /// mandatory for a measured run.
        #[arg(long)]
        ixit: PathBuf,
        /// The workload document to run, an embedded one under
        /// `perf/workloads/` or the evaluator's own (labelled `custom`).
        #[arg(long, required_unless_present = "class", conflicts_with_all = ["class", "results", "hours", "sut_name", "sut_version"])]
        workload: Option<PathBuf>,
        /// Where `--workload` writes its `perf-result` record.
        #[arg(
            long,
            requires = "workload",
            required_unless_present = "class",
            conflicts_with = "class"
        )]
        out: Option<PathBuf>,
        /// The catalogue root a workload's `requires` resolve against.
        #[arg(
            long,
            default_value = "catalogue",
            requires = "workload",
            conflicts_with = "class"
        )]
        catalogue: PathBuf,
        /// A statement whose claims keep the interactions it covers and
        /// drop the rest, with the drop recorded.
        #[arg(long, requires = "workload", conflicts_with = "class")]
        statement: Option<PathBuf>,
        /// Seed the workload's corpus, or reuse the stored one: a reuse is
        /// refused before the warm-up when the server's read-back counts or
        /// the stored digest differ.
        #[arg(long, value_enum, default_value_t = CorpusArg::Seed, requires = "workload", conflicts_with = "class")]
        corpus: CorpusArg,
        /// The stored corpus state a seeding writes and a reuse reads;
        /// defaults to the record's path with the extension `corpus.json`.
        #[arg(long, requires = "workload", conflicts_with = "class")]
        corpus_state: Option<PathBuf>,
        /// The warm-up's rate and the scout's starting rate, operations per
        /// second.
        #[arg(long, default_value_t = veredictum::perf_emulate::run::DEFAULT_START_RATE, requires = "workload", conflicts_with = "class")]
        start_rate: f64,
        /// The scout ramp's end rate, operations per second.
        #[arg(long, default_value_t = veredictum::perf_emulate::run::DEFAULT_SCOUT_END_RATE, requires = "workload", conflicts_with = "class")]
        scout_end_rate: f64,
        /// The host prescription the run claims to have run on.
        #[arg(long, requires = "workload", conflicts_with = "class")]
        host_prescription: Option<String>,
        /// The measured results record (`results.schema.json`) to merge the
        /// measurement records into; created when no file exists at the path.
        #[arg(long, required_unless_present = "workload")]
        results: Option<PathBuf>,
        /// The SUT name a new record carries when the server does not state
        /// one through System OPTIONS. Unread when `--results` exists.
        #[arg(long)]
        sut_name: Option<String>,
        /// The SUT version a new record carries when the server does not
        /// state one through System OPTIONS. Unread when `--results` exists.
        #[arg(long)]
        sut_version: Option<String>,
        /// Select the performance case(s) of this class (POC | S | L | R).
        #[arg(long)]
        class: Option<String>,
        /// Parallel seeding workers.
        #[arg(long, default_value_t = 16)]
        seed_workers: usize,
        /// The sustained-window ladder of `--class`: hours to hold the
        /// offered load — 1 (default, the case's normative window) | 2 | 4 |
        /// 6 | 8 | 12.
        #[arg(long, default_value_t = 1)]
        hours: u64,
    },
    /// Run the step-load STRESS instrument: geometric load steps to the
    /// maximum sustainable throughput — where the system breaks
    /// (exploration only; never a conformance record, and class-free by
    /// design).
    Stress {
        /// The performance schedule root, `perf/` in this repository
        /// (classes/, journeys.yaml, payloads.yaml, payloads/)
        #[arg(long)]
        root: PathBuf,
        /// The ixit topology file (JSON) — its environment block is
        /// mandatory (a throughput number without the deployment described
        /// is meaningless).
        #[arg(long)]
        ixit: PathBuf,
        /// Where to write the stress report (stress.json).
        #[arg(long)]
        out: PathBuf,
        /// The class-scale corpus the stress runs on (POC | S | L | R —
        /// the standardized corpus selector): data volume + workload mix
        /// only; no class floor enters the stress report or chart.
        #[arg(long, default_value = "POC")]
        corpus_class: String,
        /// Parallel seeding workers.
        #[arg(long, default_value_t = 16)]
        seed_workers: usize,
        /// Each load step's recorded hold, seconds (short + intense by
        /// design).
        #[arg(long, default_value_t = 120)]
        step_secs: u64,
        /// Post-breach bisection refinements.
        #[arg(long, default_value_t = 3)]
        bisections: u32,
        /// The climb cap (arrivals/s).
        #[arg(long, default_value_t = 4096.0)]
        max_rate: f64,
    },
    /// Run the universal SPEED benchmark: an embedded pack against any
    /// reachable CDR over its base URL, seeded once and measured N times
    /// open-loop (comparative speed only; never a conformance record, a
    /// certificate, or a performance-class rating).
    Bench {
        /// The system's base URL, up to and including the openEHR REST base
        /// (for example `https://cdr.example/openehr/v1`).
        #[arg(long)]
        base_url: String,
        /// How the client presents itself: `none`, `basic` or `bearer`.
        ///
        /// The secret never rides argv: `basic` reads
        /// `VEREDICTUM_CDR_PASSWORD` and `bearer` reads
        /// `VEREDICTUM_CDR_TOKEN`, the variables `test` and `identify` read,
        /// each also from the file its `_FILE` form names.
        #[arg(long, default_value = "none")]
        auth: String,
        /// The user `--auth basic` presents; without it, the user is read
        /// from `VEREDICTUM_CDR_USER`.
        #[arg(long)]
        user: Option<String>,
        /// The embedded pack to drive.
        #[arg(long, default_value = "smoke")]
        pack: String,
        /// The posture profile to declare, out of the set the pack defines.
        /// Omit to take the pack's first, which is always `minimal`.
        ///
        /// The run's canaries check the declaration against the running system
        /// before and after the measured window, and a contradiction refuses
        /// the run.
        #[arg(long)]
        posture: Option<String>,
        /// How many times to repeat the measured phases. A result with fewer
        /// than three is recorded as not submittable, and names that as one of
        /// its unmet requirements.
        #[arg(long, default_value_t = 3)]
        repetitions: u32,
        /// Multiply the pack's EHR count by this factor, for a shorter run.
        /// Anything but `1.0` takes the run off the pack's pinned
        /// configuration, and the record says so.
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
        /// Override the worker count every seed phase declares. Omit to run
        /// the pack's own value, which is what its reference figures describe.
        #[arg(long)]
        seed_workers: Option<usize>,
        /// After the target's run, compose each pinned reference CDR on this
        /// host from its own digest-pinned images, drive the same pack at the
        /// same seed against it, and record the relative index. Needs the
        /// docker CLI; a record without a baseline is not submittable.
        #[arg(long)]
        with_baselines: bool,
        /// Output directory for the result document and its summary.
        #[arg(long)]
        out: PathBuf,
        /// A label for this run, which names its column in a comparison and
        /// distinguishes its file name in a shared output directory.
        #[arg(long)]
        label: Option<String>,
    },
    /// Align two or more committed bench results into one table, one column
    /// per file, with every pack or host mismatch stated in the header.
    BenchCompare {
        /// A committed bench result. Repeat the flag once per file; at least
        /// two are needed.
        #[arg(long = "result", required = true)]
        results: Vec<PathBuf>,
        /// Output directory for the rendered comparison.
        #[arg(long)]
        out: PathBuf,
    },
    /// Write the embedded benchmark pack manifest: every pack's phases with
    /// their load discipline, its operation mix with what each entry probes,
    /// its posture profiles, its fixture pins with their provenance, its seed,
    /// and the submission requirements (byte-deterministic).
    BenchPacks {
        /// Output directory (created if missing).
        #[arg(long)]
        out: PathBuf,
    },
    /// Load the performance workloads through their gates and print, for each,
    /// the expanded per-operation mix, its read:write ratio and the planned run
    /// range after seeding. With --out, write the byte-deterministic manifest of
    /// the pinned workloads with their digests and the page generated from it;
    /// with --workload, check one document instead, labelled custom unless it
    /// matches a pinned digest.
    PerfWorkloads {
        /// The performance root, perf/ in this repository (workloads/,
        /// payloads.yaml, payloads/)
        #[arg(long)]
        root: PathBuf,
        /// The catalogue root a workload's requires tokens resolve against
        /// (catalogue/ in this repository)
        #[arg(long)]
        catalogue: PathBuf,
        /// Output directory for perf-workloads.json and perf-workloads.md
        /// (created if missing)
        #[arg(long, conflicts_with = "workload")]
        out: Option<PathBuf>,
        /// One workload document to check and summarize
        #[arg(long)]
        workload: Option<PathBuf>,
    },
    /// Render the cross-SUT stress overlay (both systems' latency-throughput
    /// curves on one canvas) FROM two committed stress reports —
    /// deterministic, both directions on equal footing.
    StressCompare {
        /// The primary SUT's committed stress.json.
        #[arg(long)]
        left: PathBuf,
        /// The primary SUT's display label.
        #[arg(long)]
        left_label: String,
        /// The comparison SUT's committed stress.json.
        #[arg(long)]
        right: PathBuf,
        /// The comparison SUT's display label.
        #[arg(long)]
        right_label: String,
        /// Where to write the overlay SVG.
        #[arg(long)]
        out: PathBuf,
    },
    /// Render the published performance SVG assets FROM a committed
    /// results.json (deterministic; CI regenerates and diffs — hand-drawn
    /// numbers are a build failure).
    PerfAssets {
        /// The performance schedule root, `perf/` in this repository (for the
        /// class-ladder floors)
        #[arg(long)]
        root: PathBuf,
        /// The committed results.json carrying the measurement records.
        #[arg(long)]
        results: PathBuf,
        /// Output directory for the SVG files.
        #[arg(long)]
        out: PathBuf,
        /// Also write the generated Markdown summary (class ladder +
        /// measured detail) to this path — the book's build-time include.
        #[arg(long)]
        summary: Option<PathBuf>,
        /// A committed stress report (stress.json) to render the
        /// latency-throughput curve from, when one exists.
        #[arg(long)]
        stress: Option<PathBuf>,
    },
    /// Re-derive the measured performance-class verdicts from a statement and
    /// a results record against the performance schedule (the pure
    /// pipeline), and write them as `verdicts.json`.
    Verdicts {
        /// The party statement (`statement.json`).
        #[arg(long)]
        statement: PathBuf,
        /// The party results (`results.json`).
        #[arg(long)]
        results: PathBuf,
        /// The performance schedule root, `perf/` in this repository
        /// (classes/, journeys.yaml, payloads.yaml, payloads/).
        #[arg(long)]
        root: PathBuf,
        /// Output directory for verdicts.json.
        #[arg(long)]
        out: PathBuf,
    },
    /// Re-derive every number of a `perf-result` record from its recorded
    /// histograms, and compare the result with the record byte for byte.
    ///
    /// The record is checked against `perf-result.schema.json`, its `derived`
    /// part is computed again from its `raw` part, and every derived field
    /// that differs is printed with its JSON pointer, the recorded value and
    /// the re-derived one. The file must also equal the canonical rendering of
    /// the re-derived record, so a number written in a different form counts
    /// as a difference.
    ///
    /// Exit status: `0` when the record reproduces byte for byte; `1` when a
    /// derived field differs or the file is not in canonical form; `2` when the
    /// file cannot be read, is not JSON, breaks the schema, or its raw part
    /// does not re-derive.
    PerfJudge {
        /// The record (`perf-result.json`).
        #[arg(long)]
        result: PathBuf,
    },
    /// Write the views of an Arazzo results document a vendor's CI shows:
    /// JUnit XML, CTRF 0.2.0 JSON and a Markdown summary, each a
    /// byte-deterministic function of the document.
    Export {
        /// The `results.json` the Arazzo evaluator wrote.
        #[arg(long)]
        results: PathBuf,
        /// Write the JUnit XML view here.
        #[arg(long)]
        junit: Option<PathBuf>,
        /// Write the CTRF JSON view here.
        #[arg(long)]
        ctrf: Option<PathBuf>,
        /// Write the Markdown summary here. Without this flag and without a
        /// job summary file, the summary is printed to standard output.
        #[arg(long)]
        markdown: Option<PathBuf>,
        /// The GitHub Actions job summary file the Markdown summary is
        /// appended to. GitHub sets the variable in every job step.
        #[arg(long, env = "GITHUB_STEP_SUMMARY", hide_env_values = true)]
        step_summary: Option<PathBuf>,
    },
    /// Show an Arazzo results document in the terminal: one line per chapter,
    /// band and option, each with its counts and a bar.
    ///
    /// A result reads in two parts. BASE comes first, chapter by chapter and
    /// band by band, in the order the catalogue declares. OPTIONS follows by
    /// the same chapters and bands, one line per option naming its selected
    /// arm and its kind (Recommended (SHOULD), Optional (MAY), or
    /// `@SpecDiverge` for an option that exists only because the released text
    /// is unclear), with the cases resting on TRIAL text in their band. A
    /// closing note counts the cases tagged `@SpecDiverge` and links the
    /// reports they are judged by. Every mark is a glyph as well
    /// as a colour: ✓ passed (green), ✕ failed (red), ? errored (yellow),
    /// ! warning and ○ skipped (grey). A warning row is a SHOULD the
    /// server did not follow and is never shown as a failure.
    ///
    /// Exit status: `0` when the view was printed; `2` when the document
    /// cannot be read.
    Show {
        /// The `results.json` the Arazzo evaluator wrote.
        results: PathBuf,
        /// Expand every failed and errored row with its four-part
        /// explanation, then list the warning rows apart.
        #[arg(long)]
        failed: bool,
        /// When to colour the view. `auto` colours when standard output is a
        /// terminal and `NO_COLOR` is unset or empty (<https://no-color.org/>).
        #[arg(long, value_enum, default_value_t = ColorArg::Auto)]
        color: ColorArg,
    },
    /// Verify a conformance certificate offline against a JWK Set, and print
    /// what it states.
    ///
    /// A certificate is a JWS in the compact serialization, signed with
    /// Ed25519 (`EdDSA`), whose header names its key by `kid`. It is refused
    /// when the header or the payload changed after signing, when the set
    /// holds no key under its `kid`, when its algorithm or type is another,
    /// when the set marks the key revoked, and when the key expired before
    /// the certificate's `issued_at`. Nothing is fetched: the set is the file
    /// you name.
    ///
    /// The payload carries the SHA-256 digests of the run's `results.json`
    /// and HAR recording, so `sha256sum` over the two files checks that they
    /// are the exchanges the certificate covers.
    ///
    /// Exit status: `0` when the certificate verifies; `1` when it is refused;
    /// `2` when a file cannot be read or the JWK Set itself is refused.
    Verify {
        /// The certificate (`certificate.jws`).
        certificate: PathBuf,
        /// The JWK Set holding the verifying keys: the published
        /// `jwks.json`, or a copy of it.
        #[arg(long)]
        jwks: PathBuf,
    },
}

/// When `show` colours its view.
#[derive(Clone, Copy, clap::ValueEnum)]
enum ColorArg {
    /// Colour on a terminal unless `NO_COLOR` is set.
    Auto,
    /// Always colour.
    Always,
    /// Never colour.
    Never,
}

impl From<ColorArg> for ColorChoice {
    fn from(arg: ColorArg) -> Self {
        match arg {
            ColorArg::Auto => Self::Auto,
            ColorArg::Always => Self::Always,
            ColorArg::Never => Self::Never,
        }
    }
}

/// The terminal view's options for `choice`, read against this process's
/// standard output and environment.
fn show_options(choice: ColorChoice, failed: bool) -> ShowOptions {
    let color = wants_color(
        choice,
        std::io::stdout().is_terminal(),
        std::env::var_os("NO_COLOR").as_deref(),
    );
    ShowOptions { color, failed }
}

/// Report a seam's failure and stop with the runner-error code.
fn fail<E: std::fmt::Display>(e: &E) -> ExitCode {
    eprintln!("{e}");
    ExitCode::from(2)
}

/// Write finished artifacts into `out`, one console line per file.
fn emit(out: &Path, files: &[RenderedFile]) -> Result<(), ExitCode> {
    if let Err(e) = std::fs::create_dir_all(out) {
        eprintln!("cannot create {}: {e}", out.display());
        return Err(ExitCode::from(2));
    }
    for file in files {
        let path = out.join(&file.name);
        if let Err(e) = std::fs::write(&path, &file.body) {
            eprintln!("cannot write {}: {e}", path.display());
            return Err(ExitCode::from(2));
        }
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// Write one finished document to `path`, creating its parent directory.
fn emit_one(path: &Path, body: &str) -> Result<(), ExitCode> {
    if let Err(e) = ensure_parent_dir(path) {
        eprintln!("{e}");
        return Err(ExitCode::from(2));
    }
    if let Err(e) = write_file(path, body) {
        eprintln!("{e}");
        return Err(ExitCode::from(2));
    }
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "the dispatch table: one arm per subcommand, each binding its own flags"
)]
fn main() -> ExitCode {
    match Cli::parse().command {
        Command::EmitSchemas { out } => emit_schemas_command(&out),
        Command::Test {
            root,
            base_url,
            platform_url,
            auth,
            admin_auth,
            statement,
            ixit,
            documents,
            out,
            rejudge,
        } => test_command(
            &ArazzoRunRequest {
                root: &root,
                statement: statement.as_deref(),
                ixit: ixit.as_deref(),
                documents: &documents,
            },
            &TestTarget {
                base_url: base_url.as_deref(),
                platform_url: platform_url.as_deref(),
                auth: &auth,
                admin_auth: &admin_auth,
                rejudge: rejudge.as_deref(),
            },
            out.as_deref(),
        ),
        Command::Identify { base_url, auth } => identify_command(&base_url, &auth),
        Command::Validate {
            root,
            specs,
            statement,
        } => validate_command(&root, specs.as_deref(), statement.as_deref()),
        Command::Export {
            results,
            junit,
            ctrf,
            markdown,
            step_summary,
        } => export_command(
            &results,
            &ExportTargets {
                junit: junit.as_deref(),
                ctrf: ctrf.as_deref(),
                markdown: markdown.as_deref(),
                step_summary: step_summary.as_deref(),
            },
        ),
        Command::Perf {
            root,
            ixit,
            workload,
            out,
            catalogue,
            statement,
            corpus,
            corpus_state,
            start_rate,
            scout_end_rate,
            host_prescription,
            results,
            sut_name,
            sut_version,
            class,
            seed_workers,
            hours,
        } => match (workload, out, class, results) {
            (Some(workload), Some(out), None, None) => {
                let corpus_state =
                    corpus_state.unwrap_or_else(|| out.with_extension("corpus.json"));
                perf_workload_command(&WorkloadRunRequest {
                    root: &root,
                    catalogue: &catalogue,
                    workload: &workload,
                    ixit: &ixit,
                    statement: statement.as_deref(),
                    out: &out,
                    corpus: match corpus {
                        CorpusArg::Seed => CorpusMode::Seed,
                        CorpusArg::Reuse => CorpusMode::Reuse,
                    },
                    corpus_state,
                    seed_workers,
                    start_rate,
                    scout_end_rate,
                    host_prescription,
                })
            }
            (None, _, Some(class), Some(results)) => perf_command(
                &root,
                &ixit,
                &results,
                OperatorIdentity {
                    name: sut_name.as_deref(),
                    version: sut_version.as_deref(),
                },
                &class,
                seed_workers,
                hours,
            ),
            _ => {
                eprintln!("perf takes either --workload with --out, or --class with --results");
                ExitCode::from(2)
            }
        },
        Command::Stress {
            root,
            ixit,
            out,
            corpus_class,
            seed_workers,
            step_secs,
            bisections,
            max_rate,
        } => stress_command(
            &StressRequest {
                root: &root,
                ixit: &ixit,
                corpus_class: &corpus_class,
                seed_workers,
                step_secs,
                bisections,
                max_rate,
            },
            &out,
        ),
        Command::Bench {
            base_url,
            auth,
            user,
            pack,
            posture,
            repetitions,
            scale,
            seed_workers,
            with_baselines,
            out,
            label,
        } => bench_command(
            &BenchInvocation {
                base_url: &base_url,
                auth_token: &auth,
                user: user.as_deref(),
                pack_token: &pack,
                posture_token: posture.as_deref(),
                repetitions,
                scale,
                seed_workers,
                with_baselines,
                label: label.as_deref(),
            },
            &out,
        ),
        Command::BenchCompare { results, out } => bench_compare_command(&results, &out),
        Command::BenchPacks { out } => bench_packs_command(&out),
        Command::PerfWorkloads {
            root,
            catalogue,
            out,
            workload,
        } => perf_workloads_command(&root, &catalogue, out.as_deref(), workload.as_deref()),
        Command::StressCompare {
            left,
            left_label,
            right,
            right_label,
            out,
        } => stress_compare_command(&left, &left_label, &right, &right_label, &out),
        Command::PerfAssets {
            root,
            results,
            out,
            summary,
            stress,
        } => perf_assets_command(&root, &results, &out, summary.as_deref(), stress.as_deref()),
        Command::Verify { certificate, jwks } => verify_command(&certificate, &jwks),
        Command::Show {
            results,
            failed,
            color,
        } => match show_results(&results, show_options(color.into(), failed)) {
            Ok(view) => {
                print!("{view}");
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        Command::Verdicts {
            statement,
            results,
            root,
            out,
        } => verdicts_command(
            &JudgementRequest {
                statement: &statement,
                results: &results,
                root: &root,
            },
            &out,
        ),
        Command::PerfJudge { result } => perf_judge_command(&result),
    }
}

fn perf_judge_command(path: &Path) -> ExitCode {
    let judgement = match judge_perf_result(path) {
        Ok(judgement) => judgement,
        Err(e) => return fail(&e),
    };
    let absent = || "absent".to_owned();
    for mismatch in &judgement.mismatches {
        println!(
            "{}: recorded {}, re-derived {}",
            mismatch.pointer,
            mismatch.recorded.clone().unwrap_or_else(absent),
            mismatch.rederived.clone().unwrap_or_else(absent),
        );
    }
    if judgement.reproduces() {
        println!(
            "perf-judge: {} reproduces byte for byte (recorded by veredictum {})",
            path.display(),
            judgement.recorded_version
        );
        return ExitCode::SUCCESS;
    }
    if !judgement.canonical {
        println!(
            "perf-judge: {} is not the canonical rendering of its re-derived record",
            path.display()
        );
    }
    println!(
        "perf-judge: {} does not reproduce: {} derived field(s) differ",
        path.display(),
        judgement.mismatches.len()
    );
    ExitCode::from(1)
}

fn emit_schemas_command(out: &Path) -> ExitCode {
    match emit(out, &schema_files()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn validate_command(root: &Path, specs: Option<&Path>, declaration: Option<&Path>) -> ExitCode {
    let validation = match veredictum::validate::validate(root, specs, declaration) {
        Ok(validation) => validation,
        Err(e) => return fail(&e),
    };
    for finding in &validation.findings {
        println!("{finding}");
    }
    println!("{}", validation.summary());
    if validation.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn verify_command(certificate: &Path, jwks: &Path) -> ExitCode {
    let read = |path: &Path| {
        std::fs::read_to_string(path).map_err(|e| {
            eprintln!("cannot read {}: {e}", path.display());
            ExitCode::from(2)
        })
    };
    let (certificate_text, jwks_text) = match (read(certificate), read(jwks)) {
        (Ok(certificate), Ok(jwks)) => (certificate, jwks),
        (Err(code), _) | (_, Err(code)) => return code,
    };
    let keys = match Jwks::parse(&jwks_text) {
        Ok(keys) => keys,
        Err(e) => {
            eprintln!("{}: {e}", jwks.display());
            return ExitCode::from(2);
        }
    };
    let verified = match verify(&certificate_text, &keys) {
        Ok(verified) => verified,
        Err(e) => {
            eprintln!("certificate REFUSED: {e}");
            return ExitCode::from(1);
        }
    };
    let payload = &verified.payload;
    let source = |field: &veredictum::party::IdentityField| {
        format!("{} ({})", field.value, field.source.token())
    };
    let verdict = &payload.verdict;
    println!("certificate  verified with key {}", verified.kid);
    println!("issued at    {}", payload.issued_at);
    println!(
        "instrument   {} {} ({})",
        payload.instrument.name, payload.instrument.version, payload.instrument.status
    );
    println!("server       {}", source(&payload.sut.name));
    println!("version      {}", source(&payload.sut.version));
    if let Some(vendor) = &payload.sut.vendor {
        println!("vendor       {}", source(vendor));
    }
    if let Some(release) = &payload.sut.restapi_specs_version {
        println!("REST release {}", source(release));
    }
    println!("base URL     {}", payload.base_url);
    println!("document     {}", payload.document);
    println!("catalogue    {}", payload.catalogue.digest);
    println!(
        "verdict      {}: {} passed, {} failed, {} warning, {} errored, {} skipped",
        match verdict.outcome {
            Outcome::Green => "green",
            Outcome::Red => "red",
        },
        verdict.passed,
        verdict.failed,
        verdict.warning,
        verdict.errored,
        verdict.skipped
    );
    println!("results      {}", payload.exchanges.results);
    println!("recording    {}", payload.exchanges.recording);
    println!("{HONESTY_LINE}");
    ExitCode::SUCCESS
}

fn verdicts_command(request: &JudgementRequest<'_>, out: &Path) -> ExitCode {
    let judgement = match judge(request) {
        Ok(judgement) => judgement,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit(out, &judgement.documents) {
        return code;
    }
    for finding in &judgement.report.review {
        println!("static-review: {}", finding.message);
    }
    println!(
        "{} performance verdict(s), {} review finding(s)",
        judgement.report.performance.len(),
        judgement.report.review.len(),
    );
    if judgement.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn perf_assets_command(
    root: &Path,
    results: &Path,
    out: &Path,
    summary: Option<&Path>,
    stress: Option<&Path>,
) -> ExitCode {
    let assets = match performance_assets(root, results, stress) {
        Ok(assets) => assets,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit(out, &assets.files) {
        return code;
    }
    if let Some(path) = summary {
        let body = match assets.summary_markdown() {
            Ok(body) => body,
            Err(e) => return fail(&e),
        };
        if let Err(code) = emit_one(path, &body) {
            return code;
        }
        println!("wrote {}", path.display());
    }
    ExitCode::SUCCESS
}

fn stress_compare_command(
    left: &Path,
    left_label: &str,
    right: &Path,
    right_label: &str,
    out: &Path,
) -> ExitCode {
    let svg = match stress_overlay((left_label, left), (right_label, right)) {
        Ok(svg) => svg,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit_one(out, &svg) {
        return code;
    }
    println!("wrote {}", out.display());
    ExitCode::SUCCESS
}

fn stress_command(request: &StressRequest<'_>, out: &Path) -> ExitCode {
    let progress = |message: String| eprintln!("[stress] {message}");
    let report = match run_stress(request, &progress) {
        Ok(report) => report,
        Err(e) => return fail(&e),
    };
    let document = match to_json_document(&report, "serialize") {
        Ok(document) => document,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit_one(out, &document) {
        return code;
    }
    println!("{}", report.remark);
    println!(
        "wrote {} ({} steps, max sustainable {:.1}/s)",
        out.display(),
        report.steps.len(),
        report.max_sustainable_throughput_per_s
    );
    ExitCode::SUCCESS
}

/// What one `bench` invocation asked for, so the command takes one argument
/// rather than eight positional ones that a swap would silently reorder.
struct BenchInvocation<'a> {
    base_url: &'a str,
    auth_token: &'a str,
    user: Option<&'a str>,
    pack_token: &'a str,
    posture_token: Option<&'a str>,
    repetitions: u32,
    scale: f64,
    seed_workers: Option<usize>,
    with_baselines: bool,
    label: Option<&'a str>,
}

/// Prints the server's identity document on stdout.
/// What `test` drives: a server, or a recording.
struct TestTarget<'a> {
    /// The server's base URL, for a live run.
    base_url: Option<&'a str>,
    /// The SMART Platform base URL, for a live run.
    platform_url: Option<&'a str>,
    /// The `--auth` token.
    auth: &'a str,
    /// The `--admin-auth` token.
    admin_auth: &'a str,
    /// The recording, for a re-judgement.
    rejudge: Option<&'a Path>,
}

/// Drives the Arazzo catalogue or re-judges its recording, prints every row
/// and writes the run's files; the exit status is the run's.
fn test_command(
    request: &ArazzoRunRequest<'_>,
    target: &TestTarget<'_>,
    out: Option<&Path>,
) -> ExitCode {
    let drive = match (target.rejudge, target.base_url) {
        (Some(recording), _) => Drive::Rejudge { recording },
        (None, Some(base_url)) => {
            // Only where each credential comes from is resolved here; its
            // value is read inside the transport that sends it.
            let (auth, admin_auth) = match (
                AuthKind::parse(target.auth),
                AuthKind::parse(target.admin_auth),
            ) {
                (Ok(auth), Ok(admin_auth)) => (auth, admin_auth),
                (Err(e), _) | (_, Err(e)) => return fail(&e),
            };
            let located = CredentialSource::locate(auth, Role::Ordinary).and_then(|ordinary| {
                CredentialSource::locate(admin_auth, Role::Administrative)
                    .map(|administrative| (ordinary, administrative))
            });
            let (ordinary, administrative) = match located {
                Ok(located) => located,
                Err(e) => return fail(&e),
            };
            Drive::Live {
                base_url,
                ordinary,
                administrative,
                platform_url: target.platform_url,
            }
        }
        (None, None) => {
            eprintln!("either --base-url or --rejudge is required");
            return ExitCode::from(2);
        }
    };
    let outcome = match run_catalogue(request, drive) {
        Ok(outcome) => outcome,
        Err(e) => return fail(&e),
    };
    for row in &outcome.results.rows {
        println!("{row}");
        if row.explanation.is_some() || row.skip.is_some() {
            println!();
        }
    }
    let tally = outcome.tally();
    println!(
        "{} row(s): {} passed, {} failed, {} warning, {} errored, {} skipped",
        tally.tests, tally.passed, tally.failed, tally.warning, tally.errored, tally.skipped
    );
    for note in &outcome.results.review {
        println!("{note}");
    }
    println!();
    print!(
        "{}",
        render(&outcome.results, show_options(ColorChoice::Auto, false))
    );
    if let Some(out) = out {
        let written = outcome.files().map_err(|e| fail(&e));
        if let Err(code) = written.and_then(|files| emit(out, &files)) {
            return code;
        }
    }
    let red = outcome.red_rows().count();
    if red == 0 {
        ExitCode::SUCCESS
    } else {
        eprintln!("{red} row(s) failed or errored");
        ExitCode::from(1)
    }
}

fn identify_command(base_url: &str, auth: &str) -> ExitCode {
    let auth = match AuthKind::parse(auth) {
        Ok(auth) => auth,
        Err(e) => return fail(&e),
    };
    let read = match veredictum::identity::identify(base_url, auth) {
        Ok(read) => read,
        Err(e) => return fail(&e),
    };
    match to_json_document(&read, "serialize the identity document") {
        Ok(document) => {
            print!("{document}");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn bench_command(invocation: &BenchInvocation<'_>, out: &Path) -> ExitCode {
    let auth = match AuthKind::parse(invocation.auth_token) {
        Ok(auth) => auth,
        Err(e) => return fail(&e),
    };
    let pack = match veredictum::bench::pack::load(invocation.pack_token) {
        Ok(pack) => pack,
        Err(e) => return fail(&e),
    };
    let profile = match pack.resolve_profile(invocation.posture_token) {
        Ok(profile) => profile,
        Err(e) => return fail(&e),
    };
    let progress = |message: String| eprintln!("[bench] {message}");
    let outcome = match run_bench(
        &BenchRequest {
            pack: &pack,
            profile,
            base_url: invocation.base_url,
            auth,
            user: invocation.user,
            repetitions: invocation.repetitions,
            label: invocation.label,
            scale: invocation.scale,
            seed_workers: invocation.seed_workers,
            with_baselines: invocation.with_baselines,
            docker: None,
        },
        &progress,
    ) {
        Ok(outcome) => outcome,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit(out, &outcome.documents) {
        return code;
    }
    println!("{}", veredictum::bench::BOUNDARY_STATEMENT);
    println!(
        "machine: {}",
        veredictum::bench::render::machine_line(&outcome.result.environment)
    );
    println!(
        "{} repetition(s) over pack {}@{}; submittable: {}",
        outcome.result.repetitions.len(),
        outcome.result.pack.id,
        outcome.result.pack.version,
        outcome.result.submittable
    );
    println!("posture `{}`:", outcome.result.posture.profile);
    for line in &outcome.result.posture.items {
        println!("  {} = {} ({})", line.item, line.declared, line.assurance);
    }
    for requirement in &outcome.result.submittable_unmet {
        println!(
            "not submittable, unmet `{requirement}`: {}",
            requirement.statement()
        );
    }
    for index in &outcome.result.relative {
        println!(
            "vs {}: {} indexed operation(s), {} gap(s)",
            index.display_name,
            index
                .phases
                .values()
                .map(|phase| phase.operations.len())
                .sum::<usize>(),
            index.gaps.len()
        );
    }
    if !outcome.result.scale.reference_configuration {
        println!(
            "scale factor {:.3}: this run is off the pack's pinned configuration, so its numbers are not comparable with the reference figures the pack describes",
            outcome.result.scale.factor
        );
    }
    ExitCode::SUCCESS
}

fn bench_compare_command(results: &[PathBuf], out: &Path) -> ExitCode {
    let outcome = match compare_bench(results) {
        Ok(outcome) => outcome,
        Err(e) => return fail(&e),
    };
    println!("{}", outcome.document.body);
    if let Err(code) = emit(out, std::slice::from_ref(&outcome.document)) {
        return code;
    }
    if outcome.comparison.warnings.is_empty() {
        ExitCode::SUCCESS
    } else {
        for warning in &outcome.comparison.warnings {
            eprintln!("bench-compare: {warning}");
        }
        ExitCode::from(1)
    }
}

fn bench_packs_command(out: &Path) -> ExitCode {
    let outcome = match describe_packs() {
        Ok(outcome) => outcome,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit(out, std::slice::from_ref(&outcome.document)) {
        return code;
    }
    for pack in &outcome.manifest.packs {
        println!(
            "{}@{}: {} phase(s), {} fixture(s)",
            pack.id,
            pack.version,
            pack.phases.len(),
            pack.fixtures.len()
        );
    }
    ExitCode::SUCCESS
}

fn perf_workloads_command(
    root: &Path,
    catalogue: &Path,
    out: Option<&Path>,
    workload: Option<&Path>,
) -> ExitCode {
    use veredictum::workload::load::{LoadContext, load_dir, load_file};
    use veredictum::workload::manifest::{
        MANIFEST_FILE, PAGE_FILE, describe_pinned, render_json, render_page, summary,
    };
    let ctx = match LoadContext::from_roots(root, catalogue) {
        Ok(ctx) => ctx,
        Err(e) => return fail(&e),
    };
    if let Some(path) = workload {
        return match load_file(path, &ctx) {
            Ok(loaded) => {
                print!("{}", summary(&loaded));
                ExitCode::SUCCESS
            }
            Err(refused) => {
                eprintln!("{refused}");
                ExitCode::from(1)
            }
        };
    }
    match load_dir(root, &ctx) {
        Ok((loaded, refused)) => {
            for workload in &loaded {
                print!("{}", summary(workload));
            }
            if !refused.is_empty() {
                for r in &refused {
                    eprintln!("{r}");
                }
                return ExitCode::from(1);
            }
        }
        Err(e) => return fail(&e),
    }
    let Some(out) = out else {
        return ExitCode::SUCCESS;
    };
    let manifest = match describe_pinned(root, &ctx) {
        Ok(manifest) => manifest,
        Err(e) => return fail(&e),
    };
    let json = match render_json(&manifest) {
        Ok(json) => json,
        Err(e) => return fail(&e),
    };
    let files = [
        RenderedFile {
            name: MANIFEST_FILE.to_owned(),
            body: json,
        },
        RenderedFile {
            name: PAGE_FILE.to_owned(),
            body: render_page(&manifest),
        },
    ];
    if let Err(code) = emit(out, &files) {
        return code;
    }
    ExitCode::SUCCESS
}

/// How `perf --workload` gets its corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum CorpusArg {
    /// Seed the workload's recipe before the clock.
    Seed,
    /// Reuse the stored corpus when its read-back counts and digest match.
    Reuse,
}

fn perf_workload_command(request: &WorkloadRunRequest<'_>) -> ExitCode {
    let progress = |message: String| eprintln!("[perf] {message}");
    match run_workload(request, &progress) {
        Ok(record) => {
            for (plateau, raw) in record.derived.plateaus.iter().zip(&record.raw.plateaus) {
                println!(
                    "  plateau {:.1}/s ({}): achieved {:.1}/s, p99 {}, errors {:.4}",
                    plateau.planned_rate,
                    raw.multiple
                        .map_or_else(String::new, |m| format!("{m} x K")),
                    plateau.throughput.achieved_per_s,
                    plateau
                        .latency
                        .p99
                        .value_ms()
                        .map_or_else(|| "not resolvable".to_owned(), |v| format!("{v:.1} ms")),
                    plateau.throughput.error_share.unwrap_or(0.0)
                );
            }
            println!("wrote {}", request.out.display());
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn perf_command(
    root: &Path,
    ixit: &Path,
    results: &Path,
    sut: OperatorIdentity<'_>,
    class_token: &str,
    seed_workers: usize,
    hours: u64,
) -> ExitCode {
    let Some(window) = SustainedWindow::hours(hours) else {
        eprintln!("--hours must be one of 1 | 2 | 4 | 6 | 8 | 12 (got {hours})");
        return ExitCode::from(2);
    };
    let request = MeasuredRequest {
        root,
        ixit,
        results,
        sut,
        class: class_token,
        seed_workers,
        window,
    };
    let observe = |event: MeasuredEvent<'_>| match event {
        MeasuredEvent::Progress(message) => eprintln!("[perf] {message}"),
        MeasuredEvent::CaseStarted { case, source } => println!(
            "case {} (class {class_token}) from {}",
            case.id,
            source.display()
        ),
        MeasuredEvent::Measured(measurement) => {
            for op in &measurement.operations {
                println!(
                    "  {}: {} requests, {} errors, p50 {:.1}ms p90 {:.1}ms p99 {:.1}ms",
                    op.operation,
                    op.requests,
                    op.errors,
                    op.latency_ms_p50,
                    op.latency_ms_p90,
                    op.latency_ms_p99
                );
            }
            println!("  {}", veredictum::perf::verdict_evidence(measurement));
        }
        MeasuredEvent::PrunedOrphan(case) => {
            println!("  pruned orphaned measurement for retired case {case}");
        }
        MeasuredEvent::Merged(path) => {
            println!("  measurement merged into {}", path.display());
        }
        MeasuredEvent::Created(path) => {
            println!(
                "  measurement written into the new record {}",
                path.display()
            );
        }
    };
    match run_measured(&request, &observe) {
        Ok(run) if run.earned_all => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(1),
        Err(e) => fail(&e),
    }
}

/// Where `export` writes each view; a view with no target is not written.
struct ExportTargets<'a> {
    /// The JUnit XML file.
    junit: Option<&'a Path>,
    /// The CTRF JSON file.
    ctrf: Option<&'a Path>,
    /// The Markdown summary file.
    markdown: Option<&'a Path>,
    /// The GitHub Actions job summary file the Markdown summary is appended
    /// to.
    step_summary: Option<&'a Path>,
}

/// Writes the views of an Arazzo results document.
///
/// Every console line goes to standard error, because standard output
/// carries the Markdown summary when no file receives it. The exit code is
/// `0` when every view was written and `2` when the document could not be
/// read or a file could not be written.
fn export_command(results: &Path, targets: &ExportTargets<'_>) -> ExitCode {
    let views = match export_results(results) {
        Ok(views) => views,
        Err(e) => return fail(&e),
    };
    for (path, body) in [
        (targets.junit, &views.junit),
        (targets.ctrf, &views.ctrf),
        (targets.markdown, &views.markdown),
    ] {
        let Some(path) = path else { continue };
        if let Err(code) = emit_one(path, body) {
            return code;
        }
        eprintln!("wrote {}", path.display());
    }
    if let Some(path) = targets.step_summary {
        // NOTE: GitHub's workflow-commands reference, "Adding a job summary", has every step
        // append to `$GITHUB_STEP_SUMMARY`, so the summary is appended and never truncates it.
        let appended = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, views.markdown.as_bytes()));
        if let Err(e) = appended {
            eprintln!("cannot append to {}: {e}", path.display());
            return ExitCode::from(2);
        }
        eprintln!("appended the summary to {}", path.display());
    }
    if targets.markdown.is_none() && targets.step_summary.is_none() {
        print!("{}", views.markdown);
    }
    ExitCode::SUCCESS
}
