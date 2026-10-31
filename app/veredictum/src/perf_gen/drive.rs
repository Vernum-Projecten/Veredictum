// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The driver: fires the seeded schedule open-loop on a `tokio` runtime and
//! records every exchange into the window holding its intended instant.
//!
//! Each arrival is one task, never an OS thread, and an in-flight semaphore
//! bounds how many exchanges are outstanding. A send that has to wait for a
//! slot waits, and the wait is part of its latency and its lag, because the
//! clock starts at the intended instant. One wait that lasts the saturation
//! limit stops the run as saturated. Every clock reading is
//! `tokio::time::Instant`, so a paused test runtime drives the whole
//! generator on virtual time
//! (<https://docs.rs/tokio/1/tokio/time/fn.pause.html>).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hdrhistogram::Histogram;
use thiserror::Error;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};
use tokio::task::{JoinError, JoinSet};
use tokio::time::{Instant, sleep_until, timeout, timeout_at};

use crate::perf_gen::arrivals::{
    Arrival, ArrivalProcess, RateProfile, intended_arrivals, think_time,
};
use crate::perf_gen::{Call, ErrorClass, FailedExchange, Mix, OperationId, Outcome, Performer};

/// The highest latency a histogram tracks, in microseconds: ten minutes, far
/// past any client timeout. A longer wait is recorded at this value.
pub const HDR_MAX_US: u64 = 600_000_000;

/// The significant figures every latency histogram keeps.
pub const HDR_SIGNIFICANT_FIGURES: u8 = 3;

/// Builds an empty latency histogram over microseconds.
///
/// # Errors
/// [`DriveError::Histogram`] if the bounds are refused, which they never are
/// for the constants above.
pub fn latency_histogram() -> Result<Histogram<u64>, DriveError> {
    Histogram::new_with_bounds(1, HDR_MAX_US, HDR_SIGNIFICANT_FIGURES)
        .map_err(|e| DriveError::Histogram(e.to_string()))
}

/// Microseconds in `duration`, saturating at `u64::MAX`.
fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

/// One measured window, as offsets from the profile's start: `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSpec {
    /// The first offset the window holds.
    pub start: Duration,
    /// The first offset past the window.
    pub end: Duration,
}

impl WindowSpec {
    fn holds(&self, offset: Duration) -> bool {
        self.start <= offset && offset < self.end
    }
}

/// The generator's own limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorLimits {
    /// The most exchanges outstanding at once.
    pub in_flight_budget: u32,
    /// How long one send may wait for an in-flight slot before the run stops
    /// as saturated.
    pub saturation_abort: Duration,
    /// The lag p99 above which a window the server kept up with is
    /// `generator_bound`.
    pub lag_limit: Duration,
    /// How long the driver waits for outstanding exchanges after the last
    /// arrival.
    pub drain: Duration,
    /// How often the in-flight count is sampled.
    pub in_flight_sample_every: Duration,
    /// How many failed exchanges each window keeps per operation.
    pub failed_samples_per_operation: usize,
}

impl Default for GeneratorLimits {
    fn default() -> Self {
        Self {
            in_flight_budget: 4_096,
            saturation_abort: Duration::from_secs(5),
            lag_limit: Duration::from_millis(5),
            drain: Duration::from_secs(10),
            in_flight_sample_every: Duration::from_millis(250),
            failed_samples_per_operation: 16,
        }
    }
}

/// A handle that stops a drive's dispatching from outside it.
///
/// Setting it is final: the dispatcher sends no arrival intended after the
/// moment it sees the handle set, sessions in progress send no further
/// stage, and the drive ends with [`Stop::Halted`]. A clone shares the one
/// flag.
#[derive(Debug, Clone, Default)]
pub struct Halt(Arc<AtomicBool>);

