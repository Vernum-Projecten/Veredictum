// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The derived part of a `perf-result`, computed from the raw part alone.
//!
//! Every function here is pure over the raw part, so [`derive()`] run twice on
//! one record gives the same bytes, and the judge reproduces a record by
//! running it again. The run itself calls [`plateau`] and [`scout_estimate`]
//! to decide its own next phase, so the decision a run took and the one its
//! record re-derives are the same computation.

use std::collections::BTreeMap;

use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};

use crate::perf_gen::drive::latency_histogram;
use crate::perf_gen::{ErrorClass, OperationId};
use crate::perf_result::capacity::{
    Knee, Peak, PopulationReading, UslReading, fit_usl, knee, peak, population, usl_points,
};
use crate::perf_result::quantile::{Quantile, QuantileName, quantile, u64_to_f64, us_to_ms};
use crate::perf_result::{
    LoadModel, PerfResultError, Raw, RawPlateau, RawQualification, RawScout, RawWindow, SCOPE,
    decode, is_zero,
};

/// The half-to-half ratio band outside which a plateau is unsteady.
const STEADY_LOW: f64 = 0.8;
/// The upper end of the steady band.
const STEADY_HIGH: f64 = 1.25;

/// Millionths in a whole.
const PPM: u128 = 1_000_000;

/// The number of passes a single run makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Passes;

impl TryFrom<u32> for Passes {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == 1 {
            Ok(Self)
        } else {
            Err(format!("a perf-result records one pass, not {value}"))
        }
    }
}

impl From<Passes> for u32 {
    fn from(_: Passes) -> Self {
        1
    }
}

/// What the record knows about the noise between runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BetweenRunNoise {
    /// One pass measures sampling intervals only; no A/A floor is attached.
    Unmeasured,
}

/// The latency summary of one histogram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Latency {
    /// The median.
    pub p50: Quantile,
    /// The 90th percentile.
    pub p90: Quantile,
    /// The 95th percentile.
    pub p95: Quantile,
    /// The 99th percentile.
    pub p99: Quantile,
    /// The 99.9th percentile.
    pub p99_9: Quantile,
    /// The largest latency, milliseconds; absent with no sample.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ms: Option<f64>,
}

impl Latency {
    fn of(histogram: &Histogram<u64>) -> Self {
        Self {
            p50: quantile(histogram, QuantileName::P50),
            p90: quantile(histogram, QuantileName::P90),
            p95: quantile(histogram, QuantileName::P95),
            p99: quantile(histogram, QuantileName::P99),
            p99_9: quantile(histogram, QuantileName::P999),
            max_ms: (!histogram.is_empty()).then(|| us_to_ms(histogram.max())),
        }
    }

    /// The quantile `name`.
    #[must_use]
    pub fn get(&self, name: QuantileName) -> &Quantile {
        match name {
            QuantileName::P50 => &self.p50,
            QuantileName::P90 => &self.p90,
            QuantileName::P95 => &self.p95,
            QuantileName::P99 => &self.p99,
            QuantileName::P999 => &self.p99_9,
        }
    }
}

/// The quantile an operation is judged at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgedAt {
    /// The 99th percentile.
    P99,
    /// The 95th percentile.
    P95,
    /// The 90th percentile.
    P90,
    /// The median.
    P50,
    /// No quantile resolves.
    None,
}

impl JudgedAt {
    /// The highest of p99, p95, p90 and p50 that `latency` resolves.
    ///
    /// The ladder stops at p99, the quantile the latency budget is stated at,
    /// so a large sample is never judged at a stricter quantile than that.
    #[must_use]
    pub fn of(latency: &Latency) -> Self {
        [
            (QuantileName::P99, Self::P99),
            (QuantileName::P95, Self::P95),
            (QuantileName::P90, Self::P90),
            (QuantileName::P50, Self::P50),
        ]
        .into_iter()
        .find(|(name, _)| latency.get(*name).is_resolved())
        .map_or(Self::None, |(_, judged)| judged)
    }

