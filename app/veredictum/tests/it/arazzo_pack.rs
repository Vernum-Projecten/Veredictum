// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The verification pack's Arazzo recordings: re-judging each committed HAR
//! recording through the evaluator reproduces every adjudicated verdict, and
//! twice over produces the same bytes. A tampered verdict is caught, which is
//! the pack's own falsifiability requirement.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken fixture must abort the test loudly, Book ch11"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use veredictum::arazzo::document::ArazzoDocument;
use veredictum::arazzo::evaluate::{Plan, rejudge};
use veredictum::arazzo::har::Har;
use veredictum::arazzo::results::{Results, Skip, SkipReason, Status, Why};

#[derive(Debug, Deserialize)]
struct Expected {
    document: String,
    recordings: Vec<Recording>,
}

#[derive(Debug, Deserialize)]
struct Recording {
    #[serde(rename = "recording")]
    file: String,
    verdicts: BTreeMap<String, Status>,
    adjudication: String,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pack() -> Expected {
    let text = std::fs::read_to_string(root().join("verification-pack/arazzo/expected.json"))
        .expect("committed expectations");
    serde_json::from_str(&text).expect("expectations parse")
}

/// Re-judges one recording with every workflow outside its verdicts skipped.
fn replay(document: &ArazzoDocument, recording: &Recording) -> Results {
    let mut plan = Plan::default();
    for workflow in &document.description().workflows {
        if !recording
            .verdicts
            .contains_key(&format!("ehr/{}", workflow.workflow_id))
        {
            plan.skips.insert(
                workflow.workflow_id.clone(),
                Skip {
                    reason: SkipReason::NotClaimed,
                    why: Why {
                        source: "verification-pack/arazzo/expected.json".to_owned(),
                        section: recording.file.clone(),
                        quote: "outside the recording".to_owned(),
                        link: None,
                        report: None,
                    },
                },
            );
        }
    }
    let har = Har::read(
        &root()
            .join("verification-pack/arazzo")
            .join(&recording.file),
    )
    .expect("committed recording");
    rejudge(document, "ehr", &root(), None, &plan, &har)
        .unwrap_or_else(|e| panic!("{}: {e}", recording.file))
}

#[test]
fn every_adjudicated_verdict_is_reproduced() {
    let expected = pack();
    let document = ArazzoDocument::load(&root().join(&expected.document)).unwrap();
    assert_eq!(expected.recordings.len(), 3);
    let mut seen = BTreeMap::new();
    for recording in &expected.recordings {
        assert!(!recording.adjudication.is_empty());
        let results = replay(&document, recording);
        for row in &results.rows {
            match recording.verdicts.get(&row.case) {
                Some(want) => {
                    assert_eq!(row.status, *want, "{} {}: {row}", recording.file, row.case);
                    *seen.entry(*want).or_insert(0) += 1;
                }
                None => assert_eq!(row.status, Status::Skipped, "{}", row.case),
            }
        }
        let again = replay(&document, recording);
        assert_eq!(again.to_text().unwrap(), results.to_text().unwrap());
    }
    for status in [Status::Passed, Status::Failed, Status::Errored] {
        assert!(seen.contains_key(&status), "the pack covers {status}");
    }
}

#[test]
fn a_tampered_verdict_is_caught() {
    let expected = pack();
    let document = ArazzoDocument::load(&root().join(&expected.document)).unwrap();
    let recording = expected
        .recordings
        .iter()
        .find(|r| r.file == "fail.har")
        .unwrap();
    let results = replay(&document, recording);
    let row = results
        .rows
        .iter()
        .find(|r| r.case == "ehr/create_ehr-same_ehr_twice-with_id")
        .unwrap();
    assert_ne!(
        row.status,
        Status::Passed,
        "a tampered `passed` would not match"
    );
    let why = row.explanation.as_ref().unwrap();
    assert_eq!(why.step, "create-again");
    assert_eq!(why.expected, "status 409");
    assert_eq!(why.received, "status 201");
}
