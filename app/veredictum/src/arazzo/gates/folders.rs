// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The gates over a document's needs and each capability folder's marks.
//!
//! `capabilities.yaml` states what a document needs before it runs, the
//! workflow count each capability folder keeps, and which folders are
//! verified only over the routes of Veredictum's own sources, the ones whose
//! `info` carries [`NON_OPENEHR_KEY`]. No openEHR spec governs these — our
//! own design. A floor makes coverage ratchet: a folder that loses a
//! workflow is refused until its floor is lowered in review. The realization scope keeps a capability verified only
//! over routes no openEHR OAS describes from reading as openEHR coverage.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use roas_arazzo::common::reusable::ReusableOr;
use roas_arazzo::v1_1::{ParameterLocation, ValueOrSelector, Workflow};
use serde_json::Value;

use crate::arazzo::capabilities::Capabilities;
use crate::arazzo::catalogue::SETUP_DIR;
use crate::arazzo::cite::{QuoteChecker, check_level};
use crate::arazzo::document::ArazzoDocument;
use crate::arazzo::gates::surface::NegotiationHeader;
use crate::arazzo::gates::{DOCUMENT_SUFFIX, Gate, GateFinding};
use crate::arazzo::source::{NON_OPENEHR_KEY, OperationIndex, split_qualified};
use crate::vocab::FormatName;

/// The capability folder a document sits in; `None` for a shared setup.
#[must_use]
pub fn folder_of(document: &ArazzoDocument) -> Option<&str> {
    let folder = document.path().parent()?.file_name()?.to_str()?;
    (folder != SETUP_DIR).then_some(folder)
}

/// The workflow count of every capability folder, by folder name.
#[must_use]
pub fn workflow_counts<'a>(
    documents: impl IntoIterator<Item = &'a ArazzoDocument>,
) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for document in documents {
        if let Some(folder) = folder_of(document) {
            let count = counts.entry(folder.to_owned()).or_default();
            *count = count.saturating_add(document.description().workflows.len());
        }
    }
    counts
}

/// Refuses a capability folder without a floor, a floor naming no folder,
/// and a folder holding fewer workflows than its floor.
#[must_use]
pub fn check_floors(
    path: &Path,
    capabilities: &Capabilities,
    counts: &BTreeMap<String, usize>,
) -> Vec<GateFinding> {
    let mut findings = Vec::new();
    for (folder, count) in counts {
        let at = format!("#/floors/{folder}");
        match capabilities.floors.get(folder) {
            None => findings.push(GateFinding::new(
                Gate::Floor,
                path,
                at,
                format!("the capability folder `{folder}` has no floor; it holds {count} workflow(s)"),
            )),
            Some(floor) if count < floor => findings.push(GateFinding::new(
                Gate::Floor,
                path,
                at,
                format!(
                    "the capability folder `{folder}` holds {count} workflow(s), below its floor of {floor}"
                ),
            )),
            Some(_) => {}
        }
    }
    for folder in capabilities.floors.keys() {
        if !counts.contains_key(folder) {
            findings.push(GateFinding::new(
                Gate::Floor,
                path,
                format!("#/floors/{folder}"),
                format!("`{folder}` is not a capability folder of the catalogue"),
            ));
        }
    }
    findings
}

/// Whether a step of `document` calls an operation of a source marked with
/// [`NON_OPENEHR_KEY`], and whether one calls an operation of another
/// source, as `(extension, other)`.
///
/// A source that did not index counts as another source: the document gate
/// already refuses it, and an unread mark cannot make a folder extension-only.
#[must_use]
pub fn realization(document: &ArazzoDocument) -> (bool, bool) {
    let (mut extension, mut other) = (false, false);
    for step in document
        .description()
        .workflows
        .iter()
        .flat_map(|workflow| &workflow.steps)
    {
        let Some((source, _)) = step.operation_id.as_deref().and_then(split_qualified) else {
            continue;
        };
        if document
            .source(source)
            .is_some_and(OperationIndex::is_non_openehr)
        {
            extension = true;
        } else {
            other = true;
        }
    }
    (extension, other)
}

