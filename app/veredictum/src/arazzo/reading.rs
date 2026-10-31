// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The reading of a results document every view prints, in two parts: BASE
//! by chapter, band and section, then OPTIONS by chapter, band and option,
//! each option labelled with its tier.
//!
//! The grouping is read from the [`Outline`] the results carry, so no view
//! invents its own. A case is BASE when no option arm names its document;
//! a BASE case whose citations rest on TRIAL text is read with the options,
//! never in a BASE tally. A row whose server did not follow a SHOULD is
//! counted as a warning and never as failed (RFC 2119 §3, which the ITS-REST
//! overview's keywords follow: there "may exist valid reasons in particular
//! circumstances to ignore a particular item"). Every list keeps document
//! order, so the reading is as deterministic as the results. No openEHR spec
//! governs the reading — our own design.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use crate::arazzo::results::{ResultRow, Results, Status, Whose};
use crate::arazzo::structure::{Formalism, Outline, Place, ReportState, Standing, Theme, Tier};
use crate::pipeline::arazzo_run::Bearing;

/// The row counts of one group of rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    /// Every row.
    pub tests: usize,
    /// The passed rows.
    pub passed: usize,
    /// The failed rows that count against the server.
    pub failed: usize,
    /// The failed rows whose server did not follow a SHOULD; they never
    /// count against a capability.
    pub warning: usize,
    /// The errored rows: the inconclusive ones, which the instrument could
    /// not judge, and those whose record names the server or no one, which
    /// count against the server ([`Tally::errored_against_server`]).
    pub errored: usize,
    /// The errored rows among [`Tally::errored`] that count against the
    /// server, as [`crate::pipeline::arazzo_run::Bearing`] reads them.
    pub errored_against_server: usize,
    /// The rows not run.
    pub skipped: usize,
}

impl Tally {
    /// Counts `rows`, a failed warning row as a warning.
    #[must_use]
    pub fn of<'a>(rows: impl IntoIterator<Item = &'a ResultRow>) -> Self {
        let mut tally = Self::default();
        for row in rows {
            tally.add(row);
        }
        tally
    }

    /// The counts of both tallies together.
    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self {
            tests: self.tests.saturating_add(other.tests),
            passed: self.passed.saturating_add(other.passed),
            failed: self.failed.saturating_add(other.failed),
            warning: self.warning.saturating_add(other.warning),
            errored: self.errored.saturating_add(other.errored),
            errored_against_server: self
                .errored_against_server
                .saturating_add(other.errored_against_server),
            skipped: self.skipped.saturating_add(other.skipped),
        }
    }

    /// The rows that count against the server: the failed ones and the
    /// errored ones against the server.
    #[must_use]
    pub const fn against_server(self) -> usize {
        self.failed.saturating_add(self.errored_against_server)
    }

    /// Counts one more row.
    pub fn add(&mut self, row: &ResultRow) {
        self.tests += 1;
        match row.status {
            Status::Passed => self.passed += 1,
            Status::Failed if is_warning(row) => self.warning += 1,
            Status::Failed => self.failed += 1,
            Status::Errored => {
                self.errored += 1;
                if Bearing::of(row) == Bearing::AgainstServer {
                    self.errored_against_server += 1;
                }
            }
            Status::Skipped => self.skipped += 1,
        }
    }
}

/// Whether `row` failed only a SHOULD: failed, and its explanation says the
/// row is a warning.
#[must_use]
pub fn is_warning(row: &ResultRow) -> bool {
    row.status == Status::Failed
        && row
            .explanation
            .as_ref()
            .is_some_and(|e| e.whose == Whose::Warning)
}

/// What one row reads as, the mark every view prints beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    /// Passed.
    Passed,
    /// Failed, counting against the server.
    Failed,
    /// Failed a SHOULD only.
    Warning,
    /// The instrument could not judge it.
    Errored,
    /// Not run.
    Skipped,
}

impl Mark {
    /// The mark of `row`.
    #[must_use]
    pub fn of(row: &ResultRow) -> Self {
        match row.status {
            Status::Passed => Self::Passed,
            Status::Failed if is_warning(row) => Self::Warning,
            Status::Failed => Self::Failed,
            Status::Errored => Self::Errored,
            Status::Skipped => Self::Skipped,
        }
    }

