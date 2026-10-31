// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Generates `examples/perf-result.example.json`: the committed EXAMPLE
//! `perf-result` record.
//!
//! It is an explicit example, never a measurement: the target is
//! `example-cdr` at `cdr.example`, the workload is a `custom` document, and
//! every latency is synthetic. What makes it worth committing is that the
//! raw part is built from real `HdrHistogram` V2 encodings through the
//! crate's own record types, and the derived part is COMPUTED from it by
//! [`veredictum::perf_result::PerfResult::from_raw`], so `perf-judge`
//! reproduces it byte for byte. The scout breaches near 200 operations per
//! second, the ladder qualifies three plateaus and fails the fourth, one
//! plateau is unsteady, a 1 % operation is judged below p99, and the
//! failing plateau's database series is truncated. `show`, `export` and
//! `perf-assets` render it in their golden tests.
//!
//! Regenerate with `cargo run --example make_example_perf_result` from the
//! repository root; the output is deterministic, and
//! `scripts/checks/example-results-drift.sh` fails when the committed copy is
//! not what this run writes. One optional argument names another output path.

#![expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::integer_division,
    reason = "an example generator: its synthetic counts and latencies stay far below 2^52, \
              so every conversion and division among them is exact or deliberately rounded"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::Duration;

use sha2::{Digest, Sha256};
use veredictum::identity::{IdentityRead, OptionsDocument};
use veredictum::ixit::Environment;
use veredictum::perf_emulate::fingerprint::EnvironmentFingerprint;
use veredictum::perf_gen::arrivals::Rate;
use veredictum::perf_gen::drive::{OperationRecord, WindowRecord, WindowSpec, latency_histogram};
use veredictum::perf_gen::{ErrorClass, FailedExchange, OperationId};
use veredictum::perf_result::derive::{scout_estimate, scout_windows};
use veredictum::perf_result::server::{
    ContainerRole, ContainerSample, ContainerSeries, ContainersReading, DatabaseReading,
    ServerReadings, Setting, SettingsReading, SizeSnapshot, StatementCost, StatementsReading,
    Truncation,
};
use veredictum::perf_result::{
    CorpusOrigin, CorpusRecord, EmbeddedWorkload, Generator, Instrument, LoadModel, PerfResult,
    Phase, PhaseRecord, Raw, RawMix, RawPlateau, RawQualification, RawScout, RawScoutWindow,
    RawWindow, SeedingRecord, Target, TargetOptions,
};
use veredictum::workload::CorpusScale;
use veredictum::workload::plan::{LADDER, hold_s};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// The example's workload document, embedded verbatim.
const WORKLOAD: &str = "id: example-mix\nversion: 1\nstatus: provisional\n";

/// The operations, their shares in millionths, and their base latency in
/// microseconds.
const MIX: [(&str, u64, u64); 4] = [
    ("ehr_read", 640_000, 6_000),
    ("composition_create", 250_000, 18_000),
    ("aql:patient_summary", 100_000, 35_000),
    ("aql:cohort_count", 10_000, 240_000),
];

/// The scout's ramp: 8 to 4,096 operations per second over 60 s, in 2 s
/// windows, halted after the window that breaches.
const SCOUT_FROM: f64 = 8.0;
const SCOUT_TO: f64 = 4_096.0;
const SCOUT_S: f64 = 60.0;
const SCOUT_WINDOWS: u64 = 17;

/// The rate past which the example server stops holding its budgets.
const CAPACITY: f64 = 200.0;

/// `sha256:` and the hex digest of `bytes`.
fn digest(bytes: &[u8]) -> String {
    let hex = Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        });
    format!("sha256:{hex}")
}

fn op(name: &str) -> Fallible<OperationId> {
    Ok(OperationId::new(name)?)
}

/// The `i`th of `n` latencies of an operation whose base latency is
/// `base_us`, under `pressure`: a body around the base and a tail that
/// grows with the pressure.
fn latency_us(base_us: u64, pressure: f64, i: u64, n: u64) -> u64 {
    let u = (i as f64 + 0.5) / n.max(1) as f64;
    let body = 0.55 + 0.9 * u;
    let tail = 1.0 + pressure * u.powi(12);
    (base_us as f64 * body * tail)
        .round()
        .clamp(1.0, 30_000_000.0) as u64
}

