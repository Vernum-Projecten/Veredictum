// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Reading the Arazzo catalogue as a reader lists it: its gate findings, and
//! every capability folder with its documents and their workflows.
//!
//! Nothing here judges or runs anything. A second consumer (the web console)
//! renders the catalogue from this listing, so no consumer re-parses a
//! workflow document. No openEHR spec governs this — our own design.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::arazzo::capabilities::{CAPABILITIES_FILE, Capabilities};
use crate::arazzo::catalogue::document_paths;
use crate::arazzo::cite::{Citation, citation_of};
use crate::arazzo::document::ArazzoDocument;
use crate::arazzo::gates::{GateFinding, check_catalogue};
use crate::pipeline::Error;

/// One step of a listed workflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedStep {
    /// The step's id.
    pub step_id: String,
    /// The operation it calls, as the document names it.
    pub operation: Option<String>,
    /// The step's description.
    pub description: Option<String>,
}

/// One workflow of a listed document: one case of the catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedWorkflow {
    /// The case id, `<document>/<workflowId>`.
    pub case: String,
    /// The workflow's summary.
    pub summary: Option<String>,
    /// The workflow's description.
    pub description: Option<String>,
    /// The workflow's own citation, when it carries one.
    pub citation: Option<Citation>,
    /// The steps, in document order.
    pub steps: Vec<ListedStep>,
}

/// One Arazzo document of the catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedDocument {
    /// The document id, its file name without the `.arazzo.yaml` suffix.
    pub id: String,
    /// The capability folder the document is in.
    pub capability: String,
    /// The document file.
    pub path: PathBuf,
    /// The option arm whose documents name this one, as `(option, arm)`,
    /// when an option in `capabilities.yaml` names it.
    pub option_arm: Option<(String, String)>,
    /// The workflows, in document order.
    pub workflows: Vec<ListedWorkflow>,
}

/// The catalogue as read: its gate findings beside its listing.
#[derive(Debug, Clone)]
pub struct Listing {
    /// The catalogue root that was read.
    pub root: PathBuf,
    /// Every gate finding; zero is the only passing result.
    pub findings: Vec<GateFinding>,
    /// Every document, in path order, the shared setups left out.
    pub documents: Vec<ListedDocument>,
}

impl Listing {
    /// Whether the catalogue gates clean.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// The capability folders, each with its documents, in folder order.
    #[must_use]
    pub fn capabilities(&self) -> BTreeMap<&str, Vec<&ListedDocument>> {
        let mut folders: BTreeMap<&str, Vec<&ListedDocument>> = BTreeMap::new();
        for document in &self.documents {
            folders
                .entry(document.capability.as_str())
                .or_default()
                .push(document);
        }
        folders
    }

    /// Every listed workflow, in document order.
    pub fn workflows(&self) -> impl Iterator<Item = (&ListedDocument, &ListedWorkflow)> {
        self.documents
            .iter()
            .flat_map(|document| document.workflows.iter().map(move |w| (document, w)))
    }
}

/// Gates the catalogue under `root` and lists it.
///
/// A document that does not load is reported by the gates and listed with no
/// workflows, so a reader still sees where it sits.
///
/// # Errors
/// [`Error::ArazzoCatalogue`] when a capability folder does not list, and
/// [`Error::Parse`] when `capabilities.yaml` is present and does not load.
pub fn read(root: &Path, spec_root: Option<&Path>) -> Result<Listing, Error> {
    let findings = check_catalogue(root, spec_root);
    let capabilities_path = root.join(CAPABILITIES_FILE);
    let arms: BTreeMap<String, (String, String)> = if capabilities_path.is_file() {
        let capabilities =
            Capabilities::load(&capabilities_path).map_err(|error| Error::Parse {
                context: capabilities_path.display().to_string(),
                message: error.to_string(),
            })?;
        capabilities
            .arm_documents()
            .map(|(option, arm, document)| {
                (document.to_owned(), (option.to_owned(), arm.to_owned()))
            })
            .collect()
    } else {
        BTreeMap::new()
    };
    let mut documents = Vec::new();
    for found in document_paths(root)? {
        // NOTE: a document that does not load is a gate finding already, so
        // its absence from the listing's workflows loses nothing.
        let workflows = ArazzoDocument::load(&found.path)
            .map(|document| list_workflows(&found.id, &document))
            .unwrap_or_default();
        documents.push(ListedDocument {
            option_arm: arms.get(&found.id).cloned(),
            id: found.id,
            capability: found.capability,
            path: found.path,
            workflows,
        });
    }
    Ok(Listing {
        root: root.to_owned(),
        findings,
        documents,
    })
}

/// The workflows of one loaded document, as listed.
fn list_workflows(document_id: &str, document: &ArazzoDocument) -> Vec<ListedWorkflow> {
    document
        .description()
        .workflows
        .iter()
        .map(|workflow| ListedWorkflow {
            case: format!("{document_id}/{}", workflow.workflow_id),
            summary: workflow.summary.clone(),
            description: workflow.description.clone(),
            // NOTE: a malformed citation is a gate finding already; the
            // listing shows the workflow without one rather than hiding it.
            citation: citation_of(workflow.extensions.as_ref()).ok().flatten(),
            steps: workflow
                .steps
                .iter()
                .map(|step| ListedStep {
                    step_id: step.step_id.clone(),
                    operation: step.operation_id.clone(),
                    description: step.description.clone(),
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result-returning tests in the Book ch11 shape, each asserting; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::read;
    use std::path::Path;

    fn repo_root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    }

    #[test]
    fn the_committed_catalogue_lists_clean_with_every_workflow()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = repo_root().join("catalogue");
        let listing = read(&root, Some(&repo_root().join("specs/openehr")))?;

        assert!(listing.is_clean(), "{:?}", listing.findings.first());
        assert!(!listing.documents.is_empty());
        assert!(
            listing
                .documents
                .iter()
                .all(|document| !document.workflows.is_empty()),
            "every committed document lists at least one workflow"
        );
        let (document, workflow) = listing
            .workflows()
            .find(|(_, w)| w.case == "ehr/create_ehr-same_ehr_twice-with_id")
            .ok_or("the worked example of ARCHITECTURE §15.5 is listed")?;
        assert_eq!(document.capability, "ehr");
        assert!(!workflow.steps.is_empty());
        Ok(())
    }

    #[test]
    fn a_document_an_option_arm_names_carries_that_arm() -> Result<(), Box<dyn std::error::Error>> {
        let listing = read(&repo_root().join("catalogue"), None)?;
        let document = listing
            .documents
            .iter()
            .find(|document| document.id == "ehr_xml")
            .ok_or("ehr_xml is a committed document")?;
        assert_eq!(
            document.option_arm,
            Some(("ehr-xml-read".to_owned(), "ehr-xml-supported".to_owned()))
        );
        Ok(())
    }

    #[test]
    fn the_capabilities_group_documents_by_folder() -> Result<(), Box<dyn std::error::Error>> {
        let listing = read(&repo_root().join("catalogue"), None)?;
        let folders = listing.capabilities();
        assert!(folders.contains_key("ehr"));
        assert!(!folders.contains_key("_setup"));
        Ok(())
    }
}
