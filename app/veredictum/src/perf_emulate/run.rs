// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The run after seeding: settle, warm-up, scout, drain, the ladder, the
//! extra plateau when the lowest one fails, and the closing canaries.
//!
//! Every phase is driven by the open-loop generator against any
//! [`Performer`], so a test drives the whole run against an in-process fake
//! on a paused clock and a live run drives it against a server. Rates are
//! operation arrivals per second, the unit of the population model; the
//! generator draws interactions at the rate divided by the binding's
//! operations per interaction. The ladder is one continuous drive, so the
//! sessions of one plateau run on into the next as a clinical front end's
//! would. The scout is halted once its mean in flight exceeds what twice the
//! largest budget allows by Little's law (Little, "A Proof for the Queuing
//! Formula: L = λW", Operations Research 9(3), 1961), so an overloaded
//! server is not buried under a minute of ramp it cannot serve.
//!
//! No openEHR specification governs this: our own design.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use thiserror::Error;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep_until, timeout_at};

use crate::perf_emulate::binding::Binding;
use crate::perf_gen::arrivals::{
    ArrivalProcess, ProfileError, RampShape, Rate, RateProfile, Segment,
};
use crate::perf_gen::drive::{
    DriveError, DrivePlan, DriveReport, GeneratorLimits, Halt, WindowSpec, drive,
};
use crate::perf_gen::{Call, ErrorClass, Outcome, Performer};
use crate::perf_result::compare::{SideName, Slot};
use crate::perf_result::derive::{self, ScoutEstimate};
use crate::perf_result::server::ServerReadings;
use crate::perf_result::{
    PerfResultError, Phase, PhaseRecord, RawPlateau, RawScout, RawScoutWindow, RawWindow, millis,
};
use crate::workload::plan::{
    CLOSING_S, DRAIN_S, EXTRA_PLATEAU, LADDER, RAMP_IN_S, SCOUT_S, SETTLE_S, WARMUP_S, hold_s,
};

/// The operation rate the warm-up holds and the scout starts from, per
/// second.
pub const DEFAULT_START_RATE: f64 = 8.0;

/// The operation rate the scout's ramp ends at, per second.
pub const DEFAULT_SCOUT_END_RATE: f64 = 4_096.0;

/// The length of one scout window.
pub const SCOUT_WINDOW: Duration = Duration::from_secs(2);

/// Separates the scout's draws from the warm-up's under one seed.
const SCOUT_STREAM: u64 = 0x5c07;

/// How often the scout's in-flight count is read.
const MONITOR_EVERY: Duration = Duration::from_millis(250);

/// The in-flight count below which the scout is never halted, so a slow
/// start on a handful of requests is not read as a server falling behind.
const HALT_FLOOR: u64 = 64;

/// The run's shape: its rates and the length of every phase.
#[derive(Debug, Clone, PartialEq)]
pub struct RunShape {
    /// The warm-up's rate and the scout's starting rate, operations per
    /// second.
    pub start_rate: Rate,
    /// The scout's end rate, operations per second.
    pub scout_end_rate: Rate,
    /// The settle phase.
    pub settle: Duration,
    /// The warm-up.
    pub warm_up: Duration,
    /// The scout's ramp.
    pub scout: Duration,
    /// One scout window.
    pub scout_window: Duration,
    /// The longest drain after a drive.
    pub drain: Duration,
    /// A plateau's ramp-in.
    pub ramp_in: Duration,
    /// The closing phase.
    pub closing: Duration,
    /// Samples per plateau a hold is sized for.
    pub sample_target: u32,
}