/// One operation's record over `n` exchanges, `failed` of which failed.
fn operation(base_us: u64, pressure: f64, n: u64, failed: u64) -> Fallible<OperationRecord> {
    let mut ok = latency_histogram()?;
    let mut bad = latency_histogram()?;
    let mut errors = BTreeMap::new();
    let mut samples = Vec::new();
    for i in 0..n.saturating_sub(failed) {
        ok.record(latency_us(base_us, pressure, i, n))?;
    }
    for i in 0..failed {
        bad.record(latency_us(base_us, pressure, n - 1 - i, n))?;
        *errors.entry(ErrorClass::ServerError).or_insert(0) += 1;
        if samples.len() < 2 {
            samples.push(FailedExchange::new(
                Some(reqwest::StatusCode::SERVICE_UNAVAILABLE),
                "POST /ehr/{ehr_id}/composition",
                "{\"message\":\"the connection pool is exhausted\"}",
            ));
        }
    }
    Ok(OperationRecord {
        ok,
        failed: bad,
        errors,
        samples,
        refused: 0,
    })
}

/// One window of `seconds` at `rate`, under `pressure`, with `error_ppm`
/// of the composition writes failing.
fn window(
    start_ms: u64,
    seconds: f64,
    rate: f64,
    pressure: f64,
    error_ppm: u64,
) -> Fallible<WindowRecord> {
    let total = (rate * seconds).round() as u64;
    let mut operations = BTreeMap::new();
    let mut lag = latency_histogram()?;
    for (name, share, base) in MIX {
        let n = total * share / 1_000_000;
        let failed = if name == "composition_create" {
            n * error_ppm / 1_000_000
        } else {
            0
        };
        operations.insert(op(name)?, operation(base, pressure, n, failed)?);
        for i in 0..n {
            lag.record(200 + (i % 7) * 40)?;
        }
    }
    let in_flight_mean = (rate * 0.012 * (1.0 + pressure)).round() as u32;
    Ok(WindowRecord {
        spec: WindowSpec {
            start: Duration::from_millis(start_ms),
            end: Duration::from_millis(start_ms + (seconds * 1_000.0).round() as u64),
        },
        operations,
        lag,
        in_flight: (0..8).map(|i| in_flight_mean + i % 3).collect(),
        budget_wait: Duration::ZERO,
        budget_exhausted: false,
    })
}

/// The pressure the example server is under at `rate`: none well below
/// its capacity, a growing tail near it, and a breach past it.
fn pressure(rate: f64) -> f64 {
    let load = rate / CAPACITY;
    if load < 0.8 {
        2.0 * load
    } else if load < 1.0 {
        4.0 + 30.0 * (load - 0.8)
    } else {
        300.0 + 600.0 * (load - 1.0)
    }
}

fn scout() -> Fallible<RawScout> {
    let mut windows = Vec::new();
    for w in 0..SCOUT_WINDOWS {
        let mid = (w as f64 * 2.0 + 1.0) / SCOUT_S;
        let rate = SCOUT_FROM * (SCOUT_TO / SCOUT_FROM).powf(mid);
        windows.push(RawScoutWindow {
            planned_rate: Rate::new(rate)?,
            window: RawWindow::from_record(&window(w * 2_000, 2.0, rate, pressure(rate), 0)?)?,
        });
    }
    Ok(RawScout {
        start_rate: Rate::new(SCOUT_FROM)?,
        end_rate: Rate::new(SCOUT_TO)?,
        halted_at_ms: Some(SCOUT_WINDOWS * 2_000),
        window_ms: 2_000,
        windows,
    })
}

/// The container series of a hold at `rate`; the database series of the
/// failing plateau is truncated.
fn containers(rate: f64, hold_ms: u64, truncate_db: bool) -> ContainersReading {
    let series = |role: ContainerRole, name: &str, cpu_per_op_us: f64, rss: u64| {
        let mut samples = Vec::new();
        let mut truncated = None;
        let mut offset = 0;
        let mut cpu = 9_000_000_000_u64;
        while offset <= hold_ms {
            if truncate_db && role == ContainerRole::Db && offset >= hold_ms / 2 {
                truncated = Some(Truncation {
                    at_ms: offset,
                    reason: "the Docker Engine API answered 500 for \
                             /containers/cdr-db/stats?stream=false&one-shot=true"
                        .to_owned(),
                });
                break;
            }
            let step = offset / 2_000;
            samples.push(ContainerSample {
                offset_ms: offset,
                cpu_ns: cpu,
                rss_bytes: rss + step * 1_048_576,
                blk_read_bytes: 40_000_000 + step * 120_000,
                blk_write_bytes: 90_000_000 + step * (rate as u64) * 9_000,
                net_rx_bytes: 500_000_000 + step * (rate as u64) * 2_400,
                net_tx_bytes: 700_000_000 + step * (rate as u64) * 6_100,
            });
            let busy = rate * cpu_per_op_us * 2.0 * (1.0 + 0.05 * (step % 3) as f64);
            cpu += (busy * 1_000.0) as u64;
            offset += 2_000;
        }
        ContainerSeries {
            role,
            name: name.to_owned(),
            interval_ms: 2_000,
            samples,
            truncated,
        }
    };
    ContainersReading::Present {
        series: vec![
            series(ContainerRole::Sut, "cdr-app", 2_900.0, 610_000_000),
            series(ContainerRole::Db, "cdr-db", 1_700.0, 1_350_000_000),
        ],
    }
}

