// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The views of an Arazzo [`Results`] document a vendor's CI shows: JUnit
//! XML, CTRF 0.2.0 JSON and a Markdown summary.
//!
//! Each view is a pure function of the results document, so it is as
//! byte-deterministic as the document itself: rows are read in document
//! order, workflows are grouped in the order they first appear, and nothing
//! is read from a clock. In every view the text of a failed, advisory or
//! errored row is its four-part explanation, the [`ResultRow`] display the
//! evaluator prints. An advisory row records a SHOULD the server did not
//! follow, and no view counts it as failed: RFC 2119 §3, whose keywords the
//! ITS-REST overview follows, says there "may exist valid reasons in
//! particular circumstances to ignore a particular item". No openEHR spec
//! governs these formats — our own design.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use serde::Serialize;
use thiserror::Error;

use crate::arazzo::reading::{
    CaseRows, Mark, OptionGroup, Reading, ReportGroup, Tally, is_advisory, read,
};
use crate::arazzo::results::{ResultRow, Results, SkipReason, Status, Whose, Why};
use crate::arazzo::rows::Row;
use crate::arazzo::structure::Outline;
use crate::party::InstrumentStatus;

/// The CTRF specification version the CTRF view is written to
/// (<https://github.com/ctrf-io/ctrf/tree/v0.2.0>).
pub const CTRF_SPEC_VERSION: &str = "0.2.0";

/// The line that opens the text of an advisory row in the JUnit view.
pub const ADVISORY_LINE: &str =
    "ADVISORY: a SHOULD the server did not follow; this row never counts against the server.";

/// The issue tracker an upstream report's number is resolved against.
const REPORT_BASE: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/");

/// A failure to write one of the views.
#[derive(Debug, Error)]
pub enum ExportError {
    /// The XML writer failed, which an in-memory buffer never does.
    #[error("cannot write the JUnit XML: {0}")]
    Xml(#[from] std::io::Error),
    /// The XML writer produced bytes that are not UTF-8, which it never does
    /// for UTF-8 input.
    #[error("the JUnit XML is not UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    /// The CTRF document could not be serialized.
    #[error("cannot serialize the CTRF document: {0}")]
    Json(#[from] serde_json::Error),
}

/// The rows of `results` grouped by case (one group per workflow), in the
/// order each case first appears, each group's rows in document order.
#[must_use]
pub fn workflows(results: &Results) -> Vec<(&str, Vec<&ResultRow>)> {
    let mut index: BTreeMap<&str, usize> = BTreeMap::new();
    let mut groups: Vec<(&str, Vec<&ResultRow>)> = Vec::new();
    for row in &results.rows {
        let case = row.case.as_str();
        let at = *index.entry(case).or_insert_with(|| {
            groups.push((case, Vec::new()));
            groups.len() - 1
        });
        if let Some((_, rows)) = groups.get_mut(at) {
            rows.push(row);
        }
    }
    groups
}

/// The name a row carries in every view: its case id and its 0-based row.
fn test_name(row: &ResultRow) -> String {
    format!("{} row {}", row.case, row.row)
}

/// The vocabulary token of whose problem a red row is.
const fn whose_token(whose: Whose) -> &'static str {
    match whose {
        Whose::Server => "server",
        Whose::Instrument => "instrument",
        Whose::Advisory => "advisory",
    }
}

/// The one-line message a red or skipped row carries beside its full text.
fn summary_line(row: &ResultRow) -> Option<String> {
    if let Some(skip) = &row.skip {
        return Some(format!(
            "{}: {}, {}",
            skip.reason, skip.why.source, skip.why.section
        ));
    }
    row.explanation.as_ref().map(|why| {
        format!(
            "step {}: expected {}, received {}",
            why.step, why.expected, why.received
        )
    })
}

/// The upstream report link a citation carries, when it names one.
fn report_link(why: Option<&Why>) -> Option<String> {
    why.and_then(|why| why.report)
        .map(|report| format!("{REPORT_BASE}{report}"))
}

/// The citation of a row: the skip's, or the explanation's when it resolved.
fn citation(row: &ResultRow) -> Option<&Why> {
    match &row.skip {
        Some(skip) => Some(&skip.why),
        None => row.explanation.as_ref().and_then(|e| e.why.as_ref()),
    }
}

/// Replaces every character XML 1.0 cannot carry with U+FFFD.
// NOTE: XML 1.0 (Fifth Edition) §2.2, production [2] `Char`, excludes C0 controls other than
// TAB, LF and CR, and U+FFFE/U+FFFF; `results.json` stays the record that carries them.
fn xml_text(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\t' | '\n' | '\r' => c,
            '\u{0}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}' => '\u{fffd}',
            _ => c,
        })
        .collect()
}

