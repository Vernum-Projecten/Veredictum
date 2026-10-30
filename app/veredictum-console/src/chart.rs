// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The chapter bars (#867): a result drawn as one stacked bar per band, under
//! one shaded header per chapter, for BASE or for OPTIONS.
//!
//! Only what the run touched is drawn: a chapter or band with no row (and,
//! for an option, no selected arm) is left out and named in one line after
//! the bars. An option row carries its kind beside its name, an option of
//! the spec-boundary tier the quiet `@SpecDiverge` tag in its place. Every bar is its own band's rows at full width, so a small
//! band reads as clearly as a large one, and the count column carries the
//! size. The SVG is a deterministic function of the reading the results page
//! receives, rendered on the server and again, byte for byte, during
//! hydration, so the chart reads with JavaScript off. Colour is never the only
//! carrier: every outcome has its glyph in the legend and in the column heads,
//! and skipped rows are hatched. The palette is Okabe-Ito
//! (<https://jfly.uni-koeln.de/color/>), one variant per console theme. No
//! openEHR spec governs a chart — our own design.

use std::fmt::Write as _;

use crate::record_api::{OptionView, ReadingView, Tally};
use crate::spec_diverge::TAG;

/// The whole width of the drawing, in user units.
const WIDTH: u32 = 960;

/// Where a chapter title starts.
const GROUP_X: u32 = 12;

/// Where a band title starts.
const BAND_X: u32 = 28;

/// Where a row's tag starts: an option's kind or its `@SpecDiverge` tag.
const TAG_X: u32 = 262;

/// Where every bar starts.
const BAR_X: f64 = 384.0;

/// The width of a full bar: all of its band's rows.
const BAR_W: f64 = 290.0;

/// The right edge of each count column and its head: the row count, then
/// one column per outcome in the bar's order.
const COLUMNS: [(u32, &str); 6] = [
    (722, "rows"),
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

/// Where an indented row's title starts: an option under its band.
const OPTION_X: u32 = 44;

/// The height of a band row.
const ROW_H: u32 = 22;

/// The height of one bar inside its row.
const BAR_H: u32 = 12;

/// The gap above a bar inside its row, which centres it.
const BAR_INSET: u32 = 5;

/// The height of one line naming what the run left out.
const NOTE_H: u32 = 18;

/// The characters one note line holds before it wraps.
const NOTE_CHARS: usize = 140;

/// The words that open a note naming what the run left out.
pub const NOT_IN_RUN: &str = "Not in this run:";

/// The small label a row carries after its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// The words.
    pub text: String,
    /// Whether it is the `@SpecDiverge` tag, set in monospace.
    pub mono: bool,
}

/// One bar row: a capability folder, or an option or TRIAL row indented
/// under its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Band {
    /// The row's heading.
    pub title: String,
    /// Where the row links: its rows on the results page.
    pub href: String,
    /// Its counts.
    pub tally: Tally,
    /// Whether the row sits indented under the band above it.
    pub indent: bool,
    /// What the row reads in place of a bar when it has no row.
    pub empty: &'static str,
    /// The label after its name, when it carries one.
    pub tag: Option<Tag>,
}

/// One shaded group of bands: a chapter.
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

/// One part of a chart: BASE or OPTIONS, its chapters as groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// The part's counts.
    pub tally: Tally,
    /// Its groups, in reading order.
    pub groups: Vec<Group>,
}

/// One chart: what it draws and the name a reader hears for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chart {
    /// The accessible name of the drawing.
    pub label: String,
    /// The parts, in reading order.
    pub parts: Vec<Part>,
    /// The titles of the chapters the run left out, in reading order.
    pub absent: Vec<String>,
    /// What the chart reads when it draws no part.
    pub empty: &'static str,
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
    /// Warning.
    warning: &'static str,
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
    /// The outline of a bar.
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
                warning: "#CC79A7",
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
                warning: "#D99BC0",
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
        (tally.warning, colours.warning.to_owned()),
        (tally.errored, colours.errored.to_owned()),
        (tally.skipped, format!("url(#hatch-{})", colours.suffix)),
    ]
}

