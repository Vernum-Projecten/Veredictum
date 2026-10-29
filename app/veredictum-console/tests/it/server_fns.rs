// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Every `#[server]` endpoint, driven server-side over a real console state.
//!
//! A server function IS a public HTTP endpoint (rules §0), and the console
//! has no login, so what each one does with an idle console and with hostile
//! input is a security property rather than a nicety. On the `ssr` side the
//! macro expansion calls the body directly, so an ordinary call under a
//! reactive owner carrying the state exercises exactly the code a request
//! would (<https://book.leptos.dev/server/25_server_functions.html>).
//!
//! The gates that need a driven engine live in `export_gate`, `run_live` and
//! `run_scope`. What is pinned here is the OTHER half: an endpoint reached
//! before any run exists answers with its honest empty state or its verbatim
//! refusal, and never with a panic.

use std::path::Path;

use leptos::prelude::{Owner, provide_context};
use veredictum_console::state::ConsoleState;

use crate::engine_gate;

/// A console state over the committed mounts, writing into `out`.
fn state_over(out: &Path) -> ConsoleState {
    let root = engine_gate::repo_root().join("catalogue");
    let specs = engine_gate::repo_root().join("specs/openehr");
    let catalogue =
        veredictum::pipeline::listing::read(&root, Some(&specs)).map_err(|e| e.to_string());
    ConsoleState {
        root,
        specs,
        out: out.to_path_buf(),
        catalogue: std::sync::Arc::new(catalogue),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        jobs: veredictum_console::run_job::JobSlot::default(),
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    }
}

/// Installs `state` as the request context the endpoints read, and hands back
/// the owner that holds it.
///
/// The RETURN VALUE is load-bearing: the reactive system tracks the current
/// owner through a `Weak`, so an owner dropped at the end of this function
/// leaves `Owner::current()` empty and every `expect_context` below panics.
/// Bind it for the whole test body.
fn provide(state: &ConsoleState) -> Owner {
    let owner = Owner::new();
    owner.set();
    provide_context(state.clone());
    owner
}

/// The status a probe answer carries, when the server answered at all.
///
/// The variant's own words, never a re-derivation: a probe is a diagnostic
/// about the network path and the console judges nothing about it.
fn probe_status(answer: &veredictum_console::run_api::ProbeAnswer) -> Option<String> {
    match answer {
        veredictum_console::run_api::ProbeAnswer::Answered { status, .. } => Some(status.clone()),
        veredictum_console::run_api::ProbeAnswer::Unreachable { .. } => None,
    }
}

/// The four catalogue endpoints answer from the committed catalogue, and each
/// answers the same thing its reader does — the server fn adds no arithmetic.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_catalogue_endpoints_answer_what_their_readers_read()
-> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::catalogue_api::{fns, read};

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    assert_eq!(
        fns::fetch_instrument().await.map_err(|e| e.to_string())?,
        read::instrument_view(&state)
    );
    assert_eq!(
        fns::fetch_chapters().await.map_err(|e| e.to_string())?,
        read::chapter_rows(&state)?
    );
    assert_eq!(
        fns::fetch_chapter_bands(String::from("ehr"), String::from("create_ehr"))
            .await
            .map_err(|e| e.to_string())?,
        read::band_rows(&state, "ehr", "create_ehr")?
    );
    let id = String::from("ehr/create_ehr-same_ehr_twice-with_id");
    assert_eq!(
        fns::fetch_case_detail(id.clone())
            .await
            .map_err(|e| e.to_string())?,
        read::case_detail(&state, &id)?
    );
    assert_eq!(
        fns::fetch_case_detail(String::from("no_document/nothing-here"))
            .await
            .map_err(|e| e.to_string())?,
        None,
        "an unknown id is an absent page, not a transport error"
    );
    Ok(())
}

/// The record endpoints reached before any run answer with the empty state
/// the screens render, never with an error the surface has to translate.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_record_endpoints_are_empty_before_a_run() -> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::record_api::fns as record_fns;

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    assert_eq!(
        record_fns::fetch_results()
            .await
            .map_err(|e| e.to_string())?,
        None
    );
    assert_eq!(
        record_fns::fetch_result_detail(String::from("ehr/create_ehr"), 0)
            .await
            .map_err(|e| e.to_string())?,
        None
    );
    Ok(())
}

