// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The operator's signal (#489), over real HTTP and over real runs.
//!
//! The route is driven through the router the server binary builds, because
//! who it answers is a property of the route and its layers together. The run
//! counters are driven through a job map spawning the gate engine, because
//! what they count is what the supervisor actually did.

use std::sync::{Arc, Mutex};

use leptos::server_fn::ServerFn;
use veredictum_console::metrics::{AccessToken, Metrics};
use veredictum_console::run_api::fns::{ProbeAndSave, SaveScope};
use veredictum_console::run_job::{JobSlot, JobStatus, Limits, RunId};

use crate::engine_gate;

/// The token every bound console in this module is opened by.
const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A bound console, its counters, and the scratch tree its state points at.
struct Bound {
    /// The origin every request in a test is sent to.
    origin: String,
    /// Kept alive for the test's length: dropping it removes the tree.
    _scratch: assert_fs::TempDir,
}

/// Binds the server binary's router in the hosted posture, its metrics opened
/// by `token`.
async fn bind(token: Option<&str>) -> Result<Bound, Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let metrics = Metrics::default().with_access(token.map(AccessToken::parse).transpose()?);
    let state = veredictum_console::state::ConsoleState {
        root: engine_gate::repo_root().join("catalogue"),
        specs: engine_gate::repo_root().join("specs/openehr"),
        out: scratch.path().to_path_buf(),
        catalogue: Arc::new(Err(String::from("no catalogue: this gate drives metrics"))),
        draft: Arc::new(Mutex::new(veredictum_console::run_api::Drafts::new())),
        jobs: JobSlot::default().with_metrics(metrics.clone()),
        client_ip_header: None,
        posture: veredictum_console::posture::Posture::Hosted,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
        metrics,
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
        _scratch: scratch,
    })
}

/// Reads the route with `authorization` as the header, when any.
async fn read_metrics(
    bound: &Bound,
    authorization: Option<&str>,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut request = reqwest::Client::new().get(format!(
        "{}{}",
        bound.origin,
        veredictum_console::metrics::ROUTE
    ));
    if let Some(value) = authorization {
        request = request.header("authorization", value);
    }
    request.send().await
}

/// Posts one URL-encoded server-function call.
async fn call(
    bound: &Bound,
    path: &str,
    form: &str,
) -> Result<reqwest::StatusCode, reqwest::Error> {
    Ok(reqwest::Client::new()
        .post(format!("{}{path}", bound.origin))
        .header("accept", "application/json")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form.to_owned())
        .send()
        .await?
        .status())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn with_no_token_set_no_metrics_are_served() -> Result<(), Box<dyn std::error::Error>> {
    let bound = bind(None).await?;
    let response = read_metrics(&bound, Some(&format!("Bearer {TOKEN}"))).await?;
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
    assert!(
        !response.text().await?.contains("veredictum_"),
        "nothing of the exposition leaks"
    );
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn only_the_operators_token_opens_the_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let bound = bind(Some(TOKEN)).await?;
    for presented in [None, Some("Bearer wrong"), Some(TOKEN)] {
        let response = read_metrics(&bound, presented).await?;
        assert_eq!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "{presented:?}"
        );
        assert_eq!(
            response
                .headers()
                .get("www-authenticate")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer realm=\"veredictum-metrics\""),
            "RFC 6750 §3: a 401 names the scheme it wants"
        );
        assert!(!response.text().await?.contains("veredictum_runs"));
    }
    let response = read_metrics(&bound, Some(&format!("Bearer {TOKEN}"))).await?;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some(veredictum_console::metrics::MEDIA_TYPE)
    );
    let text = response.text().await?;
    assert!(
        text.contains("veredictum_runs_accepted_total 0\n"),
        "{text}"
    );
    assert!(text.contains("veredictum_runs_max_concurrent "), "{text}");
    Ok(())
}

