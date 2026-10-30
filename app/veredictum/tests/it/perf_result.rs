// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `perf-result` record and `perf-judge`: a record re-derives byte for
//! byte, a tampered derived field is named by its pointer, percentiles come
//! from summed histograms, a quantile with too few samples above it is
//! withheld, and every record states what one pass cannot show.

#![expect(
    clippy::unwrap_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken fixture must abort the test loudly, Book ch11"
)]

use std::collections::BTreeMap;
use std::process::Command;
use std::time::Duration;

use serde_json::Value;
use veredictum::bench::fingerprint::EnvironmentFingerprint;
use veredictum::identity::{IdentityRead, OptionsDocument};
use veredictum::ixit::Environment;
use veredictum::perf_gen::arrivals::Rate;
use veredictum::perf_gen::drive::{OperationRecord, WindowRecord, WindowSpec, latency_histogram};
use veredictum::perf_gen::{ErrorClass, FailedExchange, OperationId};
use veredictum::perf_result::capacity::{Knee, PeakSource, PopulationReading, UslReading};
use veredictum::perf_result::derive::{BetweenRunNoise, JudgedAt, Steadiness};
use veredictum::perf_result::judge::rejudge;
use veredictum::perf_result::quantile::{Quantile, QuantileName, quantile};
use veredictum::perf_result::server::SettingsReading;
use veredictum::perf_result::{
    CorpusOrigin, CorpusRecord, EmbeddedWorkload, Generator, Instrument, LoadModel, PerfResult,
    Phase, PhaseRecord, Raw, RawMix, RawPlateau, RawQualification, RawScout, RawScoutWindow,
    RawWindow, SCOPE, SeedingRecord, Target, TargetOptions,
};
use veredictum::pipeline::perf_workload::refuse_rate_limited;
use veredictum::workload::CorpusScale;

fn op(name: &str) -> OperationId {
    OperationId::new(name).unwrap()
}

/// An operation record of successful latencies (µs) and failures.
fn operation(ok_us: &[u64], failures: &[(ErrorClass, u64)]) -> OperationRecord {
    let mut ok = latency_histogram().unwrap();
    for v in ok_us {
        ok.record(*v).unwrap();
    }
    let mut failed = latency_histogram().unwrap();
    let mut errors = BTreeMap::new();
    let mut samples = Vec::new();
    for (class, latency) in failures {
        failed.record(*latency).unwrap();
        *errors.entry(*class).or_insert(0) += 1;
        samples.push(FailedExchange::new(
            Some(reqwest::StatusCode::SERVICE_UNAVAILABLE),
            "POST /ehr",
            "busy",
        ));
    }
    OperationRecord {
        ok,
        failed,
        errors,
        samples,
        refused: 0,
    }
}

fn window(start_s: u64, end_s: u64, ops: Vec<(&str, OperationRecord)>) -> WindowRecord {
    let mut lag = latency_histogram().unwrap();
    let mut in_flight = Vec::new();
    for (_, record) in &ops {
        for _ in 0..(record.ok.len() + record.failed.len()) {
            lag.record(300).unwrap();
        }
        in_flight.push(4);
    }
    WindowRecord {
        spec: WindowSpec {
            start: Duration::from_secs(start_s),
            end: Duration::from_secs(end_s),
        },
        operations: ops.into_iter().map(|(n, r)| (op(n), r)).collect(),
        lag,
        in_flight,
        budget_wait: Duration::ZERO,
        budget_exhausted: false,
    }
}

/// `n` latencies spread evenly over `[low_us, low_us + n * step_us)`.
fn spread(n: u64, low_us: u64, step_us: u64) -> Vec<u64> {
    (0..n).map(|i| low_us + i * step_us).collect()
}

