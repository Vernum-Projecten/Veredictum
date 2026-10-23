// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The recording of a run as HAR 1.2, and its replay.
//!
//! Every exchange is one HAR entry, and every workflow run (a row, or a setup
//! workflow it depends on) is one HAR page whose `_veredictum` custom field
//! carries the case id, the row and the inputs the run used, generated ones
//! included. The log's own `_veredictum` field carries the base URL. With
//! those, a recording alone re-judges to the same results.
//! HAR 1.2 is <http://www.softwareishard.com/blog/har-12-spec/>; it allows
//! custom fields whose names start with `_`. No openEHR spec governs the
//! recording — our own design.
//!
//! The value of an `Authorization` header is never recorded; the header is
//! kept with the value [`REDACTED`], so the recording still shows that a
//! credential was sent. Bodies are recorded verbatim, so a recording is
//! operator-controlled output and is never written to a log.

use std::collections::BTreeMap;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::arazzo::rows::Row;
use crate::exec::transport::{RawResponse, Transport};
use crate::vocab::HttpMethod;

/// The value an `Authorization` header is recorded with.
pub const REDACTED: &str = "[not recorded]";

/// A HAR 1.2 document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Har {
    /// The log.
    pub log: Log,
}

/// The HAR `log` object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Log {
    /// Always `1.2`.
    pub version: String,
    /// The instrument that wrote the recording.
    pub creator: Creator,
    /// One page per workflow run.
    pub pages: Vec<Page>,
    /// Every exchange, in the order it happened.
    pub entries: Vec<Entry>,
    /// What a re-judgement needs beyond the exchanges.
    #[serde(rename = "_veredictum")]
    pub veredictum: LogContext,
}

/// The HAR `creator` object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Creator {
    /// The instrument's name.
    pub name: String,
    /// The instrument's version.
    pub version: String,
}

/// The log's custom `_veredictum` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogContext {
    /// The base URL every operation path was appended to.
    pub base_url: String,
}

/// The HAR `page` object: one workflow run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    /// When the run started.
    pub started_date_time: String,
    /// The page id entries refer to.
    pub id: String,
    /// The case id.
    pub title: String,
    /// Unused by this recording; HAR requires the object.
    #[serde(rename = "pageTimings")]
    pub timings: PageTimings,
    /// The run's identity and inputs.
    #[serde(rename = "_veredictum")]
    pub veredictum: PageContext,
}

/// The HAR `pageTimings` object, with both timings unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageTimings {
    /// Always `-1`: no page content is loaded.
    pub on_content_load: i64,
    /// Always `-1`: no page is loaded.
    pub on_load: i64,
}

/// Why a workflow run happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunRole {
    /// A row of the case under test.
    Row,
    /// A workflow the case depends on, run to set it up.
    Setup,
}

/// The page's custom `_veredictum` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageContext {
    /// The case id, `<document>/<workflowId>`.
    pub case: String,
    /// The 0-based row of the case this run belongs to.
    pub row: usize,
    /// Why the run happened.
    pub role: RunRole,
    /// The inputs the run used, generated ones included.
    pub inputs: Row,
}

/// The HAR `entry` object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The page of the workflow run that sent it.
    pub pageref: String,
    /// When the request was sent.
    pub started_date_time: String,
    /// Milliseconds from sending the request to reading the response.
    pub time: f64,
    /// The request as sent.
    pub request: Request,
    /// The response as read.
    pub response: Response,
    /// HAR requires the object; nothing is cached.
    pub cache: Cache,
    /// The timing split; only `wait` is measured.
    pub timings: Timings,
    /// The step that sent it.
    #[serde(rename = "_step")]
    pub step: String,
}

/// The empty HAR `cache` object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cache {}

/// The HAR `timings` object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timings {
    /// Always 0: not measured apart from `wait`.
    pub send: f64,
    /// The whole exchange, in milliseconds.
    pub wait: f64,
    /// Always 0: not measured apart from `wait`.
    pub receive: f64,
}

/// One HAR name and value pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameValue {
    /// The name.
    pub name: String,
    /// The value.
    pub value: String,
}

