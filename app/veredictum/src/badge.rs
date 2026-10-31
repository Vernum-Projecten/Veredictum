// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The static badges drawn from a result document.
//!
//! One badge per fact, each written as a flat two-part SVG and as a
//! Shields.io endpoint JSON document
//! (<https://shields.io/badges/endpoint-badge>). An Arazzo [`Results`]
//! document draws four badges ([`BadgeName::CONFORMANCE`]); a [`PerfResult`]
//! draws three ([`BadgeName::PERF`]): its knee, its between-run noise and the
//! instrument ([`perf_badges`]).
//!
//! The label names the subject in a few words and the message gives one
//! short value, with no separator character in either. The SVG `<title>` and
//! `aria-label` carry the full text ([`Badge::detail`]), so a hover or a screen
//! reader still gets every count the message leaves out.
//!
//! Every badge is a pure function of the document, so drawing the same
//! document twice writes the same bytes: text is measured with a fixed
//! per-character width table, never with a font, and nothing is read from a
//! clock or the network. The BASE and OPTIONS counts come from the reading
//! every other view prints ([`crate::arazzo::reading::read`]), so a badge never
//! counts a row differently from `show` or `export`. A counting badge drawn
//! from the development profile says `dev` in its own label, because a badge
//! can be embedded alone and a development tally must never read as a released
//! one. Every badge reports the vendor's own run, `self-run`, and no badge
//! says certified. No openEHR spec governs badges — our own design.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::arazzo::reading::{Tally, read};
use crate::arazzo::results::Results;
use crate::arazzo::spec_profile::Specification;
use crate::perf_result::PerfResult;
use crate::perf_result::capacity::Knee;
use crate::perf_result::derive::BetweenRunNoise;
use crate::perf_result::view::{format_rate, format_rate_ceil, format_rate_floor};

/// One badge the instrument draws, by the file name it is written under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BadgeName {
    /// The BASE rows and every row the outline places nowhere: passed out of
    /// the rows that bear on the verdict, then their warnings.
    Base,
    /// The rows of the selected option arms, with the BASE cases resting on
    /// TRIAL text, which are read with the options, counted as on
    /// [`BadgeName::Base`].
    Options,
    /// The specification profile the run was judged against.
    Profile,
    /// The instrument and its version, and that the run is the vendor's own.
    Instrument,
    /// A `perf-result`'s knee, in whole requests per second.
    Perf,
    /// Whether a `perf-result` carries an A/A floor for the noise between
    /// runs.
    Noise,
}

impl BadgeName {
    /// Every badge of a conformance result, in the order they are drawn.
    pub const CONFORMANCE: [Self; 4] = [Self::Base, Self::Options, Self::Profile, Self::Instrument];

    /// Every badge of a `perf-result`, in the order they are drawn.
    pub const PERF: [Self; 3] = [Self::Perf, Self::Noise, Self::Instrument];

    /// The file stem the badge is written under (`base` for `base.svg` and
    /// `base.json`).
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Options => "options",
            Self::Profile => "profile",
            Self::Instrument => "instrument",
            Self::Perf => "perf",
            Self::Noise => "noise",
        }
    }
}

/// The colour of a badge's message part, chosen by what the message says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeColour {
    /// Rows passed, and none failed or errored: green.
    Passing,
    /// At least one row counts against the server: it failed, or errored
    /// with the server or no one named. Red, whatever else the tally holds.
    Failing,
    /// No row counts against the server, but at least one errored row is
    /// inconclusive, which the instrument could not judge: orange, never
    /// green.
    Errored,
    /// Nothing passed and nothing failed or errored (no rows, or every row
    /// skipped), or a measurement the record does not carry: grey.
    Empty,
    /// The released profile: blue.
    Released,
    /// The development profile: purple.
    Development,
    /// The instrument badge: the brand's deep teal.
    Instrument,
    /// A reading that is neither passed nor failed, such as a knee or an
    /// attached A/A floor: slate.
    Reading,
}

impl BadgeColour {
    /// The colour as six lower-case hex digits, without a `#`.
    #[must_use]
    pub const fn hex(self) -> &'static str {
        match self {
            Self::Passing => "44cc11",
            Self::Failing => "e05d44",
            Self::Errored => "fe7d37",
            Self::Empty => "9f9f9f",
            Self::Released => "007ec6",
            Self::Development => "8250df",
            Self::Instrument => "1b6e92",
            Self::Reading => "5b7083",
        }
    }

    /// The colour of a tally: red when any row counts against the server
    /// (failed, or errored with the server or no one named), orange when none
    /// does but one is inconclusive, grey when nothing passed, and green
    /// otherwise. A warning row enters none of these rules.
    #[must_use]
    pub const fn of(tally: Tally) -> Self {
        if tally.against_server() > 0 {
            Self::Failing
        } else if tally.errored > 0 {
            Self::Errored
        } else if tally.passed == 0 {
            Self::Empty
        } else {
            Self::Passing
        }
    }
}