pub(crate) fn raw(plateaus: Vec<RawPlateau>) -> Raw {
    Raw {
        instrument: Instrument::this_build(),
        target: Target {
            base_url: "http://cdr.test/openehr/v1".to_owned(),
            options: TargetOptions::Answered {
                answer: IdentityRead::Offered {
                    status: 200,
                    options: OptionsDocument {
                        solution: Some("fake".to_owned()),
                        ..OptionsDocument::default()
                    },
                },
            },
        },
        environment: Environment {
            exclusive_server: true,
            hardware_class: "test".to_owned(),
            cores: 8,
            memory_gb: 8,
            storage_class: "ssd".to_owned(),
            topology: "single".to_owned(),
        },
        generator: Generator {
            host: EnvironmentFingerprint {
                arch: "x86_64".to_owned(),
                os: "linux".to_owned(),
                available_parallelism: Some(8),
                cpu_model: None,
                total_memory_bytes: None,
            },
            in_flight_budget: 4_096,
            lag_limit_us: 5_000,
            workers: 8,
        },
        host_prescription: None,
        network_leg: None,
        interleaving: None,
        aa_floor: None,
        posture: None,
        workload: EmbeddedWorkload {
            id: "reference".to_owned(),
            label: "custom".to_owned(),
            version: "1".to_owned(),
            sha256: format!("sha256:{}", "0".repeat(64)),
            document: "id: reference\n".to_owned(),
        },
        corpus: CorpusRecord {
            scale: CorpusScale::S,
            recipe_sha256: format!("sha256:{}", "1".repeat(64)),
            digest: format!("sha256:{}", "2".repeat(64)),
            origin: CorpusOrigin::Seeded,
            ehrs: 100,
            compositions: 1_000,
        },
        seeding: SeedingRecord {
            model: LoadModel::ClosedLoop,
            workers: 4,
            requests: 1_100,
            wall_ms: 2_000,
        },
        mix: RawMix {
            operations_per_interaction: 1.0,
            dropped: Vec::new(),
        },
        qualification: RawQualification {
            default_budget_ms: 1_000,
            budgets_ms: BTreeMap::new(),
            max_error_ppm: 1_000,
            min_achieved_ppm: 950_000,
        },
        seed: 42,
        server_settings: SettingsReading::default(),
        phases: Vec::new(),
        scout: None,
        plateaus,
    }
}

/// One plateau of 5,000 samples: a read at a 90 % share and a write at 10 %,
/// with a few write failures.
fn ladder_plateau() -> RawPlateau {
    let first = window(
        5,
        35,
        vec![
            ("read", operation(&spread(2_250, 5_000, 4), &[])),
            (
                "write",
                operation(&spread(248, 20_000, 40), &[(ErrorClass::ServerError, 900)]),
            ),
        ],
    );
    let second = window(
        35,
        65,
        vec![
            ("read", operation(&spread(2_250, 5_000, 4), &[])),
            (
                "write",
                operation(
                    &spread(250, 20_000, 40),
                    &[(ErrorClass::Timeout, 10_000_000)],
                ),
            ),
        ],
    );
    RawPlateau::from_halves(
        Rate::new(83.0).unwrap(),
        Duration::from_secs(5),
        [&first, &second],
        None,
    )
    .unwrap()
}

fn record() -> PerfResult {
    PerfResult::from_raw(raw(vec![ladder_plateau()])).unwrap()
}

