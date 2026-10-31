// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The static badges drawn from a result document.
//!
//! One badge per fact, each written as a flat two-part SVG and as a
//! Shields.io endpoint JSON document
//! (<https://shields.io/badges/endpoint-badge>). An Arazzo [`Results`]
//! document draws five badges ([`BadgeName::CONFORMANCE`]); a [`PerfResult`]
//! draws one, its knee as the record states it ([`perf_badge`]).
//!
//! Every badge is a pure function of the document, so drawing the same
//! document twice writes the same bytes: text is measured with a fixed
//! per-character width table, never with a font, and nothing is read from a
//! clock or the network. The BASE and OPTIONS counts come from the reading
//! every other view prints ([`crate::arazzo::reading::read`]), so a badge never
//! counts a row differently from `show` or `export`. Every counting badge names
//! the specification profile in its own label, because a badge can be embedded
//! alone and a development tally must never read as a released one. Every
//! badge reports the vendor's own run, `self-run`, and no badge says
//! certified. No openEHR spec governs badges — our own design.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use serde::Serialize;

use crate::arazzo::reading::{Tally, read};
use crate::arazzo::results::Results;
use crate::arazzo::spec_profile::Specification;
use crate::perf_result::PerfResult;
use crate::perf_result::capacity::Knee;
use crate::perf_result::derive::BetweenRunNoise;
use crate::perf_result::view::format_rate;

/// One badge the instrument draws, by the file name it is written under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BadgeName {
    /// The BASE rows: passed out of every BASE row.
    Base,
    /// The rows of the selected option arms, with the BASE cases resting on
    /// TRIAL text, which are read with the options.
    Options,
    /// The count of warning rows: a SHOULD the server did not follow.
    Warnings,
    /// The specification profile the run was judged against.
    Profile,
    /// The instrument and its version, and that the run is the vendor's own.
    Instrument,
    /// A `perf-result`'s knee, as the record states it.
    Perf,
}

impl BadgeName {
    /// Every badge of a conformance result, in the order they are drawn.
    pub const CONFORMANCE: [Self; 5] = [
        Self::Base,
        Self::Options,
        Self::Warnings,
        Self::Profile,
        Self::Instrument,
    ];

    /// The file stem the badge is written under (`base` for `base.svg` and
    /// `base.json`).
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Options => "options",
            Self::Warnings => "warnings",
            Self::Profile => "profile",
            Self::Instrument => "instrument",
            Self::Perf => "perf",
        }
    }
}

