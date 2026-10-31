// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The seeded arrival schedule: a rate profile, the arrival process over it,
//! and the deterministic draws the generator makes per arrival.
//!
//! Arrivals of a non-homogeneous process are placed by time transformation:
//! the n-th arrival sits where the cumulative intensity `Λ(t)` reaches the
//! sum of n unit gaps, exponential for a Poisson process and `1` for a
//! uniform one. Each ramp shape has a closed-form inverse, so the schedule
//! is a pure function of the seed, the process and the profile. No openEHR
//! specification governs this: our own design.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::perf_gen::Mix;

/// A planned arrival rate in arrivals per second: finite and positive.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Rate(f64);

impl Rate {
    /// Builds a rate, refusing zero, a negative value, NaN and infinity.
    ///
    /// # Errors
    /// [`ProfileError::Rate`] for any value that is not finite and positive.
    pub fn new(per_second: f64) -> Result<Self, ProfileError> {
        if per_second.is_finite() && per_second > 0.0 {
            Ok(Self(per_second))
        } else {
            Err(ProfileError::Rate(per_second))
        }
    }

    /// The rate in arrivals per second.
    #[must_use]
    pub fn per_second(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Rate {
    type Error = ProfileError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Rate> for f64 {
    fn from(rate: Rate) -> Self {
        rate.0
    }
}

/// How the rate moves from a segment's start to its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RampShape {
    /// The rate changes by the same amount every second.
    Linear,
    /// The rate changes by the same factor every second (k6
    /// `ramping-arrival-rate` stages chained geometrically).
    Exponential,
}

/// One segment of a rate profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    duration: Duration,
    from: Rate,
    to: Rate,
    shape: RampShape,
}

impl Segment {
    /// A segment holding one rate.
    #[must_use]
    pub fn constant(rate: Rate, duration: Duration) -> Self {
        Self {
            duration,
            from: rate,
            to: rate,
            shape: RampShape::Linear,
        }
    }

    /// A segment ramping from one rate to another.
    #[must_use]
    pub fn ramp(from: Rate, to: Rate, duration: Duration, shape: RampShape) -> Self {
        Self {
            duration,
            from,
            to,
            shape,
        }
    }

    /// The segment's length.
    #[must_use]
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// The rate at the segment's start.
    #[must_use]
    pub fn from(&self) -> Rate {
        self.from
    }

    /// The rate at the segment's end.
    #[must_use]
    pub fn to(&self) -> Rate {
        self.to
    }

    /// The planned rate `offset` into the segment, clamped to its length.
    #[must_use]
    pub fn rate_at(&self, offset: Duration) -> f64 {
        let span = self.duration.as_secs_f64();
        let t = offset.as_secs_f64().min(span);
        let (r0, r1) = (self.from.0, self.to.0);
        if span <= 0.0 {
            return r0;
        }
        match self.shape {
            RampShape::Linear => r0 + (r1 - r0) * t / span,
            RampShape::Exponential => r0 * (r1 / r0).powf(t / span),
        }
    }

    /// The exponential growth constant `ln(r1 / r0) / T`, zero for a flat
    /// or linear segment.
    fn growth(&self) -> f64 {
        let span = self.duration.as_secs_f64();
        match self.shape {
            RampShape::Exponential if span > 0.0 => (self.to.0 / self.from.0).ln() / span,
            RampShape::Exponential | RampShape::Linear => 0.0,
        }
    }

    /// The cumulative intensity over the whole segment.
    fn mass(&self) -> f64 {
        let span = self.duration.as_secs_f64();
        let (r0, r1) = (self.from.0, self.to.0);
        let g = self.growth();
        match self.shape {
            RampShape::Exponential if g != 0.0 => (r1 - r0) / g,
            RampShape::Exponential | RampShape::Linear => (r0 + r1) * span / 2.0,
        }
    }

