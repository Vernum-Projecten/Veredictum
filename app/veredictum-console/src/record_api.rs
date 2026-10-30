// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The record seam for S6 Results (#67).
//!
//! Everything renders from the finished job's own results document and HAR
//! recording, read through the published lib. A red row's explanation is the
//! one the engine wrote; nothing here re-judges a row.

use serde::{Deserialize, Serialize};

/// The row counts of one group, an advisory row counted apart from a failed
/// one, as the engine's reading counts them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    /// Every row.
    pub tests: u32,
    /// The passed rows.
    pub passed: u32,
    /// The failed rows that count against the server.
    pub failed: u32,
    /// The failed rows whose server did not follow a SHOULD.
    pub advisory: u32,
    /// The rows the instrument could not judge.
    pub errored: u32,
    /// The rows not run.
    pub skipped: u32,
}

impl Tally {
    /// The sum of `self` and `other`, saturating.
    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self {
            tests: self.tests.saturating_add(other.tests),
            passed: self.passed.saturating_add(other.passed),
            failed: self.failed.saturating_add(other.failed),
            advisory: self.advisory.saturating_add(other.advisory),
            errored: self.errored.saturating_add(other.errored),
            skipped: self.skipped.saturating_add(other.skipped),
        }
    }
}

/// One results row as every group lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultRow {
    /// The case id, `<document>/<workflowId>`.
    pub case: String,
    /// The 0-based row of the case.
    pub row: u32,
    /// The status token (`passed` / `failed` / `errored` / `skipped`).
    pub status: String,
    /// The word of the row's mark: its status, with a failed SHOULD read as
    /// `advisory`.
    pub mark: String,
    /// The glyph of the row's mark, so it reads without colour.
    pub glyph: String,
    /// What a red row received, or why a skipped row was not run.
    pub reason: Option<String>,
    /// The template formalism badge, on a case of a formalism folder.
    pub formalism: Option<String>,
    /// Whether the case rests on TRIAL text.
    pub trial: bool,
    /// The upstream reports the case rests on.
    pub reports: Vec<u64>,
}

/// One BASE section of a band.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionView {
    /// The in-page anchor of the section.
    pub anchor: String,
    /// The heading.
    pub title: String,
    /// The formalism the section's cases share, when its band splits by it.
    pub formalism: Option<String>,
    /// Its counts.
    pub tally: Tally,
    /// Its rows, red first.
    pub rows: Vec<ResultRow>,
}

/// One band: the BASE rows of one capability folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BandView {
    /// The capability folder.
    pub folder: String,
    /// The in-page anchor of the band.
    pub anchor: String,
    /// The heading.
    pub title: String,
    /// Its counts.
    pub tally: Tally,
    /// Its sections, in declared order.
    pub sections: Vec<SectionView>,
}

/// One BASE chapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterView {
    /// The in-page anchor of the chapter.
    pub anchor: String,
    /// The heading.
    pub title: String,
    /// The catalogue page of the chapter's first folder.
    pub href: String,
    /// Its counts.
    pub tally: Tally,
    /// Every band, those with no case included.
    pub bands: Vec<BandView>,
}

/// One arm of an option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmView {
    /// The arm token.
    pub arm: String,
    /// Whether the statement selected it.
    pub selected: bool,
    /// Whether the catalogue names no document for it.
    pub empty: bool,
    /// Its counts.
    pub tally: Tally,
    /// Its rows, red first.
    pub rows: Vec<ResultRow>,
}

/// One option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionView {
    /// The option name.
    pub option: String,
    /// The in-page anchor of the option.
    pub anchor: String,
    /// What it decides.
    pub description: String,
    /// The heading of its theme.
    pub theme: String,
    /// The upstream report its citation names.
    pub report: Option<u64>,
    /// Whether the statement selected one of its arms.
    pub selected: bool,
    /// Its counts.
    pub tally: Tally,
    /// Its arms.
    pub arms: Vec<ArmView>,
}