    /// The glyph a view prints, so a mark reads without colour.
    #[must_use]
    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Passed => "✓",
            Self::Failed => "✕",
            Self::Warning => "!",
            Self::Errored => "?",
            Self::Skipped => "○",
        }
    }

    /// The word a view prints.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Warning => "warning",
            Self::Errored => "errored",
            Self::Skipped => "skipped",
        }
    }
}

/// The rows of one case in a group, with where the case sits.
#[derive(Debug, Clone)]
pub struct CaseRows<'a> {
    /// The case id.
    pub case: &'a str,
    /// Where it sits; `None` when the results carry no outline for it.
    pub place: Option<&'a Place>,
    /// Its rows, in document order.
    pub rows: Vec<&'a ResultRow>,
}

impl CaseRows<'_> {
    /// The case's template formalism, on a folder that badges it.
    #[must_use]
    pub fn formalism(&self) -> Option<Formalism> {
        self.place.and_then(|p| p.formalism)
    }

    /// Whether the case rests on TRIAL text.
    #[must_use]
    pub fn trial(&self) -> bool {
        self.place.is_some_and(|p| p.trial)
    }

    /// The upstream reports the case rests on.
    #[must_use]
    pub fn reports(&self) -> &[NonZeroU64] {
        self.place.map(|p| p.reports.as_slice()).unwrap_or_default()
    }

    /// The counts of the case's rows.
    #[must_use]
    pub fn tally(&self) -> Tally {
        Tally::of(self.rows.iter().copied())
    }

    /// The mark the case reads as: the first of failed, errored, warning
    /// and passed that any of its rows carries, else skipped.
    #[must_use]
    pub fn mark(&self) -> Mark {
        let tally = self.tally();
        [
            (tally.failed, Mark::Failed),
            (tally.errored, Mark::Errored),
            (tally.warning, Mark::Warning),
            (tally.passed, Mark::Passed),
        ]
        .into_iter()
        .find_map(|(count, mark)| (count > 0).then_some(mark))
        .unwrap_or(Mark::Skipped)
    }
}

/// One BASE section of a band.
#[derive(Debug, Clone)]
pub struct SectionGroup<'a> {
    /// The section key.
    pub section: &'a str,
    /// Its heading.
    pub title: &'a str,
    /// The formalism of its cases, when its band splits by formalism.
    pub formalism: Option<Formalism>,
    /// Its counts.
    pub tally: Tally,
    /// Its cases, in document order.
    pub cases: Vec<CaseRows<'a>>,
}

/// One band: the BASE cases of one capability folder.
#[derive(Debug, Clone)]
pub struct BandGroup<'a> {
    /// The capability folder.
    pub folder: &'a str,
    /// Its heading.
    pub title: &'a str,
    /// Its counts.
    pub tally: Tally,
    /// Its sections, in declared order; a section with no case in the
    /// results is left out.
    pub sections: Vec<SectionGroup<'a>>,
}

impl BandGroup<'_> {
    /// The upstream reports the band's cases rest on, ascending, each with
    /// the number of its cases resting on it.
    #[must_use]
    pub fn reports(&self) -> Vec<(NonZeroU64, usize)> {
        reports_of(self.sections.iter().flat_map(|s| s.cases.iter()))
    }
}

impl SectionGroup<'_> {
    /// The upstream reports the section's cases rest on, ascending, each
    /// with the number of its cases resting on it.
    #[must_use]
    pub fn reports(&self) -> Vec<(NonZeroU64, usize)> {
        reports_of(self.cases.iter())
    }
}

/// The reports `cases` rest on, ascending, each with how many cases name it.
fn reports_of<'b, 'a: 'b>(
    cases: impl IntoIterator<Item = &'b CaseRows<'a>>,
) -> Vec<(NonZeroU64, usize)> {
    let mut counts: BTreeMap<NonZeroU64, usize> = BTreeMap::new();
    for case in cases {
        for report in case.reports() {
            *counts.entry(*report).or_default() += 1;
        }
    }
    counts.into_iter().collect()
}

/// One chapter of the BASE part.
#[derive(Debug, Clone)]
pub struct ChapterGroup<'a> {
    /// The chapter key.
    pub chapter: &'a str,
    /// Its heading.
    pub title: &'a str,
    /// Its counts.
    pub tally: Tally,
    /// Every band of the chapter, in declared order, those with no case
    /// included, so a view can say "no cases".
    pub bands: Vec<BandGroup<'a>>,
}

