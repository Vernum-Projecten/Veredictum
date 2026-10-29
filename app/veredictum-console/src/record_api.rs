// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The record seam for S6 Results (#67).
//!
//! Everything renders from the finished job's own results document and HAR
//! recording, read through the published lib. A red row's explanation is the
//! one the engine wrote; nothing here re-judges a row.

use serde::{Deserialize, Serialize};

/// One results row for the table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultRow {
    /// The case id, `<document>/<workflowId>`.
    pub case: String,
    /// The 0-based row of the case.
    pub row: u32,
    /// The status token (`passed` / `failed` / `errored` / `skipped`).
    pub status: String,
    /// What a red row received, or why a skipped row was not run.
    pub reason: Option<String>,
}

/// The results screen: the base URL, the tallies, and the rows red-first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultsScreen {
    /// The base URL the run drove.
    pub base_url: String,
    /// passed / failed / errored / skipped counts.
    pub tallies: (u64, u64, u64, u64),
    /// Every row, failures and errors first.
    pub rows: Vec<ResultRow>,
    /// What happened about the run's certificate.
    pub certificate: CertificateNotice,
}

/// What the results screen says about the run's certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertificateNotice {
    /// Trial mode: no certificate was created.
    Trial,
    /// Issued; the field is its download path.
    Issued(String),
    /// Issuance is on and this run got none; the reason, verbatim.
    NotIssued(String),
}

/// One recorded exchange, rendered as the two panes the drawer shows.
///
/// The bodies are the wire's own bytes. The engine never records a
/// credential, so nothing here can carry one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeView {
    /// The exchange's entry in the HAR recording.
    pub entry: u32,
    /// The request line, `METHOD url`.
    pub request_line: String,
    /// The request headers, one `name: value` line each.
    pub request_headers: String,
    /// The request body, when the request carried one.
    pub request_body: Option<String>,
    /// The status line, `HTTP <status>`.
    pub status_line: String,
    /// The response headers, one `name: value` line each.
    pub response_headers: String,
    /// The response body, when the server sent one.
    pub response_body: Option<String>,
}

/// The exchange a red row was judged on, or why the drawer has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordingView {
    /// The row names no exchange: it passed, was skipped, or went red before
    /// a request went out.
    NoExchange,
    /// The exchange the explanation names, read from the recording.
    Exchange(ExchangeView),
    /// The recording could not be read; the message is verbatim.
    Unreadable(String),
}

/// The result drawer: the row and the four-part explanation the engine wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultDetail {
    /// The row.
    pub row: ResultRow,
    /// The workflow's summary, from the catalogue.
    pub summary: Option<String>,
    /// The step the row went red at.
    pub step: Option<String>,
    /// What was checked.
    pub what: Option<String>,
    /// Why it is required: the cited source, section and quote.
    pub why: Option<String>,
    /// What the criterion expects.
    pub expected: Option<String>,
    /// What the exchange delivered, or why it could not be judged.
    pub received: Option<String>,
    /// Whose problem the row is.
    pub whose: Option<String>,
    /// The `upstream-report` issue the citation names, when any.
    pub report: Option<u64>,
    /// The skip reason and its citation, on a skipped row.
    pub skip: Option<String>,
    /// The judged exchange.
    pub recording: RecordingView,
}

#[cfg(feature = "ssr")]
pub mod read {
    //! The ssr readers over the finished job's results and recording.

    use super::{
        CertificateNotice, ExchangeView, RecordingView, ResultDetail, ResultRow, ResultsScreen,
    };
    use crate::certificates::CertificateStatus;
    use crate::engine::token;
    use crate::state::ConsoleState;
    use veredictum::arazzo::har::{Entry, Har, NameValue};
    use veredictum::arazzo::results::{ResultRow as EngineRow, Results, Why};

