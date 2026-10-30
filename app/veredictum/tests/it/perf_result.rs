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
use veredictum::perf_result::derive::{BetweenRunNoise, JudgedAt, Steadiness};
use veredictum::perf_result::judge::rejudge;
use veredictum::perf_result::quantile::{Quantile, QuantileName, quantile};
use veredictum::perf_result::{
    CorpusRecord, CorpusScale, EmbeddedWorkload, Generator, Instrument, PerfResult, Raw,
    RawPlateau, SCOPE, Target, TargetOptions,
};

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

fn raw(plateaus: Vec<RawPlateau>) -> Raw {
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
        posture: None,
        workload: EmbeddedWorkload {
            id: "reference".to_owned(),
            version: "1".to_owned(),
            sha256: format!("sha256:{}", "0".repeat(64)),
            document: "id: reference\n".to_owned(),
        },
        corpus: CorpusRecord {
            scale: CorpusScale::S,
            recipe_sha256: format!("sha256:{}", "1".repeat(64)),
            digest: format!("sha256:{}", "2".repeat(64)),
        },
        seed: 42,
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