#[test]
fn a_recorded_plateau_rejudges_byte_for_byte() {
    let text = record().render().unwrap();
    let judgement = rejudge(&text).unwrap();
    assert!(judgement.reproduces(), "{:?}", judgement.mismatches);
    assert_eq!(judgement.recorded_version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn a_tampered_derived_field_is_named_by_its_pointer() {
    let mut value: Value = serde_json::from_str(&record().render().unwrap()).unwrap();
    value["derived"]["plateaus"][0]["throughput"]["achieved_per_s"] = Value::from(999.5);
    let mut text = serde_json::to_string_pretty(&value).unwrap();
    text.push('\n');
    let judgement = rejudge(&text).unwrap();
    assert!(!judgement.reproduces());
    let pointers: Vec<&str> = judgement
        .mismatches
        .iter()
        .map(|m| m.pointer.as_str())
        .collect();
    assert_eq!(pointers, ["/derived/plateaus/0/throughput/achieved_per_s"]);
    assert_eq!(judgement.mismatches[0].recorded.as_deref(), Some("999.5"));
}

#[test]
fn a_number_in_another_form_is_not_canonical() {
    let text = record()
        .render()
        .unwrap()
        .replace("\"seed\": 42", "\"seed\": 42.0");
    // A float seed is either refused by the typed read or not canonical.
    assert!(rejudge(&text).map_or(true, |judgement| !judgement.reproduces()));
    let spaced = record().render().unwrap().replace('\n', "\n ");
    let judgement = rejudge(&spaced).unwrap();
    assert!(judgement.mismatches.is_empty());
    assert!(!judgement.canonical);
}

#[test]
fn summed_percentiles_differ_from_the_mean_of_per_half_percentiles() {
    // Half one is uniformly fast; half two hides a slow tail of 100 in 1,000.
    let fast = window(5, 35, vec![("read", operation(&vec![10_000; 1_000], &[]))]);
    let mut tail = vec![10_000; 900];
    tail.extend(vec![1_000_000; 100]);
    let slow = window(35, 65, vec![("read", operation(&tail, &[]))]);
    let plateau = RawPlateau::from_halves(
        Rate::new(66.0).unwrap(),
        Duration::from_secs(5),
        [&fast, &slow],
        None,
    )
    .unwrap();
    let result = PerfResult::from_raw(raw(vec![plateau])).unwrap();
    let summed = result.derived.plateaus[0].latency.p99.value_ms().unwrap();

    let p99 = |w: &WindowRecord| {
        quantile(&w.operations[&op("read")].ok, QuantileName::P99)
            .value_ms()
            .unwrap()
    };
    let mean_of_halves = f64::midpoint(p99(&fast), p99(&slow));
    assert!((summed - 1_000.0).abs() < 1.0, "summed p99 {summed}");
    assert!(
        (mean_of_halves - 505.0).abs() < 1.0,
        "mean of halves {mean_of_halves}"
    );
    assert!((summed - mean_of_halves).abs() > 400.0);
    assert_eq!(
        result.derived.plateaus[0].halves.steadiness,
        Steadiness::Unsteady
    );
}

#[test]
fn resolvability_and_judged_at_follow_the_sample_count() {
    let result = record();
    let plateau = &result.derived.plateaus[0];
    assert_eq!(plateau.throughput.requests, 5_000);
    // Over 4,998 successes the overall p99 carries the ranks 4,934 to 4,962.
    let Quantile::Resolved {
        low_rank,
        high_rank,
        rank,
        ..
    } = plateau.latency.p99
    else {
        panic!("the overall p99 resolves over 4,998 successes");
    };
    assert_eq!((low_rank, rank, high_rank), (4_934, 4_949, 4_962));

    let read = &plateau.operations[&op("read")];
    assert_eq!(read.requests, 4_500);
    assert_eq!(read.judged_at, JudgedAt::P99);
    let write = &plateau.operations[&op("write")];
    assert_eq!(write.requests, 500);
    assert!((write.share - 0.1).abs() < 0.001, "{}", write.share);
    assert!(matches!(
        write.latency.p99,
        Quantile::NotResolvable { samples_above: 4 }
    ));
    assert_eq!(write.judged_at, JudgedAt::P95);
    assert_eq!(write.errors.get(&ErrorClass::ServerError).copied(), Some(1));
}

#[test]
fn every_record_states_its_scope_and_its_single_pass() {
    let result = record();
    assert_eq!(result.derived.scope, SCOPE);
    assert_eq!(
        result.derived.between_run_noise,
        BetweenRunNoise::Unmeasured
    );
    let value: Value = serde_json::from_str(&result.render().unwrap()).unwrap();
    assert_eq!(value["derived"]["passes"], Value::from(1));
    assert_eq!(value["derived"]["between_run_noise"], "unmeasured");
    assert_eq!(value["derived"]["scope"], SCOPE);
    assert_eq!(value["family"], "perf-result");
}

#[test]
fn failed_exchange_samples_are_bounded_per_operation() {
    let failures: Vec<(ErrorClass, u64)> =
        (0..12).map(|_| (ErrorClass::ServerError, 800)).collect();
    let first = window(5, 35, vec![("write", operation(&[1_000], &failures))]);
    let second = window(35, 65, vec![("write", operation(&[1_000], &failures))]);
    let plateau = RawPlateau::from_halves(
        Rate::new(1.0).unwrap(),
        Duration::from_secs(5),
        [&first, &second],
        None,
    )
    .unwrap();
    let kept = &plateau.failed_exchanges[&op("write")];
    assert_eq!(kept.len(), 16);
    assert_eq!(kept[0].status, Some(503));
    assert_eq!(kept[0].request_line, "POST /ehr");
    assert_eq!(kept[0].body, "busy");
    let result = PerfResult::from_raw(raw(vec![plateau])).unwrap();
    assert!(rejudge(&result.render().unwrap()).unwrap().reproduces());
}

#[test]
fn an_error_count_that_disagrees_with_its_histogram_is_refused() {
    let mut value: Value = serde_json::from_str(&record().render().unwrap()).unwrap();
    value["raw"]["plateaus"][0]["halves"][0]["operations"]["write"]["errors"]["server_error"] =
        Value::from(3);
    let error = rejudge(&value.to_string()).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("/raw/plateaus/0/halves/0/operations/write/errors"),
        "{error}"
    );
}

#[test]
fn the_record_validates_against_its_published_schema() {
    let value: Value = serde_json::from_str(&record().render().unwrap()).unwrap();
    let validator = jsonschema::validator_for(&veredictum::schema::perf_result_schema()).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&value)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

fn perf_judge(path: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .arg("perf-judge")
        .arg("--result")
        .arg(path)
        .output()
        .unwrap()
}

#[test]
fn perf_judge_exits_1_naming_a_tampered_field() {
    let dir = assert_fs::TempDir::new().unwrap();
    let path = dir.path().join("perf-result.json");
    let text = record().render().unwrap();
    std::fs::write(&path, &text).unwrap();
    let clean = perf_judge(&path);
    assert_eq!(clean.status.code(), Some(0), "{clean:?}");
    assert!(String::from_utf8_lossy(&clean.stdout).contains("reproduces byte for byte"));

    let mut value: Value = serde_json::from_str(&text).unwrap();
    value["derived"]["plateaus"][0]["operations"]["read"]["judged_at"] = Value::from("p50");
    let mut tampered = serde_json::to_string_pretty(&value).unwrap();
    tampered.push('\n');
    std::fs::write(&path, tampered).unwrap();
    let out = perf_judge(&path);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(
            "/derived/plateaus/0/operations/read/judged_at: recorded \"p50\", re-derived \"p99\""
        ),
        "{stdout}"
    );

    std::fs::write(&path, "{}").unwrap();
    assert_eq!(perf_judge(&path).status.code(), Some(2));
}
#[test]
fn a_rate_limited_exchange_refuses_its_own_record_only() {
    let clean = raw(vec![ladder_plateau()]);
    assert!(refuse_rate_limited(&clean).is_ok());
    let mut limited = raw(vec![ladder_plateau()]);
    limited.phases.push(PhaseRecord {
        phase: Phase::WarmUp,
        planned_ms: 20_000,
        wall_ms: 20_000,
        exchanges: 160,
        errors: BTreeMap::from([(ErrorClass::RateLimited, 1)]),
    });
    assert_eq!(limited.rate_limited(), 1);
    let refusal = refuse_rate_limited(&limited).unwrap_err().to_string();
    assert!(refusal.contains("429"), "{refusal}");
    assert!(
        refuse_rate_limited(&clean).is_ok(),
        "a second record in the same process is judged on its own exchanges"
    );
}

