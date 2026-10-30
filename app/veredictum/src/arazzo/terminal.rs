// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The terminal view of an Arazzo [`Results`] document.
//!
//! One line per chapter, band and option carries its counts and a
//! proportional bar, in the reading order [`crate::arazzo::reading::read`]
//! gives every view. Every mark is a glyph as well as a colour, so the view reads the same with
//! colour off. Colour is plain ANSI SGR (ECMA-48 §8.3.117): passed green,
//! failed red, errored yellow, warning and skipped grey. Whether to use it
//! is the caller's decision, which [`wants_color`] makes the way
//! <https://no-color.org/> asks. A warning row is never shown as a failure.
//! No openEHR spec governs the view — our own design.

use std::ffi::OsStr;
use std::fmt::Write as _;

use crate::arazzo::export::{WARNING_LINE, arm_state, divergence_note, option_choice, tally_words};
use crate::arazzo::reading::{
    CaseRows, Mark, OptionGroup, OptionsTree, Reading, Tally, is_warning, read,
};
use crate::arazzo::results::{ResultRow, Results};
use crate::arazzo::structure::Place;

/// The cells of every bar.
pub const BAR_WIDTH: usize = 24;

/// The column every bar starts at.
const LABEL_WIDTH: usize = 44;

/// When to colour the view, as `--color` reads it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorChoice {
    /// Colour when standard output is a terminal and `NO_COLOR` is unset or
    /// empty.
    #[default]
    Auto,
    /// Always colour.
    Always,
    /// Never colour.
    Never,
}

/// Whether to colour the view.
///
/// `Always` and `Never` decide on their own, because a command-line flag
/// overrides `NO_COLOR`. `Auto` colours only when standard output is a
/// terminal and `NO_COLOR` is absent or empty, as <https://no-color.org/>
/// defines it.
#[must_use]
pub fn wants_color(
    choice: ColorChoice,
    stdout_is_terminal: bool,
    no_color: Option<&OsStr>,
) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => stdout_is_terminal && no_color.is_none_or(OsStr::is_empty),
    }
}

/// How to render the view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShowOptions {
    /// Whether to write ANSI colour codes.
    pub color: bool,
    /// Whether to expand every failed and errored row, and list the warning
    /// rows apart.
    pub failed: bool,
}

/// One of the colours the view uses.
#[derive(Debug, Clone, Copy)]
enum Paint {
    Green,
    Red,
    Yellow,
    Grey,
    Bold,
}

impl Paint {
    const fn sgr(self) -> &'static str {
        match self {
            Self::Green => "32",
            Self::Red => "31",
            Self::Yellow => "33",
            Self::Grey => "90",
            Self::Bold => "1",
        }
    }

    const fn of(mark: Mark) -> Self {
        match mark {
            Mark::Passed => Self::Green,
            Mark::Failed => Self::Red,
            Mark::Errored => Self::Yellow,
            Mark::Warning | Mark::Skipped => Self::Grey,
        }
    }
}

/// The marks in the order a bar and a count line print them.
const ORDER: [Mark; 5] = [
    Mark::Passed,
    Mark::Failed,
    Mark::Errored,
    Mark::Warning,
    Mark::Skipped,
];

/// The cell a bar fills for each mark, distinct without colour.
const fn cell(mark: Mark) -> char {
    match mark {
        Mark::Passed => '█',
        Mark::Failed => '▓',
        Mark::Errored => '▒',
        Mark::Warning => '░',
        Mark::Skipped => '·',
    }
}

/// The count of one mark in `tally`.
const fn count(tally: Tally, mark: Mark) -> usize {
    match mark {
        Mark::Passed => tally.passed,
        Mark::Failed => tally.failed,
        Mark::Errored => tally.errored,
        Mark::Warning => tally.warning,
        Mark::Skipped => tally.skipped,
    }
}

/// The view under construction.
struct View {
    out: String,
    color: bool,
}

impl View {
    fn paint(&self, paint: Paint, text: &str) -> String {
        if self.color {
            format!("\u{1b}[{}m{text}\u{1b}[0m", paint.sgr())
        } else {
            text.to_owned()
        }
    }

    fn heading(&mut self, text: &str) {
        let line = self.paint(Paint::Bold, text);
        let _ = writeln!(self.out, "\n{line}");
    }