fn readings(index: u64, rate: f64, hold_ms: u64, truncate_db: bool) -> ServerReadings {
    let start = 2_400_000_000 + index * 31_000_000;
    let growth = (rate * hold_ms as f64 / 1_000.0 * 0.25 * 6_200.0) as u64;
    let schemas = |total: u64| {
        BTreeMap::from([
            ("ehr".to_owned(), total * 7 / 10),
            ("public".to_owned(), 81_920),
            ("terminology".to_owned(), 24_000_000),
        ])
    };
    let calls = |share: f64| (rate * hold_ms as f64 / 1_000.0 * share).round() as u64;
    let p = pressure(rate);
    ServerReadings {
        containers: containers(rate, hold_ms, truncate_db),
        database: DatabaseReading::Present {
            database: "cdr".to_owned(),
            start: SizeSnapshot {
                database_bytes: start,
                schemas: schemas(start),
            },
            end: SizeSnapshot {
                database_bytes: start + growth,
                schemas: schemas(start + growth),
            },
        },
        statements: StatementsReading::Present {
            statements: vec![
                StatementCost {
                    query: "SELECT count(*) FROM ehr.comp_data c WHERE c.archetype_id = $1"
                        .to_owned(),
                    calls: calls(0.01),
                    total_exec_ms: calls(0.01) as f64 * 190.0 * (1.0 + p / 4.0),
                    total_plan_ms: calls(0.01) as f64 * 0.4,
                    rows: calls(0.01),
                    shared_blks_hit: calls(0.01) * 41_000,
                    shared_blks_read: calls(0.01) * 900,
                },
                StatementCost {
                    query: "INSERT INTO ehr.comp_data (id, ehr_id, data) VALUES ($1, $2, $3)"
                        .to_owned(),
                    calls: calls(0.25),
                    total_exec_ms: calls(0.25) as f64 * 3.1 * (1.0 + p / 10.0),
                    total_plan_ms: calls(0.25) as f64 * 0.05,
                    rows: calls(0.25),
                    shared_blks_hit: calls(0.25) * 60,
                    shared_blks_read: calls(0.25) * 2,
                },
                StatementCost {
                    query: "SELECT e.id, e.created FROM ehr.ehr e WHERE e.id = $1".to_owned(),
                    calls: calls(0.64),
                    total_exec_ms: calls(0.64) as f64 * 0.21,
                    total_plan_ms: calls(0.64) as f64 * 0.03,
                    rows: calls(0.64),
                    shared_blks_hit: calls(0.64) * 4,
                    shared_blks_read: 0,
                },
            ],
        },
    }
}

fn settings() -> SettingsReading {
    let setting = |value: &str, unit: Option<&str>, source: &str| Setting {
        setting: value.to_owned(),
        unit: unit.map(str::to_owned),
        source: source.to_owned(),
    };
    SettingsReading::Present {
        server_version: "18.6".to_owned(),
        settings: BTreeMap::from([
            (
                "effective_cache_size".to_owned(),
                setting("524288", Some("8kB"), "configuration file"),
            ),
            ("jit".to_owned(), setting("off", None, "command line")),
            (
                "max_connections".to_owned(),
                setting("200", None, "configuration file"),
            ),
            (
                "shared_buffers".to_owned(),
                setting("262144", Some("8kB"), "configuration file"),
            ),
            (
                "shared_preload_libraries".to_owned(),
                setting("pg_stat_statements", None, "command line"),
            ),
            (
                "work_mem".to_owned(),
                setting("16384", Some("kB"), "default"),
            ),
        ]),
    }
}

fn plateaus(estimate: f64) -> Fallible<Vec<RawPlateau>> {
    let mut out = Vec::new();
    for (index, multiple) in LADDER.iter().enumerate() {
        let rate = multiple * estimate;
        let hold = f64::from(hold_s(5_000, rate));
        let half = hold / 2.0;
        let p = pressure(rate);
        let failing = rate > CAPACITY;
        let error_ppm = if failing { 9_000 } else { 400 };
        let first = window(5_000, half, rate, p, error_ppm)?;
        // The 0.9 plateau drifts: its second half runs under twice the
        // pressure, which the half-to-half ratio marks unsteady.
        let drift = if (multiple - 0.9).abs() < 1e-9 {
            2.4
        } else {
            1.0
        };
        let second = window(
            5_000 + (half * 1_000.0) as u64,
            half,
            rate,
            p * drift,
            error_ppm,
        )?;
        let second = scale_second_half(second, drift)?;
        let mut plateau = RawPlateau::from_halves(
            Rate::new(rate)?,
            Duration::from_secs(5),
            [&first, &second],
            None,
        )?;
        plateau.multiple = Some(*multiple);
        plateau.server = readings(index as u64, rate, (hold * 1_000.0) as u64, failing);
        out.push(plateau);
    }
    Ok(out)
}

