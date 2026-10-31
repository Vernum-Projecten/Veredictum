// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The connect step's two forms over real HTTP, as a browser with and without
//! the client bundle posts them (#847).
//!
//! A browser without script posts a URL-encoded form asking for HTML and
//! follows the redirect it gets; the client bundle posts the same body asking
//! for JSON and navigates by the redirect header. Both answers are the
//! router's, so every request here travels a socket into the router the
//! server binary builds.

use std::sync::{Arc, Mutex};

use leptos::server_fn::ServerFn;
use reqwest::StatusCode;
use reqwest::header::LOCATION;
use veredictum_console::posture::Posture;
use veredictum_console::run_api::fns::{ProbeAndSave, SaveIdentity};
use veredictum_console::run_api::{AuthChoice, ConnectRefusal, PROBED_PATH, SCOPE_PATH};
use veredictum_console::state::ConsoleState;
use veredictum_console::submitter::{Submitter, of_request};

/// The `Accept` a browser sends with a plain form post.
const HTML: &str = "text/html,application/xhtml+xml";

/// The `Accept` the client bundle sends with a server-function call.
const JSON: &str = "application/json";

/// A bound console, its state, and the scratch tree that state points at.
struct Bound {
    /// The origin every request in a test is sent to.
    origin: String,
    /// The state the router serves, shared with the test.
    state: ConsoleState,
    /// Kept alive for the test's length: dropping it removes the tree.
    _scratch: assert_fs::TempDir,
}

/// Binds the router the server binary builds, under `posture`, on an
/// ephemeral loopback port.
async fn bind(posture: Posture) -> Result<Bound, Box<dyn std::error::Error>> {
    bind_with(posture, veredictum_console::rate_limit::Rates::default()).await
}

/// [`bind`] under explicit rates, so a test can spend a budget in one call.
async fn bind_with(
    posture: Posture,
    rates: veredictum_console::rate_limit::Rates,
) -> Result<Bound, Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = ConsoleState {
        root: crate::engine_gate::repo_root().join("catalogue"),
        specs: crate::engine_gate::repo_root().join("specs/openehr"),
        out: scratch.path().to_path_buf(),
        // The connect step reads no catalogue, so a stated absence keeps this
        // gate off the validation pass.
        catalogue: Arc::new(Err(String::from(
            "no catalogue: this gate drives the connect forms",
        ))),
        draft: Arc::new(Mutex::new(veredictum_console::run_api::Drafts::new())),
        jobs: veredictum_console::run_job::JobSlot::default(),
        client_ip_header: None,
        posture,
        rates: veredictum_console::rate_limit::RateLimiter::with(
            rates,
            veredictum_console::rate_limit::Clock::manual(),
        ),
        capture: false,
    };
    let options = leptos::prelude::LeptosOptions::builder()
        .output_name("veredictum-console")
        .build();
    let app = veredictum_console::router::router(&state, options)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    drop(tokio::spawn(veredictum_console::linger::serve(
        listener,
        app,
        std::future::pending(),
    )));
    Ok(Bound {
        origin: format!("http://{addr}"),
        state,
        _scratch: scratch,
    })
}

/// The submitter every request from this test process is.
fn me() -> Submitter {
    of_request(None, Some(std::net::Ipv4Addr::LOCALHOST.into()))
}

/// A client that reports a redirect instead of following it.
fn client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

/// One answer, reduced to what these tests read.
struct Answer {
    status: StatusCode,
    location: Option<String>,
    navigates: bool,
}

/// Posts a URL-encoded form to a server function, asking for `accept`.
async fn post(
    bound: &Bound,
    path: &str,
    accept: &str,
    form: String,
) -> Result<Answer, Box<dyn std::error::Error>> {
    let response = client()?
        .post(format!("{}{path}", bound.origin))
        .header("accept", accept)
        .header("referer", format!("{}/run/connect", bound.origin))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form)
        .send()
        .await?;
    Ok(Answer {
        status: response.status(),
        location: response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        navigates: response
            .headers()
            .contains_key(leptos::server_fn::redirect::REDIRECT_HEADER),
    })
}

/// The probe form exactly as the connect step's controls name it: the
/// credential modes spelled by [`AuthChoice::form_value`], every member of
/// both credentials present.
fn probe_form(base_url: &str, sut_name: &str, server: AuthChoice, admin: AuthChoice) -> String {
    format!(
        "base_url={base_url}&sut_name={sut_name}&sut_version=1.0.0\
         &server[auth]={}&server[user]=clinical&server[password]=s3cret&server[token]=\
         &admin[auth]={}&admin[user]=&admin[password]=&admin[token]=adm1n-t0ken",
        server.form_value(),
        admin.form_value(),
    )
}