/// The colour of a badge's message part, chosen by what the message says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeColour {
    /// Rows passed, and none failed or errored: green.
    Passing,
    /// At least one row failed: red, whatever else the tally holds.
    Failing,
    /// No row failed but at least one errored, which the instrument could not
    /// judge: orange, never green.
    Errored,
    /// Nothing passed and nothing failed or errored (no rows, or every row
    /// skipped): grey.
    Empty,
    /// At least one warning row: yellow.
    Warned,
    /// The released profile: blue.
    Released,
    /// The development profile: purple.
    Development,
    /// The instrument badge: the brand's deep teal.
    Instrument,
    /// A reading that is neither passed nor failed, such as a knee: slate.
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
            Self::Warned => "dfb317",
            Self::Released => "007ec6",
            Self::Development => "8250df",
            Self::Instrument => "1b6e92",
            Self::Reading => "5b7083",
        }
    }

    /// The colour of a tally: red when any row failed, orange when none failed
    /// but one errored, grey when nothing passed, and green otherwise.
    #[must_use]
    pub const fn of(tally: Tally) -> Self {
        if tally.failed > 0 {
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

/// One drawn badge: its label, its message and the message's colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    /// Which badge it is.
    pub name: BadgeName,
    /// The left-hand text.
    pub label: String,
    /// The right-hand text.
    pub message: String,
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
    /// The text a screen reader announces and the SVG's `<title>` holds.
    #[must_use]
    pub fn title(&self) -> String {
        format!("{}: {}", self.label, self.message)
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

/// The message of a counting badge: passed out of every row, then the failed
/// and errored counts when there are any.
fn tally_message(tally: Tally) -> String {
    if tally.tests == 0 {
        return String::from("no rows");
    }
    let mut message = format!("{}/{} passed", tally.passed, tally.tests);
    if tally.failed > 0 {
        let _ = write!(message, " \u{b7} {} failed", tally.failed);
    }
    if tally.errored > 0 {
        let _ = write!(message, " \u{b7} {} errored", tally.errored);
    }
    message
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
/// so both counting badges then read `no outline` in grey.
#[must_use]
pub fn badges(results: &Results) -> Vec<Badge> {
    let reading = read(results);
    let profile = results.specification.profile().token();
    let placed = results.outline.is_some();
    let counted = |tally: Tally| {
        if placed {
            (tally_message(tally), BadgeColour::of(tally))
        } else {
            (String::from("no outline"), BadgeColour::Empty)
        }
    };
    let (base_message, base_colour) = counted(reading.base_total);
    let (options_message, options_colour) = counted(reading.options.selected_tally());
    let warnings = reading.total.warning;
    BadgeName::CONFORMANCE
        .into_iter()
        .map(|name| {
            let (label, message, colour) = match name {
                BadgeName::Base => (
                    format!("openEHR BASE \u{b7} {profile}"),
                    base_message.clone(),
                    base_colour,
                ),
                BadgeName::Options => (
                    format!("openEHR options \u{b7} {profile}"),
                    options_message.clone(),
                    options_colour,
                ),
                BadgeName::Warnings => (
                    format!("openEHR warnings \u{b7} {profile}"),
                    warnings.to_string(),
                    if warnings > 0 {
                        BadgeColour::Warned
                    } else {
                        BadgeColour::Passing
                    },
                ),
                BadgeName::Profile => (
                    String::from("specification"),
                    profile_message(&results.specification),
                    if results.specification.is_released() {
                        BadgeColour::Released
                    } else {
                        BadgeColour::Development
                    },
                ),
                BadgeName::Instrument | BadgeName::Perf => (
                    String::from("veredictum"),
                    format!("self-run \u{b7} v{}", results.engine.version),
                    BadgeColour::Instrument,
                ),
            };
            Badge {
                name,
                label,
                message,
                colour,
            }
        })
        .collect()
}

/// The knee as the record states it: the bracket, the lower bound, or the
/// state the ladder ended in.
fn knee_message(knee: &Knee) -> String {
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
        Knee::BelowLadder { .. } => String::from("below ladder"),
        Knee::NoPlateau => String::from("no plateau"),
    }
}

/// Draws the one badge of a `perf-result`: its workload in the label, and the
/// knee, that the run is the vendor's own single pass, and whether an A/A
/// floor measured the noise between runs in the message.
///
/// The colour is [`BadgeColour::Reading`] whatever the knee, because a knee
/// is a reading of capacity and never a pass or a failure. The workload is
/// its recorded label: `<id>@<version>` for a workload the instrument pins,
/// `custom` for any other.
#[must_use]
pub fn perf_badge(record: &PerfResult) -> Badge {
    let noise = match record.derived.between_run_noise {
        BetweenRunNoise::Unmeasured => "1 pass, noise unmeasured",
        BetweenRunNoise::AaFloor(_) => "1 pass, A/A floor attached",
    };
    Badge {
        name: BadgeName::Perf,
        label: format!("openEHR perf \u{b7} {}", record.raw.workload.label),
        message: format!(
            "{} \u{b7} self-run \u{b7} {noise}",
            knee_message(&record.derived.knee)
        ),
        colour: BadgeColour::Reading,
    }
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
            skipped,
        }
    }

    #[test]
    fn a_failed_or_errored_row_is_never_green() {
        assert_eq!(BadgeColour::of(tally(9, 1, 0, 0)), BadgeColour::Failing);
        assert_eq!(BadgeColour::of(tally(9, 1, 1, 0)), BadgeColour::Failing);
        assert_eq!(BadgeColour::of(tally(9, 0, 1, 0)), BadgeColour::Errored);
        assert_eq!(BadgeColour::of(tally(0, 0, 0, 3)), BadgeColour::Empty);
        assert_eq!(BadgeColour::of(tally(0, 0, 0, 0)), BadgeColour::Empty);
        assert_eq!(BadgeColour::of(tally(9, 0, 0, 3)), BadgeColour::Passing);
    }

    #[test]
    fn a_tally_message_names_failed_and_errored_rows() {
        assert_eq!(tally_message(tally(9, 0, 0, 1)), "9/10 passed");
        assert_eq!(
            tally_message(tally(7, 2, 1, 0)),
            "7/10 passed \u{b7} 2 failed \u{b7} 1 errored"
        );
        assert_eq!(tally_message(tally(0, 0, 0, 0)), "no rows");
    }

    #[test]
    fn every_knee_state_reads_as_the_record_states_it() {
        assert_eq!(
            knee_message(&Knee::Bracket {
                qualified_rate: 100.0,
                achieved_per_s: 99.9,
                failing_rate: 150.0,
            }),
            "knee 100.0\u{2013}150.0/s"
        );
        assert_eq!(
            knee_message(&Knee::LowerBound {
                qualified_rate: 200.0,
                achieved_per_s: 199.0,
            }),
            "knee \u{2265} 200.0/s"
        );
        assert_eq!(
            knee_message(&Knee::BelowLadder { failing_rate: 5.0 }),
            "below ladder"
        );
        assert_eq!(knee_message(&Knee::NoPlateau), "no plateau");
    }

    #[test]
    fn text_outside_the_width_table_is_measured_wide_and_escaped() {
        assert_eq!(text_width("\u{2014}"), 110);
        assert_eq!(xml_escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
    }
}