impl Halt {
    /// A handle that is not set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the handle.
    pub fn set(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Whether the handle is set.
    #[must_use]
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Everything one drive needs besides the mix and the performer.
#[derive(Debug, Clone)]
pub struct DrivePlan {
    seed: u64,
    process: ArrivalProcess,
    profile: RateProfile,
    windows: Vec<WindowSpec>,
    limits: GeneratorLimits,
    halt: Option<Halt>,
}

impl DrivePlan {
    /// Builds a plan, refusing windows that are empty, overlap, run out of
    /// order or reach past the profile, and an in-flight budget of zero.
    ///
    /// # Errors
    /// [`DriveError::Plan`] naming the refused element.
    pub fn new(
        seed: u64,
        process: ArrivalProcess,
        profile: RateProfile,
        windows: Vec<WindowSpec>,
        limits: GeneratorLimits,
    ) -> Result<Self, DriveError> {
        if limits.in_flight_budget == 0 {
            return Err(DriveError::Plan("the in-flight budget is zero".to_owned()));
        }
        if limits.in_flight_sample_every.is_zero() {
            return Err(DriveError::Plan(
                "the in-flight sampling interval is zero".to_owned(),
            ));
        }
        let mut previous_end = Duration::ZERO;
        for (index, window) in windows.iter().enumerate() {
            if window.start >= window.end {
                return Err(DriveError::Plan(format!("window {index} is empty")));
            }
            if window.start < previous_end {
                return Err(DriveError::Plan(format!(
                    "window {index} overlaps or precedes the window before it"
                )));
            }
            if window.end > profile.duration() {
                return Err(DriveError::Plan(format!(
                    "window {index} reaches past the profile"
                )));
            }
            previous_end = window.end;
        }
        Ok(Self {
            seed,
            process,
            profile,
            windows,
            limits,
            halt: None,
        })
    }

    /// The plan with a [`Halt`] handle the dispatcher watches.
    #[must_use]
    pub fn with_halt(mut self, halt: Halt) -> Self {
        self.halt = Some(halt);
        self
    }

    /// The seed every draw derives from.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The rate profile.
    #[must_use]
    pub fn profile(&self) -> &RateProfile {
        &self.profile
    }

    /// The measured windows.
    #[must_use]
    pub fn windows(&self) -> &[WindowSpec] {
        &self.windows
    }

    /// The generator's limits.
    #[must_use]
    pub fn limits(&self) -> &GeneratorLimits {
        &self.limits
    }

    /// The planned arrivals for `mix`, a pure function of the plan.
    #[must_use]
    pub fn arrivals(&self, mix: &Mix) -> Vec<Arrival> {
        intended_arrivals(self.seed, self.process, &self.profile, mix)
    }
}

/// What one operation recorded in one window.
#[derive(Debug, Clone)]
pub struct OperationRecord {
    /// Latencies of the exchanges that succeeded, in microseconds.
    pub ok: Histogram<u64>,
    /// Latencies of the exchanges that failed, in microseconds.
    pub failed: Histogram<u64>,
    /// Failed exchanges by class; the counts sum to `failed.len()`.
    pub errors: BTreeMap<ErrorClass, u64>,
    /// The first failed exchanges, at most the limit's count.
    pub samples: Vec<FailedExchange>,
    /// Refusals counted apart from errors ([`Outcome::Refused`]).
    pub refused: u64,
}

impl OperationRecord {
    fn new() -> Result<Self, DriveError> {
        Ok(Self {
            ok: latency_histogram()?,
            failed: latency_histogram()?,
            errors: BTreeMap::new(),
            samples: Vec::new(),
            refused: 0,
        })
    }
}

/// What one window recorded.
#[derive(Debug, Clone)]
pub struct WindowRecord {
    /// The window's bounds.
    pub spec: WindowSpec,
    /// Per operation, the exchanges intended inside the window.
    pub operations: BTreeMap<OperationId, OperationRecord>,
    /// The generator's lag (actual send minus intended) per exchange, in
    /// microseconds.
    pub lag: Histogram<u64>,
    /// The in-flight count, sampled at the plan's interval.
    pub in_flight: Vec<u32>,
    /// The total time sends intended in the window waited for a slot.
    pub budget_wait: Duration,
    /// Whether any send intended in the window found the budget exhausted.
    pub budget_exhausted: bool,
}

impl WindowRecord {
    fn new(spec: WindowSpec) -> Result<Self, DriveError> {
        Ok(Self {
            spec,
            operations: BTreeMap::new(),
            lag: latency_histogram()?,
            in_flight: Vec::new(),
            budget_wait: Duration::ZERO,
            budget_exhausted: false,
        })
    }

    /// Whether the generator, not the server, failed to hold this window:
    /// its lag p99 exceeds `lag_limit` while the budget never ran out.
    #[must_use]
    pub fn generator_bound(&self, lag_limit: Duration) -> bool {
        !self.budget_exhausted && self.lag.value_at_quantile(0.99) > micros(lag_limit)
    }
}

/// Why a drive ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// Every planned arrival was dispatched.
    Completed,
    /// One send waited the saturation limit for an in-flight slot, and the
    /// driver stopped dispatching at this offset.
    Saturated {
        /// The offset from the profile's start at which the run stopped.
        at: Duration,
    },
    /// The plan's [`Halt`] handle was set, and the driver stopped
    /// dispatching at this offset.
    Halted {
        /// The offset from the profile's start at which the run stopped.
        at: Duration,
    },
}

impl Stop {
    /// The offset at which dispatching stopped early, if it did.
    #[must_use]
    pub fn stopped_at(self) -> Option<Duration> {
        match self {
            Stop::Completed => None,
            Stop::Saturated { at } | Stop::Halted { at } => Some(at),
        }
    }
}

/// The outcome of one drive.
#[derive(Debug, Clone)]
pub struct DriveReport {
    /// One record per planned window, in plan order.
    pub windows: Vec<WindowRecord>,
    /// Why the drive ended.
    pub stop: Stop,
    /// How many arrivals the schedule planned.
    pub planned: u64,
    /// How many of them were dispatched.
    pub dispatched: u64,
    /// Exchanges intended outside every window: performed and not recorded.
    pub unwindowed: u64,
    /// Sessions whose next stage fell past the profile's end or after the
    /// stop, and was never sent.
    pub truncated_sessions: u64,
    /// The drive's wall time, drain included.
    pub elapsed: Duration,
}

/// A drive that could not run to its end.
#[derive(Debug, Error)]
pub enum DriveError {
    /// A histogram could not be built.
    #[error("latency histogram: {0}")]
    Histogram(String),
    /// The plan was refused.
    #[error("drive plan: {0}")]
    Plan(String),
    /// A performer task panicked, which is a defect in the performer.
    #[error("a performer task panicked: {0}")]
    PerformerPanicked(String),
    /// The runtime could not be built.
    #[error("generator runtime: {0}")]
    Runtime(#[source] std::io::Error),
}

/// One exchange on the wire.
struct Pending {
    intended: Instant,
    operation: OperationId,
}

/// The state every task of one drive shares.
struct Shared {
    start: Instant,
    end: Instant,
    seed: u64,
    limits: GeneratorLimits,
    semaphore: Arc<Semaphore>,
    windows: Mutex<Vec<WindowRecord>>,
    pending: Mutex<BTreeMap<u64, Pending>>,
    next_ticket: AtomicU64,
    unwindowed: AtomicU64,
    truncated: AtomicU64,
    aborted: AtomicBool,
    abort_at: Mutex<Option<Instant>>,
    halt: Option<Halt>,
    halted_at: Mutex<Option<Instant>>,
}

impl Shared {
    fn window_of(windows: &[WindowRecord], offset: Duration) -> Option<usize> {
        windows.iter().position(|w| w.spec.holds(offset))
    }