    /// The quantile this names, or `None` when nothing resolves.
    #[must_use]
    pub fn quantile(self) -> Option<QuantileName> {
        match self {
            Self::P99 => Some(QuantileName::P99),
            Self::P95 => Some(QuantileName::P95),
            Self::P90 => Some(QuantileName::P90),
            Self::P50 => Some(QuantileName::P50),
            Self::None => None,
        }
    }
}

/// One operation within a plateau.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedOperation {
    /// Exchanges intended in the hold, answered or not.
    pub requests: u64,
    /// The exchanges that succeeded.
    pub succeeded: u64,
    /// Refusals of an optional operation, counted apart from errors.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub refused: u64,
    /// Failed exchanges by class.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub errors: BTreeMap<ErrorClass, u64>,
    /// Failed over all exchanges; absent with no exchange.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_share: Option<f64>,
    /// This operation's exchanges over the plateau's.
    pub share: f64,
    /// Latency of the successful exchanges, both halves summed.
    pub latency: Latency,
    /// The quantile the operation is judged at.
    pub judged_at: JudgedAt,
    /// The latency budget at that quantile, milliseconds.
    pub budget_ms: u32,
}

/// Whether a plateau held still across its two halves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Steadiness {
    /// Both ratios lie within 0.8 to 1.25.
    Steady,
    /// A ratio lies outside 0.8 to 1.25.
    Unsteady,
    /// A half resolves neither its p50 nor its p95, so no ratio exists.
    NotMeasurable,
}

/// The second half's overall p50 and p95 over the first's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HalfComparison {
    /// The p50 ratio, when both halves resolve it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p50_ratio: Option<f64>,
    /// The p95 ratio, when both halves resolve it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p95_ratio: Option<f64>,
    /// The reading of the two ratios.
    pub steadiness: Steadiness,
}

/// The numbers of one window or plateau.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Throughput {
    /// Exchanges intended per second of measured time.
    pub offered_per_s: f64,
    /// Successful exchanges per second of measured time.
    pub achieved_per_s: f64,
    /// Exchanges intended, answered or not.
    pub requests: u64,
    /// The exchanges that succeeded.
    pub succeeded: u64,
    /// Refusals of an optional operation, counted apart from errors.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub refused: u64,
    /// Failed exchanges by class.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub errors: BTreeMap<ErrorClass, u64>,
    /// Failed over all exchanges; absent with no exchange.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_share: Option<f64>,
    /// Mean exchanges in flight by Little's law: the rate of the timed
    /// exchanges times their mean latency.
    pub mean_in_flight: f64,
}

impl Throughput {
    /// The failed exchanges: every request neither succeeded nor refused.
    #[must_use]
    pub fn failed(&self) -> u64 {
        self.requests
            .saturating_sub(self.succeeded)
            .saturating_sub(self.refused)
    }
}

/// The generator's own reading of a window or plateau.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratorReading {
    /// The lag p99, milliseconds; absent with no send.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lag_p99_ms: Option<f64>,
    /// The largest lag, milliseconds; absent with no send.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lag_max_ms: Option<f64>,
    /// Whether any send found the in-flight budget exhausted.
    pub budget_exhausted: bool,
    /// Whether the lag p99 exceeded the limit while the budget never ran
    /// out: the generator, not the server, failed to hold the rate.
    pub generator_bound: bool,
}

/// One criterion a plateau or scout window fell short of (a closed
/// vocabulary).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "criterion", rename_all = "snake_case", deny_unknown_fields)]
pub enum Shortfall {
    /// Nothing was sent, so nothing qualifies.
    NoExchanges,
    /// The successes fell below the lowest achieved share of the answered
    /// exchanges.
    Achieved {
        /// Successes over the exchanges not refused.
        share: f64,
    },
    /// The failures rose above the highest error share.
    ErrorShare {
        /// Failures over every exchange.
        share: f64,
    },
    /// An operation exceeded its budget at the quantile it is judged at.
    Latency {
        /// The operation.
        operation: OperationId,
        /// The quantile it was judged at.
        judged_at: JudgedAt,
        /// Its value, milliseconds.
        value_ms: f64,
        /// The budget, milliseconds.
        budget_ms: u32,
    },
    /// The generator, not the server, failed to hold the rate.
    GeneratorBound,
    /// A saturated generator stopped inside it.
    Stopped,
}