/// Holds the `extension-only` list to the sources each folder's steps call.
///
/// It refuses a listed folder with a step calling an unmarked source, an
/// unlisted folder whose every step calls a source marked with
/// [`NON_OPENEHR_KEY`], and a listed name that is no folder.
#[must_use]
pub fn check_realization<'a>(
    path: &Path,
    capabilities: &Capabilities,
    documents: impl IntoIterator<Item = &'a ArazzoDocument>,
) -> Vec<GateFinding> {
    let mut scopes: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    for document in documents {
        if let Some(folder) = folder_of(document) {
            let (extension, other) = realization(document);
            let scope = scopes.entry(folder.to_owned()).or_default();
            scope.0 |= extension;
            scope.1 |= other;
        }
    }
    let mut findings = Vec::new();
    let refuse =
        |message: String| GateFinding::new(Gate::Realization, path, "#/extension-only", message);
    for (folder, (extension, other)) in &scopes {
        let only = *extension && !*other;
        let listed = capabilities.extension_only.contains(folder);
        if listed && !only {
            findings.push(refuse(format!(
                "`{folder}` is listed as extension-only, and a step in it calls a source without `info.{NON_OPENEHR_KEY}: true`"
            )));
        }
        if !listed && only {
            findings.push(refuse(format!(
                "every step in `{folder}` calls a source marked `info.{NON_OPENEHR_KEY}: true`, so it must be listed as extension-only"
            )));
        }
    }
    for folder in &capabilities.extension_only {
        if !scopes.contains_key(folder) {
            findings.push(refuse(format!(
                "`{folder}` is not a capability folder of the catalogue"
            )));
        }
    }
    findings
}

/// The media types the steps of `document` send in `Accept` or
/// `Content-Type`, each without its parameters.
#[must_use]
pub fn sent_media_types(document: &ArazzoDocument) -> BTreeSet<String> {
    document
        .description()
        .workflows
        .iter()
        .flat_map(workflow_media_types)
        .collect()
}

/// The media types the steps of `workflow` send in `Accept` or
/// `Content-Type`, each without its parameters.
fn workflow_media_types(workflow: &Workflow) -> BTreeSet<String> {
    let mut sent = BTreeSet::new();
    for step in &workflow.steps {
        for parameter in &step.parameters {
            if let ReusableOr::Item(parameter) = parameter
                && parameter.in_ == Some(ParameterLocation::Header)
                && matches!(
                    NegotiationHeader::parse(&parameter.name),
                    Some(NegotiationHeader::Accept | NegotiationHeader::ContentType)
                )
                && let ValueOrSelector::Literal(Value::String(value)) = &parameter.value
            {
                sent.extend(value.split(',').map(bare_media_type));
            }
        }
        if let Some(content_type) = step
            .request_body
            .as_ref()
            .and_then(|body| body.content_type.as_deref())
        {
            sent.insert(bare_media_type(content_type));
        }
    }
    sent
}

/// Whether a workflow of `document` sends the canonical JSON media type,
/// itself or through a workflow it names in `dependsOn`, resolved among
/// `documents` by document id.
#[must_use]
pub fn sends_canonical_json(
    document: &ArazzoDocument,
    documents: &BTreeMap<&str, &ArazzoDocument>,
) -> bool {
    let mut seen = BTreeSet::new();
    document
        .description()
        .workflows
        .iter()
        .any(|workflow| workflow_sends_json(document, workflow, documents, &mut seen))
}