/// One arm of an option.
#[derive(Debug, Clone)]
pub struct ArmGroup<'a> {
    /// The arm token.
    pub arm: &'a str,
    /// Whether the statement selected it.
    pub selected: bool,
    /// Whether the catalogue names no document for it: selecting it runs
    /// nothing.
    pub empty: bool,
    /// Its counts.
    pub tally: Tally,
    /// Its cases, in document order.
    pub cases: Vec<CaseRows<'a>>,
}

/// One option.
#[derive(Debug, Clone)]
pub struct OptionGroup<'a> {
    /// The option name.
    pub option: &'a str,
    /// What it decides.
    pub description: &'a str,
    /// Its theme.
    pub theme: Theme,
    /// Its tier.
    pub tier: Tier,
    /// The upstream report its citation names.
    pub report: Option<NonZeroU64>,
    /// The chapter of the band it is read under; `None` when no arm names
    /// a document.
    pub chapter: Option<&'a str>,
    /// The band: its arms hold only the cases of documents in this folder.
    pub folder: Option<&'a str>,
    /// Whether the statement selected one of its arms.
    pub selected: bool,
    /// Its counts.
    pub tally: Tally,
    /// Its arms, in name order.
    pub arms: Vec<ArmGroup<'a>>,
}

/// The BASE cases that rest on TRIAL text, read with the options.
#[derive(Debug, Clone)]
pub struct TrialGroup<'a> {
    /// Its counts.
    pub tally: Tally,
    /// Its cases, in document order.
    pub cases: Vec<CaseRows<'a>>,
}

/// One band of a tier: the options read under one capability folder.
#[derive(Debug, Clone)]
pub struct OptionBand<'a> {
    /// The capability folder.
    pub folder: &'a str,
    /// Its heading.
    pub title: &'a str,
    /// Its counts, each case once however many options name it.
    pub tally: Tally,
    /// Its options, by theme then name, each holding this band's cases only.
    pub options: Vec<OptionGroup<'a>>,
    /// The band's BASE cases resting on TRIAL text, in the OPTIONS tier.
    pub trial: Option<TrialGroup<'a>>,
}

/// One chapter of a tier.
#[derive(Debug, Clone)]
pub struct OptionChapter<'a> {
    /// The chapter key.
    pub chapter: &'a str,
    /// Its heading.
    pub title: &'a str,
    /// Its counts, each case once.
    pub tally: Tally,
    /// Its bands that hold an option or TRIAL text, in declared order.
    pub bands: Vec<OptionBand<'a>>,
}

/// Every option of a result, read chapter by chapter and band by band like
/// BASE; each option carries its [`Tier`] as a label.
#[derive(Debug, Clone)]
pub struct OptionsTree<'a> {
    /// Its counts, each case once.
    pub tally: Tally,
    /// Its chapters that hold an option or TRIAL text, in declared order.
    pub chapters: Vec<OptionChapter<'a>>,
    /// Its options whose arms name no document yet, so no band holds them.
    pub unanchored: Vec<OptionGroup<'a>>,
}

impl OptionsTree<'_> {
    /// Whether the tree holds nothing to show.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chapters.is_empty() && self.unanchored.is_empty()
    }

    /// Every option, band by band, then the unanchored ones.
    pub fn options(&self) -> impl Iterator<Item = &OptionGroup<'_>> {
        self.chapters
            .iter()
            .flat_map(|c| c.bands.iter().flat_map(|b| b.options.iter()))
            .chain(self.unanchored.iter())
    }

    /// The counts of the arms the statement selected and of the cases
    /// resting on TRIAL text, each case once; an unselected arm's rows are
    /// left out.
    #[must_use]
    pub fn selected_tally(&self) -> Tally {
        let trial = self
            .chapters
            .iter()
            .flat_map(|c| c.bands.iter().flat_map(|b| b.trial.iter()))
            .flat_map(|t| t.cases.iter());
        distinct_tally(
            self.options()
                .flat_map(|g| g.arms.iter().filter(|a| a.selected))
                .flat_map(|a| a.cases.iter())
                .chain(trial),
        )
    }
}

