// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Quantiles read off a histogram by rank, each with its distribution-free
//! 95 % interval and the honesty bound that withholds it when too few
//! samples lie above it.
//!
//! The point estimate of quantile `q` over `n` samples is the order statistic
//! of rank `⌈q·n⌉`. Its interval is the pair of order statistics of ranks
//! `⌊nq − η·√(nq(1−q))⌋` and `⌈nq + η·√(nq(1−q))⌉` with `η = 1.96`, the
//! large-sample binomial bound on the ranks (Le Boudec, "Performance
//! Evaluation of Computer and Communication Systems", EPFL Press 2010,
//! §2.2). Ranks are integer arithmetic; only the square root is floating
//! point, and IEEE 754 rounds it correctly, so the ranks are reproducible.

use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};

/// The fewest samples above a quantile for it to be reported.
pub const MIN_SAMPLES_ABOVE: u64 = 10;

/// The normal quantile of a two-sided 95 % interval.
const ETA: f64 = 1.96;

/// The quantiles a record reports (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum QuantileName {
    /// The median.
    #[serde(rename = "p50")]
    P50,
    /// The 90th percentile.
    #[serde(rename = "p90")]
    P90,
    /// The 95th percentile.
    #[serde(rename = "p95")]
    P95,
    /// The 99th percentile.
    #[serde(rename = "p99")]
    P99,
    /// The 99.9th percentile.
    #[serde(rename = "p99_9")]
    P999,
}

impl QuantileName {
    /// Every reported quantile, lowest first.
    pub const ALL: &[QuantileName] = &[
        QuantileName::P50,
        QuantileName::P90,
        QuantileName::P95,
        QuantileName::P99,
        QuantileName::P999,
    ];

    /// The quantile as an exact fraction, numerator over denominator.
    #[must_use]
    pub fn fraction(self) -> (u64, u64) {
        match self {
            Self::P50 => (50, 100),
            Self::P90 => (90, 100),
            Self::P95 => (95, 100),
            Self::P99 => (99, 100),
            Self::P999 => (999, 1_000),
        }
    }

    /// The serialized token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::P50 => "p50",
            Self::P90 => "p90",
            Self::P95 => "p95",
            Self::P99 => "p99",
            Self::P999 => "p99_9",
        }
    }
}

/// One reported quantile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Quantile {
    /// Enough samples lie above the quantile to report it with its interval.
    Resolved {
        /// The point estimate, milliseconds.
        value_ms: f64,
        /// The rank of the point estimate, from one.
        rank: u64,
        /// The interval's lower bound, milliseconds.
        low_ms: f64,
        /// The rank of the lower bound.
        low_rank: u64,
        /// The interval's upper bound, milliseconds.
        high_ms: f64,
        /// The rank of the upper bound.
        high_rank: u64,
    },
    /// Fewer than [`MIN_SAMPLES_ABOVE`] samples lie above the quantile.
    NotResolvable {
        /// How many samples lie above it.
        samples_above: u64,
    },
}

impl Quantile {
    /// Whether the quantile is reported.
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved { .. })
    }

    /// The point estimate in milliseconds, when resolved.
    #[must_use]
    pub fn value_ms(&self) -> Option<f64> {
        match self {
            Self::Resolved { value_ms, .. } => Some(*value_ms),
            Self::NotResolvable { .. } => None,
        }
    }
}

/// Microseconds to milliseconds.
pub(crate) fn us_to_ms(us: u64) -> f64 {
    u64_to_f64(us) / 1_000.0
}

/// A count or a microsecond value as `f64`; both stay far below `2^53`, where
/// the conversion is exact.
pub(crate) fn u64_to_f64(value: u64) -> f64 {
    let high = u32::try_from(value >> 32).unwrap_or(u32::MAX);
    let low = u32::try_from(value & 0xffff_ffff).unwrap_or(u32::MAX);
    f64::from(high) * 4_294_967_296.0 + f64::from(low)
}

/// The rank `⌈n·num/den⌉`, at least one.
fn rank_of(n: u64, num: u64, den: u64) -> u64 {
    let product = u128::from(n) * u128::from(num);
    let rank = product.div_ceil(u128::from(den));
    u64::try_from(rank).unwrap_or(n).max(1)
}

