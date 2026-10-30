// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The derived part of a `perf-result`, computed from the raw part alone.
//!
//! Every function here is pure over the raw part, so [`derive()`] run twice on
//! one record gives the same bytes, and the judge reproduces a record by
//! running it again.

use std::collections::BTreeMap;

use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};

use crate::perf_gen::drive::latency_histogram;
use crate::perf_gen::{ErrorClass, OperationId};
use crate::perf_result::quantile::{Quantile, QuantileName, quantile, u64_to_f64, us_to_ms};
use crate::perf_result::{PerfResultError, Raw, RawWindow, SCOPE, decode};

/// The half-to-half ratio band outside which a plateau is unsteady.
const STEADY_LOW: f64 = 0.8;
/// The upper end of the steady band.
const STEADY_HIGH: f64 = 1.25;

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
}

/// One operation within a plateau.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedOperation {
    /// Exchanges intended in the hold, answered or not.
    pub requests: u64,
    /// The exchanges that succeeded.
    pub succeeded: u64,
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
    /// Failed exchanges by class.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub errors: BTreeMap<ErrorClass, u64>,
    /// Failed over all exchanges; absent with no exchange.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_share: Option<f64>,
    /// Mean exchanges in flight by Little's law: offered rate times the mean
    /// latency of every exchange.
    pub mean_in_flight: f64,
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

/// One plateau, derived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedPlateau {
    /// The planned arrival rate, per second.
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
}

/// One scout window, derived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedScoutWindow {
    /// The window's first offset, milliseconds.
    pub start_ms: u64,
    /// The planned rate at its midpoint, per second.
    pub planned_rate: f64,
    /// Throughput and errors in the window.
    pub throughput: Throughput,
    /// Latency of every successful exchange in the window.
    pub latency: Latency,
    /// The generator's reading.
    pub generator: GeneratorReading,
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
    /// The scout windows, when the run made a scout.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scout: Vec<DerivedScoutWindow>,
    /// The plateaus, in the order they ran.
    pub plateaus: Vec<DerivedPlateau>,
}

// TODO(#886): the knee bracket, the peak achieved throughput, the USL fit and the population reading.
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
            },
        );
    }
    Ok(decoded)
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

