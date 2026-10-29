// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! A fake server under test for the GitHub Action's CI job (#770).
//!
//! It answers OPTIONS on any path with 200, an `Allow` header and an empty
//! JSON object, which passes every System API workflow, and every other
//! request with 404, which fails the EHR workflows. That gives the job a run
//! that exits 0 and a run that exits 1 from one server. It is not an openEHR
//! server and judges nothing. No openEHR spec governs this — our own design.
//!
//! Run it as `cargo run --example stub_sut -- <port>`; it listens on
//! `127.0.0.1:<port>` until it is stopped.

use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port: u16 = std::env::args()
        .nth(1)
        .ok_or("usage: stub_sut <port>")?
        .parse()?;
    let listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let server = MockServer::builder().listener(listener).start().await;
        // NOTE: wiremock answers a request no mock matches with 404 (docs.rs, wiremock 0.6, `MockServer`).
        Mock::given(method("OPTIONS"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Allow", "GET, POST, PUT, DELETE, OPTIONS")
                    .set_body_raw("{}", "application/json"),
            )
            .mount(&server)
            .await;
        std::future::pending::<()>().await;
    });
    Ok(())
}