/// The BASE rows resting on TRIAL text, read with the options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialView {
    /// The in-page anchor of the block.
    pub anchor: String,
    /// The heading.
    pub title: String,
    /// Its counts.
    pub tally: Tally,
    /// Its rows, red first.
    pub rows: Vec<ResultRow>,
}

/// One option tier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierView {
    /// The in-page anchor of the tier.
    pub anchor: String,
    /// The heading.
    pub title: String,
    /// Its counts, the TRIAL block included.
    pub tally: Tally,
    /// Its options.
    pub options: Vec<OptionView>,
    /// The TRIAL-grounded BASE rows, on the OPTIONS tier only.
    pub trial: Option<TrialView>,
}

/// The rows resting on text one upstream report records as defective.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportView {
    /// The issue number.
    pub report: u64,
    /// The in-page anchor of the block.
    pub anchor: String,
    /// The issue title, from the committed snapshot.
    pub title: Option<String>,
    /// Where it stands, from the committed snapshot.
    pub state: Option<String>,
    /// Its counts.
    pub tally: Tally,
    /// Its rows, red first.
    pub rows: Vec<ResultRow>,
}

/// The engine's reading of one results document, as the page lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingView {
    /// Every row, counted once.
    pub total: Tally,
    /// BASE, chapter by chapter.
    pub chapters: Vec<ChapterView>,
    /// RECOMMENDED, OPTIONS (with the TRIAL block) and the spec boundary,
    /// in that order.
    pub tiers: Vec<TierView>,
    /// The rows resting on reported text, by report.
    pub reports: Vec<ReportView>,
    /// The rows the results place nowhere.
    pub unplaced: Vec<ResultRow>,
}

impl ReadingView {
    /// Every row once, in reading order: BASE, the options, the TRIAL
    /// block, the unplaced rows.
    #[must_use]
    pub fn rows(&self) -> Vec<&ResultRow> {
        let base = self
            .chapters
            .iter()
            .flat_map(|c| c.bands.iter())
            .flat_map(|b| b.sections.iter())
            .flat_map(|s| s.rows.iter());
        let tiers = self.tiers.iter().flat_map(|tier| {
            tier.options
                .iter()
                .flat_map(|o| o.arms.iter())
                .flat_map(|a| a.rows.iter())
                .chain(tier.trial.iter().flat_map(|t| t.rows.iter()))
        });
        base.chain(tiers).chain(self.unplaced.iter()).collect()
    }
}