    fn with_window(&self, at: Instant, update: impl FnOnce(&mut WindowRecord)) -> bool {
        let offset = at.saturating_duration_since(self.start);
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        match Self::window_of(&windows, offset).and_then(|i| windows.get_mut(i)) {
            Some(window) => {
                update(window);
                true
            }
            None => false,
        }
    }

    fn aborted(&self) -> bool {
        self.aborted.load(Ordering::Acquire) || self.halt.as_ref().is_some_and(Halt::is_set)
    }

    /// Notes the first moment the dispatcher saw the halt handle set.
    fn note_halt(&self, at: Instant) {
        let mut slot = self
            .halted_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(at);
        }
    }

    fn trip_abort(&self, at: Instant) {
        let mut slot = self.abort_at.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(at);
        }
        self.aborted.store(true, Ordering::Release);
    }

    /// Takes an in-flight slot for a send intended at `intended`, recording
    /// any wait against that instant's window.
    async fn acquire(&self, intended: Instant) -> Option<OwnedSemaphorePermit> {
        match Arc::clone(&self.semaphore).try_acquire_owned() {
            Ok(permit) => return Some(permit),
            Err(TryAcquireError::Closed) => return None,
            Err(TryAcquireError::NoPermits) => {}
        }
        let began = Instant::now();
        let waited = timeout(
            self.limits.saturation_abort,
            Arc::clone(&self.semaphore).acquire_owned(),
        )
        .await;
        let wait = began.elapsed();
        self.with_window(intended, |w| {
            w.budget_exhausted = true;
            w.budget_wait = w.budget_wait.saturating_add(wait);
        });
        match waited {
            Ok(Ok(permit)) => Some(permit),
            Ok(Err(_closed)) => None,
            Err(_elapsed) => {
                self.trip_abort(Instant::now());
                None
            }
        }
    }

