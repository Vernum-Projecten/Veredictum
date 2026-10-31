// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The two certificate routes over real HTTP, through the router the server
//! binary builds: the published JWK Set is the committed one byte for byte,
//! and a run with no certificate downloads none.

/// A bound console in trial mode, and the scratch tree its state points at.
struct Bound {
    origin: String,
    out: std::path::PathBuf,
    _scratch: assert_fs::TempDir,
}

async fn bind() -> Result<Bound, Box<dyn std::error::Error>> {
    let scratch = assert_fs::TempDir::new()?;
    let state = crate::support::state(
        scratch.path(),
        crate::support::absent("no catalogue: this gate drives the certificate routes"),
    );
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
        out: scratch.path().to_path_buf(),
        _scratch: scratch,
    })
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn the_served_key_set_is_the_committed_copy() -> Result<(), Box<dyn std::error::Error>> {
    let bound = bind().await?;
    let response = reqwest::get(format!(
        "{}{}",
        bound.origin,
        veredictum_console::certificates::JWKS_ROUTE
    ))
    .await?;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(reqwest::header::HeaderValue::as_bytes),
        Some(&b"application/jwk-set+json"[..])
    );
    let served = response.bytes().await?;
    let committed = std::fs::read(crate::support::repo_root().join("deploy/hosted/jwks.json"))?;
    assert_eq!(
        served.as_ref(),
        committed.as_slice(),
        "the console serves the committed set byte for byte"
    );
    veredictum::certificate::Jwks::parse(std::str::from_utf8(&served)?)?;
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn a_run_without_a_certificate_downloads_none() -> Result<(), Box<dyn std::error::Error>> {
    let bound = bind().await?;
    let run = veredictum_console::run_job::RunId::mint();
    std::fs::create_dir_all(veredictum_console::run_job::job_dir(&bound.out, run))?;
    for path in [
        veredictum_console::certificates::download_href(&run.to_string()),
        veredictum_console::certificates::download_href("..%2F..%2Fetc"),
        veredictum_console::certificates::download_href("not-a-run"),
    ] {
        let response = reqwest::get(format!("{}{path}", bound.origin)).await?;
        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND, "{path}");
    }
    Ok(())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[tokio::test(flavor = "multi_thread")]
async fn a_written_certificate_downloads_as_a_jws() -> Result<(), Box<dyn std::error::Error>> {
    let bound = bind().await?;
    let run = veredictum_console::run_job::RunId::mint();
    let dir = veredictum_console::run_job::job_dir(&bound.out, run);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join(veredictum::certificate::CERTIFICATE_FILE),
        b"a.b.c\n",
    )?;
    let response = reqwest::get(format!(
        "{}{}",
        bound.origin,
        veredictum_console::certificates::download_href(&run.to_string())
    ))
    .await?;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(reqwest::header::HeaderValue::as_bytes),
        Some(&b"application/jose"[..])
    );
    assert_eq!(response.bytes().await?.as_ref(), b"a.b.c\n");
    Ok(())
}
