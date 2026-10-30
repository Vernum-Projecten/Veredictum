// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `perf-result` record: what one performance run measured, in a `raw`
//! part that holds the evidence and a `derived` part computed from it alone.
//!
//! The raw part carries the encoded HDR histograms (the standard V2
//! serialization, base64-wrapped), so [`judge::rejudge`] recomputes every
//! derived number from them and compares the result byte for byte, the
//! measured counterpart of re-judging a recording. Whole-plateau and
//! all-operation percentiles come from summed histograms, never from an
//! average of percentiles (Tene, "How NOT to Measure Latency", 2015;
//! `HdrHistogram` `add`, <https://docs.rs/hdrhistogram/7.5.4>). The record's
//! schema is `perf-result.schema.json`. No openEHR specification governs
//! this: our own design.

pub mod derive;
pub mod judge;
pub mod quantile;

use std::collections::BTreeMap;
use std::time::Duration;

use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::bench::fingerprint::EnvironmentFingerprint;
use crate::bench::posture::PostureRecord;
use crate::identity::IdentityRead;
use crate::ixit::Environment;
use crate::perf::{decode_hdr_v2, encode_hdr_v2};
use crate::perf_gen::arrivals::Rate;
use crate::perf_gen::drive::{OperationRecord, WindowRecord};
use crate::perf_gen::{ErrorClass, FailedExchange, OperationId};
use crate::perf_result::derive::Derived;

/// The sentence every record carries about what one run cannot show.
pub const SCOPE: &str = "A run of at most ten minutes measures steady-state capacity and says \
                         nothing about memory growth, data growth or behaviour over hours.";

/// The document family tag, which tells a reader this is a `perf-result`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Family {
    /// The only family this type reads.
    #[serde(rename = "perf-result")]
    PerfResult,
}

/// One performance run's record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerfResult {
    /// Always `perf-result`.
    pub family: Family,
    /// The evidence.
    pub raw: Raw,
    /// Every number computed from `raw`.
    pub derived: Derived,
}

impl PerfResult {
    /// Builds the record from its raw part, deriving the rest.
    ///
    /// # Errors
    /// Every error of [`derive::derive`].
    pub fn from_raw(raw: Raw) -> Result<Self, PerfResultError> {
        let derived = derive::derive(&raw)?;
        Ok(Self {
            family: Family::PerfResult,
            raw,
            derived,
        })
    }

    /// The record's canonical text: two-space pretty JSON and a trailing
    /// newline, the form [`judge::rejudge`] compares byte for byte.
    ///
    /// # Errors
    /// [`PerfResultError::Serialize`] if serialization fails.
    pub fn render(&self) -> Result<String, PerfResultError> {
        let mut text = serde_json::to_string_pretty(self)
            .map_err(|e| PerfResultError::Serialize(e.to_string()))?;
        text.push('\n');
        Ok(text)
    }
}

/// The instrument that wrote the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instrument {
    /// The instrument's name, `veredictum`.
    pub name: String,
    /// The instrument's version.
    pub version: String,
}

impl Instrument {
    /// This build of the instrument.
    #[must_use]
    pub fn this_build() -> Self {
        Self {
            name: env!("CARGO_PKG_NAME").to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

/// The server measured, and what it stated through ITS-REST System OPTIONS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// The base URL the run addressed.
    pub base_url: String,
    /// The answer to `OPTIONS <base>/`.
    pub options: TargetOptions,
}

/// How the System OPTIONS read went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "read", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetOptions {
    /// The server answered.
    Answered {
        /// The classified answer.
        answer: IdentityRead,
    },
    /// The read failed at the transport.
    Unanswered {
        /// The transport's own failure.
        error: String,
    },
}

/// The machine that offered the load, and the generator's own settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generator {
    /// What the engine established about the generator's host.
    pub host: EnvironmentFingerprint,
    /// The most exchanges outstanding at once.
    pub in_flight_budget: u32,
    /// The lag p99 above which a window is `generator_bound`, microseconds.
    pub lag_limit_us: u64,
    /// The runtime's worker threads.
    pub workers: u32,
}

