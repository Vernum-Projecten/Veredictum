// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The operator's signal (#489): what a hosted instance is doing, in counts,
//! durations and queue depth, and how close its container is to its limits.
//!
//! Two halves, for two questions:
//!
//! - **The counters** answer "what has this process seen since it started":
//!   runs by outcome, refusals by their stated reason, server-function
//!   responses by status class, run durations as a histogram. They live in
//!   memory like the job map, a restart zeroes them, and [`ROUTE`] serves them
//!   in the Prometheus text format
//!   (<https://prometheus.io/docs/instrumenting/exposition_formats/>) to a
//!   caller presenting the operator's bearer token, and to nobody else.
//! - **The run record** answers "what happened to run X", durably. Each run
//!   writes one JSON line when its engine starts and one when it ends, to
//!   standard error, where the host's log driver keeps it past a restart of
//!   the image. The console itself stores nothing.
//!
//! Nothing a run produces is in either half. Every label is a closed
//! vocabulary this module spells itself, so no visitor-supplied text reaches
//! the exposition: no target URL, credential, SUT name, result row or response
//! body is counted, labelled or logged here. A run is named by its id only.
//!
//! No openEHR spec governs this: it is our own design.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::Serialize;

use crate::rate_limit::Metered;
use crate::run_job::RunId;
use crate::target_safety::{RefusedFamily, TargetRefusal};

/// Where the exposition is served.
pub const ROUTE: &str = "/metrics";

/// The environment variable holding the bearer token [`ROUTE`] requires.
///
/// Unset or empty, the route answers 404 and nothing is exposed. Any other
/// value that is not a usable token refuses to start: a typo must never leave
/// the counters open to whoever asks.
pub const TOKEN_ENV: &str = "VEREDICTUM_METRICS_TOKEN";

/// The fewest characters a token may have.
///
/// `openssl rand -hex 32` writes 64, which is what the hosted runbook uses.
pub const MIN_TOKEN_CHARS: usize = 32;

/// Where a Linux host mounts the container's own cgroup v2 hierarchy.
///
/// Inside a container with a private cgroup namespace, Docker's default on a
/// cgroup v2 host, this directory IS the container's cgroup, so its counters
/// cover the console and every engine it spawned
/// (<https://docs.kernel.org/admin-guide/cgroup-v2.html>).
pub const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// The exposition's media type (Prometheus text format 0.0.4).
pub const MEDIA_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// The upper bounds of the run-duration histogram, in seconds.
///
/// They span a run against a fast local server to the wall-clock cap
/// (`run_job::RUN_WALL_CLOCK`, thirty minutes), so a campaign that ends at the
/// cap lands in the last finite bucket rather than only in `+Inf`.
pub const DURATION_BUCKETS_SECS: [u64; 7] = [60, 120, 300, 600, 900, 1_200, 1_800];

/// How a run left the job map.
///
/// Every accepted run ends in exactly one of these, so `accepted` equals the
/// sum of the ended counts plus whatever is still in flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEnd {
    /// The engine exited and its record parsed.
    Finished,
    /// The operator cancelled it while it was driving.
    Cancelled,
    /// The wall-clock cap ended it.
    Expired,
    /// The engine exited without a parseable record.
    Failed,
    /// The engine could not be spawned, at the start or at promotion from the
    /// queue, so the run never drove.
    NotStarted,
    /// The operator cancelled it while it was still queued.
    Withdrawn,
}

impl RunEnd {
    /// Every outcome, in the order the exposition lists them.
    pub const ALL: [Self; 6] = [
        Self::Finished,
        Self::Cancelled,
        Self::Expired,
        Self::Failed,
        Self::NotStarted,
        Self::Withdrawn,
    ];

    /// The label value, identical to the run record's `outcome`.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Finished => "finished",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
            Self::Failed => "failed",
            Self::NotStarted => "not_started",
            Self::Withdrawn => "withdrawn",
        }
    }
}

/// Why the target guard refused, without the address it refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetReason {
    /// The URL did not parse.
    Unreadable,
    /// The URL names no host.
    NoHost,
    /// The host did not resolve.
    Unresolvable,
    /// The host resolved into a family only this instance can reach.
    Refused(RefusedFamily),
    /// The check itself could not run.
    Unchecked,
}

impl From<&TargetRefusal> for TargetReason {
    fn from(refusal: &TargetRefusal) -> Self {
        match refusal {
            TargetRefusal::Unreadable { .. } => Self::Unreadable,
            TargetRefusal::NoHost { .. } => Self::NoHost,
            TargetRefusal::Unresolvable { .. } => Self::Unresolvable,
            TargetRefusal::Refused { family, .. } => Self::Refused(*family),
            TargetRefusal::Unchecked(_) => Self::Unchecked,
        }
    }
}

