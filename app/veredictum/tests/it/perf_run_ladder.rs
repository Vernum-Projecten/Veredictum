// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The run after seeding against in-process fake servers of known capacity,
//! on a paused clock: the ladder brackets the capacity between two adjacent
//! plateaus, and the run takes at most five
//! minutes against a server of 1,000 operations per second and at most ten
//! against one of 30, with failing server-side readings leaving the run
//! intact. Virtual time stands in for the minutes
//! (<https://docs.rs/tokio/1/tokio/time/fn.pause.html>), so the criteria
//! cost seconds of CI.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::missing_assert_message,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken harness must abort the test loudly, Book ch11"
)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;
use veredictum::perf_emulate::binding::Binding;
use veredictum::perf_emulate::run::{
    LadderOutcome, RunHooks, RunRequest, RunShape, run_after_seeding,
};
use veredictum::perf_emulate::server::{ReadingError, ServerSampler, SqlReader};
use veredictum::perf_gen::arrivals::{ArrivalProcess, Rate};
use veredictum::perf_gen::drive::GeneratorLimits;
use veredictum::perf_gen::{Call, ErrorClass, Interaction, Mix, OperationId, Outcome, Performer};
use veredictum::perf_result::derive::plateau;
use veredictum::perf_result::server::{ContainersReading, DatabaseReading, ServerReadings};
use veredictum::perf_result::{Phase, RawQualification};
use veredictum::workload::Stage;
use veredictum::workload::ops::WorkloadOp;

/// A server of known capacity: it admits at most `capacity` requests per
/// second with a one-second burst, serves each admitted one in 2 ms, and
/// answers `503` to the rest, so it qualifies up to its capacity and fails
/// past it on errors.
struct Admitting {
    capacity: f64,
    bucket: Mutex<(f64, Option<Instant>)>,
}

impl Admitting {
    fn of_capacity(per_second: u32) -> Self {
        Self {
            capacity: f64::from(per_second),
            bucket: Mutex::new((f64::from(per_second), None)),
        }
    }
}

impl Performer for Admitting {
    async fn perform(&self, _call: &Call) -> Outcome {
        let admitted = {
            let mut bucket = self.bucket.lock().unwrap();
            let now = Instant::now();
            let refill = bucket
                .1
                .map_or(0.0, |last| (now - last).as_secs_f64() * self.capacity);
            bucket.0 = (bucket.0 + refill).min(self.capacity);
            bucket.1 = Some(now);
            if bucket.0 >= 1.0 {
                bucket.0 -= 1.0;
                true
            } else {
                false
            }
        };
        tokio::time::sleep(Duration::from_millis(2)).await;
        if admitted {
            Outcome::Success
        } else {
            Outcome::Failure {
                class: ErrorClass::ServerError,
                exchange: None,
            }
        }
    }
}

struct Quiet;

impl RunHooks for Quiet {
    fn progress(&self, _message: &str) {}

    async fn settle(&self) {}

    async fn closing(&self) {}
}

fn binding() -> Binding {
    let read = OperationId::new("ehr_read").unwrap();
    Binding {
        mix: Mix::new(vec![Interaction::single("read", read, 1_000_000).unwrap()]).unwrap(),
        stages: vec![vec![Stage {
            op: WorkloadOp::EhrRead,
            payload: None,
            query: None,
        }]],
        operations_per_interaction: 1.0,
        qualification: RawQualification {
            default_budget_ms: 1_000,
            budgets_ms: BTreeMap::new(),
            max_error_ppm: 1_000,
            min_achieved_ppm: 950_000,
        },
        dropped: Vec::new(),
    }
}

fn run_against(capacity: u32) -> (LadderOutcome, Binding) {
    let binding = binding();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();
    let outcome = runtime.block_on(async {
        let request = RunRequest {
            seed: 7,
            process: ArrivalProcess::Poisson,
            binding: &binding,
            limits: GeneratorLimits::default(),
            shape: RunShape::standard(5_000, Rate::new(8.0).unwrap(), Rate::new(4_096.0).unwrap()),
        };
        run_after_seeding(&request, Arc::new(Admitting::of_capacity(capacity)), &Quiet)
            .await
            .unwrap()
    });
    (outcome, binding)
}

