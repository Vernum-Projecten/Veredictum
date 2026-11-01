// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The development-setup gate: a workflow that rests on released text never
//! depends on a development-only workflow.
//!
//! A development-only workflow ([`structure::development_citation`]) is
//! skipped under the released profile, while the setups a case names in
//! `dependsOn` run whatever they cite. A released case that depends on one,
//! directly or through other setups, in its own document or in a
//! `$sourceDescriptions.<name>.<workflowId>` of another (Arazzo 1.1.0
//! §Workflow Object `dependsOn`), would give a released verdict that rests on
//! development text, so the gate refuses the pairing. A development-only
//! workflow may depend on anything. No openEHR spec governs this — our own
//! design, held to hard rule 1: a development result never counts toward a
//! released one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use roas_arazzo::v1_1::Workflow;

use crate::arazzo::cite::Citation;
use crate::arazzo::document::ArazzoDocument;
use crate::arazzo::gates::{Gate, GateFinding, document_id};
use crate::arazzo::source::split_qualified;
use crate::arazzo::structure;

/// Refuses every workflow of `documents` that is not development-only and
/// depends, at any depth, on a development-only workflow.
///
/// `documents` are the catalogue's documents by id. A setup document a
/// `dependsOn` names outside them is loaded from its file, so no dependency
/// escapes the gate by where it lives.
#[must_use]
pub fn check(documents: &BTreeMap<&str, &ArazzoDocument>) -> Vec<GateFinding> {
    let mut index: BTreeMap<PathBuf, &ArazzoDocument> = BTreeMap::new();
    for document in documents.values() {
        if let Some(key) = key_of(document.path()) {
            index.insert(key, document);
        }
    }
    let known: BTreeSet<PathBuf> = index.keys().cloned().collect();
    let externals = load_externals(documents.values().copied(), &known);
    for (key, document) in &externals {
        index.entry(key.clone()).or_insert(document);
    }
    let mut findings = Vec::new();
    for document in documents.values() {
        for workflow in &document.description().workflows {
            if structure::development_citation(workflow).is_some() {
                continue;
            }
            let mut seen = BTreeSet::new();
            if let Some(reached) = reach(&index, document, workflow, &mut seen) {
                findings.push(finding(document, workflow, &reached));
            }
        }
    }
    findings
}

/// The development-only workflow a released workflow reaches, with the
/// setups it reaches it through.
struct Reached {
    /// The setups between the two, `<document>/<workflowId>`, nearest first.
    through: Vec<String>,
    /// The development-only workflow, `<document>/<workflowId>`.
    setup: String,
    /// The development-only workflow's first development-only citation.
    citation: Citation,
}

/// The first development-only workflow `workflow` of `document` reaches
/// through `dependsOn`, depth first in `dependsOn` order; `seen` holds every
/// workflow already visited, so a dependency cycle ends.
fn reach(
    index: &BTreeMap<PathBuf, &ArazzoDocument>,
    document: &ArazzoDocument,
    workflow: &Workflow,
    seen: &mut BTreeSet<(PathBuf, String)>,
) -> Option<Reached> {
    for id in &workflow.depends_on {
        // NOTE: a `dependsOn` entry that does not resolve is a finding of the
        // document gate already, so it is legitimately absent here.
        let Some((target, dependency)) = resolve(index, document, id) else {
            continue;
        };
        if !seen.insert((target.path().to_owned(), dependency.workflow_id.clone())) {
            continue;
        }
        let label = label(target, dependency);
        if let Some(citation) = structure::development_citation(dependency) {
            return Some(Reached {
                through: Vec::new(),
                setup: label,
                citation,
            });
        }
        if let Some(mut reached) = reach(index, target, dependency, seen) {
            reached.through.insert(0, label);
            return Some(reached);
        }
    }
    None
}

/// The document and workflow a `dependsOn` entry of `document` names: a
/// workflow of `document`, or `$sourceDescriptions.<name>.<workflowId>` of
/// a `type: arazzo` source (Arazzo 1.1.0 §Workflow Object `dependsOn`).
fn resolve<'a>(
    index: &BTreeMap<PathBuf, &'a ArazzoDocument>,
    document: &'a ArazzoDocument,
    id: &str,
) -> Option<(&'a ArazzoDocument, &'a Workflow)> {
    let Some((source, workflow_id)) = split_qualified(id) else {
        return Some((document, document.workflow(id)?));
    };
    let key = key_of(document.workflow_sources().get(source)?)?;
    let target = index.get(&key).copied()?;
    Some((target, target.workflow(workflow_id)?))
}