    /// The offset in seconds at which the segment's cumulative intensity
    /// reaches `mass`, clamped to the segment.
    fn invert(&self, mass: f64) -> f64 {
        let span = self.duration.as_secs_f64();
        if span <= 0.0 {
            return 0.0;
        }
        let r0 = self.from.0;
        let t = match self.shape {
            RampShape::Exponential => {
                let g = self.growth();
                if g == 0.0 {
                    mass / r0
                } else {
                    (g * mass / r0).ln_1p() / g
                }
            }
            RampShape::Linear => {
                // NOTE: Press et al., "Numerical Recipes", 3rd ed., §5.6: this root form stays
                // exact as the slope tends to zero, where the textbook quadratic cancels.
                let slope = (self.to.0 - r0) / span;
                2.0 * mass / (r0 + (r0 * r0 + 2.0 * slope * mass).sqrt())
            }
        };
        t.clamp(0.0, span)
    }
}

/// A rate profile: segments laid end to end from offset zero.
#[derive(Debug, Clone, PartialEq)]
pub struct RateProfile {
    segments: Vec<Segment>,
}

impl RateProfile {
    /// Builds a profile from its segments.
    ///
    /// # Errors
    /// [`ProfileError::Empty`] for no segment, [`ProfileError::ZeroSegment`]
    /// for a segment of zero length.
    pub fn new(segments: Vec<Segment>) -> Result<Self, ProfileError> {
        if segments.is_empty() {
            return Err(ProfileError::Empty);
        }
        if let Some(index) = segments.iter().position(|s| s.duration.is_zero()) {
            return Err(ProfileError::ZeroSegment(index));
        }
        Ok(Self { segments })
    }

    /// A profile holding one rate for `duration`.
    ///
    /// # Errors
    /// [`ProfileError::ZeroSegment`] for a zero `duration`.
    pub fn constant(rate: Rate, duration: Duration) -> Result<Self, ProfileError> {
        Self::new(vec![Segment::constant(rate, duration)])
    }

    /// The segments, in order.
    #[must_use]
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// The profile's total length.
    #[must_use]
    pub fn duration(&self) -> Duration {
        self.segments.iter().map(|s| s.duration).sum()
    }

    /// The planned rate at `offset`, or `None` past the profile's end.
    #[must_use]
    pub fn rate_at(&self, offset: Duration) -> Option<f64> {
        let mut start = Duration::ZERO;
        for segment in &self.segments {
            let end = start.saturating_add(segment.duration);
            if offset < end {
                return Some(segment.rate_at(offset.saturating_sub(start)));
            }
            start = end;
        }
        None
    }
}

/// The arrival process laid over a rate profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrivalProcess {
    /// Exponential gaps in the transformed time: a Poisson process at the
    /// planned rate.
    #[default]
    Poisson,
    /// Equal gaps in the transformed time: an even grid at the planned rate,
    /// with no arrival variance.
    Uniform,
}

impl ArrivalProcess {
    /// Every process, in serialization order.
    pub const ALL: &[ArrivalProcess] = &[ArrivalProcess::Poisson, ArrivalProcess::Uniform];
}

/// A profile or rate the generator refuses.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ProfileError {
    /// A rate that is not finite and positive.
    #[error("a rate must be finite and positive, got {0}")]
    Rate(f64),
    /// A profile with no segment.
    #[error("a rate profile needs at least one segment")]
    Empty,
    /// A segment of zero length.
    #[error("segment {0} of the rate profile has zero length")]
    ZeroSegment(usize),
}

/// One planned arrival of an interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arrival {
    /// The arrival's position in the schedule, from zero.
    pub index: u64,
    /// The intended instant, as an offset from the profile's start.
    pub at: Duration,
    /// The position of the drawn interaction in the [`Mix`].
    pub interaction: usize,
}

/// Stream tags keep the draws for gaps, interaction choice and think time
/// independent of each other under one seed.
const STREAM_GAPS: u64 = 0x6761_7073;
const STREAM_CHOICE: u64 = 0x6368_6f69;
const STREAM_THINK: u64 = 0x7468_696e;