/// The HAR `request` object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// The method.
    pub method: String,
    /// The full URL.
    pub url: String,
    /// Empty: the transport does not report the protocol version.
    pub http_version: String,
    /// No cookies are sent.
    pub cookies: Vec<NameValue>,
    /// The headers sent, `Authorization` redacted.
    pub headers: Vec<NameValue>,
    /// The query parameters sent.
    pub query_string: Vec<NameValue>,
    /// The body sent, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_data: Option<PostData>,
    /// Always `-1`: not measured.
    pub headers_size: i64,
    /// The body's length in bytes, 0 without one.
    pub body_size: i64,
}

/// The HAR `postData` object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostData {
    /// The body's media type, empty when none was sent.
    pub mime_type: String,
    /// The body as sent.
    pub text: String,
}

/// The HAR `response` object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    /// The status code; 0 when no response arrived.
    pub status: u16,
    /// The status code's reason phrase.
    pub status_text: String,
    /// Empty: the transport does not report the protocol version.
    pub http_version: String,
    /// Cookies are not read.
    pub cookies: Vec<NameValue>,
    /// The headers received.
    pub headers: Vec<NameValue>,
    /// The body received.
    pub content: Content,
    /// Always empty: redirects are not followed apart.
    #[serde(rename = "redirectURL")]
    pub redirect_url: String,
    /// Always `-1`: not measured.
    pub headers_size: i64,
    /// Always `-1`: the transfer size is not measured.
    pub body_size: i64,
    /// The transport failure, when no response arrived.
    #[serde(rename = "_error", default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The HAR `content` object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    /// The decoded body's length in bytes.
    pub size: i64,
    /// The `Content-Type` received, empty when none.
    pub mime_type: String,
    /// The decoded body.
    pub text: String,
}

/// Why a recording cannot be read or replayed.
#[derive(Debug, thiserror::Error)]
pub enum HarError {
    /// The file does not read.
    #[error("the recording `{path}` cannot be read: {reason}")]
    Read {
        /// The file.
        path: std::path::PathBuf,
        /// The verbatim failure.
        reason: String,
    },
    /// The text is not a HAR document this crate wrote.
    #[error("the recording is not a Veredictum HAR 1.2 document: {0}")]
    Parse(String),
}

fn length(text: &str) -> i64 {
    i64::try_from(text.len()).unwrap_or(i64::MAX)
}

impl Har {
    /// Starts an empty recording against `base_url`.
    #[must_use]
    pub fn new(base_url: &str) -> Self {
        Self {
            log: Log {
                version: "1.2".to_owned(),
                creator: Creator {
                    name: "veredictum".to_owned(),
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                },
                pages: Vec::new(),
                entries: Vec::new(),
                veredictum: LogContext {
                    base_url: base_url.to_owned(),
                },
            },
        }
    }

    /// Opens the page of one workflow run and returns its id.
    pub fn begin_page(&mut self, context: PageContext) -> String {
        let id = format!("run_{}", self.log.pages.len());
        self.log.pages.push(Page {
            started_date_time: jiff::Timestamp::now().to_string(),
            id: id.clone(),
            title: context.case.clone(),
            timings: PageTimings {
                on_content_load: -1,
                on_load: -1,
            },
            veredictum: context,
        });
        id
    }

