// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The published visuals, rendered deterministically from committed
//! artifacts.
//!
//! Every asset here is a pure function of files already in a repository (a
//! `perf-result`, or the crate's own schema definitions), so regenerating
//! one and diffing it is a build gate, and a hand-drawn number is a build
//! failure.

use std::path::Path;

use crate::pipeline::{Error, RenderedFile};

/// The charts of a `perf-result`, with the Markdown summary the book
/// includes, both drawn from the record alone.
#[derive(Debug)]
pub struct PerfResultAssets {
    /// The SVG files, in publication order.
    pub files: Vec<RenderedFile>,
    /// The Markdown summary.
    pub summary: String,
}

/// Renders the charts and the summary of the `perf-result` at `path`.
///
/// # Errors
/// Every error of [`crate::pipeline::export::read_document`], and
/// [`Error::Party`] naming the file when it is not a `perf-result`.
pub fn perf_result_assets(path: &Path) -> Result<PerfResultAssets, Error> {
    let crate::pipeline::export::Document::Perf(record) =
        crate::pipeline::export::read_document(path)?
    else {
        return Err(Error::Party(format!(
            "{}: not a perf-result",
            path.display()
        )));
    };
    Ok(PerfResultAssets {
        files: crate::perf_result::charts::charts(&record)
            .into_iter()
            .map(|chart| RenderedFile {
                name: chart.name.to_owned(),
                body: chart.svg,
            })
            .collect(),
        summary: crate::perf_result::view::markdown(&record),
    })
}

/// Returns the published JSON-Schema set, one file per artifact family.
///
/// The rendering is byte-deterministic, so the emitted set is diffable
/// against the committed one.
#[must_use]
pub fn schema_files() -> Vec<RenderedFile> {
    crate::schema::emit_all()
        .into_iter()
        .map(|(name, schema)| RenderedFile {
            name: name.to_owned(),
            body: crate::schema::render(&schema),
        })
        .collect()
}