/// Folds a value onto one line for an XML attribute, where a parser
/// normalizes every line break to a space anyway (XML 1.0 §3.3.3).
fn xml_attr(text: &str) -> String {
    xml_text(text).replace(['\r', '\n', '\t'], " ")
}

/// Writes the JUnit XML view of `results`.
///
/// The document is one `testsuites` named after the Arazzo document, one
/// `testsuite` per workflow and one `testcase` per row. A failed row carries
/// a `<failure>`, an errored row an `<error>` and a skipped row a
/// `<skipped>`; the element's `message` is a one-line summary, its `type`
/// says whose problem a red row is, and its text is the row's four-part
/// explanation (for a skipped row, its reason and citation). The view
/// carries no timing, because the results document carries none.
///
/// An advisory row is a passing `testcase` with no `<failure>`: its text sits
/// in `<system-out>`, opened by [`ADVISORY_LINE`], and no `failures` count
/// includes it, so a CI that fails a job on a JUnit failure never fails it on
/// a SHOULD.
///
/// # Errors
/// [`ExportError::Xml`] or [`ExportError::Utf8`], which an in-memory writer
/// never yields.
pub fn junit(results: &Results) -> Result<String, ExportError> {
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;
    let total = Tally::of(&results.rows);
    let mut suites = BytesStart::new("testsuites");
    suites.push_attribute(("name", xml_attr(&results.document).as_str()));
    push_tally(&mut suites, total);
    writer.write_event(Event::Start(suites))?;
    for (case, rows) in workflows(results) {
        let mut suite = BytesStart::new("testsuite");
        suite.push_attribute(("name", xml_attr(case).as_str()));
        push_tally(&mut suite, Tally::of(rows.iter().copied()));
        writer.write_event(Event::Start(suite))?;
        for row in rows {
            write_testcase(&mut writer, row)?;
        }
        writer.write_event(Event::End(BytesEnd::new("testsuite")))?;
    }
    writer.write_event(Event::End(BytesEnd::new("testsuites")))?;
    let mut text = String::from_utf8(writer.into_inner())?;
    text.push('\n');
    Ok(text)
}

/// Adds the JUnit count attributes of `tally` to `element`.
fn push_tally(element: &mut BytesStart<'_>, tally: Tally) {
    for (name, count) in [
        ("tests", tally.tests),
        ("failures", tally.failed),
        ("errors", tally.errored),
        ("skipped", tally.skipped),
    ] {
        element.push_attribute((name, count.to_string().as_str()));
    }
}

/// Writes one row as a `testcase`.
fn write_testcase(writer: &mut Writer<Vec<u8>>, row: &ResultRow) -> Result<(), ExportError> {
    let mut case = BytesStart::new("testcase");
    case.push_attribute(("name", xml_attr(&test_name(row)).as_str()));
    case.push_attribute(("classname", xml_attr(&row.case).as_str()));
    if is_advisory(row) {
        writer.write_event(Event::Start(case))?;
        writer.write_event(Event::Start(BytesStart::new("system-out")))?;
        let text = format!("{ADVISORY_LINE}\n{row}");
        writer.write_event(Event::Text(BytesText::new(&xml_text(&text))))?;
        writer.write_event(Event::End(BytesEnd::new("system-out")))?;
        writer.write_event(Event::End(BytesEnd::new("testcase")))?;
        return Ok(());
    }
    let child = match row.status {
        Status::Passed => None,
        Status::Failed => Some("failure"),
        Status::Errored => Some("error"),
        Status::Skipped => Some("skipped"),
    };
    let Some(child) = child else {
        writer.write_event(Event::Empty(case))?;
        return Ok(());
    };
    writer.write_event(Event::Start(case))?;
    let mut element = BytesStart::new(child);
    if let Some(message) = summary_line(row) {
        element.push_attribute(("message", xml_attr(&message).as_str()));
    }
    if let Some(why) = &row.explanation {
        element.push_attribute(("type", whose_token(why.whose)));
    }
    writer.write_event(Event::Start(element))?;
    writer.write_event(Event::Text(BytesText::new(&xml_text(&row.to_string()))))?;
    writer.write_event(Event::End(BytesEnd::new(child)))?;
    writer.write_event(Event::End(BytesEnd::new("testcase")))?;
    Ok(())
}