    fn line(&mut self, text: &str) {
        let _ = writeln!(self.out, "{text}");
    }

    /// Writes a label with its bar and counts, or `empty` in their place
    /// when the tally holds no row.
    fn tally_line(&mut self, indent: usize, label: &str, tally: Tally, empty: &str) {
        let label = format!("{}{label}", " ".repeat(indent));
        let width = label.chars().count();
        let pad = if width < LABEL_WIDTH {
            " ".repeat(LABEL_WIDTH - width)
        } else {
            format!("\n{}", " ".repeat(LABEL_WIDTH))
        };
        let right = if tally.tests == 0 {
            empty.to_owned()
        } else {
            format!("{}  {}", self.bar(tally), self.counts(tally))
        };
        let _ = writeln!(self.out, "{label}{pad}{right}");
    }

    fn bar(&self, tally: Tally) -> String {
        ORDER
            .into_iter()
            .zip(bar_cells(tally))
            .filter(|(_, cells)| *cells > 0)
            .map(|(mark, cells)| {
                let run: String = std::iter::repeat_n(cell(mark), cells).collect();
                self.paint(Paint::of(mark), &run)
            })
            .collect()
    }

    fn counts(&self, tally: Tally) -> String {
        ORDER
            .into_iter()
            .map(|mark| {
                let text = format!("{}{:>4}", mark.glyph(), count(tally, mark));
                self.paint(Paint::of(mark), &text)
            })
            .collect::<Vec<_>>()
            .join("  ")
    }

    fn case_line(&mut self, indent: usize, case: &CaseRows<'_>) {
        let mark = case.mark();
        let glyph = self.paint(Paint::of(mark), mark.glyph());
        let _ = writeln!(
            self.out,
            "{}{glyph} {}: {}{}",
            " ".repeat(indent),
            case.case,
            tally_words(case.tally()),
            badges(case.place),
        );
    }
}

/// Returns how many of the [`BAR_WIDTH`] cells each mark fills.
///
/// The marks are passed, failed, errored, warning and skipped, in that
/// order. Each fills cells in proportion to its count by largest remainder,
/// and every mark that has a row fills at least one, so a single failure
/// among thousands still shows.
#[must_use]
pub fn bar_cells(tally: Tally) -> [usize; 5] {
    let total = tally.tests;
    let mut cells = [0; 5];
    if total == 0 {
        return cells;
    }
    let counts = ORDER.map(|mark| count(tally, mark));
    let mut remainders = [0; 5];
    for ((cell, remainder), count) in cells.iter_mut().zip(remainders.iter_mut()).zip(counts) {
        let scaled = count.saturating_mul(BAR_WIDTH);
        *cell = scaled.div_euclid(total);
        *remainder = scaled.rem_euclid(total);
        if count > 0 && *cell == 0 {
            *cell = 1;
            *remainder = 0;
        }
    }
    for _ in 0..BAR_WIDTH {
        let filled: usize = cells.iter().sum();
        if filled < BAR_WIDTH {
            let first_largest = remainders
                .iter()
                .enumerate()
                .rev()
                .filter(|(_, remainder)| **remainder > 0)
                .max_by_key(|(_, remainder)| **remainder)
                .map(|(at, _)| at);
            if let Some(at) = first_largest
                && let (Some(cell), Some(remainder)) = (cells.get_mut(at), remainders.get_mut(at))
            {
                *cell += 1;
                *remainder = 0;
            }
        } else if filled > BAR_WIDTH {
            let first_largest = cells
                .iter()
                .enumerate()
                .rev()
                .max_by_key(|(_, cell)| **cell)
                .map(|(at, _)| at);
            if let Some(cell) = first_largest.and_then(|at| cells.get_mut(at)) {
                *cell -= 1;
            }
        }
    }
    cells
}

/// The badges of a case: its formalism, TRIAL, and each upstream report its
/// citations name.
fn badges(place: Option<&Place>) -> String {
    let Some(place) = place else {
        return String::new();
    };
    let mut badges = String::new();
    if let Some(formalism) = place.formalism {
        let _ = write!(badges, " [{}]", formalism.label());
    }
    if place.trial {
        badges.push_str(" [TRIAL]");
    }
    for report in &place.reports {
        let _ = write!(badges, " [report #{report}]");
    }
    badges
}