/// One drawn badge: its label, its message, the message in full, and the
/// message's colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    /// Which badge it is.
    pub name: BadgeName,
    /// The left-hand text: the subject, in a few words.
    pub label: String,
    /// The right-hand text: one short value.
    pub message: String,
    /// The message in full, which the SVG `<title>` and `aria-label` carry:
    /// every count of a tally, or a knee at the precision the record states
    /// it. Equal to [`Badge::message`] when the message leaves nothing out.
    pub detail: String,
    /// The colour of the right-hand part.
    pub colour: BadgeColour,
}

/// The colour of the label part, Shields' default.
const LABEL_COLOUR: &str = "555";

/// The colour the label text and the icon are drawn in.
const LABEL_TEXT: &str = "fff";

// NOTE: no openEHR spec governs this — our own design; the geometry is copied from the brand
// mark assets/brand/veredictum-icon-mono.svg, which lies outside the crate's packaged files.
const ICON_CIRCLE: &str = r#"<circle cx="32" cy="32" r="29" stroke-width="2.4"/>"#;
const ICON_PATH: &str = r#"<path d="M18.5 27 L29 45.5 L46.5 17.5" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/>"#;

/// The badge height, in pixels.
const HEIGHT: usize = 20;
/// The icon's edge, in pixels.
const ICON: usize = 14;
/// The icon's distance from the left edge.
const ICON_X: usize = 5;
/// The icon's distance from the top edge.
const ICON_Y: usize = 3;
/// The space between the icon and the label text.
const GAP: usize = 3;
/// The space on either side of a text.
const PAD: usize = 5;

/// The icon as a standalone SVG document whose stroke is white: the Shields
/// endpoint's `logoSvg`.
#[must_use]
pub fn icon_svg() -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><g fill="none" stroke="#{LABEL_TEXT}">{ICON_CIRCLE}{ICON_PATH}</g></svg>"##
    )
}

/// The advance width of `c` in tenths of a pixel, approximating Verdana at
/// 11px; a character outside the table counts as wide, so a text is never
/// measured narrower than it draws.
const fn char_width(c: char) -> usize {
    match c {
        'i' | 'l' => 30,
        '\'' => 29,
        'j' => 38,
        ' ' | 'f' => 39,
        ',' | '.' | '\u{b7}' => 40,
        '!' | 't' => 43,
        'I' => 46,
        'r' => 47,
        '(' | ')' | '/' | ':' | ';' | '[' | ']' | '\\' | '|' => 49,
        '"' | '-' | 'J' => 50,
        's' => 57,
        'c' | 'z' => 58,
        '?' => 60,
        'L' => 62,
        'F' => 63,
        'k' | 'v' | 'x' | 'y' => 65,
        'a' | 'e' | 'o' | 'P' => 66,
        'b' | 'd' | 'g' | 'p' | 'q' | 'T' | 'Y' => 68,
        'h' | 'n' | 'u' => 69,
        '0'..='9' | '$' | '*' | '_' | '`' | '{' | '}' | 'E' | '\u{2013}' => 70,
        'A' | 'S' | 'V' | 'X' | 'Z' => 75,
        'B' | 'K' => 76,
        'C' | 'R' => 77,
        '&' | 'U' => 80,
        'N' => 82,
        'H' => 83,
        'D' | 'G' => 85,
        'O' | 'Q' => 87,
        'w' => 89,
        '#' | '+' | '<' | '=' | '>' | '^' | '~' | '\u{2265}' => 90,
        'M' => 93,
        'm' => 106,
        'W' => 109,
        _ => 110,
    }
}

/// The width of `text` in tenths of a pixel.
fn text_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

/// `text` escaped for XML character data and attribute values.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// The Shields.io endpoint document: the members the schema requires, and
/// `logoSvg` for the icon.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Endpoint<'a> {
    schema_version: u8,
    label: &'a str,
    message: &'a str,
    color: &'a str,
    logo_svg: String,
}

