// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The one place bytes leave the process, behind one trait.
//!
//! The Arazzo evaluator composes every request and judges every criterion,
//! and none of that touches a socket: [`Transport`] does, and it is the only
//! seam between the evaluator and the wire. A HAR recording answers the same
//! trait when a run is re-judged, so a re-judgement reads the evidence
//! through the same composition and the same judges the live run used.

#![expect(
    clippy::disallowed_types,
    reason = "a request body is whatever JSON the catalogue sends, which has no typed model here"
)]

use std::collections::BTreeMap;

use reqwest::StatusCode;
use serde_json::Value;

use crate::vocab::HttpMethod;

/// How long a live exchange may take before the driver calls it a transport
/// failure.
const TIMEOUT_SECS: u64 = 30;

/// One response as a transport delivered it, before the driver reads it.
#[derive(Debug, Clone)]
pub struct RawResponse {
    /// The status code the answer carried.
    pub status: StatusCode,
    /// The response headers, names as the answer spelled them.
    pub headers: BTreeMap<String, String>,
    /// The response body as bytes-turned-text; empty when there was none.
    pub text: String,
}

/// Where a composed request is sent, and where its answer comes from.
///
/// # Errors
/// Implementations return the verbatim reason an exchange could not be
/// performed. The driver turns that into a transport observation, never into
/// a pass.
pub trait Transport {
    /// Perform one exchange.
    ///
    /// # Errors
    /// The verbatim reason the exchange could not be performed.
    fn exchange(
        &mut self,
        method: HttpMethod,
        url: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&Value>,
        body_is_json: bool,
    ) -> Result<RawResponse, String>;
}

/// The live transport: one blocking HTTP client against the SUT.
#[derive(Debug)]
pub struct HttpTransport {
    client: reqwest::blocking::Client,
}

impl HttpTransport {
    /// Builds the client the driver sends through.
    ///
    /// # Errors
    /// The client builder's verbatim failure.
    pub fn new() -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .build()
            .map_err(|e| format!("http client: {e}"))?;
        Ok(Self { client })
    }
}

impl Transport for HttpTransport {
    fn exchange(
        &mut self,
        method: HttpMethod,
        url: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&Value>,
        body_is_json: bool,
    ) -> Result<RawResponse, String> {
        let mut request = self.client.request(reqwest_method(method), url);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        if let Some(payload) = body {
            request = if body_is_json {
                request.body(serde_json::to_vec(payload).map_err(|e| e.to_string())?)
            } else {
                match payload {
                    Value::String(text) => request.body(text.clone()),
                    other => request.body(serde_json::to_vec(other).map_err(|e| e.to_string())?),
                }
            };
        }
        let response = request.send().map_err(|e| format!("transport: {e}"))?;
        let status = response.status();
        let mut response_headers = BTreeMap::new();
        for (name, value) in response.headers() {
            if let Ok(v) = value.to_str() {
                response_headers.insert(name.as_str().to_owned(), v.to_owned());
            }
        }
        let text = response.text().map_err(|e| format!("transport: {e}"))?;
        Ok(RawResponse {
            status,
            headers: response_headers,
            text,
        })
    }
}

/// The method as the HTTP client spells it.
fn reqwest_method(method: HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Delete => reqwest::Method::DELETE,
        HttpMethod::Head => reqwest::Method::HEAD,
        HttpMethod::Options => reqwest::Method::OPTIONS,
    }
}