/// Whether `workflow` of `document`, or a workflow its `dependsOn` names,
/// sends [`FormatName::CanonicalJson`]'s media type; `seen` holds every
/// workflow already visited, so a dependency cycle ends.
fn workflow_sends_json(
    document: &ArazzoDocument,
    workflow: &Workflow,
    documents: &BTreeMap<&str, &ArazzoDocument>,
    seen: &mut BTreeSet<(PathBuf, String)>,
) -> bool {
    if !seen.insert((document.path().to_owned(), workflow.workflow_id.clone())) {
        return false;
    }
    if workflow_media_types(workflow).contains(FormatName::CanonicalJson.media_type()) {
        return true;
    }
    workflow.depends_on.iter().any(|id| {
        // NOTE: a `dependsOn` entry that does not resolve is a finding of the
        // document gate already, so it is legitimately absent here.
        let resolved = match split_qualified(id) {
            None => Some((document, id.as_str())),
            Some((source, workflow_id)) => document
                .workflow_sources()
                .get(source)
                .and_then(|file| file.file_name()?.to_str()?.strip_suffix(DOCUMENT_SUFFIX))
                .and_then(|target| documents.get(target).copied())
                .map(|target| (target, workflow_id)),
        };
        resolved.is_some_and(|(target, workflow_id)| {
            target
                .workflow(workflow_id)
                .is_some_and(|dependency| workflow_sends_json(target, dependency, documents, seen))
        })
    })
}

/// A media type without its parameters, trimmed and lower-cased.
fn bare_media_type(value: &str) -> String {
    value
        .split_once(';')
        .map_or(value, |(bare, _)| bare)
        .trim()
        .to_ascii_lowercase()
}

