// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The byte-identity gate (#54): a run driven through the console's engine
//! seam must emit the results document the engine itself writes. A live run
//! generates fresh inputs, so a second live run from the CLI is held to the
//! same row verdicts, and the engine re-judging the console run's own
//! recording is held to full byte equality. This is what makes "no runner
//! logic is reimplemented in the console" a checked property.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use veredictum_console::engine::{self, Credential, Engine, RunSpec, Secret};
use veredictum_console::run_api::Drafts;
use veredictum_console::submitter::Submitter;

/// The peer address every gate's requests arrive from.
///
/// One fixed address, so a gate driving an axum handler and a gate calling a
/// per-submitter reader are the same visitor.
pub(crate) fn gate_peer() -> std::net::SocketAddr {
    std::net::SocketAddr::from(([198, 51, 100, 1], 40_000))
}

/// The submitter [`gate_peer`] resolves to, derived through the console's own
/// one reader rather than spelled a second time.
pub(crate) fn gate_submitter() -> Submitter {
    veredictum_console::submitter::of_request(None, Some(gate_peer().ip()))
}

/// The drafts map holding exactly one draft, for [`gate_submitter`].
pub(crate) fn drafts_of(draft: veredictum_console::run_api::RunDraft) -> Drafts {
    let mut drafts = Drafts::new();
    drafts.insert(gate_submitter(), draft);
    drafts
}

/// A minimal fixture SUT that answers slowly, so a run lasts long enough for
/// a gate to observe it in flight.
///
/// The same deterministic `500` [`fixture_sut`] gives, delayed per request:
/// the console's caps are about time and concurrency, and a run that finishes
/// instantly can prove neither.
pub(crate) fn slow_fixture_sut(delay: std::time::Duration) -> Result<u16, std::io::Error> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut scratch = [0_u8; 4096];
                let _bytes_read = stream.read(&mut scratch);
                std::thread::sleep(delay);
                let _write = stream.write_all(
                    b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 2\r\nconnection: close\r\n\r\nno",
                );
            });
        }
    });
    Ok(port)
}

/// The named ICS fixture: a filled-in declaration for a product that does not
/// exist, which is the shape these tests need and never a claim about any real
/// product (ISO/IEC 9646-7 gives the support columns to the supplier).
pub(crate) fn declaration_fixture() -> PathBuf {
    crate::support::repo_root().join("fixtures/declaration/statement.json")
}

/// The engine binary for the gate: the [`engine::ENGINE_ENV`] override when
/// set, else the build that produced THIS test executable. Both run paths use
/// the SAME binary, which is exactly the property under test — the console
/// must add and remove nothing around it.
///
/// The path is derived from `current_exe` rather than spelled as
/// `target/debug/veredictum`, because cargo honours `CARGO_TARGET_DIR` and a
/// literal path stops naming the binary cargo just built the moment anything
/// redirects it — which `cargo llvm-cov` always does
/// (<https://doc.rust-lang.org/cargo/reference/environment-variables.html>).
/// A literal path also resolves to a STALE binary from an older build instead
/// of skipping, which is a gate silently grading yesterday's engine.
pub(crate) fn gate_binary() -> PathBuf {
    if let Ok(override_path) = std::env::var(engine::ENGINE_ENV) {
        return PathBuf::from(override_path);
    }
    // `<target>/<profile>/deps/<test binary>` — two hops up is the profile
    // directory cargo places a package's binaries in.
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().and_then(Path::parent).map(Path::to_path_buf))
        .unwrap_or_else(|| crate::support::repo_root().join("target/debug"))
        .join("veredictum")
}

/// A minimal fixture SUT: answers every request `500` with a fixed body, so
/// every driven row fails or errors DETERMINISTICALLY and the two run paths
/// have identical material to record. Listens on an ephemeral loopback port;
/// the thread ends with the test process.
pub(crate) fn fixture_sut() -> Result<u16, std::io::Error> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            // Read whatever arrives without insisting on a full parse: the
            // answer is the same for every request.
            let mut scratch = [0_u8; 4096];
            let _bytes_read = stream.read(&mut scratch);
            let _write = stream.write_all(
                b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 2\r\nconnection: close\r\n\r\nno",
            );
        }
    });
    Ok(port)
}