impl Badge {
    /// The text a screen reader announces and the SVG's `<title>` holds: the
    /// label and the message in full.
    #[must_use]
    pub fn title(&self) -> String {
        format!("{}: {}", self.label, self.detail)
    }

    /// The badge as a flat two-part SVG: the icon and the label on the left
    /// on dark grey, the message on the right in the badge's colour.
    #[must_use]
    pub fn svg(&self) -> String {
        let label_text = text_width(&self.label);
        let message_text = text_width(&self.message);
        let label_start = ICON_X + ICON + GAP;
        let label_width = label_start + label_text.div_ceil(10) + PAD;
        let message_width = PAD + message_text.div_ceil(10) + PAD;
        let width = label_width + message_width;
        let label_centre = label_start * 10 + label_text.div_euclid(2);
        let message_centre = label_width * 10 + message_width * 5;
        let title = xml_escape(&self.title());
        let label = xml_escape(&self.label);
        let message = xml_escape(&self.message);
        let colour = self.colour.hex();
        format!(
            concat!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{HEIGHT}" role="img" aria-label="{title}">"#,
                "<title>{title}</title>",
                r##"<linearGradient id="s" x2="0" y2="100%"><stop offset="0" stop-color="#bbb" stop-opacity=".1"/><stop offset="1" stop-opacity=".1"/></linearGradient>"##,
                r##"<clipPath id="r"><rect width="{width}" height="{HEIGHT}" rx="3" fill="#fff"/></clipPath>"##,
                r##"<g clip-path="url(#r)"><rect width="{label_width}" height="{HEIGHT}" fill="#{LABEL_COLOUR}"/><rect x="{label_width}" width="{message_width}" height="{HEIGHT}" fill="#{colour}"/><rect width="{width}" height="{HEIGHT}" fill="url(#s)"/></g>"##,
                r##"<svg x="{ICON_X}" y="{ICON_Y}" width="{ICON}" height="{ICON}" viewBox="0 0 64 64" aria-hidden="true"><g fill="none" stroke="#{LABEL_TEXT}">{ICON_CIRCLE}{ICON_PATH}</g></svg>"##,
                r##"<g fill="#{LABEL_TEXT}" text-anchor="middle" font-family="Verdana,Geneva,DejaVu Sans,sans-serif" text-rendering="geometricPrecision" font-size="110">"##,
                r##"<text aria-hidden="true" x="{label_centre}" y="150" fill="#010101" fill-opacity=".3" transform="scale(.1)" textLength="{label_text}">{label}</text>"##,
                r#"<text aria-hidden="true" x="{label_centre}" y="140" transform="scale(.1)" textLength="{label_text}">{label}</text>"#,
                r##"<text aria-hidden="true" x="{message_centre}" y="150" fill="#010101" fill-opacity=".3" transform="scale(.1)" textLength="{message_text}">{message}</text>"##,
                r#"<text aria-hidden="true" x="{message_centre}" y="140" transform="scale(.1)" textLength="{message_text}">{message}</text>"#,
                "</g></svg>\n",
            ),
            width = width,
            HEIGHT = HEIGHT,
            title = title,
            label_width = label_width,
            LABEL_COLOUR = LABEL_COLOUR,
            message_width = message_width,
            colour = colour,
            ICON_X = ICON_X,
            ICON_Y = ICON_Y,
            ICON = ICON,
            LABEL_TEXT = LABEL_TEXT,
            ICON_CIRCLE = ICON_CIRCLE,
            ICON_PATH = ICON_PATH,
            label_centre = label_centre,
            label_text = label_text,
            label = label,
            message_centre = message_centre,
            message_text = message_text,
            message = message,
        )
    }

    /// The badge as a Shields.io endpoint document, pretty-printed with a
    /// trailing newline.
    ///
    /// # Errors
    /// [`serde_json::Error`] when the document cannot be serialized, which a
    /// struct of strings never fails to be.
    pub fn endpoint_json(&self) -> Result<String, serde_json::Error> {
        let endpoint = Endpoint {
            schema_version: 1,
            label: &self.label,
            message: &self.message,
            color: self.colour.hex(),
            logo_svg: icon_svg(),
        };
        let mut text = serde_json::to_string_pretty(&endpoint)?;
        text.push('\n');
        Ok(text)
    }
}

/// The headline of a tally: passed out of the rows that bear on the verdict
/// (passed, failed and errored), `none judged` when none does, or `no rows`.
fn tally_ratio(tally: Tally) -> String {
    if tally.tests == 0 {
        return String::from("no rows");
    }
    let judged = tally
        .passed
        .saturating_add(tally.failed)
        .saturating_add(tally.errored);
    if judged == 0 {
        String::from("none judged")
    } else {
        format!("{}/{judged} passed", tally.passed)
    }
}

