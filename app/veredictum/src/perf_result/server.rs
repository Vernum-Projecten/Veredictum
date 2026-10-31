// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The server-side readings of a run: what the declared containers and the
//! database did while a plateau held.
//!
//! The readings are context. They never bear on a plateau's qualification,
//! and each one is `present` or `absent` with the reason it is absent. The
//! raw part keeps the cumulative counters the Docker Engine API reported
//! (<https://docs.docker.com/reference/api/engine/version/v1.47/#tag/Container/operation/ContainerStats>),
//! the database and per-schema sizes `PostgreSQL` reported at the hold's
//! start and end
//! (<https://www.postgresql.org/docs/current/functions-admin.html#FUNCTIONS-ADMIN-DBOBJECT>),
//! and the statements `pg_stat_statements` counted over the hold
//! (<https://www.postgresql.org/docs/current/pgstatstatements.html>).
//! [`summarize`] derives the per-container means and maxima from those
//! counters alone, so `perf-judge` reproduces them. No openEHR
//! specification governs this: our own design.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::perf_result::quantile::u64_to_f64;

/// How many statements a plateau keeps, costliest first.
pub const STATEMENTS_PER_PLATEAU: usize = 10;

/// The longest statement text a plateau keeps, in characters.
pub const STATEMENT_TEXT_LIMIT: usize = 1_000;

/// The reason every reading carries when the run took none.
pub const NO_SAMPLER: &str = "the run declared no server-side sampler";

/// The part a container plays in the deployment (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerRole {
    /// The server under test.
    Sut,
    /// Its database.
    Db,
}

impl ContainerRole {
    /// The serialized token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Sut => "sut",
            Self::Db => "db",
        }
    }
}

/// One stats reading of one container: the cumulative counters at an
/// offset from the hold's start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerSample {
    /// The offset from the hold's start, milliseconds.
    pub offset_ms: u64,
    /// CPU time consumed since the container started, nanoseconds.
    pub cpu_ns: u64,
    /// Resident memory, bytes: usage less the reclaimable page cache.
    pub rss_bytes: u64,
    /// Bytes read from block devices since the container started.
    pub blk_read_bytes: u64,
    /// Bytes written to block devices since the container started.
    pub blk_write_bytes: u64,
    /// Bytes received over every interface since the container started.
    pub net_rx_bytes: u64,
    /// Bytes sent over every interface since the container started.
    pub net_tx_bytes: u64,
}

/// Where and why a series stopped early.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Truncation {
    /// The offset from the hold's start of the reading that failed,
    /// milliseconds.
    pub at_ms: u64,
    /// Why it failed.
    pub reason: String,
}

/// One declared container's series over a hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerSeries {
    /// The part the container plays.
    pub role: ContainerRole,
    /// The container's name, as the IXIT declares it.
    pub name: String,
    /// The planned spacing of the readings, milliseconds.
    pub interval_ms: u64,
    /// The readings, in time order.
    pub samples: Vec<ContainerSample>,
    /// The failed reading that ended the series, when one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<Truncation>,
}

/// The container readings of one hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContainersReading {
    /// The declared containers were read.
    Present {
        /// One series per declared container, the SUT first.
        series: Vec<ContainerSeries>,
    },
    /// No container was read.
    Absent {
        /// Why not.
        reason: String,
    },
}

/// The sizes of the database at one instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SizeSnapshot {
    /// `pg_database_size` of the database, bytes.
    pub database_bytes: u64,
    /// Per schema, the `pg_total_relation_size` of its tables and
    /// materialized views (indexes and TOAST included), bytes.
    pub schemas: BTreeMap<String, u64>,
}

/// The database sizes at a hold's start and end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum DatabaseReading {
    /// Both sizes were read.
    Present {
        /// The database read.
        database: String,
        /// The sizes when the hold began.
        start: SizeSnapshot,
        /// The sizes when the hold ended.
        end: SizeSnapshot,
    },
    /// The sizes were not read.
    Absent {
        /// Why not.
        reason: String,
    },
}

/// One statement's cost over a hold, as `pg_stat_statements` counted it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatementCost {
    /// The normalized statement text, at most [`STATEMENT_TEXT_LIMIT`]
    /// characters.
    pub query: String,
    /// Executions over the hold.
    pub calls: u64,
    /// Execution time over the hold, milliseconds.
    pub total_exec_ms: f64,
    /// Planning time over the hold, milliseconds; zero while
    /// `pg_stat_statements.track_planning` is off.
    pub total_plan_ms: f64,
    /// Rows retrieved or affected over the hold.
    pub rows: u64,
    /// Shared buffer hits over the hold.
    pub shared_blks_hit: u64,
    /// Shared blocks read over the hold.
    pub shared_blks_read: u64,
}

impl StatementCost {
    /// Execution and planning time together, milliseconds.
    #[must_use]
    pub fn total_ms(&self) -> f64 {
        self.total_exec_ms + self.total_plan_ms
    }
}

