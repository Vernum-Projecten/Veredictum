// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The blocking client the posture canaries ride.
//!
//! The canaries need three views of one target: the run's own credential,
//! no credential at all, and a client that leaves `Content-Encoding` on the
//! response. They send a handful of requests per bracket, outside every
//! measured window, so a blocking client on a blocking thread serves them;
//! the measured traffic rides the run's own asynchronous client.
//!
//! No openEHR specification governs this: our own design.

use std::io::Read as _;
use std::time::Duration;

use thiserror::Error;

/// Per-request timeout of a canary exchange.
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);

/// A canary client that could not be built or could not send.
#[derive(Debug, Error)]
pub enum CanaryError {
    /// The HTTP client could not be built.
    #[error("canary http client: {source}")]
    Client {
        /// The underlying failure.
        #[source]
        source: reqwest::Error,
    },
    /// A request never reached a response.
    #[error("{exchange}: {source}")]
    Transport {
        /// The exchange that failed.
        exchange: String,
        /// The underlying failure.
        #[source]
        source: reqwest::Error,
    },
}

/// The `Prefer: return=…` preference a write states.
///
/// The three tokens are the ones ITS-REST `docs/overview/Requests_and_responses`
/// §"Prefer minimal, identifier or full representation response" defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferReturn {
    /// No `Prefer` header.
    Unstated,
    /// `Prefer: return=minimal`.
    Minimal,
    /// `Prefer: return=identifier`.
    Identifier,
}

impl PreferReturn {
    /// Every preference.
    pub const ALL: &[PreferReturn] = &[
        PreferReturn::Unstated,
        PreferReturn::Minimal,
        PreferReturn::Identifier,
    ];

    /// The `Prefer` header value, or `None` for the unstated preference.
    #[must_use]
    pub const fn header_value(self) -> Option<&'static str> {
        match self {
            PreferReturn::Unstated => None,
            PreferReturn::Minimal => Some("return=minimal"),
            PreferReturn::Identifier => Some("return=identifier"),
        }
    }
}

/// One canary response, as much of it as a canary reads.
#[derive(Debug)]
pub struct CanaryReply {
    /// The status the server answered with.
    pub status: reqwest::StatusCode,
    /// The `ETag` header, when one was sent.
    pub etag: Option<String>,
    /// The `Location` header, when one was sent.
    pub location: Option<String>,
    /// The `Content-Encoding` header, when one was sent; only meaningful on a
    /// client built by [`CanaryClient::without_decompression`], because a
    /// decompressing client removes the header as it unwraps the body.
    pub content_encoding: Option<String>,
    /// The response body.
    pub body: Vec<u8>,
}

/// The blocking client of one canary view.
#[derive(Clone)]
pub struct CanaryClient {
    client: reqwest::blocking::Client,
    base_url: String,
    authorization: Option<String>,
    headers: Vec<(String, String)>,
}

impl std::fmt::Debug for CanaryClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CanaryClient")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