impl TargetReason {
    /// The label value.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::NoHost => "no_host",
            Self::Unresolvable => "unresolvable",
            Self::Unchecked => "unchecked",
            Self::Refused(family) => match family {
                RefusedFamily::Loopback => "loopback",
                RefusedFamily::Private => "private",
                RefusedFamily::LinkLocal => "link_local",
                RefusedFamily::UniqueLocal => "unique_local",
                RefusedFamily::Unspecified => "unspecified",
                RefusedFamily::Multicast => "multicast",
                RefusedFamily::Shared => "shared",
                RefusedFamily::Broadcast => "broadcast",
            },
        }
    }
}

/// Why the statement validator refused a pasted claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StatementReason {
    /// Past the statement cap.
    TooLarge,
    /// Not JSON at all.
    NotJson,
    /// JSON the published statement schema refuses.
    SchemaInvalid,
    /// Schema-valid JSON the engine's typed reader refuses.
    Unparsable,
}

impl StatementReason {
    /// The label value.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::TooLarge => "too_large",
            Self::NotJson => "not_json",
            Self::SchemaInvalid => "schema_invalid",
            Self::Unparsable => "unparsable",
        }
    }
}

/// One refusal, by the seam that refused and its stated reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Refusal {
    /// The per-submitter rate ledger.
    Rate(Metered),
    /// The hosted posture's target guard.
    Target(TargetReason),
    /// The statement validator.
    Statement(StatementReason),
    /// The submitter already has a run in flight.
    AlreadyInFlight,
}

impl Refusal {
    /// The `guard` and `reason` label values.
    #[must_use]
    pub const fn labels(self) -> (&'static str, &'static str) {
        match self {
            Self::Rate(Metered::Probe) => ("rate", "probe"),
            Self::Rate(Metered::Start) => ("rate", "start"),
            Self::Target(reason) => ("target", reason.token()),
            Self::Statement(reason) => ("statement", reason.token()),
            Self::AlreadyInFlight => ("concurrency", "already_in_flight"),
        }
    }
}

/// A server-function response, by its status class (RFC 9110 §15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResponseClass {
    /// 1xx.
    Informational,
    /// 2xx.
    Success,
    /// 3xx.
    Redirection,
    /// 4xx.
    ClientError,
    /// 5xx.
    ServerError,
}

impl ResponseClass {
    /// The class of `status`.
    #[must_use]
    pub fn of(status: axum::http::StatusCode) -> Self {
        if status.is_informational() {
            Self::Informational
        } else if status.is_success() {
            Self::Success
        } else if status.is_redirection() {
            Self::Redirection
        } else if status.is_client_error() {
            Self::ClientError
        } else {
            Self::ServerError
        }
    }

    /// The label value.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Informational => "1xx",
            Self::Success => "2xx",
            Self::Redirection => "3xx",
            Self::ClientError => "4xx",
            Self::ServerError => "5xx",
        }
    }
}

/// The durable record of one run, written as one JSON line.
///
/// Identifiers, outcomes, durations and counts only: the record carries no
/// target, no SUT name, no credential and no result row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RunRecord {
    /// The engine was spawned and the run began driving.
    RunStarted {
        /// The run's id.
        run: RunId,
        /// How long it waited in the queue, in milliseconds.
        queued_ms: u64,
    },
    /// The run left the job map.
    RunEnded {
        /// The run's id.
        run: RunId,
        /// How it ended.
        outcome: RunEnd,
        /// How long it waited in the queue, in milliseconds.
        queued_ms: u64,
        /// How long its engine drove, in milliseconds; absent for a run that
        /// never drove.
        driving_ms: Option<u64>,
        /// Cases the engine reported complete.
        cases_completed: u64,
        /// Cases the engine announced; `0` when it announced none.
        cases_total: u64,
    },
}

impl RunRecord {
    /// The record as one line of JSON.
    ///
    /// # Errors
    /// The serializer's own refusal, which a record of ids and integers does
    /// not produce.
    pub fn line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Writes the record to standard error, which the host's log driver keeps.
    #[expect(
        clippy::print_stderr,
        reason = "the run record's durable home is the container's log stream, which the host keeps past an image restart; it carries ids and counts only"
    )]
    pub fn emit(&self) {
        match self.line() {
            Ok(line) => eprintln!("{line}"),
            Err(e) => eprintln!("veredictum-console: a run record did not serialize: {e}"),
        }
    }
}

/// How full the job map is at the moment of a read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Occupancy {
    /// Runs whose engine is driving.
    pub running: u64,
    /// Runs accepted and waiting for a slot.
    pub queued: u64,
    /// The concurrency cap this host states.
    pub max_concurrent: u64,
}

/// The container's memory limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryMax {
    /// A limit, in bytes.
    Bytes(u64),
    /// `max`: no limit is set on this cgroup.
    Unlimited,
}

