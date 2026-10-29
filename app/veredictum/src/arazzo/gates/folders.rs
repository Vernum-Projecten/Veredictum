// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The gates over a document's needs and each capability folder's marks.
//!
//! `capabilities.yaml` states what a document needs before it runs, the
//! workflow count each capability folder keeps, and which folders are
//! verified only over Veredictum's own extension routes. No openEHR spec governs these — our own design. A floor makes coverage
//! ratchet: a folder that loses a workflow is refused until its floor is
//! lowered in review. The realization scope keeps a capability verified only
//! over routes no openEHR OAS describes from reading as openEHR coverage.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use roas_arazzo::common::reusable::ReusableOr;
use roas_arazzo::v1_1::{ParameterLocation, ValueOrSelector};
use serde_json::Value;

use crate::arazzo::capabilities::{Capabilities, EXTENSIONS_SOURCE};
use crate::arazzo::catalogue::SETUP_DIR;
use crate::arazzo::cite::{QuoteChecker, check_level};
use crate::arazzo::document::ArazzoDocument;
use crate::arazzo::gates::surface::NegotiationHeader;
use crate::arazzo::gates::{Gate, GateFinding};
use crate::arazzo::source::split_qualified;

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

/// Whether a step of `document` calls an operation of
/// [`EXTENSIONS_SOURCE`], and whether one calls an operation of another
/// source, as `(extension, other)`.
#[must_use]
pub fn realization(document: &ArazzoDocument) -> (bool, bool) {
    let urls: BTreeMap<&str, &str> = document
        .description()
        .source_descriptions
        .iter()
        .map(|source| (source.name.as_str(), source.url.as_str()))
        .collect();
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
        let is_extension = urls.get(source).is_some_and(|url| {
            Path::new(url)
                .file_name()
                .is_some_and(|name| name == EXTENSIONS_SOURCE)
        });
        if is_extension {
            extension = true;
        } else {
            other = true;
        }
    }
    (extension, other)
}

/// Refuses a folder listed as extension-only with a step calling another
/// source, a folder whose every step calls [`EXTENSIONS_SOURCE`] and that
/// is not listed, and a listed name that is no folder.
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
                "`{folder}` is listed as extension-only, and a step in it calls a source other than {EXTENSIONS_SOURCE}"
            )));
        }
        if !listed && only {
            findings.push(refuse(format!(
                "every step in `{folder}` calls {EXTENSIONS_SOURCE}, so it must be listed as extension-only"
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
    let mut sent = BTreeSet::new();
    for step in document
        .description()
        .workflows
        .iter()
        .flat_map(|workflow| &workflow.steps)
    {
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

/// A media type without its parameters, trimmed and lower-cased.
fn bare_media_type(value: &str) -> String {
    value
        .split_once(';')
        .map_or(value, |(bare, _)| bare)
        .trim()
        .to_ascii_lowercase()
}

/// Refuses a `documents` entry naming no document, a range whose `min` lies
/// above its `max`, and a format the document never names in `Accept` or
/// `Content-Type`.
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
            if !sent.contains(format.media_type()) {
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
    /// source at `url`, sending `accept`; built without resolving its source.
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
        let path = Path::new("catalogue/cases")
            .join(folder)
            .join(format!("{id}.arazzo.yaml"));
        ArazzoDocument::from_value(&path, raw).unwrap().0
    }

    const RELEASED: &str = "../../../specs/rest-oas/ehr-validation.openapi.yaml";
    const EXTENSION: &str = "../../sources/veredictum-extensions.openapi.yaml";

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
        let listed = capabilities(serde_json::json!({ "extension-only": ["ext"] }));
        let unlisted = capabilities(serde_json::json!({}));
        let only = [document("ext", "a", EXTENSION, "application/json")];
        let mixed = [
            document("ext", "a", EXTENSION, "application/json"),
            document("ext", "b", RELEASED, "application/json"),
        ];
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
        let xml = document(
            "ehr",
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
}