/// One `SplitMix64` step: advances `state` and returns the mixed output.
///
/// The constants are the reference ones (Steele, Lea and Flood, "Fast
/// splittable pseudorandom number generators", OOPSLA 2014;
/// <https://prng.di.unimi.it/splitmix64.c>).
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A draw keyed by the seed, a stream and the caller's indices, so the
/// value never depends on the order in which draws are made.
fn keyed(seed: u64, stream: u64, parts: &[u64]) -> u64 {
    let mut state = seed ^ stream;
    let mut out = splitmix64(&mut state);
    for part in parts {
        state ^= part.wrapping_add(out);
        out = splitmix64(&mut state);
    }
    out
}

/// A uniform draw in `(0, 1]` from the top 53 bits of `bits`.
fn unit(bits: u64) -> f64 {
    let top = (bits >> 11).saturating_add(1);
    let high = u32::try_from(top >> 32).unwrap_or(u32::MAX);
    let low = u32::try_from(top & 0xffff_ffff).unwrap_or(u32::MAX);
    (f64::from(high) * 4_294_967_296.0 + f64::from(low)) / 9_007_199_254_740_992.0
}

/// A unit-mean exponential draw.
fn exponential(bits: u64) -> f64 {
    -unit(bits).ln()
}

/// Lays the arrivals of `process` over `profile` and draws each one's
/// interaction from `mix`, all from `seed`.
///
/// The result is a pure function of its arguments: the same seed, process,
/// profile and mix give the same instants and the same interactions.
#[must_use]
pub fn intended_arrivals(
    seed: u64,
    process: ArrivalProcess,
    profile: &RateProfile,
    mix: &Mix,
) -> Vec<Arrival> {
    let mut gaps = seed ^ STREAM_GAPS;
    let mut arrivals = Vec::new();
    let mut position = 0.0_f64;
    let mut base_mass = 0.0_f64;
    let mut base_time = Duration::ZERO;
    let mut segments = profile.segments.iter();
    let Some(mut segment) = segments.next() else {
        return arrivals;
    };
    let mut index: u64 = 0;
    loop {
        position += match process {
            ArrivalProcess::Poisson => exponential(splitmix64(&mut gaps)),
            ArrivalProcess::Uniform => 1.0,
        };
        while position > base_mass + segment.mass() {
            base_mass += segment.mass();
            base_time = base_time.saturating_add(segment.duration);
            match segments.next() {
                Some(next) => segment = next,
                None => return arrivals,
            }
        }
        let offset = segment.invert(position - base_mass);
        let at = base_time.saturating_add(Duration::from_secs_f64(offset));
        arrivals.push(Arrival {
            index,
            at,
            interaction: mix.pick(keyed(seed, STREAM_CHOICE, &[index])),
        });
        index = index.saturating_add(1);
    }
}

