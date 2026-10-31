// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The #66 acceptance gates: the job supervises a real engine run to a
//! finished view, cancel kills the subprocess, and the generated ixit
//! carries env-var names only.
//!
//! Plus the #389 cap gates: two runs drive at once, a third queues with its
//! place, one address gets one run in flight, and a run past the wall clock
//! is ended by the console with its partial record discarded.

use veredictum_console::engine::{Credential, Engine, RunSpec, Secret};
use veredictum_console::run_api::{
    AuthChoice, DeclaredPostures, DigestEncoding, RunDraft, RunScreen, SigningPosture,
    StartOutcome, read,
};
use veredictum_console::run_job::{JobSlot, JobStatus, JobView, Latest, Limits, RunId};
use veredictum_console::submitter::{Submitter, of_request};

use crate::engine_gate;

/// Polls the map until the NAMED job leaves `Running`, bounded.
///
/// Addressed by id because several runs share the map (#389): "the job" is no
/// longer a thing this suite can ask about.
fn wait_terminal(slot: &JobSlot, id: RunId) -> Option<JobView> {
    // A CI runner takes about a minute to spawn and drive the debug engine,
    // so the bound is wall-clock time, never a poll count.
    let deadline = std::time::Instant::now() + std::time::Duration::from_mins(4);
    while std::time::Instant::now() < deadline {
        let view = slot.view_of(id).ok()??;
        if view.status.is_terminal() {
            return Some(view);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    None
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn a_job_supervises_a_run_to_its_finished_view() -> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;

    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let out = scratch.path().join("job-run");
    std::fs::create_dir_all(&out)?;

    let slot = JobSlot::default();
    let id = slot.allocate_id();
    slot.start(
        id,
        engine_gate::gate_submitter(),
        &engine,
        engine_gate::gate_spec(ixit, port, out.clone()),
        veredictum::party::Sut::operator("job-gate", "1"),
    )?;

    let terminal = wait_terminal(&slot, id).ok_or("the job never left Running")?;
    assert_eq!(
        terminal.status,
        JobStatus::Finished,
        "tail: {:?}",
        terminal.tail
    );
    let finished = terminal.finished.ok_or("finished without a summary")?;
    assert!(
        finished.failed + finished.errored > 0,
        "a 500-only SUT cannot pass"
    );
    assert!(std::path::Path::new(&finished.results_path).is_file());
    // `test` announces no progress, so the counters stay at zero rather than
    // showing a number the engine never stated.
    assert_eq!((terminal.completed, terminal.total), (0, 0));
    Ok(())
}

/// A draft carrying `postures`, with the credential values the redaction
/// tests look for.
fn posture_draft(postures: DeclaredPostures) -> RunDraft {
    RunDraft {
        profile: veredictum::arazzo::spec_profile::SpecProfile::Development,
        base_url: String::from("http://cdr.example"),
        sut_name: String::from("x"),
        sut_version: String::from("y"),
        identity: None,
        auth: AuthChoice::Basic,
        admin_auth: AuthChoice::None,
        credentials: vec![
            Credential {
                name: String::from(veredictum::identity::USER_ENV),
                value: Secret::new(String::from("user-value-hunter2")),
            },
            Credential {
                name: String::from(veredictum::identity::PASSWORD_ENV),
                value: Secret::new(String::from("pass-value-hunter2")),
            },
        ],
        probed_ok: true,
        statement_json: None,
        statement_product: None,
        filter: None,
        postures,
    }
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_generated_ixit_carries_names_and_never_values() -> Result<(), Box<dyn std::error::Error>> {
    let draft = posture_draft(DeclaredPostures::default());
    let document = read::ixit_document(&draft)?;
    assert!(document.contains(veredictum::identity::USER_ENV));
    assert!(document.contains(veredictum::identity::PASSWORD_ENV));
    assert!(
        !document.contains("hunter2"),
        "a credential VALUE reached the ixit document: {document}"
    );
    // The document is a valid ixit by the published lib's own reader.
    let parsed: Result<veredictum::ixit::Ixit, _> = serde_json::from_str(&document);
    assert!(
        parsed.is_ok(),
        "the generated ixit does not parse: {parsed:?}"
    );
    Ok(())
}

/// What the console composes is PINNED, posture by posture (#456): a future
/// edit that drops one fails here instead of silently costing a run its rows.
///
/// The declared document is compared verbatim, so both halves are covered at
/// once — the keys that must be present, and the values they must carry.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_composed_ixit_pins_every_posture_the_console_supplies()
-> Result<(), Box<dyn std::error::Error>> {
    let draft = posture_draft(DeclaredPostures {
        system_id: Some(String::from("cdr.example.org")),
        dump_location: Some(String::from("/var/lib/openehr/dump")),
        signing: Some(SigningPosture::Digest {
            encoding: DigestEncoding::Base64,
            prefix: String::from("sha256:"),
        }),
        spec_profile: Some(veredictum::ixit::SpecProfile::Development),
    });
    let document = read::ixit_document(&draft)?;
    assert_eq!(
        document,
        r#"{
  "instances": {
    "sut": {
      "base_url": "http://cdr.example",
      "auth": {
        "mode": "basic",
        "user_env": "VEREDICTUM_CDR_USER",
        "password_env": "VEREDICTUM_CDR_PASSWORD"
      }
    },
    "unauthenticated": {
      "base_url": "http://cdr.example",
      "auth": {
        "mode": "none"
      }
    }
  },
  "system_id": "cdr.example.org",
  "dump_location": "/var/lib/openehr/dump",
  "spec_profile": "development",
  "signing": {
    "mode": "digest",
    "algorithm": "sha256",
    "encoding": "base64",
    "prefix": "sha256:"
  }
}
"#,
        "the composed ixit drifted"
    );

    // The pgp posture composes the OTHER signing shape, key and all.
    let pgp = posture_draft(DeclaredPostures {
        signing: Some(SigningPosture::Pgp {
            public_key: String::from("-----BEGIN PGP PUBLIC KEY BLOCK-----\narmor\n"),
        }),
        ..DeclaredPostures::default()
    });
    let document = read::ixit_document(&pgp)?;
    assert!(document.contains(r#""mode": "pgp""#), "{document}");
    assert!(
        document.contains("BEGIN PGP PUBLIC KEY BLOCK"),
        "{document}"
    );
    Ok(())
}

/// Every digest vocabulary the console offers is one the engine's verifier
/// implements: composed, read back through the published ixit reader, and
/// handed to `verify`, which answers `Ok` for a mode it understands and `Err`
/// naming the unknown token for one it does not.
///
/// A console offering a posture the run cannot verify under would turn a
/// declaration into an inconclusive row, which is the failure this pin closes.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[expect(
    clippy::disallowed_types,
    reason = "the wire-bodies family: the verifier reads a served ORIGINAL_VERSION envelope as a value, exactly as the driver hands it one"
)]
#[test]
fn every_offered_digest_vocabulary_is_one_the_engine_verifies_under()
-> Result<(), Box<dyn std::error::Error>> {
    let envelope = serde_json::json!({
        "_type": "ORIGINAL_VERSION",
        "uid": { "value": "8849182c-82ad-4088-a07f-48ead4180515::sut.example::1" },
        "signature": "not-the-digest"
    });
    for encoding in DigestEncoding::ALL {
        let draft = posture_draft(DeclaredPostures {
            signing: Some(SigningPosture::Digest {
                encoding,
                prefix: String::new(),
            }),
            ..DeclaredPostures::default()
        });
        let document = read::ixit_document(&draft)?;
        let ixit: veredictum::ixit::Ixit = serde_json::from_str(&document)?;
        let Ok(instance) = ixit.default_instance() else {
            return Err("the composed ixit declares no sut instance".into());
        };
        let mode = ixit
            .signing_of(instance)
            .ok_or("the composed posture reads back")?;
        let verified = veredictum::exec::signature::verify(&envelope, "not-the-digest", mode);
        assert!(
            verified.is_ok(),
            "the engine's verifier does not implement {}: {verified:?}",
            encoding.token()
        );
    }
    Ok(())
}

/// An UNDECLARED posture composes no key at all, which is what makes the
/// engine record the cases that need it not-applicable with its own citation
/// (RM common `master06-change_control_package.adoc` §Digital Signature makes
/// signing conditional on the deployment). A stand-in value here would claim
/// something about somebody else's server.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn an_undeclared_posture_composes_no_key() -> Result<(), Box<dyn std::error::Error>> {
    let document = read::ixit_document(&posture_draft(DeclaredPostures::default()))?;
    for key in ["signing", "system_id", "dump_location", "spec_profile"] {
        assert!(
            !document.contains(key),
            "an undeclared {key} was composed anyway: {document}"
        );
    }
    // And the instances the console DOES compose stay there.
    for instance in ["sut", "unauthenticated"] {
        assert!(document.contains(instance), "{document}");
    }
    Ok(())
}

/// Without an administrative credential the console composes no `admin`
/// instance, so the IXIT never presents the server credential as a second
/// principal; with one, the `admin` instance carries the administrative
/// credential's own variable names and never the server's (#483).
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[expect(
    clippy::disallowed_types,
    reason = "the artifact-loader family: the composed IXIT is read back as a value to inspect which instances it names"
)]
#[test]
fn the_admin_instance_carries_only_the_administrative_credential()
-> Result<(), Box<dyn std::error::Error>> {
    use veredictum::arazzo::credential::{ADMIN_PASSWORD_ENV, ADMIN_TOKEN_ENV, ADMIN_USER_ENV};

    let without = read::ixit_document(&posture_draft(DeclaredPostures::default()))?;
    let parsed: serde_json::Value = serde_json::from_str(&without)?;
    let instances = parsed
        .get("instances")
        .and_then(serde_json::Value::as_object)
        .ok_or("the ixit names its instances")?;
    assert!(!instances.contains_key("admin"), "{without}");
    assert!(!without.contains("ADMIN"), "{without}");

    for (choice, names, mode) in [
        (
            AuthChoice::Basic,
            vec![ADMIN_USER_ENV, ADMIN_PASSWORD_ENV],
            "basic",
        ),
        (AuthChoice::Bearer, vec![ADMIN_TOKEN_ENV], "bearer"),
    ] {
        let mut draft = posture_draft(DeclaredPostures::default());
        draft.admin_auth = choice;
        let document = read::ixit_document(&draft)?;
        let parsed: serde_json::Value = serde_json::from_str(&document)?;
        let admin = parsed
            .pointer("/instances/admin/auth")
            .ok_or("an admin instance is composed")?;
        assert_eq!(
            admin.get("mode").and_then(serde_json::Value::as_str),
            Some(mode)
        );
        let rendered = admin.to_string();
        for name in names {
            assert!(rendered.contains(name), "{rendered}");
        }
        for server_name in [
            veredictum::identity::USER_ENV,
            veredictum::identity::PASSWORD_ENV,
            veredictum::identity::TOKEN_ENV,
        ] {
            assert!(
                !rendered.contains(&format!("\"{server_name}\"")),
                "the admin instance names the server credential: {rendered}"
            );
        }
        let ixit: Result<veredictum::ixit::Ixit, _> = serde_json::from_str(&document);
        assert!(ixit.is_ok(), "the composed ixit does not parse: {ixit:?}");
    }
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[expect(
    clippy::too_many_lines,
    reason = "one gate walks the whole chain — run, address, red-first, explanation, wire — and splitting it would hide the chain it exists to assert"
)]
#[test]
fn the_record_surfaces_read_a_finished_statement_run() -> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;

    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let out = scratch.path().join("record-run");
    std::fs::create_dir_all(&out)?;
    // What start_run does for a pasted claim: the accepted bytes travel with
    // the run.
    let statement = out.join("statement.json");
    std::fs::write(
        &statement,
        r#"{"product":{"name":"record-gate","version":"0.0.0-gate","vendor":"gate","identifier":"urn:veredictum:gate"},"schedule_release":"cnf-2.0-w2","claims":{"capabilities":["system"]}}"#,
    )?;

    let state = veredictum_console::state::ConsoleState {
        root: engine_gate::repo_root().join("catalogue"),
        specs: engine_gate::repo_root().join("specs/openehr"),
        out: scratch.path().to_path_buf(),
        catalogue: std::sync::Arc::new(
            veredictum::pipeline::listing::read(&engine_gate::repo_root().join("catalogue"), None)
                .map_err(|e| e.to_string()),
        ),
        draft: std::sync::Arc::new(std::sync::Mutex::new(engine_gate::drafts_of(RunDraft {
            profile: veredictum::arazzo::spec_profile::SpecProfile::Development,
            base_url: String::from("http://unused"),
            sut_name: String::from("record-gate"),
            sut_version: String::from("0.0.0-gate"),
            identity: None,
            auth: AuthChoice::None,
            admin_auth: AuthChoice::None,
            credentials: vec![],
            probed_ok: true,
            statement_json: Some(std::fs::read_to_string(&statement)?),
            statement_product: Some(String::from("record-gate 0.0.0-gate")),
            filter: None,
            postures: DeclaredPostures::default(),
        }))),
        jobs: JobSlot::default(),
        client_ip_header: None,
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    };
    let id = state.jobs.allocate_id();
    state
        .jobs
        .start(
            id,
            engine_gate::gate_submitter(),
            &engine,
            RunSpec {
                profile: veredictum::arazzo::spec_profile::SpecProfile::Development,
                statement: Some(statement),
                ..engine_gate::gate_spec(ixit, port, out)
            },
            veredictum::party::Sut::operator("record-gate", "1"),
        )
        .map_err(|e| e.to_string())?;
    let terminal = wait_terminal(&state.jobs, id).ok_or("the job never left Running")?;
    assert_eq!(
        terminal.status,
        JobStatus::Finished,
        "tail: {:?}",
        terminal.tail
    );
    assert_eq!(terminal.id, id, "the view answers under the allocated id");

    // The run this process drove streams under its own address, and the bare
    // address resolves to the same run (#386).
    let who = engine_gate::gate_submitter();
    let RunScreen::Live(streamed) = read::run_screen(&state, who, Some(id))? else {
        panic!("the process holding the run streams it");
    };
    assert_eq!(streamed.id, id);
    assert_eq!(
        state
            .jobs
            .latest_of(who, Latest::Any)
            .map_err(|e| e.to_string())?,
        Some(id)
    );
    let RunScreen::Live(bare) = read::run_screen(&state, who, None)? else {
        panic!("a bare /run/live is this submitter's most recent run");
    };
    assert_eq!(bare.id, id);
    // #389: the bare address is per-submitter, and an ADDRESSED run stays
    // readable by anyone holding its id.
    let stranger = of_request(Some("203.0.113.5"), None);
    assert_eq!(
        read::run_screen(&state, stranger, None)?,
        RunScreen::NoRunNamed,
        "a bare /run/live never shows another visitor's run"
    );
    assert!(
        matches!(
            read::run_screen(&state, stranger, Some(id))?,
            RunScreen::Live(_)
        ),
        "a run's own address answers whoever holds it"
    );

    // S6: the results screen reads the results red-first.
    let results = veredictum_console::record_api::read::results_screen(&state, who)
        .map_err(|e| format!("results: {e}"))?
        .ok_or("a finished run must yield a results screen")?;
    let rows = results.reading.rows();
    assert!(!rows.is_empty());
    assert_eq!(results.base_url, format!("http://127.0.0.1:{port}"));
    let first = *rows.first().ok_or("rows checked non-empty")?;
    assert!(
        matches!(first.status.as_str(), "failed" | "errored"),
        "red rows must sort first against a 500-only SUT: {first:?}"
    );

    // The drawer carries the engine's four-part explanation and the exchange
    // it names, read from the recording beside the results.
    let detail =
        veredictum_console::record_api::read::result_detail(&state, who, &first.case, first.row)
            .map_err(|e| format!("detail: {e}"))?
            .ok_or("the first row must have a detail")?;
    assert!(detail.summary.is_some(), "the catalogue join failed");
    assert!(detail.what.is_some() && detail.expected.is_some() && detail.received.is_some());
    assert!(detail.whose.is_some());
    match &detail.recording {
        veredictum_console::record_api::RecordingView::Exchange(exchange) => {
            assert!(
                exchange.request_line.contains(&format!("127.0.0.1:{port}")),
                "{exchange:?}"
            );
            assert_eq!(exchange.status_line, "HTTP 500");
        }
        veredictum_console::record_api::RecordingView::NoExchange => {
            assert!(
                first.status == "errored",
                "a failed row is judged on an exchange: {detail:?}"
            );
        }
        veredictum_console::record_api::RecordingView::Unreadable(why) => {
            panic!("the recording beside the results must read: {why}")
        }
    }
    Ok(())
}
/// A console state over one scratch output tree, with no catalogue: the live
/// screen's resolver reads the slot and the output mount, and nothing else.
fn state_over(out: &std::path::Path) -> veredictum_console::state::ConsoleState {
    veredictum_console::state::ConsoleState {
        root: engine_gate::repo_root().join("catalogue"),
        specs: engine_gate::repo_root().join("specs/openehr"),
        out: out.to_path_buf(),
        catalogue: std::sync::Arc::new(Err(String::from("unused by the live screen"))),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        jobs: JobSlot::default(),
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    }
}

/// The #386 gate: the live screen has three honest answers about a run this
/// process is not driving, and "no run is in flight" is said only about a
/// request that named none.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_live_screen_answers_for_a_run_this_process_never_drove()
-> Result<(), Box<dyn std::error::Error>> {
    // Authored as bytes, the way the engine writes the document the console
    // reads back, so a codec change fails here.
    const RECORD: &str = r#"{
        "document": "catalogue",
        "engine": { "name": "veredictum", "version": "0" },
        "instrument_status": "pre-approval",
        "base_url": "http://recorded.example/openehr/v1",
        "rows": [
            { "case": "a/a", "row": 0, "status": "passed", "inputs": {}, "generated": [] },
            { "case": "a/b", "row": 0, "status": "failed", "inputs": {}, "generated": [] },
            { "case": "a/c", "row": 0, "status": "errored", "inputs": {}, "generated": [] },
            { "case": "a/d", "row": 0, "status": "skipped", "inputs": {}, "generated": [] }
        ]
    }"#;

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let id = state.jobs.allocate_id();

    // Nothing named, nothing in flight: the one place that sentence is true.
    let who = engine_gate::gate_submitter();
    assert_eq!(read::run_screen(&state, who, None)?, RunScreen::NoRunNamed);

    // A named run with no directory: this instance knows nothing of it, and
    // says so about the run rather than about itself.
    assert_eq!(
        read::run_screen(&state, who, Some(id))?,
        RunScreen::Unknown(id)
    );

    // A directory with no results document is a recorded run that left none.
    let dir = veredictum_console::run_job::job_dir(scratch.path(), id);
    std::fs::create_dir_all(&dir)?;
    let RunScreen::Recorded(empty) = read::run_screen(&state, who, Some(id))? else {
        panic!("a job directory is a recorded run");
    };
    assert_eq!(empty.id, id);
    assert_eq!(empty.results, None, "no results document, no outcome");
    assert_eq!(empty.dir, dir.display().to_string());

    // With the record beside it, the tally is the engine's own, read through
    // the published lib.
    std::fs::write(dir.join("results.json"), RECORD)?;
    let RunScreen::Recorded(recorded) = read::run_screen(&state, who, Some(id))? else {
        panic!("a job directory is a recorded run");
    };
    let results = recorded.results.ok_or("the record parses into a tally")?;
    assert_eq!(results.base_url, "http://recorded.example/openehr/v1");
    assert_eq!(
        (
            results.passed,
            results.failed,
            results.errored,
            results.skipped
        ),
        (1, 1, 1, 1)
    );
    assert_eq!(
        results.results_path,
        dir.join("results.json").display().to_string()
    );
    Ok(())
}
/// One visitor, by the last octet of their address.
fn visitor(octet: u8) -> Submitter {
    of_request(
        None,
        Some(std::net::IpAddr::V4(std::net::Ipv4Addr::new(
            203, 0, 113, octet,
        ))),
    )
}

