// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The read seam for S1 and S2 (#64): typed rows the screens render, served
//! by thin `#[server]` fns over the startup catalogue listing.
//!
//! Every number and every text here comes from the published lib's typed
//! model — the console re-parses nothing. Serialized types carry fixed-size
//! integers only: a pointer-sized `usize` crossing the 32-bit WASM to 64-bit
//! server boundary deserializes wrong
//! (<https://book.leptos.dev/server/25_server_functions.html>).

use serde::{Deserialize, Serialize};

/// What the instrument landing shows: the catalogue listing's own numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentSummary {
    /// The listed workflow count: one case each.
    pub cases: u64,
    /// The listed Arazzo document count.
    pub documents: u64,
    /// The capability folder count.
    pub capabilities: u64,
    /// The gate finding count; zero is the only passing result.
    pub findings: u64,
    /// The catalogue root, for display.
    pub root: String,
    /// The vendored spec tree, for display.
    pub specs: String,
}

/// The catalogue-missing explanation: which mount was expected, and the
/// verbatim reason the load refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogueMissing {
    /// The catalogue root the console looked at.
    pub root: String,
    /// The spec tree the console looked at.
    pub specs: String,
    /// The load error, verbatim.
    pub reason: String,
}

/// Either the summary or the honest absence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstrumentView {
    /// The catalogue loaded; the landing renders the numbers.
    Loaded(InstrumentSummary),
    /// The catalogue did not load; the landing renders the explanation.
    Missing(CatalogueMissing),
}

/// One capability folder and its case count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterRow {
    /// The capability folder under `cases/`.
    pub key: String,
    /// How many workflows the folder's documents carry.
    pub cases: u64,
}

/// One case row in a capability listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseRow {
    /// The case id, `<document>/<workflowId>`.
    pub id: String,
    /// The workflow's summary.
    pub summary: String,
    /// The document the workflow is in.
    pub band: String,
    /// The template formalism its setups upload, on a folder that badges it.
    pub formalism: Option<String>,
}

/// One document section of a capability listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BandRows {
    /// The document id.
    pub band: String,
    /// The document's workflows, in document order.
    pub cases: Vec<CaseRow>,
}

/// One step of a case, as the detail screen lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRow {
    /// The step id.
    pub id: String,
    /// The operation the step calls.
    pub operation: String,
    /// The step's description.
    pub description: String,
}

/// The spec citation a case stands on, with the link to the released text
/// upstream when the vendored tree records where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitationRow {
    /// The vendored file, relative to the repository root.
    pub source: String,
    /// The element or the section heading.
    pub section: String,
    /// The sentence, verbatim.
    pub quote: String,
    /// The source upstream at its pinned commit, when the tree records one.
    pub link: Option<String>,
}

/// How firmly the release asks for what an option decides, as the console
/// titles it, mapped from the engine's tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OptionTier {
    /// Support is a SHOULD.
    Recommended,
    /// A MAY, or behaviour the release leaves to the implementation.
    Optional,
    /// An option only because the released text is defective.
    SpecDefect,
}

impl OptionTier {
    /// The heading a view prints.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Recommended => "Recommended (SHOULD)",
            Self::Optional => "Optional (MAY)",
            Self::SpecDefect => "Spec defect",
        }
    }

    /// The one-line explanation printed under the heading.
    #[must_use]
    pub const fn note(self) -> &'static str {
        match self {
            Self::Recommended => {
                "The release asks for these with a SHOULD; declining one is recorded as advisory."
            }
            Self::Optional => {
                "A MAY, or behaviour the release leaves to the implementation; the statement picks one arm."
            }
            Self::SpecDefect => {
                "An option only because the released text is defective; each names its upstream report."
            }
        }
    }

    /// The in-page anchor of the tier on a chapter page.
    #[must_use]
    pub const fn anchor(self) -> &'static str {
        match self {
            Self::Recommended => "tier-recommended",
            Self::Optional => "tier-optional",
            Self::SpecDefect => "tier-spec-defect",
        }
    }
}

/// One option arm a case sits on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmPlace {
    /// The option name.
    pub option: String,
    /// The arm token.
    pub arm: String,
    /// The option's tier, when the structure file places it.
    pub tier: Option<OptionTier>,
}

