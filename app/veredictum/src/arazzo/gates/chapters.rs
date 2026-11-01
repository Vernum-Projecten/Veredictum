// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The structure gate over `chapters.yaml` and `reports.yaml`.
//!
//! Every capability folder sits in exactly one chapter, every BASE document
//! in one declared section of its folder, every option is placed, and every
//! cited upstream report is in the committed snapshot.
//!
//! BASE and OPTION are derived from `capabilities.yaml`, so the gate reads
//! the arms and never a second declaration, under the released profile the
//! catalogue is gated against. No openEHR spec governs the structure — our
//! own design.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::path::Path;

use crate::arazzo::capabilities::{CAPABILITIES_FILE, Capabilities};
use crate::arazzo::document::ArazzoDocument;
use crate::arazzo::gates::folders::folder_of;
use crate::arazzo::gates::{Gate, GateFinding};
use crate::arazzo::maturity::{Maturities, MaturityError};
use crate::arazzo::spec_profile::SpecProfile;
use crate::arazzo::structure::{
    CHAPTERS_FILE, Chapters, Formalisms, REPORTS_FILE, Reports, SECTION_KEY, Tier, arms_of,
    section_of, workflow_citations,
};

/// Runs the structure gate over the catalogue under `catalogue_root`, whose
/// documents are `documents` by id; nothing runs without a structure file.
///
/// `repo_root`, when supplied, is where the cited files' release statuses are
/// read, and a cited file in no vendored component with a manifest is a
/// finding.
#[must_use]
pub fn check(
    catalogue_root: &Path,
    documents: &BTreeMap<&str, &ArazzoDocument>,
    capabilities: Option<&Capabilities>,
    repo_root: Option<&Path>,
) -> Vec<GateFinding> {
    let path = catalogue_root.join(CHAPTERS_FILE);
    if !path.is_file() {
        return Vec::new();
    }
    let mut findings = Vec::new();
    let chapters = match Chapters::load(&path) {
        Ok(chapters) => chapters,
        Err(error) => {
            return vec![GateFinding::new(Gate::Load, &path, "#", error.to_string())];
        }
    };
    check_folders(&path, &chapters, documents, &mut findings);
    let released = capabilities.map(|c| c.in_profile(SpecProfile::Released));
    check_sections(
        &path,
        &chapters,
        documents,
        released.as_ref(),
        &mut findings,
    );
    check_options(
        &path,
        &chapters,
        capabilities,
        catalogue_root,
        &mut findings,
    );
    check_formalisms(&chapters, documents, &mut findings);
    if let Some(repo_root) = repo_root {
        check_maturities(documents, repo_root, &mut findings);
    }
    check_reports(catalogue_root, documents, capabilities, &mut findings);
    findings
}

/// Every capability folder in exactly one chapter, and every listed folder a
/// capability folder; chapter keys unique.
fn check_folders(
    path: &Path,
    chapters: &Chapters,
    documents: &BTreeMap<&str, &ArazzoDocument>,
    findings: &mut Vec<GateFinding>,
) {
    let folders: BTreeSet<&str> = documents.values().filter_map(|d| folder_of(d)).collect();
    let mut listed: BTreeMap<&str, usize> = BTreeMap::new();
    let mut keys = BTreeSet::new();
    for (c, chapter) in chapters.chapters.iter().enumerate() {
        if !keys.insert(chapter.chapter.as_str()) {
            findings.push(GateFinding::new(
                Gate::Structure,
                path,
                format!("#/chapters/{c}"),
                format!("the chapter `{}` is declared twice", chapter.chapter),
            ));
        }
        for (f, folder) in chapter.folders.iter().enumerate() {
            *listed.entry(folder.folder.as_str()).or_default() += 1;
            if !folders.contains(folder.folder.as_str()) {
                findings.push(GateFinding::new(
                    Gate::Structure,
                    path,
                    format!("#/chapters/{c}/folders/{f}"),
                    format!(
                        "`{}` is not a capability folder of the catalogue",
                        folder.folder
                    ),
                ));
            }
            let mut sections = BTreeSet::new();
            for section in &folder.sections {
                if !sections.insert(section.section.as_str()) {
                    findings.push(GateFinding::new(
                        Gate::Structure,
                        path,
                        format!("#/chapters/{c}/folders/{f}"),
                        format!(
                            "the section `{}` of `{}` is declared twice",
                            section.section, folder.folder
                        ),
                    ));
                }
            }
        }
    }
    for folder in folders {
        match listed.get(folder).copied().unwrap_or_default() {
            1 => {}
            0 => findings.push(GateFinding::new(
                Gate::Structure,
                path,
                "#/chapters",
                format!("the capability folder `{folder}` is in no chapter"),
            )),
            n => findings.push(GateFinding::new(
                Gate::Structure,
                path,
                "#/chapters",
                format!("the capability folder `{folder}` is in {n} chapters"),
            )),
        }
    }
}

