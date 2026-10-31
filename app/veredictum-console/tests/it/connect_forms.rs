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

use std::sync::Arc;

use leptos::prelude::FromServerFnError;
use leptos::server_fn::ServerFn;
use reqwest::StatusCode;
use reqwest::header::LOCATION;
use veredictum_console::error::ConsoleError;
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
    // The connect step reads no catalogue, so a stated absence keeps this
    // gate off the validation pass.
    let state = ConsoleState {
        posture,
        rates: veredictum_console::rate_limit::RateLimiter::with(
            rates,
            veredictum_console::rate_limit::Clock::manual(),
        ),
        ..crate::support::state(
            scratch.path(),
            crate::support::absent("no catalogue: this gate drives the connect forms"),
        )
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
    retry_after: Option<String>,
    body: String,
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
        retry_after: response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        body: response.text().await?,
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

/// The connect step a probe returns to offers the stored address again, as
/// the input's server-rendered `value` (#936). A refused probe stores nothing,
/// so its page offers no address: the earlier draft's would read as the one
/// the operator just entered.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn the_page_a_probe_returns_to_offers_the_stored_address()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind(Posture::Local).await?;
    let port = crate::engine_gate::fixture_sut()?;
    let address = format!("http://127.0.0.1:{port}");
    let target = format!("http%3A%2F%2F127.0.0.1%3A{port}");
    let seeded = format!(r#"value="{address}""#);
    let page = |path: String| {
        let origin = bound.origin.clone();
        async move {
            client()?
                .get(format!("{origin}{path}"))
                .send()
                .await?
                .text()
                .await
        }
    };

    let fresh = page(String::from("/run/connect")).await?;
    assert!(
        !fresh.contains(&seeded),
        "a page no form returned to offers no address"
    );

    let form = probe_form(&target, "my-cdr", AuthChoice::None, AuthChoice::None);
    let probed = post(&bound, <ProbeAndSave as ServerFn>::PATH, HTML, form).await?;
    assert_eq!(probed.location.as_deref(), Some(PROBED_PATH));
    let shown = page(String::from(PROBED_PATH)).await?;
    assert!(
        shown.contains(&seeded),
        "the probed page offers the address it probed: {shown}"
    );

    // A second probe from what the page shows reaches the same target.
    let again = probe_form(&target, "my-cdr", AuthChoice::None, AuthChoice::None);
    let second = post(&bound, <ProbeAndSave as ServerFn>::PATH, HTML, again).await?;
    assert_eq!(second.location.as_deref(), Some(PROBED_PATH));

    // A probe of a DIFFERENT address the target guard refuses: its page
    // offers neither that address nor the earlier draft's.
    let elsewhere = probe_form(
        "http%3A%2F%2Fclinical%3As3cret%40cdr.example%2Fopenehr%2Fv1",
        "my-cdr",
        AuthChoice::None,
        AuthChoice::None,
    );
    let plain = post(&bound, <ProbeAndSave as ServerFn>::PATH, HTML, elsewhere).await?;
    assert_eq!(plain.location, Some(ConnectRefusal::Target.path()));
    let restated = page(ConnectRefusal::Target.path()).await?;
    assert!(
        !restated.contains(&seeded),
        "a refused page offers no earlier draft's address: {restated}"
    );
    assert!(
        !restated.contains("s3cret"),
        "the refused address came back: {restated}"
    );
    assert!(
        restated.contains("connect-refused"),
        "the refusal is stated"
    );
    assert!(
        !restated.contains("connect-continue"),
        "no Continue over a refusal"
    );
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
    // The variant the client decodes, end to end over the wire, and the 4xx
    // it answers with (#401).
    assert_eq!(client_call.status, StatusCode::UNPROCESSABLE_ENTITY);
    let decoded = <ConsoleError as FromServerFnError>::de(client_call.body.into());
    assert_eq!(
        decoded,
        ConsoleError::TooLong {
            field: String::from("display name"),
            bytes: 2_000,
            cap: veredictum_console::run_api::IDENTITY_LABEL_CAP_BYTES,
        }
    );
    assert_eq!(ConnectRefusal::from(&decoded), ConnectRefusal::Label);

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
/// Fetches a page as a browser without script does: the whole response, with
/// nothing run on it.
async fn page(bound: &Bound, address: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(client()?
        .get(format!("{}{address}", bound.origin))
        .header("accept", HTML)
        .send()
        .await?
        .text()
        .await?)
}

/// Holds the identity inputs to the markup a browser without script renders:
/// both present, and both ahead of the first streamed chunk. A streamed
/// Suspense arrives as a `<template>` only a script swaps in, so an input
/// inside one never exists for such a browser.
fn assert_identity_inline(address: &str, html: &str) {
    let streamed = html.find("<template").unwrap_or(html.len());
    for id in ["id=\"sut-name\"", "id=\"sut-version\""] {
        let at = html.find(id);
        assert!(
            at.is_some_and(|at| at < streamed),
            "{address}: {id} is not in the markup a browser without script renders"
        );
    }
}

/// The connect step's identity inputs are in the initial HTML on every
/// address it answers at, so the step works without script on all of them.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn the_identity_inputs_render_without_script_on_every_connect_address()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind(Posture::Local).await?;
    let target = fixture()?;
    assert_identity_inline("/run/connect", &page(&bound, "/run/connect").await?);

    let form = probe_form(&target, "typed-cdr", AuthChoice::None, AuthChoice::None);
    post(&bound, <ProbeAndSave as ServerFn>::PATH, HTML, form).await?;
    let probed = page(&bound, PROBED_PATH).await?;
    assert_identity_inline(PROBED_PATH, &probed);
    assert!(
        probed.contains("value=\"typed-cdr\""),
        "{PROBED_PATH}: the stored label is the one rendered"
    );

    let refused = ConnectRefusal::Label.path();
    assert_identity_inline(&refused, &page(&bound, &refused).await?);
    Ok(())
}

/// Every refusal family passes the refusal layer with its own status and a
/// body the client decodes into its variant (#401): a missing draft 409, an
/// idle run 404, a spent budget 429 with `Retry-After`, and the instance's
/// own failure 500 re-encoded rather than rewritten.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn each_refusal_family_answers_its_own_status_and_variant()
-> Result<(), Box<dyn std::error::Error>> {
    use veredictum_console::run_api::fns::{CancelRun, FetchCapabilityRows};

    let rates = veredictum_console::rate_limit::Rates {
        probes: 1,
        ..veredictum_console::rate_limit::Rates::default()
    };
    let bound = bind_with(Posture::Local, rates).await?;
    let decode = |body: String| <ConsoleError as FromServerFnError>::de(body.into());

    let no_draft = post(
        &bound,
        <SaveIdentity as ServerFn>::PATH,
        JSON,
        String::from("sut_name=a&sut_version=b"),
    )
    .await?;
    assert_eq!(no_draft.status, StatusCode::CONFLICT);
    assert_eq!(decode(no_draft.body), ConsoleError::NoDraft);

    let idle = post(
        &bound,
        <CancelRun as ServerFn>::PATH,
        JSON,
        String::from("id=3f2504e0-4f89-41d3-9a0c-0305e82c3301"),
    )
    .await?;
    assert_eq!(idle.status, StatusCode::NOT_FOUND);
    assert!(
        matches!(decode(idle.body), ConsoleError::NotFound { .. }),
        "cancelling a run this process is not driving names nothing found"
    );

    let unavailable = post(
        &bound,
        <FetchCapabilityRows as ServerFn>::PATH,
        JSON,
        String::new(),
    )
    .await?;
    assert_eq!(unavailable.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        decode(unavailable.body),
        ConsoleError::CatalogueUnavailable {
            detail: String::from("no catalogue: this gate drives the connect forms"),
        }
    );

    let target = fixture()?;
    let form = probe_form(&target, "my-cdr", AuthChoice::None, AuthChoice::None);
    let first = post(&bound, <ProbeAndSave as ServerFn>::PATH, JSON, form.clone()).await?;
    assert_eq!(first.status, StatusCode::OK);
    let spent = post(&bound, <ProbeAndSave as ServerFn>::PATH, JSON, form).await?;
    assert_eq!(spent.status, StatusCode::TOO_MANY_REQUESTS);
    let ConsoleError::RateBudget { retry_in_secs, .. } = decode(spent.body) else {
        panic!("a spent budget decodes as RateBudget");
    };
    assert_eq!(
        spent.retry_after,
        Some(retry_in_secs.to_string()),
        "RFC 6585 §4: the 429 says when the budget reopens"
    );
    Ok(())
}

/// A credential typed into the address is refused under the local posture
/// too, stores no draft, and never comes back in the refusal (#401 review).
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn an_address_carrying_a_credential_is_refused_and_never_repeated()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind(Posture::Local).await?;
    let form = probe_form(
        "http%3A%2F%2Fclinical%3As3cret%40127.0.0.1%3A9%2Fopenehr%2Fv1",
        "my-cdr",
        AuthChoice::None,
        AuthChoice::None,
    );
    let refused = post(&bound, <ProbeAndSave as ServerFn>::PATH, JSON, form).await?;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        !refused.body.contains("s3cret") && !refused.body.contains("clinical"),
        "the credential came back: {}",
        refused.body
    );
    assert!(
        matches!(
            <ConsoleError as FromServerFnError>::de(refused.body.into()),
            ConsoleError::Target { .. }
        ),
        "a credential in the address is the target guard's refusal"
    );
    let drafts = bound.state.draft.lock().map_err(|e| e.to_string())?;
    assert!(
        drafts.get(me()).is_none(),
        "a refused address stores no draft"
    );
    Ok(())
}