/// The warning count of a tally, when there is one: `1 warning`, `N warnings`.
fn tally_warnings(tally: Tally) -> Option<String> {
    match tally.warning {
        0 => None,
        1 => Some(String::from("1 warning")),
        n => Some(format!("{n} warnings")),
    }
}

/// The message of a counting badge: the ratio, then the warning count when
/// there is one.
///
/// A warning row is a SHOULD the server did not follow: it is neither a pass
/// nor a failure, so it stays out of the ratio and is named after it. The
/// failed and errored counts show in the colour and in [`tally_detail`].
fn tally_message(tally: Tally) -> String {
    let ratio = tally_ratio(tally);
    match tally_warnings(tally) {
        Some(warnings) => format!("{ratio}, {warnings}"),
        None => ratio,
    }
}

/// The full text of a counting badge: the ratio, then the failed, errored
/// and warning counts when there are any.
fn tally_detail(tally: Tally) -> String {
    let mut parts = vec![tally_ratio(tally)];
    if tally.failed > 0 {
        parts.push(format!("{} failed", tally.failed));
    }
    if tally.errored > 0 {
        parts.push(format!("{} errored", tally.errored));
    }
    parts.extend(tally_warnings(tally));
    parts.join(", ")
}

/// The profile badge's message: `released`, or `development` with the
/// branches its snapshot was taken from.
fn profile_message(specification: &Specification) -> String {
    match specification {
        Specification::Released => String::from("released"),
        Specification::Development { snapshot } => {
            let branches: BTreeSet<&str> = snapshot.iter().map(|c| c.branch.as_str()).collect();
            if branches.is_empty() {
                String::from("development")
            } else {
                format!(
                    "development ({})",
                    branches.into_iter().collect::<Vec<_>>().join(", ")
                )
            }
        }
    }
}

/// Draws every badge of `results`, in [`BadgeName::CONFORMANCE`] order.
///
/// A results document without an outline places no row in BASE or OPTIONS,
/// so both counting badges then read `no outline` in grey. A counting badge of
/// a development result says `dev` in its label.
#[must_use]
pub fn badges(results: &Results) -> Vec<Badge> {
    let reading = read(results);
    let released = results.specification.is_released();
    let counted = |name: BadgeName, subject: &str, tally: Tally| {
        let label = if released {
            format!("openEHR {subject}")
        } else {
            format!("openEHR {subject} dev")
        };
        if results.outline.is_some() {
            Badge {
                name,
                label,
                message: tally_message(tally),
                detail: tally_detail(tally),
                colour: BadgeColour::of(tally),
            }
        } else {
            Badge {
                name,
                label,
                message: String::from("no outline"),
                detail: String::from("no outline"),
                colour: BadgeColour::Empty,
            }
        }
    };
    let profile = profile_message(&results.specification);
    vec![
        // NOTE: no openEHR spec governs this — our own design; a row the outline places
        // nowhere counts on the BASE badge, so a red unplaced row never leaves it green.
        counted(
            BadgeName::Base,
            "BASE",
            reading.base_total.plus(reading.unplaced_total),
        ),
        counted(
            BadgeName::Options,
            "options",
            reading.options.selected_tally(),
        ),
        Badge {
            name: BadgeName::Profile,
            label: String::from("specification"),
            message: profile.clone(),
            detail: profile,
            colour: if released {
                BadgeColour::Released
            } else {
                BadgeColour::Development
            },
        },
        instrument_badge(&results.engine.version),
    ]
}

/// The instrument badge: the vendor's own run, drawn by `version`.
fn instrument_badge(version: &str) -> Badge {
    let message = format!("self-run v{version}");
    Badge {
        name: BadgeName::Instrument,
        label: String::from("veredictum"),
        detail: message.clone(),
        message,
        colour: BadgeColour::Instrument,
    }
}

/// The knee in whole requests per second: the bracket, the lower bound, or
/// the state the ladder ended in.
///
/// A lower end rounds down and an upper end rounds up, so a rounded bracket
/// only ever widens and a rounded lower bound never claims more than the
/// record.
fn knee_message(knee: &Knee) -> String {
    match knee {
        Knee::Bracket {
            qualified_rate,
            failing_rate,
            ..
        } => format!(
            "knee {}\u{2013}{}/s",
            format_rate_floor(*qualified_rate),
            format_rate_ceil(*failing_rate)
        ),
        Knee::LowerBound { qualified_rate, .. } => {
            format!("knee \u{2265} {}/s", format_rate_floor(*qualified_rate))
        }
        Knee::BelowLadder { .. } => String::from("below ladder"),
        Knee::NoPlateau => String::from("no plateau"),
    }
}