/// The run wizard's endpoints reached out of order refuse with the step that
/// has not run, and the idle slot refuses a cancel rather than pretending.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_run_endpoints_refuse_an_out_of_order_wizard() -> Result<(), Box<dyn std::error::Error>>
{
    use veredictum_console::run_api::{PostureForm, RunScreen, fns};
    use veredictum_console::run_job::RunId;

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    assert_eq!(
        fns::fetch_draft().await.map_err(|e| e.to_string())?,
        None,
        "no draft before Connect"
    );
    // "No run is in flight" is said only about a request that named no run.
    assert_eq!(
        fns::fetch_run(None).await.map_err(|e| e.to_string())?,
        RunScreen::NoRunNamed,
        "no job before Live"
    );
    // A run this instance never drove says so in its own words, and a run id
    // is untrusted input that can name nothing outside the output mount.
    let stranger: RunId = "3f2504e0-4f89-41d3-9a0c-0305e82c3301".parse()?;
    assert_eq!(
        fns::fetch_run(Some(stranger))
            .await
            .map_err(|e| e.to_string())?,
        RunScreen::Unknown(stranger),
        "an unknown run is not an idle console"
    );

    let scope = fns::save_scope(None, None, PostureForm::default())
        .await
        .expect_err("the scope step needs a connection draft")
        .to_string();
    assert!(scope.contains("no connection draft"), "{scope}");

    let cancel = fns::cancel_run(stranger)
        .await
        .expect_err("an idle slot has nothing to cancel")
        .to_string();
    assert!(cancel.contains("no run is in flight"), "{cancel}");

    // An empty capability selection reaches the composer's own refusal.
    let empty = fns::compose_claim(None)
        .await
        .expect_err("an empty capability selection composes no claim")
        .to_string();
    assert!(!empty.is_empty(), "{empty}");

    // #465: no endpoint dereferences a path any more, so a path pasted into
    // the claim box is refused as the non-declaration it is.
    let outside = fns::save_scope(
        Some(String::from("../../etc/passwd")),
        None,
        PostureForm::default(),
    )
    .await
    .expect_err("a path is not a declaration")
    .to_string();
    assert!(!outside.is_empty(), "{outside}");
    Ok(())
}

/// The reads the Scope screen renders come off the committed catalogue alone,
/// and the capability rows cover the catalogue's folders.
///
/// #465: nothing on this screen offers a declaration. The claim is the
/// supplier's, so the only inputs are the capabilities and the pasted bytes.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_scope_reads_come_off_the_committed_trees() -> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::run_api::fns;

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    let rows = fns::fetch_capability_rows()
        .await
        .map_err(|e| e.to_string())?;
    assert_eq!(
        rows,
        veredictum_console::run_api::read::capability_rows(&state)?,
        "the endpoint adds nothing to its reader"
    );
    assert!(
        rows.iter().any(|row| row.name == "ehr" && row.cases > 0),
        "the ehr capability carries cases: {rows:?}"
    );

    let preview = fns::fetch_scope_preview(String::from("ehr_status"))
        .await
        .map_err(|e| e.to_string())?;
    let everything = fns::fetch_scope_preview(String::new())
        .await
        .map_err(|e| e.to_string())?;
    assert!(
        preview.total > 0 && preview.total < everything.total,
        "a filter narrows the selection: {preview:?} of {everything:?}"
    );
    Ok(())
}

/// The connect step stores what it probed, and the probe's own answer is the
/// server's verbatim words — never a judgement.
///
/// Every authentication mode is driven, because each one puts a different set
/// of credential variables into the draft, and a credential that reached a
/// client-readable field would be the console's worst defect.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_connect_step_records_the_probe_and_keeps_the_secrets()
-> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::run_api::{AuthChoice, ProbeAnswer, fns};

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    let port = engine_gate::fixture_sut()?;
    let base_url = format!("http://127.0.0.1:{port}");
    for (auth, user, password, token) in [
        (AuthChoice::None, "", "", ""),
        (AuthChoice::Basic, "clinical", "s3cret", ""),
        (AuthChoice::Bearer, "", "", "t0ken"),
    ] {
        let answer = fns::probe_and_save(
            base_url.clone(),
            String::from("fixture-sut"),
            String::from("0.0.0-gate"),
            auth,
            user.to_owned(),
            password.to_owned(),
            token.to_owned(),
        )
        .await
        .map_err(|e| e.to_string())?;
        // The fixture answers every request 500, so the probe answered and
        // the answer is not ok — an unreachable server would be the other
        // variant, and neither is an error.
        assert_eq!(
            probe_status(&answer.probe).as_deref(),
            Some("HTTP 500 Internal Server Error"),
            "the probe reports the server's own status line, verbatim"
        );

        let draft = fns::fetch_draft()
            .await
            .map_err(|e| e.to_string())?
            .ok_or("the connect step stored a draft")?;
        assert_eq!(draft.base_url, base_url);
        assert_eq!(draft.sut_name, "fixture-sut");
        assert_eq!(draft.auth, auth.token());
        assert!(!draft.probed_ok, "a 500 is not a passing probe");
        // The client-safe view carries no credential field at all: the
        // secrets live in the server-side draft and the spawned run's
        // environment, nowhere else.
        let serialized = serde_json::to_string(&draft)?;
        for secret in ["s3cret", "t0ken"] {
            assert!(
                !serialized.contains(secret),
                "a credential reached the client-safe draft view: {serialized}"
            );
        }
    }

    // An address nothing listens on is UNREACHABLE, which is an answer too.
    let unreachable = fns::probe_and_save(
        String::from("http://127.0.0.1:1"),
        String::from("nothing-there"),
        String::from("0"),
        AuthChoice::None,
        String::new(),
        String::new(),
        String::new(),
    )
    .await
    .map_err(|e| e.to_string())?;
    assert!(
        matches!(unreachable.probe, ProbeAnswer::Unreachable { .. }),
        "{unreachable:?}"
    );
    Ok(())
}