/// The container's resource use, from its cgroup v2 interface files.
///
/// A field is `None` where the file is absent, which is every host that is
/// not Linux with cgroup v2: there is nothing to report there, and the
/// exposition leaves the series out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Container {
    /// `memory.current`: bytes charged now, page cache included.
    pub memory_current: Option<u64>,
    /// `memory.peak`: the most ever charged since the container started.
    pub memory_peak: Option<u64>,
    /// `memory.max`: the limit the compose file sets.
    pub memory_max: Option<MemoryMax>,
    /// `memory.stat` `anon`: anonymous memory charged now, the working set
    /// page cache does not inflate.
    pub memory_anon: Option<u64>,
    /// `memory.events` `oom_kill`: processes the kernel killed in this
    /// container for memory since it started.
    pub oom_kills: Option<u64>,
    /// `cpu.stat` `usage_usec`: CPU time consumed, in microseconds.
    pub cpu_usage_usec: Option<u64>,
    /// `pids.current`: processes and threads alive in the container.
    pub pids_current: Option<u64>,
}

/// One read of the container's resource files.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerRead {
    /// What was read.
    pub stats: Container,
    /// The files that exist and could not be read or parsed. Each is exposed
    /// as its own series, so a broken read is visible rather than a gap.
    pub unreadable: Vec<&'static str>,
}

/// Reads the container's resource use from the cgroup directory `root`.
///
/// Blocking file I/O: an async caller runs it on the blocking pool.
#[must_use]
pub fn read_container(root: &Path) -> ContainerRead {
    let mut read = ContainerRead::default();
    read.stats.memory_current = single(root, "memory.current", &mut read.unreadable);
    read.stats.memory_peak = single(root, "memory.peak", &mut read.unreadable);
    read.stats.pids_current = single(root, "pids.current", &mut read.unreadable);
    read.stats.memory_max =
        file(root, "memory.max", &mut read.unreadable).and_then(|text| match text.trim() {
            "max" => Some(MemoryMax::Unlimited),
            value => value.parse().map(MemoryMax::Bytes).map_or_else(
                |_| {
                    read.unreadable.push("memory.max");
                    None
                },
                Some,
            ),
        });
    read.stats.cpu_usage_usec = keyed(root, "cpu.stat", "usage_usec", &mut read.unreadable);
    read.stats.memory_anon = keyed(root, "memory.stat", "anon", &mut read.unreadable);
    read.stats.oom_kills = keyed(root, "memory.events", "oom_kill", &mut read.unreadable);
    read
}

/// One `key value` line of a flat-keyed interface file.
///
/// A file that exists without the key is unreadable: every kernel that
/// writes these files writes these keys.
fn keyed(
    root: &Path,
    name: &'static str,
    key: &str,
    unreadable: &mut Vec<&'static str>,
) -> Option<u64> {
    let text = file(root, name, unreadable)?;
    let value = text.lines().find_map(|line| {
        let (found, value) = line.split_once(' ')?;
        (found == key).then(|| value.trim().parse::<u64>())
    });
    if let Some(Ok(value)) = value {
        Some(value)
    } else {
        unreadable.push(name);
        None
    }
}

/// The text of one interface file, `None` when it is absent.
fn file(root: &Path, name: &'static str, unreadable: &mut Vec<&'static str>) -> Option<String> {
    match std::fs::read_to_string(root.join(name)) {
        Ok(text) => Some(text),
        // NOTE: no openEHR spec governs this — our own design; an absent file
        // means this host has no such counter, which is a legitimate absence.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            unreadable.push(name);
            None
        }
    }
}

/// One interface file holding a single unsigned integer.
fn single(root: &Path, name: &'static str, unreadable: &mut Vec<&'static str>) -> Option<u64> {
    let text = file(root, name, unreadable)?;
    if let Ok(value) = text.trim().parse() {
        Some(value)
    } else {
        unreadable.push(name);
        None
    }
}

/// The counters a process accumulates.
#[derive(Debug, Default)]
struct Counters {
    accepted: u64,
    started: u64,
    ended: BTreeMap<RunEnd, u64>,
    durations: Vec<u64>,
    duration_sum_ms: u64,
    duration_count: u64,
    cases_completed: u64,
    refusals: BTreeMap<Refusal, u64>,
    responses: BTreeMap<ResponseClass, u64>,
}

/// The bearer token that opens [`ROUTE`].
///
/// Its `Debug` never prints the value.
#[derive(Clone)]
pub struct AccessToken(Arc<str>);

impl std::fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccessToken(..)")
    }
}

/// A [`TOKEN_ENV`] value this console will not serve the counters behind.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{TOKEN_ENV} is set and is not a usable token ({reason}); the metrics stay closed and the console does not start. Generate one with `openssl rand -hex 32`, or leave the variable unset to serve no metrics"
)]
pub struct TokenRefusal {
    /// What is wrong with it, never the value itself.
    pub reason: &'static str,
}

impl AccessToken {
    /// Reads [`TOKEN_ENV`]: `None` when unset or empty.
    ///
    /// # Errors
    /// [`TokenRefusal`] for any other value [`AccessToken::parse`] refuses.
    pub fn from_env() -> Result<Option<Self>, TokenRefusal> {
        match std::env::var(TOKEN_ENV) {
            Ok(value) if !value.is_empty() => Self::parse(&value).map(Some),
            Ok(_) | Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => Err(TokenRefusal {
                reason: "it is not valid Unicode",
            }),
        }
    }