/// Writes the gate's ixit as the console writes it for a run with no
/// administrative credential: the `sut` principal on Basic credentials under
/// the ordinary principal's env-var NAMES, whose VALUES only the spawned
/// run's environment holds, the `unauthenticated` instance, and no `admin`
/// instance, which the console composes only from an administrative
/// credential with variable names of its own. Raw bytes on purpose: an
/// independently authored wire input catches codec bugs a
/// typed-then-serialized value cannot, and the lib's `Ixit` is
/// deserialize-only, so raw is also the only honest way to author one.
pub(crate) fn write_ixit(dir: &Path, port: u16) -> Result<PathBuf, std::io::Error> {
    let document = format!(
        r#"{{
  "instances": {{
    "sut": {{
      "base_url": "http://127.0.0.1:{port}",
      "auth": {{ "mode": "basic", "user_env": "VEREDICTUM_CDR_USER", "password_env": "VEREDICTUM_CDR_PASSWORD" }}
    }},
    "unauthenticated": {{
      "base_url": "http://127.0.0.1:{port}",
      "auth": {{ "mode": "none" }}
    }}
  }}
}}
"#
    );
    let path = dir.join("ixit.json");
    std::fs::write(&path, document)?;
    Ok(path)
}

/// The document the gate drives: one small document, so the run is seconds.
pub(crate) const GATE_DOCUMENT: &str = "system_options";

/// The gate's run spec over the fixture SUT at `port`, writing into `out`.
pub(crate) fn gate_spec(ixit: PathBuf, port: u16, out: PathBuf) -> RunSpec {
    RunSpec {
        profile: veredictum::arazzo::spec_profile::SpecProfile::Development,
        root: crate::support::repo_root().join("catalogue"),
        base_url: format!("http://127.0.0.1:{port}"),
        auth: "basic",
        admin_auth: "none",
        statement: None,
        ixit: Some(ixit),
        documents: vec![String::from(GATE_DOCUMENT)],
        out_dir: out,
        credentials: vec![
            Credential {
                name: String::from(veredictum::identity::USER_ENV),
                value: Secret::new(String::from("gate-user")),
            },
            Credential {
                name: String::from(veredictum::identity::PASSWORD_ENV),
                value: Secret::new(String::from("gate-pass")),
            },
        ],
    }
}