/// The CTRF document.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CtrfDocument<'a> {
    report_format: &'static str,
    spec_version: &'static str,
    generated_by: String,
    results: CtrfResults<'a>,
}

/// The CTRF `results` object.
#[derive(Debug, Serialize)]
struct CtrfResults<'a> {
    tool: CtrfTool<'a>,
    summary: CtrfSummary,
    tests: Vec<CtrfTest<'a>>,
    environment: CtrfEnvironment<'a>,
}

/// The CTRF `tool` object.
#[derive(Debug, Serialize)]
struct CtrfTool<'a> {
    name: &'a str,
    version: &'a str,
    extra: CtrfToolExtra,
}

/// The tool's own metadata.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CtrfToolExtra {
    instrument_status: InstrumentStatus,
}

/// The CTRF `summary` object.
#[derive(Debug, Serialize)]
struct CtrfSummary {
    tests: usize,
    passed: usize,
    failed: usize,
    skipped: usize,
    pending: usize,
    other: usize,
    suites: usize,
    start: u64,
    stop: u64,
}

/// One CTRF test: one row.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CtrfTest<'a> {
    test_id: String,
    name: String,
    status: &'static str,
    duration: u64,
    suite: [&'a str; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    raw_status: &'static str,
    parameters: &'a Row,
    extra: CtrfTestExtra<'a>,
}

/// The Veredictum fields of one test.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CtrfTestExtra<'a> {
    case: &'a str,
    row: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    skip_reason: Option<SkipReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    citation: Option<CtrfCitation<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    whose: Option<&'static str>,
}

/// The citation of one test.
#[derive(Debug, Serialize)]
struct CtrfCitation<'a> {
    source: &'a str,
    section: &'a str,
    quote: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    link: Option<&'a str>,
}

/// The CTRF `environment` object.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CtrfEnvironment<'a> {
    report_name: &'a str,
    extra: CtrfEnvironmentExtra<'a>,
}

/// The run's own environment fields.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CtrfEnvironmentExtra<'a> {
    base_url: &'a str,
}

/// The CTRF status and raw status of a row.
// NOTE: CTRF 0.2.0 `schema/ctrf.schema.json` closes `status` to passed, failed, skipped, pending
// and other; an errored or advisory row is `other` and keeps its own status in `rawStatus`.
fn ctrf_status(row: &ResultRow) -> (&'static str, &'static str) {
    match Mark::of(row) {
        Mark::Passed => ("passed", "passed"),
        Mark::Failed => ("failed", "failed"),
        Mark::Advisory => ("other", "advisory"),
        Mark::Errored => ("other", "errored"),
        Mark::Skipped => ("skipped", "skipped"),
    }
}

