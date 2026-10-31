// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The server-side readings of a plateau against fakes: a wiremock Docker
//! Engine API and an in-memory database. With containers and a database
//! declared, a hold carries the container series, the sizes and the
//! statement table; without them every reading is absent with its reason;
//! and a reading that fails truncates its series or leaves its reading
//! absent, never failing anything.

#![expect(
    clippy::unwrap_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken fixture must abort the test loudly, Book ch11"
)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use veredictum::ixit::Containers;
use veredictum::perf_emulate::server::{
    ContainerTargets, DockerEngine, NO_CONTAINERS, NO_DATABASE, ReadingError, ServerSampler,
    SqlReader,
};
use veredictum::perf_result::server::{
    ContainerRole, ContainersReading, DatabaseReading, ServerReadings, SettingsReading,
    StatementsReading, summarize,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A database that answers each kind of query from its own queue, and
/// fails once a queue runs dry.
#[derive(Default)]
struct FakeDatabase {
    sizes: Mutex<VecDeque<String>>,
    statements: Mutex<VecDeque<String>>,
    settings: Mutex<VecDeque<String>>,
}

impl FakeDatabase {
    fn answering(sizes: &[&str], statements: &[&str], settings: &[&str]) -> Self {
        let queue = |answers: &[&str]| {
            Mutex::new(
                answers
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect::<VecDeque<_>>(),
            )
        };
        Self {
            sizes: queue(sizes),
            statements: queue(statements),
            settings: queue(settings),
        }
    }
}

impl SqlReader for FakeDatabase {
    fn query(
        &self,
        sql: &'static str,
    ) -> impl Future<Output = Result<String, ReadingError>> + Send {
        let queue = if sql.contains("pg_stat_statements") {
            &self.statements
        } else if sql.contains("pg_settings") {
            &self.settings
        } else {
            &self.sizes
        };
        let answer = queue.lock().unwrap().pop_front().ok_or_else(|| {
            ReadingError::Sql("FATAL: the database system is shutting down".to_owned())
        });
        std::future::ready(answer)
    }
}

const SIZES_START: &str =
    r#"{"database":"cdr","database_bytes":1000000,"schemas":{"ehr":600000,"public":8192}}"#;
const SIZES_END: &str =
    r#"{"database":"cdr","database_bytes":1250000,"schemas":{"ehr":850000,"public":8192}}"#;
const STATEMENTS_START: &str = r#"[{"id":"1","calls":10,"exec_ms":100.0,"plan_ms":1.0,"rows":10,"hit":40,"read":2,"query":"SELECT * FROM ehr.ehr WHERE id = $1"}]"#;
const STATEMENTS_END: &str = r#"[{"id":"1","calls":110,"exec_ms":600.0,"plan_ms":11.0,"rows":110,"hit":440,"read":2,"query":"SELECT * FROM ehr.ehr WHERE id = $1"},{"id":"2","calls":3,"exec_ms":2400.5,"plan_ms":0.0,"rows":3000,"hit":9000,"read":800,"query":"SELECT count(*) FROM ehr.composition"}]"#;
const SETTINGS: &str = r#"{"server_version":"18.6","settings":{"shared_buffers":{"setting":"16384","unit":"8kB","source":"configuration file"},"jit":{"setting":"off","unit":null,"source":"command line"}}}"#;

/// A stats answer carrying `cpu_ns` of CPU time.
fn stats(cpu_ns: u64) -> serde_json::Value {
    serde_json::json!({
        "cpu_stats": { "cpu_usage": { "total_usage": cpu_ns } },
        "memory_stats": { "usage": 300_000_000_u64, "stats": { "inactive_file": 50_000_000_u64 } },
        "networks": { "eth0": { "rx_bytes": cpu_ns >> 10, "tx_bytes": 7 } }
    })
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "the sampler takes its containers as a target or the reason there is none"
)]
fn declared(engine: &MockServer) -> ContainerTargets {
    Ok((
        Arc::new(DockerEngine::http(&engine.uri()).unwrap()),
        vec![
            (ContainerRole::Sut, "cdr-app".to_owned()),
            (ContainerRole::Db, "cdr-db".to_owned()),
        ],
    ))
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs one hold of `hold` against `sampler`.
fn hold<S: SqlReader>(
    sampler: &ServerSampler<S>,
    hold: Duration,
) -> (SettingsReading, ServerReadings) {
    runtime().block_on(async {
        let settings = sampler.settings().await;
        sampler.begin().await;
        tokio::time::sleep(hold).await;
        (settings, sampler.end().await)
    })
}

async fn engine_answering(container: &str, answer: ResponseTemplate) -> MockServer {
    let engine = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/containers/{container}/stats")))
        .respond_with(answer)
        .mount(&engine)
        .await;
    engine
}

#[test]
fn a_declared_deployment_gives_every_reading() {
    let rt = runtime();
    let engine = rt.block_on(async {
        let engine = engine_answering(
            "cdr-app",
            ResponseTemplate::new(200).set_body_json(stats(4_000_000_000)),
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/containers/cdr-db/stats"))
            .respond_with(ResponseTemplate::new(200).set_body_json(stats(1_000_000_000)))
            .mount(&engine)
            .await;
        engine
    });
    let database = FakeDatabase::answering(
        &[SIZES_START, SIZES_END],
        &[STATEMENTS_START, STATEMENTS_END],
        &[SETTINGS],
    );
    let sampler = ServerSampler::new(declared(&engine), Ok(Arc::new(database)))
        .with_interval(Duration::from_millis(40));
    let (settings, readings) = hold(&sampler, Duration::from_millis(150));

    let SettingsReading::Present {
        server_version,
        settings,
    } = settings
    else {
        panic!("{settings:?}");
    };
    assert_eq!(server_version, "18.6");
    assert_eq!(settings.get("jit").map(|s| s.setting.as_str()), Some("off"));
    assert_eq!(settings.get("jit").and_then(|s| s.unit.clone()), None);

    let ContainersReading::Present { series } = &readings.containers else {
        panic!("{:?}", readings.containers);
    };
    let names: Vec<(&str, ContainerRole)> =
        series.iter().map(|s| (s.name.as_str(), s.role)).collect();
    assert_eq!(
        names,
        [
            ("cdr-app", ContainerRole::Sut),
            ("cdr-db", ContainerRole::Db)
        ]
    );
    for one in series {
        assert!(one.truncated.is_none(), "{:?}", one.truncated);
        assert!(
            one.samples.len() >= 3,
            "{} samples of {}",
            one.samples.len(),
            one.name
        );
        assert_eq!(one.interval_ms, 40);
        assert!(one.samples.iter().all(|s| s.rss_bytes == 250_000_000));
        assert!(
            one.samples
                .windows(2)
                .all(|w| w[0].offset_ms <= w[1].offset_ms)
        );
    }

    let DatabaseReading::Present {
        database,
        start,
        end,
    } = &readings.database
    else {
        panic!("{:?}", readings.database);
    };
    assert_eq!(database, "cdr");
    assert_eq!(start.schemas.get("ehr"), Some(&600_000));
    assert_eq!(end.database_bytes, 1_250_000);
    assert_eq!(summarize(&readings).database_growth_bytes, Some(250_000));

    let StatementsReading::Present { statements } = &readings.statements else {
        panic!("{:?}", readings.statements);
    };
    let costs: Vec<(&str, u64)> = statements
        .iter()
        .map(|s| (s.query.as_str(), s.calls))
        .collect();
    assert_eq!(
        costs,
        [
            ("SELECT count(*) FROM ehr.composition", 3),
            ("SELECT * FROM ehr.ehr WHERE id = $1", 100)
        ],
        "the hold's own cost, costliest first"
    );
    assert!(readings.absences().is_empty());
}

#[test]
fn an_undeclared_deployment_records_every_reading_absent_with_its_reason() {
    let sampler = ServerSampler::from_ixit(None);
    let (settings, readings) = hold(&sampler, Duration::ZERO);
    assert_eq!(
        settings,
        SettingsReading::Absent {
            reason: NO_CONTAINERS.to_owned()
        }
    );
    assert_eq!(readings, ServerReadings::absent(NO_CONTAINERS));
    assert!(summarize(&readings).is_empty());

    let no_database = Containers {
        sut: "cdr-app".to_owned(),
        db: "cdr-db".to_owned(),
        database: None,
    };
    let sampler = ServerSampler::from_ixit(Some(&no_database));
    let (settings, readings) = runtime().block_on(async {
        let settings = sampler.settings().await;
        (settings, sampler.end().await)
    });
    assert_eq!(
        settings,
        SettingsReading::Absent {
            reason: NO_DATABASE.to_owned()
        }
    );
    assert!(
        matches!(&readings.containers, ContainersReading::Absent { reason } if reason.contains("start was not read")),
        "{:?}",
        readings.containers
    );
}

#[test]
fn a_failing_container_is_truncated_with_its_reason_and_the_other_reads_on() {
    let rt = runtime();
    let engine = rt.block_on(async {
        let engine = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/containers/cdr-app/stats"))
            .respond_with(ResponseTemplate::new(200).set_body_json(stats(1_000)))
            .up_to_n_times(2)
            .with_priority(1)
            .mount(&engine)
            .await;
        Mock::given(method("GET"))
            .and(path("/containers/cdr-app/stats"))
            .respond_with(ResponseTemplate::new(500))
            .with_priority(2)
            .mount(&engine)
            .await;
        Mock::given(method("GET"))
            .and(path("/containers/cdr-db/stats"))
            .respond_with(ResponseTemplate::new(200).set_body_json(stats(2_000)))
            .mount(&engine)
            .await;
        engine
    });
    let database =
        FakeDatabase::answering(&[SIZES_START], &[STATEMENTS_START, STATEMENTS_END], &[]);
    let sampler = ServerSampler::new(declared(&engine), Ok(Arc::new(database)))
        .with_interval(Duration::from_millis(30));
    let (settings, readings) = hold(&sampler, Duration::from_millis(200));

    assert!(
        matches!(settings, SettingsReading::Absent { reason } if reason.contains("shutting down"))
    );
    let ContainersReading::Present { series } = &readings.containers else {
        panic!("{:?}", readings.containers);
    };
    let app = &series[0];
    assert_eq!(app.samples.len(), 2, "the series ends at the first failure");
    let cut = app
        .truncated
        .as_ref()
        .expect("the failing container is truncated");
    assert!(cut.reason.contains("500"), "{}", cut.reason);
    assert!(cut.at_ms >= app.samples[1].offset_ms);
    let db = &series[1];
    assert!(db.truncated.is_none());
    assert!(db.samples.len() > app.samples.len());

    assert!(
        matches!(&readings.database, DatabaseReading::Absent { reason } if reason.starts_with("the sizes at the hold's end")),
        "{:?}",
        readings.database
    );
    assert!(matches!(
        readings.statements,
        StatementsReading::Present { .. }
    ));
    let summary = summarize(&readings);
    assert!(summary.containers[0].truncated);
    assert_eq!(summary.database_growth_bytes, None);
}

#[test]
fn an_unreachable_engine_truncates_every_series_at_its_first_reading() {
    let engine = Arc::new(DockerEngine::http(&crate::fake_sut::closed_port_url()).unwrap());
    let sampler: ServerSampler<FakeDatabase> = ServerSampler::new(
        Ok((engine, vec![(ContainerRole::Sut, "cdr-app".to_owned())])),
        Err(NO_DATABASE.to_owned()),
    )
    .with_interval(Duration::from_millis(20));
    let (_, readings) = hold(&sampler, Duration::from_millis(60));
    let ContainersReading::Present { series } = &readings.containers else {
        panic!("{:?}", readings.containers);
    };
    assert!(series[0].samples.is_empty());
    let cut = series[0].truncated.as_ref().expect("truncated");
    assert!(
        cut.reason.starts_with("the Docker Engine API"),
        "{}",
        cut.reason
    );
    let absences = readings.absences();
    assert_eq!(absences.len(), 3, "{absences:?}");
}