/// Holds the `documents` needs to the documents they name.
///
/// It refuses an entry naming no document, a range whose `min` lies above its
/// `max`, a format the document never names in `Accept` or `Content-Type`,
/// and a capability document that sends the canonical JSON media type,
/// itself or through a `dependsOn` setup, without stating `canonical-json`.
///
/// The `canonical-json` rule has no XML twin: a document pinning an XML
/// refusal (406, 415) sends `application/xml` and runs for every party, while
/// no document pins a JSON refusal. No openEHR spec governs this — our own
/// design.
#[must_use]
pub fn check_needs(
    path: &Path,
    capabilities: &Capabilities,
    documents: &BTreeMap<&str, &ArazzoDocument>,
) -> Vec<GateFinding> {
    let mut findings = Vec::new();
    for (id, needs) in &capabilities.documents {
        let at = format!("#/documents/{id}");
        let Some(document) = documents.get(id.as_str()) else {
            findings.push(GateFinding::new(
                Gate::Identity,
                path,
                at,
                format!("`{id}` is not a document of the catalogue"),
            ));
            continue;
        };
        for (component, range) in &needs.applies {
            if let (Some(min), Some(max)) = (range.min, range.max)
                && min > max
            {
                findings.push(GateFinding::new(
                    Gate::Identity,
                    path,
                    format!("{at}/applies/{}", component.token()),
                    format!("the range starts at {min}, after its end {max}"),
                ));
            }
        }
        let sent = sent_media_types(document);
        for format in &needs.formats {
            let sends = match format {
                FormatName::CanonicalJson => sends_canonical_json(document, documents),
                _ => sent.contains(format.media_type()),
            };
            if !sends {
                findings.push(GateFinding::new(
                    Gate::Identity,
                    path,
                    format!("{at}/formats"),
                    format!(
                        "`{id}` needs `{}`, and no step of it sends `{}` in Accept or Content-Type",
                        format.token(),
                        format.media_type()
                    ),
                ));
            }
        }
    }
    for (id, document) in documents {
        if folder_of(document).is_none() || !sends_canonical_json(document, documents) {
            continue;
        }
        let states = capabilities
            .documents
            .get(*id)
            .is_some_and(|needs| needs.formats.contains(&FormatName::CanonicalJson));
        if !states {
            findings.push(GateFinding::new(
                Gate::Identity,
                path,
                format!("#/documents/{id}/formats"),
                format!(
                    "`{id}` sends `{}`, itself or through a `dependsOn` setup, and does not state `{}`",
                    FormatName::CanonicalJson.media_type(),
                    FormatName::CanonicalJson.token()
                ),
            ));
        }
    }
    findings
}
/// Refuses an `applies` range whose citation's `level` disagrees with its
/// quote, or whose quote does not occur in its source when `quotes` checks
/// the vendored files.
#[must_use]
pub fn check_range_cites(
    path: &Path,
    capabilities: &Capabilities,
    mut quotes: Option<&mut QuoteChecker<'_>>,
) -> Vec<GateFinding> {
    let mut findings = Vec::new();
    for (id, needs) in &capabilities.documents {
        for (component, range) in &needs.applies {
            let at = format!("#/documents/{id}/applies/{}", component.token());
            if let Err(error) = check_level(&range.cite) {
                findings.push(GateFinding::new(Gate::Level, path, &at, error.to_string()));
            }
            if let Some(quotes) = quotes.as_deref_mut()
                && let Err(error) = quotes.check(&range.cite)
            {
                findings.push(GateFinding::new(
                    Gate::Citation,
                    path,
                    &at,
                    error.to_string(),
                ));
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document in `cases/<folder>/` whose one step calls `operation` of a
    /// source at `url`, sending `accept`; `url` resolves from the repository
    /// catalogue, so `folder` names a folder it holds.
    fn document(folder: &str, id: &str, url: &str, accept: &str) -> ArazzoDocument {
        let raw = serde_json::json!({
            "arazzo": "1.1.0",
            "info": { "title": "t", "version": "1" },
            "sourceDescriptions": [{ "name": "s", "url": url, "type": "openapi" }],
            "workflows": [{
                "workflowId": "w",
                "steps": [{
                    "stepId": "call",
                    "operationId": "$sourceDescriptions.s.op",
                    "parameters": [{ "name": "Accept", "in": "header", "value": accept }],
                    "successCriteria": [{ "condition": "$statusCode == 200" }]
                }]
            }]
        });
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../catalogue/cases")
            .join(folder)
            .join(format!("{id}.arazzo.yaml"));
        ArazzoDocument::from_value(&path, raw).unwrap().0
    }

    const RELEASED: &str = "../../../specs/rest-oas/ehr-validation.openapi.yaml";
    const EXTENSION: &str = "../../sources/veredictum-probes.openapi.yaml";

    fn capabilities(yaml: Value) -> Capabilities {
        serde_json::from_value(yaml).unwrap()
    }

    fn gates(findings: &[GateFinding]) -> Vec<Gate> {
        findings.iter().map(|f| f.gate).collect()
    }

    #[test]
    fn a_folder_is_held_to_its_floor_and_every_floor_to_a_folder() {
        let docs = [
            document("ehr", "a", RELEASED, "application/json"),
            document("ehr", "b", RELEASED, "application/json"),
            document("_setup", "s", RELEASED, "application/json"),
        ];
        let counts = workflow_counts(&docs);
        assert_eq!(counts, BTreeMap::from([("ehr".to_owned(), 2)]));
        let path = Path::new("capabilities.yaml");
        let at = |floors: Value| {
            check_floors(
                path,
                &capabilities(serde_json::json!({ "floors": floors })),
                &counts,
            )
        };
        assert!(at(serde_json::json!({ "ehr": 2 })).is_empty());
        assert!(at(serde_json::json!({ "ehr": 1 })).is_empty());
        assert_eq!(gates(&at(serde_json::json!({ "ehr": 3 }))), [Gate::Floor]);
        assert_eq!(gates(&at(serde_json::json!({}))), [Gate::Floor]);
        assert_eq!(
            gates(&at(serde_json::json!({ "ehr": 2, "gone": 1 }))),
            [Gate::Floor]
        );
    }

    #[test]
    fn the_extension_only_list_agrees_with_the_sources_in_both_directions() {
        let path = Path::new("capabilities.yaml");
        let listed = capabilities(serde_json::json!({ "extension-only": ["security"] }));
        let unlisted = capabilities(serde_json::json!({}));
        let only = [document("security", "a", EXTENSION, "application/json")];
        let mixed = [
            document("security", "a", EXTENSION, "application/json"),
            document("security", "b", RELEASED, "application/json"),
        ];
        assert_eq!(only.first().map(realization), Some((true, false)));
        assert!(check_realization(path, &listed, &only).is_empty());
        assert!(check_realization(path, &unlisted, &mixed).is_empty());
        assert_eq!(
            gates(&check_realization(path, &unlisted, &only)),
            [Gate::Realization]
        );
        assert_eq!(
            gates(&check_realization(path, &listed, &mixed)),
            [Gate::Realization]
        );
        let elsewhere = [document("ehr", "a", RELEASED, "application/json")];
        assert_eq!(
            gates(&check_realization(path, &listed, &elsewhere)),
            [Gate::Realization]
        );
    }

    #[test]
    fn a_document_need_names_a_document_an_ordered_range_and_a_sent_format() {
        // A shared setup, so the canonical-json rule, which holds capability
        // documents only, adds nothing to what this test pins.
        let xml = document(
            "_setup",
            "xml",
            RELEASED,
            "application/xml; q=0.9, application/json",
        );
        let docs = BTreeMap::from([("xml", &xml)]);
        let path = Path::new("capabilities.yaml");
        let cite = serde_json::json!({ "source": "s", "section": "t", "quote": "q" });
        let needs = |documents: Value| {
            check_needs(
                path,
                &capabilities(serde_json::json!({ "documents": documents })),
                &docs,
            )
        };
        assert!(
            needs(serde_json::json!({ "xml": { "formats": ["canonical-xml", "canonical-json"] } }))
                .is_empty()
        );
        assert_eq!(
            gates(&needs(
                serde_json::json!({ "xml": { "formats": ["wt-flat"] } })
            )),
            [Gate::Identity]
        );
        assert_eq!(
            gates(&needs(
                serde_json::json!({ "absent": { "formats": ["canonical-xml"] } })
            )),
            [Gate::Identity]
        );
        assert_eq!(
            gates(&needs(
                serde_json::json!({ "xml": { "applies": { "its_rest": {
                "min": "1.1.0", "max": "1.0.2", "x-veredictum-cite": cite
            } } } })
            )),
            [Gate::Identity]
        );
    }

    #[test]
    fn a_capability_document_sending_json_states_canonical_json() {
        let json = document("ehr", "json", RELEASED, "application/json");
        let xml = document("ehr", "xml", RELEASED, "application/xml");
        let shared = document("_setup", "shared", RELEASED, "application/json");
        let docs = BTreeMap::from([("json", &json), ("xml", &xml), ("shared", &shared)]);
        let path = Path::new("capabilities.yaml");
        let needs = |documents: Value| {
            check_needs(
                path,
                &capabilities(serde_json::json!({ "documents": documents })),
                &docs,
            )
        };
        assert!(sends_canonical_json(&json, &docs));
        assert!(!sends_canonical_json(&xml, &docs));
        assert!(needs(serde_json::json!({ "json": { "formats": ["canonical-json"] } })).is_empty());
        let unstated = needs(serde_json::json!({}));
        assert_eq!(gates(&unstated), [Gate::Identity]);
        assert!(
            unstated
                .first()
                .is_some_and(|f| f.location == "#/documents/json/formats"),
            "{unstated:?}"
        );
        assert_eq!(
            gates(&needs(
                serde_json::json!({ "json": { "formats": ["canonical-xml"] } })
            )),
            [Gate::Identity, Gate::Identity]
        );
        assert_eq!(
            gates(&needs(
                serde_json::json!({ "json": { "formats": ["canonical-json"] }, "xml": { "formats": ["canonical-json"] } })
            )),
            [Gate::Identity]
        );
    }
}