fn builder() -> reqwest::blocking::ClientBuilder {
    reqwest::blocking::Client::builder()
        .timeout(CLIENT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
}

impl CanaryClient {
    /// Builds the client that presents the run's own `Authorization` value
    /// and the headers its IXIT instance adds.
    ///
    /// # Errors
    /// [`CanaryError::Client`] when the HTTP client cannot be built.
    pub fn new(
        base_url: &str,
        authorization: Option<String>,
        headers: Vec<(String, String)>,
    ) -> Result<Self, CanaryError> {
        Ok(Self {
            client: builder()
                .build()
                .map_err(|source| CanaryError::Client { source })?,
            base_url: base_url.trim_end_matches('/').to_owned(),
            authorization,
            headers,
        })
    }

    /// The same target and credential over a client that never decompresses.
    ///
    /// A decompressing client removes `Content-Encoding` from the response as
    /// it unwraps the body (reqwest 0.13 `ClientBuilder::gzip`, whose contract
    /// is that a `gzip` value and `Content-Length` "are removed from the
    /// headers' set"), which is the header the compression canary reads.
    ///
    /// # Errors
    /// [`CanaryError::Client`] when the HTTP client cannot be built.
    pub fn without_decompression(&self) -> Result<Self, CanaryError> {
        Ok(Self {
            client: builder()
                .gzip(false)
                .brotli(false)
                .build()
                .map_err(|source| CanaryError::Client { source })?,
            base_url: self.base_url.clone(),
            authorization: self.authorization.clone(),
            headers: self.headers.clone(),
        })
    }

    /// The same target over a client that presents no credential at all,
    /// which is the only way to see whether a server enforces the mode a run
    /// declares.
    ///
    /// # Errors
    /// [`CanaryError::Client`] when the HTTP client cannot be built.
    pub fn without_credential(&self) -> Result<Self, CanaryError> {
        let mut anonymous = self.without_decompression()?;
        anonymous.authorization = None;
        Ok(anonymous)
    }

    /// The base URL this client drives, with any userinfo removed.
    #[must_use]
    pub fn recorded_base_url(&self) -> String {
        strip_userinfo(&self.base_url)
    }

    /// Issues one request against an absolute path under the base URL.
    ///
    /// # Errors
    /// [`CanaryError::Transport`] when the request never reached a response.
    pub fn send(
        &self,
        exchange: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<(&'static str, Vec<u8>)>,
        prefer: PreferReturn,
    ) -> Result<CanaryReply, CanaryError> {
        self.send_with_headers(exchange, method, path, body, prefer, &[])
    }

    /// Issues one request carrying additional request headers.
    ///
    /// # Errors
    /// [`CanaryError::Transport`] when the request never reached a response.
    pub fn send_with_headers(
        &self,
        exchange: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<(&'static str, Vec<u8>)>,
        prefer: PreferReturn,
        extra: &[(&'static str, &'static str)],
    ) -> Result<CanaryReply, CanaryError> {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base_url))
            .header("Accept", "application/json");
        for (name, value) in &self.headers {
            request = request.header(name.as_str(), value.as_str());
        }
        if let Some(authorization) = &self.authorization {
            request = request.header("Authorization", authorization);
        }
        if let Some(preference) = prefer.header_value() {
            request = request.header("Prefer", preference);
        }
        if let Some((media_type, bytes)) = body {
            request = request.header("Content-Type", media_type).body(bytes);
        }
        for (name, value) in extra {
            request = request.header(*name, *value);
        }
        let response = request.send().map_err(|source| CanaryError::Transport {
            exchange: exchange.to_owned(),
            source,
        })?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let status = response.status();
        let etag = header("etag");
        let location = header("location");
        let content_encoding = header("content-encoding");
        let mut sink = Vec::new();
        let mut reader = response;
        // NOTE: no openEHR spec governs this — our own design: a body that fails to drain is read as what arrived.
        let _drained = reader.read_to_end(&mut sink);
        Ok(CanaryReply {
            status,
            etag,
            location,
            content_encoding,
            body: sink,
        })
    }
}

/// Removes any `user:password@` from a URL's authority, so a recorded target
/// never republishes a credential typed into a base URL.
#[must_use]
pub fn strip_userinfo(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let (authority, path) = match rest.find('/') {
        Some(cut) => rest.split_at(cut),
        None => (rest, ""),
    };
    let Some((_userinfo, host)) = authority.rsplit_once('@') else {
        return url.to_owned();
    };
    format!("{scheme}://{host}{path}")
}

/// `W/"uid"` or `"uid"` becomes `uid` (ITS-REST overview §Concurrency
/// control describes the weak-validator form the create answers with).
#[must_use]
pub fn strip_weak_quotes(etag: &str) -> String {
    etag.trim_start_matches("W/").trim_matches('"').to_owned()
}

/// The last path segment of a `Location` header, which is the identifier a
/// `return=minimal` create discloses.
#[must_use]
pub fn location_last_segment(location: &str) -> Option<String> {
    location
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
}

/// The identifier a create discloses, whichever way it discloses one.
///
/// A `return=identifier` reply carries a body of one `uid` attribute
/// (ITS-REST `docs/overview/Requests_and_responses` §"Prefer only
/// identifier"), while `return=minimal` carries the identifying headers
/// instead. The body is read first, because it is the only source the
/// identifier preference guarantees.
#[must_use]
#[expect(
    clippy::disallowed_types,
    reason = "the approved wire-body seam: an identifier reply is a JSON document read for one attribute"
)]
pub fn created_identifier(reply: &CanaryReply) -> Option<String> {
    // NOTE: a body that is not JSON legitimately discloses no identifier; the headers are read next.
    let from_body = serde_json::from_slice::<serde_json::Value>(&reply.body)
        .ok()
        .and_then(|document| {
            document.pointer("/uid").and_then(|uid| {
                uid.as_str().map(str::to_owned).or_else(|| {
                    uid.pointer("/value")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
            })
        })
        .filter(|uid| !uid.is_empty());
    from_body
        .or_else(|| reply.etag.as_deref().map(strip_weak_quotes))
        .or_else(|| reply.location.as_deref().and_then(location_last_segment))
        .filter(|identifier| !identifier.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A credential typed into the base URL never reaches the record.
    #[test]
    fn userinfo_is_stripped_from_a_recorded_target() {
        assert_eq!(
            strip_userinfo("https://alice:s3cret@cdr.example/openehr/v1"),
            "https://cdr.example/openehr/v1"
        );
        assert_eq!(
            strip_userinfo("https://alice@cdr.example"),
            "https://cdr.example"
        );
        assert_eq!(
            strip_userinfo("http://127.0.0.1:8080/rest/openehr/v1"),
            "http://127.0.0.1:8080/rest/openehr/v1"
        );
        assert_eq!(strip_userinfo("not-a-url"), "not-a-url");
    }

    /// The two identifier captures follow the wire forms.
    #[test]
    fn identifier_captures_match_the_wire_forms() {
        assert_eq!(strip_weak_quotes("W/\"abc::sys::1\""), "abc::sys::1");
        assert_eq!(strip_weak_quotes("\"abc\""), "abc");
        assert_eq!(
            location_last_segment("http://sut/ehr/42").as_deref(),
            Some("42")
        );
        assert_eq!(location_last_segment("http://sut/ehr/"), None);
    }

    /// Each preference sends the header value the specification names, and
    /// the unstated preference sends no header at all.
    #[test]
    fn every_preference_sends_its_own_header_value() {
        assert_eq!(PreferReturn::Unstated.header_value(), None);
        assert_eq!(PreferReturn::Minimal.header_value(), Some("return=minimal"));
        assert_eq!(
            PreferReturn::Identifier.header_value(),
            Some("return=identifier")
        );
        assert_eq!(PreferReturn::ALL.len(), 3);
    }

    /// A `return=identifier` body wins over the headers, and a reply that
    /// discloses nothing yields `None` rather than an empty identifier.
    #[test]
    fn a_created_identifier_reads_the_body_before_the_headers() {
        let reply = |body: &str, etag: Option<&str>, location: Option<&str>| CanaryReply {
            status: reqwest::StatusCode::CREATED,
            etag: etag.map(str::to_owned),
            location: location.map(str::to_owned),
            content_encoding: None,
            body: body.as_bytes().to_vec(),
        };
        assert_eq!(
            created_identifier(&reply(
                r#"{"uid":"body::sys::1"}"#,
                Some("\"etag::sys::1\""),
                Some("http://sut/ehr/loc")
            ))
            .as_deref(),
            Some("body::sys::1")
        );
        assert_eq!(
            created_identifier(&reply(r#"{"uid":{"value":"nested::sys::1"}}"#, None, None))
                .as_deref(),
            Some("nested::sys::1")
        );
        assert_eq!(
            created_identifier(&reply("", Some("W/\"etag::sys::1\""), None)).as_deref(),
            Some("etag::sys::1")
        );
        assert_eq!(
            created_identifier(&reply("", None, Some("http://sut/ehr/EHR-9"))).as_deref(),
            Some("EHR-9")
        );
        assert_eq!(created_identifier(&reply("", None, None)), None);
    }
}