fn throughput(
    ok: &Histogram<u64>,
    failed: &Histogram<u64>,
    errors: &BTreeMap<ErrorClass, u64>,
    seconds: f64,
) -> Throughput {
    let succeeded = ok.len();
    let requests = succeeded.saturating_add(failed.len());
    let per_s = |count: u64| {
        if seconds > 0.0 {
            u64_to_f64(count) / seconds
        } else {
            0.0
        }
    };
    let total_us = sum_us(ok).saturating_add(sum_us(failed));
    let mean_s = if requests > 0 {
        let total = u64::try_from(total_us).unwrap_or(u64::MAX);
        u64_to_f64(total) / u64_to_f64(requests) / 1_000_000.0
    } else {
        0.0
    };
    Throughput {
        offered_per_s: per_s(requests),
        achieved_per_s: per_s(succeeded),
        requests,
        succeeded,
        errors: errors.clone(),
        error_share: share(failed.len(), requests),
        mean_in_flight: per_s(requests) * mean_s,
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

fn derive_plateau(raw: &Raw, index: usize) -> Result<DerivedPlateau, PerfResultError> {
    let at = format!("/raw/plateaus/{index}");
    let plateau = raw
        .plateaus
        .get(index)
        .ok_or_else(|| PerfResultError::Inconsistent {
            at: at.clone(),
            message: "no such plateau".to_owned(),
        })?;
    let [first_raw, second_raw] = &plateau.halves;
    let first = decode_window(first_raw, &format!("{at}/halves/0"))?;
    let second = decode_window(second_raw, &format!("{at}/halves/1"))?;

    let mut ok = empty()?;
    let mut failed = empty()?;
    let mut lag = empty()?;
    let mut errors = BTreeMap::new();
    let mut operations: BTreeMap<OperationId, DecodedOperation> = BTreeMap::new();
    for half in [&first, &second] {
        add(&mut ok, &half.all_ok, &at)?;
        add(&mut failed, &half.all_failed, &at)?;
        add(&mut lag, &half.lag, &at)?;
        for (id, op) in &half.per_operation {
            add_counts(&mut errors, &op.errors);
            let merged = match operations.entry(id.clone()) {
                std::collections::btree_map::Entry::Occupied(slot) => slot.into_mut(),
                std::collections::btree_map::Entry::Vacant(slot) => slot.insert(DecodedOperation {
                    ok: empty()?,
                    failed: empty()?,
                    errors: BTreeMap::new(),
                }),
            };
            add(&mut merged.ok, &op.ok, &at)?;
            add(&mut merged.failed, &op.failed, &at)?;
            add_counts(&mut merged.errors, &op.errors);
        }
    }
    let hold_s = u64_to_f64(plateau.hold_ms) / 1_000.0;
    let throughput = throughput(&ok, &failed, &errors, hold_s);
    let operations = operations
        .into_iter()
        .map(|(id, op)| {
            let requests = op.ok.len().saturating_add(op.failed.len());
            let latency = Latency::of(&op.ok);
            let judged_at = JudgedAt::of(&latency);
            let derived = DerivedOperation {
                requests,
                succeeded: op.ok.len(),
                error_share: share(op.failed.len(), requests),
                errors: op.errors,
                share: share(requests, throughput.requests).unwrap_or(0.0),
                latency,
                judged_at,
            };
            (id, derived)
        })
        .collect();
    let budget_exhausted = first_raw.budget_exhausted || second_raw.budget_exhausted;
    Ok(DerivedPlateau {
        planned_rate: plateau.planned_rate.per_second(),
        hold_s,
        latency: Latency::of(&ok),
        throughput,
        operations,
        halves: compare_halves(&first.all_ok, &second.all_ok),
        generator: generator_reading(&lag, budget_exhausted, raw.generator.lag_limit_us),
        stopped: plateau.stopped_at_ms.is_some(),
    })
}

fn derive_scout_window(raw: &Raw, index: usize) -> Result<DerivedScoutWindow, PerfResultError> {
    let at = format!("/raw/scout/windows/{index}");
    let scout = raw
        .scout
        .as_ref()
        .and_then(|s| s.windows.get(index))
        .ok_or_else(|| PerfResultError::Inconsistent {
            at: at.clone(),
            message: "no such scout window".to_owned(),
        })?;
    let decoded = decode_window(&scout.window, &format!("{at}/window"))?;
    let mut errors = BTreeMap::new();
    for op in decoded.per_operation.values() {
        add_counts(&mut errors, &op.errors);
    }
    let seconds = u64_to_f64(scout.window.end_ms.saturating_sub(scout.window.start_ms)) / 1_000.0;
    Ok(DerivedScoutWindow {
        start_ms: scout.window.start_ms,
        planned_rate: scout.planned_rate.per_second(),
        throughput: throughput(&decoded.all_ok, &decoded.all_failed, &errors, seconds),
        latency: Latency::of(&decoded.all_ok),
        generator: generator_reading(
            &decoded.lag,
            scout.window.budget_exhausted,
            raw.generator.lag_limit_us,
        ),
    })
}

/// Derives every number of a record from its raw part.
///
/// # Errors
/// [`PerfResultError::Histogram`] for a histogram that does not decode or
/// add, and [`PerfResultError::Inconsistent`] for error counts that
/// disagree with their failed histogram, each naming the JSON pointer.
pub fn derive(raw: &Raw) -> Result<Derived, PerfResultError> {
    let scout_windows = raw.scout.as_ref().map_or(0, |s| s.windows.len());
    Ok(Derived {
        passes: Passes,
        between_run_noise: BetweenRunNoise::Unmeasured,
        scope: SCOPE.to_owned(),
        scout: (0..scout_windows)
            .map(|i| derive_scout_window(raw, i))
            .collect::<Result<_, _>>()?,
        plateaus: (0..raw.plateaus.len())
            .map(|i| derive_plateau(raw, i))
            .collect::<Result<_, _>>()?,
    })
}
