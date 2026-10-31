// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The server-side sampler: the declared containers through the Docker
//! Engine API, and the database through `psql` inside its container.
//!
//! At a hold's start the sampler reads the database sizes and a snapshot of
//! `pg_stat_statements`, and starts reading every declared container's stats
//! at a fixed interval. At the hold's end it stops the series with one last
//! reading, reads the sizes and the snapshot again, and keeps the statements
//! whose counters moved, costliest first. Every reading is read-only: the
//! sampler creates no extension and resets no counter, so a database without
//! `pg_stat_statements` gets an absent statement table with the reason.
//!
//! A failed reading never fails the run. A container whose stats cannot be
//! read ends its series there with the reason; a size or statement read that
//! fails makes that reading absent with the reason. The readings are context
//! and never bear on a plateau's qualification. No openEHR specification
//! governs this: our own design.

use std::collections::BTreeMap;
use std::future::Future;
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep_until, timeout, timeout_at};

use crate::ixit::Containers;
use crate::perf_result::millis;
use crate::perf_result::server::{
    ContainerRole, ContainerSample, ContainerSeries, ContainersReading, DatabaseReading,
    STATEMENT_TEXT_LIMIT, STATEMENTS_PER_PLATEAU, ServerReadings, Setting, SettingsReading,
    SizeSnapshot, StatementCost, StatementsReading, Truncation,
};

/// The spacing of a container's stats readings.
pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);

/// The longest one Engine API request may take.
const ENGINE_TIMEOUT: Duration = Duration::from_secs(5);

/// The longest one `psql` call may take, the container runtime included.
const SQL_TIMEOUT: Duration = Duration::from_secs(15);

/// The statement timeout every reading runs under, milliseconds.
const STATEMENT_TIMEOUT_MS: u32 = 5_000;

/// The database role every reading connects as: the superuser the stock
/// `postgres` image creates (<https://hub.docker.com/_/postgres>,
/// `POSTGRES_USER`), the role the maintenance settle uses too.
const DATABASE_ROLE: &str = "postgres";

/// The Docker Engine's socket when `DOCKER_HOST` names none
/// (<https://docs.docker.com/reference/cli/dockerd/#daemon-socket-option>).
const DEFAULT_SOCKET: &str = "/var/run/docker.sock";

/// The settings read whatever their source: the planner, memory, WAL and
/// parallelism settings a latency depends on.
const NAMED_SETTINGS: &[&str] = &[
    "checkpoint_timeout",
    "effective_cache_size",
    "effective_io_concurrency",
    "jit",
    "maintenance_work_mem",
    "max_connections",
    "max_parallel_workers",
    "max_parallel_workers_per_gather",
    "max_wal_size",
    "max_worker_processes",
    "random_page_cost",
    "shared_buffers",
    "shared_preload_libraries",
    "synchronous_commit",
    "wal_level",
    "work_mem",
];

/// The sizes of the connected database and of each schema in it.
const SIZES_SQL: &str = "SELECT json_build_object('database', current_database(), \
     'database_bytes', pg_database_size(current_database()), \
     'schemas', COALESCE((SELECT json_object_agg(nspname, bytes) FROM \
     (SELECT n.nspname, sum(pg_total_relation_size(c.oid))::bigint AS bytes \
     FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
     WHERE c.relkind IN ('r', 'm') AND n.nspname NOT IN ('pg_catalog', 'information_schema') \
     AND n.nspname NOT LIKE 'pg\\_toast%' AND n.nspname NOT LIKE 'pg\\_temp%' \
     GROUP BY n.nspname) s), '{}'::json))";

/// Every statement's cumulative counters, by query id; the sampler's own
/// readings are left out by the catalog objects they name.
const STATEMENTS_SQL: &str = "SELECT COALESCE(json_agg(t), '[]'::json) FROM \
     (SELECT queryid::text AS id, sum(calls)::bigint AS calls, \
     sum(total_exec_time)::float8 AS exec_ms, sum(total_plan_time)::float8 AS plan_ms, \
     sum(rows)::bigint AS rows, sum(shared_blks_hit)::bigint AS hit, \
     sum(shared_blks_read)::bigint AS read, min(left(query, 1000)) AS query \
     FROM pg_stat_statements WHERE queryid IS NOT NULL \
     AND query !~* 'pg_stat_statements|pg_database_size|pg_total_relation_size|pg_settings' \
     GROUP BY queryid) t";