/// The rows of a results document as `(case, row, status)`, the part of a
/// live run that does not depend on the inputs the instrument generated.
fn verdict_rows(
    results: &veredictum::arazzo::results::Results,
) -> Vec<(String, usize, veredictum::arazzo::results::Status)> {
    results
        .rows
        .iter()
        .map(|row| (row.case.clone(), row.row, row.status))
        .collect()
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_console_run_and_a_cli_run_emit_identical_documents() -> Result<(), Box<dyn std::error::Error>>
{
    let binary = gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first, or set {}",
            binary.display(),
            engine::ENGINE_ENV
        );
        return Ok(());
    }
    // The pin IS the workspace engine version (#179), held there by
    // `scripts/release/check-console-pin.sh`, so a version mismatch here is a
    // broken invariant and fails the gate rather than skipping it.
    let engine = Engine::verified(&binary)?;

    let scratch = assert_fs::TempDir::new()?;
    let port = fixture_sut()?;
    let ixit = write_ixit(scratch.path(), port)?;
    let root = crate::support::repo_root().join("catalogue");

    // Path one: the console's seam.
    let console_out = scratch.path().join("console-run");
    std::fs::create_dir_all(&console_out)?;
    let finished = engine.run(&gate_spec(ixit.clone(), port, console_out), |_line| {})?;
    assert!(
        !finished.results.rows.is_empty(),
        "the document drove no row at all — the gate compared two empty runs"
    );

    // Path two: the CLI, exactly as a terminal user runs it, with the same
    // credentials in ITS environment.
    let cli_out = scratch.path().join("cli-run");
    std::fs::create_dir_all(&cli_out)?;
    let status = std::process::Command::new(engine.binary())
        .args([
            "test",
            "--root",
            &root.to_string_lossy(),
            "--base-url",
            &format!("http://127.0.0.1:{port}"),
            "--auth",
            "basic",
            "--ixit",
            &ixit.to_string_lossy(),
            "--document",
            GATE_DOCUMENT,
            "--out",
            &cli_out.to_string_lossy(),
        ])
        .env(veredictum::identity::USER_ENV, "gate-user")
        .env(veredictum::identity::PASSWORD_ENV, "gate-pass")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    assert_eq!(
        status.success(),
        finished.clean_exit,
        "the two paths disagree about whether the run was green"
    );
    let cli_results: veredictum::arazzo::results::Results =
        serde_json::from_slice(&std::fs::read(cli_out.join("results.json"))?)?;
    assert_eq!(
        verdict_rows(&finished.results),
        verdict_rows(&cli_results),
        "the two live runs judged the same exchanges differently"
    );

    // The gate itself: the engine re-judging the console run's own recording
    // writes the console's results byte for byte, so the console added and
    // removed nothing around the engine.
    let rejudged_out = scratch.path().join("rejudged");
    std::fs::create_dir_all(&rejudged_out)?;
    let rejudged = std::process::Command::new(engine.binary())
        .args([
            "test",
            "--root",
            &root.to_string_lossy(),
            "--ixit",
            &ixit.to_string_lossy(),
            "--document",
            GATE_DOCUMENT,
            "--rejudge",
            &finished.recording_path.to_string_lossy(),
            "--out",
            &rejudged_out.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    assert_eq!(rejudged.success(), finished.clean_exit);
    let console_results = std::fs::read(&finished.results_path)?;
    assert_eq!(
        console_results,
        std::fs::read(rejudged_out.join("results.json"))?,
        "results.json differs between the console seam and the engine's re-judgement"
    );

    // The typed read seam: the document the lib parsed is the document on
    // disk.
    let reread: veredictum::arazzo::results::Results = serde_json::from_slice(&console_results)?;
    assert_eq!(reread, finished.results);
    Ok(())
}

/// The image's entry point is the console binary, so an engine subcommand
/// given to it reaches the pinned engine: its output and its exit status
/// pass through unchanged, and an engine that cannot be found exits `2`.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_console_binary_hands_an_engine_subcommand_to_the_engine()
-> Result<(), Box<dyn std::error::Error>> {
    let binary = gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first, or set {}",
            binary.display(),
            engine::ENGINE_ENV
        );
        return Ok(());
    }
    let console = |engine_path: &Path, args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_veredictum-console"))
            .args(args)
            .env(engine::ENGINE_ENV, engine_path)
            .output()
    };
    let version = console(&binary, &["--version"])?;
    assert_eq!(version.status.code(), Some(0), "{version:?}");
    assert_eq!(
        String::from_utf8(version.stdout)?.trim(),
        engine::expected_version_report()
    );

    let missing = crate::support::repo_root().join("no-such-recording.har");
    let refused = console(&binary, &["test", "--rejudge", &missing.to_string_lossy()])?;
    assert_eq!(refused.status.code(), Some(2), "{refused:?}");
    assert!(
        String::from_utf8(refused.stderr)?.contains("no-such-recording.har"),
        "the engine's own diagnostic passes through"
    );

    let absent = console(Path::new("/nonexistent/veredictum"), &["--version"])?;
    assert_eq!(absent.status.code(), Some(2), "{absent:?}");
    assert!(String::from_utf8(absent.stderr)?.contains(engine::ENGINE_ENV));
    Ok(())
}