/// The cases resting on text one upstream report records as defective.
#[derive(Debug, Clone)]
pub struct ReportGroup<'a> {
    /// The issue number.
    pub report: NonZeroU64,
    /// The issue title, from the committed snapshot.
    pub title: Option<&'a str>,
    /// Where it stands, from the committed snapshot.
    pub state: Option<ReportState>,
    /// Its counts.
    pub tally: Tally,
    /// Its cases, in document order.
    pub cases: Vec<CaseRows<'a>>,
}

/// The whole reading of one results document.
#[derive(Debug, Clone)]
pub struct Reading<'a> {
    /// Every row, counted once.
    pub total: Tally,
    /// The BASE rows outside the TRIAL group, counted once.
    pub base_total: Tally,
    /// BASE, chapter by chapter.
    pub base: Vec<ChapterGroup<'a>>,
    /// Every option, whatever its tier, with the BASE cases resting on TRIAL
    /// text in their bands.
    pub options: OptionsTree<'a>,
    /// Every BASE case resting on TRIAL text, also read in the bands of
    /// [`Reading::options`].
    pub trial: TrialGroup<'a>,
    /// Every case resting on text an upstream report records, by report.
    pub reports: Vec<ReportGroup<'a>>,
    /// The cases the results place nowhere: every case, when the results
    /// carry no outline.
    pub unplaced: Vec<CaseRows<'a>>,
    /// The rows of [`Reading::unplaced`], counted once. They are in neither
    /// [`Reading::base_total`] nor the options, so a view that shows those
    /// shows this too, and a failed or errored unplaced row never reads green.
    pub unplaced_total: Tally,
}

/// The cases of a result that rest on released text an upstream report
/// records, tagged with [`crate::arazzo::structure::SPEC_DIVERGE_TAG`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpecDivergence {
    /// How many distinct cases rest on such text.
    pub cases: usize,
    /// The reports they rest on, ascending.
    pub reports: Vec<NonZeroU64>,
}

impl Reading<'_> {
    /// The cases of the result that rest on text an upstream report records.
    #[must_use]
    pub fn spec_divergence(&self) -> SpecDivergence {
        let mut cases = std::collections::BTreeSet::new();
        let mut reports = std::collections::BTreeSet::new();
        for group in &self.reports {
            reports.insert(group.report);
            cases.extend(group.cases.iter().map(|c| c.case));
        }
        SpecDivergence {
            cases: cases.len(),
            reports: reports.into_iter().collect(),
        }
    }
}

/// The rows of `results` grouped by case, in the order each case first
/// appears, each case's rows in document order.
#[must_use]
pub fn cases(results: &Results) -> Vec<CaseRows<'_>> {
    let mut index: BTreeMap<&str, usize> = BTreeMap::new();
    let mut groups: Vec<CaseRows<'_>> = Vec::new();
    let outline = results.outline.as_ref();
    for row in &results.rows {
        let case = row.case.as_str();
        let at = *index.entry(case).or_insert_with(|| {
            groups.push(CaseRows {
                case,
                place: outline.and_then(|o| o.place(case)),
                rows: Vec::new(),
            });
            groups.len() - 1
        });
        if let Some(group) = groups.get_mut(at) {
            group.rows.push(row);
        }
    }
    groups
}

fn tally_of(cases: &[CaseRows<'_>]) -> Tally {
    Tally::of(cases.iter().flat_map(|c| c.rows.iter().copied()))
}

/// Reads `results` into BASE, the options by tier, the TRIAL group and the
/// report groups.
#[must_use]
pub fn read(results: &Results) -> Reading<'_> {
    let all = cases(results);
    let total = Tally::of(&results.rows);
    let Some(outline) = results.outline.as_ref() else {
        return Reading {
            total,
            base_total: Tally::default(),
            base: Vec::new(),
            options: OptionsTree::empty(),
            trial: TrialGroup {
                tally: Tally::default(),
                cases: Vec::new(),
            },
            reports: Vec::new(),
            unplaced: all,
            unplaced_total: total,
        };
    };
    let mut unplaced = Vec::new();
    let mut trial = Vec::new();
    let mut base_cases: BTreeMap<(&str, &str), Vec<CaseRows<'_>>> = BTreeMap::new();
    let mut arm_cases: BTreeMap<(&str, &str), Vec<CaseRows<'_>>> = BTreeMap::new();
    for case in &all {
        let Some(place) = case.place else {
            unplaced.push(case.clone());
            continue;
        };
        match &place.standing {
            Standing::Base { .. } if place.trial => trial.push(case.clone()),
            Standing::Base { section } => base_cases
                .entry((place.folder.as_str(), section.as_str()))
                .or_default()
                .push(case.clone()),
            Standing::Option { arms } => {
                for arm in arms {
                    arm_cases
                        .entry((arm.option.as_str(), arm.arm.as_str()))
                        .or_default()
                        .push(case.clone());
                }
            }
        }
    }
    let base = base_chapters(outline, &base_cases);
    let base_total = Tally::of(
        base_cases
            .values()
            .flatten()
            .flat_map(|c| c.rows.iter().copied()),
    );
    let groups = option_groups(outline, &arm_cases);
    let options = options_tree(outline, &groups, &trial);
    Reading {
        total,
        base_total,
        base,
        options,
        trial: TrialGroup {
            tally: tally_of(&trial),
            cases: trial,
        },
        reports: report_groups(outline, &all),
        unplaced_total: tally_of(&unplaced),
        unplaced,
    }
}

