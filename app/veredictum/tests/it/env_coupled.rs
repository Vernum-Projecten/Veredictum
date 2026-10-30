// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The container-runtime-coupled half of the measured instruments, driven
//! against a real engine.
//!
//! Three code paths reach outside this process for their answer: the
//! resource sampler reads the Docker Engine API's per-container stats, the
//! disk anchor and the maintenance settling shell into the DB container, and
//! the server-side readings read the database's sizes, settings and
//! `pg_stat_statements` through `psql` in its container. A fake
//! cannot establish that any of them works, because what is being checked IS
//! the reply of another program, so this module composes a real container
//! and reads the real answers.
//!
//! It SKIPS with a printed reason when no container runtime answers, so a
//! contributor without one is not blocked, and the CI job asserts the engine
//! and the pinned image are both present before the suite runs, so a skip can
//! never be the outcome that gates a merge. When the engine IS present and a
//! path is broken, every test here fails.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken harness must abort the test loudly, Book ch11"
)]
#![expect(
    clippy::print_stderr,
    reason = "a skipped gate must say why on the run log, which is the standing shape of the GnuPG interop gate beside this one"
)]

use std::process::Command;
use std::time::Duration;

use veredictum::ixit::Containers;
use veredictum::perf_emulate::server::ServerSampler;
use veredictum::perf_result::server::{
    ContainersReading, DatabaseReading, SettingsReading, StatementsReading,
};
use veredictum::perf_run::resources::{
    ResourceSampler, SAMPLE_INTERVAL, db_volume_bytes, settle_maintenance,
};

/// The database image the container-coupled paths are read against. The
/// upstream `postgres` image ships `du`, `vacuumdb`, `psql` and the
/// `pg_stat_statements` extension, which is exactly the set those paths
/// shell into, and it keeps its data under the prefix the disk anchor
/// probes.
const DB_IMAGE: &str = "postgres:18.6";

/// Whether a container runtime answers, with its server version as the
/// evidence.
fn engine_version() -> Option<String> {
    let out = Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let version = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (!version.is_empty()).then_some(version)
}

/// Whether the pinned image is on this host, pulling it once if it is not.
fn image_present() -> bool {
    let inspect = Command::new("docker")
        .args(["image", "inspect", DB_IMAGE])
        .output();
    if inspect.is_ok_and(|out| out.status.success()) {
        return true;
    }
    Command::new("docker")
        .args(["pull", DB_IMAGE])
        .output()
        .is_ok_and(|out| out.status.success())
}

/// A running container, removed when the guard drops.
struct DbContainer {
    name: String,
}

impl Drop for DbContainer {
    fn drop(&mut self) {
        let _removed = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .output();
    }
}