/// Whether a case runs for every claimant or only under an option arm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseStanding {
    /// Every claimant of the capability runs it.
    Base {
        /// The section key, when the document names a declared section.
        section: Option<String>,
        /// The section heading, when the document names a declared section.
        title: Option<String>,
    },
    /// It runs only when a statement selects one of these arms.
    Option {
        /// The arms naming its document.
        arms: Vec<ArmPlace>,
    },
}

/// Everything the case-detail screen shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseDetail {
    /// The case id.
    pub id: String,
    /// The capability folder the case lives under.
    pub chapter: String,
    /// The key of the chapter the folder sits in, when declared.
    pub chapter_key: Option<String>,
    /// The heading of the chapter the folder sits in, when declared.
    pub chapter_title: Option<String>,
    /// The folder's heading, when declared.
    pub band_title: Option<String>,
    /// BASE with its section, or the option arms that run it.
    pub standing: CaseStanding,
    /// The document the case is a workflow of.
    pub document: String,
    /// The workflow's summary.
    pub summary: String,
    /// The workflow's description.
    pub description: String,
    /// The workflow's own citation.
    pub citation: Option<CitationRow>,
    /// The steps, in document order.
    pub steps: Vec<StepRow>,
    /// The option arm the document tests, as `option: arm`, when an option
    /// names it.
    pub option: Option<String>,
}

/// One capability folder as the catalogue home lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BandCard {
    /// The capability folder under `cases/`.
    pub folder: String,
    /// The folder's heading; the folder name when no chapter declares it.
    pub title: String,
    /// Every case of the folder.
    pub cases: u64,
    /// The cases every claimant runs.
    pub base: u64,
    /// The cases an option arm runs.
    pub options: u64,
}

/// One chapter of the catalogue home, its folders in declared order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterGroup {
    /// The chapter key; `other` for the folders no chapter declares.
    pub key: String,
    /// The heading.
    pub title: String,
    /// Every case of its folders.
    pub cases: u64,
    /// Its folders.
    pub bands: Vec<BandCard>,
}

/// One BASE section of a capability folder, as its chapter page lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionBlock {
    /// The in-page anchor of the section.
    pub anchor: String,
    /// The heading.
    pub title: String,
    /// How many cases the section carries.
    pub cases: u64,
    /// Its cases: documents in path order, workflows in document order.
    pub rows: Vec<CaseRow>,
}

/// One arm of an option, as a chapter page lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmBlock {
    /// The arm token.
    pub arm: String,
    /// Whether the catalogue names no document for it: selecting it runs
    /// nothing.
    pub empty: bool,
    /// Its cases in this folder, documents in arm order.
    pub rows: Vec<CaseRow>,
}

/// One option of `capabilities.yaml` touching a capability folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionBlock {
    /// The in-page anchor of the option.
    pub anchor: String,
    /// The option name.
    pub option: String,
    /// Its tier, when the structure file places it.
    pub tier: Option<OptionTier>,
    /// The heading of its theme, when the structure file places it.
    pub theme: Option<String>,
    /// What it decides.
    pub description: String,
    /// How many cases its arms carry in this folder.
    pub cases: u64,
    /// Its arms, in name order.
    pub arms: Vec<ArmBlock>,
}

/// One capability folder grouped as a reader meets it: BASE sections first,
/// then the options touching it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterPage {
    /// The capability folder.
    pub folder: String,
    /// The folder's heading, when the structure file declares it.
    pub title: Option<String>,
    /// The heading of the chapter the folder sits in.
    pub chapter: Option<String>,
    /// The key of the chapter the folder sits in.
    pub chapter_key: Option<String>,
    /// The BASE sections, in declared order; a section with no matching case
    /// is left out.
    pub sections: Vec<SectionBlock>,
    /// The options touching the folder, by tier, theme and name.
    pub options: Vec<OptionBlock>,
    /// BASE cases whose document names no declared section of the folder.
    pub unsectioned: Vec<CaseRow>,
}

impl ChapterPage {
    /// Every case the page lists, counted once per place it is listed.
    #[must_use]
    pub fn cases(&self) -> u64 {
        self.sections
            .iter()
            .map(|s| s.cases)
            .chain(self.options.iter().map(|o| o.cases))
            .chain(std::iter::once(
                u64::try_from(self.unsectioned.len()).unwrap_or(u64::MAX),
            ))
            .fold(0, u64::saturating_add)
    }
}
#[cfg(feature = "ssr")]
pub mod read {
    //! The ssr-side readers over the startup state — component-free, so the
    //! mapping from the listing to the rows is plain testable code.