/// Loads every `type: arazzo` source a `dependsOn` names, at any depth, that
/// is not among `known`, keyed by its canonical path.
fn load_externals<'a>(
    documents: impl IntoIterator<Item = &'a ArazzoDocument>,
    known: &BTreeSet<PathBuf>,
) -> BTreeMap<PathBuf, ArazzoDocument> {
    let mut loaded = BTreeMap::new();
    let mut pending: Vec<PathBuf> = documents.into_iter().flat_map(setup_files).collect();
    while let Some(file) = pending.pop() {
        let Some(key) = key_of(&file) else {
            continue;
        };
        if known.contains(&key) || loaded.contains_key(&key) {
            continue;
        }
        // NOTE: a setup document that does not load is a finding of the
        // document gate already, so it is legitimately absent here.
        let Ok(document) = ArazzoDocument::load(&file) else {
            continue;
        };
        pending.extend(setup_files(&document));
        loaded.insert(key, document);
    }
    loaded
}

/// The files of the `type: arazzo` sources the `dependsOn` entries of
/// `document` name.
fn setup_files(document: &ArazzoDocument) -> Vec<PathBuf> {
    document
        .description()
        .workflows
        .iter()
        .flat_map(|workflow| &workflow.depends_on)
        .filter_map(|id| split_qualified(id))
        .filter_map(|(source, _)| document.workflow_sources().get(source).cloned())
        .collect()
}

/// The canonical path of `path`, so a source reached through `../` and the
/// same document listed under the catalogue are one key.
fn key_of(path: &Path) -> Option<PathBuf> {
    // NOTE: a path that does not resolve names no document on disk, which the
    // load and document gates report, so it is legitimately absent here.
    std::fs::canonicalize(path).ok()
}

/// `<document>/<workflowId>`, the way a case id names a workflow.
fn label(document: &ArazzoDocument, workflow: &Workflow) -> String {
    let path = document.path();
    let id = document_id(path).unwrap_or_else(|| path.display().to_string());
    format!("{id}/{}", workflow.workflow_id)
}