    fn register(&self, intended: Instant, operation: &OperationId) -> u64 {
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        let lag = Instant::now().saturating_duration_since(intended);
        if !self.with_window(intended, |w| w.lag.saturating_record(micros(lag))) {
            self.unwindowed.fetch_add(1, Ordering::Relaxed);
        }
        let _replaced = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                ticket,
                Pending {
                    intended,
                    operation: operation.clone(),
                },
            );
        ticket
    }

    /// Records the end of exchange `ticket`; `false` when the drain already
    /// recorded it as unfinished.
    fn complete(&self, ticket: u64, done: Instant, outcome: Outcome) -> Result<bool, DriveError> {
        let Some(pending) = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&ticket)
        else {
            return Ok(false);
        };
        self.record(&pending, done, outcome)?;
        Ok(true)
    }

    fn record(&self, pending: &Pending, done: Instant, outcome: Outcome) -> Result<(), DriveError> {
        let latency = micros(done.saturating_duration_since(pending.intended));
        let sample_limit = self.limits.failed_samples_per_operation;
        let mut fault = None;
        self.with_window(pending.intended, |w| {
            let record = match w.operations.get_mut(&pending.operation) {
                Some(record) => record,
                None => match OperationRecord::new() {
                    Ok(fresh) => w
                        .operations
                        .entry(pending.operation.clone())
                        .or_insert(fresh),
                    Err(e) => {
                        fault = Some(e);
                        return;
                    }
                },
            };
            match outcome {
                Outcome::Success => record.ok.saturating_record(latency),
                Outcome::Refused => record.refused = record.refused.saturating_add(1),
                Outcome::Failure { class, exchange } => {
                    record.failed.saturating_record(latency);
                    let count = record.errors.entry(class).or_insert(0);
                    *count = count.saturating_add(1);
                    if let Some(exchange) = exchange
                        && record.samples.len() < sample_limit
                    {
                        record.samples.push(exchange);
                    }
                }
            }
        });
        fault.map_or(Ok(()), Err)
    }

    /// Records every exchange still on the wire as unfinished at `deadline`.
    fn flush_unfinished(&self, deadline: Instant) -> Result<(), DriveError> {
        let pending =
            std::mem::take(&mut *self.pending.lock().unwrap_or_else(PoisonError::into_inner));
        for exchange in pending.values() {
            self.record(
                exchange,
                deadline,
                Outcome::Failure {
                    class: ErrorClass::Unfinished,
                    exchange: None,
                },
            )?;
        }
        Ok(())
    }

    fn sample_in_flight(&self, at: Instant) {
        let budget = usize::try_from(self.limits.in_flight_budget).unwrap_or(usize::MAX);
        let in_flight = budget.saturating_sub(self.semaphore.available_permits());
        let in_flight = u32::try_from(in_flight).unwrap_or(u32::MAX);
        self.with_window(at, |w| w.in_flight.push(in_flight));
    }
}