/// Writes the CTRF 0.2.0 view of `results`.
///
/// One test per row, named `<case> row <n>` and filed under the suite
/// `[<document>, <case>]`. *passed*, *failed* and *skipped* map to the CTRF
/// statuses of the same name, and *errored* and *advisory* to `other`, with
/// the row's own status in `rawStatus`. An advisory row is therefore never
/// `failed`, and `summary.failed` excludes it, so a CI that fails a job on a
/// CTRF failure never fails it on a SHOULD; `summary.other` counts it. A red row's `message` is its four-part explanation
/// and a skipped row's is its reason and citation; `extra` carries the case
/// id, the row, the skip reason, the citation, the upstream report link and
/// whose problem a red row is. The row's inputs are the test's `parameters`.
/// The results document carries no timing, so every duration and the run's
/// `start` and `stop` are 0.
///
/// # Errors
/// [`ExportError::Json`] when the document cannot be serialized, which a
/// results document that deserialized never yields.
pub fn ctrf(results: &Results) -> Result<String, ExportError> {
    let tally = Tally::of(&results.rows);
    let tests = results
        .rows
        .iter()
        .map(|row| {
            let why = citation(row);
            let (status, raw_status) = ctrf_status(row);
            CtrfTest {
                test_id: format!("{}#{}", row.case, row.row),
                name: test_name(row),
                status,
                duration: 0,
                suite: [results.document.as_str(), row.case.as_str()],
                message: (row.status != Status::Passed).then(|| row.to_string()),
                raw_status,
                parameters: &row.inputs,
                extra: CtrfTestExtra {
                    case: &row.case,
                    row: row.row,
                    skip_reason: row.skip.as_ref().map(|skip| skip.reason),
                    citation: why.map(|why| CtrfCitation {
                        source: &why.source,
                        section: &why.section,
                        quote: &why.quote,
                        link: why.link.as_deref(),
                    }),
                    report: report_link(why),
                    whose: row.explanation.as_ref().map(|e| whose_token(e.whose)),
                },
            }
        })
        .collect();
    // NOTE: no openEHR spec governs this — our own design; the results document carries no
    // clock reading, so the CTRF-required `start` and `stop` are 0 rather than an invented time.
    let document = CtrfDocument {
        report_format: "CTRF",
        spec_version: CTRF_SPEC_VERSION,
        generated_by: format!("{} {}", results.engine.name, results.engine.version),
        results: CtrfResults {
            tool: CtrfTool {
                name: &results.engine.name,
                version: &results.engine.version,
                extra: CtrfToolExtra {
                    instrument_status: results.instrument_status,
                },
            },
            summary: CtrfSummary {
                tests: tally.tests,
                passed: tally.passed,
                failed: tally.failed,
                skipped: tally.skipped,
                pending: 0,
                other: tally.errored + tally.advisory,
                suites: workflows(results).len(),
                start: 0,
                stop: 0,
            },
            tests,
            environment: CtrfEnvironment {
                report_name: &results.document,
                extra: CtrfEnvironmentExtra {
                    base_url: &results.base_url,
                },
            },
        },
    };
    let mut text = serde_json::to_string_pretty(&document)?;
    text.push('\n');
    Ok(text)
}

/// Escapes text for a Markdown table cell or an HTML inline context.
fn md_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' | '\n' => out.push(' '),
            '\\' | '`' | '*' | '_' | '[' | ']' | '|' | '#' | '~' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Escapes text for raw HTML, where a Markdown backslash escape would show.
fn html_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace(['\r', '\n'], " ")
}

/// A code fence longer than any backtick run inside `text`.
fn fence_for(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

/// The counts of `tally` as the marks every view prints.
#[must_use]
pub fn tally_marks(tally: Tally) -> String {
    format!(
        "✓ {}  ✕ {}  ? {}  ! {} advisory  ○ {} skipped",
        tally.passed, tally.failed, tally.errored, tally.advisory, tally.skipped
    )
}

/// The non-zero counts of `tally` in words, `1 passed, 2 skipped`.
#[must_use]
pub fn tally_words(tally: Tally) -> String {
    let words: Vec<String> = [
        (tally.passed, Mark::Passed),
        (tally.failed, Mark::Failed),
        (tally.advisory, Mark::Advisory),
        (tally.errored, Mark::Errored),
        (tally.skipped, Mark::Skipped),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, mark)| format!("{count} {}", mark.word()))
    .collect();
    if words.is_empty() {
        String::from("no rows")
    } else {
        words.join(", ")
    }
}

/// The badges a case line carries: its formalism, TRIAL, and each upstream
/// report its citations name.
fn md_badges(case: &CaseRows<'_>) -> String {
    let mut badges = String::new();
    if let Some(formalism) = case.formalism() {
        let _ = write!(badges, " `{}`", formalism.label());
    }
    if case.trial() {
        badges.push_str(" `TRIAL`");
    }
    for report in case.reports() {
        let _ = write!(badges, " `report #{report}`");
    }
    badges
}

/// The citation of a red row as `source, section`.
#[must_use]
pub fn cited(row: &ResultRow) -> String {
    citation(row).map_or_else(
        || String::from("no citation resolved"),
        |why| format!("{}, {}", why.source, why.section),
    )
}