    /// Records one exchange and returns its entry index.
    pub fn record(&mut self, exchange: &Recorded<'_>) -> usize {
        let started = jiff::Timestamp::now().to_string();
        let headers = exchange
            .request_headers
            .iter()
            .map(|(name, value)| NameValue {
                name: name.clone(),
                value: if name.eq_ignore_ascii_case("authorization") {
                    REDACTED.to_owned()
                } else {
                    value.clone()
                },
            })
            .collect();
        let query_string = exchange
            .query
            .iter()
            .map(|(name, value)| NameValue {
                name: name.clone(),
                value: value.clone(),
            })
            .collect();
        let post_data = exchange.body.map(|text| PostData {
            mime_type: exchange
                .request_headers
                .get("content-type")
                .cloned()
                .unwrap_or_default(),
            text: text.to_owned(),
        });
        let response = match exchange.outcome {
            Ok(raw) => Response {
                status: raw.status.as_u16(),
                status_text: raw.status.canonical_reason().unwrap_or_default().to_owned(),
                http_version: String::new(),
                cookies: Vec::new(),
                headers: raw
                    .headers
                    .iter()
                    .map(|(name, value)| NameValue {
                        name: name.clone(),
                        value: value.clone(),
                    })
                    .collect(),
                content: Content {
                    size: length(&raw.text),
                    mime_type: raw
                        .headers
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                        .map(|(_, value)| value.clone())
                        .unwrap_or_default(),
                    text: raw.text.clone(),
                },
                redirect_url: String::new(),
                headers_size: -1,
                body_size: -1,
                error: None,
            },
            Err(reason) => Response {
                status: 0,
                status_text: String::new(),
                http_version: String::new(),
                cookies: Vec::new(),
                headers: Vec::new(),
                content: Content {
                    size: 0,
                    mime_type: String::new(),
                    text: String::new(),
                },
                redirect_url: String::new(),
                headers_size: -1,
                body_size: -1,
                error: Some(reason.to_owned()),
            },
        };
        self.log.entries.push(Entry {
            pageref: exchange.page.to_owned(),
            started_date_time: started,
            time: exchange.millis,
            request: Request {
                method: exchange.method.to_owned(),
                url: exchange.url.to_owned(),
                http_version: String::new(),
                cookies: Vec::new(),
                headers,
                query_string,
                body_size: exchange.body.map_or(0, length),
                post_data,
                headers_size: -1,
            },
            response,
            cache: Cache {},
            timings: Timings {
                send: 0.0,
                wait: exchange.millis,
                receive: 0.0,
            },
            step: exchange.step.to_owned(),
        });
        self.log.entries.len().saturating_sub(1)
    }

    /// The recording as pretty-printed JSON with a trailing newline.
    ///
    /// # Errors
    /// [`HarError::Parse`] if serialization fails.
    pub fn to_text(&self) -> Result<String, HarError> {
        let mut text =
            serde_json::to_string_pretty(self).map_err(|e| HarError::Parse(e.to_string()))?;
        text.push('\n');
        Ok(text)
    }

    /// Parses a recording.
    ///
    /// # Errors
    /// [`HarError::Parse`] when the text is not a recording this crate wrote.
    pub fn from_text(text: &str) -> Result<Self, HarError> {
        serde_json::from_str(text).map_err(|e| HarError::Parse(e.to_string()))
    }

    /// Reads a recording from a file.
    ///
    /// # Errors
    /// [`HarError`] when the file does not read or parse.
    pub fn read(path: &std::path::Path) -> Result<Self, HarError> {
        let text = std::fs::read_to_string(path).map_err(|e| HarError::Read {
            path: path.to_owned(),
            reason: e.to_string(),
        })?;
        Self::from_text(&text)
    }
}

/// One exchange as the evaluator hands it to the recording.
#[derive(Debug)]
pub struct Recorded<'a> {
    /// The page of the run that sent it.
    pub page: &'a str,
    /// The step that sent it.
    pub step: &'a str,
    /// The method, upper case.
    pub method: &'a str,
    /// The full URL.
    pub url: &'a str,
    /// The headers sent, names lower case.
    pub request_headers: &'a BTreeMap<String, String>,
    /// The query parameters sent.
    pub query: &'a BTreeMap<String, String>,
    /// The body as sent, if any.
    pub body: Option<&'a str>,
    /// The response, or the transport failure.
    pub outcome: Result<&'a RawResponse, &'a str>,
    /// How long the exchange took.
    pub millis: f64,
}

/// The text a request body is sent as: JSON text for a document, the string
/// itself for a raw body.
#[must_use]
pub fn body_text(body: &Value, is_json: bool) -> String {
    match body {
        Value::String(text) if !is_json => text.clone(),
        other => other.to_string(),
    }
}

/// Answers each request from the next recorded entry.
///
/// Matching is positional, and the method and the full URL must equal the
/// recorded ones: a re-judgement reuses every recorded input, so a request
/// that differs means the replay diverged, and it is refused rather than
/// judged.
#[derive(Debug)]
pub struct HarTransport<'a> {
    entries: &'a [Entry],
    cursor: usize,
}

