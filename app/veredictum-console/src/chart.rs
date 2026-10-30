// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The chapter bars (#867): a result drawn as one stacked bar per band, under
//! one shaded header per chapter or option tier.
//!
//! The SVG is a deterministic function of the reading the results page
//! receives, rendered on the server and again, byte for byte, during
//! hydration, so the chart reads with JavaScript off. Colour is never the only
//! carrier: every outcome has its glyph in the legend and in the column heads,
//! and skipped rows are hatched. The palette is Okabe-Ito
//! (<https://jfly.uni-koeln.de/color/>), one variant per console theme. No
//! openEHR spec governs a chart — our own design.

use std::fmt::Write as _;

use crate::record_api::{ReadingView, Tally};

/// The whole width of the drawing, in user units.
const WIDTH: u32 = 960;

/// Where a chapter title starts.
const GROUP_X: u32 = 12;

/// Where a band title starts.
const BAND_X: u32 = 28;

/// Where every bar starts.
const BAR_X: f64 = 250.0;

/// The width of a full bar: the scale's test count.
const BAR_W: f64 = 420.0;

/// The right edge of each count column and its head: the test count, then
/// one column per outcome in the bar's order.
const COLUMNS: [(u32, &str); 6] = [
    (722, "tests"),
    (774, "✓"),
    (818, "✕"),
    (862, "!"),
    (906, "?"),
    (948, "○"),
];

/// The height of the legend and column-head block above the first group.
const HEAD_H: u32 = 52;

/// The height of a group header.
const GROUP_H: u32 = 26;

/// The height of a band row.
const ROW_H: u32 = 22;

/// The height of one bar inside its row.
const BAR_H: u32 = 12;

/// The gap above a bar inside its row, which centres it.
const BAR_INSET: u32 = 5;

/// One band: a stacked bar and its counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Band {
    /// The band's heading.
    pub title: String,
    /// Where the band links: its rows on the results page.
    pub href: String,
    /// Its counts.
    pub tally: Tally,
}

/// One shaded group of bands: a chapter, or an option tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The group's heading.
    pub title: String,
    /// Where the heading links.
    pub href: String,
    /// The group's counts.
    pub tally: Tally,
    /// Its bands, in reading order.
    pub bands: Vec<Band>,
}

/// One chart: what it draws and the name a reader hears for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chart {
    /// The accessible name of the drawing.
    pub label: String,
    /// The groups, in reading order.
    pub groups: Vec<Group>,
}

/// The console theme a drawing is coloured for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Palette {
    /// The light theme.
    Light,
    /// The dark theme.
    Dark,
}

/// The colours of one palette.
struct Colours {
    /// Passed.
    passed: &'static str,
    /// Failed.
    failed: &'static str,
    /// Advisory.
    advisory: &'static str,
    /// Errored.
    errored: &'static str,
    /// Skipped: the hatch ground.
    skipped: &'static str,
    /// Skipped: the hatch lines.
    hatch: &'static str,
    /// Body text.
    ink: &'static str,
    /// Secondary text.
    muted: &'static str,
    /// The group header shade.
    shade: &'static str,
    /// The outline of an empty bar track.
    track: &'static str,
    /// The suffix that keeps the pattern id unique on a page carrying both
    /// variants.
    suffix: &'static str,
}

impl Palette {
    /// The colours of this palette.
    const fn colours(self) -> Colours {
        match self {
            Self::Light => Colours {
                passed: "#0072B2",
                failed: "#D55E00",
                advisory: "#CC79A7",
                errored: "#E69F00",
                skipped: "#D9D9D9",
                hatch: "#8C8C8C",
                ink: "#363636",
                muted: "#6A6259",
                shade: "#FDF1E6",
                track: "#E8DCCF",
                suffix: "light",
            },
            Self::Dark => Colours {
                passed: "#56B4E9",
                failed: "#E8803A",
                advisory: "#D99BC0",
                errored: "#F0B840",
                skipped: "#4A5A62",
                hatch: "#9AA8AE",
                ink: "#EDE7DE",
                muted: "#A8B4BA",
                shade: "#0B1820",
                track: "#28454F",
                suffix: "dark",
            },
        }
    }
}