/// The counts in column order, the row count first.
const fn column_values(tally: &Tally) -> [u32; 6] {
    [
        tally.tests,
        tally.passed,
        tally.failed,
        tally.warning,
        tally.errored,
        tally.skipped,
    ]
}

/// One line of text naming every count of `tally`, the tooltip of a row.
fn summary(title: &str, tally: &Tally) -> String {
    let rows = if tally.tests == 1 { "row" } else { "rows" };
    format!(
        "{title}: {} {rows}, {} passed, {} failed, {} warning, {} errored, {} skipped",
        tally.tests, tally.passed, tally.failed, tally.warning, tally.errored, tally.skipped
    )
}

/// `titles` after `prefix` as lines of at most `width` characters, broken
/// between titles; empty when there is no title.
#[must_use]
pub fn wrap(prefix: &str, titles: &[String], width: usize) -> Vec<String> {
    if titles.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut line = String::from(prefix);
    let mut fresh = true;
    for (at, title) in titles.iter().enumerate() {
        let piece = if at + 1 == titles.len() {
            title.clone()
        } else {
            format!("{title},")
        };
        let wide = line.chars().count() + 1 + piece.chars().count();
        if wide > width && !fresh {
            lines.push(std::mem::take(&mut line));
        } else if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&piece);
        fresh = false;
    }
    lines.push(line);
    lines
}

