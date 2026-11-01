// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `dependsOn` cycle gate: no workflow depends on itself.
//!
//! Arazzo 1.1.0 §Workflow Object `dependsOn` lists the workflows that must
//! complete before this one runs, in its own document or as
//! `$sourceDescriptions.<name>.<workflowId>` of another. A workflow that
//! reaches itself through that list, directly or through other workflows and
//! other documents, can never start, and the evaluator would refuse it only
//! at run time. The gate refuses every workflow on such a cycle and names the
//! chain. It walks every document the catalogue's documents reach, so a cycle
//! among setups outside the catalogue is refused at the file that holds it.
//! No openEHR spec governs this — our own design.

use std::collections::BTreeMap;
use std::ops::ControlFlow;

use roas_arazzo::v1_1::Workflow;

use crate::arazzo::depends::{Library, Seen};
use crate::arazzo::document::ArazzoDocument;
use crate::arazzo::gates::{Gate, GateFinding, workflow_label};

/// Refuses every workflow that `documents`, or a document they reach,
/// holds and whose `dependsOn` leads back to itself.
///
/// `documents` are the catalogue's documents by id. A document a `dependsOn`
/// names outside them is loaded from its file and checked the same way.
#[must_use]
pub fn check(documents: &BTreeMap<&str, &ArazzoDocument>) -> Vec<GateFinding> {
    let library = Library::new(documents.values().copied());
    let mut findings = Vec::new();
    for document in library.documents() {
        for workflow in &document.description().workflows {
            if let Some(chain) = cycle(&library, document, workflow) {
                findings.push(GateFinding::new(
                    Gate::DependsOnCycle,
                    document.path(),
                    workflow.workflow_id.clone(),
                    format!(
                        "`{}` depends on itself through `dependsOn`: `{}`; a workflow on a \
                         cycle can never start",
                        workflow_label(document, workflow),
                        chain.join("` -> `"),
                    ),
                ));
            }
        }
    }
    findings
}