    use std::collections::BTreeMap;
    use std::path::Path;

    use super::{
        ArmBlock, ArmPlace, BandCard, BandRows, CaseDetail, CaseRow, CaseStanding,
        CatalogueMissing, ChapterGroup, ChapterPage, ChapterRow, CitationRow, InstrumentSummary,
        InstrumentView, OptionBlock, OptionTier, SectionBlock, StepRow,
    };
    use crate::state::ConsoleState;
    use veredictum::arazzo::capabilities::{CAPABILITIES_FILE, Capabilities};
    use veredictum::arazzo::document::ArazzoDocument;
    use veredictum::arazzo::results::upstream_link;
    use veredictum::arazzo::structure::{
        CHAPTERS_FILE, Chapters, Formalisms, Theme, Tier, arms_of, section_of,
    };
    use veredictum::pipeline::listing::ListedDocument;

    /// A collection length as the wire's fixed-size integer.
    ///
    /// NOTE: no openEHR spec governs this — our own design; usize → u64 is
    /// lossless on every supported target, so the saturation arm is
    /// unreachable and exists only because `as` is banned.
    fn count(n: usize) -> u64 {
        u64::try_from(n).unwrap_or(u64::MAX)
    }

    /// The console's title of the engine's tier.
    #[must_use]
    pub const fn tier_of(tier: Tier) -> OptionTier {
        match tier {
            Tier::Recommended => OptionTier::Recommended,
            Tier::Options => OptionTier::Optional,
            Tier::SpecBoundary => OptionTier::SpecDefect,
        }
    }

    /// Maps the startup state to the landing view.
    #[must_use]
    pub fn instrument_view(state: &ConsoleState) -> InstrumentView {
        match state.catalogue.as_ref() {
            Ok(listing) => InstrumentView::Loaded(InstrumentSummary {
                cases: count(listing.workflows().count()),
                documents: count(listing.documents.len()),
                capabilities: count(listing.capabilities().len()),
                findings: count(listing.findings.len()),
                root: state.root.display().to_string(),
                specs: state.specs.display().to_string(),
            }),
            Err(reason) => InstrumentView::Missing(CatalogueMissing {
                root: state.root.display().to_string(),
                specs: state.specs.display().to_string(),
                reason: reason.clone(),
            }),
        }
    }