/// The knee as the record states it, to one decimal: the full text of the
/// perf badge.
fn knee_detail(knee: &Knee) -> String {
    match knee {
        Knee::Bracket {
            qualified_rate,
            failing_rate,
            ..
        } => format!(
            "knee {}\u{2013}{}/s",
            format_rate(*qualified_rate),
            format_rate(*failing_rate)
        ),
        Knee::LowerBound { qualified_rate, .. } => {
            format!("knee \u{2265} {}/s", format_rate(*qualified_rate))
        }
        Knee::BelowLadder { .. } | Knee::NoPlateau => knee_message(knee),
    }
}

/// Draws every badge of a `perf-result`, in [`BadgeName::PERF`] order.
///
/// `perf` carries the workload in its label and the knee in whole requests
/// per second, `noise` whether an A/A floor measured the noise between runs,
/// and `instrument` the version that wrote the record. The knee and an
/// attached floor are [`BadgeColour::Reading`], because a knee is a reading of
/// capacity and never a pass or a failure; a floor the record does not carry
/// is [`BadgeColour::Empty`]. The workload is its recorded label:
/// `<id>@<version>` for a workload the instrument pins, `custom` for any other.
#[must_use]
pub fn perf_badges(record: &PerfResult) -> Vec<Badge> {
    let knee = &record.derived.knee;
    let (noise, noise_colour) = match record.derived.between_run_noise {
        BetweenRunNoise::Unmeasured => ("not measured", BadgeColour::Empty),
        BetweenRunNoise::AaFloor(_) => ("attached", BadgeColour::Reading),
    };
    vec![
        Badge {
            name: BadgeName::Perf,
            label: format!("perf {}", record.raw.workload.label),
            message: knee_message(knee),
            detail: knee_detail(knee),
            colour: BadgeColour::Reading,
        },
        Badge {
            name: BadgeName::Noise,
            label: String::from("A/A floor"),
            message: noise.to_owned(),
            detail: noise.to_owned(),
            colour: noise_colour,
        },
        instrument_badge(&record.raw.instrument.version),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(passed: usize, failed: usize, errored: usize, skipped: usize) -> Tally {
        Tally {
            tests: passed + failed + errored + skipped,
            passed,
            failed,
            warning: 0,
            errored,
            errored_against_server: 0,
            skipped,
        }
    }

    fn warned(tally: Tally, warning: usize) -> Tally {
        Tally {
            tests: tally.tests + warning,
            warning,
            ..tally
        }
    }

    #[test]
    fn a_failed_or_errored_row_is_never_green() {
        assert_eq!(BadgeColour::of(tally(9, 1, 0, 0)), BadgeColour::Failing);
        assert_eq!(BadgeColour::of(tally(9, 1, 1, 0)), BadgeColour::Failing);
        assert_eq!(BadgeColour::of(tally(9, 0, 1, 0)), BadgeColour::Errored);
        let against = Tally {
            errored_against_server: 1,
            ..tally(9, 0, 2, 0)
        };
        assert_eq!(
            BadgeColour::of(against),
            BadgeColour::Failing,
            "an errored row against the server is red, like a failed one"
        );
        assert_eq!(BadgeColour::of(tally(0, 0, 0, 3)), BadgeColour::Empty);
        assert_eq!(BadgeColour::of(tally(0, 0, 0, 0)), BadgeColour::Empty);
        assert_eq!(BadgeColour::of(tally(9, 0, 0, 3)), BadgeColour::Passing);
    }

    #[test]
    fn a_warning_never_changes_the_colour() {
        for base in [
            tally(9, 0, 0, 0),
            tally(9, 1, 0, 0),
            tally(9, 0, 1, 0),
            tally(0, 0, 0, 3),
        ] {
            assert_eq!(BadgeColour::of(warned(base, 2)), BadgeColour::of(base));
        }
    }

    #[test]
    fn a_tally_detail_names_failed_and_errored_rows_and_the_message_keeps_the_ratio() {
        assert_eq!(tally_message(tally(9, 0, 0, 1)), "9/9 passed");
        assert_eq!(tally_detail(tally(9, 0, 0, 1)), "9/9 passed");
        assert_eq!(
            tally_detail(tally(7, 2, 1, 0)),
            "7/10 passed, 2 failed, 1 errored"
        );
        assert_eq!(tally_message(tally(7, 2, 1, 0)), "7/10 passed");
        assert_eq!(tally_message(tally(0, 0, 0, 0)), "no rows");
        assert_eq!(tally_detail(tally(0, 0, 0, 0)), "no rows");
        assert_eq!(tally_message(tally(0, 0, 0, 2)), "none judged");
        assert_eq!(tally_detail(tally(0, 0, 0, 2)), "none judged");
    }

    #[test]
    fn a_warning_is_named_after_the_ratio_and_never_inside_it() {
        assert_eq!(
            tally_message(warned(tally(3, 0, 0, 0), 1)),
            "3/3 passed, 1 warning"
        );
        assert_eq!(
            tally_detail(warned(tally(2, 1, 1, 0), 1)),
            "2/4 passed, 1 failed, 1 errored, 1 warning"
        );
        assert_eq!(
            tally_message(warned(tally(2, 1, 1, 0), 1)),
            "2/4 passed, 1 warning"
        );
        assert_eq!(
            tally_message(warned(tally(5, 0, 0, 1), 2)),
            "5/5 passed, 2 warnings"
        );
        assert_eq!(
            tally_message(warned(tally(0, 0, 0, 0), 1)),
            "none judged, 1 warning"
        );
    }

    #[test]
    fn every_knee_state_reads_in_whole_rates_and_in_full_as_the_record_states_it() {
        let bracket = Knee::Bracket {
            qualified_rate: 100.0,
            achieved_per_s: 99.9,
            failing_rate: 150.0,
        };
        assert_eq!(knee_message(&bracket), "knee 100\u{2013}150/s");
        assert_eq!(knee_detail(&bracket), "knee 100.0\u{2013}150.0/s");
        let lower = Knee::LowerBound {
            qualified_rate: 200.0,
            achieved_per_s: 199.0,
        };
        assert_eq!(knee_message(&lower), "knee \u{2265} 200/s");
        assert_eq!(knee_detail(&lower), "knee \u{2265} 200.0/s");
        let below = Knee::BelowLadder { failing_rate: 5.0 };
        assert_eq!(knee_message(&below), "below ladder");
        assert_eq!(knee_detail(&below), "below ladder");
        assert_eq!(knee_message(&Knee::NoPlateau), "no plateau");
        assert_eq!(knee_detail(&Knee::NoPlateau), "no plateau");
    }

    #[test]
    fn a_whole_rate_rounds_outward_so_a_bracket_only_widens() {
        assert_eq!(
            knee_message(&Knee::Bracket {
                qualified_rate: 180.8,
                achieved_per_s: 180.1,
                failing_rate: 210.1,
            }),
            "knee 180\u{2013}211/s"
        );
        assert_eq!(
            knee_message(&Knee::LowerBound {
                qualified_rate: 1_250.9,
                achieved_per_s: 1_249.5,
            }),
            "knee \u{2265} 1,250/s",
            "a lower bound never claims more than the record"
        );
        assert_eq!(
            knee_message(&Knee::Bracket {
                qualified_rate: 1_999.2,
                achieved_per_s: 1_999.0,
                failing_rate: 1_000_000.4,
            }),
            "knee 1,999\u{2013}1,000,001/s"
        );
    }

    #[test]
    fn the_title_carries_the_full_text_and_the_message_carries_no_separator() {
        let badge = Badge {
            name: BadgeName::Base,
            label: String::from("openEHR BASE"),
            message: tally_message(warned(tally(2, 1, 1, 0), 1)),
            detail: tally_detail(warned(tally(2, 1, 1, 0), 1)),
            colour: BadgeColour::Failing,
        };
        let full = "openEHR BASE: 2/4 passed, 1 failed, 1 errored, 1 warning";
        assert_eq!(badge.title(), full);
        let svg = badge.svg();
        assert!(svg.contains(&format!("aria-label=\"{full}\"")), "{svg}");
        assert!(svg.contains(&format!("<title>{full}</title>")), "{svg}");
        assert!(!svg.contains('\u{b7}'), "{svg}");
    }

    #[test]
    fn text_outside_the_width_table_is_measured_wide_and_escaped() {
        assert_eq!(text_width("\u{2014}"), 110);
        assert_eq!(xml_escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
    }
}