/// The qualification of one plateau or scout window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Qualification {
    /// Whether every criterion held.
    pub qualified: bool,
    /// Each criterion that did not, in a fixed order: exchanges, achieved,
    /// errors, latency by operation, generator, stop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shortfalls: Vec<Shortfall>,
}

/// One plateau, derived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedPlateau {
    /// The planned operation arrival rate, per second.
    pub planned_rate: f64,
    /// The hold, seconds.
    pub hold_s: f64,
    /// Throughput and errors over the hold.
    pub throughput: Throughput,
    /// Latency of every successful exchange, all operations and both
    /// halves summed.
    pub latency: Latency,
    /// Per operation.
    pub operations: BTreeMap<OperationId, DerivedOperation>,
    /// The half-to-half comparison.
    pub halves: HalfComparison,
    /// The generator's reading.
    pub generator: GeneratorReading,
    /// Whether a saturated generator stopped inside the plateau.
    pub stopped: bool,
    /// The plateau's qualification.
    pub qualification: Qualification,
}

/// One scout window, derived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedScoutWindow {
    /// The window's first offset, milliseconds.
    pub start_ms: u64,
    /// The planned operation rate at its midpoint, per second.
    pub planned_rate: f64,
    /// Throughput and errors in the window.
    pub throughput: Throughput,
    /// Latency of every successful exchange in the window.
    pub latency: Latency,
    /// The generator's reading.
    pub generator: GeneratorReading,
    /// The window's qualification, judged like a plateau's.
    pub qualification: Qualification,
}

/// The knee estimate K̂ the scout gives, which sets the ladder's rates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoutEstimate {
    /// K̂: the planned operation rate of the first breaching window at its
    /// midpoint, or the ramp's end rate when no window breached.
    pub rate: f64,
    /// The first breaching window; absent when the ramp ended unbreached,
    /// so K̂ is a lower bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breached_window: Option<usize>,
}

/// The seeding's own reading, apart from every measured number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedingReading {
    /// Always `closed_loop`: the server set the seeding's pace.
    pub model: LoadModel,
    /// Requests per second of seeding wall time; absent for no wall time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requests_per_s: Option<f64>,
}

/// The derived part of a record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Derived {
    /// Always `1`: one run is one pass.
    pub passes: Passes,
    /// What the record knows about the noise between runs.
    pub between_run_noise: BetweenRunNoise,
    /// What a single run cannot show.
    pub scope: String,
    /// The seeding, labelled closed-loop.
    pub seeding: SeedingReading,
    /// The scout windows, when the run made a scout.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scout: Vec<DerivedScoutWindow>,
    /// The knee estimate, when the run made a scout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scout_estimate: Option<ScoutEstimate>,
    /// The plateaus, in the order they ran.
    pub plateaus: Vec<DerivedPlateau>,
    /// The knee: a bracket, a lower bound, or below the ladder.
    pub knee: Knee,
    /// The highest achieved throughput, whatever its latency; absent when
    /// nothing succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak: Option<Peak>,
    /// The Universal Scalability Law fitted to the scout windows and the
    /// plateaus: an explanatory model, never a verdict.
    pub usl: UslReading,
    /// The population the knee could serve at its peak.
    pub population: PopulationReading,
}
// TODO(#890): an attached A/A floor as a second state of `between_run_noise`.

/// Everything one window holds, decoded and summed over its operations.
struct Decoded {
    per_operation: BTreeMap<OperationId, DecodedOperation>,
    all_ok: Histogram<u64>,
    all_failed: Histogram<u64>,
    lag: Histogram<u64>,
}

