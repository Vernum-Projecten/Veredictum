// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Exporting an Arazzo results document to the views a vendor's CI shows.
//!
//! The seam reads one `results.json` the Arazzo evaluator wrote and hands
//! back its JUnit XML, CTRF and Markdown views, rendered by
//! [`crate::arazzo::export`]. Where each view is written is the caller's
//! decision.

use std::path::Path;

use crate::arazzo::export::{ctrf, junit, markdown};
use crate::arazzo::results::Results;
use crate::pipeline::{Error, read_json};

/// The three views of one results document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultsExports {
    /// The JUnit XML view.
    pub junit: String,
    /// The CTRF 0.2.0 JSON view.
    pub ctrf: String,
    /// The Markdown summary.
    pub markdown: String,
}

/// Reads an Arazzo results document and renders its three views.
///
/// # Errors
/// [`Error::Read`] or [`Error::Parse`] for the results document, and
/// [`Error::Export`] when a view cannot be written.
pub fn export_results(results: &Path) -> Result<ResultsExports, Error> {
    let results: Results = read_json(results, "Arazzo results")?;
    Ok(ResultsExports {
        junit: junit(&results)?,
        ctrf: ctrf(&results)?,
        markdown: markdown(&results),
    })
}