/// Writes one row as a collapsed section holding its four-part explanation.
fn md_details(out: &mut String, row: &ResultRow) {
    let mark = Mark::of(row);
    let text = if mark == Mark::Advisory {
        format!("{ADVISORY_LINE}\n{row}")
    } else {
        row.to_string()
    };
    let fence = fence_for(&text);
    let step = row
        .explanation
        .as_ref()
        .map(|why| format!(", step {}", html_text(&why.step)))
        .unwrap_or_default();
    let should = if mark == Mark::Advisory {
        ": a SHOULD the server did not follow"
    } else {
        ""
    };
    let _ = write!(
        out,
        "\n<details>\n<summary>{} {} {}{step}{should}</summary>\n\n{fence}text\n{text}\n{fence}\n\n</details>\n",
        mark.glyph(),
        mark.word(),
        html_text(&test_name(row)),
    );
}

/// Whether a row is shown expanded: failed, advisory or errored.
fn is_expanded(row: &ResultRow) -> bool {
    matches!(Mark::of(row), Mark::Failed | Mark::Advisory | Mark::Errored)
}

/// Writes one case: a line with its mark, counts and badges, then every
/// failed, advisory and errored row expanded.
fn md_case(out: &mut String, case: &CaseRows<'_>) {
    let _ = writeln!(
        out,
        "\n- {} {}: {}{}",
        case.mark().glyph(),
        md_inline(case.case),
        tally_words(case.tally()),
        md_badges(case),
    );
    for row in case.rows.iter().filter(|row| is_expanded(row)) {
        md_details(out, row);
    }
}

/// Writes the BASE part, chapter by chapter.
fn md_base(out: &mut String, reading: &Reading<'_>) {
    let _ = writeln!(
        out,
        "\n## BASE\n\nEvery case no option decides, outside the TRIAL text: {}.",
        tally_marks(reading.base_total)
    );
    for chapter in &reading.base {
        let _ = writeln!(
            out,
            "\n### {}  {}",
            md_inline(chapter.title),
            tally_marks(chapter.tally)
        );
        for band in &chapter.bands {
            if band.tally.tests == 0 {
                let _ = writeln!(out, "\n#### {}: no cases", md_inline(band.title));
                continue;
            }
            let _ = writeln!(
                out,
                "\n#### {}  {}",
                md_inline(band.title),
                tally_marks(band.tally)
            );
            for section in &band.sections {
                let formalism = section
                    .formalism
                    .map(|f| format!(" ({})", f.label()))
                    .unwrap_or_default();
                let _ = writeln!(out, "\n##### {}{formalism}", md_inline(section.title));
                for case in &section.cases {
                    md_case(out, case);
                }
            }
        }
    }
}

/// The path an option is read under: its chapter, its band and its name.
#[must_use]
pub fn option_path(outline: &Outline, group: &OptionGroup<'_>) -> String {
    let mut parts = Vec::new();
    if let Some((chapter, folder)) = group.folder.and_then(|f| outline.folder(f)) {
        parts.push(chapter.title.as_str());
        parts.push(folder.title.as_str());
    }
    parts.push(group.option);
    parts.join(" / ")
}

/// What an option's heading says about its arms: the selected one, or that
/// none is.
#[must_use]
pub fn option_choice(group: &OptionGroup<'_>) -> String {
    let selected: Vec<&str> = group
        .arms
        .iter()
        .filter(|arm| arm.selected)
        .map(|arm| arm.arm)
        .collect();
    if selected.is_empty() {
        String::from("not selected")
    } else {
        format!("arm {} (selected)", selected.join(", "))
    }
}

/// What a view says about one arm of a selected option.
#[must_use]
pub fn arm_state(selected: bool, empty: bool) -> &'static str {
    match (selected, empty) {
        (true, true) => "selected, nothing to run",
        (true, false) => "selected",
        (false, _) => "not selected",
    }
}

/// The heading of a report: its number, linked, its title and state.
fn report_heading(report: u64, title: Option<&str>, state: Option<String>) -> String {
    match (title, state) {
        (Some(title), Some(state)) => format!(
            "[#{report}]({REPORT_BASE}{report}): {} ({state})",
            md_inline(title)
        ),
        _ => format!("[#{report}]({REPORT_BASE}{report}) (not in the report snapshot)"),
    }
}