impl RunShape {
    /// The shape the owner ruled: 15 s settle, 20 s warm-up, a 60 s scout in
    /// 2 s windows, at most 10 s of drain, 5 s ramp-ins and 10 s closing.
    #[must_use]
    pub fn standard(sample_target: u32, start_rate: Rate, scout_end_rate: Rate) -> Self {
        Self {
            start_rate,
            scout_end_rate,
            settle: Duration::from_secs(u64::from(SETTLE_S)),
            warm_up: Duration::from_secs(u64::from(WARMUP_S)),
            scout: Duration::from_secs(u64::from(SCOUT_S)),
            scout_window: SCOUT_WINDOW,
            drain: Duration::from_secs(u64::from(DRAIN_S)),
            ramp_in: Duration::from_secs(u64::from(RAMP_IN_S)),
            closing: Duration::from_secs(u64::from(CLOSING_S)),
            sample_target,
        }
    }

    /// The hold of a plateau at `rate` operations per second.
    #[must_use]
    pub fn hold(&self, rate: f64) -> Duration {
        Duration::from_secs(u64::from(hold_s(self.sample_target, rate)))
    }
}

/// What the run needs besides its performer and its hooks.
#[derive(Debug, Clone)]
pub struct RunRequest<'a> {
    /// The root of every seeded draw.
    pub seed: u64,
    /// How arrivals are spaced.
    pub process: ArrivalProcess,
    /// The bound workload.
    pub binding: &'a Binding,
    /// The generator's limits; the drain is the shape's.
    pub limits: GeneratorLimits,
    /// The shape.
    pub shape: RunShape,
}

/// The side of a run that is not the generator: maintenance, the closing
/// canaries, progress lines and the per-plateau server-side seam.
pub trait RunHooks: Sync {
    /// Reports progress.
    fn progress(&self, message: &str);

    /// Runs the settle phase's maintenance; the run idles for the rest of
    /// the phase.
    fn settle(&self) -> impl Future<Output = ()> + Send;

    /// Runs the closing canaries.
    fn closing(&self) -> impl Future<Output = ()> + Send;

    /// Notes that the hold of plateau `index` began.
    fn plateau_began(&self, _index: usize) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Returns the server-side readings over the hold of plateau `index`.
    fn plateau_ended(&self, _index: usize) -> impl Future<Output = ServerReadings> + Send {
        async { ServerReadings::default() }
    }
}

/// What the run after seeding produced.
#[derive(Debug, Clone)]
pub struct LadderOutcome {
    /// The scout ramp's windows.
    pub scout: RawScout,
    /// The knee estimate the ladder was built from.
    pub estimate: ScoutEstimate,
    /// The plateaus, in the order they ran.
    pub plateaus: Vec<RawPlateau>,
    /// The phases with their planned and measured wall times.
    pub phases: Vec<PhaseRecord>,
    /// The run's wall time after seeding.
    pub elapsed: Duration,
}

