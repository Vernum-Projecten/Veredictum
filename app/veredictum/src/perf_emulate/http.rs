// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The asynchronous wire client the measured run and its seeding send
//! through.
//!
//! The principal is the IXIT instance's, resolved once by
//! [`PerfClient`]: the same `Authorization` header (a fixed credential or
//! the standing minted SMART grant) and the same instance headers, so a
//! request goes on the wire one way whichever instrument sends it. Every
//! request is a [`WireRequest`] built by [`crate::workload::wire`].

use std::time::Duration;

use reqwest::StatusCode;

use crate::perf_gen::{ErrorClass, FailedExchange, Outcome};
use crate::perf_run::client::PerfClient;
use crate::workload::wire::WireRequest;

/// The per-request timeout: a minute, far past every latency budget, so an
/// answer slower than it is a hard violation either way.
pub const REQUEST_TIMEOUT: Duration = Duration::from_mins(1);

/// One answer: its status, the two identifying headers and its body.
#[derive(Debug, Clone)]
pub struct Reply {
    /// The status.
    pub status: StatusCode,
    /// The `ETag`, its weak marker and quotes removed.
    pub etag: Option<String>,
    /// The `Location`.
    pub location: Option<String>,
    /// The body as text.
    pub body: String,
}

impl Reply {
    /// The last path segment of the `Location`, the id a minimal create
    /// names its object by.
    #[must_use]
    pub fn location_id(&self) -> Option<String> {
        self.location
            .as_deref()
            .and_then(|l| l.rsplit('/').next())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    }
}

/// Why an exchange produced no answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Fault {
    /// No answer within [`REQUEST_TIMEOUT`].
    #[error("timeout")]
    Timeout,
    /// The connection failed, or the request could not be formed.
    #[error("transport: {0}")]
    Transport(String),
}

impl Fault {
    /// The error class the exchange is counted under.
    #[must_use]
    pub fn class(&self) -> ErrorClass {
        match self {
            Fault::Timeout => ErrorClass::Timeout,
            Fault::Transport(_) => ErrorClass::Transport,
        }
    }
}

/// The asynchronous client of one principal.
#[derive(Debug, Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    principal: PerfClient,
}

impl HttpClient {
    /// Builds the client over the principal's credential and base URL.
    ///
    /// # Errors
    /// The client builder's message.
    pub fn new(principal: PerfClient) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .pool_max_idle_per_host(1_024)
            .build()
            .map_err(|e| format!("http client: {e}"))?;
        Ok(Self { client, principal })
    }

    /// The base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        self.principal.base_url()
    }

    /// The principal the client presents.
    pub(crate) fn principal(&self) -> &PerfClient {
        &self.principal
    }

    /// Sends `request` and reads its answer to the end.
    ///
    /// # Errors
    /// [`Fault`] when no answer arrives.
    pub async fn send(&self, request: &WireRequest) -> Result<Reply, Fault> {
        let method = request
            .method()
            .map_err(|e| Fault::Transport(e.to_string()))?;
        let url = request
            .url(self.principal.base_url())
            .map_err(|e| Fault::Transport(e.to_string()))?;
        let mut builder = self.client.request(method, url);
        for (name, value) in &request.headers {
            builder = builder.header(*name, value);
        }
        if let Some(auth) = self.principal.authorization().map_err(Fault::Transport)? {
            builder = builder.header("Authorization", auth);
        }
        for (name, value) in self.principal.extra_headers() {
            builder = builder.header(name, value);
        }
        if let Some(body) = &request.body {
            builder = builder
                .header("Content-Type", body.content_type)
                .body(body.bytes.clone());
        }
        let response = builder.send().await.map_err(|e| classify(&e))?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let status = response.status();
        let etag = header("etag").map(|e| e.trim_start_matches("W/").trim_matches('"').to_owned());
        let location = header("location");
        let body = response.text().await.map_err(|e| classify(&e))?;
        Ok(Reply {
            status,
            etag,
            location,
            body,
        })
    }

    /// Sends `request` and judges the answer against `expected`.
    ///
    /// A `429` counts as [`ErrorClass::RateLimited`] whatever the operation
    /// expects; a status outside `expected` is a failure of its class.
    pub async fn judged(
        &self,
        request: &WireRequest,
        expected: &[StatusCode],
    ) -> (Outcome, Option<Reply>) {
        match self.send(request).await {
            Ok(reply) if expected.contains(&reply.status) => (Outcome::Success, Some(reply)),
            Ok(reply) => {
                let outcome = Outcome::Failure {
                    class: ErrorClass::for_status(reply.status),
                    exchange: Some(FailedExchange::new(
                        Some(reply.status),
                        &request_line(request),
                        &reply.body,
                    )),
                };
                (outcome, Some(reply))
            }
            Err(fault) => (
                Outcome::Failure {
                    class: fault.class(),
                    exchange: Some(FailedExchange::new(
                        None,
                        &request_line(request),
                        &fault.to_string(),
                    )),
                },
                None,
            ),
        }
    }
}

/// `METHOD path`, the request line a failed sample keeps.
#[must_use]
pub fn request_line(request: &WireRequest) -> String {
    format!("{} {}", request.operation.method, request.path)
}

fn classify(error: &reqwest::Error) -> Fault {
    if error.is_timeout() {
        Fault::Timeout
    } else {
        Fault::Transport(error.to_string())
    }
}