/// Writes the legend and the column heads.
fn head(out: &mut String, colours: &Colours) {
    let legend = [
        ("✓", "passed", colours.passed.to_owned()),
        ("✕", "failed", colours.failed.to_owned()),
        ("!", "warning", colours.warning.to_owned()),
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
        r#"<text x="{}" y="18" text-anchor="end" fill="{}">each bar is 100% of its own rows</text>"#,
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

/// Writes the stacked bar of `tally` at full width, its top edge at `top`.
fn bar(out: &mut String, tally: &Tally, top: u32, height: u32, colours: &Colours) {
    let full = f64::from(tally.tests.max(1));
    let mut done = 0_u32;
    for (count, fill) in segments(tally, colours) {
        if count == 0 {
            continue;
        }
        let from = BAR_X + BAR_W * f64::from(done) / full;
        done = done.saturating_add(count);
        let to = BAR_X + BAR_W * f64::from(done) / full;
        let _ = write!(
            out,
            r#"<rect x="{from:.2}" y="{top}" width="{:.2}" height="{height}" fill="{fill}"/>"#,
            to - from
        );
    }
    let _ = write!(
        out,
        r#"<rect x="{BAR_X}" y="{top}" width="{BAR_W}" height="{height}" fill="none" stroke="{}" stroke-width="0.5"/>"#,
        colours.track
    );
}

/// Writes the tag of a row on the baseline `y`.
fn tag(out: &mut String, tag: &Tag, y: u32, colours: &Colours) {
    let family = if tag.mono {
        r#" font-family="ui-monospace, monospace""#
    } else {
        ""
    };
    let _ = write!(
        out,
        r#"<text x="{TAG_X}" y="{y}" fill="{}" font-size="10"{family}>{}</text>"#,
        colours.muted,
        esc(&tag.text)
    );
}

/// Writes one band row whose top edge is `top`.
fn band(out: &mut String, band: &Band, top: u32, colours: &Colours) {
    let baseline = top + 15;
    let _ = write!(
        out,
        r#"<a href="{}"><title>{}</title><rect x="0" y="{top}" width="{WIDTH}" height="{ROW_H}" fill="transparent"/><text x="{}" y="{baseline}" fill="{}">{}</text>"#,
        esc(&band.href),
        esc(&summary(&band.title, &band.tally)),
        if band.indent { OPTION_X } else { BAND_X },
        colours.ink,
        esc(&band.title)
    );
    if let Some(label) = &band.tag {
        tag(out, label, baseline, colours);
    }
    if band.tally.tests == 0 {
        let _ = write!(
            out,
            r#"<text x="{BAR_X}" y="{baseline}" fill="{}" font-style="italic">{}</text></a>"#,
            colours.muted,
            esc(band.empty)
        );
        return;
    }
    bar(out, &band.tally, top + BAR_INSET, BAR_H, colours);
    counts(out, &band.tally, baseline, colours.ink, "");
    out.push_str("</a>");
}

/// Writes one chapter header whose top edge is `top`.
fn group_header(out: &mut String, group: &Group, top: u32, colours: &Colours) {
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
        out,
        &group.tally,
        top + 17,
        colours.ink,
        r#" font-weight="600""#,
    );
    out.push_str("</a>");
}

/// Writes the muted line naming `titles`, after `prefix`, as left out of the
/// run, from `top`, and returns the top edge after it.
fn absent(out: &mut String, prefix: &str, titles: &[String], top: u32, colours: &Colours) -> u32 {
    let mut top = top;
    for line in wrap(prefix, titles, NOTE_CHARS) {
        let _ = write!(
            out,
            r#"<text x="{GROUP_X}" y="{}" fill="{}" font-style="italic">{}</text>"#,
            top + 13,
            colours.muted,
            esc(&line)
        );
        top += NOTE_H;
    }
    top
}

/// Renders `chart` as one self-contained SVG document in `palette`.
#[must_use]
pub fn render(chart: &Chart, palette: Palette) -> String {
    let colours = palette.colours();
    let mut body = String::new();
    head(&mut body, &colours);
    let mut top = HEAD_H;
    if chart.parts.is_empty() {
        let _ = write!(
            body,
            r#"<text x="{GROUP_X}" y="{}" fill="{}" font-style="italic">{}</text>"#,
            top + 15,
            colours.muted,
            esc(chart.empty)
        );
        top += ROW_H;
    }
    for part in &chart.parts {
        for group in &part.groups {
            group_header(&mut body, group, top, &colours);
            top += GROUP_H;
            for each in &group.bands {
                band(&mut body, each, top, &colours);
                top += ROW_H;
            }
        }
    }
    top = absent(&mut body, NOT_IN_RUN, &chart.absent, top, &colours);
    let height = top.saturating_add(8);
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
    out.push_str(&body);
    out.push_str("</svg>");
    out
}

/// Which part of a result a chart draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// BASE, chapter by chapter.
    Base,
    /// OPTIONS, chapter by chapter.
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

/// The BASE chart: one group per chapter the run touched, one band per
/// capability folder holding a row.
#[must_use]
pub fn base(reading: &ReadingView) -> Chart {
    let mut absent = Vec::new();
    let mut groups = Vec::new();
    for chapter in &reading.chapters {
        if chapter.tally.tests == 0 {
            absent.push(chapter.title.clone());
            continue;
        }
        groups.push(Group {
            title: chapter.title.clone(),
            href: chapter.href.clone(),
            tally: chapter.tally,
            bands: chapter
                .bands
                .iter()
                .filter(|band| band.tally.tests > 0)
                .map(|band| Band {
                    title: band.title.clone(),
                    href: format!("#{}", band.anchor),
                    tally: band.tally,
                    indent: false,
                    empty: "no rows",
                    tag: None,
                })
                .collect(),
        });
    }
    let parts = if groups.is_empty() {
        Vec::new()
    } else {
        vec![Part {
            tally: reading
                .chapters
                .iter()
                .map(|c| c.tally)
                .fold(Tally::default(), Tally::plus),
            groups,
        }]
    };
    Chart {
        label: String::from("BASE outcomes by chapter"),
        parts,
        absent,
        empty: "No BASE row in this run.",
    }
}

/// The row of one option: its counts, or why it has none.
fn option_band(option: &OptionView, indent: bool) -> Band {
    let empty = if option.arms.iter().all(|arm| arm.empty) {
        "nothing to run"
    } else if option.selected {
        "selected, no rows"
    } else {
        "not selected"
    };
    let text = if option.diverges {
        String::from(TAG)
    } else {
        option.kind.clone()
    };
    Band {
        title: option.option.clone(),
        href: format!("#{}", option.anchor),
        tally: option.tally,
        indent,
        empty,
        tag: Some(Tag {
            text,
            mono: option.diverges,
        }),
    }
}

/// The OPTIONS chart: each chapter the run touched, each band followed by
/// its options and its TRIAL row, indented, exactly as BASE reads.
#[must_use]
pub fn options(reading: &ReadingView) -> Chart {
    let tree = &reading.options;
    let mut absent = Vec::new();
    let mut groups = Vec::new();
    for chapter in &tree.chapters {
        if !chapter.in_run() {
            absent.push(chapter.title.clone());
            continue;
        }
        let mut bands = Vec::new();
        for band in chapter.bands.iter().filter(|band| band.in_run()) {
            bands.push(Band {
                title: band.title.clone(),
                href: format!("#{}", band.anchor),
                tally: band.tally,
                indent: false,
                empty: "no rows",
                tag: None,
            });
            bands.extend(
                band.options
                    .iter()
                    .filter(|option| option.in_run())
                    .map(|option| option_band(option, true)),
            );
            if let Some(trial) = band.trial.as_ref().filter(|t| t.tally.tests > 0) {
                bands.push(Band {
                    title: String::from("TRIAL text"),
                    href: format!("#{}", trial.anchor),
                    tally: trial.tally,
                    indent: true,
                    empty: "no rows",
                    tag: None,
                });
            }
        }
        groups.push(Group {
            title: chapter.title.clone(),
            href: format!("#{}", chapter.anchor),
            tally: chapter.tally,
            bands,
        });
    }
    let unanchored: Vec<Band> = tree
        .unanchored
        .iter()
        .filter(|option| option.in_run())
        .map(|option| option_band(option, false))
        .collect();
    if !unanchored.is_empty() {
        groups.push(Group {
            title: String::from("No document yet"),
            href: format!("#{}", tree.anchor),
            tally: Tally::default(),
            bands: unanchored,
        });
    }
    let parts = if groups.is_empty() {
        Vec::new()
    } else {
        vec![Part {
            tally: tree.tally,
            groups,
        }]
    };
    Chart {
        label: String::from("Option outcomes by chapter"),
        parts,
        absent,
        empty: "No option row in this run.",
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

    use super::{Palette, base, options, render, wrap};
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
                ChapterDecl {
                    chapter: String::from("definitions"),
                    title: String::from("Definitions"),
                    folders: vec![folder("template", "Templates", &[("upload", "Upload")])],
                },
            ],
            options: vec![
                option("flat", Tier::Recommended, None),
                option("gap", Tier::SpecBoundary, report),
                option("xml", Tier::Options, None),
                OutlineOption {
                    folders: Vec::new(),
                    arms: vec![OutlineArm {
                        arm: String::from("planned-supported"),
                        selected: false,
                        documents: Vec::new(),
                    }],
                    ..option("planned", Tier::Options, None)
                },
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
    /// chapters with rows and one without, a band with no case, all five
    /// marks, every kind of option, a TRIAL case and a reported one.
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
                    Some(Whose::Warning),
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
                row("flat_doc/read", 0, Status::Failed, Some(Whose::Warning)),
                row("gap_doc/read", 0, Status::Failed, Some(Whose::Server)),
            ],
            outline: Some(outline(NonZeroU64::new(352))),
        }
    }

    /// The reading keeps a warning row out of the failed count, puts the
    /// TRIAL case in its OPTIONS band, and lists a group red first.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_warning_row_is_never_counted_as_failed() -> Result<(), serde_json::Error> {
        let reading = reading_view(&results())?;
        assert_eq!(reading.total.failed, 3);
        assert_eq!(reading.total.warning, 2);
        let band = reading
            .options
            .chapters
            .first()
            .and_then(|c| c.bands.first())
            .map(|band| {
                (
                    band.anchor.as_str(),
                    band.trial.as_ref().map(|x| x.tally.failed),
                )
            });
        assert_eq!(
            band,
            Some(("options-band-composition", Some(1))),
            "the TRIAL row sits in its band, in OPTIONS"
        );
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

    /// Every option sits in one OPTIONS tree, Recommended, Optional, then
    /// the spec-boundary ones within its band, each labelled with the
    /// engine's kind, the spec-boundary one marked as diverging with its
    /// report.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn every_option_carries_its_kind_in_one_tree() -> Result<(), serde_json::Error> {
        let reading = reading_view(&results())?;
        let kinds: Vec<(&str, bool, Option<u64>)> = reading
            .options
            .options()
            .map(|o| (o.option.as_str(), o.diverges, o.report))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("flat", false, None),
                ("xml", false, None),
                ("gap", true, Some(352)),
                ("planned", false, None),
            ]
        );
        let labels: Vec<&str> = reading
            .options
            .options()
            .filter(|o| !o.diverges)
            .map(|o| o.kind.as_str())
            .collect();
        assert_eq!(
            labels,
            vec!["Recommended (SHOULD)", "Optional (MAY)", "Optional (MAY)"]
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

    /// A chapter and a band with no row are not drawn, and the chapter is
    /// named in the line after the bars.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_chapter_the_run_never_touched_is_named_not_drawn() -> Result<(), serde_json::Error> {
        let chart = base(&reading_view(&results())?);
        let drawn: Vec<(&str, Vec<&str>)> = chart
            .parts
            .iter()
            .flat_map(|part| part.groups.iter())
            .map(|group| {
                (
                    group.title.as_str(),
                    group.bands.iter().map(|b| b.title.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            drawn,
            vec![("EHR", vec!["EHR", "COMPOSITION"]), ("Query", vec!["AQL"])],
            "the DIRECTORY band and the Definitions chapter hold no row"
        );
        assert_eq!(chart.absent, vec![String::from("Definitions")]);
        let svg = render(&chart, Palette::Light);
        assert!(
            svg.contains("Not in this run: Definitions"),
            "the left-out chapter is named: {svg}"
        );
        assert!(!svg.contains("DIRECTORY"), "an empty band is not drawn");
        Ok(())
    }

    /// The OPTIONS chart is one part with no tier header: each option row
    /// carries its kind after its name, a spec-boundary one the quiet
    /// `@SpecDiverge` tag, and an unselected option with no row is not drawn.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn the_options_chart_labels_each_option_with_its_kind() -> Result<(), serde_json::Error> {
        let chart = options(&reading_view(&results())?);
        assert_eq!(chart.parts.len(), 1, "OPTIONS is one part");
        let rows: Vec<(&str, Option<&str>)> = chart
            .parts
            .iter()
            .flat_map(|p| p.groups.iter())
            .flat_map(|g| g.bands.iter())
            .map(|b| (b.title.as_str(), b.tag.as_ref().map(|t| t.text.as_str())))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("COMPOSITION", None),
                ("flat", Some("Recommended (SHOULD)")),
                ("xml", Some("Optional (MAY)")),
                ("gap", Some("@SpecDiverge")),
                ("TRIAL text", None),
            ],
            "an unselected option with no row (planned) is not drawn"
        );
        let svg = render(&chart, Palette::Dark);
        assert!(
            !svg.contains("Tiers not in this run") && !svg.contains("Spec defect"),
            "no tier header, no tier note and no warning: {svg}"
        );
        Ok(())
    }

    /// The BASE chart carries no warning on a band whose cases rest on
    /// reported text.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn the_base_chart_tags_no_band() -> Result<(), serde_json::Error> {
        let chart = base(&reading_view(&results())?);
        assert!(
            chart
                .parts
                .iter()
                .flat_map(|p| p.groups.iter())
                .flat_map(|g| g.bands.iter())
                .all(|b| b.tag.is_none())
        );
        Ok(())
    }

    /// Every bar is its own band's rows at full width, whatever the size of
    /// the largest band.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn every_bar_spans_its_own_rows() -> Result<(), serde_json::Error> {
        let chart = base(&reading_view(&results())?);
        let svg = render(&chart, Palette::Light);
        let outlines = svg
            .matches(r#"width="290" height="12" fill="none""#)
            .count();
        assert_eq!(outlines, 3, "one full-width outline per drawn band: {svg}");
        assert!(!svg.contains("full bar"), "no scale legend is left");
        Ok(())
    }

    /// A long list of left-out titles wraps between titles, never inside one.
    #[test]
    fn a_long_absent_line_wraps_between_titles() {
        let titles: Vec<String> = ["Definitions", "Query", "Demographics"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(
            wrap("Not in this run:", &titles, 40),
            vec![
                String::from("Not in this run: Definitions, Query,"),
                String::from("Demographics"),
            ]
        );
        assert_eq!(
            wrap("Not in this run:", &titles, 200),
            vec![String::from(
                "Not in this run: Definitions, Query, Demographics"
            )]
        );
        assert!(wrap("Not in this run:", &[], 40).is_empty());
    }
}