/// Writes one option: its heading, what it decides, each arm, and the cases
/// of its selected arms.
fn md_option(out: &mut String, outline: &Outline, group: &OptionGroup<'_>) {
    let _ = writeln!(
        out,
        "\n### {}: {}\n\n{}\n\nTheme: {}.",
        md_inline(&option_path(outline, group)),
        md_inline(&option_choice(group)),
        md_inline(group.description),
        group.theme.title(),
    );
    if let Some(report) = group.report {
        let entry = outline.report(report);
        let _ = writeln!(
            out,
            "\nUpstream report {}.",
            report_heading(
                report.get(),
                entry.map(|e| e.title.as_str()),
                entry.map(|e| e.state.to_string())
            )
        );
    }
    if !group.selected {
        out.push_str(
            "\nThe statement selects no arm, so its rows are skipped as option-not-selected \
             and counted in no tally above.\n",
        );
        return;
    }
    out.push('\n');
    for arm in &group.arms {
        let counts = if arm.selected && !arm.empty {
            format!(", {}", tally_marks(arm.tally))
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "- arm {}: {}{counts}",
            md_inline(arm.arm),
            arm_state(arm.selected, arm.empty)
        );
    }
    for arm in group.arms.iter().filter(|arm| arm.selected) {
        for case in &arm.cases {
            md_case(out, case);
        }
    }
}

/// Writes one tier of options, saying so when it holds none.
fn md_tier(out: &mut String, outline: &Outline, groups: &[OptionGroup<'_>]) {
    if groups.is_empty() {
        out.push_str("\nNo option in this tier.\n");
    }
    for group in groups {
        md_option(out, outline, group);
    }
}

/// Writes the cases resting on text one upstream report records.
fn md_report(out: &mut String, group: &ReportGroup<'_>) {
    let _ = writeln!(
        out,
        "\n### Report {}\n\n{}\n",
        report_heading(
            group.report.get(),
            group.title,
            group.state.map(|s| s.to_string())
        ),
        tally_marks(group.tally),
    );
    for case in &group.cases {
        let _ = writeln!(
            out,
            "- {} {}: {}{}",
            case.mark().glyph(),
            md_inline(case.case),
            tally_words(case.tally()),
            md_badges(case),
        );
        for row in case.rows.iter().filter(|row| is_expanded(row)) {
            let mark = Mark::of(row);
            let _ = writeln!(
                out,
                "  - {} {} {}: {}",
                mark.glyph(),
                mark.word(),
                md_inline(&test_name(row)),
                md_inline(&cited(row)),
            );
        }
    }
}

/// Writes the placed parts of the reading: BASE, RECOMMENDED, OPTIONS with
/// the TRIAL text, and where the released spec is defective.
fn md_placed(out: &mut String, outline: &Outline, reading: &Reading<'_>) {
    md_base(out, reading);
    out.push_str(
        "\n## RECOMMENDED\n\nThe release asks for these with a SHOULD; a server that \
         declines one records advisory rows, never failed ones.\n",
    );
    md_tier(out, outline, &reading.recommended);
    out.push_str(
        "\n## OPTIONS\n\nA MAY, or behaviour the release leaves to the implementation; \
         the statement selects one arm of each.\n",
    );
    md_tier(out, outline, &reading.options);
    let _ = writeln!(
        out,
        "\n### TRIAL text\n\nThe release marks the text these cases cite TRIAL, so they \
         are read with the options and do not count in the BASE tally: {}.",
        tally_marks(reading.trial.tally)
    );
    if reading.trial.cases.is_empty() {
        out.push_str("\nNo case rests on TRIAL text.\n");
    }
    for case in &reading.trial.cases {
        md_case(out, case);
    }
    out.push_str(
        "\n## Where the released spec is defective\n\nThe options below exist only because \
         the released text is defective, and each report lists the cases resting on text \
         it records as defective.\n",
    );
    md_tier(out, outline, &reading.boundary);
    if reading.reports.is_empty() {
        out.push_str("\nNo case rests on reported text.\n");
    }
    for group in &reading.reports {
        md_report(out, group);
    }
}

/// Writes the Markdown summary of `results`: the page a GitHub Actions job
/// summary shows.
///
/// A heading naming the document, the engine and the base URL, then a table
/// of the row counts by mark. When the results carry an outline, the rows
/// follow under BASE (chapter, band and section, in the catalogue's declared
/// order), RECOMMENDED, OPTIONS with the cases resting on TRIAL text, and
/// where the released spec is defective, grouped by upstream report. Inside a
/// group, a failed, advisory or errored row is a collapsed section holding
/// its four-part explanation, and passed and skipped rows are counted per
/// case. A case the outline does not place, or every case when the results
/// carry no outline, is listed under Unplaced. The advisory rows follow in
/// their own table, each as a SHOULD the server did not follow with its
/// citation, and the skipped rows in one with their reasons. An advisory row
/// is never counted as failed.
#[must_use]
pub fn markdown(results: &Results) -> String {
    let reading = read(results);
    let tally = reading.total;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Veredictum results: {}\n",
        md_inline(&results.document)
    );
    let _ = writeln!(
        out,
        "{} {} ({}), driven against {}.\n",
        md_inline(&results.engine.name),
        md_inline(&results.engine.version),
        md_inline(&results.instrument_status.to_string()),
        md_inline(&results.base_url),
    );
    out.push_str("| Status | Rows |\n| --- | ---: |\n");
    for (label, count) in [
        ("Passed", tally.passed),
        ("Failed", tally.failed),
        ("Advisory", tally.advisory),
        ("Errored", tally.errored),
        ("Skipped", tally.skipped),
        ("Total", tally.tests),
    ] {
        let _ = writeln!(out, "| {label} | {count} |");
    }
    out.push_str(
        "\nOnly a failed row counts against the server. An advisory row records a SHOULD \
         the server did not follow and never counts against the server. An errored row is \
         one the instrument could not judge, and a skipped row was not run.\n",
    );
    if let Some(outline) = &results.outline {
        md_placed(&mut out, outline, &reading);
    }
    if !reading.unplaced.is_empty() {
        out.push_str("\n## Unplaced\n");
        out.push_str(if results.outline.is_some() {
            "\nThe outline the results carry places these cases nowhere.\n"
        } else {
            "\nThe results carry no outline, so no case is placed in BASE or OPTIONS.\n"
        });
        for case in &reading.unplaced {
            md_case(&mut out, case);
        }
    }
    md_advisory(&mut out, results);
    md_skipped(&mut out, results);
    out
}