    /// A token from its text.
    ///
    /// The characters are RFC 6750 §2.1's `b64token`, so the value fits an
    /// `Authorization` header verbatim, and there are at least
    /// [`MIN_TOKEN_CHARS`] of them.
    ///
    /// # Errors
    /// [`TokenRefusal`] naming what is wrong.
    pub fn parse(value: &str) -> Result<Self, TokenRefusal> {
        let b64token = value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'+' | b'/' | b'=')
        });
        if !b64token {
            return Err(TokenRefusal {
                reason: "it holds a character outside RFC 6750's b64token",
            });
        }
        if value.len() < MIN_TOKEN_CHARS {
            return Err(TokenRefusal {
                reason: "it is shorter than 32 characters",
            });
        }
        Ok(Self(Arc::from(value)))
    }

    /// Whether an `Authorization` header value presents this token.
    ///
    /// The comparison reads every byte whatever it finds, so its timing says
    /// nothing about how much of a guess was right.
    #[must_use]
    pub fn admits(&self, authorization: Option<&axum::http::HeaderValue>) -> bool {
        let Some(presented) = authorization
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
        else {
            return false;
        };
        let expected = self.0.as_bytes();
        let presented = presented.as_bytes();
        let differs = expected
            .iter()
            .zip(presented)
            .fold(0_u8, |acc, (a, b)| acc | (a ^ b));
        differs == 0 && expected.len() == presented.len()
    }
}

/// The operator's signal: the counters, and the token that opens them.
///
/// Cloning shares the counters, which is how the job map and the request
/// layers report into the one set the route serves.
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    counters: Arc<Mutex<Counters>>,
    access: Option<AccessToken>,
}

impl Metrics {
    /// The same counters, opened by `token`.
    #[must_use]
    pub fn with_access(mut self, token: Option<AccessToken>) -> Self {
        self.access = token;
        self
    }

    /// The token that opens [`ROUTE`], when the operator set one.
    #[must_use]
    pub fn access(&self) -> Option<&AccessToken> {
        self.access.as_ref()
    }

    /// The counters' guard.
    fn counters(&self) -> MutexGuard<'_, Counters> {
        // NOTE: no openEHR spec governs this — our own design; every update is
        // a single increment, so a panic elsewhere leaves no half-written count.
        self.counters.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Counts a run the job map accepted.
    pub fn run_accepted(&self) {
        let mut counters = self.counters();
        counters.accepted = counters.accepted.saturating_add(1);
    }

    /// Counts a run whose engine started, and writes its record.
    pub fn run_started(&self, run: RunId, queued: Duration) {
        {
            let mut counters = self.counters();
            counters.started = counters.started.saturating_add(1);
        }
        RunRecord::RunStarted {
            run,
            queued_ms: millis(queued),
        }
        .emit();
    }

    /// Counts a run that left the job map, and writes its record.
    pub fn run_ended(&self, ended: &Ended) {
        let driving_ms = ended.driving.map(millis);
        {
            let mut counters = self.counters();
            let count = counters.ended.entry(ended.outcome).or_default();
            *count = count.saturating_add(1);
            counters.cases_completed = counters
                .cases_completed
                .saturating_add(ended.cases_completed);
            if let Some(ms) = driving_ms {
                if counters.durations.is_empty() {
                    counters.durations = vec![0; DURATION_BUCKETS_SECS.len()];
                }
                let secs = ms.div_ceil(1_000);
                for (bound, count) in DURATION_BUCKETS_SECS.iter().zip(&mut counters.durations) {
                    if secs <= *bound {
                        *count = count.saturating_add(1);
                    }
                }
                counters.duration_sum_ms = counters.duration_sum_ms.saturating_add(ms);
                counters.duration_count = counters.duration_count.saturating_add(1);
            }
        }
        RunRecord::RunEnded {
            run: ended.run,
            outcome: ended.outcome,
            queued_ms: millis(ended.queued),
            driving_ms,
            cases_completed: ended.cases_completed,
            cases_total: ended.cases_total,
        }
        .emit();
    }

    /// Counts one refusal.
    pub fn refused(&self, refusal: Refusal) {
        let mut counters = self.counters();
        let count = counters.refusals.entry(refusal).or_default();
        *count = count.saturating_add(1);
    }

    /// Counts one server-function response.
    pub fn responded(&self, class: ResponseClass) {
        let mut counters = self.counters();
        let count = counters.responses.entry(class).or_default();
        *count = count.saturating_add(1);
    }