/// The fixture SUT's address, URL-encoded for a form.
fn fixture() -> Result<String, std::io::Error> {
    let port = crate::engine_gate::fixture_sut()?;
    Ok(format!("http%3A%2F%2F127.0.0.1%3A{port}"))
}

/// A probe posted without script returns to the connect step with its stored
/// answer, and the same body from the client bundle answers in place. The
/// stored modes are the ones the form named, which pins the `arg[field]`
/// indexing the controls rely on.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn a_plain_probe_returns_to_its_stored_answer_and_a_client_probe_stays()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind(Posture::Local).await?;
    let target = fixture()?;
    let form = probe_form(&target, "my-cdr", AuthChoice::Basic, AuthChoice::Bearer);

    let plain = post(&bound, <ProbeAndSave as ServerFn>::PATH, HTML, form.clone()).await?;
    assert_eq!(plain.status, StatusCode::FOUND);
    assert_eq!(plain.location.as_deref(), Some(PROBED_PATH));
    {
        let drafts = bound.state.draft.lock().map_err(|e| e.to_string())?;
        let draft = drafts.get(me()).ok_or("the probe stored a draft")?;
        assert_eq!(
            draft.auth,
            AuthChoice::Basic,
            "server[auth] reached the draft"
        );
        assert_eq!(
            draft.admin_auth,
            AuthChoice::Bearer,
            "admin[auth] reached the draft"
        );
        assert!(draft.probe.is_some(), "the answer is kept to show again");
    }

    let client_call = post(&bound, <ProbeAndSave as ServerFn>::PATH, JSON, form).await?;
    assert_eq!(client_call.status, StatusCode::OK);
    assert_eq!(
        client_call.location, None,
        "the hydrated page does not navigate"
    );
    assert!(!client_call.navigates);
    Ok(())
}

/// Continue posted without script is redirected to Scope; from the client
/// bundle it answers with the redirect header the router navigates by.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn a_saved_identity_continues_to_scope_with_and_without_script()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind(Posture::Local).await?;
    let target = fixture()?;
    let form = probe_form(&target, "my-cdr", AuthChoice::None, AuthChoice::None);
    post(&bound, <ProbeAndSave as ServerFn>::PATH, JSON, form).await?;

    let labels = String::from("sut_name=my-cdr&sut_version=1.0.1");
    let plain = post(
        &bound,
        <SaveIdentity as ServerFn>::PATH,
        HTML,
        labels.clone(),
    )
    .await?;
    assert_eq!(plain.status, StatusCode::FOUND);
    assert_eq!(plain.location.as_deref(), Some(SCOPE_PATH));

    let client_call = post(&bound, <SaveIdentity as ServerFn>::PATH, JSON, labels).await?;
    assert_eq!(client_call.status, StatusCode::OK);
    assert!(client_call.navigates, "the client navigates by the header");
    assert_eq!(client_call.location.as_deref(), Some(SCOPE_PATH));
    Ok(())
}

/// A refused form posted without script returns to the connect step naming
/// the refusal, and a refused newer probe forgets the older answer, so the
/// page can never show Continue over a refusal (#847).
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_plain_form_returns_to_the_connect_step_naming_its_refusal()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind(Posture::Local).await?;
    let target = fixture()?;

    // A save before any probe has no draft to land on.
    let no_draft = post(
        &bound,
        <SaveIdentity as ServerFn>::PATH,
        HTML,
        String::from("sut_name=cdr&sut_version=1"),
    )
    .await?;
    assert_eq!(no_draft.status, StatusCode::FOUND);
    assert_eq!(
        no_draft.location,
        Some(ConnectRefusal::NoDraft.path()),
        "the refusal travels by kind"
    );

    let probed = probe_form(&target, "my-cdr", AuthChoice::None, AuthChoice::None);
    post(&bound, <ProbeAndSave as ServerFn>::PATH, HTML, probed).await?;
    let oversized = "x".repeat(2_000);
    let refused = probe_form(&target, &oversized, AuthChoice::None, AuthChoice::None);
    let plain = post(
        &bound,
        <ProbeAndSave as ServerFn>::PATH,
        HTML,
        refused.clone(),
    )
    .await?;
    assert_eq!(plain.status, StatusCode::FOUND);
    assert_eq!(plain.location, Some(ConnectRefusal::Label.path()));
    {
        let drafts = bound.state.draft.lock().map_err(|e| e.to_string())?;
        let draft = drafts.get(me()).ok_or("the earlier probe's draft stays")?;
        assert!(
            draft.probe.is_none(),
            "the older answer is forgotten once a newer probe is refused"
        );
    }

    let client_call = post(&bound, <ProbeAndSave as ServerFn>::PATH, JSON, refused).await?;
    assert_ne!(client_call.status, StatusCode::FOUND);
    assert_eq!(
        client_call.location, None,
        "the client reads the refusal from the body"
    );

    let page = client()?
        .get(format!("{}{}", bound.origin, ConnectRefusal::Label.path()))
        .send()
        .await?
        .text()
        .await?;
    assert!(page.contains("connect-refused"), "the refusal is stated");
    assert!(
        !page.contains("connect-continue"),
        "no Continue over a refusal"
    );
    Ok(())
}

