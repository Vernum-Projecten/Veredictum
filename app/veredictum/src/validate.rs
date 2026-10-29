// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `validate` gate: every Arazzo gate over a catalogue root, and the
//! static review of one supplied statement against that catalogue.
//!
//! Zero findings is the only passing result. The review holds a statement to
//! the published statement schema and to the catalogue it is run against: a
//! claimed capability must be a capability folder, a selected option arm
//! must be an arm of an option in `capabilities.yaml`, one per option, and a
//! claimed capability folder must hold a workflow the statement lets run. No
//! openEHR spec governs the gate itself — our own design.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use std::fmt;
use std::path::{Path, PathBuf};

use crate::arazzo::catalogue::Selection;
use crate::arazzo::gates::{CASES_DIR, Gate};
use crate::pipeline::Error;
use crate::pipeline::listing::{self, Listing};

/// The suffix a rows file carries beside its document.
pub const ROWS_SUFFIX: &str = ".rows.yaml";

/// Which gate produced a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CheckId {
    /// The supplied statement does not hold against its schema or the
    /// catalogue.
    Statement,
    /// The supplied statement claims a capability folder in which no
    /// workflow can run under the statement's own declarations.
    ClaimCompleteness,
    /// One of the Arazzo catalogue's gates.
    Arazzo(Gate),
}

impl CheckId {
    /// Stable token for reports and tests.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Statement => "statement",
            Self::ClaimCompleteness => "claim-completeness",
            Self::Arazzo(gate) => gate.token(),
        }
    }
}

/// One violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Which gate produced the finding.
    pub check: CheckId,
    /// The offending file.
    pub artifact: String,
    /// What is wrong, in one line.
    pub message: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {}: {}",
            self.check.token(),
            self.artifact,
            self.message
        )
    }
}

/// One validation pass over a catalogue root.
#[derive(Debug, Clone)]
pub struct Validation {
    /// The catalogue as read: its documents, workflows and folders.
    pub listing: Listing,
    /// Every finding, the catalogue's first and the statement's after.
    pub findings: Vec<Finding>,
    /// The rows files under the catalogue's `cases/`.
    pub rows_files: usize,
}

impl Validation {
    /// Whether the pass found nothing.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// The one-line summary `validate` prints: what was read, and how many
    /// findings it carries, so a run that read nothing cannot print the same
    /// line as one that read everything.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} document(s), {} workflow(s), {} rows file(s), {} capability folder(s), {} finding(s)",
            self.listing.documents.len(),
            self.listing.workflows().count(),
            self.rows_files,
            self.listing.capabilities().len(),
            self.findings.len()
        )
    }
}

/// Runs every gate over the catalogue under `root`, and, when `statement` is
/// supplied, the static review of that declaration.
///
/// # Errors
/// [`Error::ArazzoCatalogue`] when a capability folder does not list, and
/// [`Error::Parse`] when `capabilities.yaml` does not load.
pub fn validate(
    root: &Path,
    spec_root: Option<&Path>,
    statement: Option<&Path>,
) -> Result<Validation, Error> {
    let listing = listing::read(root, spec_root)?;
    let mut findings: Vec<Finding> = listing
        .findings
        .iter()
        .map(|finding| Finding {
            check: CheckId::Arazzo(finding.gate),
            artifact: finding.file.display().to_string(),
            message: format!("{}: {}", finding.location, finding.message),
        })
        .collect();
    if let Some(path) = statement {
        findings.extend(review_statement(root, path));
    }
    Ok(Validation {
        rows_files: rows_files(&root.join(CASES_DIR)),
        listing,
        findings,
    })
}

/// The static review of one supplied statement: its published schema, then
/// the claims and the option arms against the catalogue under `root`, then
/// the completeness of every claim.
#[must_use]
pub fn review_statement(root: &Path, path: &Path) -> Vec<Finding> {
    let finding = |message: String| Finding {
        check: CheckId::Statement,
        artifact: path.display().to_string(),
        message,
    };
    let value: serde_json::Value = match std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
    {
        Ok(value) => value,
        Err(reason) => return vec![finding(format!("the statement does not read: {reason}"))],
    };
    let validator = match jsonschema::validator_for(&crate::schema::statement_schema()) {
        Ok(validator) => validator,
        Err(e) => {
            return vec![finding(format!(
                "the statement schema does not compile: {e}"
            ))];
        }
    };
    let violations: Vec<Finding> = validator
        .iter_errors(&value)
        .map(|e| finding(format!("{}: {e}", e.instance_path())))
        .collect();
    if !violations.is_empty() {
        return violations;
    }
    match Selection::read(root, Some(&value), &[]) {
        Ok(selection) => incomplete_claims(root, path, &selection),
        Err(e) => vec![finding(e.to_string())],
    }
}

