// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The facts every surface reads about cases resting on released text an
//! upstream report records as unclear or contradictory.
//!
//! Such a case, and an option of the spec-boundary tier, carries the one
//! `@SpecDiverge` tag; a closing note counts the tagged cases and links each
//! report. No openEHR spec governs this — our own design: the reports are
//! this repository's own `upstream-report` issues.

use std::collections::BTreeSet;

/// The tag a case or option resting on reported text carries.
pub const TAG: &str = "@SpecDiverge";

/// The tracker every upstream report is an issue of.
pub const ISSUES: &str = "https://github.com/Vernum-Projecten/Veredictum/issues";

/// The tracker address of report `report`.
#[must_use]
pub fn issue_href(report: u64) -> String {
    format!("{ISSUES}/{report}")
}

/// The hover text of a tag over `reports`: the report numbers it rests on.
#[must_use]
pub fn tag_title(reports: &[u64]) -> String {
    let numbers: Vec<String> = reports.iter().map(|r| format!("#{r}")).collect();
    format!(
        "Rests on released text an upstream report records: {}",
        numbers.join(", ")
    )
}

/// The tagged cases of a surface: each case id once, with every report any
/// of them rests on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Divergence {
    /// The distinct tagged case ids.
    pub cases: BTreeSet<String>,
    /// The reports, ascending.
    pub reports: BTreeSet<u64>,
}

impl Divergence {
    /// Adds case `case` resting on `reports`; a case resting on none is not
    /// tagged and adds nothing.
    pub fn add(&mut self, case: &str, reports: &[u64]) {
        if reports.is_empty() {
            return;
        }
        self.cases.insert(case.to_owned());
        self.reports.extend(reports.iter().copied());
    }

    /// The number of distinct tagged cases.
    #[must_use]
    pub fn count(&self) -> u32 {
        u32::try_from(self.cases.len()).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::{Divergence, issue_href, tag_title};

    /// A report links to its issue on this repository's tracker.
    #[test]
    fn a_report_links_to_its_issue() {
        assert_eq!(
            issue_href(352),
            "https://github.com/Vernum-Projecten/Veredictum/issues/352"
        );
    }

    /// The tag's hover text names the report numbers.
    #[test]
    fn a_tag_names_its_reports() {
        assert_eq!(
            tag_title(&[661, 673]),
            "Rests on released text an upstream report records: #661, #673"
        );
    }

    /// A case counts once however often it is listed, and an untagged case
    /// not at all.
    #[test]
    fn every_tagged_case_counts_once() {
        let mut found = Divergence::default();
        found.add("a/x", &[673, 661]);
        found.add("a/x", &[661]);
        found.add("b/y", &[161]);
        found.add("c/z", &[]);
        assert_eq!(found.count(), 2);
        assert_eq!(
            found.reports.into_iter().collect::<Vec<_>>(),
            vec![161, 661, 673]
        );
    }
}