#[test]
fn a_refusal_counted_apart_is_neither_a_success_nor_an_error() {
    let mut refused = operation(&spread(100, 5_000, 4), &[]);
    refused.refused = 40;
    let first = window(5, 35, vec![("aql:optional", refused.clone())]);
    let second = window(35, 65, vec![("aql:optional", refused)]);
    let plateau = RawPlateau::from_halves(
        Rate::new(10.0).unwrap(),
        Duration::from_secs(5),
        [&first, &second],
        None,
    )
    .unwrap();
    let record = PerfResult::from_raw(raw(vec![plateau])).unwrap();
    let derived = &record.derived.plateaus[0];
    assert_eq!(derived.throughput.refused, 80);
    assert_eq!(derived.throughput.requests, 280);
    assert_eq!(derived.throughput.error_share, Some(0.0));
    assert_eq!(derived.operations[&op("aql:optional")].refused, 80);
    assert!(
        derived.qualification.qualified,
        "{:?}",
        derived.qualification
    );
    let judgement = rejudge(&record.render().unwrap()).unwrap();
    assert!(judgement.reproduces(), "{:?}", judgement.mismatches);
}

#[test]
fn the_seeding_is_reported_apart_and_labelled_closed_loop() {
    let derived = record().derived;
    assert_eq!(derived.seeding.model, LoadModel::ClosedLoop);
    assert_eq!(derived.seeding.requests_per_s, Some(550.0));
    let text = record().render().unwrap();
    assert!(text.contains("\"model\": \"closed_loop\""));
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

/// A plateau at `rate` whose 5,000 reads all take about `latency_us`.
fn plateau_at(rate: f64, latency_us: u64) -> RawPlateau {
    let half = |start, end| {
        window(
            start,
            end,
            vec![("read", operation(&spread(2_500, latency_us, 2), &[]))],
        )
    };
    let (first, second) = (half(5, 35), half(35, 65));
    RawPlateau::from_halves(
        Rate::new(rate).unwrap(),
        Duration::from_secs(5),
        [&first, &second],
        None,
    )
    .unwrap()
}

#[test]
fn a_failing_plateau_below_a_qualifying_one_puts_the_knee_below_the_gap() {
    let record = PerfResult::from_raw(raw(vec![
        plateau_at(50.0, 5_000),
        plateau_at(75.0, 2_000_000),
        plateau_at(90.0, 6_000),
        plateau_at(105.0, 3_000_000),
    ]))
    .unwrap();
    let Knee::Bracket {
        qualified_rate,
        failing_rate,
        ..
    } = record.derived.knee
    else {
        panic!("{:?}", record.derived.knee);
    };
    assert_eq!((qualified_rate, failing_rate), (50.0, 75.0));
    let peak = record.derived.peak.as_ref().unwrap();
    assert_eq!(peak.source, PeakSource::Plateau);
    assert!(matches!(
        record.derived.usl,
        UslReading::Fitted { points: 4, .. }
    ));
    let judgement = rejudge(&record.render().unwrap()).unwrap();
    assert!(judgement.reproduces(), "{:?}", judgement.mismatches);
}

#[test]
fn a_qualifying_top_plateau_reads_the_knee_as_a_lower_bound() {
    let record = PerfResult::from_raw(raw(vec![
        plateau_at(110.0, 5_000),
        plateau_at(50.0, 5_000),
        plateau_at(80.0, 5_000),
    ]))
    .unwrap();
    assert!(
        matches!(record.derived.knee, Knee::LowerBound { qualified_rate, .. } if close(qualified_rate, 110.0)),
        "{:?}",
        record.derived.knee
    );
    let PopulationReading::Read {
        lower_bound,
        population,
        corpus_scale,
        derivation,
        arrivals_per_100k,
        ..
    } = &record.derived.population
    else {
        panic!("{:?}", record.derived.population);
    };
    assert!(*lower_bound);
    assert!(close(arrivals_per_100k[0], 13.0) && close(arrivals_per_100k[1], 75.0));
    assert!(
        close(population[0], 110.0 / 75.0 * 100_000.0),
        "{population:?}"
    );
    assert!(
        close(population[1], 110.0 / 13.0 * 100_000.0),
        "{population:?}"
    );
    assert_eq!(*corpus_scale, CorpusScale::S);
    assert!(derivation.contains("13 to 75 peak API arrivals"));
    assert!(matches!(
        record.derived.usl,
        UslReading::NotFitted { points: 3, .. }
    ));
}

#[test]
fn a_failing_lowest_plateau_has_no_population_reading() {
    let record = PerfResult::from_raw(raw(vec![plateau_at(50.0, 2_000_000)])).unwrap();
    assert!(
        matches!(record.derived.knee, Knee::BelowLadder { failing_rate } if close(failing_rate, 50.0))
    );
    assert!(matches!(
        record.derived.population,
        PopulationReading::NotRead { .. }
    ));
}

#[test]
fn a_tampered_knee_or_fit_is_named_by_perf_judge() {
    let record = PerfResult::from_raw(raw(vec![
        plateau_at(50.0, 5_000),
        plateau_at(75.0, 2_000_000),
        plateau_at(90.0, 6_000),
        plateau_at(105.0, 3_000_000),
    ]))
    .unwrap();
    let mut value: Value = serde_json::from_str(&record.render().unwrap()).unwrap();
    value["derived"]["knee"]["qualified_rate"] = Value::from(90.0);
    value["derived"]["usl"]["alpha"] = Value::from(0.5);
    let mut text = serde_json::to_string_pretty(&value).unwrap();
    text.push('\n');
    let judgement = rejudge(&text).unwrap();
    let pointers: Vec<&str> = judgement
        .mismatches
        .iter()
        .map(|m| m.pointer.as_str())
        .collect();
    assert_eq!(
        pointers,
        vec!["/derived/knee/qualified_rate", "/derived/usl/alpha"]
    );
}
/// Scout windows of 2 s whose achieved throughput and mean in flight follow
/// the USL with `alpha`, `beta` and `gamma`: `X(N)` exchanges per second,
/// each taking `N / X(N)` seconds, so Little's law reads back `N`.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "rounded synthetic counts and latencies, all small and positive"
)]
fn usl_windows(alpha: f64, beta: f64, gamma: f64) -> RawScout {
    let windows = [1.0, 2.0, 4.0, 8.0, 12.0, 16.0, 24.0, 32.0, 48.0, 64.0, 96.0]
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let x = gamma * n / (1.0 + alpha * (n - 1.0) + beta * n * (n - 1.0));
            let count = (x * 2.0).round() as u64;
            let latency_us = (n / x * 1_000_000.0).round() as u64;
            let start = 2 * i as u64;
            let record = window(
                start,
                start + 2,
                vec![("read", operation(&vec![latency_us; count as usize], &[]))],
            );
            RawScoutWindow {
                planned_rate: Rate::new(x).unwrap(),
                window: RawWindow::from_record(&record).unwrap(),
            }
        })
        .collect();
    RawScout {
        start_rate: Rate::new(8.0).unwrap(),
        end_rate: Rate::new(4_096.0).unwrap(),
        halted_at_ms: None,
        window_ms: 2_000,
        windows,
    }
}

#[test]
fn the_usl_fit_recovers_known_alpha_and_beta_from_synthetic_windows() {
    let (alpha, beta, gamma) = (0.03, 0.000_2, 150.0);
    let mut synthetic = raw(Vec::new());
    synthetic.scout = Some(usl_windows(alpha, beta, gamma));
    let record = PerfResult::from_raw(synthetic).unwrap();
    let UslReading::Fitted {
        alpha: a, beta: b, ..
    } = record.derived.usl
    else {
        panic!("{:?}", record.derived.usl);
    };
    assert!((a - alpha).abs() <= 0.05 * alpha, "alpha {a}");
    assert!((b - beta).abs() <= 0.05 * beta, "beta {b}");
    let judgement = rejudge(&record.render().unwrap()).unwrap();
    assert!(judgement.reproduces(), "{:?}", judgement.mismatches);
}
