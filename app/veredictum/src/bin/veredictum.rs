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

use veredictum::arazzo::credential::AuthKind;
use veredictum::arazzo::credential::{CredentialSource, Role};
use veredictum::arazzo::spec_profile::SpecProfile;
use veredictum::arazzo::terminal::{ColorChoice, ShowOptions, render, wants_color};
use veredictum::certificate::{HONESTY_LINE, Jwks, Outcome, verify};
use veredictum::party::version_line;
use veredictum::perf_emulate::posture::PostureProfile;
use veredictum::perf_result::compare::DEFAULT_TOLERANCE;
use veredictum::perf_result::host::NetworkLeg;
use veredictum::pipeline::arazzo_run::{ArazzoRunRequest, Drive, run_catalogue};
use veredictum::pipeline::assets::{perf_result_assets, schema_files};
use veredictum::pipeline::export::{Exports, badge_files, export_document, show_results};
use veredictum::pipeline::perf_compare::{
    InterleavedRequest, align_records, run_compare, run_with_baselines, write_comparison,
};
use veredictum::pipeline::perf_judge::judge_perf_result;
use veredictum::pipeline::perf_workload::{
    CorpusMode, WorkloadRunRequest, WorkloadSettings, run_workload,
};
use veredictum::pipeline::{RenderedFile, ensure_parent_dir, to_json_document, write_file};
use veredictum::validate::profile_report;

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
    /// Exit status follows whose problem each red row's record names. `1`
    /// when any row counts against the server: it failed (a warning row
    /// excepted) or errored with the server named, whatever else the run
    /// holds. `3` when no row counts against the server and at least one is
    /// inconclusive: it failed or errored with the instrument named, because
    /// the instrument could not judge it. `0` when every row passed or was
    /// skipped, or failed only as a warning (a SHOULD, which never counts
    /// against a capability). `2` when the run could not start or finish: a
    /// usage error, a gate finding, an unreadable file, a missing credential,
    /// or a recording that does not replay. The line before the exit names
    /// both counts. `--rejudge` exits the same way.
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
        /// The specification text the case citations are read against.
        ///
        /// `development` (the default): the pinned `master` of each openEHR
        /// specification repository under `specs/openehr-development/`, the
        /// line that carries the newest text. A case citing text the snapshot lacks is skipped as
        /// `cited-text-absent`, naming what is missing. The results name the
        /// profile and every snapshot commit, and a development result never
        /// builds a certificate and is never compared with or counted toward
        /// a released result.
        ///
        /// `released`: the frozen release under `specs/openehr/`. Only a
        /// released result backs a released claim or a certificate.
        ///
        /// The catalogue is gated against the release under either profile.
        /// A re-judgement takes the profile the recorded run was given.
        #[arg(long, value_enum, default_value_t = ProfileArg::Development)]
        profile: ProfileArg,
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
        /// The specification profile to report the case citations against,
        /// beside the gates.
        ///
        /// `released` (the default): the gates alone, against the frozen
        /// release. Zero findings is the only passing result.
        ///
        /// `development`: the gates as under `released`, then every case
        /// citation read against the snapshot under
        /// `specs/openehr-development/` beside `--specs`. A citation whose
        /// file, section or quote the snapshot lacks is printed as `[info]`
        /// and counted per component; it is never a finding. A citation
        /// refused for any other reason is a `profile-citation` finding.
        /// Needs `--specs`.
        #[arg(long, value_enum, default_value_t = ProfileArg::Released)]
        profile: ProfileArg,
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
    /// Run a workload document against a live server and write one
    /// `perf-result`.
    ///
    /// The corpus is seeded (or reused with `--corpus reuse`) before the
    /// clock, closed-loop and reported apart. The posture canaries then take
    /// their first bracket, and the run after seeding takes at most ten
    /// minutes: a 15 s settle, a 20 s warm-up, a 60 s scout ramp judged in
    /// 2 s windows, a drain, a ladder of four plateaus at 0.5, 0.75, 0.9 and
    /// 1.05 times the knee estimate (a fifth at 0.25 only when the lowest
    /// fails), and the closing canaries. The planned duration is printed
    /// before the first measured request. A server that answers any exchange
    /// `429` gets no record, and a canary that contradicts the declared
    /// posture refuses the run.
    Perf {
        /// The performance root, `perf/` in this repository (workloads/,
        /// payloads.yaml, payloads/, hosts/).
        #[arg(long)]
        root: PathBuf,
        /// The ixit topology file (JSON); its environment block is mandatory
        /// for a measured run.
        #[arg(long)]
        ixit: PathBuf,
        /// The workload document to run, an embedded one under
        /// `perf/workloads/` or the evaluator's own (labelled `custom`).
        #[arg(long)]
        workload: PathBuf,
        /// Where the `perf-result` record is written.
        #[arg(long)]
        out: PathBuf,
        /// The catalogue root a workload's `requires` resolve against.
        #[arg(long, default_value = "catalogue")]
        catalogue: PathBuf,
        /// A statement whose claims keep the interactions it covers and
        /// drop the rest, with the drop recorded.
        #[arg(long)]
        statement: Option<PathBuf>,
        /// Seed the workload's corpus, or reuse the stored one: a reuse is
        /// refused before the warm-up when the server's read-back counts or
        /// the stored digest differ.
        #[arg(long, value_enum, default_value_t = CorpusArg::Seed)]
        corpus: CorpusArg,
        /// The stored corpus state a seeding writes and a reuse reads;
        /// defaults to the record's path with the extension `corpus.json`.
        #[arg(long)]
        corpus_state: Option<PathBuf>,
        /// Parallel closed-loop seeding workers.
        #[arg(long, default_value_t = 16)]
        seed_workers: usize,
        /// The warm-up's rate and the scout's starting rate, operations per
        /// second.
        #[arg(long, default_value_t = veredictum::perf_emulate::run::DEFAULT_START_RATE)]
        start_rate: f64,
        /// The scout ramp's end rate, operations per second.
        #[arg(long, default_value_t = veredictum::perf_emulate::run::DEFAULT_SCOUT_END_RATE)]
        scout_end_rate: f64,
        /// The id of the host prescription under `perf/hosts/` the run
        /// claims to have run on; the record reads the generator's measured
        /// fingerprint against it.
        #[arg(long)]
        host_prescription: Option<String>,
        /// Where the load generator sits relative to the server.
        #[arg(long, value_enum, default_value_t = NetworkLegArg::SameHost)]
        network_leg: NetworkLegArg,
        /// The posture profile the run declares: `minimal`,
        /// `minimal-signed-digest`, `minimal-signed-pgp` or `clinical-default`.
        /// The canaries check it after seeding and after the last plateau,
        /// and a contradiction refuses the run.
        #[arg(long, default_value = "minimal")]
        posture: String,
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
    /// Compare performance records: align committed `perf-result` files, or
    /// run two targets (`--ab`) or one target against itself (`--aa`)
    /// interleaved on one host.
    ///
    /// With `--result`, two or more records are read into one table, one
    /// column per file. Every field on which they describe different things
    /// is named above it: the workload digest, the corpus recipe, the
    /// posture, the host prescription, the generator's fingerprint and the
    /// network leg. Two records also get a ratio per plateau, operation and
    /// metric, with both values beside it; plateaus offered different rates
    /// are shown and not judged.
    ///
    /// With `--ab`, both targets are seeded from the same recipe and seed and
    /// stay resident on one host. Each gets its settle, warm-up and scout; the
    /// shared ladder is built from the larger knee estimate, so both sides are
    /// offered the same rates and holds; and on each rung the plateaus
    /// alternate in ABBA order. `--aa` runs the same interleaving with one
    /// target on both sides and attaches its floor, the largest A/A ratio per
    /// metric, to both records. A difference counts only when its ratio
    /// exceeds the tolerance and the attached floor and the two sides' 95 %
    /// intervals do not overlap. Both write `a.perf-result.json`,
    /// `b.perf-result.json`, `perf-compare.json` and `perf-compare.md` under
    /// `--out`, in about twice a single run.
    ///
    /// Exit status: `0` when the records align; `1` when any mismatch applies;
    /// `2` when a record cannot be read or a run cannot finish.
    PerfCompare {
        /// A committed `perf-result`. Repeat the flag once per file; at least
        /// two are needed.
        #[arg(long = "result", conflicts_with_all = ["ab", "aa", "with_baselines"], required_unless_present_any = ["ab", "aa", "with_baselines"])]
        results: Vec<PathBuf>,
        /// Side A's and side B's IXIT, for an interleaved A/B run.
        #[arg(long, num_args = 2, value_names = ["IXIT_A", "IXIT_B"], conflicts_with = "aa")]
        ab: Option<Vec<PathBuf>>,
        /// The IXIT of the one target an A/A run puts on both sides.
        #[arg(long, value_name = "IXIT", conflicts_with = "with_baselines")]
        aa: Option<PathBuf>,
        /// The IXIT of a target to run against every pinned reference CDR
        /// (EHRbase and FerroEHR), each composed on this host from its own
        /// digest-pinned images with the docker CLI and torn down with its
        /// volumes afterwards. Each comparison, the target's index relative to
        /// that reference, is written under `--out/<reference>/`.
        #[arg(long, value_name = "IXIT", conflicts_with = "ab")]
        with_baselines: Option<PathBuf>,
        #[command(flatten)]
        run: CompareRunArgs,
        /// The declared tolerance: a ratio within it either way is never a
        /// difference.
        #[arg(long, default_value_t = DEFAULT_TOLERANCE)]
        tolerance: f64,
        /// A `perf-result` or `perf-compare.json` carrying an A/A floor, which
        /// the ratios are judged against and an A/B run attaches to both
        /// records.
        #[arg(long)]
        floor: Option<PathBuf>,
        /// Output directory: the comparison for `--result`, and the records,
        /// corpora and comparison for `--ab` and `--aa`.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Render the charts of a `perf-result` as SVG, deterministically: the
    /// same record always draws the same bytes.
    ///
    /// Three charts: `perf-saturation.svg` (p50, p95 and p99 against achieved
    /// throughput over the scout windows and the plateaus, the knee bracket
    /// shaded, the Universal Scalability Law fit dashed), `perf-throughput.svg`
    /// (achieved against offered with the identity line) and
    /// `perf-operations.svg` (each operation's latency at the knee at its
    /// judged quantile, beside its budget). A quantile that does not resolve
    /// is marked below the plot and never joined to its neighbours.
    PerfAssets {
        /// The committed `perf-result.json`.
        #[arg(long)]
        results: PathBuf,
        /// Output directory for the SVG files.
        #[arg(long)]
        out: PathBuf,
        /// Also write the `export --markdown` view of the record to this
        /// path, the book's build-time include.
        #[arg(long)]
        summary: Option<PathBuf>,
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
    /// Write the views of a result document a vendor's CI shows, each a
    /// byte-deterministic function of the document.
    ///
    /// An Arazzo results document has three views: JUnit XML, CTRF 0.2.0
    /// JSON and a Markdown summary. A `perf-result`, told apart by its
    /// `family`, has the Markdown summary only, so `--junit` and `--ctrf`
    /// refuse it.
    ///
    /// A result judged under the development profile names the profile and
    /// every snapshot commit in each view: a `profile` property on every
    /// JUnit suite, a `specification` member in the CTRF environment, and a
    /// `Specification` row with a warning in the Markdown summary.
    Export {
        /// The `results.json` the Arazzo evaluator wrote, or a
        /// `perf-result.json`.
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
        /// Write the GitHub Actions annotations here: one `::error` workflow
        /// command per failed or errored row and one `::warning` per warning
        /// row, titled with the case id and carrying what was checked, the
        /// cited sentence and what was expected and received. GitHub shows
        /// at most 10 of each kind per step, so at most 10 of each are
        /// written and a closing `::notice` names how many more rows the
        /// summary lists. An Arazzo results document only.
        #[arg(long)]
        annotations: Option<PathBuf>,
    },
    /// Draw the static badges of a result document, for a README.
    ///
    /// Each badge is written as `<name>.svg`, a flat badge drawn without a
    /// font or the network, and as `<name>.json`, a Shields.io endpoint
    /// document (<https://shields.io/badges/endpoint-badge>), and each is a
    /// byte-deterministic function of the document. Every badge reports the
    /// vendor's own run, and none is a certificate.
    ///
    /// An Arazzo results document draws five: `base` (passed out of every
    /// BASE row), `options` (passed out of every row of the selected option
    /// arms and of the cases resting on TRIAL text), `warnings` (the warning
    /// rows, a SHOULD the server did not follow), `profile` (the
    /// specification profile) and `instrument` (the version, and `self-run`).
    /// Each counting badge names the profile in its label, so a development
    /// tally never reads as a released one. A counting badge is green only
    /// when no row failed or errored: red when a row failed, orange when none
    /// failed but one errored, grey when nothing passed.
    ///
    /// A `perf-result` draws one, `perf`: the workload in the label and the
    /// knee as the record states it, in a neutral colour, because a knee is a
    /// reading and never a pass or a failure.
    ///
    /// Exit status: `0` when every badge was written; `2` when the document
    /// cannot be read or a file cannot be written.
    Badge {
        /// The `results.json` the Arazzo evaluator wrote, or a
        /// `perf-result.json`.
        #[arg(long)]
        results: PathBuf,
        /// Output directory for the badges (created if missing).
        #[arg(long)]
        out: PathBuf,
    },
    /// Show a result document in the terminal: an Arazzo results document
    /// one line per chapter, band and option, each with its counts and a
    /// bar, or a `perf-result` one line per plateau.
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
    /// server did not follow and is never shown as a failure. A result judged
    /// under the development profile opens with a line naming the profile
    /// and every snapshot commit, and saying it is never a released result.
    ///
    /// A `perf-result` reads as a header naming the target, environment,
    /// host, workload (a `custom` one named so), corpus, the knee estimate,
    /// the between-run noise and the record's scope; one line per plateau
    /// with offered and achieved rates, p50 to max, the error share and a ✓
    /// or ✕ qualification mark, with `unsteady` where recorded; the
    /// operations at the knee and at the lowest plateau with the quantile
    /// each is judged at (`n/r` where a quantile does not resolve); the
    /// knee, peak, population and fit sentences; a sparkline of p99 against
    /// the offered rate; and the server-side readings as context. It fits 80
    /// columns. `--failed` does not apply to it.
    ///
    /// Exit status: `0` when the view was printed; `2` when the document
    /// cannot be read.
    Show {
        /// The `results.json` the Arazzo evaluator wrote, or a
        /// `perf-result.json`.
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

/// The specification profile a run or a validation reads.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum ProfileArg {
    /// The pinned `master` of each openEHR repository: the newest text.
    Development,
    /// The frozen release: the oracle of every released claim.
    Released,
}

impl From<ProfileArg> for SpecProfile {
    fn from(arg: ProfileArg) -> Self {
        match arg {
            ProfileArg::Released => Self::Released,
            ProfileArg::Development => Self::Development,
        }
    }
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
            profile,
        } => test_command(
            &ArazzoRunRequest {
                root: &root,
                statement: statement.as_deref(),
                ixit: ixit.as_deref(),
                documents: &documents,
                profile: profile.into(),
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
            profile,
        } => validate_command(
            &root,
            specs.as_deref(),
            statement.as_deref(),
            profile.into(),
        ),
        Command::Export {
            results,
            junit,
            ctrf,
            markdown,
            step_summary,
            annotations,
        } => export_command(
            &results,
            &ExportTargets {
                junit: junit.as_deref(),
                ctrf: ctrf.as_deref(),
                markdown: markdown.as_deref(),
                step_summary: step_summary.as_deref(),
                annotations: annotations.as_deref(),
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
            seed_workers,
            start_rate,
            scout_end_rate,
            host_prescription,
            network_leg,
            posture,
        } => {
            let posture = match PostureProfile::by_name(&posture) {
                Ok(profile) => profile,
                Err(e) => return fail(&e),
            };
            let corpus_state = corpus_state.unwrap_or_else(|| out.with_extension("corpus.json"));
            perf_workload_command(&WorkloadRunRequest {
                settings: WorkloadSettings {
                    root: &root,
                    catalogue: &catalogue,
                    workload: &workload,
                    statement: statement.as_deref(),
                    corpus: corpus.into(),
                    seed_workers,
                    start_rate,
                    scout_end_rate,
                    host_prescription,
                    network_leg: network_leg.into(),
                    posture,
                },
                ixit: &ixit,
                out: &out,
                corpus_state,
            })
        }
        Command::PerfCompare {
            results,
            ab,
            aa,
            with_baselines,
            run,
            tolerance,
            floor,
            out,
        } => {
            let invocation = match (ab.as_deref(), aa.as_deref(), with_baselines.as_deref()) {
                (Some([a, b]), None, None) => CompareInvocation::Interleaved { a, b: Some(b) },
                (None, Some(a), None) => CompareInvocation::Interleaved { a, b: None },
                (None, None, Some(a)) => CompareInvocation::Baselines { a },
                _ => CompareInvocation::Records(&results),
            };
            perf_compare_command(
                &invocation,
                &run,
                tolerance,
                floor.as_deref(),
                out.as_deref(),
            )
        }
        Command::PerfWorkloads {
            root,
            catalogue,
            out,
            workload,
        } => perf_workloads_command(&root, &catalogue, out.as_deref(), workload.as_deref()),
        Command::PerfAssets {
            results,
            out,
            summary,
        } => perf_assets_command(&results, &out, summary.as_deref()),
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
        Command::PerfJudge { result } => perf_judge_command(&result),
        Command::Badge { results, out } => match badge_files(&results) {
            Ok(files) => match emit(&out, &files) {
                Ok(()) => ExitCode::SUCCESS,
                Err(code) => code,
            },
            Err(e) => fail(&e),
        },
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
    let files = match schema_files() {
        Ok(files) => files,
        Err(e) => return fail(&e),
    };
    match emit(out, &files) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn validate_command(
    root: &Path,
    specs: Option<&Path>,
    declaration: Option<&Path>,
    profile: SpecProfile,
) -> ExitCode {
    let validation = match veredictum::validate::validate(root, specs, declaration) {
        Ok(validation) => validation,
        Err(e) => return fail(&e),
    };
    for finding in &validation.findings {
        println!("{finding}");
    }
    println!("{}", validation.summary());
    let mut clean = validation.is_clean();
    if profile != SpecProfile::Released {
        let Some(specs) = specs else {
            eprintln!("--profile {profile} needs --specs, beside which the snapshot sits");
            return ExitCode::from(2);
        };
        let report = match profile_report(root, &specs.join("../.."), profile) {
            Ok(report) => report,
            Err(e) => return fail(&e),
        };
        for absent in &report.absent {
            println!("{absent}");
        }
        for finding in &report.findings {
            println!("{finding}");
        }
        for line in report.summary() {
            println!("{line}");
        }
        clean &= report.findings.is_empty();
    }
    if clean {
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

/// Renders the charts and summary of a `perf-result`.
fn perf_assets_command(results: &Path, out: &Path, summary: Option<&Path>) -> ExitCode {
    let assets = match perf_result_assets(results) {
        Ok(assets) => assets,
        Err(e) => return fail(&e),
    };
    if let Err(code) = emit(out, &assets.files) {
        return code;
    }
    if let Some(path) = summary {
        if let Err(code) = emit_one(path, &assets.summary) {
            return code;
        }
        println!("wrote {}", path.display());
    }
    ExitCode::SUCCESS
}

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
    if !outcome.results.specification.is_released() {
        println!(
            "Profile  {}: never a released result",
            outcome.results.specification.label()
        );
    }
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
    let verdict = outcome.verdict();
    let status = verdict.exit_status();
    eprintln!("{verdict}: exit status {status}");
    ExitCode::from(status)
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

impl From<CorpusArg> for CorpusMode {
    fn from(arg: CorpusArg) -> Self {
        match arg {
            CorpusArg::Seed => Self::Seed,
            CorpusArg::Reuse => Self::Reuse,
        }
    }
}

/// Where the load generator sits relative to the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum NetworkLegArg {
    /// The generator and the server share one host.
    SameHost,
    /// Different hosts in one zone of one provider.
    SameZone,
    /// Different regions.
    CrossRegion,
}

impl From<NetworkLegArg> for NetworkLeg {
    fn from(arg: NetworkLegArg) -> Self {
        match arg {
            NetworkLegArg::SameHost => Self::SameHost,
            NetworkLegArg::SameZone => Self::SameZone,
            NetworkLegArg::CrossRegion => Self::CrossRegion,
        }
    }
}

/// The workload and run settings an interleaved `perf-compare` takes, each
/// as `perf` takes it.
#[derive(clap::Args)]
struct CompareRunArgs {
    /// The performance root, `perf/` in this repository (workloads/,
    /// payloads.yaml, payloads/, hosts/). Needed by `--ab` and `--aa`.
    #[arg(long)]
    root: Option<PathBuf>,
    /// The workload document both sides are offered. Needed by `--ab` and
    /// `--aa`.
    #[arg(long)]
    workload: Option<PathBuf>,
    /// The catalogue root a workload's `requires` resolve against.
    #[arg(long, default_value = "catalogue")]
    catalogue: PathBuf,
    /// A statement whose claims keep the interactions it covers and drop the
    /// rest.
    #[arg(long)]
    statement: Option<PathBuf>,
    /// Seed both corpora, or reuse the ones stored under `--out`.
    #[arg(long, value_enum, default_value_t = CorpusArg::Seed)]
    corpus: CorpusArg,
    /// Parallel seeding workers.
    #[arg(long, default_value_t = 16)]
    seed_workers: usize,
    /// The warm-up's rate and the scout's starting rate, operations per
    /// second.
    #[arg(long, default_value_t = veredictum::perf_emulate::run::DEFAULT_START_RATE)]
    start_rate: f64,
    /// The scout ramp's end rate, operations per second.
    #[arg(long, default_value_t = veredictum::perf_emulate::run::DEFAULT_SCOUT_END_RATE)]
    scout_end_rate: f64,
    /// The id of the host prescription under `perf/hosts/` the run claims.
    #[arg(long)]
    host_prescription: Option<String>,
    /// Where the load generator sits relative to the servers.
    #[arg(long, value_enum, default_value_t = NetworkLegArg::SameHost)]
    network_leg: NetworkLegArg,
    /// The posture profile both sides declare.
    #[arg(long, default_value = "minimal")]
    posture: String,
}

/// What `perf-compare` was asked to do.
enum CompareInvocation<'a> {
    /// Align committed records.
    Records(&'a [PathBuf]),
    /// Run one or two targets interleaved.
    Interleaved {
        /// Side A's IXIT.
        a: &'a Path,
        /// Side B's IXIT; absent for an A/A run.
        b: Option<&'a Path>,
    },
    /// Run one target against every pinned reference CDR.
    Baselines {
        /// The target's IXIT.
        a: &'a Path,
    },
}

fn perf_compare_command(
    invocation: &CompareInvocation<'_>,
    run: &CompareRunArgs,
    tolerance: f64,
    floor: Option<&Path>,
    out: Option<&Path>,
) -> ExitCode {
    if !(tolerance.is_finite() && tolerance >= 0.0) {
        eprintln!("perf-compare: --tolerance must be a non-negative number (got {tolerance})");
        return ExitCode::from(2);
    }
    let comparison = match invocation {
        CompareInvocation::Records(paths) => {
            let comparison = match align_records(paths, tolerance, floor) {
                Ok(comparison) => comparison,
                Err(e) => return fail(&e),
            };
            if let Some(dir) = out
                && let Err(e) = write_comparison(&comparison, dir)
            {
                return fail(&e);
            }
            comparison
        }
        CompareInvocation::Interleaved { a, .. } | CompareInvocation::Baselines { a } => {
            let (Some(root), Some(workload), Some(out)) = (&run.root, &run.workload, out) else {
                eprintln!(
                    "perf-compare: --ab, --aa and --with-baselines need --root, --workload and --out"
                );
                return ExitCode::from(2);
            };
            let b = match invocation {
                CompareInvocation::Interleaved { b, .. } => *b,
                _ => None,
            };
            let progress = |message: String| eprintln!("[perf-compare] {message}");
            let posture = match PostureProfile::by_name(&run.posture) {
                Ok(profile) => profile,
                Err(e) => return fail(&e),
            };
            let request = InterleavedRequest {
                settings: WorkloadSettings {
                    root,
                    catalogue: &run.catalogue,
                    workload,
                    statement: run.statement.as_deref(),
                    corpus: run.corpus.into(),
                    seed_workers: run.seed_workers,
                    start_rate: run.start_rate,
                    scout_end_rate: run.scout_end_rate,
                    host_prescription: run.host_prescription.clone(),
                    network_leg: run.network_leg.into(),
                    posture,
                },
                ixit_a: a,
                ixit_b: b,
                out,
                tolerance,
                floor,
            };
            if matches!(invocation, CompareInvocation::Baselines { .. }) {
                let docker = veredictum::perf_emulate::baselines::DockerCli::new();
                let done = match run_with_baselines(&request, &docker, &progress) {
                    Ok(done) => done,
                    Err(e) => return fail(&e),
                };
                let mut mismatched = false;
                for (cdr, comparison) in &done {
                    println!("against {}:", cdr.display_name());
                    print!(
                        "{}",
                        veredictum::perf_result::compare::terminal(&comparison.comparison)
                    );
                    mismatched |= comparison.comparison.mismatched();
                }
                return if mismatched {
                    ExitCode::from(1)
                } else {
                    ExitCode::SUCCESS
                };
            }
            match run_compare(&request, &progress) {
                Ok(done) => done.comparison,
                Err(e) => return fail(&e),
            }
        }
    };
    print!(
        "{}",
        veredictum::perf_result::compare::terminal(&comparison)
    );
    if comparison.mismatched() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
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
    /// The GitHub Actions annotations file.
    annotations: Option<&'a Path>,
}

/// Writes the views of an Arazzo results document.
///
/// Every console line goes to standard error, because standard output
/// carries the Markdown summary when no file receives it. The exit code is
/// `0` when every view was written and `2` when the document could not be
/// read or a file could not be written.
fn export_command(results: &Path, targets: &ExportTargets<'_>) -> ExitCode {
    let views = match export_document(results) {
        Ok(views) => views,
        Err(e) => return fail(&e),
    };
    let (junit, ctrf, annotations) = match &views {
        Exports::Results(views) => (
            Some(&views.junit),
            Some(&views.ctrf),
            Some(&views.annotations),
        ),
        Exports::Perf { .. } => (None, None, None),
    };
    if junit.is_none()
        && (targets.junit.is_some() || targets.ctrf.is_some() || targets.annotations.is_some())
    {
        eprintln!(
            "{} is a perf-result, which has a Markdown view only: --junit, --ctrf and \
             --annotations read an Arazzo results document",
            results.display()
        );
        return ExitCode::from(2);
    }
    for (path, body) in [
        (targets.junit, junit),
        (targets.ctrf, ctrf),
        (targets.markdown, Some(&views.markdown().to_owned())),
        (targets.annotations, annotations),
    ] {
        let (Some(path), Some(body)) = (path, body) else {
            continue;
        };
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
            .and_then(|mut file| std::io::Write::write_all(&mut file, views.markdown().as_bytes()));
        if let Err(e) = appended {
            eprintln!("cannot append to {}: {e}", path.display());
            return ExitCode::from(2);
        }
        eprintln!("appended the summary to {}", path.display());
    }
    if targets.markdown.is_none() && targets.step_summary.is_none() {
        print!("{}", views.markdown());
    }
    ExitCode::SUCCESS
}