/// A target refusal is counted by its reason and the address never appears;
/// a malformed call is counted as the 4xx its caller read.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn refusals_and_server_fn_answers_are_counted_and_name_no_target()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bind(Some(TOKEN)).await?;
    let refused = call(
        &bound,
        <ProbeAndSave as ServerFn>::PATH,
        "base_url=http%3A%2F%2F127.0.0.42%3A8080%2Fsecret-path&sut_name=a&sut_version=1\
         &server[auth]=None&server[user]=&server[password]=&server[token]=\
         &admin[auth]=None&admin[user]=&admin[password]=&admin[token]=",
    )
    .await?;
    assert!(
        refused.is_server_error(),
        "a server function's refusal answers 5xx: {refused}"
    );
    let malformed = call(&bound, <SaveScope as ServerFn>::PATH, "nonsense").await?;
    assert_eq!(malformed, reqwest::StatusCode::BAD_REQUEST);

    let text = read_metrics(&bound, Some(&format!("Bearer {TOKEN}")))
        .await?
        .text()
        .await?;
    assert!(
        text.contains("veredictum_refusals_total{guard=\"target\",reason=\"loopback\"} 1\n"),
        "{text}"
    );
    assert!(
        text.contains("veredictum_server_fn_responses_total{class=\"4xx\"} 1\n"),
        "{text}"
    );
    assert!(
        text.contains("veredictum_server_fn_responses_total{class=\"5xx\"} 1\n"),
        "{text}"
    );
    for leaked in ["127.0.0.42", "secret-path", "8080"] {
        assert!(!text.contains(leaked), "{leaked} reached the exposition");
    }
    Ok(())
}

/// Polls the map until the named run is terminal, bounded by wall-clock time.
fn wait_terminal(slot: &JobSlot, id: RunId) -> Option<JobStatus> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_mins(4);
    while std::time::Instant::now() < deadline {
        let status = slot.view_of(id).ok()??.status;
        if status.is_terminal() {
            return Some(status);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    None
}

/// Every accepted run is counted once, where it ended: one that finished, one
/// cancelled while it drove, and one withdrawn from the queue.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
#[test]
fn the_job_map_counts_every_run_where_it_ended() -> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = veredictum_console::engine::Engine::verified(&binary)?;
    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::slow_fixture_sut(std::time::Duration::from_millis(400))?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let slot = JobSlot::with_limits(Limits {
        max_concurrent: 1,
        ..Limits::default()
    });
    let visitor = |octet: u8| {
        veredictum_console::submitter::Submitter::Peer(std::net::IpAddr::V4(
            std::net::Ipv4Addr::new(198, 51, 100, octet),
        ))
    };
    let start = |octet: u8| -> Result<RunId, Box<dyn std::error::Error>> {
        let id = slot.allocate_id();
        let dir = veredictum_console::run_job::job_dir(scratch.path(), id);
        std::fs::create_dir_all(&dir)?;
        slot.start(
            id,
            visitor(octet),
            &engine,
            engine_gate::gate_spec(ixit.clone(), port, dir),
            veredictum::party::Sut::operator("metrics-gate", "1"),
        )?;
        Ok(id)
    };

    let driving = start(1)?;
    let waiting = start(2)?;
    let occupancy = slot.occupancy();
    assert_eq!((occupancy.running, occupancy.queued), (1, 1));
    slot.cancel(waiting)?;
    slot.cancel(driving)?;
    assert_eq!(wait_terminal(&slot, driving), Some(JobStatus::Cancelled));
    let finishing = start(3)?;
    assert_eq!(wait_terminal(&slot, finishing), Some(JobStatus::Finished));

    let text = slot.metrics().render(
        slot.occupancy(),
        &veredictum_console::metrics::ContainerRead::default(),
    );
    for line in [
        "veredictum_runs_accepted_total 3\n",
        "veredictum_runs_started_total 2\n",
        "veredictum_runs_ended_total{outcome=\"finished\"} 1\n",
        "veredictum_runs_ended_total{outcome=\"cancelled\"} 1\n",
        "veredictum_runs_ended_total{outcome=\"withdrawn\"} 1\n",
        "veredictum_run_duration_seconds_count 2\n",
        "veredictum_runs_running 0\n",
        "veredictum_runs_queued 0\n",
    ] {
        assert!(text.contains(line), "{line} in {text}");
    }
    Ok(())
}