/// The statement table of one hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum StatementsReading {
    /// The table was read.
    Present {
        /// At most [`STATEMENTS_PER_PLATEAU`] statements, by total time,
        /// costliest first.
        statements: Vec<StatementCost>,
    },
    /// The table was not read.
    Absent {
        /// Why not.
        reason: String,
    },
}

/// The server-side readings of one plateau's hold: context that never bears
/// on its qualification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerReadings {
    /// The declared containers' series.
    pub containers: ContainersReading,
    /// The database sizes at the hold's start and end.
    pub database: DatabaseReading,
    /// The costliest statements over the hold.
    pub statements: StatementsReading,
}

impl ServerReadings {
    /// Readings that are all absent for one `reason`.
    #[must_use]
    pub fn absent(reason: &str) -> Self {
        Self {
            containers: ContainersReading::Absent {
                reason: reason.to_owned(),
            },
            database: DatabaseReading::Absent {
                reason: reason.to_owned(),
            },
            statements: StatementsReading::Absent {
                reason: reason.to_owned(),
            },
        }
    }
}

impl Default for ServerReadings {
    fn default() -> Self {
        Self::absent(NO_SAMPLER)
    }
}

impl ServerReadings {
    /// Every reading that is absent or cut short, as what it is and why.
    #[must_use]
    pub fn absences(&self) -> Vec<(String, &str)> {
        let mut out = Vec::new();
        match &self.containers {
            ContainersReading::Absent { reason } => {
                out.push(("container".to_owned(), reason.as_str()));
            }
            ContainersReading::Present { series } => {
                for one in series {
                    if let Some(cut) = &one.truncated {
                        out.push((format!("further {} reading", one.name), cut.reason.as_str()));
                    }
                }
            }
        }
        if let DatabaseReading::Absent { reason } = &self.database {
            out.push(("database size".to_owned(), reason.as_str()));
        }
        if let StatementsReading::Absent { reason } = &self.statements {
            out.push(("statement".to_owned(), reason.as_str()));
        }
        out
    }
}

/// One `PostgreSQL` setting as `pg_settings` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_field_names,
    reason = "`setting` is the `pg_settings` column the value is read from"
)]
pub struct Setting {
    /// The value, as text.
    pub setting: String,
    /// The unit the value is counted in, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Where the value came from.
    pub source: String,
}

/// The database server's settings, read once per run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum SettingsReading {
    /// The settings were read.
    Present {
        /// `server_version`.
        server_version: String,
        /// Every setting not at its built-in default, and the planner and
        /// memory settings whatever their source, by name.
        settings: BTreeMap<String, Setting>,
    },
    /// The settings were not read.
    Absent {
        /// Why not.
        reason: String,
    },
}

impl Default for SettingsReading {
    fn default() -> Self {
        Self::Absent {
            reason: NO_SAMPLER.to_owned(),
        }
    }
}

/// One container's series, summarized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerSummary {
    /// The part the container plays.
    pub role: ContainerRole,
    /// The container's name.
    pub name: String,
    /// The readings the series holds.
    pub samples: u64,
    /// Whether a failed reading ended the series early.
    pub truncated: bool,
    /// CPU over the series, percent of one core; absent with fewer than two
    /// readings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_mean_pct: Option<f64>,
    /// The highest CPU between two consecutive readings, percent of one
    /// core; absent with fewer than two readings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_max_pct: Option<f64>,
    /// The largest resident memory, bytes; absent with no reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rss_max_bytes: Option<u64>,
    /// Bytes read from block devices over the series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blk_read_bytes: Option<u64>,
    /// Bytes written to block devices over the series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blk_write_bytes: Option<u64>,
    /// Bytes received over the series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_rx_bytes: Option<u64>,
    /// Bytes sent over the series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_tx_bytes: Option<u64>,
}

/// A plateau's server-side readings, summarized.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerSummary {
    /// One summary per container series.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub containers: Vec<ContainerSummary>,
    /// The database's growth over the hold, bytes; negative when it shrank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_growth_bytes: Option<i64>,
}

impl ServerSummary {
    /// Whether the summary holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.containers.is_empty() && self.database_growth_bytes.is_none()
    }
}

/// The growth of a cumulative counter between two readings; absent when the
/// counter went backwards, which a restarted container does.
fn grew(first: u64, last: u64) -> Option<u64> {
    last.checked_sub(first)
}

/// CPU between two readings, percent of one core; absent when no time
/// passed or the counter went backwards.
fn cpu_pct(first: &ContainerSample, last: &ContainerSample) -> Option<f64> {
    let cpu_ns = grew(first.cpu_ns, last.cpu_ns)?;
    let wall_ns = grew(first.offset_ms, last.offset_ms)?.checked_mul(1_000_000)?;
    (wall_ns > 0).then(|| u64_to_f64(cpu_ns) / u64_to_f64(wall_ns) * 100.0)
}