    /// The capability folders, sorted by name, each with its case count.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    pub fn chapter_rows(state: &ConsoleState) -> Result<Vec<ChapterRow>, String> {
        let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        Ok(listing
            .capabilities()
            .into_iter()
            .map(|(key, documents)| ChapterRow {
                key: key.to_owned(),
                cases: count(documents.iter().map(|d| d.workflows.len()).sum()),
            })
            .collect())
    }

    /// The catalogue home: the chapters in declared order, each with its
    /// folders in declared order; a folder no chapter declares lands in a
    /// trailing `other` group.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue or a structure file
    /// present under its root does not load.
    pub fn catalogue_home(state: &ConsoleState) -> Result<Vec<ChapterGroup>, String> {
        let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let (chapters, capabilities) = structure(&listing.root)?;
        let mut cards: BTreeMap<&str, BandCard> = listing
            .capabilities()
            .into_iter()
            .map(|(folder, documents)| {
                let (base, options) =
                    documents
                        .iter()
                        .fold((0_usize, 0_usize), |(base, options), document| {
                            let n = document.workflows.len();
                            if arms_of(capabilities.as_ref(), &document.id).is_empty() {
                                (base.saturating_add(n), options)
                            } else {
                                (base, options.saturating_add(n))
                            }
                        });
                let card = BandCard {
                    folder: folder.to_owned(),
                    title: folder.to_owned(),
                    cases: count(base.saturating_add(options)),
                    base: count(base),
                    options: count(options),
                };
                (folder, card)
            })
            .collect();
        let group = |key: &str, title: &str, bands: Vec<BandCard>| ChapterGroup {
            key: key.to_owned(),
            title: title.to_owned(),
            cases: bands.iter().map(|b| b.cases).fold(0, u64::saturating_add),
            bands,
        };
        let mut groups = Vec::new();
        for chapter in chapters.iter().flat_map(|c| c.chapters.iter()) {
            let bands: Vec<BandCard> = chapter
                .folders
                .iter()
                .filter_map(|folder| {
                    let mut card = cards.remove(folder.folder.as_str())?;
                    card.title.clone_from(&folder.title);
                    Some(card)
                })
                .collect();
            if !bands.is_empty() {
                groups.push(group(&chapter.chapter, &chapter.title, bands));
            }
        }
        if !cards.is_empty() {
            groups.push(group(
                "other",
                "Other folders",
                cards.into_values().collect(),
            ));
        }
        Ok(groups)
    }

    /// One capability folder's cases, grouped by document and filtered by the
    /// case id substring `q`.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    pub fn band_rows(
        state: &ConsoleState,
        chapter: &str,
        q: &str,
    ) -> Result<Vec<BandRows>, String> {
        let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        Ok(listing
            .documents
            .iter()
            .filter(|document| document.capability == chapter)
            .map(|document| BandRows {
                band: document.id.clone(),
                cases: document
                    .workflows
                    .iter()
                    .filter(|workflow| q.is_empty() || workflow.case.contains(q))
                    .map(|workflow| CaseRow {
                        id: workflow.case.clone(),
                        summary: workflow.summary.clone().unwrap_or_default(),
                        band: document.id.clone(),
                        formalism: None,
                    })
                    .collect(),
            })
            .filter(|band| !band.cases.is_empty())
            .collect())
    }

    /// The catalogue's structure files under `root`, each `None` when absent.
    ///
    /// # Errors
    /// The verbatim load failure of a structure file that is present.
    fn structure(root: &Path) -> Result<(Option<Chapters>, Option<Capabilities>), String> {
        let chapters_path = root.join(CHAPTERS_FILE);
        let chapters = if chapters_path.is_file() {
            Some(Chapters::load(&chapters_path).map_err(|e| e.to_string())?)
        } else {
            None
        };
        let capabilities_path = root.join(CAPABILITIES_FILE);
        let capabilities = if capabilities_path.is_file() {
            Some(Capabilities::load(&capabilities_path).map_err(|e| e.to_string())?)
        } else {
            None
        };
        Ok((chapters, capabilities))
    }

    /// One listed document's cases matching `q`, each with its formalism
    /// badge when `badge` holds.
    fn document_rows(
        listed: &ListedDocument,
        loaded: Option<&ArazzoDocument>,
        badge: bool,
        formalisms: &mut Formalisms,
        q: &str,
    ) -> Vec<CaseRow> {
        listed
            .workflows
            .iter()
            .filter(|workflow| q.is_empty() || workflow.case.contains(q))
            .map(|workflow| {
                let formalism = loaded.filter(|_| badge).and_then(|document| {
                    let id = workflow.case.rsplit_once('/').map_or("", |(_, id)| id);
                    let found = formalisms.of(document, document.workflow(id)?);
                    let labels: Vec<&str> = found.iter().map(|f| f.label()).collect();
                    (!labels.is_empty()).then(|| labels.join(" + "))
                });
                CaseRow {
                    id: workflow.case.clone(),
                    summary: workflow.summary.clone().unwrap_or_default(),
                    band: listed.id.clone(),
                    formalism,
                }
            })
            .collect()
    }

    /// The options of `capabilities` naming a document of `by_document`,
    /// each arm listing its cases there, ordered by tier, theme and name;
    /// under a filter `q`, an option with no matching case is left out.
    fn option_blocks(
        chapters: Option<&Chapters>,
        capabilities: Option<&Capabilities>,
        by_document: &BTreeMap<&str, Vec<CaseRow>>,
        q: &str,
    ) -> Vec<OptionBlock> {
        let mut options: Vec<(Option<(Tier, Theme)>, OptionBlock)> = capabilities
            .map(|c| {
                c.options
                    .iter()
                    .filter(|(_, option)| {
                        option
                            .arms
                            .values()
                            .flat_map(|arm| arm.documents.iter())
                            .any(|d| by_document.contains_key(d.as_str()))
                    })
                    .map(|(name, option)| {
                        let placement = chapters
                            .and_then(|c| c.options.get(name))
                            .map(|p| (p.tier, p.theme));
                        let arms: Vec<ArmBlock> = option
                            .arms
                            .iter()
                            .map(|(arm, decl)| ArmBlock {
                                arm: arm.clone(),
                                empty: decl.documents.is_empty(),
                                rows: decl
                                    .documents
                                    .iter()
                                    .filter_map(|d| by_document.get(d.as_str()))
                                    .flatten()
                                    .cloned()
                                    .collect(),
                            })
                            .collect();
                        let cases = arms.iter().map(|a| count(a.rows.len())).sum();
                        (
                            placement,
                            OptionBlock {
                                anchor: format!("option-{name}"),
                                option: name.clone(),
                                tier: placement.map(|(tier, _)| tier_of(tier)),
                                theme: placement.map(|(_, theme)| theme.title().to_owned()),
                                description: option.description.clone(),
                                cases,
                                arms,
                            },
                        )
                    })
                    .filter(|(_, block)| q.is_empty() || block.cases > 0)
                    .collect()
            })
            .unwrap_or_default();
        options.sort_by(|(a, x), (b, y)| {
            a.is_none()
                .cmp(&b.is_none())
                .then(a.cmp(b))
                .then(x.option.cmp(&y.option))
        });
        options.into_iter().map(|(_, block)| block).collect()
    }

    /// One capability folder grouped as a reader meets it: its BASE sections
    /// in declared order, then the options of `capabilities.yaml` touching
    /// it, every case filtered by the case id substring `q`.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue or a structure file
    /// present under its root does not load.
    pub fn chapter_page(
        state: &ConsoleState,
        folder: &str,
        q: &str,
    ) -> Result<ChapterPage, String> {
        let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let (chapters, capabilities) = structure(&listing.root)?;
        let declared = chapters.as_ref().and_then(|c| c.folder(folder));
        let badge = declared.is_some_and(|(_, decl)| decl.formalism);
        let mut formalisms = Formalisms::default();
        let mut by_section: BTreeMap<String, Vec<CaseRow>> = BTreeMap::new();
        let mut by_document: BTreeMap<&str, Vec<CaseRow>> = BTreeMap::new();
        let mut unsectioned = Vec::new();
        for listed in listing.documents.iter().filter(|d| d.capability == folder) {
            // NOTE: no openEHR spec governs this — our own design; a document that does not load
            // is a gate finding already, and it lists here with no section or badge.
            let loaded = ArazzoDocument::load(&listed.path).ok();
            let rows = document_rows(listed, loaded.as_ref(), badge, &mut formalisms, q);
            if !arms_of(capabilities.as_ref(), &listed.id).is_empty() {
                by_document.insert(listed.id.as_str(), rows);
                continue;
            }
            if rows.is_empty() {
                continue;
            }
            let section = loaded.as_ref().and_then(section_of).filter(|s| {
                declared.is_some_and(|(_, d)| d.sections.iter().any(|x| x.section == *s))
            });
            match section {
                Some(section) => by_section
                    .entry(section.to_owned())
                    .or_default()
                    .extend(rows),
                None => unsectioned.extend(rows),
            }
        }
        let sections = declared
            .map(|(_, decl)| {
                decl.sections
                    .iter()
                    .filter_map(|section| {
                        let rows = by_section.remove(&section.section)?;
                        Some(SectionBlock {
                            anchor: format!("section-{}", section.section),
                            title: section.title.clone(),
                            cases: count(rows.len()),
                            rows,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let options = option_blocks(chapters.as_ref(), capabilities.as_ref(), &by_document, q);
        Ok(ChapterPage {
            folder: folder.to_owned(),
            title: declared.map(|(_, d)| d.title.clone()),
            chapter: declared.map(|(c, _)| c.title.clone()),
            chapter_key: declared.map(|(c, _)| c.chapter.clone()),
            sections,
            options,
            unsectioned,
        })
    }

    /// The full case detail, or `Ok(None)` for an id the catalogue does not
    /// carry — a legitimately absent page, not an error.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue or a structure file
    /// present under its root does not load.
    pub fn case_detail(state: &ConsoleState, id: &str) -> Result<Option<CaseDetail>, String> {
        let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let Some((document, workflow)) = listing.workflows().find(|(_, w)| w.case == id) else {
            return Ok(None);
        };
        let (chapters, capabilities) = structure(&listing.root)?;
        let declared = chapters
            .as_ref()
            .and_then(|c| c.folder(&document.capability));
        let arms = arms_of(capabilities.as_ref(), &document.id);
        let standing = if arms.is_empty() {
            // NOTE: no openEHR spec governs this — our own design; a document that does not load
            // is a gate finding already, and its case shows with no section.
            let loaded = ArazzoDocument::load(&document.path).ok();
            let section = loaded.as_ref().and_then(section_of).and_then(|key| {
                declared.and_then(|(_, d)| d.sections.iter().find(|s| s.section == key))
            });
            CaseStanding::Base {
                section: section.map(|s| s.section.clone()),
                title: section.map(|s| s.title.clone()),
            }
        } else {
            CaseStanding::Option {
                arms: arms
                    .into_iter()
                    .map(|arm| ArmPlace {
                        tier: chapters
                            .as_ref()
                            .and_then(|c| c.options.get(&arm.option))
                            .map(|p| tier_of(p.tier)),
                        option: arm.option,
                        arm: arm.arm,
                    })
                    .collect(),
            }
        };
        // NOTE: no openEHR spec governs this — our own design; a citation source is relative to
        // the repository root, which holds the mounted `specs/openehr` two levels down.
        let repo_root = state
            .specs
            .parent()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new(""));
        Ok(Some(CaseDetail {
            id: workflow.case.clone(),
            chapter: document.capability.clone(),
            chapter_key: declared.map(|(c, _)| c.chapter.clone()),
            chapter_title: declared.map(|(c, _)| c.title.clone()),
            band_title: declared.map(|(_, d)| d.title.clone()),
            standing,
            document: document.id.clone(),
            summary: workflow.summary.clone().unwrap_or_default(),
            description: workflow.description.clone().unwrap_or_default(),
            citation: workflow.citation.as_ref().map(|citation| CitationRow {
                source: citation.source.clone(),
                section: citation.section.clone(),
                quote: citation.quote.clone(),
                link: upstream_link(&citation.source, repo_root),
            }),
            steps: workflow
                .steps
                .iter()
                .map(|step| StepRow {
                    id: step.step_id.clone(),
                    operation: step.operation.clone().unwrap_or_default(),
                    description: step.description.clone().unwrap_or_default(),
                })
                .collect(),
            option: document
                .option_arm
                .as_ref()
                .map(|(option, arm)| format!("{option}: {arm}")),
        }))
    }
}
pub mod fns {
    //! The `#[server]` endpoints, one module for one inner suppression.
    //!
    //! The suppression covers what the macro expands: `unused_async` and
    //! `unused_async_trait_impl` on the async fn it wraps in a trait impl, and
    //! `missing_docs` on the argument struct it mints. Both fire only in SOME
    //! expansions, so `#[expect]` would itself warn through
    //! `unfulfilled_lint_expectations` in the quiet ones
    //! (<https://doc.rust-lang.org/rustc/lints/levels.html#expect>).
    #![allow(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        missing_docs,
        reason = "fires only in some #[server] expansions; see the module doc"
    )]

    use super::{BandRows, CaseDetail, ChapterGroup, ChapterPage, ChapterRow, InstrumentView};
    use leptos::prelude::{ServerFnError, server};

    #[cfg(feature = "ssr")]
    use super::read;

    /// The landing view: the counts, or the named-mount explanation.
    ///
    /// # Errors
    /// The server-fn transport only; the catalogue's absence is data, not an
    /// error.
    #[server]
    pub async fn fetch_instrument() -> Result<InstrumentView, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        Ok(read::instrument_view(&state))
    }

    /// The capability folder list.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    #[server]
    pub async fn fetch_chapters() -> Result<Vec<ChapterRow>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        read::chapter_rows(&state).map_err(ServerFnError::new)
    }

    /// The catalogue home: chapters in declared order, each with its folders.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue or a structure file
    /// present under its root does not load.
    #[server]
    pub async fn fetch_catalogue_home() -> Result<Vec<ChapterGroup>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        read::catalogue_home(&state).map_err(ServerFnError::new)
    }

    /// One capability folder's cases, grouped by document, filtered by the
    /// case id substring `q`.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    #[server]
    pub async fn fetch_chapter_bands(
        chapter: String,
        q: String,
    ) -> Result<Vec<BandRows>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        read::band_rows(&state, &chapter, &q).map_err(ServerFnError::new)
    }

    /// One capability folder grouped into its BASE sections and the options
    /// touching it, filtered by the case id substring `q`.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue or a structure file
    /// present under its root does not load.
    #[server]
    pub async fn fetch_chapter_page(
        chapter: String,
        q: String,
    ) -> Result<ChapterPage, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        read::chapter_page(&state, &chapter, &q).map_err(ServerFnError::new)
    }

    /// One case in full; `None` for an id the catalogue does not carry.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    #[server]
    pub async fn fetch_case_detail(id: String) -> Result<Option<CaseDetail>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        read::case_detail(&state, &id).map_err(ServerFnError::new)
    }
}