    /// The exposition: every counter, the job map's occupancy and the
    /// container's resource use, in the Prometheus text format.
    #[must_use]
    pub fn render(&self, occupancy: Occupancy, container: &ContainerRead) -> String {
        let counters = self.counters();
        let mut out = String::new();

        run_series(&mut out, &counters);

        family(
            &mut out,
            "veredictum_runs_running",
            "gauge",
            "Runs whose engine is driving now.",
        );
        let _ = writeln!(out, "veredictum_runs_running {}", occupancy.running);
        family(
            &mut out,
            "veredictum_runs_queued",
            "gauge",
            "Runs accepted and waiting for a slot now.",
        );
        let _ = writeln!(out, "veredictum_runs_queued {}", occupancy.queued);
        family(
            &mut out,
            "veredictum_runs_max_concurrent",
            "gauge",
            "The concurrency cap this host states.",
        );
        let _ = writeln!(
            out,
            "veredictum_runs_max_concurrent {}",
            occupancy.max_concurrent
        );

        family(
            &mut out,
            "veredictum_refusals_total",
            "counter",
            "Requests refused, by the guard that refused and its stated reason.",
        );
        for (refusal, count) in &counters.refusals {
            let (guard, reason) = refusal.labels();
            let _ = writeln!(
                out,
                "veredictum_refusals_total{{guard=\"{guard}\",reason=\"{reason}\"}} {count}"
            );
        }
        family(
            &mut out,
            "veredictum_server_fn_responses_total",
            "counter",
            "Server-function responses by status class. A refusal a server function raises answers 5xx, so the refusal counts are part of the 5xx count.",
        );
        for (class, count) in &counters.responses {
            let _ = writeln!(
                out,
                "veredictum_server_fn_responses_total{{class=\"{}\"}} {count}",
                class.token()
            );
        }
        drop(counters);

        container_series(&mut out, container);
        out
    }
}

/// What the job map knows about a run as it leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ended {
    /// The run's id.
    pub run: RunId,
    /// How it ended.
    pub outcome: RunEnd,
    /// How long it waited in the queue.
    pub queued: Duration,
    /// How long its engine drove; `None` for a run that never drove.
    pub driving: Option<Duration>,
    /// Cases the engine reported complete.
    pub cases_completed: u64,
    /// Cases the engine announced.
    pub cases_total: u64,
}

/// The run half of the exposition: the lifecycle counters and the duration
/// histogram.
fn run_series(out: &mut String, counters: &Counters) {
    family(
        out,
        "veredictum_runs_accepted_total",
        "counter",
        "Runs the job map accepted, queued or started at once.",
    );
    let _ = writeln!(out, "veredictum_runs_accepted_total {}", counters.accepted);
    family(
        out,
        "veredictum_runs_started_total",
        "counter",
        "Runs whose engine was spawned.",
    );
    let _ = writeln!(out, "veredictum_runs_started_total {}", counters.started);
    family(
        out,
        "veredictum_runs_ended_total",
        "counter",
        "Runs that left the job map, by how they ended.",
    );
    for outcome in RunEnd::ALL {
        let count = counters.ended.get(&outcome).copied().unwrap_or_default();
        let _ = writeln!(
            out,
            "veredictum_runs_ended_total{{outcome=\"{}\"}} {count}",
            outcome.token()
        );
    }
    family(
        out,
        "veredictum_run_cases_completed_total",
        "counter",
        "Cases the engine reported complete, over every ended run.",
    );
    let _ = writeln!(
        out,
        "veredictum_run_cases_completed_total {}",
        counters.cases_completed
    );

    family(
        out,
        "veredictum_run_duration_seconds",
        "histogram",
        "How long each run's engine drove.",
    );
    for (index, bound) in DURATION_BUCKETS_SECS.iter().enumerate() {
        let count = counters.durations.get(index).copied().unwrap_or_default();
        let _ = writeln!(
            out,
            "veredictum_run_duration_seconds_bucket{{le=\"{bound}\"}} {count}"
        );
    }
    let _ = writeln!(
        out,
        "veredictum_run_duration_seconds_bucket{{le=\"+Inf\"}} {}",
        counters.duration_count
    );
    let _ = writeln!(
        out,
        "veredictum_run_duration_seconds_sum {}.{:03}",
        counters.duration_sum_ms.div_euclid(1_000),
        counters.duration_sum_ms.rem_euclid(1_000)
    );
    let _ = writeln!(
        out,
        "veredictum_run_duration_seconds_count {}",
        counters.duration_count
    );
}