/// One run against the fixture SUT, writing into its own directory.
fn spec_for(port: u16, ixit: &std::path::Path, out_dir: std::path::PathBuf) -> RunSpec {
    engine_gate::gate_spec(ixit.to_path_buf(), port, out_dir)
}

/// Polls until the NAMED job leaves `Running`, within `budget`.
fn wait_terminal_for(slot: &JobSlot, id: RunId, budget: std::time::Duration) -> Option<JobView> {
    let deadline = std::time::Instant::now() + budget;
    while std::time::Instant::now() < deadline {
        let view = slot.view_of(id).ok()??;
        if view.status.is_terminal() {
            return Some(view);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    None
}

/// The #389 concurrency gate, driven at the caps the console ships: two
/// engine processes drive at once, each addressed by its own id and showing
/// only its own facts, and the start past the ceiling is QUEUED with its
/// place stated rather than refused or left spinning.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn two_runs_drive_at_once_and_the_third_is_queued() -> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;
    let scratch = assert_fs::TempDir::new()?;
    // A slow SUT, so the runs are still in flight when the assertions read
    // them: a cap about concurrency cannot be observed on a run that ends
    // before the next line executes.
    let port = engine_gate::slow_fixture_sut(std::time::Duration::from_millis(400))?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;

    let slot = JobSlot::default();
    let limits = slot.limits();
    let mut started: Vec<RunId> = Vec::new();
    for seat in 0..=u8::try_from(limits.max_concurrent).unwrap_or(2) {
        let id = slot.allocate_id();
        let dir = veredictum_console::run_job::job_dir(scratch.path(), id);
        std::fs::create_dir_all(&dir)?;
        slot.start(
            id,
            visitor(seat),
            &engine,
            spec_for(port, &ixit, dir),
            veredictum::party::Sut::operator(format!("cap-gate-{seat}"), "1"),
        )?;
        started.push(id);
    }

    assert_eq!(
        slot.running()?,
        limits.max_concurrent,
        "the ceiling is what drives at once"
    );
    let mut queued = 0_usize;
    for (seat, id) in started.iter().enumerate() {
        let view = slot
            .view_of(*id)?
            .ok_or("every started run is in the map")?;
        assert_eq!(view.id, *id, "a run's view is its own");
        assert_eq!(
            view.sut_name,
            format!("cap-gate-{seat}"),
            "one run's view never carries another's facts"
        );
        if let JobStatus::Queued { position } = view.status {
            queued += 1;
            assert_eq!(
                position,
                u32::try_from(queued).unwrap_or(0),
                "a queued run states its place"
            );
        }
    }
    assert_eq!(
        queued, 1,
        "the start past the ceiling is queued, not refused"
    );

    // Cancelling the queued run removes it without ever spawning a process,
    // and the runs that were driving are untouched.
    let queued_run = started
        .iter()
        .copied()
        .find(|id| {
            slot.view_of(*id)
                .ok()
                .flatten()
                .is_some_and(|view| matches!(view.status, JobStatus::Queued { .. }))
        })
        .ok_or("the queued run was located above")?;
    slot.cancel(queued_run)?;
    assert_eq!(
        slot.view_of(queued_run)?,
        None,
        "a cancelled queue entry leaves no run behind"
    );
    assert_eq!(slot.running()?, limits.max_concurrent);

    for id in started {
        if id != queued_run {
            drop(slot.cancel(id));
        }
    }
    Ok(())
}

/// The #389 per-submitter cap: one address gets one run in flight, and the
/// second start is refused NAMING the run they already have. Another visitor
/// is unaffected.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn a_second_start_from_one_address_names_the_run_it_already_has()
-> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;
    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::slow_fixture_sut(std::time::Duration::from_millis(400))?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;

    let slot = JobSlot::default();
    let mine = visitor(1);
    let first = slot.allocate_id();
    let dir = veredictum_console::run_job::job_dir(scratch.path(), first);
    std::fs::create_dir_all(&dir)?;
    slot.start(
        first,
        mine,
        &engine,
        spec_for(port, &ixit, dir),
        veredictum::party::Sut::operator("cap-gate-first", "1"),
    )?;

    let second = slot.allocate_id();
    let second_dir = veredictum_console::run_job::job_dir(scratch.path(), second);
    std::fs::create_dir_all(&second_dir)?;
    let refusal = slot
        .start(
            second,
            mine,
            &engine,
            spec_for(port, &ixit, second_dir),
            veredictum::party::Sut::operator("cap-gate-second", "1"),
        )
        .expect_err("one address gets one run in flight");
    assert!(
        matches!(refusal, veredictum_console::run_job::JobError::Busy(named) if named == first),
        "the refusal names the run they already have: {refusal:?}"
    );
    assert_eq!(
        slot.view_of(second)?,
        None,
        "the refused start left nothing in the map"
    );
    assert_eq!(
        slot.in_flight_of(mine)?,
        Some(first),
        "the pre-flight and the enforced check agree"
    );
    assert_eq!(
        slot.in_flight_of(visitor(2))?,
        None,
        "another visitor is unaffected"
    );

    drop(slot.cancel(first));
    Ok(())
}

