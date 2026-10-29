// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Judging a measured campaign: the pure verdict pipeline over a statement,
//! a results record and the performance schedule.
//!
//! Nothing here touches a network or a system under test. The inputs are two
//! committed party artifacts and the performance schedule; the output is the
//! verdict report, which makes a judgement reproducible by anyone holding the
//! same three inputs.

use std::path::Path;

use crate::party::{Results, Statement};
use crate::pipeline::measured::load_clean_schedule;
use crate::pipeline::{Error, RenderedFile, load_party_json, to_json_document};
use crate::schema::{results_schema, statement_schema};
use crate::verdict::VerdictReport;

/// Which campaign to judge, against which performance schedule.
#[derive(Debug)]
pub struct JudgementRequest<'a> {
    /// The party statement, the claim being judged.
    pub statement: &'a Path,
    /// The party results, the measured campaign being judged.
    pub results: &'a Path,
    /// The performance schedule root (`perf/` in this repository) whose
    /// cases the measurements re-derive against.
    pub root: &'a Path,
}

/// One completed judgement.
#[derive(Debug)]
pub struct Judgement {
    /// The claim that was judged.
    pub statement: Statement,
    /// The campaign that was judged.
    pub results: Results,
    /// The verdicts and the static-review findings.
    pub report: VerdictReport,
    /// The submission set: the verdict record.
    pub documents: Vec<RenderedFile>,
}

impl Judgement {
    /// Returns whether the judgement is clean, which means the static review
    /// found nothing.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.report.review.is_empty()
    }
}

/// The file the verdict record is written to.
pub const VERDICTS_FILE: &str = "verdicts.json";

/// Computes the verdicts for one campaign.
///
/// # Errors
/// [`Error::Party`] when either party artifact fails its schema or model
/// stage, [`Error::ResultsInvariants`] when the results violate their own
/// invariants, the schedule loader's own errors when the performance schedule
/// does not load, and [`Error::Serialize`] when the record cannot be produced.
pub fn judge(request: &JudgementRequest<'_>) -> Result<Judgement, Error> {
    let statement: Statement = load_party_json(
        request.statement,
        &statement_schema(),
        "statement.schema.json",
    )?;
    let results: Results =
        load_party_json(request.results, &results_schema(), "results.schema.json")?;
    results
        .check_invariants()
        .map_err(Error::ResultsInvariants)?;

    let schedule = load_clean_schedule(request.root)?;
    let cases: Vec<_> = schedule.cases.into_iter().map(|(_, case)| case).collect();
    let report = crate::verdict::compute(&statement, &results, &cases);
    let documents = vec![RenderedFile {
        name: VERDICTS_FILE.to_owned(),
        body: to_json_document(&report, "cannot serialize verdicts")?,
    }];

    Ok(Judgement {
        statement,
        results,
        report,
        documents,
    })
}