/// A reading that could not be taken. Its text becomes the reason the
/// reading is recorded absent or its series truncated.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReadingError {
    /// `DOCKER_HOST` names a transport the sampler does not speak.
    #[error("DOCKER_HOST {0} is neither a unix:// socket nor a tcp:// address")]
    UnsupportedHost(String),
    /// The Engine API could not be reached or did not answer in time.
    #[error("the Docker Engine API: {0}")]
    Engine(String),
    /// The Engine API answered with an error status.
    #[error("the Docker Engine API answered {status} for {path}")]
    EngineStatus {
        /// The status.
        status: u16,
        /// The path asked for.
        path: String,
    },
    /// `psql` could not run, failed, or took too long.
    #[error("psql: {0}")]
    Sql(String),
    /// An answer did not have the expected shape.
    #[error("{what}: {message}")]
    Shape {
        /// The answer.
        what: &'static str,
        /// What was wrong with it.
        message: String,
    },
}

/// The Docker Engine API, over its unix socket or a TCP address.
#[derive(Debug, Clone)]
pub struct DockerEngine {
    client: reqwest::Client,
    base: String,
}

impl DockerEngine {
    /// The engine `DOCKER_HOST` names, or the default socket when it names
    /// none.
    ///
    /// # Errors
    /// [`ReadingError::UnsupportedHost`] for another scheme, and
    /// [`ReadingError::Engine`] when the client cannot be built.
    pub fn from_env() -> Result<Self, ReadingError> {
        let host = std::env::var("DOCKER_HOST").unwrap_or_default();
        Self::for_host(&host)
    }

    /// The engine a `DOCKER_HOST` value names; empty names the default
    /// socket.
    ///
    /// # Errors
    /// As [`DockerEngine::from_env`].
    pub fn for_host(host: &str) -> Result<Self, ReadingError> {
        if host.is_empty() {
            return Self::socket(DEFAULT_SOCKET);
        }
        if let Some(path) = host.strip_prefix("unix://") {
            return Self::socket(path);
        }
        if let Some(address) = host.strip_prefix("tcp://") {
            return Self::http(&format!("http://{address}"));
        }
        Err(ReadingError::UnsupportedHost(host.to_owned()))
    }

    /// The engine listening on the unix socket at `path`.
    ///
    /// # Errors
    /// [`ReadingError::Engine`] when the client cannot be built, or on a
    /// platform without unix sockets.
    pub fn socket(path: &str) -> Result<Self, ReadingError> {
        #[cfg(unix)]
        {
            let client = reqwest::Client::builder()
                .unix_socket(path)
                .timeout(ENGINE_TIMEOUT)
                .build()
                .map_err(|e| ReadingError::Engine(e.to_string()))?;
            Ok(Self {
                client,
                base: "http://localhost".to_owned(),
            })
        }
        #[cfg(not(unix))]
        {
            Err(ReadingError::Engine(format!(
                "the socket {path} needs unix sockets, which this platform lacks"
            )))
        }
    }

    /// The engine answering plain HTTP at `base`, such as
    /// `http://127.0.0.1:2375`.
    ///
    /// # Errors
    /// [`ReadingError::Engine`] when the client cannot be built.
    pub fn http(base: &str) -> Result<Self, ReadingError> {
        let client = reqwest::Client::builder()
            .timeout(ENGINE_TIMEOUT)
            .build()
            .map_err(|e| ReadingError::Engine(e.to_string()))?;
        Ok(Self {
            client,
            base: base.trim_end_matches('/').to_owned(),
        })
    }