/// Every BASE document in one declared section of its folder, no OPTION or
/// setup document in any, and every declared section in use.
///
/// `capabilities` is the released view ([`Capabilities::in_profile`]), so a
/// document only an option of the development profile names is BASE here
/// and keeps the section a released run places it by.
fn check_sections(
    path: &Path,
    chapters: &Chapters,
    documents: &BTreeMap<&str, &ArazzoDocument>,
    capabilities: Option<&Capabilities>,
    findings: &mut Vec<GateFinding>,
) {
    let mut used: BTreeSet<(&str, &str)> = BTreeSet::new();
    for (id, document) in documents {
        let section = section_of(document);
        let at = format!("#/info/{SECTION_KEY}");
        let refuse = |message: String| {
            GateFinding::new(Gate::Structure, document.path(), at.clone(), message)
        };
        let Some(folder) = folder_of(document) else {
            if section.is_some() {
                findings.push(refuse(
                    "a shared setup is no case and names no section".to_owned(),
                ));
            }
            continue;
        };
        let armed = !arms_of(capabilities, id).is_empty();
        match (armed, section) {
            (true, Some(section)) => findings.push(refuse(format!(
                "an option arm names the document, so it is OPTION and names no section, not `{section}`"
            ))),
            (true, None) => {}
            (false, None) => findings.push(refuse(format!(
                "no option arm names the document, so it is BASE and names a section of `{folder}`"
            ))),
            (false, Some(section)) => {
                let declared = chapters.folder(folder).is_some_and(|(_, decl)| {
                    decl.sections.iter().any(|s| s.section == section)
                });
                if declared {
                    used.insert((folder, section));
                } else {
                    findings.push(refuse(format!(
                        "`{section}` is not a section `{CHAPTERS_FILE}` declares for `{folder}`"
                    )));
                }
            }
        }
    }
    for chapter in &chapters.chapters {
        for folder in &chapter.folders {
            for section in &folder.sections {
                if !used.contains(&(folder.folder.as_str(), section.section.as_str())) {
                    findings.push(GateFinding::new(
                        Gate::Structure,
                        path,
                        "#/chapters",
                        format!(
                            "no document of `{}` names the section `{}`",
                            folder.folder, section.section
                        ),
                    ));
                }
            }
        }
    }
}

/// Every option of the capabilities placed once, every placement an option,
/// and every `spec-boundary` option's citation naming its report.
fn check_options(
    path: &Path,
    chapters: &Chapters,
    capabilities: Option<&Capabilities>,
    catalogue_root: &Path,
    findings: &mut Vec<GateFinding>,
) {
    let options = capabilities.map(|c| &c.options);
    for name in chapters.options.keys() {
        if !options.is_some_and(|o| o.contains_key(name)) {
            findings.push(GateFinding::new(
                Gate::Structure,
                path,
                format!("#/options/{name}"),
                format!("`{name}` is not an option of `{CAPABILITIES_FILE}`"),
            ));
        }
    }
    let capabilities_path = catalogue_root.join(CAPABILITIES_FILE);
    for (name, option) in options.into_iter().flatten() {
        match chapters.options.get(name) {
            None => findings.push(GateFinding::new(
                Gate::Structure,
                path,
                "#/options",
                format!("the option `{name}` has no theme and tier"),
            )),
            Some(placement)
                if placement.tier == Tier::SpecBoundary && option.cite.report.is_none() =>
            {
                findings.push(GateFinding::new(
                    Gate::Structure,
                    &capabilities_path,
                    format!("#/options/{name}"),
                    format!(
                        "the option `{name}` is a spec boundary, so its citation names the upstream report in `report`"
                    ),
                ));
            }
            Some(_) => {}
        }
    }
}