/// The container half of the exposition.
fn container_series(out: &mut String, container: &ContainerRead) {
    let stats = container.stats;
    let gauges = [
        (
            "veredictum_container_memory_bytes",
            "Memory charged to the container now (cgroup v2 memory.current), page cache included.",
            stats.memory_current,
        ),
        (
            "veredictum_container_memory_peak_bytes",
            "The most memory ever charged to the container since it started (cgroup v2 memory.peak).",
            stats.memory_peak,
        ),
        (
            "veredictum_container_memory_limit_bytes",
            "The container's memory limit (cgroup v2 memory.max); absent when unlimited.",
            match stats.memory_max {
                Some(MemoryMax::Bytes(bytes)) => Some(bytes),
                Some(MemoryMax::Unlimited) | None => None,
            },
        ),
        (
            "veredictum_container_memory_anon_bytes",
            "Anonymous memory charged to the container now (cgroup v2 memory.stat anon), the working set without page cache.",
            stats.memory_anon,
        ),
        (
            "veredictum_container_pids",
            "Processes and threads alive in the container (cgroup v2 pids.current).",
            stats.pids_current,
        ),
    ];
    for (name, help, value) in gauges {
        if let Some(value) = value {
            family(out, name, "gauge", help);
            let _ = writeln!(out, "{name} {value}");
        }
    }
    if let Some(usec) = stats.cpu_usage_usec {
        family(
            out,
            "veredictum_container_cpu_seconds_total",
            "counter",
            "CPU time the container consumed, the console and every engine it spawned (cgroup v2 cpu.stat usage_usec).",
        );
        let _ = writeln!(
            out,
            "veredictum_container_cpu_seconds_total {}.{:06}",
            usec.div_euclid(1_000_000),
            usec.rem_euclid(1_000_000)
        );
    }
    if let Some(kills) = stats.oom_kills {
        family(
            out,
            "veredictum_container_oom_kills_total",
            "counter",
            "Processes the kernel killed in the container for memory since it started (cgroup v2 memory.events oom_kill).",
        );
        let _ = writeln!(out, "veredictum_container_oom_kills_total {kills}");
    }
    if !container.unreadable.is_empty() {
        family(
            out,
            "veredictum_container_stat_unreadable",
            "gauge",
            "A cgroup interface file that exists and could not be read or parsed.",
        );
        for name in &container.unreadable {
            let _ = writeln!(
                out,
                "veredictum_container_stat_unreadable{{file=\"{name}\"}} 1"
            );
        }
    }
}

/// One metric family's `HELP` and `TYPE` lines.
fn family(out: &mut String, name: &str, kind: &str, help: &str) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} {kind}");
}

/// A duration in whole milliseconds, saturating rather than wrapping.
fn millis(of: Duration) -> u64 {
    u64::try_from(of.as_millis()).unwrap_or(u64::MAX)
}