/// Renders the terminal view of `results`.
///
/// A heading naming the document, the engine and the base URL, a legend,
/// and a line for every row; then BASE by chapter and band, RECOMMENDED,
/// OPTIONS with the cases resting on TRIAL text, where the released spec is
/// defective with the cases each upstream report covers, and the cases the
/// outline places nowhere that did not pass. With
/// [`ShowOptions::failed`], every failed and errored row follows with its
/// four-part explanation, and the warning rows after them, apart.
#[must_use]
pub fn render(results: &Results, options: ShowOptions) -> String {
    let reading = read(results);
    let mut view = View {
        out: String::new(),
        color: options.color,
    };
    let title = view.paint(
        Paint::Bold,
        &format!("Veredictum results: {}", results.document),
    );
    view.line(&title);
    view.line(&format!(
        "{} {} ({}), driven against {}.",
        results.engine.name, results.engine.version, results.instrument_status, results.base_url
    ));
    view.line("");
    let legend: Vec<String> = ORDER
        .into_iter()
        .map(|mark| {
            let glyph = view.paint(Paint::of(mark), mark.glyph());
            format!("{glyph} {}", mark.word())
        })
        .collect();
    view.line(&format!(
        "{}. A warning row is a SHOULD the server did not follow and never counts against it.",
        legend.join("  ")
    ));
    view.line("");
    view.tally_line(0, "Every row", reading.total, "no rows");
    if results.outline.is_some() {
        placed(&mut view, &reading);
    }
    let unplaced: Vec<&CaseRows<'_>> = reading.unplaced.iter().collect();
    if !unplaced.is_empty() {
        view.heading("Unplaced");
        let tally = Tally::of(unplaced.iter().flat_map(|c| c.rows.iter().copied()));
        view.tally_line(2, "Every unplaced case", tally, "no rows");
        for case in unplaced
            .iter()
            .filter(|c| !matches!(c.mark(), Mark::Passed | Mark::Skipped))
        {
            view.case_line(4, case);
        }
    }
    if let Some((text, reports)) = divergence_note(&reading.spec_divergence()) {
        let numbers: Vec<String> = reports.iter().map(|r| format!("#{r}")).collect();
        view.line("");
        for line in wrap(&format!("{text} {}.", numbers.join(", ")), NOTE_WIDTH) {
            view.line(&view.paint(Paint::Grey, &line));
        }
    }
    if options.failed {
        expanded(&mut view, results);
    }
    view.out
}

/// The column the closing note wraps at, so it reads as a few lines in an
/// 80-column terminal and wider ones alike.
const NOTE_WIDTH: usize = 78;

/// `text` broken at spaces into lines of at most `width` characters; a word
/// longer than `width` stands on its own line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn placed(view: &mut View, reading: &Reading<'_>) {
    view.heading("BASE");
    view.tally_line(2, "Every BASE case", reading.base_total, "no cases");
    for chapter in &reading.base {
        view.tally_line(2, chapter.title, chapter.tally, "no cases");
        for band in &chapter.bands {
            view.tally_line(4, band.title, band.tally, "no cases");
        }
    }
    view.heading("OPTIONS");
    options(view, &reading.options);
}

/// Writes every option the way BASE reads: chapter, band, then each option
/// of the band with its selected arm and its tier, and the band's TRIAL text.
fn options(view: &mut View, tree: &OptionsTree<'_>) {
    if tree.is_empty() {
        view.line("  No option.");
        return;
    }
    view.tally_line(2, "Every option case", tree.tally, "no cases");
    for chapter in &tree.chapters {
        view.tally_line(2, chapter.title, chapter.tally, "no cases");
        for band in &chapter.bands {
            view.tally_line(4, band.title, band.tally, "no cases");
            for group in &band.options {
                option_line(view, 6, group);
            }
            if let Some(trial) = &band.trial {
                view.tally_line(6, "TRIAL text", trial.tally, "no cases");
                for case in &trial.cases {
                    view.case_line(8, case);
                }
            }
        }
    }
    if !tree.unanchored.is_empty() {
        view.line("  Options no document tests yet");
        for group in &tree.unanchored {
            option_line(view, 4, group);
        }
    }
}