/// Runs one arrival's interaction to its end.
async fn interaction<P: Performer>(
    shared: Arc<Shared>,
    mix: Arc<Mix>,
    performer: Arc<P>,
    arrival: Arrival,
    first_permit: OwnedSemaphorePermit,
) -> Result<(), DriveError> {
    let Some(interaction) = mix.interactions().get(arrival.interaction) else {
        return Ok(());
    };
    let mut intended = shared.start.checked_add(arrival.at).unwrap_or(shared.end);
    let mut permit = Some(first_permit);
    let mut stage: u32 = 0;
    while let Some(operation) = interaction.operation(stage) {
        let held = match permit.take() {
            Some(held) => held,
            None => match shared.acquire(intended).await {
                Some(held) => held,
                None => break,
            },
        };
        let ticket = shared.register(intended, operation);
        let call = Call {
            arrival: arrival.index,
            stage,
            interaction: arrival.interaction,
            operation: operation.clone(),
        };
        let outcome = performer.perform(&call).await;
        let done = Instant::now();
        drop(held);
        if !shared.complete(ticket, done, outcome)? {
            break;
        }
        let Some(follow) = usize::try_from(stage)
            .ok()
            .and_then(|i| interaction.follow_ups().get(i))
        else {
            break;
        };
        stage = stage.saturating_add(1);
        let think = think_time(shared.seed, arrival.index, stage, follow.think_mean);
        intended = done.checked_add(think).unwrap_or(shared.end);
        if intended >= shared.end || shared.aborted() {
            shared.truncated.fetch_add(1, Ordering::Relaxed);
            break;
        }
        sleep_until(intended).await;
    }
    Ok(())
}

fn joined(result: Result<Result<(), DriveError>, JoinError>) -> Result<(), DriveError> {
    match result {
        Ok(outcome) => outcome,
        Err(e) if e.is_panic() => Err(DriveError::PerformerPanicked(e.to_string())),
        Err(_cancelled) => Ok(()),
    }
}

/// Samples the in-flight count at the plan's interval until the profile ends
/// or the run stops.
async fn sample_in_flight(shared: Arc<Shared>) {
    let mut at = shared.start;
    while let Some(next) = at.checked_add(shared.limits.in_flight_sample_every) {
        if next >= shared.end || shared.aborted() {
            break;
        }
        sleep_until(next).await;
        shared.sample_in_flight(next);
        at = next;
    }
}

/// Waits for outstanding tasks until `deadline`, then cancels the rest and
/// waits for the cancellations.
async fn drain(
    tasks: &mut JoinSet<Result<(), DriveError>>,
    deadline: Instant,
) -> Result<(), DriveError> {
    while let Ok(Some(result)) = timeout_at(deadline, tasks.join_next()).await {
        joined(result)?;
    }
    tasks.abort_all();
    while let Some(result) = tasks.join_next().await {
        joined(result)?;
    }
    Ok(())
}