fn base_chapters<'a>(
    outline: &'a Outline,
    base_cases: &BTreeMap<(&str, &str), Vec<CaseRows<'a>>>,
) -> Vec<ChapterGroup<'a>> {
    outline
        .chapters
        .iter()
        .map(|chapter| {
            let bands: Vec<BandGroup<'a>> = chapter
                .folders
                .iter()
                .map(|folder| {
                    let mut sections = Vec::new();
                    for section in &folder.sections {
                        let Some(cases) =
                            base_cases.get(&(folder.folder.as_str(), section.section.as_str()))
                        else {
                            continue;
                        };
                        let mut by_formalism: BTreeMap<Option<Formalism>, Vec<CaseRows<'a>>> =
                            BTreeMap::new();
                        for case in cases {
                            by_formalism
                                .entry(case.formalism())
                                .or_default()
                                .push(case.clone());
                        }
                        let split = by_formalism.len() > 1;
                        for (formalism, cases) in by_formalism {
                            sections.push(SectionGroup {
                                section: &section.section,
                                title: &section.title,
                                formalism: if split { formalism } else { None },
                                tally: tally_of(&cases),
                                cases,
                            });
                        }
                    }
                    let tally = Tally::of(
                        sections
                            .iter()
                            .flat_map(|s| s.cases.iter().flat_map(|c| c.rows.iter().copied())),
                    );
                    BandGroup {
                        folder: &folder.folder,
                        title: &folder.title,
                        tally,
                        sections,
                    }
                })
                .collect();
            let tally = Tally::of(bands.iter().flat_map(|b| {
                b.sections
                    .iter()
                    .flat_map(|s| s.cases.iter().flat_map(|c| c.rows.iter().copied()))
            }));
            ChapterGroup {
                chapter: &chapter.chapter,
                title: &chapter.title,
                tally,
                bands,
            }
        })
        .collect()
}

impl OptionsTree<'_> {
    fn empty() -> Self {
        Self {
            tally: Tally::default(),
            chapters: Vec::new(),
            unanchored: Vec::new(),
        }
    }
}

/// Counts every row of `cases` once per case, however often a case repeats.
fn distinct_tally<'a, 'b>(cases: impl IntoIterator<Item = &'b CaseRows<'a>>) -> Tally
where
    'a: 'b,
{
    let mut seen = std::collections::BTreeSet::new();
    let mut tally = Tally::default();
    for case in cases {
        if seen.insert(case.case) {
            for row in &case.rows {
                tally.add(row);
            }
        }
    }
    tally
}

/// Every case an option group holds.
fn group_cases<'b, 'a>(group: &'b OptionGroup<'a>) -> impl Iterator<Item = &'b CaseRows<'a>> {
    group.arms.iter().flat_map(|a| a.cases.iter())
}