/// Writes one option: its name with its counts, then which arm the
/// statement selected.
fn option_line(view: &mut View, indent: usize, group: &OptionGroup<'_>) {
    let label = group.option.to_owned();
    let kind = view.paint(Paint::Grey, group.tier.title());
    if !group.selected {
        view.tally_line(indent, &label, Tally::default(), "not selected");
        view.line(&format!("{}{kind}", " ".repeat(indent + 2)));
        return;
    }
    let nothing = group.arms.iter().any(|a| a.selected && a.empty);
    let empty = if nothing {
        arm_state(true, true)
    } else {
        "no cases"
    };
    view.tally_line(indent, &label, group.tally, empty);
    let choice = if nothing {
        String::new()
    } else {
        format!("{} · ", view.paint(Paint::Grey, &option_choice(group)))
    };
    view.line(&format!("{}{choice}{kind}", " ".repeat(indent + 2)));
}

/// Writes one expanded row: its mark, the first line of its four-part
/// explanation with the case's badges, then the rest of it.
fn expanded_row(view: &mut View, results: &Results, row: &ResultRow, lead: &str) {
    let mark = Mark::of(row);
    let glyph = view.paint(Paint::of(mark), mark.glyph());
    let badges = badges(results.outline.as_ref().and_then(|o| o.place(&row.case)));
    let text = row.to_string();
    let (first, rest) = text.split_once('\n').unwrap_or((text.as_str(), ""));
    let _ = writeln!(view.out, "\n{lead}{glyph} {first}{badges}\n{rest}");
}

fn expanded(view: &mut View, results: &Results) {
    view.heading("Failed and errored rows");
    let red: Vec<&ResultRow> = results
        .rows
        .iter()
        .filter(|row| matches!(Mark::of(row), Mark::Failed | Mark::Errored))
        .collect();
    if red.is_empty() {
        view.line("No row failed or errored.");
    }
    for row in red {
        expanded_row(view, results, row, "");
    }
    let warning: Vec<&ResultRow> = results.rows.iter().filter(|r| is_warning(r)).collect();
    if warning.is_empty() {
        return;
    }
    view.heading("Warning rows");
    for row in warning {
        expanded_row(view, results, row, &format!("{WARNING_LINE}\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(
        passed: usize,
        failed: usize,
        errored: usize,
        warning: usize,
        skipped: usize,
    ) -> Tally {
        Tally {
            tests: passed + failed + errored + warning + skipped,
            passed,
            failed,
            warning,
            errored,
            skipped,
        }
    }

    #[test]
    fn the_closing_note_wraps_at_word_boundaries() {
        let lines = wrap("one two three four five six", 9);
        assert_eq!(lines, ["one two", "three", "four five", "six"]);
        assert!(lines.iter().all(|l| l.chars().count() <= 9));
        assert_eq!(
            wrap("averyveryverylongword x", 5),
            ["averyveryverylongword", "x"]
        );
    }

    #[test]
    fn a_bar_fills_every_cell_and_never_hides_a_failure() {
        assert_eq!(bar_cells(tally(999, 1, 0, 0, 0)), [23, 1, 0, 0, 0]);
        assert_eq!(bar_cells(tally(1, 1, 1, 0, 0)), [8, 8, 8, 0, 0]);
        assert_eq!(bar_cells(tally(0, 0, 0, 0, 0)), [0; 5]);
        for t in [
            tally(7, 3, 2, 1, 11),
            tally(1, 1, 1, 1, 1),
            tally(100, 0, 0, 0, 1),
        ] {
            let cells = bar_cells(t);
            assert_eq!(cells.iter().sum::<usize>(), BAR_WIDTH, "{t:?}");
            for (mark, cells) in ORDER.into_iter().zip(cells) {
                assert_eq!(count(t, mark) > 0, cells > 0, "{t:?}");
            }
        }
    }

    #[test]
    fn colour_follows_the_terminal_and_no_color() {
        let set = Some(OsStr::new("1"));
        let empty = Some(OsStr::new(""));
        assert!(wants_color(ColorChoice::Auto, true, None));
        assert!(wants_color(ColorChoice::Auto, true, empty));
        assert!(!wants_color(ColorChoice::Auto, true, set));
        assert!(!wants_color(ColorChoice::Auto, false, None));
        assert!(wants_color(ColorChoice::Always, false, set));
        assert!(!wants_color(ColorChoice::Never, true, None));
    }
}