/// Drives `mix` through `plan` against `performer` on the current runtime.
///
/// Arrivals fire at their intended instants whatever the server does, every
/// latency runs from the intended instant, and each exchange lands in the
/// window holding that instant. After the last arrival the driver waits up
/// to the drain limit, then records whatever is still outstanding as
/// [`ErrorClass::Unfinished`] at the time it had waited by then.
///
/// # Errors
/// [`DriveError::PerformerPanicked`] when a performer task panics, and
/// [`DriveError::Histogram`] if a histogram cannot be built.
#[expect(
    clippy::too_many_lines,
    reason = "one dispatch loop with its drain and its stop reading"
)]
pub async fn drive<P: Performer>(
    plan: &DrivePlan,
    mix: &Mix,
    performer: Arc<P>,
) -> Result<DriveReport, DriveError> {
    let arrivals = plan.arrivals(mix);
    let start = Instant::now();
    let end = start
        .checked_add(plan.profile.duration())
        .ok_or_else(|| DriveError::Plan("the profile is too long".to_owned()))?;
    let windows = plan
        .windows
        .iter()
        .map(|spec| WindowRecord::new(*spec))
        .collect::<Result<Vec<_>, _>>()?;
    let budget = usize::try_from(plan.limits.in_flight_budget).unwrap_or(usize::MAX);
    let shared = Arc::new(Shared {
        start,
        end,
        seed: plan.seed,
        limits: plan.limits.clone(),
        semaphore: Arc::new(Semaphore::new(budget.min(Semaphore::MAX_PERMITS))),
        windows: Mutex::new(windows),
        pending: Mutex::new(BTreeMap::new()),
        next_ticket: AtomicU64::new(0),
        unwindowed: AtomicU64::new(0),
        truncated: AtomicU64::new(0),
        aborted: AtomicBool::new(false),
        abort_at: Mutex::new(None),
        halt: plan.halt.clone(),
        halted_at: Mutex::new(None),
    });
    let mix = Arc::new(mix.clone());

    let sampler = tokio::spawn(sample_in_flight(Arc::clone(&shared)));
    let mut tasks = JoinSet::new();
    let mut dispatched: u64 = 0;
    for arrival in &arrivals {
        if shared.aborted() {
            break;
        }
        let intended = start.checked_add(arrival.at).unwrap_or(end);
        if Instant::now() < intended {
            sleep_until(intended).await;
        }
        if shared.halt.as_ref().is_some_and(Halt::is_set) {
            shared.note_halt(Instant::now());
            break;
        }
        let Some(permit) = shared.acquire(intended).await else {
            break;
        };
        dispatched = dispatched.saturating_add(1);
        tasks.spawn(interaction(
            Arc::clone(&shared),
            Arc::clone(&mix),
            Arc::clone(&performer),
            *arrival,
            permit,
        ));
        while let Some(result) = tasks.try_join_next() {
            joined(result)?;
        }
    }

    let stopped = *shared
        .abort_at
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let halted = *shared
        .halted_at
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let deadline = stopped
        .or(halted)
        .unwrap_or(end)
        .max(Instant::now())
        .checked_add(plan.limits.drain)
        .unwrap_or(end);
    drain(&mut tasks, deadline).await?;
    sampler.abort();
    shared.flush_unfinished(deadline)?;

    let windows = std::mem::take(
        &mut *shared
            .windows
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
    );
    let stop = match (stopped, halted) {
        (Some(at), _) => Stop::Saturated {
            at: at.saturating_duration_since(start),
        },
        (None, Some(at)) => Stop::Halted {
            at: at.saturating_duration_since(start),
        },
        (None, None) => Stop::Completed,
    };
    Ok(DriveReport {
        windows,
        stop,
        planned: u64::try_from(arrivals.len()).unwrap_or(u64::MAX),
        dispatched,
        unwindowed: shared.unwindowed.load(Ordering::Relaxed),
        truncated_sessions: shared.truncated.load(Ordering::Relaxed),
        elapsed: start.elapsed(),
    })
}

