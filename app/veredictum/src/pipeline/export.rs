// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Exporting a result document to the views a vendor's CI shows.
//!
//! The seam reads one document and tells the two families apart by their
//! `family` member: a `perf-result` names itself, and an Arazzo results
//! document carries no `family`. An Arazzo results document has JUnit XML,
//! CTRF and Markdown views, rendered by [`crate::arazzo::export`], and a
//! terminal view, rendered by [`crate::arazzo::terminal`]. A `perf-result`
//! has a Markdown and a terminal view, rendered by
//! [`crate::perf_result::view`]. Where each view is written is the caller's
//! decision.

use std::path::Path;

use serde::Deserialize;

use crate::arazzo::export::{ctrf, github_annotations, junit, markdown};
use crate::arazzo::results::Results;
use crate::arazzo::terminal::{ShowOptions, render};
use crate::badge::{Badge, badges, perf_badges};
use crate::perf_result::{PerfResult, view};
use crate::pipeline::{Error, RenderedFile, read_json};

/// The views of one results document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultsExports {
    /// The JUnit XML view.
    pub junit: String,
    /// The CTRF 0.2.0 JSON view.
    pub ctrf: String,
    /// The Markdown summary.
    pub markdown: String,
    /// The GitHub Actions workflow commands annotating each row that needs
    /// attention, at most GitHub's per-step limit of each kind.
    pub annotations: String,
}

/// The views of a document of either family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exports {
    /// An Arazzo results document's three views.
    Results(ResultsExports),
    /// A `perf-result`'s Markdown summary, its only exported view.
    Perf {
        /// The Markdown summary.
        markdown: String,
    },
}

impl Exports {
    /// The Markdown summary, which every family has.
    #[must_use]
    pub fn markdown(&self) -> &str {
        match self {
            Self::Results(views) => &views.markdown,
            Self::Perf { markdown } => markdown,
        }
    }
}

/// A result document of either family.
#[derive(Debug, Clone)]
pub enum Document {
    /// An Arazzo results document.
    Results(Box<Results>),
    /// A `perf-result` record.
    Perf(Box<PerfResult>),
}

/// The one member the families are told apart by.
#[derive(Deserialize)]
struct Family {
    #[serde(default)]
    family: Option<String>,
}

/// Whether the document at `path` names itself a `perf-result`.
///
/// # Errors
/// [`Error::Read`] when the file cannot be read, and [`Error::Parse`] when
/// it is not a JSON object.
pub fn is_perf_result(path: &Path) -> Result<bool, Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })?;
    let family: Family = serde_json::from_str(&text).map_err(|e| Error::Parse {
        context: path.display().to_string(),
        message: e.to_string(),
    })?;
    Ok(family.family.as_deref() == Some("perf-result"))
}

/// Reads the document at `path`, whichever family it is.
///
/// # Errors
/// [`Error::Read`] when the file cannot be read, [`Error::Parse`] when it is
/// not JSON or not a document of its family, and [`Error::Party`] naming the
/// file for a `perf-result` outside its schema or a `family` no view reads.
pub fn read_document(path: &Path) -> Result<Document, Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })?;
    let family: Family = serde_json::from_str(&text).map_err(|e| Error::Parse {
        context: path.display().to_string(),
        message: e.to_string(),
    })?;
    match family.family.as_deref() {
        None => serde_json::from_str(&text)
            .map(|results| Document::Results(Box::new(results)))
            .map_err(|e| Error::Parse {
                context: "Arazzo results".to_owned(),
                message: e.to_string(),
            }),
        Some("perf-result") => PerfResult::read(&text)
            .map(|record| Document::Perf(Box::new(record)))
            .map_err(|e| Error::Party(format!("{}: {e}", path.display()))),
        Some(other) => Err(Error::Party(format!(
            "{}: the family {other:?} is neither an Arazzo results document nor a perf-result",
            path.display()
        ))),
    }
}

/// Reads an Arazzo results document and renders its three views.
///
/// # Errors
/// [`Error::Read`] or [`Error::Parse`] for the results document, and
/// [`Error::Export`] when a view cannot be written.
pub fn export_results(results: &Path) -> Result<ResultsExports, Error> {
    let results: Results = read_json(results, "Arazzo results")?;
    results_exports(&results)
}

fn results_exports(results: &Results) -> Result<ResultsExports, Error> {
    Ok(ResultsExports {
        junit: junit(results)?,
        ctrf: ctrf(results)?,
        markdown: markdown(results),
        annotations: github_annotations(results),
    })
}

/// Reads a document of either family and renders its views.
///
/// # Errors
/// Every error of [`read_document`], and [`Error::Export`] when a view of a
/// results document cannot be written.
pub fn export_document(path: &Path) -> Result<Exports, Error> {
    match read_document(path)? {
        Document::Results(results) => Ok(Exports::Results(results_exports(&results)?)),
        Document::Perf(record) => Ok(Exports::Perf {
            markdown: view::markdown(&record),
        }),
    }
}

/// Reads a document of either family and draws its badges.
///
/// Each badge is `<name>.svg` and `<name>.json`: the four of a results document
/// in [`crate::badge::BadgeName::CONFORMANCE`] order, or the three of a
/// `perf-result` in [`crate::badge::BadgeName::PERF`] order.
///
/// # Errors
/// Every error of [`read_document`], and [`Error::Serialize`] when an
/// endpoint document cannot be written.
pub fn badge_files(path: &Path) -> Result<Vec<RenderedFile>, Error> {
    let drawn: Vec<Badge> = match read_document(path)? {
        Document::Results(results) => badges(&results),
        Document::Perf(record) => perf_badges(&record),
    };
    let mut files = Vec::new();
    for badge in drawn {
        let name = badge.name.token();
        files.push(RenderedFile {
            name: format!("{name}.svg"),
            body: badge.svg(),
        });
        files.push(RenderedFile {
            name: format!("{name}.json"),
            body: badge.endpoint_json().map_err(|source| Error::Serialize {
                context: format!("the {name} badge"),
                source,
            })?,
        });
    }
    Ok(files)
}

/// Reads a document of either family and renders its terminal view.
///
/// # Errors
/// Every error of [`read_document`].
pub fn show_results(results: &Path, options: ShowOptions) -> Result<String, Error> {
    match read_document(results)? {
        Document::Results(results) => Ok(render(&results, options)),
        Document::Perf(record) => Ok(view::terminal(&record, options.color)),
    }
}
