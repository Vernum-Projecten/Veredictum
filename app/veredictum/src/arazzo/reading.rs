// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The reading of a results document every view prints: BASE by chapter,
//! band and section, then the options by tier, the TRIAL-grounded cases, and
//! the cases resting on text reported as defective.
//!
//! The grouping is read from the [`Outline`] the results carry, so no view
//! invents its own. A case is BASE when no option arm names its document;
//! a BASE case whose citations rest on TRIAL text is read with the options,
//! never in a BASE tally. A row whose server did not follow a SHOULD is
//! counted as advisory and never as failed (RFC 2119 §3, which the ITS-REST
//! overview's keywords follow: there "may exist valid reasons in particular
//! circumstances to ignore a particular item"). Every list keeps document
//! order, so the reading is as deterministic as the results. No openEHR spec
//! governs the reading — our own design.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use crate::arazzo::results::{ResultRow, Results, Status, Whose};
use crate::arazzo::structure::{Formalism, Outline, Place, ReportState, Standing, Theme, Tier};

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
    pub advisory: usize,
    /// The rows the instrument could not judge.
    pub errored: usize,
    /// The rows not run.
    pub skipped: usize,
}

impl Tally {
    /// Counts `rows`, a failed advisory row as advisory.
    #[must_use]
    pub fn of<'a>(rows: impl IntoIterator<Item = &'a ResultRow>) -> Self {
        let mut tally = Self::default();
        for row in rows {
            tally.add(row);
        }
        tally
    }

    /// Counts one more row.
    pub fn add(&mut self, row: &ResultRow) {
        self.tests += 1;
        match row.status {
            Status::Passed => self.passed += 1,
            Status::Failed if is_advisory(row) => self.advisory += 1,
            Status::Failed => self.failed += 1,
            Status::Errored => self.errored += 1,
            Status::Skipped => self.skipped += 1,
        }
    }
}

/// Whether `row` failed only a SHOULD: failed, and its explanation says the
/// row is advisory.
#[must_use]
pub fn is_advisory(row: &ResultRow) -> bool {
    row.status == Status::Failed
        && row
            .explanation
            .as_ref()
            .is_some_and(|e| e.whose == Whose::Advisory)
}

/// What one row reads as, the mark every view prints beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    /// Passed.
    Passed,
    /// Failed, counting against the server.
    Failed,
    /// Failed a SHOULD only.
    Advisory,
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
            Status::Failed if is_advisory(row) => Self::Advisory,
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
            Self::Advisory => "!",
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
            Self::Advisory => "advisory",
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
    /// The chapter and band it is read under: those of the first folder its
    /// documents sit in.
    pub chapter: Option<&'a str>,
    /// The band.
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
    /// The options whose support is a SHOULD.
    pub recommended: Vec<OptionGroup<'a>>,
    /// The options a MAY or the release's silence leaves open.
    pub options: Vec<OptionGroup<'a>>,
    /// The BASE cases resting on TRIAL text.
    pub trial: TrialGroup<'a>,
    /// The options that exist only because the released text is defective.
    pub boundary: Vec<OptionGroup<'a>>,
    /// Every case resting on text an upstream report records, by report.
    pub reports: Vec<ReportGroup<'a>>,
    /// The cases the results place nowhere: every case, when the results
    /// carry no outline.
    pub unplaced: Vec<CaseRows<'a>>,
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
            recommended: Vec::new(),
            options: Vec::new(),
            trial: TrialGroup {
                tally: Tally::default(),
                cases: Vec::new(),
            },
            boundary: Vec::new(),
            reports: Vec::new(),
            unplaced: all,
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
    let mut groups = option_groups(outline, &arm_cases);
    let mut by_tier = |tier: Tier| -> Vec<OptionGroup<'_>> {
        let (taken, kept): (Vec<_>, Vec<_>) = groups.drain(..).partition(|g| g.tier == tier);
        groups = kept;
        taken
    };
    let recommended = by_tier(Tier::Recommended);
    let options = by_tier(Tier::Options);
    let boundary = by_tier(Tier::SpecBoundary);
    Reading {
        total,
        base_total,
        base,
        recommended,
        options,
        trial: TrialGroup {
            tally: tally_of(&trial),
            cases: trial,
        },
        boundary,
        reports: report_groups(outline, &all),
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

fn option_groups<'a>(
    outline: &'a Outline,
    arm_cases: &BTreeMap<(&str, &str), Vec<CaseRows<'a>>>,
) -> Vec<OptionGroup<'a>> {
    let order: Vec<(&str, &str)> = outline
        .chapters
        .iter()
        .flat_map(|c| {
            c.folders
                .iter()
                .map(move |f| (c.chapter.as_str(), f.folder.as_str()))
        })
        .collect();
    let mut groups: Vec<(usize, OptionGroup<'a>)> = outline
        .options
        .iter()
        .map(|option| {
            let arms: Vec<ArmGroup<'a>> = option
                .arms
                .iter()
                .map(|arm| {
                    let cases = arm_cases
                        .get(&(option.option.as_str(), arm.arm.as_str()))
                        .cloned()
                        .unwrap_or_default();
                    ArmGroup {
                        arm: &arm.arm,
                        selected: arm.selected,
                        empty: arm.documents.is_empty(),
                        tally: tally_of(&cases),
                        cases,
                    }
                })
                .collect();
            let tally = Tally::of(
                arms.iter()
                    .flat_map(|a| a.cases.iter().flat_map(|c| c.rows.iter().copied())),
            );
            let first = option
                .folders
                .first()
                .and_then(|f| order.iter().position(|(_, folder)| folder == f));
            let (chapter, folder) = first
                .and_then(|at| order.get(at))
                .map_or((None, None), |(c, f)| (Some(*c), Some(*f)));
            (
                first.unwrap_or(usize::MAX),
                OptionGroup {
                    option: &option.option,
                    description: &option.description,
                    theme: option.theme,
                    tier: option.tier,
                    report: option.report,
                    chapter,
                    folder,
                    selected: arms.iter().any(|a| a.selected),
                    tally,
                    arms,
                },
            )
        })
        .collect();
    groups.sort_by(|(a, x), (b, y)| {
        a.cmp(b)
            .then(x.theme.cmp(&y.theme))
            .then(x.option.cmp(y.option))
    });
    groups.into_iter().map(|(_, group)| group).collect()
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
            base_url: "http://sut".to_owned(),
            server: None,
            review: Vec::new(),
            rows: vec![
                row("c/create", Status::Passed, None),
                row("c/create", Status::Failed, Some(Whose::Server)),
                row("c/read", Status::Failed, Some(Whose::Advisory)),
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
    fn an_advisory_row_is_counted_apart_from_a_failed_one() {
        let results = results();
        let total = Tally::of(&results.rows);
        assert_eq!(total.failed, 2);
        assert_eq!(total.advisory, 1);
        assert_eq!(total.tests, 6);
        assert_eq!(Mark::of(&results.rows[2]), Mark::Advisory);
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
        assert_eq!(band.tally.advisory, 1);
        assert_eq!(reading.base_total.tests, 3);
        assert_eq!(reading.trial.cases.len(), 1);
        assert_eq!(reading.trial.tally.failed, 1);
        assert_eq!(
            reading.unplaced.len(),
            1,
            "a case the outline does not place"
        );
    }

    #[test]
    fn an_option_lists_every_arm_and_a_report_lists_its_cases() {
        let results = results();
        let reading = read(&results);
        assert!(reading.recommended.is_empty() && reading.boundary.is_empty());
        let [option] = reading.options.as_slice() else {
            panic!("{:?}", reading.options)
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