/// Drives `mix` through `plan` on a fresh multi-thread runtime, blocking the
/// calling thread until the drive ends.
///
/// `workers` sets the runtime's worker threads; `None` takes `tokio`'s
/// default of one per core.
///
/// # Errors
/// [`DriveError::Runtime`] when the runtime cannot be built, and every error
/// of [`drive`].
pub fn drive_blocking<P: Performer>(
    plan: &DrivePlan,
    mix: &Mix,
    performer: Arc<P>,
    workers: Option<usize>,
) -> Result<DriveReport, DriveError> {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();
    if let Some(workers) = workers {
        builder.worker_threads(workers.max(1));
    }
    let runtime = builder.build().map_err(DriveError::Runtime)?;
    runtime.block_on(drive(plan, mix, performer))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    use super::*;
    use crate::perf_gen::arrivals::{Rate, think_time};
    use crate::perf_gen::{FollowUp, Interaction};

    fn paused_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap()
    }

    fn op(name: &str) -> OperationId {
        OperationId::new(name).unwrap()
    }

    fn single_mix() -> Mix {
        Mix::new(vec![Interaction::single("read", op("read"), 1).unwrap()]).unwrap()
    }

    fn window(start_ms: u64, end_ms: u64) -> WindowSpec {
        WindowSpec {
            start: Duration::from_millis(start_ms),
            end: Duration::from_millis(end_ms),
        }
    }

    /// A fake server answering in `service`, except that everything arriving
    /// in `[stall_from, stall_to)` waits until `stall_to`.
    struct StallingSut {
        start: Instant,
        stall_from: Duration,
        stall_to: Duration,
        service: Duration,
    }

    impl Performer for StallingSut {
        async fn perform(&self, _call: &Call) -> Outcome {
            let offset = Instant::now().saturating_duration_since(self.start);
            if self.stall_from <= offset && offset < self.stall_to {
                sleep_until(self.start + self.stall_to + self.service).await;
            } else {
                tokio::time::sleep(self.service).await;
            }
            Outcome::Success
        }
    }

    fn p99_ms(record: &WindowRecord) -> u64 {
        record.operations[&op("read")]
            .ok
            .value_at_quantile(0.99)
            .div_euclid(1_000)
    }

    #[test]
    fn a_stall_raises_the_p99_of_every_arrival_intended_during_it() {
        paused_runtime().block_on(async {
            let profile =
                RateProfile::constant(Rate::new(100.0).unwrap(), Duration::from_secs(10)).unwrap();
            let plan = DrivePlan::new(
                42,
                ArrivalProcess::Poisson,
                profile,
                vec![
                    window(0, 4_000),
                    window(4_000, 6_000),
                    window(6_000, 10_000),
                ],
                GeneratorLimits::default(),
            )
            .unwrap();
            let sut = Arc::new(StallingSut {
                start: Instant::now(),
                stall_from: Duration::from_secs(4),
                stall_to: Duration::from_secs(6),
                service: Duration::from_millis(2),
            });
            let report = drive(&plan, &single_mix(), sut).await.unwrap();
            assert_eq!(report.stop, Stop::Completed);
            let [before, during, after] = &*report.windows else {
                panic!("three windows");
            };
            assert!(p99_ms(before) < 10, "before: {}", p99_ms(before));
            assert!(p99_ms(during) >= 1_900, "during: {}", p99_ms(during));
            assert!(p99_ms(after) < 10, "after: {}", p99_ms(after));
            // Every arrival intended in the stall is recorded: none omitted.
            let stalled = during.operations[&op("read")].ok.len();
            assert!((150..=250).contains(&stalled), "{stalled}");
            let total: u64 = report
                .windows
                .iter()
                .map(|w| w.operations[&op("read")].ok.len())
                .sum();
            assert_eq!(total, report.dispatched);
        });
    }

    /// Logs when each stage of each arrival was sent.
    struct Recorder {
        start: Instant,
        service: Duration,
        sends: Mutex<Vec<(u64, u32, Duration)>>,
    }

    impl Performer for Recorder {
        async fn perform(&self, call: &Call) -> Outcome {
            let sent = Instant::now().saturating_duration_since(self.start);
            self.sends
                .lock()
                .unwrap()
                .push((call.arrival, call.stage, sent));
            tokio::time::sleep(self.service).await;
            Outcome::Success
        }
    }

    #[test]
    fn a_session_stage_waits_for_the_answer_and_its_think_time() {
        paused_runtime().block_on(async {
            let think_mean = Duration::from_secs(1);
            let mix = Mix::new(vec![
                Interaction::session(
                    "visit",
                    1,
                    op("open"),
                    vec![FollowUp {
                        think_mean,
                        operation: op("read"),
                    }],
                )
                .unwrap(),
            ])
            .unwrap();
            let profile =
                RateProfile::constant(Rate::new(5.0).unwrap(), Duration::from_secs(20)).unwrap();
            let plan = DrivePlan::new(
                9,
                ArrivalProcess::Poisson,
                profile,
                vec![window(0, 20_000)],
                GeneratorLimits::default(),
            )
            .unwrap();
            let service = Duration::from_millis(500);
            let recorder = Arc::new(Recorder {
                start: Instant::now(),
                service,
                sends: Mutex::new(Vec::new()),
            });
            let report = drive(&plan, &mix, Arc::clone(&recorder)).await.unwrap();
            let sends = recorder.sends.lock().unwrap().clone();
            let firsts: BTreeMap<u64, Duration> = sends
                .iter()
                .filter(|s| s.1 == 0)
                .map(|s| (s.0, s.2))
                .collect();
            let mut checked = 0;
            for (arrival, _, sent) in sends.iter().filter(|s| s.1 == 1) {
                let earliest = firsts[arrival] + service + think_time(9, *arrival, 1, think_mean);
                assert!(*sent >= earliest, "stage 1 of {arrival} went early");
                assert!(
                    sent.saturating_sub(earliest) < Duration::from_millis(2),
                    "stage 1 of {arrival} went late"
                );
                checked += 1;
            }
            assert!(checked > 50, "{checked} sessions reached stage 1");
            assert_eq!(
                u64::try_from(checked).unwrap() + report.truncated_sessions,
                report.dispatched
            );
        });
    }

    #[test]
    fn an_exhausted_budget_is_recorded_and_stops_the_run_as_saturated() {
        paused_runtime().block_on(async {
            let profile =
                RateProfile::constant(Rate::new(10.0).unwrap(), Duration::from_secs(20)).unwrap();
            let limits = GeneratorLimits {
                in_flight_budget: 2,
                drain: Duration::from_secs(1),
                ..GeneratorLimits::default()
            };
            let plan = DrivePlan::new(
                3,
                ArrivalProcess::Uniform,
                profile,
                vec![window(0, 20_000)],
                limits,
            )
            .unwrap();
            let sut = Arc::new(StallingSut {
                start: Instant::now(),
                stall_from: Duration::ZERO,
                stall_to: Duration::ZERO,
                service: Duration::from_secs(60),
            });
            let report = drive(&plan, &single_mix(), sut).await.unwrap();
            // The third arrival, at 0.3 s, waits 5 s for a slot.
            assert_eq!(
                report.stop,
                Stop::Saturated {
                    at: Duration::from_millis(5_300)
                }
            );
            assert_eq!(report.dispatched, 2);
            let window = &report.windows[0];
            assert!(window.budget_exhausted);
            assert_eq!(window.budget_wait, Duration::from_secs(5));
            assert!(!window.generator_bound(Duration::from_millis(5)));
            let read = &window.operations[&op("read")];
            assert_eq!(read.errors.get(&ErrorClass::Unfinished), Some(&2));
            assert_eq!(read.ok.len(), 0);
        });
    }

    /// Records which OS thread ran each exchange.
    struct ThreadLog(Mutex<BTreeSet<String>>);

    impl Performer for ThreadLog {
        async fn perform(&self, _call: &Call) -> Outcome {
            let id = format!("{:?}", std::thread::current().id());
            let _fresh = self.0.lock().unwrap().insert(id);
            tokio::time::sleep(Duration::from_millis(200)).await;
            Outcome::Success
        }
    }

    #[test]
    fn arrivals_are_tasks_and_never_an_os_thread_each() {
        paused_runtime().block_on(async {
            let profile =
                RateProfile::constant(Rate::new(2_000.0).unwrap(), Duration::from_secs(2)).unwrap();
            let plan = DrivePlan::new(
                1,
                ArrivalProcess::Poisson,
                profile,
                vec![window(0, 2_000)],
                GeneratorLimits::default(),
            )
            .unwrap();
            let log = Arc::new(ThreadLog(Mutex::new(BTreeSet::new())));
            let report = drive(&plan, &single_mix(), Arc::clone(&log)).await.unwrap();
            assert!(report.dispatched > 3_500, "{}", report.dispatched);
            assert_eq!(
                log.0.lock().unwrap().len(),
                1,
                "one thread ran every exchange"
            );
            let peak = report.windows[0].in_flight.iter().max().copied().unwrap();
            assert!(peak > 300, "about 400 exchanges overlap, sampled {peak}");
        });
    }

    #[test]
    fn a_bad_plan_is_refused() {
        let profile =
            RateProfile::constant(Rate::new(1.0).unwrap(), Duration::from_secs(10)).unwrap();
        let plan = |windows| {
            DrivePlan::new(
                0,
                ArrivalProcess::Poisson,
                profile.clone(),
                windows,
                GeneratorLimits::default(),
            )
        };
        assert!(plan(vec![window(5, 5)]).is_err());
        assert!(plan(vec![window(0, 6), window(5, 9)]).is_err());
        assert!(plan(vec![window(0, 11_000)]).is_err());
        assert!(plan(vec![window(0, 5), window(5, 10)]).is_ok());
    }
}
