// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The views of an Arazzo [`Results`] document a vendor's CI shows: JUnit
//! XML, CTRF 0.2.0 JSON and a Markdown summary.
//!
//! Each view is a pure function of the results document, so it is as
//! byte-deterministic as the document itself: rows are read in document
//! order, workflows are grouped in the order they first appear, and nothing
//! is read from a clock. In every view the text of a failed, warning or
//! errored row is its four-part explanation, the [`ResultRow`] display the
//! evaluator prints. A warning row records a SHOULD the server did not
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
    CaseRows, Mark, OptionGroup, OptionsTree, Reading, SpecDivergence, Tally, is_warning, read,
};
use crate::arazzo::results::{ResultRow, Results, SkipReason, Status, Whose, Why};
use crate::arazzo::rows::Row;
use crate::arazzo::spec_profile::Specification;
use crate::arazzo::structure::{Outline, Place, SPEC_DIVERGE_TAG, Standing, Tier};
use crate::party::InstrumentStatus;

/// The CTRF specification version the CTRF view is written to
/// (<https://github.com/ctrf-io/ctrf/tree/v0.2.0>).
pub const CTRF_SPEC_VERSION: &str = "0.2.0";

/// The line that opens the text of a warning row in the JUnit view.
pub const WARNING_LINE: &str =
    "WARNING: a SHOULD the server did not follow; this row never counts against the server.";

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
        Whose::Warning => "warning",
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

/// The part of a result a case is read under, the BASE and option tiers.
fn part_of(place: &Place) -> &'static str {
    match &place.standing {
        Standing::Base { .. } if !place.trial => "BASE",
        Standing::Base { .. } | Standing::Option { .. } => "OPTIONS",
    }
}

