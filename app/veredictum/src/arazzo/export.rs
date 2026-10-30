// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The views of an Arazzo [`Results`] document a vendor's CI shows: JUnit
//! XML, CTRF 0.2.0 JSON and a Markdown summary.
//!
//! Each view is a pure function of the results document, so it is as
//! byte-deterministic as the document itself: rows are read in document
//! order, workflows are grouped in the order they first appear, and nothing
//! is read from a clock. In every view the text of a failed or errored row is
//! its four-part explanation, the [`ResultRow`] display the evaluator prints.
//! No openEHR spec governs these formats — our own design.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use serde::Serialize;
use thiserror::Error;

use crate::arazzo::reading::Tally;
use crate::arazzo::results::{ResultRow, Results, SkipReason, Status, Whose, Why};
use crate::arazzo::rows::Row;
use crate::party::InstrumentStatus;

/// The CTRF specification version the CTRF view is written to
/// (<https://github.com/ctrf-io/ctrf/tree/v0.2.0>).
pub const CTRF_SPEC_VERSION: &str = "0.2.0";

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

/// The vocabulary token of a status.
const fn status_token(status: Status) -> &'static str {
    match status {
        Status::Passed => "passed",
        Status::Failed => "failed",
        Status::Errored => "errored",
        Status::Skipped => "skipped",
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

/// The CTRF status of a row.
// NOTE: CTRF 0.2.0 `schema/ctrf.schema.json` closes `status` to passed, failed, skipped, pending
// and other; an errored row is `other` and keeps its own status in `rawStatus`.
const fn ctrf_status(status: Status) -> &'static str {
    match status {
        Status::Passed => "passed",
        Status::Failed => "failed",
        Status::Errored => "other",
        Status::Skipped => "skipped",
    }
}

/// Writes the CTRF 0.2.0 view of `results`.
///
/// One test per row, named `<case> row <n>` and filed under the suite
/// `[<document>, <case>]`. *passed*, *failed* and *skipped* map to the CTRF
/// statuses of the same name and *errored* to `other`, with the row's own
/// status in `rawStatus`. A red row's `message` is its four-part explanation
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
            CtrfTest {
                test_id: format!("{}#{}", row.case, row.row),
                name: test_name(row),
                status: ctrf_status(row.status),
                duration: 0,
                suite: [results.document.as_str(), row.case.as_str()],
                message: (row.status != Status::Passed).then(|| row.to_string()),
                raw_status: status_token(row.status),
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
                other: tally.errored,
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

/// Writes the Markdown summary of `results`: the page a GitHub Actions job
/// summary shows.
///
/// A heading naming the document, the engine and the base URL; a table of
/// the row counts by status; every failed and errored row as a collapsed
/// section holding its four-part explanation; and a table of the skipped
/// rows with their reasons and citations.
#[must_use]
pub fn markdown(results: &Results) -> String {
    let tally = Tally::of(&results.rows);
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
        ("Errored", tally.errored),
        ("Skipped", tally.skipped),
        ("Total", tally.tests),
    ] {
        let _ = writeln!(out, "| {label} | {count} |");
    }
    out.push_str(
        "\nOnly a failed row counts against the server. An errored row is one the \
         instrument could not judge, and a skipped row was not run.\n",
    );
    let red: Vec<&ResultRow> = results
        .rows
        .iter()
        .filter(|row| matches!(row.status, Status::Failed | Status::Errored))
        .collect();
    if !red.is_empty() {
        out.push_str("\n## Failed and errored rows\n");
        for row in red {
            let text = row.to_string();
            let fence = fence_for(&text);
            let step = row
                .explanation
                .as_ref()
                .map(|why| format!(", step {}", html_text(&why.step)))
                .unwrap_or_default();
            let _ = write!(
                out,
                "\n<details>\n<summary>{} {}{step}</summary>\n\n{fence}text\n{text}\n{fence}\n\n</details>\n",
                row.status,
                html_text(&test_name(row)),
            );
        }
    }
    let skipped: Vec<&ResultRow> = results
        .rows
        .iter()
        .filter(|row| row.status == Status::Skipped)
        .collect();
    if !skipped.is_empty() {
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
    out
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