/// The chain `workflow` of `document` reaches itself through, as
/// `<document>/<workflowId>` labels from it back to it, or `None` when it
/// is on no cycle.
fn cycle(
    library: &Library<'_>,
    document: &ArazzoDocument,
    workflow: &Workflow,
) -> Option<Vec<String>> {
    let start = library.key(document.path());
    library.walk(&mut Seen::default(), document, workflow, &mut |visit| {
        let closes = visit.workflow.depends_on.iter().any(|entry| {
            // NOTE: no openEHR spec governs this — our own design; an entry that does not
            // resolve is the document gate's finding, so it closes no cycle here.
            library.resolve(visit.document, entry).is_ok_and(|target| {
                target.workflow.workflow_id == workflow.workflow_id
                    && library.key(target.document.path()) == start
            })
        });
        if !closes {
            return ControlFlow::Continue(());
        }
        let mut chain: Vec<String> = visit
            .path
            .iter()
            .map(|(document, workflow)| workflow_label(document, workflow))
            .collect();
        chain.push(workflow_label(visit.document, visit.workflow));
        chain.push(workflow_label(document, workflow));
        ControlFlow::Break(chain)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::gates::check_catalogue;
    use assert_fs::prelude::*;

    const OAS: &str = "openapi: 3.0.3\npaths:\n  /ehr:\n    post:\n      operationId: ehr_create\n";

    const CITE: &str =
        "{ source: specs/rest-oas/ehr-validation.openapi.yaml, section: openapi, quote: x }";

    /// One workflow: its id and its `dependsOn` list.
    fn workflow(id: &str, depends_on: &str) -> String {
        let depends_on = if depends_on.is_empty() {
            String::new()
        } else {
            format!("    dependsOn: [{depends_on}]\n")
        };
        format!(
            "  - workflowId: {id}\n{depends_on}    x-veredictum-cite: {CITE}\n    steps:\n      - stepId: create\n        operationId: $sourceDescriptions.ehr.ehr_create\n        successCriteria:\n          - condition: $statusCode == 201\n"
        )
    }

    /// A document over the seeded OAS whose `setup` source is `setup`.
    fn document(setup: &str, workflows: &[String]) -> String {
        format!(
            "arazzo: 1.1.0\ninfo: {{ title: t, version: '1' }}\nsourceDescriptions:\n  - {{ name: ehr, url: ../../../specs/rest-oas/ehr-validation.openapi.yaml, type: openapi }}\n  - {{ name: setup, url: {setup}, type: arazzo }}\nworkflows:\n{}",
            workflows.concat()
        )
    }

    /// A catalogue whose `_setup/setup` document holds `setups` beside a
    /// `plain` workflow and whose `ehr/ehr` document holds `cases`, each
    /// naming the other as `setup`.
    fn world(setups: &[String], cases: &[String]) -> assert_fs::TempDir {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("specs/rest-oas/ehr-validation.openapi.yaml")
            .write_str(OAS)
            .unwrap();
        dir.child("catalogue/cases/_setup/setup.arazzo.yaml")
            .write_str(&document(
                "../ehr/ehr.arazzo.yaml",
                &[setups, &[workflow("plain", "")]].concat(),
            ))
            .unwrap();
        dir.child("catalogue/cases/ehr/ehr.arazzo.yaml")
            .write_str(&document("../_setup/setup.arazzo.yaml", cases))
            .unwrap();
        dir
    }

    /// Every finding of the catalogue, as `(gate, location, message)`.
    fn findings(dir: &assert_fs::TempDir) -> Vec<(Gate, String, String)> {
        check_catalogue(&dir.path().join("catalogue"), None)
            .into_iter()
            .map(|f| (f.gate, f.location, f.message))
            .collect()
    }

    /// The cycle findings, as `(location, chain)`.
    fn cycles(dir: &assert_fs::TempDir) -> Vec<(String, String)> {
        let found = findings(dir);
        assert!(
            found.iter().all(|(gate, ..)| *gate == Gate::DependsOnCycle),
            "{found:?}"
        );
        found
            .into_iter()
            .map(|(_, at, message)| {
                let chain = message
                    .split_once(": ")
                    .and_then(|(_, rest)| rest.split_once(';'))
                    .map(|(chain, _)| chain.to_owned())
                    .unwrap();
                (at, chain)
            })
            .collect()
    }

    #[test]
    fn a_workflow_that_depends_on_itself_is_refused() {
        let dir = world(&[], &[workflow("a", "a")]);
        assert_eq!(
            cycles(&dir),
            [("a".to_owned(), "`ehr/a` -> `ehr/a`".to_owned())]
        );
        let message = &findings(&dir)[0].2;
        assert_eq!(
            message,
            "`ehr/a` depends on itself through `dependsOn`: `ehr/a` -> `ehr/a`; a workflow on a \
             cycle can never start"
        );
    }

    #[test]
    fn a_cycle_within_a_document_refuses_every_workflow_on_it() {
        let dir = world(
            &[],
            &[
                workflow("a", "b"),
                workflow("b", "a"),
                workflow("reaches", "a"),
            ],
        );
        assert_eq!(
            cycles(&dir),
            [
                ("a".to_owned(), "`ehr/a` -> `ehr/b` -> `ehr/a`".to_owned()),
                ("b".to_owned(), "`ehr/b` -> `ehr/a` -> `ehr/b`".to_owned()),
            ],
            "a workflow that only reaches the cycle is not on it"
        );
    }

    #[test]
    fn a_cycle_across_documents_is_refused_in_each() {
        let dir = world(
            &[workflow("s", "$sourceDescriptions.setup.a")],
            &[workflow("a", "$sourceDescriptions.setup.s")],
        );
        assert_eq!(
            cycles(&dir),
            [
                (
                    "s".to_owned(),
                    "`setup/s` -> `ehr/a` -> `setup/s`".to_owned()
                ),
                ("a".to_owned(), "`ehr/a` -> `setup/s` -> `ehr/a`".to_owned()),
            ]
        );
    }

    #[test]
    fn a_shared_setup_reached_twice_is_no_cycle() {
        let dir = world(
            &[workflow("s", ""), workflow("t", "s")],
            &[
                workflow("a", "b, $sourceDescriptions.setup.t"),
                workflow("b", "$sourceDescriptions.setup.s"),
            ],
        );
        assert_eq!(cycles(&dir), []);
    }

    #[test]
    fn a_cycle_outside_the_catalogue_is_refused_where_it_lives() {
        let dir = world(&[], &[]);
        dir.child("outside/far.arazzo.yaml")
            .write_str(
                &document(
                    "far.arazzo.yaml",
                    &[workflow("x", "$sourceDescriptions.setup.x")],
                )
                .replace("../../../specs", "../specs"),
            )
            .unwrap();
        dir.child("catalogue/cases/ehr/ehr.arazzo.yaml")
            .write_str(&document(
                "../../../outside/far.arazzo.yaml",
                &[workflow("a", "$sourceDescriptions.setup.x")],
            ))
            .unwrap();
        let found = findings(&dir);
        assert!(
            found
                .iter()
                .any(|(gate, at, message)| *gate == Gate::DependsOnCycle
                    && at == "x"
                    && message.contains("`far/x` -> `far/x`")),
            "{found:?}"
        );
        assert!(
            !found.iter().any(|(_, at, _)| at == "a"),
            "a case reaching the cycle is not on it: {found:?}"
        );
    }
}