/// The think time before follow-up `stage` of arrival `arrival`: an
/// exponential draw with mean `mean`, keyed so it never depends on the
/// order in which sessions progress.
#[must_use]
pub fn think_time(seed: u64, arrival: u64, stage: u32, mean: Duration) -> Duration {
    let draw = exponential(keyed(seed, STREAM_THINK, &[arrival, u64::from(stage)]));
    Duration::try_from_secs_f64(draw * mean.as_secs_f64()).unwrap_or(Duration::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perf_gen::{Interaction, Mix, OperationId};

    fn mix() -> Mix {
        Mix::new(vec![
            Interaction::single("read", OperationId::new("read").unwrap(), 9).unwrap(),
            Interaction::single("write", OperationId::new("write").unwrap(), 1).unwrap(),
        ])
        .unwrap()
    }

    fn rate(r: f64) -> Rate {
        Rate::new(r).unwrap()
    }

    #[test]
    fn the_same_seed_and_profile_give_the_same_instants() {
        let profile = RateProfile::new(vec![
            Segment::ramp(
                rate(5.0),
                rate(200.0),
                Duration::from_secs(20),
                RampShape::Exponential,
            ),
            Segment::constant(rate(200.0), Duration::from_secs(10)),
        ])
        .unwrap();
        let a = intended_arrivals(7, ArrivalProcess::Poisson, &profile, &mix());
        let b = intended_arrivals(7, ArrivalProcess::Poisson, &profile, &mix());
        assert_eq!(a, b);
        let c = intended_arrivals(8, ArrivalProcess::Poisson, &profile, &mix());
        assert_ne!(a, c, "a different seed draws a different schedule");
    }

    #[test]
    fn instants_are_ordered_and_inside_the_profile() {
        let profile = RateProfile::new(vec![
            Segment::ramp(
                rate(10.0),
                rate(100.0),
                Duration::from_secs(10),
                RampShape::Linear,
            ),
            Segment::ramp(
                rate(100.0),
                rate(20.0),
                Duration::from_secs(10),
                RampShape::Exponential,
            ),
        ])
        .unwrap();
        let arrivals = intended_arrivals(1, ArrivalProcess::Poisson, &profile, &mix());
        assert!(arrivals.windows(2).all(|w| w[0].at <= w[1].at));
        assert!(arrivals.iter().all(|a| a.at <= profile.duration()));
    }

    #[test]
    fn a_poisson_count_tracks_the_cumulative_intensity() {
        let profile = RateProfile::constant(rate(1_000.0), Duration::from_secs(20)).unwrap();
        let count = intended_arrivals(3, ArrivalProcess::Poisson, &profile, &mix()).len();
        // 20,000 expected, standard deviation about 141.
        assert!((19_400..=20_600).contains(&count), "{count}");

        let ramp = RateProfile::new(vec![Segment::ramp(
            rate(10.0),
            rate(1_000.0),
            Duration::from_secs(30),
            RampShape::Exponential,
        )])
        .unwrap();
        // Mass (r1 - r0) / ln(r1 / r0) * T = 990 / ln(100) * 30, about 6,449.
        let count = intended_arrivals(3, ArrivalProcess::Uniform, &ramp, &mix()).len();
        assert_eq!(count, 6_449);
    }

    #[test]
    fn a_uniform_process_is_an_even_grid() {
        let profile = RateProfile::constant(rate(4.0), Duration::from_secs(2)).unwrap();
        let arrivals = intended_arrivals(0, ArrivalProcess::Uniform, &profile, &mix());
        let at: Vec<u128> = arrivals.iter().map(|a| a.at.as_millis()).collect();
        assert_eq!(at, vec![250, 500, 750, 1_000, 1_250, 1_500, 1_750, 2_000]);
    }

    #[test]
    fn interactions_follow_their_weights() {
        let profile = RateProfile::constant(rate(1_000.0), Duration::from_secs(10)).unwrap();
        let arrivals = intended_arrivals(11, ArrivalProcess::Poisson, &profile, &mix());
        let writes = arrivals.iter().filter(|a| a.interaction == 1).count();
        let share = (writes * 1_000).div_euclid(arrivals.len());
        assert!((85..=115).contains(&share), "write share {share} per mille");
    }

    #[test]
    fn think_times_are_keyed_and_have_their_mean() {
        let mean = Duration::from_secs(2);
        assert_eq!(think_time(5, 10, 1, mean), think_time(5, 10, 1, mean));
        let total: f64 = (0..10_000)
            .map(|i| think_time(5, i, 1, mean).as_secs_f64())
            .sum();
        let average = total / 10_000.0;
        assert!((1.9..2.1).contains(&average), "{average}");
    }

    #[test]
    fn a_bad_rate_or_profile_is_refused() {
        assert_eq!(Rate::new(0.0), Err(ProfileError::Rate(0.0)));
        assert!(Rate::new(f64::NAN).is_err());
        assert_eq!(RateProfile::new(vec![]), Err(ProfileError::Empty));
        assert_eq!(
            RateProfile::constant(rate(1.0), Duration::ZERO),
            Err(ProfileError::ZeroSegment(0))
        );
    }

    #[test]
    fn the_planned_rate_reads_along_the_profile() {
        let profile = RateProfile::new(vec![Segment::ramp(
            rate(10.0),
            rate(1_000.0),
            Duration::from_secs(10),
            RampShape::Exponential,
        )])
        .unwrap();
        let mid = profile.rate_at(Duration::from_secs(5)).unwrap();
        assert!((mid - 100.0).abs() < 1e-9, "{mid}");
        assert_eq!(profile.rate_at(Duration::from_secs(11)), None);
    }
}