/// Every option read chapter by chapter and band by band, with the TRIAL
/// cases `trial` in the bands they sit in.
fn options_tree<'a>(
    outline: &'a Outline,
    groups: &[OptionGroup<'a>],
    trial: &[CaseRows<'a>],
) -> OptionsTree<'a> {
    let in_tier: Vec<&OptionGroup<'a>> = groups.iter().collect();
    let mut chapters = Vec::new();
    for chapter in &outline.chapters {
        let mut bands = Vec::new();
        for folder in &chapter.folders {
            let name = folder.folder.as_str();
            let options: Vec<OptionGroup<'a>> = in_tier
                .iter()
                .filter(|g| g.folder == Some(name))
                .map(|g| (*g).clone())
                .collect();
            let trial_cases: Vec<CaseRows<'a>> = trial
                .iter()
                .filter(|c| c.place.is_some_and(|p| p.folder == name))
                .cloned()
                .collect();
            if options.is_empty() && trial_cases.is_empty() {
                continue;
            }
            let tally = distinct_tally(
                options
                    .iter()
                    .flat_map(group_cases)
                    .chain(trial_cases.iter()),
            );
            bands.push(OptionBand {
                folder: name,
                title: &folder.title,
                tally,
                options,
                trial: (!trial_cases.is_empty()).then(|| TrialGroup {
                    tally: tally_of(&trial_cases),
                    cases: trial_cases,
                }),
            });
        }
        if bands.is_empty() {
            continue;
        }
        let tally = distinct_tally(bands.iter().flat_map(|b| {
            b.options
                .iter()
                .flat_map(group_cases)
                .chain(b.trial.iter().flat_map(|t| t.cases.iter()))
        }));
        chapters.push(OptionChapter {
            chapter: &chapter.chapter,
            title: &chapter.title,
            tally,
            bands,
        });
    }
    let unanchored: Vec<OptionGroup<'a>> = in_tier
        .iter()
        .filter(|g| g.folder.is_none())
        .map(|g| (*g).clone())
        .collect();
    let tally = distinct_tally(chapters.iter().flat_map(|c| {
        c.bands.iter().flat_map(|b| {
            b.options
                .iter()
                .flat_map(group_cases)
                .chain(b.trial.iter().flat_map(|t| t.cases.iter()))
        })
    }));
    OptionsTree {
        tally,
        chapters,
        unanchored,
    }
}

/// One group per option and band its documents sit in, each arm holding that
/// band's cases only, and one group with no band per option whose arms name
/// no document; sorted by tier, then theme, then option name.
fn option_groups<'a>(
    outline: &'a Outline,
    arm_cases: &BTreeMap<(&str, &str), Vec<CaseRows<'a>>>,
) -> Vec<OptionGroup<'a>> {
    let chapter_of = |folder: &str| {
        outline
            .folder(folder)
            .map(|(chapter, _)| chapter.chapter.as_str())
    };
    let mut groups = Vec::new();
    for option in &outline.options {
        let bands: Vec<Option<&'a str>> = if option.folders.is_empty() {
            vec![None]
        } else {
            option.folders.iter().map(|f| Some(f.as_str())).collect()
        };
        for band in bands {
            let arms: Vec<ArmGroup<'a>> = option
                .arms
                .iter()
                .map(|arm| {
                    let cases: Vec<CaseRows<'a>> = arm_cases
                        .get(&(option.option.as_str(), arm.arm.as_str()))
                        .into_iter()
                        .flatten()
                        .filter(|c| band.is_none_or(|f| c.place.is_some_and(|p| p.folder == f)))
                        .cloned()
                        .collect();
                    ArmGroup {
                        arm: &arm.arm,
                        selected: arm.selected,
                        empty: arm.documents.is_empty(),
                        tally: tally_of(&cases),
                        cases,
                    }
                })
                .collect();
            let tally = distinct_tally(arms.iter().flat_map(|a| a.cases.iter()));
            groups.push(OptionGroup {
                option: &option.option,
                description: &option.description,
                theme: option.theme,
                tier: option.tier,
                report: option.report,
                chapter: band.and_then(chapter_of),
                folder: band,
                selected: arms.iter().any(|a| a.selected),
                tally,
                arms,
            });
        }
    }
    groups.sort_by(|x, y| {
        x.tier
            .cmp(&y.tier)
            .then(x.theme.cmp(&y.theme))
            .then(x.option.cmp(y.option))
    });
    groups
}