/// The claim-completeness gate: every capability folder the statement
/// claims holds a document that the statement's own declarations (its
/// option arms, its versions and its formats) let run. A claim no workflow
/// can be run for would verify nothing. No openEHR spec governs this — our
/// own design.
fn incomplete_claims(root: &Path, path: &Path, selection: &Selection) -> Vec<Finding> {
    let repo_root = root.join("..");
    let Some(claimed) = selection.claimed() else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    for folder in claimed {
        let mut reasons = std::collections::BTreeSet::new();
        let mut runs = false;
        for found in selection.paths().iter().filter(|f| &f.capability == folder) {
            match selection.choice(found).skip(&found.id, &repo_root) {
                None => runs = true,
                Some(skip) => {
                    reasons.insert(skip.reason.to_string());
                }
            }
        }
        if !runs {
            let reasons: Vec<String> = reasons.into_iter().collect();
            findings.push(Finding {
                check: CheckId::ClaimCompleteness,
                artifact: path.display().to_string(),
                message: format!(
                    "the statement claims `{folder}`, and no workflow in it can run under the \
                     statement: every document is skipped ({})",
                    reasons.join(", ")
                ),
            });
        }
    }
    findings
}

/// The rows files under `dir`, counted recursively; a directory that does
/// not list counts none, which the gates report on their own.
fn rows_files(dir: &Path) -> usize {
    let mut stack: Vec<PathBuf> = vec![dir.to_owned()];
    let mut count = 0_usize;
    while let Some(next) = stack.pop() {
        // NOTE: a folder that does not list is a gate finding already, so it
        // adds nothing to the count rather than failing the summary.
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|name| name.ends_with(ROWS_SUFFIX))
            {
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result-returning tests in the Book ch11 shape, each asserting; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::{CheckId, review_statement, validate};
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).to_path_buf()
    }

    #[test]
    fn the_committed_catalogue_validates_clean() -> Result<(), Box<dyn std::error::Error>> {
        let root = repo_root();
        let validation = validate(
            &root.join("catalogue"),
            Some(&root.join("specs/openehr")),
            None,
        )?;
        assert!(validation.is_clean(), "{:?}", validation.findings.first());
        assert!(
            validation.rows_files > 0,
            "the catalogue carries rows files"
        );
        assert!(
            validation.summary().ends_with(", 0 finding(s)"),
            "{}",
            validation.summary()
        );
        Ok(())
    }

    #[test]
    fn the_declaration_fixture_passes_the_static_review() {
        let root = repo_root();
        let findings = review_statement(
            &root.join("catalogue"),
            &root.join("fixtures/declaration/statement.json"),
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_claim_outside_the_catalogue_is_a_statement_finding()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = assert_fs::TempDir::new()?;
        let path = dir.path().join("statement.json");
        std::fs::write(
            &path,
            r#"{"product":{"name":"x","version":"1","vendor":"v","identifier":"urn:x"},"schedule_release":"cnf-2.0-w2","claims":{"capabilities":["NoSuchFolder"]}}"#,
        )?;
        let findings = review_statement(&repo_root().join("catalogue"), &path);
        assert_eq!(findings.len(), 1, "{findings:?}");
        let finding = findings.first().ok_or("one finding")?;
        assert_eq!(finding.check, CheckId::Statement);
        assert!(finding.message.contains("NoSuchFolder"), "{finding}");
        Ok(())
    }

    /// The published schema, `validate --statement` and `test` give one
    /// answer to a statement that claims nothing: refused.
    #[test]
    fn a_statement_claiming_no_capability_is_refused_by_schema_validate_and_test_alike()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::arazzo::catalogue::{CatalogueError, claimed_capabilities};
        let validator = jsonschema::validator_for(&crate::schema::statement_schema())?;
        let dir = assert_fs::TempDir::new()?;
        let path = dir.path().join("statement.json");
        let folders = ["ehr".to_owned()].into();
        for claims in [
            serde_json::json!({}),
            serde_json::json!({ "capabilities": [] }),
        ] {
            let statement = serde_json::json!({
                "product": { "name": "x", "version": "1", "vendor": "v", "identifier": "urn:x" },
                "schedule_release": "cnf-2.0-w2",
                "claims": claims
            });
            assert!(!validator.is_valid(&statement), "{claims}");
            std::fs::write(&path, statement.to_string())?;
            let findings = review_statement(&repo_root().join("catalogue"), &path);
            assert!(
                findings
                    .iter()
                    .any(|f| f.check == CheckId::Statement && f.message.contains("/claims")),
                "{claims}: {findings:?}"
            );
            assert!(
                matches!(
                    claimed_capabilities(Some(&statement), &folders),
                    Err(CatalogueError::ClaimsShape(_))
                ),
                "{claims}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_statement_off_its_schema_is_refused_before_the_catalogue_reads_it()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = assert_fs::TempDir::new()?;
        let path = dir.path().join("statement.json");
        std::fs::write(&path, r#"{"hello":"world"}"#)?;
        let findings = review_statement(&repo_root().join("catalogue"), &path);
        assert!(!findings.is_empty());
        assert!(
            findings
                .iter()
                .all(|finding| finding.check == CheckId::Statement)
        );
        Ok(())
    }
}
