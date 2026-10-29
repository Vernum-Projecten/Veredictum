// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! One recorded wire exchange: the request as it was sent and the response
//! as it came back, the way the `identify` read records the System OPTIONS
//! exchange it judged nothing on.
//!
//! NOTE: no openEHR spec governs this — our own design.
//!
//! The record can carry what a server answered verbatim, so it is
//! operator-controlled output, never a log. The one value the recorder
//! withholds is the `authorization` request header, whose credential belongs
//! to the process environment and to nothing on disk.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What replaces a credential-bearing request header value.
pub const REDACTED: &str = "«redacted»";

/// The request header whose value never lands in the artifact.
const CREDENTIAL_HEADER: &str = "authorization";

/// One recorded request, as the instrument sent it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedRequest {
    /// The HTTP method.
    pub method: String,
    /// The absolute request URL, query string included.
    pub url: String,
    /// The request headers, names lower-cased so the ordering is stable.
    pub headers: BTreeMap<String, String>,
    /// The request body, when the request carried one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

/// One recorded response, as the SUT answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedResponse {
    /// The status code.
    pub status: u16,
    /// The response headers, names lower-cased.
    pub headers: BTreeMap<String, String>,
    /// The response body, when the SUT sent one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

/// One recorded exchange: what went out, and what came back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedExchange {
    /// The exchange's ordinal, counting from 1 in the order the instrument
    /// sent them.
    pub seq: u32,
    /// The row the instrument was on when it sent this exchange.
    pub row: u32,
    /// The request.
    pub request: RecordedRequest,
    /// The response.
    pub response: RecordedResponse,
}

/// Lower-cases the header names and withholds the credential header's value.
///
/// The names are lower-cased because a `BTreeMap` orders by the key it is
/// given, and the composed request headers are spelled in mixed case while a
/// SUT's response headers arrive lower-cased — one casing keeps the two sides
/// of the artifact comparable and the ordering stable.
#[must_use]
pub fn recorded_headers(headers: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    headers
        .iter()
        .map(|(name, value)| {
            let name = name.to_ascii_lowercase();
            if name == CREDENTIAL_HEADER {
                (name, REDACTED.to_owned())
            } else {
                (name, value.clone())
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{RecordedExchange, RecordedRequest, RecordedResponse, recorded_headers};

    fn header_map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    /// The credential never lands in the artifact, and every other header
    /// does, verbatim.
    #[test]
    fn the_authorization_header_value_is_withheld() {
        let recorded = recorded_headers(&header_map(&[
            ("Authorization", "Basic dXNlcjpwYXNz"),
            ("Content-Type", "application/json"),
        ]));
        assert_eq!(
            recorded.get("authorization").map(String::as_str),
            Some(super::REDACTED)
        );
        assert!(
            !recorded
                .values()
                .any(|value| value.contains("dXNlcjpwYXNz")),
            "the credential leaked: {recorded:?}"
        );
        assert_eq!(
            recorded.get("content-type").map(String::as_str),
            Some("application/json")
        );
    }

    /// One exchange survives a serialization round trip unchanged.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
    )]
    #[test]
    fn an_exchange_round_trips() -> Result<(), serde_json::Error> {
        let exchange = RecordedExchange {
            seq: 1,
            row: 0,
            request: RecordedRequest {
                method: String::from("OPTIONS"),
                url: String::from("http://sut.invalid/"),
                headers: BTreeMap::new(),
                body: None,
            },
            response: RecordedResponse {
                status: 200,
                headers: BTreeMap::new(),
                body: None,
            },
        };
        let text = serde_json::to_string(&exchange)?;
        let parsed: RecordedExchange = serde_json::from_str(&text)?;
        assert_eq!(parsed, exchange);
        Ok(())
    }
}