/// Lifts the body of every latency in `window` by `factor`, so a drifting
/// half moves its median as well as its tail.
fn scale_second_half(window: WindowRecord, factor: f64) -> Fallible<WindowRecord> {
    if (factor - 1.0).abs() < 1e-9 {
        return Ok(window);
    }
    let mut operations = BTreeMap::new();
    for (id, record) in window.operations {
        let mut ok = latency_histogram()?;
        for value in record.ok.iter_recorded() {
            let lifted = (value.value_iterated_to() as f64 * factor).round() as u64;
            ok.record_n(lifted, value.count_at_value())?;
        }
        operations.insert(id, OperationRecord { ok, ..record });
    }
    Ok(WindowRecord {
        operations,
        ..window
    })
}

fn phases(plateaus: &[RawPlateau]) -> Vec<PhaseRecord> {
    let ladder_ms: u64 = plateaus.iter().map(|p| p.ramp_in_ms + p.hold_ms).sum();
    let phase = |phase: Phase, planned_ms: u64, wall_ms: u64, exchanges: u64| PhaseRecord {
        phase,
        planned_ms,
        wall_ms,
        exchanges,
        errors: BTreeMap::new(),
    };
    vec![
        phase(Phase::Settle, 15_000, 15_002, 0),
        phase(Phase::WarmUp, 20_000, 20_011, 160),
        phase(Phase::Scout, 60_000, SCOUT_WINDOWS * 2_000, 3_870),
        phase(Phase::Drain, 10_000, 1_240, 0),
        phase(Phase::Ladder, ladder_ms + 10_000, ladder_ms + 1_903, 25_310),
        phase(Phase::Closing, 10_000, 412, 1),
    ]
}

fn raw() -> Fallible<Raw> {
    let qualification = RawQualification {
        default_budget_ms: 1_000,
        budgets_ms: BTreeMap::from([(op("aql:cohort_count")?, 5_000)]),
        max_error_ppm: 1_000,
        min_achieved_ppm: 950_000,
    };
    let scout = scout()?;
    let windows = scout_windows(&scout, &qualification, 5_000)?;
    let estimate = scout_estimate(&scout, &windows);
    let plateaus = plateaus(estimate.rate)?;
    Ok(Raw {
        instrument: Instrument::this_build(),
        target: Target {
            base_url: "https://cdr.example/openehr/v1".to_owned(),
            options: TargetOptions::Answered {
                answer: IdentityRead::Offered {
                    status: 200,
                    options: OptionsDocument {
                        solution: Some("example-cdr".to_owned()),
                        solution_version: Some("2.4.0".to_owned()),
                        vendor: Some("Example".to_owned()),
                        restapi_specs_version: Some("1.0.3".to_owned()),
                        ..OptionsDocument::default()
                    },
                },
            },
        },
        environment: Environment {
            exclusive_server: true,
            hardware_class: "example 8-vCPU VM".to_owned(),
            cores: 8,
            memory_gb: 8,
            storage_class: "NVMe".to_owned(),
            topology: "single node".to_owned(),
        },
        generator: Generator {
            host: EnvironmentFingerprint {
                arch: "x86_64".to_owned(),
                os: "linux".to_owned(),
                available_parallelism: Some(8),
                cpu_model: Some("Example CPU @ 3.0GHz".to_owned()),
                total_memory_bytes: Some(17_179_869_184),
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
            id: "example-mix".to_owned(),
            label: "custom".to_owned(),
            version: "1".to_owned(),
            sha256: digest(WORKLOAD.as_bytes()),
            document: WORKLOAD.to_owned(),
        },
        corpus: CorpusRecord {
            scale: CorpusScale::S,
            recipe_sha256: digest(b"example recipe"),
            digest: digest(b"example corpus"),
            origin: CorpusOrigin::Seeded,
            ehrs: 100_000,
            compositions: 1_000_000,
        },
        seeding: SeedingRecord {
            model: LoadModel::ClosedLoop,
            workers: 16,
            requests: 1_100_412,
            wall_ms: 1_934_000,
        },
        walk: None,
        mix: RawMix {
            operations_per_interaction: 1.0,
            dropped: Vec::new(),
        },
        qualification,
        seed: 20_261_030,
        server_settings: settings(),
        phases: phases(&plateaus),
        scout: Some(scout),
        plateaus,
    })
}

fn main() -> Fallible<()> {
    let out = std::env::args().nth(1).unwrap_or_else(|| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/perf-result.example.json"
        )
        .to_owned()
    });
    let record = PerfResult::from_raw(raw()?)?;
    std::fs::write(&out, record.render()?)?;
    Ok(())
}