impl DbContainer {
    /// Start a fresh database container under a name unique to this test
    /// process, so the suite's own parallelism cannot collide.
    fn start(label: &str) -> Self {
        let name = format!("veredictum-cov-{label}-{}", std::process::id());
        let _stale = Command::new("docker").args(["rm", "-f", &name]).output();
        let run = Command::new("docker")
            .args([
                "run",
                "-d",
                "--name",
                &name,
                "-e",
                "POSTGRES_PASSWORD=postgres",
                DB_IMAGE,
                "-c",
                "shared_preload_libraries=pg_stat_statements",
            ])
            .output()
            .expect("the container runtime answered `docker version`, so `docker run` must run");
        assert!(
            run.status.success(),
            "starting {DB_IMAGE} failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        Self { name }
    }

    /// Block until the database accepts connections, or fail the test.
    ///
    /// `pg_isready` alone is not enough: on a fresh data directory the
    /// image's entrypoint starts a temporary server for initialization,
    /// stops it, and only then starts the real one
    /// (<https://hub.docker.com/_/postgres>, "Caveats"), so a probe can
    /// succeed against the temporary server and the next statement can land
    /// in the restart gap. The entrypoint prints "init process complete"
    /// between the two, so readiness is that line plus a successful probe.
    fn wait_ready(&self) {
        for _ in 0..120 {
            let logs = Command::new("docker")
                .args(["logs", &self.name])
                .output()
                .map(|out| {
                    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
                    text.push_str(&String::from_utf8_lossy(&out.stderr));
                    text
                });
            let initialized =
                logs.is_ok_and(|text| text.contains("PostgreSQL init process complete"));
            let ready = Command::new("docker")
                .args(["exec", &self.name, "pg_isready", "-U", "postgres"])
                .output();
            if initialized && ready.is_ok_and(|out| out.status.success()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        panic!("{} never accepted connections", self.name);
    }

    /// The ixit capability block naming this container in both roles.
    fn containers(&self) -> Containers {
        Containers {
            sut: self.name.clone(),
            db: self.name.clone(),
            database: Some("postgres".to_owned()),
        }
    }
}

/// Prints why a test is skipped and yields nothing, so the reason is in the
/// run log rather than inferred from a silence.
fn skip(test: &str) -> bool {
    match (engine_version(), image_present()) {
        (None, _) => {
            eprintln!("SKIP {test}: no container runtime answers `docker version`");
            true
        }
        (Some(_), false) => {
            eprintln!("SKIP {test}: the pinned image {DB_IMAGE} is neither present nor pullable");
            true
        }
        (Some(version), true) => {
            eprintln!("engine {version}, image {DB_IMAGE}");
            false
        }
    }
}

/// The sampler's whole point is the DELTA: CPU percentage needs two
/// readings, so a container's first tick only seeds the slot and the first
/// emitted sample lands one interval in. The window here is held past that
/// interval, which is the only way to read a real cpu/rss/blkio/network
/// sample off a real engine.
#[test]
fn the_resource_sampler_records_a_real_container_series() {
    if skip("the_resource_sampler_records_a_real_container_series") {
        return;
    }
    let container = DbContainer::start("sampler");
    let containers = container.containers();

    // The phase stamps derive from these bounds; the sample lands one
    // interval in, so it stamps as warmup.
    let sampler = ResourceSampler::start(&containers, 60, 60);
    std::thread::sleep(SAMPLE_INTERVAL + Duration::from_secs(3));
    let (series, notes) = sampler.stop();

    assert!(
        notes.is_empty(),
        "a reachable container still produced degradation notes: {notes:?}"
    );
    assert_eq!(series.len(), 2, "one series per declared container role");
    let recorded = series
        .iter()
        .find(|s| !s.samples.is_empty())
        .expect("no container produced a sample within one interval past the baseline");
    let sample = recorded
        .samples
        .first()
        .expect("the series carries a sample");
    assert!(
        sample.offset_s >= SAMPLE_INTERVAL.as_secs(),
        "the first emitted sample landed before one interval: {}s",
        sample.offset_s
    );
    assert!(sample.rss_bytes > 0, "a running postgres reported no RSS");
    assert!(
        sample.cpu_pct >= 0.0 && sample.cpu_pct.is_finite(),
        "cpu {} is not a percentage of one core",
        sample.cpu_pct
    );
    assert_eq!(sample.phase, veredictum::perf::ResourcePhase::Warmup);
    assert!(
        recorded.cpu_peak() >= sample.cpu_pct,
        "the peak is below a recorded sample"
    );
}

/// The two paths that shell into the DB container: the disk anchor reads
/// the volume's byte total, and the maintenance settling pays the seeding's
/// vacuum debt outside the window. Both are read against a real database,
/// and both name the container when it is not there.
#[test]
fn the_disk_anchor_and_the_maintenance_settling_run_through_the_container() {
    if skip("the_disk_anchor_and_the_maintenance_settling_run_through_the_container") {
        return;
    }
    let container = DbContainer::start("anchors");
    container.wait_ready();

    let bytes = db_volume_bytes(&container.name).expect("the volume has a byte total");
    assert!(bytes > 0, "an initialized cluster occupies no bytes");
    settle_maintenance(&container.name).expect("vacuumdb --all --analyze settles a live cluster");

    // The absent-container side: an anchor that cannot be probed is an
    // absent anchor with a reason, never a fabricated number.
    let missing = "veredictum-cov-no-such-container";
    let error = db_volume_bytes(missing).expect_err("no such container has no volume");
    assert!(error.contains("no byte total"), "{error}");
    let error = settle_maintenance(missing).expect_err("no such container to settle");
    assert!(error.contains("vacuumdb failed"), "{error}");
}

/// The server-side readings, end to end against a real database: the
/// settings, the sizes at a hold's start and end, the statements
/// `pg_stat_statements` counted over the hold, and the container series read
/// through the Engine API. The background query loop stands in for the SUT's
/// own database work, which is the traffic the statement table attributes.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one end-to-end reading: the database, its traffic, the hold, then each reading checked"
)]
fn the_server_side_readings_read_a_real_database_and_container() {
    if skip("the_server_side_readings_read_a_real_database_and_container") {
        return;
    }
    let container = DbContainer::start("readings");
    container.wait_ready();
    let create = Command::new("docker")
        .args([
            "exec",
            &container.name,
            "psql",
            "-U",
            "postgres",
            "-d",
            "postgres",
            "-c",
            "CREATE EXTENSION IF NOT EXISTS pg_stat_statements;",
        ])
        .output()
        .expect("docker exec runs");
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );

    let name = container.name.clone();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let running = std::sync::Arc::clone(&stop);
    let traffic = std::thread::spawn(move || {
        while !running.load(std::sync::atomic::Ordering::Relaxed) {
            let _query = Command::new("docker")
                .args([
                    "exec",
                    &name,
                    "psql",
                    "-U",
                    "postgres",
                    "-d",
                    "postgres",
                    "-Atc",
                    "SELECT count(*) FROM pg_class WHERE relname LIKE 'pg_%';",
                ])
                .output();
        }
    });

    let sampler = ServerSampler::from_ixit(Some(&container.containers()))
        .with_interval(Duration::from_millis(500));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime builds");
    let (settings, readings) = runtime.block_on(async {
        let settings = sampler.settings().await;
        sampler.begin().await;
        tokio::time::sleep(Duration::from_secs(3)).await;
        (settings, sampler.end().await)
    });
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    traffic.join().expect("the traffic thread finished");

    let SettingsReading::Present {
        server_version,
        settings,
    } = settings
    else {
        panic!("no settings: {settings:?}");
    };
    assert!(server_version.starts_with("18"), "{server_version}");
    assert!(settings.contains_key("shared_buffers"));
    assert!(settings.contains_key("shared_preload_libraries"));

    let ContainersReading::Present { series } = &readings.containers else {
        panic!("no container series: {:?}", readings.containers);
    };
    assert_eq!(series.len(), 2, "one series per declared role");
    for one in series {
        assert!(one.truncated.is_none(), "{:?}", one.truncated);
        assert!(
            one.samples.len() >= 2,
            "{} has {} samples",
            one.name,
            one.samples.len()
        );
        assert!(one.samples.iter().all(|s| s.rss_bytes > 0));
    }
    let DatabaseReading::Present { database, end, .. } = &readings.database else {
        panic!("no sizes: {:?}", readings.database);
    };
    assert_eq!(database, "postgres");
    assert!(end.database_bytes > 0);
    let StatementsReading::Present { statements } = &readings.statements else {
        panic!("no statement table: {:?}", readings.statements);
    };
    let statement = statements
        .iter()
        .find(|s| s.query.contains("pg_class"))
        .unwrap_or_else(|| panic!("the background query is not in the table: {statements:?}"));
    assert!(statement.calls > 0);
    assert!(
        statement.shared_blks_hit > 0,
        "a catalogue scan hit no buffer"
    );
    assert!(
        statements
            .iter()
            .all(|s| !s.query.contains("pg_stat_statements")),
        "the sampler's own reads are in its table"
    );
}

/// A declared database without `pg_stat_statements`: the statement table
/// is absent with the server's own reason, and the sizes are still read.
#[test]
fn a_database_without_the_extension_gets_an_absent_statement_table() {
    if skip("a_database_without_the_extension_gets_an_absent_statement_table") {
        return;
    }
    let container = DbContainer::start("no-extension");
    container.wait_ready();
    let sampler = ServerSampler::from_ixit(Some(&container.containers()));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime builds");
    let readings = runtime.block_on(async {
        sampler.begin().await;
        sampler.end().await
    });
    assert!(
        matches!(&readings.statements, StatementsReading::Absent { reason } if reason.contains("pg_stat_statements")),
        "{:?}",
        readings.statements
    );
    assert!(matches!(readings.database, DatabaseReading::Present { .. }));
}