/// The results screen: the base URL, the engine's reading of the rows, and
/// the certificate notice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultsScreen {
    /// The base URL the run drove.
    pub base_url: String,
    /// Every row, grouped as the engine reads the results.
    pub reading: ReadingView,
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
    /// The catalogue page of the case the row came from, when the catalogue
    /// carries it.
    pub catalogue: Option<String>,
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
        ArmView, BandView, CertificateNotice, ChapterView, ExchangeView, OptionView, ReadingView,
        RecordingView, ReportView, ResultDetail, ResultRow, ResultsScreen, SectionView, Tally,
        TierView, TrialView,
    };
    use crate::certificates::CertificateStatus;
    use crate::engine::token;
    use crate::state::ConsoleState;
    use veredictum::arazzo::har::{Entry, Har, NameValue};
    use veredictum::arazzo::reading::{CaseRows, Mark, OptionGroup, Tally as EngineTally, read};
    use veredictum::arazzo::results::{ResultRow as EngineRow, Results, Why};
    use veredictum::arazzo::structure::{Place, Tier};

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

    /// A count as the wire's fixed-size integer.
    ///
    /// NOTE: no openEHR spec governs this — our own design; a row count fits
    /// in `u32` on every catalogue, and saturating keeps `as` out.
    fn count(n: usize) -> u32 {
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// The engine's tally on the wire.
    fn tally_of(tally: &EngineTally) -> Tally {
        Tally {
            tests: count(tally.tests),
            passed: count(tally.passed),
            failed: count(tally.failed),
            advisory: count(tally.advisory),
            errored: count(tally.errored),
            skipped: count(tally.skipped),
        }
    }

    /// An in-page anchor: `prefix` and `parts` joined by `-`, every character
    /// outside `[a-z0-9]` read as `-`.
    #[must_use]
    pub fn anchor(prefix: &str, parts: &[&str]) -> String {
        let mut out = String::from(prefix);
        for part in parts {
            out.push('-');
            out.extend(part.chars().map(|c| {
                let c = c.to_ascii_lowercase();
                if c.is_ascii_alphanumeric() { c } else { '-' }
            }));
        }
        out
    }

    /// Where a mark sorts in a group: what counts against the server first,
    /// then what the instrument could not judge, then advisory, passed and
    /// skipped.
    const fn rank(mark: Mark) -> u8 {
        match mark {
            Mark::Failed => 0,
            Mark::Errored => 1,
            Mark::Advisory => 2,
            Mark::Passed => 3,
            Mark::Skipped => 4,
        }
    }

    /// Maps one engine row, placed at `place`, to its listed row.
    ///
    /// # Errors
    /// The status token's `serde_json` failure, verbatim.
    fn row_of(row: &EngineRow, place: Option<&Place>) -> Result<ResultRow, serde_json::Error> {
        let reason = match (&row.explanation, &row.skip) {
            (Some(explanation), _) => Some(explanation.received.clone()),
            (None, Some(skip)) => Some(skip.reason.to_string()),
            (None, None) => None,
        };
        let mark = Mark::of(row);
        Ok(ResultRow {
            case: row.case.clone(),
            row: count(row.row),
            status: token(&row.status)?,
            mark: mark.word().to_owned(),
            glyph: mark.glyph().to_owned(),
            reason,
            formalism: place
                .and_then(|p| p.formalism)
                .map(|f| f.label().to_owned()),
            trial: place.is_some_and(|p| p.trial),
            reports: place
                .map(|p| p.reports.iter().map(|r| r.get()).collect())
                .unwrap_or_default(),
        })
    }

    /// The rows of `cases`, red first, document order kept within a mark.
    ///
    /// # Errors
    /// The status token's `serde_json` failure, verbatim.
    fn rows_of(cases: &[CaseRows<'_>]) -> Result<Vec<ResultRow>, serde_json::Error> {
        let mut rows: Vec<(u8, ResultRow)> = Vec::new();
        for case in cases {
            for row in &case.rows {
                rows.push((rank(Mark::of(row)), row_of(row, case.place)?));
            }
        }
        rows.sort_by_key(|(rank, _)| *rank);
        Ok(rows.into_iter().map(|(_, row)| row).collect())
    }

    /// One option as the page lists it.
    ///
    /// # Errors
    /// The status token's `serde_json` failure, verbatim.
    fn option_of(group: &OptionGroup<'_>) -> Result<OptionView, serde_json::Error> {
        Ok(OptionView {
            option: group.option.to_owned(),
            anchor: anchor("option", &[group.option]),
            description: group.description.to_owned(),
            theme: group.theme.title().to_owned(),
            report: group.report.map(std::num::NonZeroU64::get),
            selected: group.selected,
            tally: tally_of(&group.tally),
            arms: group
                .arms
                .iter()
                .map(|arm| {
                    Ok(ArmView {
                        arm: arm.arm.to_owned(),
                        selected: arm.selected,
                        empty: arm.empty,
                        tally: tally_of(&arm.tally),
                        rows: rows_of(&arm.cases)?,
                    })
                })
                .collect::<Result<_, serde_json::Error>>()?,
        })
    }

    /// One tier as the page lists it.
    ///
    /// # Errors
    /// The status token's `serde_json` failure, verbatim.
    fn tier_of(
        tier: Tier,
        groups: &[OptionGroup<'_>],
        trial: Option<TrialView>,
    ) -> Result<TierView, serde_json::Error> {
        let options = groups
            .iter()
            .map(option_of)
            .collect::<Result<Vec<_>, _>>()?;
        let tally = options
            .iter()
            .map(|o| o.tally)
            .chain(trial.iter().map(|t| t.tally))
            .fold(Tally::default(), Tally::plus);
        let key = match tier {
            Tier::Recommended => "recommended",
            Tier::Options => "options",
            Tier::SpecBoundary => "boundary",
        };
        Ok(TierView {
            anchor: anchor("tier", &[key]),
            title: tier.title().to_owned(),
            tally,
            options,
            trial,
        })
    }

    /// The engine's reading of `results`, as the page lists it.
    ///
    /// # Errors
    /// The status token's `serde_json` failure, verbatim.
    pub fn reading_view(results: &Results) -> Result<ReadingView, serde_json::Error> {
        let reading = read(results);
        let chapters = reading
            .base
            .iter()
            .map(|chapter| {
                let bands = chapter
                    .bands
                    .iter()
                    .map(|band| {
                        let sections = band
                            .sections
                            .iter()
                            .map(|section| {
                                let formalism = section.formalism.map(|f| f.label().to_owned());
                                let mut parts = vec![band.folder, section.section];
                                if let Some(label) = formalism.as_deref() {
                                    parts.push(label);
                                }
                                Ok(SectionView {
                                    anchor: anchor("section", &parts),
                                    title: section.title.to_owned(),
                                    formalism,
                                    tally: tally_of(&section.tally),
                                    rows: rows_of(&section.cases)?,
                                })
                            })
                            .collect::<Result<Vec<_>, serde_json::Error>>()?;
                        Ok(BandView {
                            folder: band.folder.to_owned(),
                            anchor: anchor("band", &[band.folder]),
                            title: band.title.to_owned(),
                            tally: tally_of(&band.tally),
                            sections,
                        })
                    })
                    .collect::<Result<Vec<_>, serde_json::Error>>()?;
                Ok(ChapterView {
                    anchor: anchor("chapter", &[chapter.chapter]),
                    title: chapter.title.to_owned(),
                    href: bands.first().map_or_else(
                        || String::from("/catalogue"),
                        |b| format!("/catalogue/{}", b.folder),
                    ),
                    tally: tally_of(&chapter.tally),
                    bands,
                })
            })
            .collect::<Result<Vec<_>, serde_json::Error>>()?;
        let trial = TrialView {
            anchor: String::from("trial"),
            title: String::from("Resting on TRIAL text"),
            tally: tally_of(&reading.trial.tally),
            rows: rows_of(&reading.trial.cases)?,
        };
        let tiers = vec![
            tier_of(Tier::Recommended, &reading.recommended, None)?,
            tier_of(Tier::Options, &reading.options, Some(trial))?,
            tier_of(Tier::SpecBoundary, &reading.boundary, None)?,
        ];
        let reports = reading
            .reports
            .iter()
            .map(|group| {
                Ok(ReportView {
                    report: group.report.get(),
                    anchor: anchor("report", &[&group.report.to_string()]),
                    title: group.title.map(str::to_owned),
                    state: group.state.map(|s| s.to_string()),
                    tally: tally_of(&group.tally),
                    rows: rows_of(&group.cases)?,
                })
            })
            .collect::<Result<Vec<_>, serde_json::Error>>()?;
        Ok(ReadingView {
            total: tally_of(&reading.total),
            chapters,
            tiers,
            reports,
            unplaced: rows_of(&reading.unplaced)?,
        })
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
        let reading =
            reading_view(&results).map_err(|e| format!("a result row did not render: {e}"))?;
        Ok(Some(ResultsScreen {
            base_url: results.base_url.clone(),
            reading,
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
            entry: count(entry),
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
            .find(|held| held.case == case && count(held.row) == row)
        else {
            return Ok(None);
        };
        let listed = state.catalogue.as_ref().as_ref().ok().and_then(|listing| {
            listing
                .workflows()
                .find(|(_, workflow)| workflow.case == case)
        });
        let place = results.outline.as_ref().and_then(|o| o.place(case));
        let explanation = found.explanation.as_ref();
        Ok(Some(ResultDetail {
            row: row_of(found, place).map_err(|e| format!("a result row did not render: {e}"))?,
            summary: listed.and_then(|(_, workflow)| workflow.summary.clone()),
            catalogue: listed.map(|(document, workflow)| {
                format!("/catalogue/{}/{}", document.capability, workflow.case)
            }),
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
