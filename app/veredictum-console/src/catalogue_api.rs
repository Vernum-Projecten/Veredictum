// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The read seam for S1 and S2 (#64): typed rows the screens render, served
//! by thin `#[server]` fns over the startup catalogue listing.
//!
//! Every number and every text here comes from the published lib's typed
//! model — the console re-parses nothing. Serialized types carry fixed-size
//! integers only: a pointer-sized `usize` crossing the 32-bit WASM to 64-bit
//! server boundary deserializes wrong
//! (<https://book.leptos.dev/server/25_server_functions.html>).

use serde::{Deserialize, Serialize};

/// What the instrument landing shows: the catalogue listing's own numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentSummary {
    /// The listed workflow count: one case each.
    pub cases: u64,
    /// The listed Arazzo document count.
    pub documents: u64,
    /// The capability folder count.
    pub capabilities: u64,
    /// The gate finding count; zero is the only passing result.
    pub findings: u64,
    /// The catalogue root, for display.
    pub root: String,
    /// The vendored spec tree, for display.
    pub specs: String,
}

/// The catalogue-missing explanation: which mount was expected, and the
/// verbatim reason the load refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogueMissing {
    /// The catalogue root the console looked at.
    pub root: String,
    /// The spec tree the console looked at.
    pub specs: String,
    /// The load error, verbatim.
    pub reason: String,
}

/// Either the summary or the honest absence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstrumentView {
    /// The catalogue loaded; the landing renders the numbers.
    Loaded(InstrumentSummary),
    /// The catalogue did not load; the landing renders the explanation.
    Missing(CatalogueMissing),
}

/// One capability folder and its case count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterRow {
    /// The capability folder under `cases/`.
    pub key: String,
    /// How many workflows the folder's documents carry.
    pub cases: u64,
}

/// One case row in a capability listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseRow {
    /// The case id, `<document>/<workflowId>`.
    pub id: String,
    /// The workflow's summary.
    pub summary: String,
    /// The document the workflow is in.
    pub band: String,
}

/// One document section of a capability listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BandRows {
    /// The document id.
    pub band: String,
    /// The document's workflows, in document order.
    pub cases: Vec<CaseRow>,
}

/// One step of a case, as the detail screen lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRow {
    /// The step id.
    pub id: String,
    /// The operation the step calls.
    pub operation: String,
    /// The step's description.
    pub description: String,
}

/// Everything the case-detail screen shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseDetail {
    /// The case id.
    pub id: String,
    /// The capability folder the case lives under.
    pub chapter: String,
    /// The document the case is a workflow of.
    pub document: String,
    /// The workflow's summary.
    pub summary: String,
    /// The workflow's description.
    pub description: String,
    /// The workflow's own citation, as `source, section: "quote"`.
    pub citation: Option<String>,
    /// The steps, in document order.
    pub steps: Vec<StepRow>,
    /// The option arm the document tests, as `option: arm`, when an option
    /// names it.
    pub option: Option<String>,
}

#[cfg(feature = "ssr")]
pub mod read {
    //! The ssr-side readers over the startup state — component-free, so the
    //! mapping from the listing to the rows is plain testable code.

    use super::{
        BandRows, CaseDetail, CaseRow, CatalogueMissing, ChapterRow, InstrumentSummary,
        InstrumentView, StepRow,
    };
    use crate::state::ConsoleState;

    /// A collection length as the wire's fixed-size integer.
    ///
    /// NOTE: no openEHR spec governs this — our own design; usize → u64 is
    /// lossless on every supported target, so the saturation arm is
    /// unreachable and exists only because `as` is banned.
    fn count(n: usize) -> u64 {
        u64::try_from(n).unwrap_or(u64::MAX)
    }

    /// Maps the startup state to the landing view.
    #[must_use]
    pub fn instrument_view(state: &ConsoleState) -> InstrumentView {
        match state.catalogue.as_ref() {
            Ok(listing) => InstrumentView::Loaded(InstrumentSummary {
                cases: count(listing.workflows().count()),
                documents: count(listing.documents.len()),
                capabilities: count(listing.capabilities().len()),
                findings: count(listing.findings.len()),
                root: state.root.display().to_string(),
                specs: state.specs.display().to_string(),
            }),
            Err(reason) => InstrumentView::Missing(CatalogueMissing {
                root: state.root.display().to_string(),
                specs: state.specs.display().to_string(),
                reason: reason.clone(),
            }),
        }
    }