/// The finding refusing `workflow` of `document` for what it reaches.
fn finding(document: &ArazzoDocument, workflow: &Workflow, reached: &Reached) -> GateFinding {
    let through = if reached.through.is_empty() {
        String::new()
    } else {
        format!(" through `{}`", reached.through.join("`, `"))
    };
    GateFinding::new(
        Gate::DevelopmentSetup,
        document.path(),
        workflow.workflow_id.clone(),
        format!(
            "`{}` rests on released text and depends{through} on the development-only `{}`, \
             which cites `{}` ({}); a released verdict never rests on development text",
            label(document, workflow),
            reached.setup,
            reached.citation.source,
            reached.citation.section,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::gates::check_catalogue;
    use assert_fs::prelude::*;

    const OAS: &str = "openapi: 3.0.3\npaths:\n  /ehr:\n    post:\n      operationId: ehr_create\n";

    const RELEASED: &str =
        "{ source: specs/rest-oas/ehr-validation.openapi.yaml, section: openapi, quote: x }";

    const DEVELOPMENT: &str = "{ source: specs/openehr-development/specifications-ITS-REST/docs/ehr.adoc, section: EHR, quote: x }";

    /// One workflow: its id, its `dependsOn` list and its citation.
    fn workflow(id: &str, depends_on: &str, cite: &str) -> String {
        let depends_on = if depends_on.is_empty() {
            String::new()
        } else {
            format!("    dependsOn: [{depends_on}]\n")
        };
        format!(
            "  - workflowId: {id}\n{depends_on}    x-veredictum-cite: {cite}\n    steps:\n      - stepId: create\n        operationId: $sourceDescriptions.ehr.ehr_create\n        successCriteria:\n          - condition: $statusCode == 201\n"
        )
    }

    /// A document over the seeded OAS and the shared setup document.
    fn document(workflows: &[String]) -> String {
        format!(
            "arazzo: 1.1.0\ninfo: {{ title: t, version: '1' }}\nsourceDescriptions:\n  - {{ name: ehr, url: ../../../specs/rest-oas/ehr-validation.openapi.yaml, type: openapi }}\n  - {{ name: setup, url: ../_setup/setup.arazzo.yaml, type: arazzo }}\nworkflows:\n{}",
            workflows.concat()
        )
    }

    /// A catalogue whose shared setups hold a development-only setup, a
    /// released one depending on it and a released one depending on
    /// nothing, and whose `ehr` document holds `cases`.
    fn world(cases: &[String]) -> assert_fs::TempDir {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("specs/rest-oas/ehr-validation.openapi.yaml")
            .write_str(OAS)
            .unwrap();
        dir.child("catalogue/cases/_setup/setup.arazzo.yaml")
            .write_str(&document(&[
                workflow("development", "", DEVELOPMENT),
                workflow("through", "development", RELEASED),
                workflow("plain", "", RELEASED),
            ]))
            .unwrap();
        dir.child("catalogue/cases/ehr/ehr.arazzo.yaml")
            .write_str(&document(cases))
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

    #[test]
    fn a_released_case_on_a_development_only_setup_is_refused_directly_and_transitively() {
        let dir = world(&[
            workflow("direct", "$sourceDescriptions.setup.development", RELEASED),
            workflow("transitive", "$sourceDescriptions.setup.through", RELEASED),
            workflow("local_development", "", DEVELOPMENT),
            workflow("local", "local_development", RELEASED),
        ]);
        let found = findings(&dir);
        assert!(
            found
                .iter()
                .all(|(gate, ..)| *gate == Gate::DevelopmentSetup),
            "{found:?}"
        );
        let at: Vec<&str> = found.iter().map(|(_, at, _)| at.as_str()).collect();
        assert_eq!(at, ["through", "direct", "local", "transitive"]);
        let message = |location: &str| {
            found
                .iter()
                .find(|(_, at, _)| at == location)
                .map(|(.., message)| message.clone())
                .unwrap()
        };
        let direct = message("direct");
        assert!(direct.contains("`ehr/direct`"), "{direct}");
        assert!(
            direct.contains("development-only `setup/development`"),
            "{direct}"
        );
        assert!(
            direct.contains(
                "`specs/openehr-development/specifications-ITS-REST/docs/ehr.adoc` (EHR)"
            ),
            "{direct}"
        );
        let transitive = message("transitive");
        assert!(
            transitive.contains(
                "depends through `setup/through` on the development-only `setup/development`"
            ),
            "{transitive}"
        );
        let local = message("local");
        assert!(
            local.contains("development-only `ehr/local_development`"),
            "{local}"
        );
    }

    #[test]
    fn a_development_only_case_may_depend_on_a_development_only_setup() {
        let dir = world(&[
            workflow(
                "development_case",
                "$sourceDescriptions.setup.development",
                DEVELOPMENT,
            ),
            workflow(
                "development_through",
                "$sourceDescriptions.setup.through",
                DEVELOPMENT,
            ),
            workflow("released", "$sourceDescriptions.setup.plain", RELEASED),
        ]);
        let found = findings(&dir);
        assert_eq!(
            found
                .iter()
                .map(|(_, at, _)| at.as_str())
                .collect::<Vec<_>>(),
            ["through"],
            "only the shared released setup on the development-only one is refused: {found:?}"
        );
    }

    #[test]
    fn a_dependency_cycle_ends() {
        let dir = world(&[workflow("a", "b", RELEASED), workflow("b", "a", RELEASED)]);
        let found = findings(&dir);
        assert!(
            found
                .iter()
                .all(|(gate, at, _)| *gate != Gate::DevelopmentSetup || at == "through"),
            "{found:?}"
        );
    }

    #[test]
    fn a_setup_outside_the_catalogue_is_followed() {
        let dir = world(&[]);
        dir.child("outside/setup.arazzo.yaml")
            .write_str(
                &document(&[workflow("development", "", DEVELOPMENT)])
                    .replace("../../../specs", "../specs")
                    .replace("../_setup/setup.arazzo.yaml", "setup.arazzo.yaml"),
            )
            .unwrap();
        dir.child("catalogue/cases/ehr/ehr.arazzo.yaml")
            .write_str(
                &document(&[workflow(
                    "far",
                    "$sourceDescriptions.setup.development",
                    RELEASED,
                )])
                .replace(
                    "../_setup/setup.arazzo.yaml",
                    "../../../outside/setup.arazzo.yaml",
                ),
            )
            .unwrap();
        let found = findings(&dir);
        assert!(
            found
                .iter()
                .any(|(gate, at, message)| *gate == Gate::DevelopmentSetup
                    && at == "far"
                    && message.contains("`setup/development`")),
            "{found:?}"
        );
    }
}