struct DecodedOperation {
    ok: Histogram<u64>,
    failed: Histogram<u64>,
    errors: BTreeMap<ErrorClass, u64>,
    refused: u64,
}

impl DecodedOperation {
    fn empty() -> Result<Self, PerfResultError> {
        Ok(Self {
            ok: empty()?,
            failed: empty()?,
            errors: BTreeMap::new(),
            refused: 0,
        })
    }

    fn requests(&self) -> u64 {
        self.ok
            .len()
            .saturating_add(self.failed.len())
            .saturating_add(self.refused)
    }
}

fn empty() -> Result<Histogram<u64>, PerfResultError> {
    latency_histogram().map_err(|e| PerfResultError::Histogram {
        at: "new".to_owned(),
        message: e.to_string(),
    })
}

fn add(into: &mut Histogram<u64>, from: &Histogram<u64>, at: &str) -> Result<(), PerfResultError> {
    into.add(from).map_err(|e| PerfResultError::Histogram {
        at: at.to_owned(),
        message: e.to_string(),
    })
}

fn add_counts(into: &mut BTreeMap<ErrorClass, u64>, from: &BTreeMap<ErrorClass, u64>) {
    for (class, count) in from {
        let slot = into.entry(*class).or_insert(0);
        *slot = slot.saturating_add(*count);
    }
}

fn decode_window(window: &RawWindow, at: &str) -> Result<Decoded, PerfResultError> {
    let mut decoded = Decoded {
        per_operation: BTreeMap::new(),
        all_ok: empty()?,
        all_failed: empty()?,
        lag: decode(&window.lag_hdr, &format!("{at}/lag_hdr"))?,
    };
    for (id, op) in &window.operations {
        let here = format!("{at}/operations/{id}");
        let ok = decode(&op.ok_hdr, &format!("{here}/ok_hdr"))?;
        let failed = match &op.failed_hdr {
            Some(encoded) => decode(encoded, &format!("{here}/failed_hdr"))?,
            None => empty()?,
        };
        let counted: u64 = op.errors.values().copied().fold(0, u64::saturating_add);
        if counted != failed.len() {
            return Err(PerfResultError::Inconsistent {
                at: format!("{here}/errors"),
                message: format!(
                    "the error counts sum to {counted} but the failed histogram holds {}",
                    failed.len()
                ),
            });
        }
        add(&mut decoded.all_ok, &ok, &here)?;
        add(&mut decoded.all_failed, &failed, &here)?;
        let _replaced = decoded.per_operation.insert(
            id.clone(),
            DecodedOperation {
                ok,
                failed,
                errors: op.errors.clone(),
                refused: op.refused,
            },
        );
    }
    Ok(decoded)
}

/// Sums decoded windows per operation.
fn merge_operations<'a>(
    windows: impl IntoIterator<Item = &'a Decoded>,
    at: &str,
) -> Result<BTreeMap<OperationId, DecodedOperation>, PerfResultError> {
    let mut operations: BTreeMap<OperationId, DecodedOperation> = BTreeMap::new();
    for window in windows {
        for (id, op) in &window.per_operation {
            let merged = match operations.entry(id.clone()) {
                std::collections::btree_map::Entry::Occupied(slot) => slot.into_mut(),
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(DecodedOperation::empty()?)
                }
            };
            add(&mut merged.ok, &op.ok, at)?;
            add(&mut merged.failed, &op.failed, at)?;
            add_counts(&mut merged.errors, &op.errors);
            merged.refused = merged.refused.saturating_add(op.refused);
        }
    }
    Ok(operations)
}

/// `numerator / denominator`, absent for a zero denominator.
fn share(numerator: u64, denominator: u64) -> Option<f64> {
    (denominator > 0).then(|| u64_to_f64(numerator) / u64_to_f64(denominator))
}

/// The exact sum of a histogram's values, microseconds, each sample read at
/// its bucket's median-equivalent value.
fn sum_us(histogram: &Histogram<u64>) -> u128 {
    histogram
        .iter_recorded()
        .map(|step| {
            u128::from(histogram.median_equivalent(step.value_iterated_to()))
                * u128::from(step.count_at_value())
        })
        .sum()
}