    /// This submitter's most recent finished run's results document, through
    /// the published lib.
    ///
    /// Several runs share the process (#389), so the record surface reads the
    /// run the person in front of it just drove, picked through the job map's
    /// ONE per-submitter reader (#134).
    fn finished_results(
        state: &ConsoleState,
        submitter: crate::submitter::Submitter,
    ) -> Result<Option<(Results, std::path::PathBuf, CertificateNotice)>, String> {
        let latest = state
            .jobs
            .latest_of(submitter, crate::run_job::Latest::Finished)
            .map_err(|e| e.to_string())?;
        let Some(id) = latest else {
            return Ok(None);
        };
        let view = state.jobs.view_of(id).map_err(|e| e.to_string())?;
        let Some(finished) = view.and_then(|view| view.finished) else {
            return Ok(None);
        };
        // The engine's own recorded path, never a second derivation of it.
        let path = std::path::PathBuf::from(finished.results_path);
        let body =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let results: Results =
            serde_json::from_str(&body).map_err(|e| format!("{}: {e}", path.display()))?;
        let certificate = match finished.certificate {
            CertificateStatus::Trial => CertificateNotice::Trial,
            CertificateStatus::Issued => {
                CertificateNotice::Issued(crate::certificates::download_href(&id.to_string()))
            }
            CertificateStatus::NotIssued(reason) => CertificateNotice::NotIssued(reason),
        };
        Ok(Some((results, path, certificate)))
    }

    /// A row index as the wire's fixed-size integer.
    ///
    /// NOTE: no openEHR spec governs this — our own design; a row index fits
    /// in `u32` on every catalogue, and saturating keeps `as` out.
    fn index(n: usize) -> u32 {
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// Maps one engine row to its table row.
    ///
    /// # Errors
    /// The status token's `serde_json` failure, verbatim.
    fn row_of(row: &EngineRow) -> Result<ResultRow, serde_json::Error> {
        let reason = match (&row.explanation, &row.skip) {
            (Some(explanation), _) => Some(explanation.received.clone()),
            (None, Some(skip)) => Some(skip.reason.to_string()),
            (None, None) => None,
        };
        Ok(ResultRow {
            case: row.case.clone(),
            row: index(row.row),
            status: token(&row.status)?,
            reason,
        })
    }

    /// The red-first ordering: failures, then errors, then the rest, id-tied.
    fn red_first(rows: &mut [ResultRow]) {
        rows.sort_by(|a, b| {
            let rank = |status: &str| match status {
                "failed" => 0_u8,
                "errored" => 1,
                "passed" => 2,
                _ => 3,
            };
            rank(&a.status)
                .cmp(&rank(&b.status))
                .then_with(|| a.case.cmp(&b.case))
                .then_with(|| a.row.cmp(&b.row))
        });
    }

    /// The results screen, or `Ok(None)` before a finished run.
    ///
    /// # Errors
    /// The verbatim read failures.
    pub fn results_screen(
        state: &ConsoleState,
        submitter: crate::submitter::Submitter,
    ) -> Result<Option<ResultsScreen>, String> {
        let Some((results, _, certificate)) = finished_results(state, submitter)? else {
            return Ok(None);
        };
        let mut rows: Vec<ResultRow> = results
            .rows
            .iter()
            .map(row_of)
            .collect::<Result<Vec<ResultRow>, serde_json::Error>>()
            .map_err(|e| format!("a result row did not render: {e}"))?;
        red_first(&mut rows);
        Ok(Some(ResultsScreen {
            base_url: results.base_url.clone(),
            tallies: crate::run_job::tally(&results),
            rows,
            certificate,
        }))
    }

    /// One `name: value` line per header, in recorded order.
    fn header_lines(headers: &[NameValue]) -> String {
        headers
            .iter()
            .map(|header| format!("{}: {}", header.name, header.value))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// One HAR entry as the drawer's two panes.
    #[must_use]
    pub fn exchange_of(entry: usize, recorded: &Entry) -> ExchangeView {
        let text = |body: &str| (!body.is_empty()).then(|| body.to_owned());
        ExchangeView {
            entry: index(entry),
            request_line: format!("{} {}", recorded.request.method, recorded.request.url),
            request_headers: header_lines(&recorded.request.headers),
            request_body: recorded
                .request
                .post_data
                .as_ref()
                .and_then(|data| text(&data.text)),
            status_line: format!("HTTP {}", recorded.response.status),
            response_headers: header_lines(&recorded.response.headers),
            response_body: text(&recorded.response.content.text),
        }
    }

    /// The exchange an explanation names, read from the recording beside the
    /// results document.
    fn recording_of(results_path: &std::path::Path, entry: Option<usize>) -> RecordingView {
        let Some(entry) = entry else {
            return RecordingView::NoExchange;
        };
        let Some(path) = results_path
            .parent()
            .map(|dir| dir.join(veredictum::pipeline::arazzo_run::RECORDING_FILE))
        else {
            return RecordingView::NoExchange;
        };
        match Har::read(&path) {
            Ok(har) => har.log.entries.get(entry).map_or_else(
                || {
                    RecordingView::Unreadable(format!(
                        "{}: the recording carries no entry {entry}",
                        path.display()
                    ))
                },
                |recorded| RecordingView::Exchange(exchange_of(entry, recorded)),
            ),
            Err(e) => RecordingView::Unreadable(e.to_string()),
        }
    }

    /// A citation as one line: source, section and the quote.
    fn why_line(why: &Why) -> String {
        format!("{}, {}: \"{}\"", why.source, why.section, why.quote)
    }

    /// One row joined to its explanation and its judged exchange, or
    /// `Ok(None)` when the results document does not carry it.
    ///
    /// # Errors
    /// The verbatim read failures.
    pub fn result_detail(
        state: &ConsoleState,
        submitter: crate::submitter::Submitter,
        case: &str,
        row: u32,
    ) -> Result<Option<ResultDetail>, String> {
        let Some((results, results_path, _)) = finished_results(state, submitter)? else {
            return Ok(None);
        };
        let Some(found) = results
            .rows
            .iter()
            .find(|held| held.case == case && index(held.row) == row)
        else {
            return Ok(None);
        };
        let summary = state.catalogue.as_ref().as_ref().ok().and_then(|listing| {
            listing
                .workflows()
                .find(|(_, workflow)| workflow.case == case)
                .and_then(|(_, workflow)| workflow.summary.clone())
        });
        let explanation = found.explanation.as_ref();
        Ok(Some(ResultDetail {
            row: row_of(found).map_err(|e| format!("a result row did not render: {e}"))?,
            summary,
            step: explanation.map(|e| e.step.clone()),
            what: explanation.map(|e| e.what.clone()),
            why: explanation.and_then(|e| e.why.as_ref()).map(why_line),
            expected: explanation.map(|e| e.expected.clone()),
            received: explanation.map(|e| e.received.clone()),
            whose: explanation.map(|e| e.whose.to_string()),
            report: explanation
                .and_then(|e| e.why.as_ref())
                .and_then(|why| why.report)
                .map(std::num::NonZeroU64::get),
            skip: found
                .skip
                .as_ref()
                .map(|skip| format!("{}: {}", skip.reason, why_line(&skip.why))),
            recording: recording_of(
                &results_path,
                explanation
                    .and_then(|e| e.exchange.as_ref())
                    .map(|x| x.entry),
            ),
        }))
    }
}

pub mod fns {
    //! The `#[server]` endpoints, one module for one inner suppression.
    #![allow(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        missing_docs,
        reason = "fires only in some #[server] expansions; see catalogue_api::fns"
    )]