/// `text` with the five XML-special characters escaped.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// The five outcome counts in bar order, each with its colour.
fn segments(tally: &Tally, colours: &Colours) -> [(u32, String); 5] {
    [
        (tally.passed, colours.passed.to_owned()),
        (tally.failed, colours.failed.to_owned()),
        (tally.advisory, colours.advisory.to_owned()),
        (tally.errored, colours.errored.to_owned()),
        (tally.skipped, format!("url(#hatch-{})", colours.suffix)),
    ]
}

/// The counts in column order, the test count first.
const fn column_values(tally: &Tally) -> [u32; 6] {
    [
        tally.tests,
        tally.passed,
        tally.failed,
        tally.advisory,
        tally.errored,
        tally.skipped,
    ]
}

/// One line of text naming every count of `tally`, the tooltip of a row.
fn summary(title: &str, tally: &Tally) -> String {
    format!(
        "{title}: {} tests, {} passed, {} failed, {} advisory, {} errored, {} skipped",
        tally.tests, tally.passed, tally.failed, tally.advisory, tally.errored, tally.skipped
    )
}

/// The test count a full bar stands for: the largest band of the chart.
fn scale(chart: &Chart) -> u32 {
    chart
        .groups
        .iter()
        .flat_map(|group| group.bands.iter())
        .map(|band| band.tally.tests)
        .max()
        .unwrap_or_default()
        .max(1)
}

/// Writes the legend and the column heads.
fn head(out: &mut String, colours: &Colours, scale: u32) {
    let legend = [
        ("✓", "passed", colours.passed.to_owned()),
        ("✕", "failed", colours.failed.to_owned()),
        ("!", "advisory", colours.advisory.to_owned()),
        ("?", "errored", colours.errored.to_owned()),
        (
            "○",
            "skipped or N/A",
            format!("url(#hatch-{})", colours.suffix),
        ),
    ];
    let mut x = GROUP_X;
    for (glyph, word, fill) in legend {
        let _ = write!(
            out,
            r#"<rect x="{x}" y="8" width="12" height="12" fill="{fill}"/><text x="{}" y="18" fill="{}">{glyph} {word}</text>"#,
            x + 16,
            colours.ink
        );
        x += if word.len() > 8 { 140 } else { 96 };
    }
    let _ = write!(
        out,
        r#"<text x="{}" y="18" text-anchor="end" fill="{}">{scale} tests = full bar</text>"#,
        COLUMNS[5].0, colours.muted
    );
    for (right, label) in COLUMNS {
        let _ = write!(
            out,
            r#"<text x="{right}" y="44" text-anchor="end" fill="{}">{label}</text>"#,
            colours.muted
        );
    }
}

/// Writes the counts of `tally` right-aligned in the columns, on the
/// baseline `y`.
fn counts(out: &mut String, tally: &Tally, y: u32, fill: &str, weight: &str) {
    for ((right, _), value) in COLUMNS.iter().zip(column_values(tally)) {
        let _ = write!(
            out,
            r#"<text x="{right}" y="{y}" text-anchor="end" fill="{fill}"{weight}>{value}</text>"#
        );
    }
}

/// Writes one band row whose top edge is `top`.
fn band(out: &mut String, band: &Band, top: u32, colours: &Colours, scale: u32) {
    let baseline = top + 15;
    let _ = write!(
        out,
        r#"<a href="{}"><title>{}</title><rect x="0" y="{top}" width="{WIDTH}" height="{ROW_H}" fill="transparent"/><text x="{BAND_X}" y="{baseline}" fill="{}">{}</text>"#,
        esc(&band.href),
        esc(&summary(&band.title, &band.tally)),
        colours.ink,
        esc(&band.title)
    );
    let bar_top = top + BAR_INSET;
    if band.tally.tests == 0 {
        let _ = write!(
            out,
            r#"<text x="{BAR_X}" y="{baseline}" fill="{}" font-style="italic">no cases</text></a>"#,
            colours.muted
        );
        return;
    }
    let full = f64::from(scale);
    let mut done = 0_u32;
    for (count, fill) in segments(&band.tally, colours) {
        if count == 0 {
            continue;
        }
        let from = BAR_X + BAR_W * f64::from(done) / full;
        done = done.saturating_add(count);
        let to = BAR_X + BAR_W * f64::from(done) / full;
        let _ = write!(
            out,
            r#"<rect x="{from:.2}" y="{bar_top}" width="{:.2}" height="{BAR_H}" fill="{fill}"/>"#,
            to - from
        );
    }
    let _ = write!(
        out,
        r#"<rect x="{BAR_X}" y="{bar_top}" width="{:.2}" height="{BAR_H}" fill="none" stroke="{}" stroke-width="0.5"/>"#,
        BAR_W * f64::from(done) / full,
        colours.track
    );
    counts(out, &band.tally, baseline, colours.ink, "");
    out.push_str("</a>");
}