/// `x` clamped to `[1, n]` and converted to a rank.
fn clamp_rank(x: f64, n: u64) -> u64 {
    let ceiling = u64_to_f64(n);
    let clamped = if x.is_nan() {
        1.0
    } else {
        x.clamp(1.0, ceiling)
    };
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to [1, n] with n below 2^53, so the integral value converts exactly"
    )]
    let rank = clamped as u64;
    rank
}

/// The value of the sample of rank `rank` (from one), as the highest value
/// equivalent to its bucket, the `HdrHistogram` convention.
#[must_use]
pub fn value_at_rank(histogram: &Histogram<u64>, rank: u64) -> u64 {
    let mut seen: u64 = 0;
    for step in histogram.iter_recorded() {
        seen = seen.saturating_add(step.count_since_last_iteration());
        if seen >= rank {
            return histogram.highest_equivalent(step.value_iterated_to());
        }
    }
    histogram.highest_equivalent(histogram.max())
}

/// Reads quantile `name` off `histogram`.
#[must_use]
pub fn quantile(histogram: &Histogram<u64>, name: QuantileName) -> Quantile {
    let n = histogram.len();
    if n == 0 {
        return Quantile::NotResolvable { samples_above: 0 };
    }
    let (num, den) = name.fraction();
    let rank = rank_of(n, num, den);
    let samples_above = n.saturating_sub(rank);
    if samples_above < MIN_SAMPLES_ABOVE {
        return Quantile::NotResolvable { samples_above };
    }
    let q = u64_to_f64(num) / u64_to_f64(den);
    let center = u64_to_f64(n) * q;
    let spread = ETA * (center * (1.0 - q)).sqrt();
    let low_rank = clamp_rank((center - spread).floor(), n);
    let high_rank = clamp_rank((center + spread).ceil(), n);
    Quantile::Resolved {
        value_ms: us_to_ms(value_at_rank(histogram, rank)),
        rank,
        low_ms: us_to_ms(value_at_rank(histogram, low_rank)),
        low_rank,
        high_ms: us_to_ms(value_at_rank(histogram, high_rank)),
        high_rank,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perf_gen::drive::latency_histogram;

    fn histogram(n: u64) -> Histogram<u64> {
        let mut h = latency_histogram().unwrap();
        for v in 1..=n {
            h.record(v * 10).unwrap();
        }
        h
    }

    #[test]
    fn a_p99_over_500_samples_is_not_resolvable() {
        assert_eq!(
            quantile(&histogram(500), QuantileName::P99),
            Quantile::NotResolvable { samples_above: 5 }
        );
    }

    #[test]
    fn a_p99_over_5000_samples_carries_ranks_4936_to_4964() {
        let Quantile::Resolved {
            rank,
            low_rank,
            high_rank,
            value_ms,
            low_ms,
            high_ms,
        } = quantile(&histogram(5_000), QuantileName::P99)
        else {
            panic!("a p99 over 5,000 samples resolves");
        };
        assert_eq!((low_rank, rank, high_rank), (4_936, 4_950, 4_964));
        assert!(low_ms <= value_ms && value_ms <= high_ms);
        // The sample of rank r is r * 10 µs, read at three significant figures.
        assert!((value_ms - 49.5).abs() < 0.05, "{value_ms}");
    }

    #[test]
    fn an_empty_histogram_resolves_nothing() {
        for name in QuantileName::ALL {
            assert_eq!(
                quantile(&latency_histogram().unwrap(), *name),
                Quantile::NotResolvable { samples_above: 0 }
            );
        }
    }

    #[test]
    fn the_tokens_match_the_serialization() {
        for name in QuantileName::ALL {
            assert_eq!(
                serde_json::to_string(name).unwrap(),
                format!("\"{}\"", name.token())
            );
        }
    }

    #[test]
    fn the_rank_ceiling_is_exact() {
        assert_eq!(rank_of(5_000, 99, 100), 4_950);
        assert_eq!(rank_of(1, 50, 100), 1);
        assert_eq!(rank_of(3, 50, 100), 2);
        assert_eq!(rank_of(10_000, 999, 1_000), 9_990);
    }
}