/// The scope step over a real connection draft: a committed statement is
/// accepted and summarized, an oversized one and a non-statement are refused
/// with the reason, and a capability selection composes a claim the published
/// lib parses back.
///
/// The composed claim is the console's only document-AUTHORING path, so what
/// it emits is held to the lib's own model rather than to a string compare.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test]
async fn the_scope_step_accepts_a_claim_and_refuses_a_non_claim()
-> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::run_api::{AuthChoice, PostureForm, fns};

    let scratch = assert_fs::TempDir::new()?;
    let state = state_over(scratch.path());
    let _owner = provide(&state);

    // The Connect step first: every later step reads the draft it writes.
    let port = engine_gate::fixture_sut()?;
    let probed = fns::probe_and_save(
        format!("http://127.0.0.1:{port}"),
        String::from("scope-sut"),
        String::from("1.2.3"),
        AuthChoice::None,
        String::new(),
        String::new(),
        String::new(),
    )
    .await
    .map_err(|e| e.to_string())?;
    assert!(
        probe_status(&probed.probe).is_some(),
        "the fixture answered, so the draft carries a probe result"
    );

    // The named ICS fixture's own bytes: accepted, and summarized from its
    // own fields.
    let body = std::fs::read_to_string(engine_gate::declaration_fixture())?;
    let summary = fns::save_scope(Some(body.clone()), None, PostureForm::default())
        .await
        .map_err(|e| e.to_string())?
        .ok_or("a pasted statement yields a claim summary")?;
    let declared: veredictum::party::Statement = serde_json::from_str(&body)?;
    assert_eq!(
        summary.product,
        format!("{} {}", declared.product.name, declared.product.version),
        "the summary states the statement's own product identity"
    );
    assert!(summary.capabilities > 0, "{summary:?}");

    let draft = fns::fetch_draft()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("the draft survives the scope step")?;
    assert!(
        draft.statement.is_some(),
        "the accepted claim is on the draft the run will carry"
    );

    // A document that is not JSON at all, and one that is JSON but not a
    // statement: each refused with what is wrong, never silently dropped.
    let bare = PostureForm::default();
    let not_json = fns::save_scope(Some(String::from("{ not json")), None, bare.clone())
        .await
        .expect_err("a body that is not JSON is not a claim")
        .to_string();
    assert!(not_json.contains("not JSON"), "{not_json}");

    let hello = String::from(r#"{"hello":"world"}"#);
    let not_a_statement = fns::save_scope(Some(hello), None, bare.clone())
        .await
        .expect_err("a JSON document that is not a statement is not a claim")
        .to_string();
    assert!(!not_a_statement.is_empty(), "{not_a_statement}");

    // The cap is checked before the parse, so an abusive body never reaches
    // the schema validator at all.
    let oversized = fns::save_scope(Some("0".repeat(4 * 1024 * 1024)), None, bare)
        .await
        .expect_err("a body past the cap is refused")
        .to_string();
    assert!(oversized.contains("the cap is"), "{oversized}");

    // A composed claim: the console's own document, held to the lib's model.
    let composed = fns::compose_claim(Some(vec![String::from("ehr"), String::from("ehr")]))
        .await
        .map_err(|e| e.to_string())?;
    let parsed: veredictum::party::Statement = serde_json::from_str(&composed)?;
    assert_eq!(parsed.product.name, "scope-sut");
    assert_eq!(parsed.product.version, "1.2.3");
    assert!(
        !parsed.schedule_release.is_empty(),
        "a composed claim names the schedule release it targets"
    );
    // The duplicate name collapsed rather than claiming ehr twice.
    let composed_once = fns::compose_claim(Some(vec![String::from("ehr")]))
        .await
        .map_err(|e| e.to_string())?;
    assert_eq!(composed, composed_once);

    // Every capability the Scope screen offers composes into one claim.
    let every: Vec<String> = veredictum_console::run_api::read::capability_rows(&state)?
        .into_iter()
        .map(|row| row.name)
        .collect();
    let all = fns::compose_claim(Some(every.clone()))
        .await
        .map_err(|e| e.to_string())?;
    let parsed: veredictum::party::Statement = serde_json::from_str(&all)?;
    assert_eq!(
        parsed.claims.capabilities.len(),
        every.len(),
        "every offered capability reaches the composed claim: {parsed:?}"
    );
    Ok(())
}