/// The throughput of the operations `ops` over `seconds`.
fn throughput(ops: &BTreeMap<OperationId, DecodedOperation>, seconds: f64) -> Throughput {
    let mut succeeded: u64 = 0;
    let mut failed: u64 = 0;
    let mut refused: u64 = 0;
    let mut total_us: u128 = 0;
    let mut errors = BTreeMap::new();
    for op in ops.values() {
        succeeded = succeeded.saturating_add(op.ok.len());
        failed = failed.saturating_add(op.failed.len());
        refused = refused.saturating_add(op.refused);
        total_us = total_us
            .saturating_add(sum_us(&op.ok))
            .saturating_add(sum_us(&op.failed));
        add_counts(&mut errors, &op.errors);
    }
    let timed = succeeded.saturating_add(failed);
    let requests = timed.saturating_add(refused);
    let per_s = |count: u64| {
        if seconds > 0.0 {
            u64_to_f64(count) / seconds
        } else {
            0.0
        }
    };
    let mean_s = if timed > 0 {
        let total = u64::try_from(total_us).unwrap_or(u64::MAX);
        u64_to_f64(total) / u64_to_f64(timed) / 1_000_000.0
    } else {
        0.0
    };
    Throughput {
        offered_per_s: per_s(requests),
        achieved_per_s: per_s(succeeded),
        requests,
        succeeded,
        refused,
        errors,
        error_share: share(failed, requests),
        mean_in_flight: per_s(timed) * mean_s,
    }
}

fn generator_reading(
    lag: &Histogram<u64>,
    budget_exhausted: bool,
    lag_limit_us: u64,
) -> GeneratorReading {
    let (lag_p99_ms, lag_max_ms, p99_us) = if lag.is_empty() {
        (None, None, 0)
    } else {
        let p99 = lag.value_at_quantile(0.99);
        (Some(us_to_ms(p99)), Some(us_to_ms(lag.max())), p99)
    };
    GeneratorReading {
        lag_p99_ms,
        lag_max_ms,
        budget_exhausted,
        generator_bound: !budget_exhausted && p99_us > lag_limit_us,
    }
}

fn ratio(first: &Quantile, second: &Quantile) -> Option<f64> {
    match (first.value_ms(), second.value_ms()) {
        (Some(a), Some(b)) if a > 0.0 => Some(b / a),
        _ => None,
    }
}

fn compare_halves(first: &Histogram<u64>, second: &Histogram<u64>) -> HalfComparison {
    let p50_ratio = ratio(
        &quantile(first, QuantileName::P50),
        &quantile(second, QuantileName::P50),
    );
    let p95_ratio = ratio(
        &quantile(first, QuantileName::P95),
        &quantile(second, QuantileName::P95),
    );
    let measured: Vec<f64> = [p50_ratio, p95_ratio].into_iter().flatten().collect();
    let steadiness = if measured.is_empty() {
        Steadiness::NotMeasurable
    } else if measured
        .iter()
        .all(|r| (STEADY_LOW..=STEADY_HIGH).contains(r))
    {
        Steadiness::Steady
    } else {
        Steadiness::Unsteady
    };
    HalfComparison {
        p50_ratio,
        p95_ratio,
        steadiness,
    }
}

/// The per-operation numbers of a plateau or a window.
fn operations(
    decoded: BTreeMap<OperationId, DecodedOperation>,
    total_requests: u64,
    q: &RawQualification,
) -> BTreeMap<OperationId, DerivedOperation> {
    decoded
        .into_iter()
        .map(|(id, op)| {
            let requests = op.requests();
            let latency = Latency::of(&op.ok);
            let judged_at = JudgedAt::of(&latency);
            let budget_ms = q.budget_ms(&id);
            let derived = DerivedOperation {
                requests,
                succeeded: op.ok.len(),
                refused: op.refused,
                error_share: share(op.failed.len(), requests),
                errors: op.errors,
                share: share(requests, total_requests).unwrap_or(0.0),
                latency,
                judged_at,
                budget_ms,
            };
            (id, derived)
        })
        .collect()
}