impl<'a> HarTransport<'a> {
    /// Replays the entries of `har`, oldest first.
    #[must_use]
    pub fn new(har: &'a Har) -> Self {
        Self {
            entries: &har.log.entries,
            cursor: 0,
        }
    }

    /// How many recorded entries the replay has not reached.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.entries.len().saturating_sub(self.cursor)
    }
}

impl Transport for HarTransport<'_> {
    fn exchange(
        &mut self,
        method: HttpMethod,
        url: &str,
        _headers: &BTreeMap<String, String>,
        _body: Option<&Value>,
        _body_is_json: bool,
    ) -> Result<RawResponse, String> {
        let Some(entry) = self.entries.get(self.cursor) else {
            return Err(format!(
                "the recording holds {} entries and the replay asked for one more",
                self.entries.len()
            ));
        };
        let index = self.cursor;
        self.cursor = self.cursor.saturating_add(1);
        let sent = format!("{method:?}").to_uppercase();
        if entry.request.method != sent || entry.request.url != url {
            return Err(format!(
                "entry {index} was recorded as {} {} and the replay sent {sent} {url}: the replay \
                 diverged from the recording",
                entry.request.method, entry.request.url
            ));
        }
        if let Some(error) = &entry.response.error {
            return Err(error.clone());
        }
        let status = StatusCode::from_u16(entry.response.status).map_err(|e| {
            format!(
                "entry {index} records status {}: {e}",
                entry.response.status
            )
        })?;
        Ok(RawResponse {
            status,
            headers: entry
                .response
                .headers
                .iter()
                .map(|h| (h.name.clone(), h.value.clone()))
                .collect(),
            text: entry.response.content.text.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(har: &mut Har, outcome: Result<&RawResponse, &str>) {
        let headers = BTreeMap::from([
            ("authorization".to_owned(), "Basic c2VjcmV0".to_owned()),
            ("content-type".to_owned(), "application/json".to_owned()),
        ]);
        let page = har.begin_page(PageContext {
            case: "ehr/w".to_owned(),
            row: 0,
            role: RunRole::Row,
            inputs: Row::new(),
        });
        har.record(&Recorded {
            page: &page,
            step: "create",
            method: "POST",
            url: "http://sut/ehr",
            request_headers: &headers,
            query: &BTreeMap::new(),
            body: Some("{}"),
            outcome,
            millis: 1.5,
        });
    }

    #[test]
    fn the_credential_is_never_recorded() {
        let mut har = Har::new("http://sut");
        let raw = RawResponse {
            status: StatusCode::CREATED,
            headers: BTreeMap::new(),
            text: String::new(),
        };
        record(&mut har, Ok(&raw));
        let text = har.to_text().unwrap();
        assert!(!text.contains("c2VjcmV0"), "{text}");
        assert!(text.contains(REDACTED));
        assert!(text.contains("\"version\": \"1.2\""));
    }

    #[test]
    fn a_replay_answers_in_order_and_refuses_a_divergence() {
        let mut har = Har::new("http://sut");
        let raw = RawResponse {
            status: StatusCode::CONFLICT,
            headers: BTreeMap::from([("etag".to_owned(), "\"1\"".to_owned())]),
            text: "{\"a\":1}".to_owned(),
        };
        record(&mut har, Ok(&raw));
        record(&mut har, Err("transport: connection refused"));
        let har = Har::from_text(&har.to_text().unwrap()).unwrap();
        let mut replay = HarTransport::new(&har);
        let answer = replay
            .exchange(
                HttpMethod::Post,
                "http://sut/ehr",
                &BTreeMap::new(),
                None,
                true,
            )
            .unwrap();
        assert_eq!(answer.status, StatusCode::CONFLICT);
        assert_eq!(answer.text, "{\"a\":1}");
        assert_eq!(
            replay
                .exchange(
                    HttpMethod::Post,
                    "http://sut/ehr",
                    &BTreeMap::new(),
                    None,
                    true
                )
                .err(),
            Some("transport: connection refused".to_owned())
        );
        let mut diverged = HarTransport::new(&har);
        assert!(
            diverged
                .exchange(
                    HttpMethod::Put,
                    "http://sut/ehr",
                    &BTreeMap::new(),
                    None,
                    true
                )
                .is_err()
        );
        assert_eq!(diverged.remaining(), 1);
    }
}