/// The rates of the highest qualifying plateau below the first failing
/// one, and of that failing plateau.
fn bracket(outcome: &LadderOutcome, binding: &Binding) -> (f64, f64) {
    let mut judged: Vec<(f64, bool)> = outcome
        .plateaus
        .iter()
        .map(|p| {
            let derived = plateau(p, &binding.qualification, 5_000, "/raw/plateaus").unwrap();
            (p.planned_rate.per_second(), derived.qualification.qualified)
        })
        .collect();
    judged.sort_by(|a, b| a.0.total_cmp(&b.0));
    let failing = judged
        .iter()
        .position(|(_, ok)| !ok)
        .expect("a plateau fails");
    assert!(failing > 0, "the lowest plateau fails: {judged:?}");
    (judged[failing - 1].0, judged[failing].0)
}

fn assert_phases_recorded(outcome: &LadderOutcome) {
    let phases: Vec<Phase> = outcome.phases.iter().map(|p| p.phase).collect();
    assert_eq!(
        &phases[..5],
        &[
            Phase::Settle,
            Phase::WarmUp,
            Phase::Scout,
            Phase::Drain,
            Phase::Ladder
        ]
    );
    assert_eq!(phases.last(), Some(&Phase::Closing));
    let total: u64 = outcome.phases.iter().map(|p| p.wall_ms).sum();
    let elapsed = u64::try_from(outcome.elapsed.as_millis()).unwrap();
    assert!(
        total <= elapsed && elapsed - total < 1_000,
        "{total} vs {elapsed}"
    );
}

#[test]
fn a_fast_server_is_bracketed_in_at_most_five_minutes() {
    let (outcome, binding) = run_against(1_000);
    let (below, above) = bracket(&outcome, &binding);
    assert!(below <= 1_000.0 && 1_000.0 <= above, "{below} .. {above}");
    assert!(
        outcome.elapsed <= Duration::from_secs(300),
        "{:?}",
        outcome.elapsed
    );
    assert_phases_recorded(&outcome);
}

#[test]
fn a_slow_server_is_bracketed_in_at_most_ten_minutes() {
    let (outcome, binding) = run_against(30);
    let (below, above) = bracket(&outcome, &binding);
    assert!(below <= 30.0 && 30.0 <= above, "{below} .. {above}");
    assert!(
        outcome.elapsed <= Duration::from_secs(600),
        "{:?}",
        outcome.elapsed
    );
    assert_phases_recorded(&outcome);
}

/// A database that refuses every reading.
struct RefusingDatabase;

impl SqlReader for RefusingDatabase {
    fn query(
        &self,
        _sql: &'static str,
    ) -> impl Future<Output = Result<String, ReadingError>> + Send {
        std::future::ready(Err(ReadingError::Sql("connection refused".to_owned())))
    }
}

/// Hooks whose server-side sampler fails at every reading.
struct FailingReadings(ServerSampler<RefusingDatabase>);

impl RunHooks for FailingReadings {
    fn progress(&self, _message: &str) {}

    async fn settle(&self) {}

    async fn closing(&self) {}

    async fn plateau_began(&self, _index: usize) {
        self.0.begin().await;
    }

    async fn plateau_ended(&self, _index: usize) -> ServerReadings {
        self.0.end().await
    }
}

#[test]
fn failing_server_side_readings_leave_the_run_and_its_ladder_intact() {
    let binding = binding();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();
    let hooks = FailingReadings(ServerSampler::new(
        Err("the IXIT declares no `containers`".to_owned()),
        Ok(Arc::new(RefusingDatabase)),
    ));
    let outcome = runtime.block_on(async {
        let request = RunRequest {
            seed: 7,
            process: ArrivalProcess::Poisson,
            binding: &binding,
            limits: GeneratorLimits::default(),
            shape: RunShape::standard(5_000, Rate::new(8.0).unwrap(), Rate::new(4_096.0).unwrap()),
        };
        run_after_seeding(&request, Arc::new(Admitting::of_capacity(1_000)), &hooks)
            .await
            .unwrap()
    });
    let (below, above) = bracket(&outcome, &binding);
    assert!(below <= 1_000.0 && 1_000.0 <= above, "{below} .. {above}");
    assert!(outcome.plateaus.len() >= 4);
    for plateau in &outcome.plateaus {
        assert!(
            matches!(&plateau.server.database, DatabaseReading::Absent { reason } if reason.contains("connection refused")),
            "{:?}",
            plateau.server.database
        );
        assert!(matches!(
            &plateau.server.containers,
            ContainersReading::Absent { .. }
        ));
    }
}