/// Writes the table of advisory rows, when there is one.
fn md_advisory(out: &mut String, results: &Results) {
    let advisory: Vec<&ResultRow> = results.rows.iter().filter(|r| is_advisory(r)).collect();
    if advisory.is_empty() {
        return;
    }
    out.push_str(
        "\n## Advisory rows\n\nEach is a SHOULD the server did not follow. None counts \
         against the server.\n\n| Row | Step | Citation |\n| --- | --- | --- |\n",
    );
    for row in advisory {
        let step = row
            .explanation
            .as_ref()
            .map(|why| why.step.as_str())
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            md_inline(&test_name(row)),
            md_inline(step),
            md_inline(&cited(row)),
        );
    }
}

/// Writes the table of skipped rows, when there is one.
fn md_skipped(out: &mut String, results: &Results) {
    let skipped: Vec<&ResultRow> = results
        .rows
        .iter()
        .filter(|row| row.status == Status::Skipped)
        .collect();
    if skipped.is_empty() {
        return;
    }
    out.push_str("\n## Skipped rows\n\n| Row | Reason | Citation |\n| --- | --- | --- |\n");
    for row in skipped {
        let (reason, cited) = row.skip.as_ref().map_or_else(
            || (String::from("(none recorded)"), String::new()),
            |skip| {
                (
                    skip.reason.to_string(),
                    format!("{}, {}", skip.why.source, skip.why.section),
                )
            },
        );
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            md_inline(&test_name(row)),
            md_inline(&reason),
            md_inline(&cited),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_xml_cannot_carry_is_replaced() {
        assert_eq!(xml_text("a\u{1}b\tc\u{ffff}"), "a\u{fffd}b\tc\u{fffd}");
        assert_eq!(xml_attr("a\nb"), "a b");
    }

    #[test]
    fn a_fence_outruns_every_backtick_run() {
        assert_eq!(fence_for("plain"), "```");
        assert_eq!(fence_for("a ```` b"), "`````");
    }

    #[test]
    fn a_cell_cannot_break_its_table() {
        assert_eq!(md_inline("a|b\n<c>"), "a\\|b &lt;c&gt;");
    }
}