    /// One stats reading of `container`, stamped at `offset_ms`.
    ///
    /// # Errors
    /// [`ReadingError::Engine`] for a transport failure,
    /// [`ReadingError::EngineStatus`] for an error status, and
    /// [`ReadingError::Shape`] for an answer without the counters.
    pub async fn sample(
        &self,
        container: &str,
        offset_ms: u64,
    ) -> Result<ContainerSample, ReadingError> {
        let path = format!(
            "/containers/{}/stats?stream=false&one-shot=true",
            urlencoding::encode(container)
        );
        let response = self
            .client
            .get(format!("{}{path}", self.base))
            .send()
            .await
            .map_err(|e| ReadingError::Engine(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(ReadingError::EngineStatus {
                status: status.as_u16(),
                path,
            });
        }
        let body = response
            .bytes()
            .await
            .map_err(|e| ReadingError::Engine(e.to_string()))?;
        let answer: Stats = serde_json::from_slice(&body).map_err(|e| ReadingError::Shape {
            what: "the stats answer",
            message: e.to_string(),
        })?;
        answer.sample(offset_ms)
    }
}

/// The part of an Engine API stats answer the sampler reads.
#[derive(Debug, Deserialize)]
#[expect(
    clippy::struct_field_names,
    reason = "the members are named by the Docker Engine API's own JSON keys"
)]
struct Stats {
    cpu_stats: CpuStats,
    memory_stats: MemoryStats,
    #[serde(default)]
    blkio_stats: Option<BlkioStats>,
    #[serde(default)]
    networks: Option<BTreeMap<String, NetworkStats>>,
}

#[derive(Debug, Deserialize)]
struct CpuStats {
    cpu_usage: CpuUsage,
}

#[derive(Debug, Deserialize)]
struct CpuUsage {
    total_usage: u64,
}

#[derive(Debug, Deserialize)]
struct MemoryStats {
    #[serde(default)]
    usage: Option<u64>,
    #[serde(default)]
    stats: Option<BTreeMap<String, u64>>,
}

#[derive(Debug, Deserialize)]
struct BlkioStats {
    #[serde(default)]
    io_service_bytes_recursive: Option<Vec<BlkioEntry>>,
}

#[derive(Debug, Deserialize)]
struct BlkioEntry {
    op: String,
    value: u64,
}

#[derive(Debug, Deserialize)]
struct NetworkStats {
    rx_bytes: u64,
    tx_bytes: u64,
}

impl Stats {
    /// The counters, with resident memory as the docker CLI computes it:
    /// usage less `inactive_file` on cgroup v2, less `cache` on v1.
    fn sample(&self, offset_ms: u64) -> Result<ContainerSample, ReadingError> {
        let usage = self.memory_stats.usage.ok_or_else(|| ReadingError::Shape {
            what: "the stats answer",
            message: "it carries no memory usage, which a stopped container's does not".to_owned(),
        })?;
        let reclaimable = self
            .memory_stats
            .stats
            .as_ref()
            .and_then(|s| s.get("inactive_file").or_else(|| s.get("cache")))
            .copied()
            .unwrap_or(0);
        let (mut blk_read_bytes, mut blk_write_bytes) = (0_u64, 0_u64);
        let entries = self
            .blkio_stats
            .as_ref()
            .and_then(|b| b.io_service_bytes_recursive.as_deref())
            .unwrap_or_default();
        for entry in entries {
            if entry.op.eq_ignore_ascii_case("read") {
                blk_read_bytes = blk_read_bytes.saturating_add(entry.value);
            } else if entry.op.eq_ignore_ascii_case("write") {
                blk_write_bytes = blk_write_bytes.saturating_add(entry.value);
            }
        }
        let (mut received, mut sent) = (0_u64, 0_u64);
        for network in self.networks.iter().flat_map(BTreeMap::values) {
            received = received.saturating_add(network.rx_bytes);
            sent = sent.saturating_add(network.tx_bytes);
        }
        Ok(ContainerSample {
            offset_ms,
            cpu_ns: self.cpu_stats.cpu_usage.total_usage,
            rss_bytes: usage.saturating_sub(reclaimable),
            blk_read_bytes,
            blk_write_bytes,
            net_rx_bytes: received,
            net_tx_bytes: sent,
        })
    }
}