    use leptos::prelude::{ServerFnError, server};

    use super::{ResultDetail, ResultsScreen};

    /// The results screen, `None` before a finished run.
    ///
    /// # Errors
    /// The verbatim read failures.
    #[server]
    pub async fn fetch_results() -> Result<Option<ResultsScreen>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        super::read::results_screen(&state, who)
            .map(|screen| crate::capture::results(&state, screen))
            .map_err(ServerFnError::new)
    }

    /// One row's detail; `None` for a row the results document does not
    /// carry.
    ///
    /// # Errors
    /// The verbatim read failures.
    #[server]
    pub async fn fetch_result_detail(
        case: String,
        row: u32,
    ) -> Result<Option<ResultDetail>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        super::read::result_detail(&state, who, &case, row)
            .map(|detail| crate::capture::detail(&state, detail))
            .map_err(ServerFnError::new)
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::read::exchange_of;

    /// The repository's committed recording of a red row, the same file the
    /// engine's own verification pack replays.
    fn recording() -> Result<veredictum::arazzo::har::Har, Box<dyn std::error::Error>> {
        let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
            .join("verification-pack/arazzo/fail.har");
        Ok(veredictum::arazzo::har::Har::read(&path)?)
    }

    /// One entry renders both sides of its wire, the request line and the
    /// status line taken from the recording verbatim.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_recorded_entry_renders_both_sides_of_its_wire() -> Result<(), Box<dyn std::error::Error>> {
        let har = recording()?;
        let entry = har
            .log
            .entries
            .first()
            .ok_or("the recording carries entries")?;
        let view = exchange_of(0, entry);
        assert_eq!(view.entry, 0);
        assert_eq!(
            view.request_line,
            format!("{} {}", entry.request.method, entry.request.url)
        );
        assert_eq!(view.status_line, format!("HTTP {}", entry.response.status));
        assert!(
            view.request_headers
                .contains("authorization: [not recorded]"),
            "the recording never carries a credential: {}",
            view.request_headers
        );
        Ok(())
    }
}