/// A run that could not finish.
#[derive(Debug, Error)]
pub enum RunError {
    /// The generator failed.
    #[error(transparent)]
    Drive(#[from] DriveError),
    /// A rate or a profile was refused.
    #[error(transparent)]
    Profile(#[from] ProfileError),
    /// A drive task could not be joined.
    #[error("a drive task ended abnormally: {0}")]
    Join(String),
    /// A recorded window did not encode or derive.
    #[error(transparent)]
    Record(#[from] PerfResultError),
}

/// Counts every exchange and its class, and the exchanges in flight.
#[derive(Debug, Default)]
struct Tally {
    exchanges: u64,
    errors: BTreeMap<ErrorClass, u64>,
}

/// A performer that counts what passes through it.
struct Counted<P> {
    inner: Arc<P>,
    in_flight: AtomicU64,
    tally: Mutex<Tally>,
}

impl<P: Performer> Counted<P> {
    fn new(inner: Arc<P>) -> Self {
        Self {
            inner,
            in_flight: AtomicU64::new(0),
            tally: Mutex::new(Tally::default()),
        }
    }

    fn take(&self) -> Tally {
        std::mem::take(&mut *self.tally.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl<P: Performer> Performer for Counted<P> {
    async fn perform(&self, call: &Call) -> Outcome {
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        let outcome = self.inner.perform(call).await;
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
        let mut tally = self.tally.lock().unwrap_or_else(PoisonError::into_inner);
        tally.exchanges = tally.exchanges.saturating_add(1);
        if let Outcome::Failure { class, .. } = &outcome {
            let count = tally.errors.entry(*class).or_insert(0);
            *count = count.saturating_add(1);
        }
        outcome
    }
}

fn phase(phase: Phase, planned: Duration, wall: Duration, tally: Tally) -> PhaseRecord {
    PhaseRecord {
        phase,
        planned_ms: millis(planned),
        wall_ms: millis(wall),
        exchanges: tally.exchanges,
        errors: tally.errors,
    }
}

/// The interaction rate the generator draws at for `op_rate` operations
/// per second.
fn interaction_rate(op_rate: f64, binding: &Binding) -> Result<Rate, ProfileError> {
    Rate::new(op_rate / binding.operations_per_interaction)
}

fn spawn_drive<P: Performer>(
    plan: DrivePlan,
    binding: &Binding,
    performer: Arc<P>,
) -> JoinHandle<Result<DriveReport, DriveError>> {
    let mix = binding.mix.clone();
    tokio::spawn(async move { drive(&plan, &mix, performer).await })
}

fn joined(
    result: Result<Result<DriveReport, DriveError>, tokio::task::JoinError>,
) -> Result<DriveReport, RunError> {
    result
        .map_err(|e| RunError::Join(e.to_string()))?
        .map_err(RunError::from)
}

fn lag_limit_us(request: &RunRequest<'_>) -> u64 {
    u64::try_from(request.limits.lag_limit.as_micros()).unwrap_or(u64::MAX)
}

fn limits(request: &RunRequest<'_>) -> GeneratorLimits {
    GeneratorLimits {
        drain: request.shape.drain,
        ..request.limits.clone()
    }
}

/// One side of a run: its counted performer, its hooks, and what it has
/// recorded so far.
struct Side<'s, P, H> {
    counted: Arc<Counted<P>>,
    hooks: &'s H,
    phases: Vec<PhaseRecord>,
    began: Instant,
    plateaus: Vec<RawPlateau>,
    last_rate: f64,
}

impl<'s, P: Performer, H: RunHooks> Side<'s, P, H> {
    fn new(performer: Arc<P>, hooks: &'s H, start_rate: f64) -> Self {
        Self {
            counted: Arc::new(Counted::new(performer)),
            hooks,
            phases: Vec::new(),
            began: Instant::now(),
            plateaus: Vec::new(),
            last_rate: start_rate,
        }
    }

    /// Runs the settle phase: maintenance, then idle for the rest of it.
    async fn settle(&mut self, request: &RunRequest<'_>) {
        self.began = Instant::now();
        let shape = &request.shape;
        let end = self.began + shape.settle;
        self.hooks.progress("settle: maintenance, then idle");
        self.hooks.settle().await;
        sleep_until(end).await;
        self.phases.push(phase(
            Phase::Settle,
            shape.settle,
            self.began.elapsed(),
            self.counted.take(),
        ));
    }

    /// Runs the warm-up at the start rate, discarded.
    async fn warm_up(&mut self, request: &RunRequest<'_>) -> Result<(), RunError> {
        let shape = &request.shape;
        self.hooks.progress(&format!(
            "warm-up: {} s at {} operations/s",
            shape.warm_up.as_secs(),
            shape.start_rate.per_second()
        ));
        let warm_rate = interaction_rate(shape.start_rate.per_second(), request.binding)?;
        let warm = DrivePlan::new(
            request.seed,
            request.process,
            RateProfile::constant(warm_rate, shape.warm_up)?,
            Vec::new(),
            limits(request),
        )?;
        let at = Instant::now();
        let _warm = joined(spawn_drive(warm, request.binding, Arc::clone(&self.counted)).await)?;
        self.phases.push(phase(
            Phase::WarmUp,
            shape.warm_up,
            at.elapsed(),
            self.counted.take(),
        ));
        Ok(())
    }

    /// Runs the scout and its drain, and returns the windows with the knee
    /// estimate they give.
    async fn scout(
        &mut self,
        request: &RunRequest<'_>,
    ) -> Result<(RawScout, ScoutEstimate), RunError> {
        let (scout, scout_wall, drain_wall) = run_scout(request, &self.counted, self.hooks).await?;
        self.phases.push(phase(
            Phase::Scout,
            request.shape.scout,
            scout_wall,
            self.counted.take(),
        ));
        self.phases.push(phase(
            Phase::Drain,
            request.shape.drain,
            drain_wall,
            Tally::default(),
        ));
        let windows = derive::scout_windows(
            &scout,
            &request.binding.qualification,
            lag_limit_us(request),
        )?;
        let estimate = derive::scout_estimate(&scout, &windows);
        self.hooks.progress(&format!(
            "scout: knee estimate {:.1} operations/s{}",
            estimate.rate,
            if estimate.breached_window.is_some() {
                ""
            } else {
                " (the ramp ended unbreached, a lower bound)"
            }
        ));
        Ok((scout, estimate))
    }

    /// Runs one leg of plateaus, ramping in from the side's last rate.
    async fn leg(
        &mut self,
        request: &RunRequest<'_>,
        ladder: &[(f64, f64)],
        which: Phase,
        drain: Duration,
    ) -> Result<(), RunError> {
        let leg = Leg {
            ladder,
            first_index: self.plateaus.len(),
            which,
            from_rate: self.last_rate,
            drain,
        };
        let plateaus =
            run_plateaus(request, &self.counted, self.hooks, &leg, &mut self.phases).await?;
        self.plateaus.extend(plateaus);
        if let Some((_, rate)) = ladder.last() {
            self.last_rate = *rate;
        }
        Ok(())
    }

    /// Whether the side's plateau at `index` failed its qualification.
    fn plateau_fails(&self, request: &RunRequest<'_>, index: usize) -> Result<bool, RunError> {
        Ok(match self.plateaus.get(index) {
            Some(raw) => {
                !derive::plateau(
                    raw,
                    &request.binding.qualification,
                    lag_limit_us(request),
                    &format!("/raw/plateaus/{index}"),
                )?
                .qualification
                .qualified
            }
            None => false,
        })
    }

    /// Runs the closing canaries and returns what the side recorded.
    async fn close(
        mut self,
        request: &RunRequest<'_>,
        scout: RawScout,
        estimate: ScoutEstimate,
    ) -> LadderOutcome {
        let at = Instant::now();
        self.hooks.progress("closing canaries");
        self.hooks.closing().await;
        self.phases.push(phase(
            Phase::Closing,
            request.shape.closing,
            at.elapsed(),
            self.counted.take(),
        ));
        LadderOutcome {
            scout,
            estimate,
            plateaus: self.plateaus,
            phases: self.phases,
            elapsed: self.began.elapsed(),
        }
    }
}

/// Runs the phases after seeding against `performer`.
///
/// # Errors
/// [`RunError`] when the generator fails, a planned rate is refused, or a
/// recorded window does not encode.
pub async fn run_after_seeding<P: Performer, H: RunHooks>(
    request: &RunRequest<'_>,
    performer: Arc<P>,
    hooks: &H,
) -> Result<LadderOutcome, RunError> {
    let shape = &request.shape;
    let mut side = Side::new(performer, hooks, shape.start_rate.per_second());
    side.settle(request).await;
    side.warm_up(request).await?;
    let (scout, estimate) = side.scout(request).await?;

    let rate_of = |multiple: f64| (multiple, multiple * estimate.rate);
    let (lowest, higher) = LADDER
        .split_first()
        .map_or((None, &[][..]), |(l, h)| (Some(*l), h));
    let between = between_legs(request);
    if let Some(lowest) = lowest {
        side.leg(request, &[rate_of(lowest)], Phase::Ladder, between)
            .await?;
        if side.plateau_fails(request, 0)? {
            hooks.progress("the lowest plateau failed: the extra plateau runs next");
            side.last_rate = shape.start_rate.per_second();
            side.leg(
                request,
                &[rate_of(EXTRA_PLATEAU)],
                Phase::ExtraPlateau,
                between,
            )
            .await?;
        }
    }
    let rest: Vec<(f64, f64)> = higher.iter().map(|m| rate_of(*m)).collect();
    if !rest.is_empty() {
        side.leg(request, &rest, Phase::Ladder, shape.drain).await?;
    }
    Ok(side.close(request, scout, estimate).await)
}

/// The longest wait between two legs: the largest latency budget, past
/// which an outstanding exchange is over every budget already.
fn between_legs(request: &RunRequest<'_>) -> Duration {
    Duration::from_millis(u64::from(request.binding.qualification.largest_budget_ms()))
}

/// What an interleaved run produced.
#[derive(Debug, Clone)]
pub struct InterleavedOutcome {
    /// Side A's phases, scout and plateaus.
    pub a: LadderOutcome,
    /// Side B's.
    pub b: LadderOutcome,
    /// The knee estimate the shared ladder was built from: the larger of
    /// the two sides'.
    pub shared_estimate: f64,
    /// The plateaus in the order they ran.
    pub sequence: Vec<Slot>,
}

/// Returns the order of the two sides on rung `rung`: A then B on even
/// rungs, B then A on odd ones, so the plateaus run A B B A A B B A.
#[must_use]
pub fn rung_order(rung: usize) -> [SideName; 2] {
    if rung.is_multiple_of(2) {
        [SideName::A, SideName::B]
    } else {
        [SideName::B, SideName::A]
    }
}

/// Runs two targets interleaved on one host.
///
/// Each side settles, warms up and scouts on its own. The shared ladder is
/// built from the larger knee estimate, so both sides are offered the same
/// rates and holds, and on each rung the two plateaus alternate in ABBA
/// order. There is no extra plateau: a rung that fails is read from both
/// records as it stands. Each side closes with its own canaries.
///
/// # Errors
/// [`RunError`] when the generator fails on either side, a planned rate is
/// refused, or a recorded window does not encode.
pub async fn run_interleaved<PA: Performer, HA: RunHooks, PB: Performer, HB: RunHooks>(
    request: &RunRequest<'_>,
    a: (Arc<PA>, &HA),
    b: (Arc<PB>, &HB),
) -> Result<InterleavedOutcome, RunError> {
    let start = request.shape.start_rate.per_second();
    let mut side_a = Side::new(a.0, a.1, start);
    let mut side_b = Side::new(b.0, b.1, start);
    side_a.settle(request).await;
    side_a.warm_up(request).await?;
    let (scout_a, estimate_a) = side_a.scout(request).await?;
    side_b.settle(request).await;
    side_b.warm_up(request).await?;
    let (scout_b, estimate_b) = side_b.scout(request).await?;
    let shared_estimate = estimate_a.rate.max(estimate_b.rate);
    let between = between_legs(request);
    let mut sequence = Vec::new();
    for (rung, multiple) in LADDER.iter().enumerate() {
        let planned_rate = multiple * shared_estimate;
        let drain = if rung + 1 == LADDER.len() {
            request.shape.drain
        } else {
            between
        };
        let ladder = [(*multiple, planned_rate)];
        for side in rung_order(rung) {
            match side {
                SideName::A => side_a.leg(request, &ladder, Phase::Ladder, drain).await?,
                SideName::B => side_b.leg(request, &ladder, Phase::Ladder, drain).await?,
            }
            sequence.push(Slot {
                rung,
                planned_rate,
                side,
            });
        }
    }
    let a = side_a.close(request, scout_a, estimate_a).await;
    let b = side_b.close(request, scout_b, estimate_b).await;
    Ok(InterleavedOutcome {
        a,
        b,
        shared_estimate,
        sequence,
    })
}
/// Drives the scout ramp, halting it once the server has fallen behind,
/// and returns its windows with the ramp's and the drain's wall times.
async fn run_scout<P: Performer, H: RunHooks>(
    request: &RunRequest<'_>,
    counted: &Arc<Counted<P>>,
    hooks: &H,
) -> Result<(RawScout, Duration, Duration), RunError> {
    let shape = &request.shape;
    let binding = request.binding;
    let opi = binding.operations_per_interaction;
    let from = interaction_rate(shape.start_rate.per_second(), binding)?;
    let to = interaction_rate(shape.scout_end_rate.per_second(), binding)?;
    let profile = RateProfile::new(vec![Segment::ramp(
        from,
        to,
        shape.scout,
        RampShape::Exponential,
    )])?;
    let mut windows = Vec::new();
    let mut offset = Duration::ZERO;
    while offset < shape.scout && !shape.scout_window.is_zero() {
        let end = offset.saturating_add(shape.scout_window).min(shape.scout);
        windows.push(WindowSpec { start: offset, end });
        offset = end;
    }
    let halt = Halt::new();
    let plan = DrivePlan::new(
        request.seed ^ SCOUT_STREAM,
        request.process,
        profile.clone(),
        windows,
        limits(request),
    )?
    .with_halt(halt.clone());
    hooks.progress(&format!(
        "scout: {} s exponential ramp from {} to {} operations/s",
        shape.scout.as_secs(),
        shape.start_rate.per_second(),
        shape.scout_end_rate.per_second()
    ));
    let largest_budget_s = f64::from(binding.qualification.largest_budget_ms()) / 1_000.0;
    let start = Instant::now();
    let mut handle = spawn_drive(plan, binding, Arc::clone(counted));
    let mut tick = start;
    let report = loop {
        tick += MONITOR_EVERY;
        match timeout_at(tick, &mut handle).await {
            Ok(result) => break joined(result)?,
            Err(_elapsed) => {
                let offset = tick.saturating_duration_since(start);
                let Some(rate) = profile.rate_at(offset) else {
                    continue;
                };
                let in_flight = counted.in_flight.load(Ordering::Relaxed);
                let allowed = 2.0 * rate * opi * largest_budget_s;
                if !halt.is_set()
                    && in_flight > HALT_FLOOR
                    && crate::perf_result::quantile::u64_to_f64(in_flight) > allowed
                {
                    hooks.progress(&format!(
                        "scout: halted at {:.1} s, {in_flight} exchanges in flight",
                        offset.as_secs_f64()
                    ));
                    halt.set();
                }
            }
        }
    };
    let ramp_wall = report
        .stop
        .stopped_at()
        .unwrap_or(shape.scout)
        .min(report.elapsed);
    let drain_wall = report.elapsed.saturating_sub(ramp_wall);
    let windows = report
        .windows
        .iter()
        .map(|w| {
            let mid = w.spec.start + (w.spec.end.saturating_sub(w.spec.start)) / 2;
            let rate = profile.rate_at(mid).unwrap_or(to.per_second()) * opi;
            Ok(RawScoutWindow {
                planned_rate: Rate::new(rate)?,
                window: RawWindow::from_record(w)?,
            })
        })
        .collect::<Result<Vec<_>, RunError>>()?;
    let scout = RawScout {
        start_rate: shape.start_rate,
        end_rate: shape.scout_end_rate,
        halted_at_ms: match report.stop {
            crate::perf_gen::drive::Stop::Halted { at } => Some(millis(at)),
            _ => None,
        },
        window_ms: millis(shape.scout_window),
        windows,
    };
    Ok((scout, ramp_wall, drain_wall))
}

/// One planned plateau of a drive: its rate, multiple and timing.
struct PlannedPlateau {
    multiple: f64,
    rate: f64,
    start: Duration,
    hold_start: Duration,
    hold_end: Duration,
}

/// One drive of the ladder: plateaus as multiples and operation rates,
/// ascending, numbered from `first_index`, ramping in from `from_rate`.
struct Leg<'l> {
    ladder: &'l [(f64, f64)],
    first_index: usize,
    which: Phase,
    from_rate: f64,
    drain: Duration,
}

/// Drives one leg of the ladder as one continuous profile, so the sessions
/// of one plateau run on into the next.
#[expect(
    clippy::too_many_lines,
    reason = "one leg: its profile, its drive with the plateau hooks, and its records"
)]
async fn run_plateaus<P: Performer, H: RunHooks>(
    request: &RunRequest<'_>,
    counted: &Arc<Counted<P>>,
    hooks: &H,
    leg: &Leg<'_>,
    phases: &mut Vec<PhaseRecord>,
) -> Result<Vec<RawPlateau>, RunError> {
    let Leg {
        ladder,
        first_index,
        which,
        from_rate,
        drain,
    } = *leg;
    let shape = &request.shape;
    let binding = request.binding;
    let mut segments = Vec::new();
    let mut windows = Vec::new();
    let mut planned = Vec::new();
    let mut previous = interaction_rate(from_rate, binding)?;
    let mut offset = Duration::ZERO;
    for (multiple, rate) in ladder {
        let target = interaction_rate(*rate, binding)?;
        let hold = shape.hold(*rate);
        segments.push(Segment::ramp(
            previous,
            target,
            shape.ramp_in,
            RampShape::Linear,
        ));
        segments.push(Segment::constant(target, hold));
        let hold_start = offset + shape.ramp_in;
        let hold_end = hold_start + hold;
        let middle = hold_start + hold / 2;
        windows.push(WindowSpec {
            start: hold_start,
            end: middle,
        });
        windows.push(WindowSpec {
            start: middle,
            end: hold_end,
        });
        planned.push(PlannedPlateau {
            multiple: *multiple,
            rate: *rate,
            start: offset,
            hold_start,
            hold_end,
        });
        previous = target;
        offset = hold_end;
    }
    let profile = RateProfile::new(segments)?;
    hooks.progress(&format!(
        "{}: {} plateau(s) at {} operations/s, {} s planned",
        match which {
            Phase::ExtraPlateau => "extra plateau",
            _ => "ladder",
        },
        planned.len(),
        planned
            .iter()
            .map(|p| format!("{:.1}", p.rate))
            .collect::<Vec<_>>()
            .join(", "),
        offset.as_secs()
    ));
    let plan = DrivePlan::new(
        request.seed ^ u64::try_from(first_index).unwrap_or(0).wrapping_add(0x1add),
        request.process,
        profile,
        windows,
        GeneratorLimits {
            drain,
            ..request.limits.clone()
        },
    )?;
    let start = Instant::now();
    let mut handle = spawn_drive(plan, binding, Arc::clone(counted));
    let mut finished = None;
    let mut readings = Vec::with_capacity(planned.len());
    for (index, p) in planned.iter().enumerate() {
        let number = first_index + index;
        for (at, begin) in [(p.hold_start, true), (p.hold_end, false)] {
            if finished.is_none()
                && let Ok(result) = timeout_at(start + at, &mut handle).await
            {
                finished = Some(result);
            }
            if begin {
                hooks.plateau_began(number).await;
            } else {
                readings.push(hooks.plateau_ended(number).await);
            }
        }
    }
    let report = match finished {
        Some(result) => joined(result)?,
        None => joined(handle.await)?,
    };
    phases.push(phase(which, offset + drain, report.elapsed, counted.take()));
    let stopped_at = report.stop.stopped_at();
    let (halves, _unpaired) = report.windows.as_chunks::<2>();
    let mut plateaus = Vec::with_capacity(planned.len());
    for ((p, server), [first, second]) in planned.iter().zip(readings).zip(halves) {
        let stopped = stopped_at
            .filter(|at| *at < p.hold_end)
            .map(|at| at.saturating_sub(p.start));
        let mut plateau =
            RawPlateau::from_halves(Rate::new(p.rate)?, shape.ramp_in, [first, second], stopped)?;
        plateau.multiple = Some(p.multiple);
        plateau.server = server;
        plateaus.push(plateau);
    }
    Ok(plateaus)
}