/// Runs one read-only SQL statement against the database and returns what
/// it printed: a single JSON value.
pub trait SqlReader: Send + Sync + 'static {
    /// Runs `sql` and returns its output.
    fn query(&self, sql: &'static str)
    -> impl Future<Output = Result<String, ReadingError>> + Send;
}

/// `psql` inside the database container, through `docker exec`.
#[derive(Debug, Clone)]
pub struct DockerPsql {
    container: String,
    database: String,
}

impl DockerPsql {
    /// `psql` against `database` inside `container`.
    #[must_use]
    pub fn new(container: &str, database: &str) -> Self {
        Self {
            container: container.to_owned(),
            database: database.to_owned(),
        }
    }
}

impl SqlReader for DockerPsql {
    async fn query(&self, sql: &'static str) -> Result<String, ReadingError> {
        let container = self.container.clone();
        let database = self.database.clone();
        let run = tokio::task::spawn_blocking(move || {
            Command::new("docker")
                .args([
                    "exec",
                    "-e",
                    &format!("PGOPTIONS=-c statement_timeout={STATEMENT_TIMEOUT_MS}"),
                    &container,
                    "psql",
                    "-X",
                    "-q",
                    "-v",
                    "ON_ERROR_STOP=1",
                    "-U",
                    DATABASE_ROLE,
                    "-d",
                    &database,
                    "-At",
                    "-c",
                    sql,
                ])
                .output()
        });
        let output = timeout(SQL_TIMEOUT, run)
            .await
            .map_err(|_elapsed| {
                ReadingError::Sql(format!("no answer within {} s", SQL_TIMEOUT.as_secs()))
            })?
            .map_err(|e| ReadingError::Sql(e.to_string()))?
            .map_err(|e| ReadingError::Sql(format!("docker exec: {e}")))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(ReadingError::Sql(format!(
                "exit {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }
}

fn parse<T: for<'de> Deserialize<'de>>(
    output: &str,
    what: &'static str,
) -> Result<T, ReadingError> {
    serde_json::from_str(output.trim()).map_err(|e| ReadingError::Shape {
        what,
        message: e.to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct SizesRow {
    database: String,
    database_bytes: u64,
    schemas: BTreeMap<String, u64>,
}

async fn read_sizes<S: SqlReader>(sql: &S) -> Result<(String, SizeSnapshot), ReadingError> {
    let row: SizesRow = parse(&sql.query(SIZES_SQL).await?, "the size answer")?;
    Ok((
        row.database,
        SizeSnapshot {
            database_bytes: row.database_bytes,
            schemas: row.schemas,
        },
    ))
}

/// One statement's cumulative counters.
#[derive(Debug, Clone, Deserialize)]
struct StatementRow {
    id: String,
    calls: u64,
    exec_ms: f64,
    plan_ms: f64,
    rows: u64,
    hit: u64,
    read: u64,
    query: String,
}

async fn read_statements<S: SqlReader>(
    sql: &S,
) -> Result<BTreeMap<String, StatementRow>, ReadingError> {
    let rows: Vec<StatementRow> = parse(&sql.query(STATEMENTS_SQL).await?, "the statement answer")?;
    Ok(rows.into_iter().map(|row| (row.id.clone(), row)).collect())
}

/// The statements whose counters moved between two snapshots, costliest
/// first, at most [`STATEMENTS_PER_PLATEAU`]. A statement whose calls went
/// backwards was reset in between, so its end counters are its whole cost.
fn statement_table(
    begin: &BTreeMap<String, StatementRow>,
    end: &BTreeMap<String, StatementRow>,
) -> Vec<StatementCost> {
    let mut table: Vec<StatementCost> = end
        .values()
        .filter_map(|last| {
            let first = begin.get(&last.id).filter(|f| f.calls <= last.calls);
            let calls = last.calls.saturating_sub(first.map_or(0, |f| f.calls));
            (calls > 0).then(|| StatementCost {
                query: last.query.chars().take(STATEMENT_TEXT_LIMIT).collect(),
                calls,
                total_exec_ms: (last.exec_ms - first.map_or(0.0, |f| f.exec_ms)).max(0.0),
                total_plan_ms: (last.plan_ms - first.map_or(0.0, |f| f.plan_ms)).max(0.0),
                rows: last.rows.saturating_sub(first.map_or(0, |f| f.rows)),
                shared_blks_hit: last.hit.saturating_sub(first.map_or(0, |f| f.hit)),
                shared_blks_read: last.read.saturating_sub(first.map_or(0, |f| f.read)),
            })
        })
        .collect();
    table.sort_by(|a, b| {
        b.total_ms()
            .total_cmp(&a.total_ms())
            .then_with(|| a.query.cmp(&b.query))
    });
    table.truncate(STATEMENTS_PER_PLATEAU);
    table
}

#[derive(Debug, Deserialize)]
struct SettingsRow {
    server_version: String,
    settings: BTreeMap<String, Setting>,
}

/// The settings query: every setting not at its built-in default, and the
/// named ones whatever their source.
fn settings_sql() -> &'static str {
    static SQL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        let names = NAMED_SETTINGS
            .iter()
            .map(|n| format!("'{n}'"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "SELECT json_build_object('server_version', current_setting('server_version'), \
             'settings', COALESCE((SELECT json_object_agg(name, json_build_object('setting', \
             setting, 'unit', unit, 'source', source)) FROM pg_settings WHERE source NOT IN \
             ('default', 'override', 'client', 'session') OR name IN ({names})), '{{}}'::json))"
        )
    });
    SQL.as_str()
}

/// The containers a sampler reads, or why it reads none.
pub type ContainerTargets = Result<(Arc<DockerEngine>, Vec<(ContainerRole, String)>), String>;

/// The database a sampler reads, or why it reads none.
pub type DatabaseTarget<S> = Result<Arc<S>, String>;

/// What the sampler read when a hold began.
struct OpenHold {
    sizes: Result<(String, SizeSnapshot), ReadingError>,
    statements: Result<BTreeMap<String, StatementRow>, ReadingError>,
    series: Option<(watch::Sender<bool>, JoinHandle<Vec<ContainerSeries>>)>,
}

/// Reads the server side of every plateau's hold.
pub struct ServerSampler<S: SqlReader> {
    containers: ContainerTargets,
    database: DatabaseTarget<S>,
    interval: Duration,
    open: Mutex<Option<OpenHold>>,
}

impl<S: SqlReader> std::fmt::Debug for ServerSampler<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerSampler")
            .field("containers", &self.containers.as_ref().map(|(_, t)| t))
            .field("database", &self.database.as_ref().map(|_| "declared"))
            .field("interval", &self.interval)
            .finish_non_exhaustive()
    }
}

/// The reason every reading carries when the IXIT declares no containers.
pub const NO_CONTAINERS: &str = "the IXIT declares no `containers`";

/// The reason the database readings carry when the IXIT names no database.
pub const NO_DATABASE: &str = "the IXIT `containers` block names no `database`";

impl ServerSampler<DockerPsql> {
    /// The sampler an IXIT's `containers` block declares: the Engine API
    /// `DOCKER_HOST` names for both containers, and `psql` in the database
    /// container for the database it names.
    #[must_use]
    pub fn from_ixit(containers: Option<&Containers>) -> Self {
        let Some(declared) = containers else {
            return Self::new(Err(NO_CONTAINERS.to_owned()), Err(NO_CONTAINERS.to_owned()));
        };
        let targets = DockerEngine::from_env()
            .map(|engine| {
                (
                    Arc::new(engine),
                    vec![
                        (ContainerRole::Sut, declared.sut.clone()),
                        (ContainerRole::Db, declared.db.clone()),
                    ],
                )
            })
            .map_err(|e| e.to_string());
        let database = declared
            .database
            .as_deref()
            .map(|db| Arc::new(DockerPsql::new(&declared.db, db)))
            .ok_or_else(|| NO_DATABASE.to_owned());
        Self::new(targets, database)
    }
}

impl<S: SqlReader> ServerSampler<S> {
    /// A sampler over `containers` and `database`, each either a target or
    /// the reason it is absent.
    #[must_use]
    pub fn new(containers: ContainerTargets, database: DatabaseTarget<S>) -> Self {
        Self {
            containers,
            database,
            interval: SAMPLE_INTERVAL,
            open: Mutex::new(None),
        }
    }

    /// The same sampler reading the containers every `interval`.
    #[must_use]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Reads the database server's settings.
    pub async fn settings(&self) -> SettingsReading {
        let sql = match &self.database {
            Ok(sql) => sql,
            Err(reason) => {
                return SettingsReading::Absent {
                    reason: reason.clone(),
                };
            }
        };
        let read = async {
            let row: SettingsRow = parse(&sql.query(settings_sql()).await?, "the settings answer")?;
            Ok::<_, ReadingError>(row)
        };
        match read.await {
            Ok(row) => SettingsReading::Present {
                server_version: row.server_version,
                settings: row.settings,
            },
            Err(e) => SettingsReading::Absent {
                reason: e.to_string(),
            },
        }
    }

    /// Starts reading a hold: the sizes, the statement snapshot, and the
    /// container series.
    pub async fn begin(&self) {
        let started = Instant::now();
        let series = self.containers.as_ref().ok().map(|(engine, targets)| {
            let (stop, stopped) = watch::channel(false);
            let handle = tokio::spawn(sample_series(
                Arc::clone(engine),
                targets.clone(),
                self.interval,
                started,
                stopped,
            ));
            (stop, handle)
        });
        let (sizes, statements) = match &self.database {
            Ok(sql) => (
                read_sizes(sql.as_ref()).await,
                read_statements(sql.as_ref()).await,
            ),
            Err(reason) => (
                Err(ReadingError::Sql(reason.clone())),
                Err(ReadingError::Sql(reason.clone())),
            ),
        };
        let hold = OpenHold {
            sizes,
            statements,
            series,
        };
        let _previous = self
            .open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(hold);
    }

    /// Ends the hold [`ServerSampler::begin`] started and returns its
    /// readings.
    pub async fn end(&self) -> ServerReadings {
        let open = self
            .open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let Some(hold) = open else {
            return ServerReadings::absent("the hold's start was not read");
        };
        let containers = match (&self.containers, hold.series) {
            (Err(reason), _) => ContainersReading::Absent {
                reason: reason.clone(),
            },
            (Ok(_), None) => ContainersReading::Absent {
                reason: "the series was not started".to_owned(),
            },
            (Ok(_), Some((stop, handle))) => {
                let _sent = stop.send(true);
                match handle.await {
                    Ok(series) => ContainersReading::Present { series },
                    Err(e) => ContainersReading::Absent {
                        reason: format!("the sampling task ended abnormally: {e}"),
                    },
                }
            }
        };
        let (database, statements) = match &self.database {
            Err(reason) => (
                DatabaseReading::Absent {
                    reason: reason.clone(),
                },
                StatementsReading::Absent {
                    reason: reason.clone(),
                },
            ),
            Ok(sql) => {
                let end_sizes = read_sizes(sql.as_ref()).await;
                let end_statements = read_statements(sql.as_ref()).await;
                (
                    database_reading(hold.sizes, end_sizes),
                    statements_reading(hold.statements, end_statements),
                )
            }
        };
        ServerReadings {
            containers,
            database,
            statements,
        }
    }
}

fn database_reading(
    start: Result<(String, SizeSnapshot), ReadingError>,
    end: Result<(String, SizeSnapshot), ReadingError>,
) -> DatabaseReading {
    match (start, end) {
        (Ok((database, start)), Ok((_, end))) => DatabaseReading::Present {
            database,
            start,
            end,
        },
        (Err(e), _) => DatabaseReading::Absent {
            reason: format!("the sizes at the hold's start: {e}"),
        },
        (Ok(_), Err(e)) => DatabaseReading::Absent {
            reason: format!("the sizes at the hold's end: {e}"),
        },
    }
}

fn statements_reading(
    begin: Result<BTreeMap<String, StatementRow>, ReadingError>,
    end: Result<BTreeMap<String, StatementRow>, ReadingError>,
) -> StatementsReading {
    match (begin, end) {
        (Ok(begin), Ok(end)) => StatementsReading::Present {
            statements: statement_table(&begin, &end),
        },
        (Err(e), _) => StatementsReading::Absent {
            reason: format!("pg_stat_statements at the hold's start: {e}"),
        },
        (Ok(_), Err(e)) => StatementsReading::Absent {
            reason: format!("pg_stat_statements at the hold's end: {e}"),
        },
    }
}

/// Reads every target every `interval` from `started` until `stopped`
/// turns, then once more, so each series spans the hold. A target whose
/// reading fails is truncated there and read no more.
async fn sample_series(
    engine: Arc<DockerEngine>,
    targets: Vec<(ContainerRole, String)>,
    interval: Duration,
    started: Instant,
    mut stopped: watch::Receiver<bool>,
) -> Vec<ContainerSeries> {
    let mut series: Vec<ContainerSeries> = targets
        .into_iter()
        .map(|(role, name)| ContainerSeries {
            role,
            name,
            interval_ms: millis(interval),
            samples: Vec::new(),
            truncated: None,
        })
        .collect();
    let mut tick: u32 = 0;
    loop {
        let last = *stopped.borrow();
        for one in series.iter_mut().filter(|s| s.truncated.is_none()) {
            let offset_ms = millis(started.elapsed());
            match engine.sample(&one.name, offset_ms).await {
                Ok(sample) => one.samples.push(sample),
                Err(e) => {
                    one.truncated = Some(Truncation {
                        at_ms: offset_ms,
                        reason: e.to_string(),
                    });
                }
            }
        }
        if last || series.iter().all(|s| s.truncated.is_some()) {
            break;
        }
        tick = tick.saturating_add(1);
        let next = started + interval.saturating_mul(tick);
        match timeout_at(next, stopped.changed()).await {
            Ok(Ok(())) => {}
            Ok(Err(_closed)) => sleep_until(next).await,
            Err(_elapsed) => {}
        }
    }
    series
}

/// Pays the database's maintenance debt before the clock.
///
/// The settle phase runs `vacuumdb --all --analyze` through the declared
/// database container, so a corpus seeded faster than autovacuum keeps up is
/// measured with current statistics and no vacuum fires inside a plateau.
///
/// NOTE: no openEHR spec governs this — our own design; FerroEHR#4318 traced A/A differences to an un-analyzed clone.
///
/// # Errors
/// A message when the container runtime or `vacuumdb` is unavailable or
/// fails; the run reports it and idles out the settle phase.
pub fn settle_maintenance(db_container: &str) -> Result<(), String> {
    let output = Command::new("docker")
        .args([
            "exec",
            db_container,
            "vacuumdb",
            "-U",
            "postgres",
            "--all",
            "--analyze",
        ])
        .output()
        .map_err(|e| format!("docker exec: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "vacuumdb failed (exit {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, calls: u64, exec_ms: f64, query: &str) -> StatementRow {
        StatementRow {
            id: id.to_owned(),
            calls,
            exec_ms,
            plan_ms: 0.0,
            rows: calls,
            hit: calls * 2,
            read: 0,
            query: query.to_owned(),
        }
    }

    fn snapshot(rows: Vec<StatementRow>) -> BTreeMap<String, StatementRow> {
        rows.into_iter().map(|r| (r.id.clone(), r)).collect()
    }

    #[test]
    fn the_statement_table_holds_what_moved_over_the_hold_costliest_first() {
        let begin = snapshot(vec![
            row("1", 10, 100.0, "SELECT a"),
            row("2", 5, 50.0, "SELECT b"),
            row("3", 900, 9_000.0, "SELECT reset"),
        ]);
        let end = snapshot(vec![
            row("1", 30, 400.0, "SELECT a"),
            row("2", 5, 50.0, "SELECT b"),
            row("3", 4, 40.0, "SELECT reset"),
            row("4", 2, 500.0, "SELECT new"),
        ]);
        let table = statement_table(&begin, &end);
        let names: Vec<&str> = table.iter().map(|s| s.query.as_str()).collect();
        assert_eq!(names, ["SELECT new", "SELECT a", "SELECT reset"]);
        let a = table.iter().find(|s| s.query == "SELECT a").unwrap();
        assert_eq!(a.calls, 20);
        assert!((a.total_exec_ms - 300.0).abs() < 1e-9);
        assert_eq!(a.shared_blks_hit, 40);
    }

    #[test]
    fn the_table_keeps_at_most_ten_statements() {
        let end = snapshot(
            (0..25_u32)
                .map(|i| row(&i.to_string(), 1, f64::from(i), &format!("SELECT {i}")))
                .collect(),
        );
        let table = statement_table(&BTreeMap::new(), &end);
        assert_eq!(table.len(), STATEMENTS_PER_PLATEAU);
        assert_eq!(table.first().map(|s| s.query.as_str()), Some("SELECT 24"));
    }

    #[test]
    fn docker_host_names_a_socket_or_an_address_and_nothing_else() {
        assert!(DockerEngine::for_host("").is_ok());
        assert!(DockerEngine::for_host("unix:///run/user/1000/docker.sock").is_ok());
        let tcp = DockerEngine::for_host("tcp://127.0.0.1:2375").unwrap();
        assert_eq!(tcp.base, "http://127.0.0.1:2375");
        assert_eq!(
            DockerEngine::for_host("ssh://builder").unwrap_err(),
            ReadingError::UnsupportedHost("ssh://builder".to_owned())
        );
    }

    #[test]
    fn a_stats_answer_reads_to_counters_net_of_the_page_cache() {
        let stats: Stats = serde_json::from_str(
            r#"{"cpu_stats":{"cpu_usage":{"total_usage":5000000000}},
                "memory_stats":{"usage":300000000,"stats":{"inactive_file":50000000}},
                "blkio_stats":{"io_service_bytes_recursive":[
                    {"major":254,"minor":0,"op":"read","value":1000},
                    {"major":254,"minor":0,"op":"write","value":2000},
                    {"major":254,"minor":16,"op":"Read","value":10}]},
                "networks":{"eth0":{"rx_bytes":111,"tx_bytes":222},"eth1":{"rx_bytes":9,"tx_bytes":1}}}"#,
        )
        .unwrap();
        let sample = stats.sample(7).unwrap();
        assert_eq!(sample.offset_ms, 7);
        assert_eq!(sample.cpu_ns, 5_000_000_000);
        assert_eq!(sample.rss_bytes, 250_000_000);
        assert_eq!(sample.blk_read_bytes, 1_010);
        assert_eq!(sample.blk_write_bytes, 2_000);
        assert_eq!(sample.net_rx_bytes, 120);
        assert_eq!(sample.net_tx_bytes, 223);

        let stopped: Stats = serde_json::from_str(
            r#"{"cpu_stats":{"cpu_usage":{"total_usage":1}},"memory_stats":{},"blkio_stats":{"io_service_bytes_recursive":null}}"#,
        )
        .unwrap();
        assert!(matches!(stopped.sample(0), Err(ReadingError::Shape { .. })));
    }

    #[test]
    fn the_statement_query_cuts_the_text_where_the_record_does() {
        assert!(STATEMENTS_SQL.contains(&format!("left(query, {STATEMENT_TEXT_LIMIT})")));
    }

    #[test]
    fn the_settings_query_names_every_named_setting() {
        let sql = settings_sql();
        for name in NAMED_SETTINGS {
            assert!(sql.contains(&format!("'{name}'")), "{name}");
        }
    }
}