/// The workload the run offered, embedded verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddedWorkload {
    /// The workload's id.
    pub id: String,
    /// The workload's version.
    pub version: String,
    /// `sha256:` and the hex digest of `document`.
    pub sha256: String,
    /// The workload document's text.
    pub document: String,
}

/// The corpus scale names (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorpusScale {
    /// Proof-of-concept volumes.
    #[serde(rename = "POC")]
    Poc,
    /// Small deployment volumes.
    S,
    /// Large deployment volumes.
    L,
    /// Regional deployment volumes.
    R,
}

impl CorpusScale {
    /// Every scale, smallest first.
    pub const ALL: &[CorpusScale] = &[Self::Poc, Self::S, Self::L, Self::R];

    /// The serialized token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Poc => "POC",
            Self::S => "S",
            Self::L => "L",
            Self::R => "R",
        }
    }
}

/// The corpus the run measured against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusRecord {
    /// The scale the recipe names.
    pub scale: CorpusScale,
    /// `sha256:` digest of the recipe the corpus was seeded from.
    pub recipe_sha256: String,
    /// `sha256:` digest of the seeded corpus as read back.
    pub digest: String,
}

/// One operation's evidence in one window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawOperation {
    /// Latencies of the successful exchanges, microseconds, encoded.
    pub ok_hdr: String,
    /// Latencies of the failed exchanges, encoded; absent when none failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_hdr: Option<String>,
    /// Failed exchanges by class, summing to the failed histogram's count.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub errors: BTreeMap<ErrorClass, u64>,
}

impl RawOperation {
    /// Encodes one operation's record.
    ///
    /// # Errors
    /// [`PerfResultError::Histogram`] if a histogram does not encode.
    pub fn from_record(record: &OperationRecord) -> Result<Self, PerfResultError> {
        let failed_hdr = if record.failed.is_empty() {
            None
        } else {
            Some(encode(&record.failed)?)
        };
        Ok(Self {
            ok_hdr: encode(&record.ok)?,
            failed_hdr,
            errors: record.errors.clone(),
        })
    }
}

/// One measured window's evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawWindow {
    /// The window's first offset from its phase's start, milliseconds.
    pub start_ms: u64,
    /// The first offset past the window, milliseconds.
    pub end_ms: u64,
    /// Per operation, the exchanges intended inside the window.
    pub operations: BTreeMap<OperationId, RawOperation>,
    /// The generator's lag per exchange, microseconds, encoded.
    pub lag_hdr: String,
    /// The in-flight count, sampled at a fixed interval.
    pub in_flight: Vec<u32>,
    /// The total time sends waited for an in-flight slot, milliseconds.
    pub budget_wait_ms: u64,
    /// Whether any send found the in-flight budget exhausted.
    pub budget_exhausted: bool,
}

impl RawWindow {
    /// Encodes one window the generator recorded.
    ///
    /// # Errors
    /// [`PerfResultError::Histogram`] if a histogram does not encode.
    pub fn from_record(record: &WindowRecord) -> Result<Self, PerfResultError> {
        let operations = record
            .operations
            .iter()
            .map(|(id, op)| Ok((id.clone(), RawOperation::from_record(op)?)))
            .collect::<Result<BTreeMap<_, _>, PerfResultError>>()?;
        Ok(Self {
            start_ms: millis(record.spec.start),
            end_ms: millis(record.spec.end),
            operations,
            lag_hdr: encode(&record.lag)?,
            in_flight: record.in_flight.clone(),
            budget_wait_ms: millis(record.budget_wait),
            budget_exhausted: record.budget_exhausted,
        })
    }
}

/// One plateau of the ladder: a planned rate, a ramp-in, and a hold
/// recorded as two halves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawPlateau {
    /// The planned arrival rate, per second.
    pub planned_rate: Rate,
    /// The ramp-in before the hold, milliseconds; never measured.
    pub ramp_in_ms: u64,
    /// The hold, milliseconds.
    pub hold_ms: u64,
    /// The hold's first and second halves.
    pub halves: [RawWindow; 2],
    /// The offset from the plateau's start at which a saturated generator
    /// stopped, when it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_at_ms: Option<u64>,
    /// The first failed exchanges per operation, at most
    /// [`FAILED_EXCHANGES_PER_OPERATION`] each.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub failed_exchanges: BTreeMap<OperationId, Vec<FailedExchange>>,
}