/// Summarizes one container series.
#[must_use]
pub fn summarize_series(series: &ContainerSeries) -> ContainerSummary {
    let first = series.samples.first();
    let last = series.samples.last();
    let span = |counter: fn(&ContainerSample) -> u64| match (first, last) {
        (Some(a), Some(b)) if series.samples.len() > 1 => grew(counter(a), counter(b)),
        _ => None,
    };
    let cpu_mean_pct = match (first, last) {
        (Some(a), Some(b)) if series.samples.len() > 1 => cpu_pct(a, b),
        _ => None,
    };
    let cpu_max_pct = series
        .samples
        .windows(2)
        .filter_map(|pair| match pair {
            [a, b] => cpu_pct(a, b),
            _ => None,
        })
        .reduce(f64::max);
    ContainerSummary {
        role: series.role,
        name: series.name.clone(),
        samples: u64::try_from(series.samples.len()).unwrap_or(u64::MAX),
        truncated: series.truncated.is_some(),
        cpu_mean_pct,
        cpu_max_pct,
        rss_max_bytes: series.samples.iter().map(|s| s.rss_bytes).max(),
        blk_read_bytes: span(|s| s.blk_read_bytes),
        blk_write_bytes: span(|s| s.blk_write_bytes),
        net_rx_bytes: span(|s| s.net_rx_bytes),
        net_tx_bytes: span(|s| s.net_tx_bytes),
    }
}

/// Summarizes a plateau's readings from the readings alone.
#[must_use]
pub fn summarize(readings: &ServerReadings) -> ServerSummary {
    let containers = match &readings.containers {
        ContainersReading::Present { series } => series.iter().map(summarize_series).collect(),
        ContainersReading::Absent { .. } => Vec::new(),
    };
    let database_growth_bytes = match &readings.database {
        DatabaseReading::Present { start, end, .. } => {
            let start = i128::from(start.database_bytes);
            let end = i128::from(end.database_bytes);
            end.checked_sub(start)
                .and_then(|growth| i64::try_from(growth).ok())
        }
        DatabaseReading::Absent { .. } => None,
    };
    ServerSummary {
        containers,
        database_growth_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(offset_ms: u64, cpu_ns: u64, rss_bytes: u64, io: u64) -> ContainerSample {
        ContainerSample {
            offset_ms,
            cpu_ns,
            rss_bytes,
            blk_read_bytes: io,
            blk_write_bytes: io * 2,
            net_rx_bytes: io * 3,
            net_tx_bytes: io * 4,
        }
    }

    #[test]
    fn a_series_summarizes_to_its_mean_maximum_and_growth() {
        let series = ContainerSeries {
            role: ContainerRole::Sut,
            name: "sut".to_owned(),
            interval_ms: 2_000,
            // Half a core for 2 s, then one and a half cores for 2 s.
            samples: vec![
                sample(0, 0, 100, 0),
                sample(2_000, 1_000_000_000, 300, 10),
                sample(4_000, 4_000_000_000, 200, 30),
            ],
            truncated: None,
        };
        let summary = summarize_series(&series);
        assert_eq!(summary.samples, 3);
        assert_eq!(summary.cpu_mean_pct, Some(100.0));
        assert_eq!(summary.cpu_max_pct, Some(150.0));
        assert_eq!(summary.rss_max_bytes, Some(300));
        assert_eq!(summary.blk_read_bytes, Some(30));
        assert_eq!(summary.net_tx_bytes, Some(120));
        assert!(!summary.truncated);
    }

    #[test]
    fn one_reading_gives_no_rate_and_a_reset_counter_gives_no_growth() {
        let single = ContainerSeries {
            role: ContainerRole::Db,
            name: "db".to_owned(),
            interval_ms: 2_000,
            samples: vec![sample(0, 5, 7, 1)],
            truncated: Some(Truncation {
                at_ms: 2_000,
                reason: "gone".to_owned(),
            }),
        };
        let summary = summarize_series(&single);
        assert_eq!(summary.cpu_mean_pct, None);
        assert_eq!(summary.blk_read_bytes, None);
        assert_eq!(summary.rss_max_bytes, Some(7));
        assert!(summary.truncated);

        let restarted = ContainerSeries {
            samples: vec![sample(0, 9_000, 1, 50), sample(2_000, 10, 1, 5)],
            truncated: None,
            ..single
        };
        let summary = summarize_series(&restarted);
        assert_eq!(summary.cpu_mean_pct, None);
        assert_eq!(summary.blk_read_bytes, None);
    }

    #[test]
    fn absent_readings_summarize_to_nothing_and_a_shrinking_database_is_negative() {
        assert!(summarize(&ServerReadings::default()).is_empty());
        let shrinking = ServerReadings {
            database: DatabaseReading::Present {
                database: "cdr".to_owned(),
                start: SizeSnapshot {
                    database_bytes: 1_000,
                    schemas: BTreeMap::new(),
                },
                end: SizeSnapshot {
                    database_bytes: 400,
                    schemas: BTreeMap::new(),
                },
            },
            ..ServerReadings::default()
        };
        assert_eq!(summarize(&shrinking).database_growth_bytes, Some(-600));
    }
}