/// Judges a plateau or a window against the run's limits.
///
/// The share criteria are integer comparisons in millionths. A refusal
/// counted apart is neither a success nor a failure, so the achieved share
/// is taken over the exchanges not refused. An operation whose latency
/// resolves no quantile is judged on its errors alone.
#[must_use]
pub fn qualify(
    throughput: &Throughput,
    operations: &BTreeMap<OperationId, DerivedOperation>,
    generator: &GeneratorReading,
    stopped: bool,
    q: &RawQualification,
) -> Qualification {
    let mut shortfalls = Vec::new();
    let answered = throughput.requests.saturating_sub(throughput.refused);
    if throughput.requests == 0 {
        shortfalls.push(Shortfall::NoExchanges);
    }
    if answered > 0
        && u128::from(throughput.succeeded) * PPM
            < u128::from(q.min_achieved_ppm) * u128::from(answered)
    {
        shortfalls.push(Shortfall::Achieved {
            share: share(throughput.succeeded, answered).unwrap_or(0.0),
        });
    }
    let failed = throughput.failed();
    if u128::from(failed) * PPM > u128::from(q.max_error_ppm) * u128::from(throughput.requests) {
        shortfalls.push(Shortfall::ErrorShare {
            share: share(failed, throughput.requests).unwrap_or(0.0),
        });
    }
    for (id, op) in operations {
        let Some(name) = op.judged_at.quantile() else {
            continue;
        };
        if let Some(value_ms) = op.latency.get(name).value_ms()
            && value_ms > f64::from(op.budget_ms)
        {
            shortfalls.push(Shortfall::Latency {
                operation: id.clone(),
                judged_at: op.judged_at,
                value_ms,
                budget_ms: op.budget_ms,
            });
        }
    }
    if generator.generator_bound {
        shortfalls.push(Shortfall::GeneratorBound);
    }
    if stopped {
        shortfalls.push(Shortfall::Stopped);
    }
    Qualification {
        qualified: shortfalls.is_empty(),
        shortfalls,
    }
}

/// Derives one plateau of a run judged against `q`.
///
/// `at` is the plateau's JSON pointer, named in every error.
///
/// # Errors
/// [`PerfResultError::Histogram`] for a histogram that does not decode or
/// add, and [`PerfResultError::Inconsistent`] for error counts that
/// disagree with their failed histogram.
pub fn plateau(
    plateau: &RawPlateau,
    q: &RawQualification,
    lag_limit_us: u64,
    at: &str,
) -> Result<DerivedPlateau, PerfResultError> {
    let [first_raw, second_raw] = &plateau.halves;
    let first = decode_window(first_raw, &format!("{at}/halves/0"))?;
    let second = decode_window(second_raw, &format!("{at}/halves/1"))?;
    let mut ok = empty()?;
    let mut lag = empty()?;
    for half in [&first, &second] {
        add(&mut ok, &half.all_ok, at)?;
        add(&mut lag, &half.lag, at)?;
    }
    let decoded = merge_operations([&first, &second], at)?;
    let hold_s = u64_to_f64(plateau.hold_ms) / 1_000.0;
    let throughput = throughput(&decoded, hold_s);
    let operations = operations(decoded, throughput.requests, q);
    let budget_exhausted = first_raw.budget_exhausted || second_raw.budget_exhausted;
    let generator = generator_reading(&lag, budget_exhausted, lag_limit_us);
    let stopped = plateau.stopped_at_ms.is_some();
    let qualification = qualify(&throughput, &operations, &generator, stopped, q);
    Ok(DerivedPlateau {
        planned_rate: plateau.planned_rate.per_second(),
        hold_s,
        latency: Latency::of(&ok),
        throughput,
        operations,
        halves: compare_halves(&first.all_ok, &second.all_ok),
        generator,
        stopped,
        qualification,
    })
}