/// Renders `chart` as one self-contained SVG document in `palette`.
#[must_use]
pub fn render(chart: &Chart, palette: Palette) -> String {
    let colours = palette.colours();
    let scale = scale(chart);
    let rows: u32 = chart
        .groups
        .iter()
        .map(|g| {
            let bands = u32::try_from(g.bands.len()).unwrap_or(u32::MAX);
            GROUP_H.saturating_add(ROW_H.saturating_mul(bands))
        })
        .fold(0_u32, u32::saturating_add);
    let height = HEAD_H.saturating_add(rows).saturating_add(8);
    let mut out = String::new();
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {WIDTH} {height}" width="100%" role="group" aria-label="{}" font-family="ui-sans-serif, system-ui, sans-serif" font-size="12">"#,
        esc(&chart.label)
    );
    let _ = write!(
        out,
        r#"<defs><pattern id="hatch-{s}" patternUnits="userSpaceOnUse" width="6" height="6" patternTransform="rotate(45)"><rect width="6" height="6" fill="{}"/><line x1="0" y1="0" x2="0" y2="6" stroke="{}" stroke-width="2"/></pattern></defs>"#,
        colours.skipped,
        colours.hatch,
        s = colours.suffix
    );
    head(&mut out, &colours, scale);
    let mut top = HEAD_H;
    for group in &chart.groups {
        let _ = write!(
            out,
            r#"<a href="{}"><title>{}</title><rect x="0" y="{top}" width="{WIDTH}" height="{GROUP_H}" fill="{}"/><text x="{GROUP_X}" y="{}" fill="{}" font-weight="600">{}</text>"#,
            esc(&group.href),
            esc(&summary(&group.title, &group.tally)),
            colours.shade,
            top + 17,
            colours.ink,
            esc(&group.title)
        );
        counts(
            &mut out,
            &group.tally,
            top + 17,
            colours.ink,
            r#" font-weight="600""#,
        );
        out.push_str("</a>");
        top += GROUP_H;
        for each in &group.bands {
            band(&mut out, each, top, &colours, scale);
            top += ROW_H;
        }
    }
    out.push_str("</svg>");
    out
}

/// Which part of a result a chart draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// BASE, chapter by chapter.
    Base,
    /// The options, tier by tier.
    Options,
}

impl View {
    /// The view the `?view=` token names; BASE for anything else.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        if token == "options" {
            Self::Options
        } else {
            Self::Base
        }
    }
}