/// How many failed exchanges a plateau keeps per operation.
pub const FAILED_EXCHANGES_PER_OPERATION: usize = 16;

impl RawPlateau {
    /// Builds a plateau from the generator's two hold halves, keeping the
    /// first failed exchanges of the first half, then the second.
    ///
    /// # Errors
    /// [`PerfResultError::Histogram`] if a histogram does not encode.
    pub fn from_halves(
        planned_rate: Rate,
        ramp_in: Duration,
        halves: [&WindowRecord; 2],
        stopped_at: Option<Duration>,
    ) -> Result<Self, PerfResultError> {
        let [first, second] = halves;
        let mut failed_exchanges: BTreeMap<OperationId, Vec<FailedExchange>> = BTreeMap::new();
        for half in [first, second] {
            for (id, op) in &half.operations {
                let kept = failed_exchanges.entry(id.clone()).or_default();
                let room = FAILED_EXCHANGES_PER_OPERATION.saturating_sub(kept.len());
                kept.extend(op.samples.iter().take(room).cloned());
            }
        }
        failed_exchanges.retain(|_, kept| !kept.is_empty());
        Ok(Self {
            planned_rate,
            ramp_in_ms: millis(ramp_in),
            hold_ms: millis(second.spec.end.saturating_sub(first.spec.start)),
            halves: [
                RawWindow::from_record(first)?,
                RawWindow::from_record(second)?,
            ],
            stopped_at_ms: stopped_at.map(millis),
            failed_exchanges,
        })
    }
}

/// One window of the scout ramp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawScoutWindow {
    /// The planned rate at the window's midpoint, per second.
    pub planned_rate: Rate,
    /// The window's evidence.
    pub window: RawWindow,
}

/// The scout ramp's windows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawScout {
    /// The length of each window, milliseconds.
    pub window_ms: u64,
    /// The windows, in time order.
    pub windows: Vec<RawScoutWindow>,
}

/// The evidence part of a record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Raw {
    /// The instrument that wrote the record.
    pub instrument: Instrument,
    /// The server measured.
    pub target: Target,
    /// The deployment the IXIT declared.
    pub environment: Environment,
    /// The load generator.
    pub generator: Generator,
    /// The host prescription the run claims to have run on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_prescription: Option<String>,
    /// The deployment posture the run declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub posture: Option<PostureRecord>,
    /// The workload offered.
    pub workload: EmbeddedWorkload,
    /// The corpus measured against.
    pub corpus: CorpusRecord,
    /// The seed every draw derived from.
    pub seed: u64,
    /// The scout ramp, when the run made one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scout: Option<RawScout>,
    /// The plateaus, in the order they ran.
    pub plateaus: Vec<RawPlateau>,
}

/// A record that cannot be built, read or re-derived.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PerfResultError {
    /// A histogram did not encode or decode.
    #[error("{at}: histogram: {message}")]
    Histogram {
        /// The JSON pointer of the histogram, or what was being encoded.
        at: String,
        /// The codec's diagnostic.
        message: String,
    },
    /// The raw part contradicts itself.
    #[error("{at}: {message}")]
    Inconsistent {
        /// The JSON pointer of the inconsistent element.
        at: String,
        /// What is inconsistent.
        message: String,
    },
    /// The document is not JSON.
    #[error("JSON: {0}")]
    Json(String),
    /// The document breaks the published schema.
    #[error("schema: {0}")]
    Schema(String),
    /// The record could not be serialized.
    #[error("serialize: {0}")]
    Serialize(String),
}

/// Whole milliseconds in `duration`, saturating.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn encode(histogram: &Histogram<u64>) -> Result<String, PerfResultError> {
    encode_hdr_v2(histogram).map_err(|message| PerfResultError::Histogram {
        at: "encode".to_owned(),
        message,
    })
}

/// Decodes the histogram at JSON pointer `at`.
fn decode(encoded: &str, at: &str) -> Result<Histogram<u64>, PerfResultError> {
    decode_hdr_v2(encoded).map_err(|message| PerfResultError::Histogram {
        at: at.to_owned(),
        message,
    })
}