/// A public instance refuses a loopback target, and a probe posted without
/// script is told so by kind.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_target_returns_to_the_connect_step_naming_the_target()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind(Posture::Hosted).await?;
    let target = fixture()?;
    let form = probe_form(&target, "my-cdr", AuthChoice::None, AuthChoice::None);
    let plain = post(&bound, <ProbeAndSave as ServerFn>::PATH, HTML, form).await?;
    assert_eq!(plain.status, StatusCode::FOUND);
    assert_eq!(plain.location, Some(ConnectRefusal::Target.path()));
    Ok(())
}

/// Every address the connect forms redirect to is a route the console
/// serves, so a constant can never drift from the route table.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn every_connect_redirect_names_a_served_route() -> Result<(), Box<dyn std::error::Error>> {
    let bound = bind(Posture::Local).await?;
    let mut addresses = vec![String::from(PROBED_PATH), String::from(SCOPE_PATH)];
    addresses.extend(ConnectRefusal::ALL.map(ConnectRefusal::path));
    for address in addresses {
        let response = client()?
            .get(format!("{}{address}", bound.origin))
            .send()
            .await?;
        assert_eq!(response.status(), StatusCode::OK, "{address}");
        let page = response.text().await?;
        assert!(
            !page.contains("Page not found"),
            "{address} fell through to the 404 fallback"
        );
    }
    Ok(())
}

/// Reads the refused page a plain form post landed on, and holds it to
/// stating the refusal with no Continue.
async fn assert_refused_page(
    bound: &Bound,
    location: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let location = location.ok_or("a refused plain post is redirected")?;
    let page = client()?
        .get(format!("{}{location}", bound.origin))
        .send()
        .await?
        .text()
        .await?;
    assert!(page.contains("connect-refused"), "the refusal is stated");
    assert!(
        !page.contains("connect-continue"),
        "no Continue over a refusal"
    );
    Ok(())
}

/// A spent probe budget refuses a plain probe by kind, and the page it lands
/// on offers no Continue.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn a_spent_probe_budget_returns_to_the_connect_step_naming_the_budget()
-> Result<(), Box<dyn std::error::Error>> {
    let rates = veredictum_console::rate_limit::Rates {
        probes: 1,
        ..veredictum_console::rate_limit::Rates::default()
    };
    let bound = bind_with(Posture::Local, rates).await?;
    let target = fixture()?;
    let form = probe_form(&target, "my-cdr", AuthChoice::None, AuthChoice::None);
    let first = post(&bound, <ProbeAndSave as ServerFn>::PATH, HTML, form.clone()).await?;
    assert_eq!(first.location.as_deref(), Some(PROBED_PATH));

    let spent = post(&bound, <ProbeAndSave as ServerFn>::PATH, HTML, form).await?;
    assert_eq!(spent.status, StatusCode::FOUND);
    assert_eq!(spent.location, Some(ConnectRefusal::Budget.path()));
    {
        let drafts = bound.state.draft.lock().map_err(|e| e.to_string())?;
        let draft = drafts.get(me()).ok_or("the first probe's draft stays")?;
        assert!(draft.probe.is_none(), "the older answer is forgotten");
    }
    assert_refused_page(&bound, spent.location.as_deref()).await
}

/// A draft store that cannot be written refuses a plain probe by kind, after
/// the server was probed, and the page it lands on offers no Continue.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_draft_store_returns_to_the_connect_step_naming_the_store()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind(Posture::Local).await?;
    // A writer that panics while holding the lock poisons it, which is the
    // one way the in-memory store fails.
    let draft = Arc::clone(&bound.state.draft);
    let poisoned = std::thread::spawn(move || {
        let _held = draft.lock();
        panic!("poisons the draft lock on purpose");
    })
    .join();
    assert!(poisoned.is_err(), "the writer panicked holding the lock");

    let target = fixture()?;
    let form = probe_form(&target, "my-cdr", AuthChoice::None, AuthChoice::None);
    let refused = post(&bound, <ProbeAndSave as ServerFn>::PATH, HTML, form).await?;
    assert_eq!(refused.status, StatusCode::FOUND);
    assert_eq!(refused.location, Some(ConnectRefusal::Store.path()));
    assert_refused_page(&bound, refused.location.as_deref()).await
}