/// The BASE chart: one group per chapter, one band per capability folder.
#[must_use]
pub fn base(reading: &ReadingView) -> Chart {
    Chart {
        label: String::from("BASE outcomes by chapter"),
        groups: reading
            .chapters
            .iter()
            .map(|chapter| Group {
                title: chapter.title.clone(),
                href: chapter.href.clone(),
                tally: chapter.tally,
                bands: chapter
                    .bands
                    .iter()
                    .map(|band| Band {
                        title: band.title.clone(),
                        href: format!("#{}", band.anchor),
                        tally: band.tally,
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The OPTIONS chart: one group per tier, one band per option, and the
/// TRIAL-grounded cases as a band of the OPTIONS tier.
#[must_use]
pub fn options(reading: &ReadingView) -> Chart {
    let groups = reading
        .tiers
        .iter()
        .map(|tier| {
            let mut bands: Vec<Band> = tier
                .options
                .iter()
                .map(|option| Band {
                    title: option.option.clone(),
                    href: format!("#{}", option.anchor),
                    tally: option.tally,
                })
                .collect();
            if let Some(trial) = tier.trial.as_ref() {
                bands.push(Band {
                    title: trial.title.clone(),
                    href: format!("#{}", trial.anchor),
                    tally: trial.tally,
                });
            }
            Group {
                title: tier.title.clone(),
                href: format!("#{}", tier.anchor),
                tally: tier.tally,
                bands,
            }
        })
        .collect();
    Chart {
        label: String::from("OPTIONS outcomes by tier"),
        groups,
    }
}

/// The chart `view` draws of `reading`.
#[must_use]
pub fn of(reading: &ReadingView, view: View) -> Chart {
    match view {
        View::Base => base(reading),
        View::Options => options(reading),
    }
}
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use std::collections::BTreeMap;
    use std::num::NonZeroU64;

    use veredictum::arazzo::results::{Engine, Explanation, ResultRow, Results, Status, Whose};
    use veredictum::arazzo::structure::{
        ArmRef, ChapterDecl, FolderDecl, Outline, OutlineArm, OutlineOption, OutlineReport, Place,
        ReportState, SectionDecl, Standing, Theme, Tier,
    };

    use super::{Palette, base, options, render};
    use crate::record_api::read::reading_view;

    /// One row of `case` with `status`, explained as `whose` when red.
    fn row(case: &str, row: usize, status: Status, whose: Option<Whose>) -> ResultRow {
        ResultRow {
            case: case.to_owned(),
            row,
            status,
            inputs: BTreeMap::new(),
            generated: Vec::new(),
            skip: None,
            explanation: whose.map(|whose| Explanation {
                step: String::from("s"),
                what: String::from("w"),
                why: None,
                expected: String::from("e"),
                received: String::from("r"),
                exchange: None,
                whose,
            }),
        }
    }

    /// A BASE place in `folder` under `section`.
    fn base_place(chapter: &str, folder: &str, section: &str) -> Place {
        Place {
            chapter: chapter.to_owned(),
            folder: folder.to_owned(),
            standing: Standing::Base {
                section: section.to_owned(),
            },
            trial: false,
            reports: Vec::new(),
            formalism: None,
        }
    }

    /// An option place on `option`'s `arm`.
    fn arm_place(option: &str) -> Place {
        Place {
            standing: Standing::Option {
                arms: vec![ArmRef {
                    option: option.to_owned(),
                    arm: format!("{option}-supported"),
                }],
            },
            ..base_place("ehr", "composition", "")
        }
    }

    /// One option of `tier` with a selected and an empty arm.
    fn option(name: &str, tier: Tier, report: Option<NonZeroU64>) -> OutlineOption {
        OutlineOption {
            option: name.to_owned(),
            description: format!("What {name} decides."),
            theme: Theme::LeftOpen,
            tier,
            report,
            folders: vec![String::from("composition")],
            arms: vec![
                OutlineArm {
                    arm: format!("{name}-supported"),
                    selected: true,
                    documents: vec![format!("{name}_doc")],
                },
                OutlineArm {
                    arm: format!("{name}-unsupported"),
                    selected: false,
                    documents: Vec::new(),
                },
            ],
        }
    }

    /// One declared folder with its sections.
    fn folder(folder: &str, title: &str, keys: &[(&str, &str)]) -> FolderDecl {
        FolderDecl {
            folder: folder.to_owned(),
            title: title.to_owned(),
            formalism: false,
            sections: keys
                .iter()
                .map(|(section, title)| SectionDecl {
                    section: (*section).to_owned(),
                    title: (*title).to_owned(),
                })
                .collect(),
        }
    }

    /// The outline of [`results`].
    fn outline(report: Option<NonZeroU64>) -> Outline {
        let mut trial = base_place("ehr", "composition", "read");
        trial.trial = true;
        let mut reported = base_place("ehr", "composition", "create");
        reported.reports = report.into_iter().collect();
        Outline {
            chapters: vec![
                ChapterDecl {
                    chapter: String::from("ehr"),
                    title: String::from("EHR"),
                    folders: vec![
                        folder("ehr", "EHR", &[("create", "Create")]),
                        folder(
                            "composition",
                            "COMPOSITION",
                            &[("create", "Create"), ("read", "Read")],
                        ),
                        folder("directory", "DIRECTORY & folders", &[("read", "Read")]),
                    ],
                },
                ChapterDecl {
                    chapter: String::from("query"),
                    title: String::from("Query"),
                    folders: vec![folder("query", "AQL", &[("aql", "AQL")])],
                },
            ],
            options: vec![
                option("flat", Tier::Recommended, None),
                option("gap", Tier::SpecBoundary, report),
                option("xml", Tier::Options, None),
            ],
            reports: report
                .map(|report| OutlineReport {
                    report,
                    title: String::from("EHR_STATUS has no flat form"),
                    state: ReportState::Confirmed,
                })
                .into_iter()
                .collect(),
            cases: BTreeMap::from([
                (
                    String::from("ehr_create/ok"),
                    base_place("ehr", "ehr", "create"),
                ),
                (String::from("composition_create/ok"), reported),
                (
                    String::from("composition_read/ok"),
                    base_place("ehr", "composition", "read"),
                ),
                (String::from("composition_read/trial"), trial),
                (
                    String::from("query_aql/ok"),
                    base_place("query", "query", "aql"),
                ),
                (String::from("xml_doc/read"), arm_place("xml")),
                (String::from("flat_doc/read"), arm_place("flat")),
                (String::from("gap_doc/read"), arm_place("gap")),
            ]),
        }
    }

    /// A fixed results document touching every part of the reading: two
    /// chapters, a band with no case, all five marks, every tier, a TRIAL
    /// case and a reported one.
    fn results() -> Results {
        Results {
            document: String::from("catalogue"),
            engine: Engine {
                name: String::from("veredictum"),
                version: String::from("0"),
            },
            instrument_status: veredictum::party::InstrumentStatus::CURRENT,
            base_url: String::from("http://sut"),
            server: None,
            review: Vec::new(),
            rows: vec![
                row("ehr_create/ok", 0, Status::Passed, None),
                row("ehr_create/ok", 1, Status::Failed, Some(Whose::Server)),
                row("composition_create/ok", 0, Status::Passed, None),
                row(
                    "composition_create/ok",
                    1,
                    Status::Errored,
                    Some(Whose::Instrument),
                ),
                row(
                    "composition_read/ok",
                    0,
                    Status::Failed,
                    Some(Whose::Advisory),
                ),
                row("composition_read/ok", 1, Status::Skipped, None),
                row(
                    "composition_read/trial",
                    0,
                    Status::Failed,
                    Some(Whose::Server),
                ),
                row("query_aql/ok", 0, Status::Passed, None),
                row("query_aql/ok", 1, Status::Passed, None),
                row("query_aql/ok", 2, Status::Passed, None),
                row("xml_doc/read", 0, Status::Passed, None),
                row("flat_doc/read", 0, Status::Failed, Some(Whose::Advisory)),
                row("gap_doc/read", 0, Status::Failed, Some(Whose::Server)),
            ],
            outline: Some(outline(NonZeroU64::new(352))),
        }
    }

    /// The reading keeps an advisory row out of the failed count, puts the
    /// TRIAL case on the OPTIONS tier, and lists a group red first.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn an_advisory_row_is_never_counted_as_failed() -> Result<(), serde_json::Error> {
        let reading = reading_view(&results())?;
        assert_eq!(reading.total.failed, 3);
        assert_eq!(reading.total.advisory, 2);
        let options = reading
            .tiers
            .get(1)
            .map(|t| (t.title.as_str(), t.trial.as_ref().map(|x| x.tally.failed)));
        assert_eq!(options, Some(("OPTIONS", Some(1))));
        let first = reading
            .chapters
            .first()
            .and_then(|c| c.bands.get(1))
            .and_then(|b| b.sections.first())
            .and_then(|s| s.rows.first())
            .map(|r| (r.mark.as_str(), r.reports.clone()));
        assert_eq!(
            first,
            Some(("errored", vec![352])),
            "red first, report marked"
        );
        Ok(())
    }

    /// The BASE chart, pinned in both themes.
    #[test]
    fn the_base_chart_is_pinned() -> Result<(), serde_json::Error> {
        let chart = base(&reading_view(&results())?);
        insta::assert_snapshot!("base_light", render(&chart, Palette::Light));
        insta::assert_snapshot!("base_dark", render(&chart, Palette::Dark));
        Ok(())
    }

    /// The OPTIONS chart, pinned in both themes.
    #[test]
    fn the_options_chart_is_pinned() -> Result<(), serde_json::Error> {
        let chart = options(&reading_view(&results())?);
        insta::assert_snapshot!("options_light", render(&chart, Palette::Light));
        insta::assert_snapshot!("options_dark", render(&chart, Palette::Dark));
        Ok(())
    }
}