/// The #389 wall-clock cap: a run that outlives its budget is ended by the
/// console, says so rather than blaming the operator, and its partial record
/// is discarded.
///
/// Driven at an injected two seconds. Thirty minutes is the value the server
/// ships and no test may take that long, so what is proven here is the
/// mechanism that number feeds.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_wall_clock_cap_ends_a_run_and_discards_its_record() -> Result<(), Box<dyn std::error::Error>>
{
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;
    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::slow_fixture_sut(std::time::Duration::from_secs(3))?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;

    let slot = JobSlot::with_limits(Limits {
        wall_clock: std::time::Duration::from_secs(2),
        watchdog_tick: std::time::Duration::from_millis(200),
        ..Limits::default()
    });
    let id = slot.allocate_id();
    let dir = veredictum_console::run_job::job_dir(scratch.path(), id);
    std::fs::create_dir_all(&dir)?;
    slot.start(
        id,
        visitor(1),
        &engine,
        spec_for(port, &ixit, dir.clone()),
        veredictum::party::Sut::operator("cap-gate-slow", "1"),
    )?;

    let terminal = wait_terminal_for(&slot, id, std::time::Duration::from_mins(2))
        .ok_or("the capped run never stopped")?;
    assert_eq!(
        terminal.status,
        JobStatus::Expired,
        "the cap ended it, so the screen must not read `cancelled`: {:?}",
        terminal.tail
    );
    assert!(
        terminal.finished.is_none(),
        "a capped run states no outcome"
    );
    // The partial record is discarded: what the run wrote graded nothing.
    for _ in 0..50 {
        if !dir.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(
        !dir.exists(),
        "the capped run's directory survived: {}",
        dir.display()
    );
    Ok(())
}

/// The wizard's own start seam refuses a second run from one address with an
/// ANSWER naming that run, which is what lets the screen link to it.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_start_seam_answers_with_the_run_already_in_flight() -> Result<(), Box<dyn std::error::Error>>
{
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;
    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::slow_fixture_sut(std::time::Duration::from_millis(400))?;

    let state = veredictum_console::state::ConsoleState {
        root: engine_gate::repo_root().join("catalogue"),
        specs: engine_gate::repo_root().join("specs/openehr"),
        out: scratch.path().to_path_buf(),
        catalogue: std::sync::Arc::new(
            veredictum::pipeline::listing::read(&engine_gate::repo_root().join("catalogue"), None)
                .map_err(|e| e.to_string()),
        ),
        draft: std::sync::Arc::new(std::sync::Mutex::new(engine_gate::drafts_of(RunDraft {
            profile: veredictum::arazzo::spec_profile::SpecProfile::Development,
            base_url: format!("http://127.0.0.1:{port}"),
            sut_name: String::from("busy-gate"),
            sut_version: String::from("0.0.0-gate"),
            identity: None,
            auth: AuthChoice::None,
            admin_auth: AuthChoice::None,
            credentials: vec![],
            probed_ok: true,
            statement_json: None,
            statement_product: None,
            filter: Some(String::from(engine_gate::GATE_DOCUMENT)),
            postures: DeclaredPostures::default(),
        }))),
        jobs: JobSlot::default(),
        client_ip_header: None,
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    };

    let who = engine_gate::gate_submitter();
    let StartOutcome::Accepted(first) =
        read::start_run_with(&state, who, &engine).map_err(|e| format!("start: {e}"))?
    else {
        panic!("the first start from an idle address is accepted");
    };
    let again = read::start_run_with(&state, who, &engine).map_err(|e| format!("start: {e}"))?;
    assert_eq!(
        again,
        StartOutcome::AlreadyInFlight(first),
        "the second start names the run they already have"
    );

    drop(state.jobs.cancel(first));
    Ok(())
}