/// Every case of a folder that badges its formalism uploads templates of
/// exactly one formalism.
fn check_formalisms(
    chapters: &Chapters,
    documents: &BTreeMap<&str, &ArazzoDocument>,
    findings: &mut Vec<GateFinding>,
) {
    let mut formalisms = Formalisms::default();
    for document in documents.values() {
        let badged = folder_of(document)
            .and_then(|folder| chapters.folder(folder))
            .is_some_and(|(_, decl)| decl.formalism);
        if !badged {
            continue;
        }
        for workflow in &document.description().workflows {
            let found = formalisms.of(document, workflow);
            if found.len() != 1 {
                findings.push(GateFinding::new(
                    Gate::Structure,
                    document.path(),
                    workflow.workflow_id.clone(),
                    format!(
                        "its folder badges the template formalism, and the case uploads templates of {} formalisms, itself or through its setups",
                        found.len()
                    ),
                ));
            }
        }
    }
}

/// Every cited file in a vendored component whose manifest states statuses.
fn check_maturities(
    documents: &BTreeMap<&str, &ArazzoDocument>,
    repo_root: &Path,
    findings: &mut Vec<GateFinding>,
) {
    let mut maturities = Maturities::new(repo_root);
    for document in documents.values() {
        for workflow in &document.description().workflows {
            for citation in workflow_citations(workflow) {
                if let Err(error @ MaturityError::NoManifest(_)) = maturities.of(&citation.source) {
                    findings.push(GateFinding::new(
                        Gate::Structure,
                        document.path(),
                        workflow.workflow_id.clone(),
                        error.to_string(),
                    ));
                }
            }
        }
    }
}

/// Every report a citation names in the snapshot, and every snapshot entry
/// named by a citation.
fn check_reports(
    catalogue_root: &Path,
    documents: &BTreeMap<&str, &ArazzoDocument>,
    capabilities: Option<&Capabilities>,
    findings: &mut Vec<GateFinding>,
) {
    let mut cited: BTreeMap<NonZeroU64, &Path> = BTreeMap::new();
    for document in documents.values() {
        for workflow in &document.description().workflows {
            for citation in workflow_citations(workflow) {
                if let Some(report) = citation.report {
                    cited.entry(report).or_insert(document.path());
                }
            }
        }
    }
    let capabilities_path = catalogue_root.join(CAPABILITIES_FILE);
    if let Some(capabilities) = capabilities {
        let option_cites = capabilities
            .options
            .values()
            .flat_map(|o| std::iter::once(&o.cite).chain(o.decides.iter().map(|d| &d.cite)));
        let range_cites = capabilities
            .documents
            .values()
            .flat_map(|needs| needs.applies.values().map(|range| &range.cite));
        for cite in option_cites.chain(range_cites) {
            if let Some(report) = cite.report {
                cited.entry(report).or_insert(capabilities_path.as_path());
            }
        }
    }
    let path = catalogue_root.join(REPORTS_FILE);
    let reports = if path.is_file() {
        match Reports::load(&path) {
            Ok(reports) => reports,
            Err(error) => {
                findings.push(GateFinding::new(Gate::Load, &path, "#", error.to_string()));
                return;
            }
        }
    } else {
        Reports {
            reports: BTreeMap::new(),
        }
    };
    for (report, file) in &cited {
        if !reports.reports.contains_key(report) {
            findings.push(GateFinding::new(
                Gate::Structure,
                file,
                "#",
                format!("a citation names upstream report #{report}, which `{REPORTS_FILE}` does not record"),
            ));
        }
    }
    for report in reports.reports.keys() {
        if !cited.contains_key(report) {
            findings.push(GateFinding::new(
                Gate::Structure,
                &path,
                format!("#/reports/{report}"),
                format!("no citation names upstream report #{report}"),
            ));
        }
    }
}