    /// The capability folders, sorted by name, each with its case count.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    pub fn chapter_rows(state: &ConsoleState) -> Result<Vec<ChapterRow>, String> {
        let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        Ok(listing
            .capabilities()
            .into_iter()
            .map(|(key, documents)| ChapterRow {
                key: key.to_owned(),
                cases: count(documents.iter().map(|d| d.workflows.len()).sum()),
            })
            .collect())
    }

    /// One capability folder's cases, grouped by document and filtered by the
    /// case id substring `q`.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    pub fn band_rows(
        state: &ConsoleState,
        chapter: &str,
        q: &str,
    ) -> Result<Vec<BandRows>, String> {
        let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        Ok(listing
            .documents
            .iter()
            .filter(|document| document.capability == chapter)
            .map(|document| BandRows {
                band: document.id.clone(),
                cases: document
                    .workflows
                    .iter()
                    .filter(|workflow| q.is_empty() || workflow.case.contains(q))
                    .map(|workflow| CaseRow {
                        id: workflow.case.clone(),
                        summary: workflow.summary.clone().unwrap_or_default(),
                        band: document.id.clone(),
                    })
                    .collect(),
            })
            .filter(|band| !band.cases.is_empty())
            .collect())
    }

    /// The full case detail, or `Ok(None)` for an id the catalogue does not
    /// carry — a legitimately absent page, not an error.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    pub fn case_detail(state: &ConsoleState, id: &str) -> Result<Option<CaseDetail>, String> {
        let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let Some((document, workflow)) = listing.workflows().find(|(_, w)| w.case == id) else {
            return Ok(None);
        };
        Ok(Some(CaseDetail {
            id: workflow.case.clone(),
            chapter: document.capability.clone(),
            document: document.id.clone(),
            summary: workflow.summary.clone().unwrap_or_default(),
            description: workflow.description.clone().unwrap_or_default(),
            citation: workflow.citation.as_ref().map(|citation| {
                format!(
                    "{}, {}: \"{}\"",
                    citation.source, citation.section, citation.quote
                )
            }),
            steps: workflow
                .steps
                .iter()
                .map(|step| StepRow {
                    id: step.step_id.clone(),
                    operation: step.operation.clone().unwrap_or_default(),
                    description: step.description.clone().unwrap_or_default(),
                })
                .collect(),
            option: document
                .option_arm
                .as_ref()
                .map(|(option, arm)| format!("{option}: {arm}")),
        }))
    }
}
pub mod fns {
    //! The `#[server]` endpoints, one module for one inner suppression.
    //!
    //! The suppression covers what the macro expands: `unused_async` and
    //! `unused_async_trait_impl` on the async fn it wraps in a trait impl, and
    //! `missing_docs` on the argument struct it mints. Both fire only in SOME
    //! expansions, so `#[expect]` would itself warn through
    //! `unfulfilled_lint_expectations` in the quiet ones
    //! (<https://doc.rust-lang.org/rustc/lints/levels.html#expect>).
    #![allow(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        missing_docs,
        reason = "fires only in some #[server] expansions; see the module doc"
    )]

    use super::{BandRows, CaseDetail, ChapterRow, InstrumentView};
    use leptos::prelude::{ServerFnError, server};

    #[cfg(feature = "ssr")]
    use super::read;

    /// The landing view: the counts, or the named-mount explanation.
    ///
    /// # Errors
    /// The server-fn transport only; the catalogue's absence is data, not an
    /// error.
    #[server]
    pub async fn fetch_instrument() -> Result<InstrumentView, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        Ok(read::instrument_view(&state))
    }

    /// The capability folder list.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    #[server]
    pub async fn fetch_chapters() -> Result<Vec<ChapterRow>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        read::chapter_rows(&state).map_err(ServerFnError::new)
    }

    /// One capability folder's cases, grouped by document, filtered by the
    /// case id substring `q`.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    #[server]
    pub async fn fetch_chapter_bands(
        chapter: String,
        q: String,
    ) -> Result<Vec<BandRows>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        read::band_rows(&state, &chapter, &q).map_err(ServerFnError::new)
    }

    /// One case in full; `None` for an id the catalogue does not carry.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    #[server]
    pub async fn fetch_case_detail(id: String) -> Result<Option<CaseDetail>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        read::case_detail(&state, &id).map_err(ServerFnError::new)
    }
}