/// The route's answer.
///
/// 404 when the operator set no token, 401 with a `Bearer` challenge
/// (RFC 6750 §3) when the request does not present it, and the exposition
/// otherwise. The container is read on the blocking pool.
pub async fn answer(
    state: &crate::state::ConsoleState,
    headers: &axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::http::{StatusCode, header};
    use axum::response::IntoResponse as _;

    let Some(token) = state.metrics.access() else {
        return (StatusCode::NOT_FOUND, "no metrics are served here").into_response();
    };
    if !token.admits(headers.get(header::AUTHORIZATION)) {
        return (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                "Bearer realm=\"veredictum-metrics\"",
            )],
            "the metrics need the operator's bearer token",
        )
            .into_response();
    }
    let container = tokio::task::spawn_blocking(|| read_container(Path::new(CGROUP_ROOT)))
        .await
        .unwrap_or_else(|_| ContainerRead {
            stats: Container::default(),
            unreadable: vec!["cgroup"],
        });
    let body = state.metrics.render(state.jobs.occupancy(), &container);
    (
        [
            (header::CONTENT_TYPE, MEDIA_TYPE),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}

/// Counts every server-function response by its status class.
///
/// Placed outside the refusal layer, so a malformed call is counted as the
/// 4xx its caller reads rather than the 500 the framework wrote.
pub async fn count_server_fns(
    axum::extract::State(metrics): axum::extract::State<Metrics>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let server_fn = request.uri().path().starts_with(SERVER_FN_PREFIX);
    let response = next.run(request).await;
    if server_fn {
        metrics.responded(ResponseClass::of(response.status()));
    }
    response
}

/// The path prefix every `#[server]` function is mounted under, Leptos's
/// default (<https://docs.rs/server_fn/latest/server_fn/attr.server.html>).
pub const SERVER_FN_PREFIX: &str = "/api/";

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
mod tests {
    use std::time::Duration;

    use super::{
        AccessToken, Container, ContainerRead, Ended, MemoryMax, Metrics, Occupancy, Refusal,
        ResponseClass, RunEnd, RunRecord, StatementReason, TargetReason, read_container,
    };
    use crate::rate_limit::Metered;
    use crate::run_job::RunId;
    use crate::target_safety::RefusedFamily;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn run() -> Result<RunId, crate::run_job::RunIdError> {
        "3f2504e0-4f89-41d3-9a0c-0305e82c3301".parse()
    }

    #[test]
    fn every_outcome_token_is_the_records_own_spelling() -> Result<(), serde_json::Error> {
        for outcome in RunEnd::ALL {
            assert_eq!(
                serde_json::to_string(&outcome)?,
                format!("\"{}\"", outcome.token()),
                "the metric label and the record's outcome are one vocabulary"
            );
        }
        Ok(())
    }

    #[test]
    fn the_run_record_is_one_json_line_of_ids_and_counts() -> Result<(), Box<dyn std::error::Error>>
    {
        let line = RunRecord::RunEnded {
            run: run()?,
            outcome: RunEnd::Finished,
            queued_ms: 1_500,
            driving_ms: Some(435_000),
            cases_completed: 1_689,
            cases_total: 1_689,
        }
        .line()?;
        assert_eq!(
            line,
            "{\"event\":\"run_ended\",\"run\":\"3f2504e0-4f89-41d3-9a0c-0305e82c3301\",\"outcome\":\"finished\",\"queued_ms\":1500,\"driving_ms\":435000,\"cases_completed\":1689,\"cases_total\":1689}"
        );
        assert!(!line.contains('\n'), "one record is one line");
        let started = RunRecord::RunStarted {
            run: run()?,
            queued_ms: 0,
        }
        .line()?;
        assert_eq!(
            started,
            "{\"event\":\"run_started\",\"run\":\"3f2504e0-4f89-41d3-9a0c-0305e82c3301\",\"queued_ms\":0}"
        );
        Ok(())
    }

    #[test]
    fn a_token_is_long_and_fits_a_header() {
        assert!(AccessToken::parse(TOKEN).is_ok(), "64 hex characters");
        assert!(AccessToken::parse("short").is_err(), "too short");
        assert!(
            AccessToken::parse(&format!("{TOKEN} ")).is_err(),
            "a space is not a b64token character"
        );
        assert!(
            AccessToken::parse(&format!("{TOKEN}\u{e9}")).is_err(),
            "non-ASCII is refused"
        );
    }

    #[test]
    fn a_tokens_debug_never_prints_it() -> Result<(), super::TokenRefusal> {
        let token = AccessToken::parse(TOKEN)?;
        assert!(!format!("{token:?}").contains("0123"), "redacted");
        Ok(())
    }

    #[test]
    fn only_the_exact_bearer_token_is_admitted() -> Result<(), Box<dyn std::error::Error>> {
        let token = AccessToken::parse(TOKEN)?;
        let header = |text: &str| axum::http::HeaderValue::from_str(text);
        assert!(token.admits(Some(&header(&format!("Bearer {TOKEN}"))?)));
        assert!(!token.admits(None), "no header");
        assert!(!token.admits(Some(&header(TOKEN)?)), "no scheme");
        assert!(
            !token.admits(Some(&header(&format!("Basic {TOKEN}"))?)),
            "another scheme"
        );
        assert!(
            !token.admits(Some(&header(&format!("Bearer {TOKEN}0"))?)),
            "a longer value"
        );
        let prefix = TOKEN.get(..40).ok_or("the fixture is 64 characters")?;
        assert!(
            !token.admits(Some(&header(&format!("Bearer {prefix}"))?)),
            "a prefix"
        );
        Ok(())
    }

    #[test]
    fn every_accepted_run_is_counted_once_where_it_ended() -> Result<(), Box<dyn std::error::Error>>
    {
        let metrics = Metrics::default();
        for outcome in [RunEnd::Finished, RunEnd::Failed, RunEnd::Withdrawn] {
            metrics.run_accepted();
            metrics.run_ended(&Ended {
                run: run()?,
                outcome,
                queued: Duration::ZERO,
                driving: (outcome != RunEnd::Withdrawn).then_some(Duration::from_secs(90)),
                cases_completed: 10,
                cases_total: 12,
            });
        }
        let text = metrics.render(Occupancy::default(), &ContainerRead::default());
        assert!(
            text.contains("veredictum_runs_accepted_total 3\n"),
            "{text}"
        );
        assert!(
            text.contains("veredictum_runs_ended_total{outcome=\"finished\"} 1\n"),
            "{text}"
        );
        assert!(
            text.contains("veredictum_runs_ended_total{outcome=\"withdrawn\"} 1\n"),
            "{text}"
        );
        assert!(
            text.contains("veredictum_runs_ended_total{outcome=\"expired\"} 0\n"),
            "an outcome nobody reached is a zero, not a gap"
        );
        assert!(text.contains("veredictum_run_cases_completed_total 30\n"));
        // Two runs drove ninety seconds: above the 60 s bound, inside 120 s.
        assert!(text.contains("veredictum_run_duration_seconds_bucket{le=\"60\"} 0\n"));
        assert!(text.contains("veredictum_run_duration_seconds_bucket{le=\"120\"} 2\n"));
        assert!(text.contains("veredictum_run_duration_seconds_bucket{le=\"+Inf\"} 2\n"));
        assert!(text.contains("veredictum_run_duration_seconds_sum 180.000\n"));
        assert!(text.contains("veredictum_run_duration_seconds_count 2\n"));
        Ok(())
    }

    #[test]
    fn refusals_are_counted_by_guard_and_reason() {
        let metrics = Metrics::default();
        metrics.refused(Refusal::Rate(Metered::Start));
        metrics.refused(Refusal::Rate(Metered::Start));
        metrics.refused(Refusal::Target(TargetReason::Refused(
            RefusedFamily::Private,
        )));
        metrics.refused(Refusal::Statement(StatementReason::SchemaInvalid));
        metrics.refused(Refusal::AlreadyInFlight);
        let text = metrics.render(Occupancy::default(), &ContainerRead::default());
        for line in [
            "veredictum_refusals_total{guard=\"rate\",reason=\"start\"} 2\n",
            "veredictum_refusals_total{guard=\"target\",reason=\"private\"} 1\n",
            "veredictum_refusals_total{guard=\"statement\",reason=\"schema_invalid\"} 1\n",
            "veredictum_refusals_total{guard=\"concurrency\",reason=\"already_in_flight\"} 1\n",
        ] {
            assert!(text.contains(line), "{line} in {text}");
        }
    }

    #[test]
    fn responses_are_counted_by_class() {
        let metrics = Metrics::default();
        metrics.responded(ResponseClass::of(axum::http::StatusCode::OK));
        metrics.responded(ResponseClass::of(axum::http::StatusCode::PAYLOAD_TOO_LARGE));
        metrics.responded(ResponseClass::of(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        ));
        metrics.responded(ResponseClass::of(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        ));
        let text = metrics.render(Occupancy::default(), &ContainerRead::default());
        assert!(text.contains("veredictum_server_fn_responses_total{class=\"2xx\"} 1\n"));
        assert!(text.contains("veredictum_server_fn_responses_total{class=\"4xx\"} 1\n"));
        assert!(text.contains("veredictum_server_fn_responses_total{class=\"5xx\"} 2\n"));
    }

    #[test]
    fn occupancy_is_read_at_render_time() {
        let text = Metrics::default().render(
            Occupancy {
                running: 2,
                queued: 3,
                max_concurrent: 2,
            },
            &ContainerRead::default(),
        );
        assert!(text.contains("veredictum_runs_running 2\n"));
        assert!(text.contains("veredictum_runs_queued 3\n"));
        assert!(text.contains("veredictum_runs_max_concurrent 2\n"));
    }

    #[test]
    fn every_label_comes_from_a_closed_vocabulary() {
        let metrics = Metrics::default();
        metrics.refused(Refusal::Target(TargetReason::Unresolvable));
        let text = metrics.render(Occupancy::default(), &ContainerRead::default());
        for line in text.lines().filter(|line| !line.starts_with('#')) {
            assert!(
                line.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_{}=\",.+ ".contains(&b)),
                "{line} carries a character no closed label spells"
            );
        }
    }

    #[test]
    fn the_container_is_read_from_its_cgroup_files() -> Result<(), Box<dyn std::error::Error>> {
        let dir = assert_fs::TempDir::new()?;
        std::fs::write(dir.path().join("memory.current"), "1048576\n")?;
        std::fs::write(dir.path().join("memory.peak"), "4294967296\n")?;
        std::fs::write(dir.path().join("memory.max"), "6442450944\n")?;
        std::fs::write(
            dir.path().join("cpu.stat"),
            "usage_usec 12500000\nuser_usec 10000000\nsystem_usec 2500000\n",
        )?;
        std::fs::write(dir.path().join("pids.current"), "17\n")?;
        std::fs::write(
            dir.path().join("memory.stat"),
            "anon 734003200\nfile 104857600\n",
        )?;
        std::fs::write(
            dir.path().join("memory.events"),
            "low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\noom_group_kill 0\n",
        )?;
        let read = read_container(dir.path());
        assert_eq!(
            read,
            ContainerRead {
                stats: Container {
                    memory_current: Some(1_048_576),
                    memory_peak: Some(4_294_967_296),
                    memory_max: Some(MemoryMax::Bytes(6_442_450_944)),
                    memory_anon: Some(734_003_200),
                    oom_kills: Some(0),
                    cpu_usage_usec: Some(12_500_000),
                    pids_current: Some(17),
                },
                unreadable: Vec::new(),
            }
        );
        let text = Metrics::default().render(Occupancy::default(), &read);
        assert!(text.contains("veredictum_container_memory_peak_bytes 4294967296\n"));
        assert!(text.contains("veredictum_container_memory_limit_bytes 6442450944\n"));
        assert!(text.contains("veredictum_container_cpu_seconds_total 12.500000\n"));
        assert!(text.contains("veredictum_container_memory_anon_bytes 734003200\n"));
        assert!(
            text.contains("veredictum_container_oom_kills_total 0\n"),
            "oom_kill is read as a key, never confused with oom_group_kill"
        );
        Ok(())
    }

    #[test]
    fn an_absent_file_is_a_gap_and_a_broken_one_is_named() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = assert_fs::TempDir::new()?;
        std::fs::write(dir.path().join("memory.max"), "max\n")?;
        std::fs::write(dir.path().join("memory.peak"), "lots\n")?;
        std::fs::write(dir.path().join("cpu.stat"), "user_usec 1\n")?;
        let read = read_container(dir.path());
        assert_eq!(read.stats.memory_max, Some(MemoryMax::Unlimited));
        assert_eq!(read.stats.memory_current, None, "absent");
        assert_eq!(read.unreadable, vec!["memory.peak", "cpu.stat"]);
        let text = Metrics::default().render(Occupancy::default(), &read);
        assert!(!text.contains("veredictum_container_memory_limit_bytes "));
        assert!(text.contains("veredictum_container_stat_unreadable{file=\"memory.peak\"} 1\n"));
        Ok(())
    }
}