/// Derives every window of a scout judged against `q`.
///
/// # Errors
/// As [`plateau`].
pub fn scout_windows(
    scout: &RawScout,
    q: &RawQualification,
    lag_limit_us: u64,
) -> Result<Vec<DerivedScoutWindow>, PerfResultError> {
    let mut derived = Vec::with_capacity(scout.windows.len());
    for (index, window) in scout.windows.iter().enumerate() {
        let at = format!("/raw/scout/windows/{index}/window");
        let decoded = decode_window(&window.window, &at)?;
        let seconds =
            u64_to_f64(window.window.end_ms.saturating_sub(window.window.start_ms)) / 1_000.0;
        let latency = Latency::of(&decoded.all_ok);
        let generator =
            generator_reading(&decoded.lag, window.window.budget_exhausted, lag_limit_us);
        let per_operation = decoded.per_operation;
        let throughput = throughput(&per_operation, seconds);
        let operations = operations(per_operation, throughput.requests, q);
        let qualification = qualify(&throughput, &operations, &generator, false, q);
        derived.push(DerivedScoutWindow {
            start_ms: window.window.start_ms,
            planned_rate: window.planned_rate.per_second(),
            throughput,
            latency,
            generator,
            qualification,
        });
    }
    Ok(derived)
}

/// The knee estimate a scout gives: the first window that fails its
/// qualification, or that the run halted in or before, gives K̂ at its
/// midpoint rate; a ramp that ends unbreached gives its end rate.
#[must_use]
pub fn scout_estimate(scout: &RawScout, windows: &[DerivedScoutWindow]) -> ScoutEstimate {
    let halted = |index: usize| {
        scout.halted_at_ms.is_some_and(|halt| {
            scout
                .windows
                .get(index)
                .is_some_and(|w| w.window.end_ms > halt)
        })
    };
    let breach = windows
        .iter()
        .enumerate()
        .find(|(index, window)| !window.qualification.qualified || halted(*index));
    match breach {
        Some((index, window)) => ScoutEstimate {
            rate: window.planned_rate,
            breached_window: Some(index),
        },
        None => ScoutEstimate {
            rate: scout.end_rate.per_second(),
            breached_window: None,
        },
    }
}

/// Derives every number of a record from its raw part.
///
/// # Errors
/// [`PerfResultError::Histogram`] for a histogram that does not decode or
/// add, and [`PerfResultError::Inconsistent`] for error counts that
/// disagree with their failed histogram, each naming the JSON pointer.
pub fn derive(raw: &Raw) -> Result<Derived, PerfResultError> {
    let lag_limit = raw.generator.lag_limit_us;
    let (scout, scout_estimate) = match &raw.scout {
        Some(scout) => {
            let windows = scout_windows(scout, &raw.qualification, lag_limit)?;
            let estimate = scout_estimate(scout, &windows);
            (windows, Some(estimate))
        }
        None => (Vec::new(), None),
    };
    let plateaus: Vec<DerivedPlateau> = raw
        .plateaus
        .iter()
        .enumerate()
        .map(|(i, p)| {
            plateau(
                p,
                &raw.qualification,
                lag_limit,
                &format!("/raw/plateaus/{i}"),
            )
        })
        .collect::<Result<_, _>>()?;
    let seconds = u64_to_f64(raw.seeding.wall_ms) / 1_000.0;
    let knee = knee(&plateaus);
    let peak = peak(&plateaus, &scout);
    let usl = fit_usl(&usl_points(&plateaus, &scout));
    let population = population(&knee, raw.corpus.scale, &raw.environment);
    Ok(Derived {
        passes: Passes,
        between_run_noise: BetweenRunNoise::Unmeasured,
        scope: SCOPE.to_owned(),
        seeding: SeedingReading {
            model: raw.seeding.model,
            requests_per_s: (seconds > 0.0).then(|| u64_to_f64(raw.seeding.requests) / seconds),
        },
        scout,
        scout_estimate,
        plateaus,
        knee,
        peak,
        usl,
        population,
    })
}