/// Where a case sits, as named values every machine-readable view carries.
///
/// The values are the part, chapter, band and capability, the section of a BASE case or the
/// options and arms of an OPTION case, its formalism, a TRIAL mark and the
/// upstream reports its citations name. Empty for a case the outline does
/// not place.
#[must_use]
pub fn placement(outline: Option<&Outline>, case: &str) -> Vec<(&'static str, Vec<String>)> {
    let Some((outline, place)) = outline.and_then(|o| o.place(case).map(|p| (o, p))) else {
        return Vec::new();
    };
    let mut named: Vec<(&'static str, Vec<String>)> =
        vec![("part", vec![part_of(place).to_owned()])];
    if let Some((chapter, folder)) = outline.folder(&place.folder) {
        named.push(("chapter", vec![chapter.title.clone()]));
        named.push(("band", vec![folder.title.clone()]));
    }
    named.push(("capability", vec![place.folder.clone()]));
    match &place.standing {
        Standing::Base { section } => named.push((
            "section",
            vec![
                outline
                    .section_title(&place.folder, section)
                    .unwrap_or(section)
                    .to_owned(),
            ],
        )),
        Standing::Option { arms } => {
            named.push(("option", arms.iter().map(|a| a.option.clone()).collect()));
            named.push((
                "kind",
                arms.iter()
                    .map(|a| {
                        outline
                            .option(&a.option)
                            .map_or("Optional (MAY)", |o| o.tier.title())
                            .to_owned()
                    })
                    .collect(),
            ));
            named.push(("arm", arms.iter().map(|a| a.arm.clone()).collect()));
        }
    }
    if let Some(formalism) = place.formalism {
        named.push(("formalism", vec![formalism.label().to_owned()]));
    }
    if place.trial {
        named.push(("trial", vec![String::from("true")]));
    }
    if !place.reports.is_empty() {
        named.push(("tag", vec![SPEC_DIVERGE_TAG.to_owned()]));
    }
    if !place.reports.is_empty() {
        named.push((
            "report",
            place.reports.iter().map(|r| format!("#{r}")).collect(),
        ));
    }
    named
}

/// The namespace every name Veredictum adds to a JUnit `<property>` or a
/// CTRF `extra` object carries.
pub const EXTENSION_NAMESPACE: &str = "eu.veredictum";

/// Writes the JUnit XML view of `results`.
///
/// No single body owns JUnit XML, so the view is held to the two schemas the
/// tools that read it publish: each `testsuite` to Apache Maven Surefire's
/// test report schema, and the document to the Jenkins xUnit plugin's
/// junit-10 schema, which adds the `testsuites` root.
///
/// The document is one `testsuites` named after the Arazzo document, one
/// `testsuite` per workflow and one `testcase` per row. A suite of a placed
/// case carries `<properties>` naming where it sits, each property named
/// `eu.veredictum.<name>`. A failed row carries a `<failure>`, an errored row
/// an `<error>` and a skipped row a `<skipped>`; the element's `message` is a
/// one-line summary, its `type` says whose problem a red row is, and its text
/// is the row's four-part explanation (for a skipped row, its reason and
/// citation). Every `time` is 0, because the results document carries no
/// timing.
///
/// A warning row is a passing `testcase` with no `<failure>`: its text sits
/// in `<system-out>`, opened by [`WARNING_LINE`], and no `failures` count
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
    // NOTE: the Jenkins xUnit junit-10 schema gives `testsuites` no `skipped` attribute, so the
    // root counts tests, failures and errors only; each `testsuite` carries its skips.
    for (name, count) in [
        ("tests", total.tests),
        ("failures", total.failed),
        ("errors", total.errored),
    ] {
        suites.push_attribute((name, count.to_string().as_str()));
    }
    suites.push_attribute(("time", NO_TIME));
    writer.write_event(Event::Start(suites))?;
    for (case, rows) in workflows(results) {
        let mut suite = BytesStart::new("testsuite");
        suite.push_attribute(("name", xml_attr(case).as_str()));
        push_tally(&mut suite, Tally::of(rows.iter().copied()));
        suite.push_attribute(("time", NO_TIME));
        writer.write_event(Event::Start(suite))?;
        let mut named = placement(results.outline.as_ref(), case);
        if !results.specification.is_released() {
            named.insert(0, ("profile", vec![results.specification.label()]));
        }
        if !named.is_empty() {
            writer.write_event(Event::Start(BytesStart::new("properties")))?;
            for (name, values) in named {
                let mut property = BytesStart::new("property");
                property.push_attribute(("name", format!("{EXTENSION_NAMESPACE}.{name}").as_str()));
                property.push_attribute(("value", xml_attr(&values.join(", ")).as_str()));
                writer.write_event(Event::Empty(property))?;
            }
            writer.write_event(Event::End(BytesEnd::new("properties")))?;
        }
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

/// The `time` every JUnit element carries.
// NOTE: the Surefire schema makes `testcase/@time` required, and the results document carries no
// clock reading, so every time is 0 rather than an invented duration (no openEHR spec governs it).
const NO_TIME: &str = "0";

/// Writes one row as a `testcase`.
fn write_testcase(writer: &mut Writer<Vec<u8>>, row: &ResultRow) -> Result<(), ExportError> {
    let mut case = BytesStart::new("testcase");
    case.push_attribute(("name", xml_attr(&test_name(row)).as_str()));
    case.push_attribute(("classname", xml_attr(&row.case).as_str()));
    case.push_attribute(("time", NO_TIME));
    if is_warning(row) {
        writer.write_event(Event::Start(case))?;
        writer.write_event(Event::Start(BytesStart::new("system-out")))?;
        let text = format!("{WARNING_LINE}\n{row}");
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

/// A CTRF `extra` object holding Veredictum's one extension, under its
/// namespaced key.
// NOTE: CTRF 0.2.0 `spec/ctrf.md` §4.6.1: extension keys "SHOULD be namespaced" and a producer
// "SHOULD group related extension data under a single extension key".
#[derive(Debug, Serialize)]
struct Extra<T> {
    #[serde(rename = "eu.veredictum")]
    veredictum: T,
}

/// The CTRF `tool` object.
#[derive(Debug, Serialize)]
struct CtrfTool<'a> {
    name: &'a str,
    version: &'a str,
    extra: Extra<CtrfToolExtra>,
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
    suite: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    raw_status: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    labels: BTreeMap<&'static str, CtrfLabel>,
    parameters: &'a Row,
    extra: Extra<CtrfTestExtra<'a>>,
}

/// One CTRF label value: a string, or a list of strings.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum CtrfLabel {
    /// One value.
    One(String),
    /// Several values.
    Many(Vec<String>),
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
    extra: Extra<CtrfEnvironmentExtra<'a>>,
}

/// The run's own environment fields.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CtrfEnvironmentExtra<'a> {
    base_url: &'a str,
    /// The development profile and its snapshot; absent for the released
    /// profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    specification: Option<&'a Specification>,
}

/// The CTRF status and raw status of a row.
// NOTE: CTRF 0.2.0 `schema/ctrf.schema.json` closes `status` to passed, failed, skipped, pending
// and other; an errored or warning row is `other` and keeps its own status in `rawStatus`.
fn ctrf_status(row: &ResultRow) -> (&'static str, &'static str) {
    match Mark::of(row) {
        Mark::Passed => ("passed", "passed"),
        Mark::Failed => ("failed", "failed"),
        Mark::Warning => ("other", "warning"),
        Mark::Errored => ("other", "errored"),
        Mark::Skipped => ("skipped", "skipped"),
    }
}

/// The CTRF `suite` path of a case: the document, then the part, chapter,
/// band and section or option it is read under, or the case id when the
/// outline does not place it.
fn ctrf_suite(results: &Results, named: &[(&'static str, Vec<String>)], case: &str) -> Vec<String> {
    let value = |name: &str| {
        named
            .iter()
            .find(|(n, _)| *n == name)
            .and_then(|(_, values)| values.first().cloned())
    };
    let path: Vec<String> = ["part", "chapter", "band"]
        .into_iter()
        .filter_map(value)
        .chain(value("section").or_else(|| value("option")))
        .collect();
    let mut suite = vec![results.document.clone()];
    if path.is_empty() {
        suite.push(case.to_owned());
    } else {
        suite.extend(path);
    }
    suite
}

/// Writes the CTRF 0.2.0 view of `results`.
///
/// One test per row, named `<case> row <n>`, of `type` `conformance`, and
/// filed under the `suite` path of the document, then the part, chapter, band
/// and section or option the outline reads it under. Its `labels` name where
/// it sits, as [`placement`] gives it, and every `extra` object holds one
/// extension under the key [`EXTENSION_NAMESPACE`], as CTRF §4.6.1 asks. *passed*, *failed* and *skipped* map to the CTRF
/// statuses of the same name, and *errored* and *warning* to `other`, with
/// the row's own status in `rawStatus`. A warning row is therefore never
/// `failed`, and `summary.failed` excludes it, so a CI that fails a job on a
/// CTRF failure never fails it on a SHOULD; `summary.other` counts it. A red row's `message` is its four-part explanation
/// and a skipped row's is its reason and citation; the extension carries the case
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
            let named = placement(results.outline.as_ref(), &row.case);
            let suite = ctrf_suite(results, &named, &row.case);
            let tags: Vec<String> = named
                .iter()
                .filter(|(name, _)| *name == "tag")
                .flat_map(|(_, values)| values.iter().cloned())
                .collect();
            let labels = named
                .into_iter()
                .filter(|(name, _)| *name != "tag")
                .map(|(name, mut values)| {
                    let value = if values.len() == 1 {
                        CtrfLabel::One(values.remove(0))
                    } else {
                        CtrfLabel::Many(values)
                    };
                    (name, value)
                })
                .collect();
            CtrfTest {
                test_id: format!("{}#{}", row.case, row.row),
                name: test_name(row),
                status,
                duration: 0,
                suite,
                message: (row.status != Status::Passed).then(|| row.to_string()),
                raw_status,
                kind: "conformance",
                tags,
                labels,
                parameters: &row.inputs,
                extra: Extra {
                    veredictum: CtrfTestExtra {
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
                extra: Extra {
                    veredictum: CtrfToolExtra {
                        instrument_status: results.instrument_status,
                    },
                },
            },
            summary: CtrfSummary {
                tests: tally.tests,
                passed: tally.passed,
                failed: tally.failed,
                skipped: tally.skipped,
                pending: 0,
                other: tally.errored + tally.warning,
                suites: workflows(results).len(),
                start: 0,
                stop: 0,
            },
            tests,
            environment: CtrfEnvironment {
                report_name: &results.document,
                extra: Extra {
                    veredictum: CtrfEnvironmentExtra {
                        base_url: &results.base_url,
                        specification: (!results.specification.is_released())
                            .then_some(&results.specification),
                    },
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

/// A Markdown code span holding `text` verbatim inside a table cell: GFM
/// processes no backslash escape inside a code span apart from `\|`, so only
/// the pipe and line breaks are rewritten.
fn md_code(text: &str) -> String {
    let text = text.replace(['\r', '\n'], " ").replace('|', "\\|");
    let longest = text
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let fence = "`".repeat(longest + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// The counts of `tally` as the marks the terminal view prints.
#[must_use]
pub fn tally_marks(tally: Tally) -> String {
    format!(
        "✓ {}  ✕ {}  ? {}  ! {} warning  ○ {} skipped",
        tally.passed, tally.failed, tally.errored, tally.warning, tally.skipped
    )
}

/// The counts of `tally` in words, leaving out the zero ones.
#[must_use]
pub fn tally_words(tally: Tally) -> String {
    let words: Vec<String> = [
        (tally.passed, Mark::Passed),
        (tally.failed, Mark::Failed),
        (tally.warning, Mark::Warning),
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

/// What an option's line says about its arms: the selected one, or that
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

/// What a view says about one arm of an option.
#[must_use]
pub fn arm_state(selected: bool, empty: bool) -> &'static str {
    match (selected, empty) {
        (true, true) => "selected, nothing to run",
        (true, false) => "selected",
        (false, _) => "not selected",
    }
}

/// The column heads of every count table, after its first column(s).
const COUNT_HEADS: &str = "Passed | Failed | Warning | Errored | Skipped";

/// The alignment row cells of the count columns.
const COUNT_ALIGN: &str = "---: | ---: | ---: | ---: | ---:";

/// The count cells of one table row.
fn count_cells(tally: Tally) -> String {
    format!(
        "{} | {} | {} | {} | {}",
        tally.passed, tally.failed, tally.warning, tally.errored, tally.skipped
    )
}

/// The word a row's outcome reads as, capitalized for a heading.
const fn outcome_word(mark: Mark) -> &'static str {
    match mark {
        Mark::Passed => "Passed",
        Mark::Failed => "Failed",
        Mark::Warning => "Warning",
        Mark::Errored => "Errored",
        Mark::Skipped => "Skipped",
    }
}

/// Whether a row is listed with its explanation: failed, warning or
/// errored.
fn needs_attention(row: &ResultRow) -> bool {
    matches!(Mark::of(row), Mark::Failed | Mark::Warning | Mark::Errored)
}

/// The labels a case carries beside its id: its formalism, TRIAL, and the
/// upstream reports its text rests on.
fn case_labels(case: &CaseRows<'_>) -> Vec<String> {
    let mut labels = Vec::new();
    if let Some(formalism) = case.formalism() {
        labels.push(formalism.label().to_owned());
    }
    if case.trial() {
        labels.push(String::from("TRIAL text"));
    }
    if !case.reports().is_empty() {
        labels.push(SPEC_DIVERGE_TAG.to_owned());
    }
    labels
}

/// The link to an upstream report, with its title and state when the
/// snapshot records it.
fn report_ref(report: u64, title: Option<&str>, state: Option<String>) -> String {
    match (title, state) {
        (Some(title), Some(state)) => format!(
            "[#{report}]({REPORT_BASE}{report}): {} ({state})",
            md_inline(title)
        ),
        _ => format!("[#{report}]({REPORT_BASE}{report})"),
    }
}

/// The citation a row rests on, as source and section.
fn cited(row: &ResultRow) -> String {
    citation(row).map_or_else(
        || String::from("(no citation resolved)"),
        |why| format!("{}, {}", why.source, why.section),
    )
}

/// The labels a row's summary carries after where it sits: its formalism,
/// TRIAL, and the upstream reports its case rests on, each led by a comma.
fn row_labels(outline: Option<&Outline>, case: &str) -> String {
    let mut labels = Vec::new();
    if let Some(place) = outline.and_then(|o| o.place(case)) {
        if let Some(formalism) = place.formalism {
            labels.push(formalism.label().to_owned());
        }
        if place.trial {
            labels.push(String::from("TRIAL text"));
        }
        if !place.reports.is_empty() {
            labels.push(SPEC_DIVERGE_TAG.to_owned());
        }
    }
    if labels.is_empty() {
        String::new()
    } else {
        format!(", {}", labels.join(", "))
    }
}

/// Writes one row that needs attention as a collapsed block: the summary
/// names the outcome, the case, the row and where it sits, and opening it
/// shows the four-part explanation as a table with the cited sentence
/// quoted below it.
fn md_row(out: &mut String, outline: Option<&Outline>, row: &ResultRow, place: &str) {
    let mark = Mark::of(row);
    let labels = row_labels(outline, &row.case);
    let _ = writeln!(
        out,
        "\n<details>\n<summary><b>{}</b>: <code>{}</code> row {}, {}{}</summary>\n",
        outcome_word(mark),
        html_text(&row.case),
        row.row,
        html_text(place),
        html_text(&labels),
    );
    out.push_str("| | |\n| --- | --- |\n");
    if let Some(why) = &row.explanation {
        let _ = writeln!(out, "| **Step** | {} |", md_code(&why.step));
        let _ = writeln!(out, "| **What was checked** | {} |", md_inline(&why.what));
        let _ = writeln!(out, "| **Expected** | {} |", md_inline(&why.expected));
        let _ = writeln!(out, "| **Received** | {} |", md_inline(&why.received));
        if let Some(exchange) = &why.exchange {
            let _ = writeln!(
                out,
                "| **Exchange** | {} answered {}, recording entry {} |",
                md_code(&format!("{} {}", exchange.method, exchange.url)),
                exchange.status,
                exchange.entry
            );
        }
        let whose = match why.whose {
            Whose::Server => "The server. A failed row counts against the claimed capability.",
            Whose::Instrument => {
                "The instrument could not judge it. The row does not count against the server."
            }
            Whose::Warning => {
                "Warning. The server did not follow a SHOULD, and the row never counts against it."
            }
        };
        let _ = writeln!(out, "| **Whose problem** | {whose} |");
    }
    if !row.inputs.is_empty() {
        let inputs: Vec<String> = row
            .inputs
            .iter()
            .map(|(name, value)| {
                let generated = if row.generated.contains(name) {
                    " (generated)"
                } else {
                    ""
                };
                format!(
                    "{} = {}{generated}",
                    md_code(name),
                    md_code(&value.to_string())
                )
            })
            .collect();
        let _ = writeln!(out, "| **Inputs** | {} |", inputs.join(", "));
    }
    let why = row.explanation.as_ref().and_then(|e| e.why.as_ref());
    if let Some(report) = why.and_then(|w| w.report) {
        let entry = outline.and_then(|o| o.report(report));
        let _ = writeln!(
            out,
            "| **Upstream report** | {} |",
            report_ref(
                report.get(),
                entry.map(|e| e.title.as_str()),
                entry.map(|e| e.state.to_string())
            )
        );
    }
    match why {
        Some(why) => {
            let source = format!("{}, {}", md_inline(&why.source), md_inline(&why.section));
            let source = match &why.link {
                Some(link) => format!("[{source}]({link})"),
                None => source,
            };
            let _ = writeln!(
                out,
                "\n**Why it is required:** {source}\n\n> {}",
                md_inline(&why.quote)
            );
        }
        None => out.push_str("\n**Why it is required:** no citation resolved.\n"),
    }
    out.push_str("\n</details>\n");
}

/// Writes the rows of `cases` that need attention, each as a collapsed
/// block, under a short lead-in; nothing when every row passed or was
/// skipped.
fn md_attention<'a>(
    out: &mut String,
    outline: Option<&Outline>,
    cases: impl IntoIterator<Item = (&'a CaseRows<'a>, String)>,
) {
    let mut lead = false;
    for (case, place) in cases {
        for row in case.rows.iter().filter(|row| needs_attention(row)) {
            if !lead {
                out.push_str("\nRows to look at (open one for its explanation):\n");
                lead = true;
            }
            md_row(out, outline, row, &place);
        }
    }
}

/// Writes BASE: per chapter a table of its bands, and per band a table of
/// its sections followed by the rows that need attention.
fn md_base(out: &mut String, outline: &Outline, reading: &Reading<'_>) {
    out.push_str(
        "\n## BASE\n\nWhat every claimant of a capability must do. A case no option arm \
         names is BASE.\n",
    );
    for chapter in &reading.base {
        let _ = writeln!(
            out,
            "\n### {}\n\n| Band | {COUNT_HEADS} |\n| --- | {COUNT_ALIGN} |",
            md_inline(chapter.title)
        );
        for band in &chapter.bands {
            let _ = writeln!(
                out,
                "| {} | {} |",
                md_inline(band.title),
                count_cells(band.tally)
            );
        }
        for band in chapter.bands.iter().filter(|b| b.tally.tests > 0) {
            let _ = writeln!(
                out,
                "\n#### {}\n\n| Section | {COUNT_HEADS} |\n| --- | {COUNT_ALIGN} |",
                md_inline(band.title)
            );
            for section in &band.sections {
                let formalism = section
                    .formalism
                    .map(|f| format!(" ({})", f.label()))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "| {}{formalism} | {} |",
                    md_inline(section.title),
                    count_cells(section.tally)
                );
            }
            md_attention(
                out,
                Some(outline),
                band.sections.iter().flat_map(|section| {
                    section
                        .cases
                        .iter()
                        .map(move |case| (case, format!("BASE, {}", section.title)))
                }),
            );
        }
    }
}

/// The kind cell of an option's table row: its tier, and the tag for an
/// option that exists only because the released text diverges.
fn kind_cell(group: &OptionGroup<'_>) -> String {
    if group.tier == Tier::SpecBoundary {
        format!("`{SPEC_DIVERGE_TAG}`")
    } else {
        group.tier.title().to_owned()
    }
}

/// The note that closes a view when cases rest on diverging text: how many,
/// their tag, and the reports they are judged by; empty when none does.
#[must_use]
pub fn divergence_note(divergence: &SpecDivergence) -> Option<(String, Vec<u64>)> {
    (divergence.cases > 0).then(|| {
        (
            format!(
                "Spec divergence: {} case(s) rest on released text that is unclear or contradictory, and are tagged {SPEC_DIVERGE_TAG}. Until the specification resolves it, Veredictum judges them as the linked upstream reports describe:",
                divergence.cases
            ),
            divergence.reports.iter().map(|r| r.get()).collect(),
        )
    })
}

/// The arm cell of an option's table row.
fn arm_cell(group: &OptionGroup<'_>) -> String {
    let selected: Vec<String> = group
        .arms
        .iter()
        .filter(|arm| arm.selected)
        .map(|arm| {
            if arm.empty {
                format!("{}, nothing to run", md_code(arm.arm))
            } else {
                md_code(arm.arm)
            }
        })
        .collect();
    if selected.is_empty() {
        String::from("not selected")
    } else {
        selected.join(", ")
    }
}

/// Writes one table row per option, and a TRIAL row when the band has one.
fn md_option_rows(out: &mut String, options: &[OptionGroup<'_>], trial: Option<Tally>) {
    for group in options {
        let counts = if group.selected {
            count_cells(group.tally)
        } else {
            String::from(" | | | | ")
        };
        let _ = writeln!(
            out,
            "| {} | {} | {} | {counts} |",
            md_code(group.option),
            kind_cell(group),
            arm_cell(group)
        );
    }
    if let Some(tally) = trial {
        let _ = writeln!(
            out,
            "| TRIAL text | BASE on TRIAL text | | {} |",
            count_cells(tally)
        );
    }
}

/// Writes what each option of a band decides, collapsed.
fn md_option_notes(out: &mut String, outline: &Outline, options: &[OptionGroup<'_>], trial: bool) {
    if options.is_empty() && !trial {
        return;
    }
    out.push_str("\n<details>\n<summary>What these options decide</summary>\n\n");
    for group in options {
        let _ = write!(
            out,
            "- **{}** ({}): {}",
            md_code(group.option),
            group.theme.title(),
            md_inline(group.description)
        );
        if let Some(report) = group.report {
            let entry = outline.report(report);
            let _ = write!(
                out,
                " Upstream report {}.",
                report_ref(
                    report.get(),
                    entry.map(|e| e.title.as_str()),
                    entry.map(|e| e.state.to_string())
                )
            );
        }
        out.push('\n');
    }
    if trial {
        out.push_str(
            "- **TRIAL text**: BASE cases whose cited text the release marks TRIAL. They are \
             read with the options and never count in the BASE tally.\n",
        );
    }
    out.push_str("\n</details>\n");
}

/// Writes one tier like BASE: per chapter and band a table of its options
/// with the selected arm and the counts, what the options decide, and the
/// rows that need attention.
fn md_options(out: &mut String, outline: &Outline, tree: &OptionsTree<'_>) {
    if tree.is_empty() {
        out.push_str("\nNo option.\n");
        return;
    }
    for chapter in &tree.chapters {
        let _ = writeln!(out, "\n### {}", md_inline(chapter.title));
        for band in &chapter.bands {
            let _ = writeln!(
                out,
                "\n#### {}\n\n| Option | Kind | Selected arm | {COUNT_HEADS} |\n| --- | --- | --- | {COUNT_ALIGN} |",
                md_inline(band.title)
            );
            md_option_rows(out, &band.options, band.trial.as_ref().map(|t| t.tally));
            md_option_notes(out, outline, &band.options, band.trial.is_some());
            let option_cases = band.options.iter().flat_map(|group| {
                group
                    .arms
                    .iter()
                    .filter(|arm| arm.selected)
                    .flat_map(move |arm| {
                        arm.cases
                            .iter()
                            .map(move |case| (case, format!("option {}", group.option)))
                    })
            });
            let trial_cases = band
                .trial
                .iter()
                .flat_map(|t| t.cases.iter().map(|case| (case, String::from("OPTIONS"))));
            md_attention(out, Some(outline), option_cases.chain(trial_cases));
        }
    }
    if !tree.unanchored.is_empty() {
        let _ = writeln!(
            out,
            "\n### Options no document tests yet\n\n| Option | Kind | Selected arm | {COUNT_HEADS} |\n| --- | --- | --- | {COUNT_ALIGN} |"
        );
        md_option_rows(out, &tree.unanchored, None);
        md_option_notes(out, outline, &tree.unanchored, false);
    }
}

/// Writes the summary table: one line per part of the result, then every
/// row once.
fn md_summary(out: &mut String, reading: &Reading<'_>, placed: bool) {
    let _ = writeln!(
        out,
        "\n## Summary\n\n| Part | {COUNT_HEADS} | Total |\n| --- | {COUNT_ALIGN} | ---: |"
    );
    if placed {
        for (part, tally) in [
            ("BASE", reading.base_total),
            ("OPTIONS", reading.options.tally),
        ] {
            let _ = writeln!(out, "| {part} | {} | {} |", count_cells(tally), tally.tests);
        }
    }
    let total = reading.total;
    let _ = writeln!(
        out,
        "| **Every row** | **{}** | **{}** | **{}** | **{}** | **{}** | **{}** |",
        total.passed, total.failed, total.warning, total.errored, total.skipped, total.tests
    );
    out.push_str(
        "\n> [!NOTE]\n> Only a **failed** row counts against the server. A **warning** row \
         is a SHOULD the server did not follow, and it never counts against the server. An \
         **errored** row is one the instrument could not judge, and a **skipped** row was \
         not run.\n",
    );
}

/// Writes the Markdown summary of `results`: the page a GitHub Actions job
/// summary or a pull request shows.
///
/// A table of what was run, then a summary table with one line per part of
/// the result. When the results carry an outline, BASE follows chapter by
/// chapter, each chapter a table of its bands and each band a table of its
/// sections; RECOMMENDED, OPTIONS (with the cases resting on TRIAL text) and
/// where the released spec is defective follow the same chapters and bands,
/// each band a table of its options with the selected arm. Only the rows that
/// need attention are listed: each failed, warning or errored row is a
/// collapsed block whose summary names the outcome and the case, and which
/// opens on its explanation as a table with the cited sentence quoted. The
/// cases resting on reported text follow by report, the warning rows in
/// their own table with their citations, and the skipped rows counted by
/// reason. A case the outline does not place, or every case when the results
/// carry no outline, is listed under Unplaced. A warning row is never
/// counted as failed.
#[must_use]
pub fn markdown(results: &Results) -> String {
    let reading = read(results);
    let outline = results.outline.as_ref();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Veredictum results\n\n| | |\n| --- | --- |\n| **Server** | {} |\n| **Instrument** | {} {}, {} |\n| **Document** | {} |",
        md_inline(&results.base_url),
        md_inline(&results.engine.name),
        md_inline(&results.engine.version),
        md_inline(&results.instrument_status.to_string()),
        md_code(&results.document),
    );
    if !results.specification.is_released() {
        let _ = writeln!(
            out,
            "| **Specification** | {} |\n\n> [!WARNING]\n> Judged against a pinned snapshot of the openEHR development branch, not the release. \
             This is never a released result: it builds no certificate and is never compared with \
             or counted toward one.",
            md_inline(&results.specification.label()),
        );
    }
    md_summary(&mut out, &reading, outline.is_some());
    if let Some(outline) = outline {
        md_base(&mut out, outline, &reading);
        out.push_str(
            "\n## OPTIONS\n\nWhat the statement selected. Each option is labelled with its kind: \
             **Recommended (SHOULD)**, which a server may decline with a warning row; \
             **Optional (MAY)**, behaviour the release leaves to the implementation; or \
             **Spec defect**, an option only because the released text is defective. The \
             statement selects one arm of each, and the other arms are skipped.\n",
        );
        md_options(&mut out, outline, &reading.options);
    }
    if !reading.unplaced.is_empty() {
        out.push_str("\n## Unplaced\n\n");
        out.push_str(if outline.is_some() {
            "The outline the results carry places these cases nowhere.\n"
        } else {
            "The results carry no outline, so no case is placed in BASE or OPTIONS.\n"
        });
        let _ = writeln!(out, "\n| Case | Outcome |\n| --- | --- |");
        for case in &reading.unplaced {
            let labels = case_labels(case);
            let labels = if labels.is_empty() {
                String::new()
            } else {
                format!(" ({})", labels.join(", "))
            };
            let _ = writeln!(
                out,
                "| {}{} | {} |",
                md_code(case.case),
                md_inline(&labels),
                tally_words(case.tally())
            );
        }
        md_attention(
            &mut out,
            outline,
            reading
                .unplaced
                .iter()
                .map(|case| (case, String::from("unplaced"))),
        );
    }
    md_warning(&mut out, results);
    md_skipped(&mut out, results);
    if let Some((text, reports)) = divergence_note(&reading.spec_divergence()) {
        let links: Vec<String> = reports
            .iter()
            .map(|r| format!("[#{r}]({REPORT_BASE}{r})"))
            .collect();
        let _ = writeln!(out, "\n---\n\n{} {}.", md_inline(&text), links.join(", "));
    }
    out
}

/// Writes the table of warning rows, when there is one.
fn md_warning(out: &mut String, results: &Results) {
    let warning: Vec<&ResultRow> = results.rows.iter().filter(|r| is_warning(r)).collect();
    if warning.is_empty() {
        return;
    }
    out.push_str(
        "\n## Warning rows\n\nEach is a SHOULD the server did not follow. None counts \
         against the server.\n\n| Row | Step | SHOULD cited |\n| --- | --- | --- |\n",
    );
    for row in warning {
        let step = row
            .explanation
            .as_ref()
            .map(|why| why.step.as_str())
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            md_code(&test_name(row)),
            md_code(step),
            md_inline(&cited(row)),
        );
    }
}

/// Writes the skipped rows counted by reason and citation, with every
/// skipped row listed in a collapsed table.
fn md_skipped(out: &mut String, results: &Results) {
    let skipped: Vec<&ResultRow> = results
        .rows
        .iter()
        .filter(|row| row.status == Status::Skipped)
        .collect();
    if skipped.is_empty() {
        return;
    }
    let reason_of = |row: &ResultRow| {
        row.skip.as_ref().map_or_else(
            || (String::from("(none recorded)"), String::new()),
            |skip| {
                (
                    skip.reason.to_string(),
                    format!("{}, {}", skip.why.source, skip.why.section),
                )
            },
        )
    };
    let mut by_reason: BTreeMap<(String, String), usize> = BTreeMap::new();
    for row in &skipped {
        *by_reason.entry(reason_of(row)).or_default() += 1;
    }
    out.push_str(
        "\n## Skipped rows\n\nA skipped row was not run, for the reason given. It never \
         counts for or against the server.\n\n| Reason | Citation | Rows |\n| --- | --- | ---: |\n",
    );
    for ((reason, cited), count) in &by_reason {
        let _ = writeln!(
            out,
            "| {} | {} | {count} |",
            md_inline(reason),
            md_inline(cited)
        );
    }
    let _ = writeln!(
        out,
        "\n<details>\n<summary>Every skipped row ({})</summary>\n\n| Row | Reason |\n| --- | --- |",
        skipped.len()
    );
    for row in skipped {
        let _ = writeln!(
            out,
            "| {} | {} |",
            md_code(&test_name(row)),
            md_inline(&reason_of(row).0)
        );
    }
    out.push_str("\n</details>\n");
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
    fn a_code_span_keeps_its_text_and_its_table() {
        assert_eq!(
            md_code("ehr/create_ehr-with_id"),
            "`ehr/create_ehr-with_id`"
        );
        assert_eq!(md_code("a|b"), "`a\\|b`");
        assert_eq!(md_code("x `y` z"), "``x `y` z``");
    }

    #[test]
    fn a_cell_cannot_break_its_table() {
        assert_eq!(md_inline("a|b\n<c>"), "a\\|b &lt;c&gt;");
    }
}
