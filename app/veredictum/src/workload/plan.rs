// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The run shape a workload is planned against, and its planned duration.
//!
//! The run after seeding is settle, warm-up, scout, drain, a ladder of four
//! plateaus (a fifth at 0.25 × K̂ only when the 0.5 plateau fails), and the
//! closing canaries after the last plateau's sessions finish. A plateau is a
//! ramp-in plus a hold of `clamp(sample_target / rate, 30 s, 90 s)`, a
//! function of the planned rate alone. The planned duration is therefore a
//! range over the rates a run could meet, and it does not depend on the
//! server.
//!
//! NOTE: no openEHR spec governs this — our own design; the phase lengths are
//! the owner's ruling of 2026-10-30 on a run of at most ten minutes.

use crate::workload::Workload;

/// The settle phase, in seconds.
pub const SETTLE_S: u32 = 15;
/// The warm-up at the start rate, in seconds.
pub const WARMUP_S: u32 = 20;
/// The scout's exponential ramp, in seconds.
pub const SCOUT_S: u32 = 60;
/// The longest drain after the scout, in seconds.
pub const DRAIN_S: u32 = 10;
/// The closing canaries, in seconds, before the session tail is added.
pub const CLOSING_S: u32 = 10;
/// A plateau's ramp-in, in seconds.
pub const RAMP_IN_S: u32 = 5;
/// The shortest hold, in seconds.
pub const HOLD_MIN_S: u32 = 30;
/// The longest hold, in seconds.
pub const HOLD_MAX_S: u32 = 90;
/// The ladder's plateaus as multiples of the scout's knee estimate.
pub const LADDER: [f64; 4] = [0.5, 0.75, 0.9, 1.05];
/// The extra plateau, run only when the 0.5 plateau fails.
pub const EXTRA_PLATEAU: f64 = 0.25;
/// The longest a run may take after seeding, in seconds.
pub const RUN_LIMIT_S: u32 = 600;

/// The phases of fixed length, in seconds: settle, warm-up, scout, drain and
/// the closing canaries.
#[must_use]
pub fn fixed_phases_s() -> u32 {
    SETTLE_S + WARMUP_S + SCOUT_S + DRAIN_S + CLOSING_S
}

/// Returns the hold of a plateau offered `rate_per_s`, in whole seconds:
/// `clamp(ceil(sample_target / rate), 30, 90)`.
///
/// A rate that is not a positive finite number holds for the longest hold.
#[must_use]
pub fn hold_s(sample_target: u32, rate_per_s: f64) -> u32 {
    if !(rate_per_s.is_finite() && rate_per_s > 0.0) {
        return HOLD_MAX_S;
    }
    let wanted = (f64::from(sample_target) / rate_per_s).ceil();
    if wanted >= f64::from(HOLD_MAX_S) {
        HOLD_MAX_S
    } else if wanted <= f64::from(HOLD_MIN_S) {
        HOLD_MIN_S
    } else {
        #[expect(
            clippy::as_conversions,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the value is a whole number strictly between 30 and 90 by the branches above"
        )]
        {
            wanted as u32
        }
    }
}

/// The planned run after seeding, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_field_names,
    reason = "every field is a count of seconds, and the suffix names the unit"
)]
pub struct PlannedRange {
    /// Every plateau at the shortest hold and no extra plateau.
    pub min_s: u32,
    /// Every plateau at the longest hold, with the extra plateau.
    pub max_s: u32,
    /// The longest expected session span, waited for before the closing
    /// canaries, rounded up to a second.
    pub session_tail_s: u32,
}

impl PlannedRange {
    /// Returns whether the longest plan fits the ten-minute limit.
    #[must_use]
    pub fn within_limit(&self) -> bool {
        self.max_s <= RUN_LIMIT_S
    }
}

/// Returns the planned run range of `workload`.
#[must_use]
pub fn planned_range(workload: &Workload) -> PlannedRange {
    let tail_ms = workload.longest_session_span_ms();
    let session_tail_s = u32::try_from(tail_ms.div_ceil(1_000)).unwrap_or(u32::MAX);
    let plateaus = u32::try_from(LADDER.len()).unwrap_or(u32::MAX);
    let fixed = fixed_phases_s().saturating_add(session_tail_s);
    let min_s = fixed.saturating_add(plateaus.saturating_mul(RAMP_IN_S + HOLD_MIN_S));
    let max_s = fixed.saturating_add((plateaus + 1).saturating_mul(RAMP_IN_S + HOLD_MAX_S));
    PlannedRange {
        min_s,
        max_s,
        session_tail_s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_follow_the_planned_rate_only() {
        assert_eq!(hold_s(5_000, 1_000.0), HOLD_MIN_S);
        assert_eq!(hold_s(5_000, 167.0), HOLD_MIN_S);
        assert_eq!(hold_s(5_000, 100.0), 50);
        assert_eq!(hold_s(5_000, 56.0), HOLD_MAX_S);
        assert_eq!(hold_s(5_000, 0.0), HOLD_MAX_S);
        assert_eq!(hold_s(5_000, f64::NAN), HOLD_MAX_S);
    }

    /// The fixed phases sum to 115 s, so four short plateaus give 255 s and
    /// five long ones 590 s before any session tail.
    #[test]
    fn the_fixed_phases_and_the_ladder_add_up() {
        assert_eq!(fixed_phases_s(), 115);
        assert_eq!(fixed_phases_s() + 4 * (RAMP_IN_S + HOLD_MIN_S), 255);
        assert_eq!(fixed_phases_s() + 5 * (RAMP_IN_S + HOLD_MAX_S), 590);
    }
}