fn report_groups<'a>(outline: &'a Outline, all: &[CaseRows<'a>]) -> Vec<ReportGroup<'a>> {
    let mut by_report: BTreeMap<NonZeroU64, Vec<CaseRows<'a>>> = BTreeMap::new();
    for case in all {
        for report in case.reports() {
            by_report.entry(*report).or_default().push(case.clone());
        }
    }
    by_report
        .into_iter()
        .map(|(report, cases)| {
            let entry = outline.report(report);
            ReportGroup {
                report,
                title: entry.map(|e| e.title.as_str()),
                state: entry.map(|e| e.state),
                tally: tally_of(&cases),
                cases,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::results::{Engine, Explanation};
    use crate::arazzo::rows::Row;
    use crate::arazzo::structure::{
        ArmRef, ChapterDecl, FolderDecl, OutlineArm, OutlineOption, OutlineReport, SectionDecl,
    };

    fn row(case: &str, status: Status, whose: Option<Whose>) -> ResultRow {
        ResultRow {
            case: case.to_owned(),
            row: 0,
            status,
            inputs: Row::new(),
            generated: Vec::new(),
            skip: None,
            explanation: whose.map(|whose| Explanation {
                step: "s".to_owned(),
                what: "w".to_owned(),
                why: None,
                expected: "e".to_owned(),
                received: "r".to_owned(),
                exchange: None,
                whose,
            }),
        }
    }

    fn base(section: &str) -> Place {
        Place {
            chapter: "ehr".to_owned(),
            folder: "composition".to_owned(),
            standing: Standing::Base {
                section: section.to_owned(),
            },
            trial: false,
            reports: Vec::new(),
            formalism: None,
        }
    }

    fn results() -> Results {
        let mut trial = base("read");
        trial.trial = true;
        let mut reported = base("create");
        reported.reports = vec![NonZeroU64::new(352).unwrap()];
        let option = Place {
            standing: Standing::Option {
                arms: vec![ArmRef {
                    option: "composition-xml-read".to_owned(),
                    arm: "composition-xml-supported".to_owned(),
                }],
            },
            ..base("create")
        };
        let cases = BTreeMap::from([
            ("c/create".to_owned(), reported),
            ("c/read".to_owned(), base("read")),
            ("c/trial".to_owned(), trial),
            ("c/xml".to_owned(), option),
        ]);
        Results {
            document: "catalogue".to_owned(),
            engine: Engine {
                name: "veredictum".to_owned(),
                version: "0".to_owned(),
            },
            instrument_status: crate::party::InstrumentStatus::CURRENT,
            specification: crate::arazzo::spec_profile::Specification::Released,
            base_url: "http://sut".to_owned(),
            server: None,
            review: Vec::new(),
            rows: vec![
                row("c/create", Status::Passed, None),
                row("c/create", Status::Failed, Some(Whose::Server)),
                row("c/read", Status::Failed, Some(Whose::Warning)),
                row("c/trial", Status::Failed, Some(Whose::Server)),
                row("c/xml", Status::Passed, None),
                row("c/stray", Status::Errored, Some(Whose::Instrument)),
            ],
            outline: Some(Outline {
                chapters: vec![ChapterDecl {
                    chapter: "ehr".to_owned(),
                    title: "EHR".to_owned(),
                    folders: vec![FolderDecl {
                        folder: "composition".to_owned(),
                        title: "COMPOSITION".to_owned(),
                        formalism: false,
                        sections: vec![
                            SectionDecl {
                                section: "create".to_owned(),
                                title: "Create".to_owned(),
                            },
                            SectionDecl {
                                section: "read".to_owned(),
                                title: "Read".to_owned(),
                            },
                        ],
                    }],
                }],
                options: vec![OutlineOption {
                    option: "composition-xml-read".to_owned(),
                    description: "d".to_owned(),
                    theme: Theme::CanonicalXml,
                    tier: Tier::Options,
                    report: None,
                    folders: vec!["composition".to_owned()],
                    arms: vec![
                        OutlineArm {
                            arm: "composition-xml-supported".to_owned(),
                            selected: true,
                            documents: vec!["c".to_owned()],
                        },
                        OutlineArm {
                            arm: "composition-xml-unsupported".to_owned(),
                            selected: false,
                            documents: Vec::new(),
                        },
                    ],
                }],
                reports: vec![OutlineReport {
                    report: NonZeroU64::new(352).unwrap(),
                    title: "EHR_STATUS has no flat form".to_owned(),
                    state: ReportState::Confirmed,
                }],
                cases,
            }),
        }
    }

    #[test]
    fn a_warning_row_is_counted_apart_from_a_failed_one() {
        let results = results();
        let total = Tally::of(&results.rows);
        assert_eq!(total.failed, 2);
        assert_eq!(total.warning, 1);
        assert_eq!(total.tests, 6);
        assert_eq!(Mark::of(&results.rows[2]), Mark::Warning);
    }

    #[test]
    fn base_is_read_by_chapter_band_and_section_and_trial_sits_with_the_options() {
        let results = results();
        let reading = read(&results);
        let [chapter] = reading.base.as_slice() else {
            panic!("{:?}", reading.base)
        };
        let [band] = chapter.bands.as_slice() else {
            panic!("{chapter:?}")
        };
        let sections: Vec<&str> = band.sections.iter().map(|s| s.section).collect();
        assert_eq!(sections, ["create", "read"]);
        assert_eq!(band.tally.failed, 1, "the TRIAL case is not in the band");
        assert_eq!(band.tally.warning, 1);
        assert_eq!(reading.base_total.tests, 3);
        assert_eq!(reading.trial.cases.len(), 1);
        assert_eq!(reading.trial.tally.failed, 1);
        assert_eq!(
            reading.unplaced.len(),
            1,
            "a case the outline does not place"
        );
        assert_eq!(reading.unplaced_total.tests, 1);
        assert_eq!(
            reading.unplaced_total.errored, 1,
            "the unplaced row is counted, outside BASE and the options"
        );
        assert_eq!(reading.base_total.errored, 0);
    }

    #[test]
    fn tallies_add_count_by_count() {
        let a = Tally {
            tests: 3,
            passed: 1,
            failed: 1,
            warning: 0,
            errored: 1,
            errored_against_server: 1,
            skipped: 0,
        };
        let b = Tally {
            tests: 3,
            passed: 0,
            failed: 0,
            warning: 1,
            errored: 1,
            errored_against_server: 0,
            skipped: 1,
        };
        assert_eq!(
            a.plus(b),
            Tally {
                tests: 6,
                passed: 1,
                failed: 1,
                warning: 1,
                errored: 2,
                errored_against_server: 1,
                skipped: 1,
            }
        );
        assert_eq!(a.plus(b).against_server(), 2);
    }

    #[test]
    fn an_errored_row_counts_against_the_server_by_its_attribution() {
        let tally = Tally::of(&[
            row("c/a", Status::Errored, Some(Whose::Instrument)),
            row("c/b", Status::Errored, Some(Whose::Server)),
            row("c/c", Status::Errored, None),
        ]);
        assert_eq!(tally.errored, 3);
        assert_eq!(
            tally.errored_against_server, 2,
            "server-named and unattributed"
        );
        assert_eq!(tally.against_server(), 2);
    }

    #[test]
    fn an_option_lists_every_arm_and_a_report_lists_its_cases() {
        let results = results();
        let reading = read(&results);
        let [chapter] = reading.options.chapters.as_slice() else {
            panic!("{:?}", reading.options)
        };
        let [band] = chapter.bands.as_slice() else {
            panic!("{chapter:?}")
        };
        assert_eq!(band.folder, "composition");
        assert_eq!(
            band.trial.as_ref().map(|t| t.cases.len()),
            Some(1),
            "the TRIAL case is read in its band under OPTIONS"
        );
        assert_eq!(band.tally.tests, 2, "the option case and the TRIAL case");
        let [option] = band.options.as_slice() else {
            panic!("{band:?}")
        };
        assert!(option.selected);
        assert_eq!(option.folder, Some("composition"));
        let arms: Vec<(&str, bool, bool, usize)> = option
            .arms
            .iter()
            .map(|a| (a.arm, a.selected, a.empty, a.cases.len()))
            .collect();
        assert_eq!(
            arms,
            [
                ("composition-xml-supported", true, false, 1),
                ("composition-xml-unsupported", false, true, 0),
            ]
        );
        let [report] = reading.reports.as_slice() else {
            panic!("{:?}", reading.reports)
        };
        assert_eq!(report.report.get(), 352);
        assert_eq!(report.state, Some(ReportState::Confirmed));
        assert_eq!(report.tally.tests, 2);
    }

    #[test]
    fn results_without_an_outline_leave_every_case_unplaced() {
        let mut results = results();
        results.outline = None;
        let reading = read(&results);
        assert!(reading.base.is_empty());
        assert_eq!(reading.unplaced.len(), 5);
        assert_eq!(reading.total.tests, 6);
    }
}
