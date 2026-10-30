// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S6 — the results surface (#61, #67, #867).
//!
//! The finished run's results document as the engine reads it: the counts,
//! the chapter bars, then the rows grouped BASE chapter by chapter, then the
//! option tiers and the rows resting on reported text. By default a group
//! lists only the rows that need attention (failed, advisory, errored), red
//! first, and says in its heading how many passed or were skipped;
//! `?rows=all` lists every row. A selected row's four-part explanation and
//! the exchange it was judged on open at the top of the page, with a link
//! back to the row in the list. The selected row (`?case=`, `?row=`), the
//! chart's part (`?view=base|options`) and the listing (`?rows=`) live in the
//! URL, so a finding is shareable and every control works without
//! JavaScript.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fmt::Write as _;

use leptos::prelude::{
    AddAnyAttr, AnyView, AriaAttributes, ClassAttribute, CollectView, ElementChild, Get,
    GlobalAttributes, InnerHtmlAttribute, IntoAny, IntoView, Memo, Resource, Suspend, Suspense,
    With, component, view,
};
use leptos_meta::Title;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::chart::{NOT_IN_RUN, Palette, TIERS_NOT_IN_RUN, View, of, render};
use crate::components::data_table::{TABLE, TABLE_WRAP, TD, TH};
use crate::components::empty_state::EmptyState;
use crate::components::field::BTN_SECONDARY;
use crate::components::format_view::{Pane, inline_error};
use crate::components::page_header::{Crumb, PageHeader};
use crate::components::stat_card::StatCard;
use crate::components::surface::{CARD_PAD, CARD_TITLE};
use crate::pages::run::steps;
use crate::record_api::fns::{fetch_result_detail, fetch_results};
use crate::record_api::{
    BandView, CertificateNotice, ChapterView, ExchangeView, OptionView, ReadingView, RecordingView,
    ReportView, ResultDetail, ResultRow, ResultsScreen, SectionView, Tally, TierBandView,
    TierChapterView, TierView,
};

/// Which rows the grouped list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rows {
    /// Only the rows that need attention: failed, advisory, errored.
    Attention,
    /// Every row, passed and skipped included.
    All,
}

impl Rows {
    /// The listing the `?rows=` token names; attention for anything else.
    fn from_token(token: &str) -> Self {
        if token == "all" {
            Self::All
        } else {
            Self::Attention
        }
    }
}

/// Where the list stands in the URL: the chart's part and which rows show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Listing {
    /// The chart's part.
    view: View,
    /// Which rows the list shows.
    rows: Rows,
}

/// The `?case=` selection from the URL.
fn query_case() -> Memo<String> {
    let query = use_query_map();
    Memo::new(move |_| query.with(|q| q.get("case").unwrap_or_default()))
}

/// The `?row=` selection from the URL; `0` when absent or not a number.
fn query_row() -> Memo<u32> {
    let query = use_query_map();
    Memo::new(move |_| {
        query.with(|q| {
            q.get("row")
                .and_then(|row| row.parse().ok())
                .unwrap_or_default()
        })
    })
}

/// The `?view=` part of the chart and the `?rows=` listing from the URL.
fn query_listing() -> Memo<Listing> {
    let query = use_query_map();
    Memo::new(move |_| {
        query.with(|q| Listing {
            view: View::from_token(&q.get("view").unwrap_or_default()),
            rows: Rows::from_token(&q.get("rows").unwrap_or_default()),
        })
    })
}

/// `value` percent-encoded for a query string: everything outside the
/// unreserved set and `/` (RFC 3986 §2.3, §3.4) is escaped.
fn query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The in-page anchor of one result row: `row-<case>-<row>`, the case id
/// kept where it is a letter, digit, `_`, `-` or `/` and every other byte
/// written `.XX`, so the anchor is one and the same token in an `id` and in
/// a URL fragment.
fn row_anchor(case: &str, row: u32) -> String {
    let mut out = String::from("row-");
    for byte in case.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'/') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, ".{byte:02X}");
        }
    }
    let _ = write!(out, "-{row}");
    out
}

/// The address of the results page at `listing`, with the selected row and
/// the fragment when given; the defaults (BASE, attention) stay out of it.
fn page_href(listing: Listing, selected: Option<(&str, u32)>, fragment: Option<&str>) -> String {
    let mut params = Vec::new();
    if let Some((case, row)) = selected {
        params.push(format!("case={}", query_value(case)));
        params.push(format!("row={row}"));
    }
    if listing.view == View::Options {
        params.push(String::from("view=options"));
    }
    if listing.rows == Rows::All {
        params.push(String::from("rows=all"));
    }
    let mut href = String::from("/run/results");
    if !params.is_empty() {
        href.push('?');
        href.push_str(&params.join("&"));
    }
    if let Some(fragment) = fragment {
        href.push('#');
        href.push_str(fragment);
    }
    href
}

/// One group's counts in words: "All 25 rows passed", or the row count and
/// every outcome that occurs.
fn counts_line(tally: &Tally) -> String {
    match (tally.tests, tally.passed == tally.tests) {
        (0, _) => String::from("no rows"),
        (1, true) => String::from("The one row passed"),
        (n, true) => format!("All {n} rows passed"),
        (n, false) => {
            let parts: Vec<String> = [
                (tally.passed, "passed"),
                (tally.failed, "failed"),
                (tally.advisory, "advisory"),
                (tally.errored, "errored"),
                (tally.skipped, "skipped"),
            ]
            .into_iter()
            .filter(|(count, _)| *count > 0)
            .map(|(count, word)| format!("{count} {word}"))
            .collect();
            let rows = if n == 1 {
                String::from("1 row")
            } else {
                format!("{n} rows")
            };
            format!("{rows}: {}", parts.join(", "))
        }
    }
}

/// The line naming, after `prefix`, what the run left out, or nothing when
/// it left nothing.
fn absent_line(prefix: &str, titles: &[&str]) -> Option<AnyView> {
    (!titles.is_empty()).then(|| {
        let line = format!("{prefix} {}", titles.join(", "));
        view! { <p class="mb-4 text-sm italic text-ink-muted">{line}</p> }.into_any()
    })
}

/// The chip classes of a mark, verdict semantics only: an advisory row never
/// wears the failed colour.
fn mark_chip(mark: &str) -> &'static str {
    match mark {
        "failed" => "rounded-control bg-danger-subtle px-1.5 py-0.5 text-xs font-medium text-ink",
        "advisory" => {
            "rounded-control bg-advisory-subtle px-1.5 py-0.5 text-xs font-medium text-ink"
        }
        "errored" => "rounded-control bg-warn-subtle px-1.5 py-0.5 text-xs font-medium text-ink",
        "passed" => "rounded-control bg-ok-subtle px-1.5 py-0.5 text-xs font-medium text-ink",
        _ => "rounded-control bg-sunken px-1.5 py-0.5 text-xs text-ink-muted",
    }
}

/// The small badge classes beside a case id.
const BADGE: &str = "ml-1.5 rounded-control border border-edge-strong px-1 py-px text-[10px] font-medium uppercase tracking-wide text-ink-muted";

/// The counts beside a heading.
const COUNTS: &str = "text-xs font-normal tabular-nums text-ink-muted";

/// A tier heading: BASE, Recommended (SHOULD), Optional (MAY), Spec defect.
const H_TIER: &str = "mb-3 mt-8 flex flex-wrap items-baseline gap-x-3 border-b border-edge pb-1.5 text-lg font-semibold text-ink-heading";

/// A chapter heading.
const H_CHAPTER: &str =
    "mb-2 mt-5 flex flex-wrap items-baseline gap-x-3 text-base font-semibold text-ink-heading";

/// A band heading.
const H_BAND: &str =
    "mb-1.5 flex flex-wrap items-baseline gap-x-3 text-sm font-semibold text-ink-heading";

/// What every listed row needs from the page: the listing, the selected row,
/// and the row anchors already given out, so each anchor names one element.
struct Ctx {
    /// Where the list stands in the URL.
    listing: Listing,
    /// The selected row's case.
    case: Memo<String>,
    /// The selected row's number.
    row: Memo<u32>,
    /// The row anchors already on the page.
    seen: RefCell<BTreeSet<String>>,
}

impl Ctx {
    /// Whether the list shows `row`.
    fn shows(&self, row: &ResultRow) -> bool {
        self.listing.rows == Rows::All || row.needs_attention()
    }

    /// Whether the list shows any of `rows`.
    fn shows_any(&self, rows: &[ResultRow]) -> bool {
        rows.iter().any(|row| self.shows(row))
    }
}

/// The results surface.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Results() -> impl IntoView {
    let case = query_case();
    let row = query_row();
    let listing = query_listing();
    let screen = Resource::new(|| (), |()| fetch_results());
    let detail = Resource::new(
        move || (case.get(), row.get()),
        |(case, row)| async move {
            if case.is_empty() {
                Ok(None)
            } else {
                fetch_result_detail(case, row).await
            }
        },
    );

    view! {
        <Title text="Results · Run · Veredictum console" />
        <PageHeader
            title="Results"
            subtitle="What your server did, chapter by chapter. BASE is what every claimant must do; options are what your statement selected."
            crumbs=vec![Crumb::new("Run", "/run/connect")]
        >
            <div class="flex items-center gap-1">{steps("results")}</div>
        </PageHeader>
        // The detail renders ABOVE the row list: a navigation scrolls to the
        // top, so a detail below thousands of rows opens out of sight (#869).
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                match detail.await {
                    Ok(Some(found)) => detail_view(&found, listing.get()).into_any(),
                    Ok(None) => ().into_any(),
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
        <Suspense fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the results…"</p> }
        }>
            {move || Suspend::new(async move {
                match screen.await {
                    Ok(Some(results)) => {
                        let ctx = Ctx {
                            listing: listing.get(),
                            case,
                            row,
                            seen: RefCell::new(BTreeSet::new()),
                        };
                        results_view(&results, &ctx).into_any()
                    }
                    Ok(None) => {
                        view! {
                            <EmptyState
                                icon=icondata_lu::LuHourglass
                                message="No finished run yet"
                                hint="Results render here the moment a run completes."
                            >
                                <A href="/run/connect" attr:class=BTN_SECONDARY>
                                    "Grade a server"
                                </A>
                            </EmptyState>
                        }
                            .into_any()
                    }
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
    }
}

/// What the screen says about the run's certificate.
fn certificate_view(notice: &CertificateNotice) -> impl IntoView + use<> {
    match notice.clone() {
        CertificateNotice::Trial => view! {
            <p id="certificate-notice" class="mb-4 text-sm text-ink-muted">
                "Trial run: the results and verdicts are complete, and no certificate is issued."
            </p>
        }
        .into_any(),
        CertificateNotice::Issued(href) => view! {
            <p id="certificate-notice" class="mb-4 text-sm text-ink">
                <a
                    href=href
                    rel="external"
                    download="certificate.jws"
                    class="font-medium text-accent hover:underline"
                >
                    "Download the certificate"
                </a>
                " — it verifies offline with "
                <code class="font-mono text-xs">
                    "veredictum verify certificate.jws --jwks jwks.json"
                </code>
            </p>
        }
        .into_any(),
        CertificateNotice::NotIssued(reason) => view! {
            <p id="certificate-notice" class="mb-4 text-sm text-ink">
                {format!("No certificate was issued for this run: {reason}")}
            </p>
        }
        .into_any(),
    }
}

/// The five stat tiles: an advisory row has its own and is never failed.
fn stats_view(total: &Tally) -> AnyView {
    view! {
        <div class="mb-4 grid grid-cols-2 gap-4 md:grid-cols-3 xl:grid-cols-5">
            <StatCard
                label="Passed"
                value=total.passed.to_string()
                icon=icondata_lu::LuCheck
                hint="did what the cited text requires"
            />
            <StatCard
                label="Failed"
                value=total.failed.to_string()
                icon=icondata_lu::LuX
                hint="counts against the server"
            />
            <StatCard
                label="Advisory"
                value=total.advisory.to_string()
                icon=icondata_lu::LuCircleAlert
                hint="a SHOULD not followed, never counts"
            />
            <StatCard
                label="Errored"
                value=total.errored.to_string()
                icon=icondata_lu::LuCircleHelp
                hint="the instrument could not judge"
            />
            <StatCard
                label="Skipped"
                value=total.skipped.to_string()
                icon=icondata_lu::LuFileText
                hint="not run, with its reason"
            />
        </div>
    }
    .into_any()
}

/// One link of a toggle: the current choice is marked, every link keeps the
/// selected row.
fn toggle_link(target: Listing, current: bool, label: String, ctx: &Ctx) -> AnyView {
    let class = if current {
        "rounded-control bg-accent-subtle px-2 py-1 font-medium text-accent-ink"
    } else {
        "rounded-control px-2 py-1 text-accent hover:underline"
    };
    let (case, row) = (ctx.case, ctx.row);
    let href = move || {
        case.with(|case| {
            let selected = (!case.is_empty()).then(|| (case.as_str(), row.get()));
            page_href(target, selected, None)
        })
    };
    view! {
        <a href=href class=class aria-current=current.then_some("true")>
            {label}
        </a>
    }
    .into_any()
}

/// The chapter bars, both theme variants, with the links choosing the
/// chart's part and which rows the list shows.
fn chart_view(reading: &ReadingView, ctx: &Ctx) -> AnyView {
    let here = ctx.listing;
    let chart = of(reading, here.view);
    let light = render(&chart, Palette::Light);
    let dark = render(&chart, Palette::Dark);
    let view_link = |view: View, label: &str| {
        toggle_link(
            Listing { view, ..here },
            here.view == view,
            label.to_owned(),
            ctx,
        )
    };
    let rows_link = |rows: Rows, label: String| {
        toggle_link(Listing { rows, ..here }, here.rows == rows, label, ctx)
    };
    let title = match here.view {
        View::Base => "BASE by chapter",
        View::Options => "Options by tier and chapter",
    };
    view! {
        <section id="outcome-chart" class=format!("{CARD_PAD} mb-6")>
            <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
                <h2 class="text-sm font-semibold text-ink-heading">{title}</h2>
                <div class="flex flex-wrap items-center gap-x-5 gap-y-2 text-sm">
                    <nav aria-label="Chart part" class="flex items-center gap-1">
                        <span class="mr-1 text-xs text-ink-muted">"Chart"</span>
                        {view_link(View::Base, "BASE")}
                        {view_link(View::Options, "Options")}
                    </nav>
                    <nav aria-label="Rows listed" class="flex items-center gap-1">
                        <span class="mr-1 text-xs text-ink-muted">"Rows"</span>
                        {rows_link(
                            Rows::Attention,
                            format!("Needing attention ({})", reading.total.attention()),
                        )}
                        {rows_link(Rows::All, format!("All ({})", reading.total.tests))}
                    </nav>
                </div>
            </div>
            <div class="chart-light" inner_html=light></div>
            <div class="chart-dark" inner_html=dark></div>
        </section>
    }
    .into_any()
}

/// The badges a row carries beside its case id: the template formalism,
/// TRIAL, and each upstream report its text rests on.
fn badges(row: &ResultRow) -> AnyView {
    let formalism = row
        .formalism
        .clone()
        .map(|label| view! { <span class=BADGE>{label}</span> });
    let trial = row.trial.then(|| {
        view! {
            <span class=BADGE title="rests on TRIAL text">
                "TRIAL"
            </span>
        }
    });
    let reports = row
        .reports
        .iter()
        .map(|report| {
            view! {
                <a href=format!("#report-{report}") class=format!("{BADGE} hover:text-accent")>
                    {format!("report #{report}")}
                </a>
            }
        })
        .collect_view();
    view! {
        {formalism}
        {trial}
        {reports}
    }
    .into_any()
}

/// One listed row: its anchor (the first time the row is listed), its link
/// to the detail, and the highlight while it is the selected row.
fn row_view(row: &ResultRow, ctx: &Ctx) -> AnyView {
    let anchor = row_anchor(&row.case, row.row);
    let id = ctx
        .seen
        .borrow_mut()
        .insert(anchor.clone())
        .then_some(anchor);
    let href = page_href(ctx.listing, Some((&row.case, row.row)), Some("detail"));
    let (case, at) = (ctx.case, ctx.row);
    let mine = row.case.clone();
    let number = row.row;
    let selected = move || at.get() == number && case.with(|case| *case == mine);
    let current = selected.clone();
    let chip = mark_chip(&row.mark);
    let badges = badges(row);
    view! {
        <tr id=id class="result-line hover:bg-sunken" class:result-row-selected=selected>
            <td class=TD>
                <a
                    href=href
                    class="result-row font-mono text-xs text-accent hover:underline"
                    aria-current=move || current().then_some("true")
                >
                    {row.case.clone()}
                </a>
                {badges}
            </td>
            <td class=format!("{TD} text-right")>
                <span class="tabular-nums">{row.row}</span>
            </td>
            <td class=format!("{TD} whitespace-nowrap")>
                <span class="mr-1 text-ink-muted" aria-hidden="true">
                    {row.glyph.clone()}
                </span>
                <span class=chip>{row.mark.clone()}</span>
            </td>
            <td class=TD>
                <span class="line-clamp-2 font-mono text-xs">
                    {row.reason.clone().unwrap_or_default()}
                </span>
            </td>
        </tr>
    }
    .into_any()
}

/// The listed rows of one group.
fn row_cells(rows: &[ResultRow], ctx: &Ctx) -> AnyView {
    rows.iter()
        .filter(|row| ctx.shows(row))
        .map(|row| row_view(row, ctx))
        .collect_view()
        .into_any()
}

/// The column heads every group's table shares.
fn table_head() -> AnyView {
    view! {
        <thead>
            <tr>
                <th class=format!("{TH} w-1/2")>"Case"</th>
                <th class=format!("{TH} w-12 text-right")>"Row"</th>
                <th class=format!("{TH} w-28")>"Outcome"</th>
                <th class=TH>"Received, or why it was skipped"</th>
            </tr>
        </thead>
    }
    .into_any()
}

/// One labelled block of a table: a header row carrying the anchor and the
/// counts, then the listed rows.
fn table_block(
    anchor: String,
    title: String,
    extra: Option<AnyView>,
    tally: &Tally,
    rows: &[ResultRow],
    ctx: &Ctx,
) -> AnyView {
    let cells = row_cells(rows, ctx);
    view! {
        <tbody id=anchor>
            <tr>
                <th
                    colspan="4"
                    class="border-t border-edge bg-sunken px-3 py-1.5 text-left text-xs font-semibold text-ink-heading"
                >
                    <span>{title}</span>
                    {extra}
                    <span class=format!("ml-3 {COUNTS}")>{counts_line(tally)}</span>
                </th>
            </tr>
            {cells}
        </tbody>
    }
    .into_any()
}

/// One table of blocks under the shared column heads.
fn blocks_table(blocks: AnyView) -> AnyView {
    let head = table_head();
    view! {
        <div class=TABLE_WRAP>
            <table class=format!("{TABLE} table-fixed")>{head} {blocks}</table>
        </div>
    }
    .into_any()
}

/// One BASE section as a block of its band's table.
fn section_block(section: &SectionView, ctx: &Ctx) -> AnyView {
    let extra = section
        .formalism
        .clone()
        .map(|label| view! { <span class=BADGE>{label}</span> }.into_any());
    table_block(
        section.anchor.clone(),
        section.title.clone(),
        extra,
        &section.tally,
        &section.rows,
        ctx,
    )
}

/// One BASE band: its heading and counts, then its sections as one table
/// when it lists a row.
fn band_view(band: &BandView, ctx: &Ctx) -> AnyView {
    let sections: Vec<&SectionView> = band
        .sections
        .iter()
        .filter(|section| section.tally.tests > 0)
        .collect();
    let table = sections
        .iter()
        .any(|section| ctx.shows_any(&section.rows))
        .then(|| {
            let blocks = sections
                .iter()
                .map(|section| section_block(section, ctx))
                .collect_view()
                .into_any();
            blocks_table(blocks)
        });
    view! {
        <div id=band.anchor.clone() class="mb-4">
            <h4 class=H_BAND>
                <span>{band.title.clone()}</span>
                <span class=COUNTS>{counts_line(&band.tally)}</span>
            </h4>
            {table}
        </div>
    }
    .into_any()
}

/// One BASE chapter with its bands that hold a row.
fn chapter_view(chapter: &ChapterView, ctx: &Ctx) -> AnyView {
    let bands = chapter
        .bands
        .iter()
        .filter(|band| band.tally.tests > 0)
        .map(|band| band_view(band, ctx))
        .collect_view();
    view! {
        <section id=chapter.anchor.clone() class="mb-6">
            <h3 class=H_CHAPTER>
                <a href=chapter.href.clone() class="hover:underline">
                    {chapter.title.clone()}
                </a>
                <span class=COUNTS>{counts_line(&chapter.tally)}</span>
            </h3>
            {bands}
        </section>
    }
    .into_any()
}

/// One block of rows under a heading of its own: the TRIAL rows, a report's
/// rows, the unplaced rows.
fn rows_block(
    anchor: String,
    title: String,
    note: Option<String>,
    tally: &Tally,
    rows: &[ResultRow],
    ctx: &Ctx,
) -> AnyView {
    let table = ctx.shows_any(rows).then(|| {
        let head = table_head();
        let cells = row_cells(rows, ctx);
        view! {
            <div class=TABLE_WRAP>
                <table class=format!("{TABLE} table-fixed")>{head} <tbody>{cells}</tbody></table>
            </div>
        }
    });
    let note = note.map(|note| view! { <p class="mb-1.5 text-xs text-ink-muted">{note}</p> });
    view! {
        <div id=anchor class="mb-4">
            <h4 class=H_BAND>
                <span>{title}</span>
                <span class=COUNTS>{counts_line(tally)}</span>
            </h4>
            {note}
            {table}
        </div>
    }
    .into_any()
}

/// One report's rows, with its title and state from the committed snapshot.
fn report_view(report: &ReportView, ctx: &Ctx) -> AnyView {
    let title = match &report.title {
        Some(title) => format!("report #{}: {title}", report.report),
        None => format!("report #{}", report.report),
    };
    let note = report
        .state
        .clone()
        .map(|state| format!("Where it stands: {state}."));
    rows_block(
        report.anchor.clone(),
        title,
        note,
        &report.tally,
        &report.rows,
        ctx,
    )
}

/// What an option's arm or the option itself reads as its state.
fn arm_state(empty: bool, selected: bool) -> &'static str {
    if empty {
        "nothing to run"
    } else if selected {
        "selected"
    } else {
        "not selected"
    }
}

/// One option under its band: its name with the arm the statement selected,
/// its theme and counts, then its arms as one table when it lists a row.
fn option_view(option: &OptionView, ctx: &Ctx) -> AnyView {
    let arms: Vec<_> = option
        .arms
        .iter()
        .filter(|arm| arm.tally.tests > 0 || arm.selected)
        .collect();
    let listed = arms.iter().any(|arm| ctx.shows_any(&arm.rows));
    let table = listed.then(|| {
        let blocks = arms
            .iter()
            .map(|arm| {
                let state = arm_state(arm.empty, arm.selected);
                let extra = Some(view! { <span class=BADGE>{state}</span> }.into_any());
                table_block(
                    format!("{}-{}", option.anchor, arm.arm),
                    arm.arm.clone(),
                    extra,
                    &arm.tally,
                    &arm.rows,
                    ctx,
                )
            })
            .collect_view()
            .into_any();
        blocks_table(blocks)
    });
    let chosen: Vec<&str> = option
        .arms
        .iter()
        .filter(|arm| arm.selected)
        .map(|arm| arm.arm.as_str())
        .collect();
    let selection = if chosen.is_empty() {
        String::from("not selected")
    } else {
        format!("selected: {}", chosen.join(", "))
    };
    let report = option.report.map(|report| {
        view! {
            <a href=format!("#report-{report}") class=format!("{BADGE} hover:text-accent")>
                {format!("report #{report}")}
            </a>
        }
    });
    let description = listed.then(|| {
        view! { <p class="mb-1.5 text-xs text-ink-muted">{option.description.clone()}</p> }
    });
    view! {
        <div id=option.anchor.clone() class="mb-3 ml-4">
            <h5 class="mb-1 flex flex-wrap items-baseline gap-x-2 text-sm font-semibold text-ink-heading">
                <span class="font-mono" title=option.description.clone()>
                    {option.option.clone()}
                </span>
                <span class="text-xs font-normal text-ink-muted">{selection}</span>
                <span class=BADGE>{option.theme.clone()}</span>
                {report}
                <span class=COUNTS>{counts_line(&option.tally)}</span>
            </h5>
            {description}
            {table}
        </div>
    }
    .into_any()
}

/// One band of a tier: its options in this run, then its TRIAL rows.
fn tier_band_view(band: &TierBandView, ctx: &Ctx) -> AnyView {
    let options = band
        .options
        .iter()
        .filter(|option| option.in_run())
        .map(|option| option_view(option, ctx))
        .collect_view();
    let trial = band
        .trial
        .as_ref()
        .filter(|trial| trial.tally.tests > 0)
        .map(|trial| {
            let block = rows_block(
                trial.anchor.clone(),
                trial.title.clone(),
                Some(String::from(
                    "BASE cases whose citations rest on text the release marks TRIAL, read with the options.",
                )),
                &trial.tally,
                &trial.rows,
                ctx,
            );
            view! { <div class="ml-4">{block}</div> }
        });
    view! {
        <div id=band.anchor.clone() class="mb-4">
            <h4 class=H_BAND>
                <span>{band.title.clone()}</span>
                <span class=COUNTS>{counts_line(&band.tally)}</span>
            </h4>
            {options}
            {trial}
        </div>
    }
    .into_any()
}

/// One chapter of a tier with its bands in this run.
fn tier_chapter_view(chapter: &TierChapterView, ctx: &Ctx) -> AnyView {
    let bands = chapter
        .bands
        .iter()
        .filter(|band| band.in_run())
        .map(|band| tier_band_view(band, ctx))
        .collect_view();
    view! {
        <section id=chapter.anchor.clone() class="mb-5">
            <h3 class=H_CHAPTER>
                <span>{chapter.title.clone()}</span>
                <span class=COUNTS>{counts_line(&chapter.tally)}</span>
            </h3>
            {bands}
        </section>
    }
    .into_any()
}

/// One option tier read like BASE: chapter, band, option, with the TRIAL
/// rows inside their band and the options naming no document after.
fn tier_view(tier: &TierView, ctx: &Ctx) -> AnyView {
    let chapters = tier
        .chapters
        .iter()
        .filter(|chapter| chapter.in_run())
        .map(|chapter| tier_chapter_view(chapter, ctx))
        .collect_view();
    let absent: Vec<&str> = tier
        .chapters
        .iter()
        .filter(|chapter| !chapter.in_run())
        .map(|chapter| chapter.title.as_str())
        .collect();
    let unanchored: Vec<&OptionView> = tier
        .unanchored
        .iter()
        .filter(|option| option.in_run())
        .collect();
    let unanchored = (!unanchored.is_empty()).then(|| {
        let options = unanchored
            .iter()
            .map(|option| option_view(option, ctx))
            .collect_view();
        view! {
            <section class="mb-5">
                <h3 class=H_CHAPTER>"Options naming no document yet"</h3>
                {options}
            </section>
        }
    });
    view! {
        <section id=tier.anchor.clone() class="mb-6">
            <h2 class=H_TIER>
                <span>{tier.title.clone()}</span>
                <span class=COUNTS>{counts_line(&tier.tally)}</span>
            </h2>
            {chapters}
            {unanchored}
            {absent_line(NOT_IN_RUN, &absent)}
        </section>
    }
    .into_any()
}

/// The rows resting on text an upstream report records as defective,
/// grouped by report; nothing when no row does.
fn reports_view(reports: &[ReportView], ctx: &Ctx) -> Option<AnyView> {
    (!reports.is_empty()).then(|| {
        let blocks = reports
            .iter()
            .map(|report| report_view(report, ctx))
            .collect_view();
        view! {
            <section id="reported-rows" class="mb-6">
                <h2 class=H_TIER>"Rows resting on text reported as defective"</h2>
                {blocks}
            </section>
        }
        .into_any()
    })
}

/// The sentence above the list saying which rows it holds.
fn listing_note(total: &Tally, rows: Rows) -> String {
    match (rows, total.attention()) {
        (Rows::All, _) => format!(
            "Listing all {} rows, red rows first in each group.",
            total.tests
        ),
        (Rows::Attention, 0) => String::from(
            "No row needs attention: every row passed or was skipped. Each heading counts its rows.",
        ),
        (Rows::Attention, 1) => String::from(
            "Listing the one row that needs attention. Each heading counts its passed and skipped rows.",
        ),
        (Rows::Attention, n) => format!(
            "Listing the {n} rows that need attention, red first. Each heading counts its passed and skipped rows."
        ),
    }
}

/// The rows, grouped as the engine reads the results.
fn grouped_view(reading: &ReadingView, ctx: &Ctx) -> AnyView {
    let base = reading
        .chapters
        .iter()
        .map(|c| c.tally)
        .fold(Tally::default(), Tally::plus);
    let chapters = reading
        .chapters
        .iter()
        .filter(|chapter| chapter.tally.tests > 0)
        .map(|chapter| chapter_view(chapter, ctx))
        .collect_view();
    let absent_chapters: Vec<&str> = reading
        .chapters
        .iter()
        .filter(|chapter| chapter.tally.tests == 0)
        .map(|chapter| chapter.title.as_str())
        .collect();
    let tiers = reading
        .tiers
        .iter()
        .filter(|tier| tier.in_run())
        .map(|tier| tier_view(tier, ctx))
        .collect_view();
    let absent_tiers: Vec<&str> = reading
        .tiers
        .iter()
        .filter(|tier| !tier.in_run())
        .map(|tier| tier.title.as_str())
        .collect();
    let reports = reports_view(&reading.reports, ctx);
    let unplaced = (!reading.unplaced.is_empty()).then(|| {
        rows_block(
            String::from("unplaced"),
            String::from("Placed nowhere"),
            Some(String::from(
                "Rows the results document places in no chapter or option.",
            )),
            &Tally::of_rows(&reading.unplaced),
            &reading.unplaced,
            ctx,
        )
    });
    let note = listing_note(&reading.total, ctx.listing.rows);
    view! {
        <div id="result-rows">
            <p class="mb-2 text-sm text-ink-muted">{note}</p>
            <h2 class=H_TIER>
                <span>"BASE"</span>
                <span class=COUNTS>{counts_line(&base)}</span>
            </h2>
            {chapters}
            {absent_line(NOT_IN_RUN, &absent_chapters)}
            {tiers}
            {absent_line(TIERS_NOT_IN_RUN, &absent_tiers)}
            {reports}
            {unplaced}
        </div>
    }
    .into_any()
}

/// The tallies, the chart and the grouped rows.
fn results_view(results: &ResultsScreen, ctx: &Ctx) -> AnyView {
    let stats = stats_view(&results.reading.total);
    let certificate = certificate_view(&results.certificate);
    let chart = chart_view(&results.reading, ctx);
    let rows = grouped_view(&results.reading, ctx);
    view! {
        {stats}
        {certificate}
        <p class="mb-3 text-sm text-ink-muted">
            "Base URL: " <span class="font-mono text-xs text-ink">{results.base_url.clone()}</span>
        </p>
        {chart}
        {rows}
    }
    .into_any()
}

/// One exchange as its request and response panes.
fn exchange_view(exchange: &ExchangeView) -> impl IntoView + use<> {
    let entry = exchange.entry;
    let request = format!("{}\n{}", exchange.request_line, exchange.request_headers);
    let response = format!("{}\n{}", exchange.status_line, exchange.response_headers);
    let request_body = exchange
        .request_body
        .clone()
        .map(|body| view! { <Pane label="request body" body=body /> });
    let response_body = exchange
        .response_body
        .clone()
        .map(|body| view! { <Pane label="response body" body=body /> });
    view! {
        <div class="space-y-2 border-t border-line pt-3">
            <p class="text-xs font-medium uppercase tracking-wide text-ink-muted">
                {format!("recording entry {entry}")}
            </p>
            <Pane label="request" body=request />
            {request_body}
            <Pane label="response" body=response />
            {response_body}
        </div>
    }
}

/// The wire section: the judged exchange, or the honest line saying why
/// there is none.
fn recording_view(recording: &RecordingView) -> impl IntoView + use<> {
    match recording {
        RecordingView::NoExchange => {
            view! {
                <p class="text-sm text-ink-muted">
                    "This row names no judged exchange: it passed, was skipped, or went red before a request went out."
                </p>
            }
                .into_any()
        }
        RecordingView::Unreadable(diagnostic) => inline_error(diagnostic).into_any(),
        RecordingView::Exchange(exchange) => exchange_view(exchange).into_any(),
    }
}

/// One labelled part of the explanation, absent when the row has none.
fn part(label: &'static str, value: Option<String>) -> impl IntoView + use<> {
    value.map(|body| view! { <Pane label=label body=body /> })
}

/// The selected row's detail: the engine's four-part explanation beside the
/// exchange it was judged on, the catalogue case it came from, the way back
/// to the row in the list, and the attribution law stated where a red row is
/// read.
fn detail_view(detail: &ResultDetail, listing: Listing) -> AnyView {
    let report = detail.report.map(|report| {
        view! {
            <p class="text-sm text-ink">
                {format!("The cited text has a known spec gap: upstream report #{report}.")}
            </p>
        }
    });
    let catalogue = detail.catalogue.clone().map(|href| {
        view! {
            <a
                id="result-catalogue-case"
                href=href
                class="text-sm font-medium text-accent hover:underline"
            >
                "Open the catalogue case"
            </a>
        }
    });
    let back = page_href(
        listing,
        None,
        Some(&row_anchor(&detail.row.case, detail.row.row)),
    );
    let chip = mark_chip(&detail.row.mark);
    let badges = badges(&detail.row);
    let wire = recording_view(&detail.recording);
    // NOTE: no openEHR spec governs this — our own design; a full load lets
    // the browser itself scroll to the streamed row and mark it `:target`.
    view! {
        <section id="detail" class=format!("{CARD_PAD} mb-4")>
            <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
                <h2 class="text-sm font-semibold text-ink-heading">
                    {format!("{} · row {}", detail.row.case, detail.row.row)}
                </h2>
                <div class="flex items-center gap-4">
                    {catalogue}
                    <a
                        id="result-back"
                        href=back
                        rel="external"
                        class="text-sm font-medium text-accent hover:underline"
                    >
                        "Back to the row"
                    </a>
                </div>
            </div>
            <div class="mb-3 flex flex-wrap items-center gap-1 text-sm">
                <span class="text-ink-muted" aria-hidden="true">
                    {detail.row.glyph.clone()}
                </span>
                <span class=chip>{detail.row.mark.clone()}</span>
                {badges}
            </div>
            {detail
                .summary
                .clone()
                .map(|summary| {
                    view! { <p class="mb-3 text-sm text-ink-muted">{summary}</p> }
                })}
            <div class="space-y-3">
                {part("skipped", detail.skip.clone())} {part("step", detail.step.clone())}
                {part("what", detail.what.clone())} {part("why", detail.why.clone())}
                {part("expected", detail.expected.clone())}
                {part("received", detail.received.clone())} {part("whose", detail.whose.clone())}
                {report} <h3 class=CARD_TITLE>"The wire"</h3> {wire}
                <p class="text-sm text-ink-muted">
                    "A red row names a defect in exactly one of three suspects — the server, the runner, or the catalogue — and the cited text is the reference, never any side's confidence."
                </p>
            </div>
        </section>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::{Listing, Rows, View, counts_line, page_href, query_value, row_anchor};
    use crate::record_api::Tally;

    /// A case id keeps its `/` and loses nothing that would split a query.
    #[test]
    fn a_case_id_travels_whole_in_a_query() {
        assert_eq!(
            query_value("agent_tags_get/agent_tags_get-empty"),
            "agent_tags_get/agent_tags_get-empty"
        );
        assert_eq!(query_value("a&b=c d+é"), "a%26b%3Dc%20d%2B%C3%A9");
    }

    /// A row anchor reads its case id as written, and escapes anything that
    /// a fragment would carry differently from an id.
    #[test]
    fn a_row_anchor_is_one_token_in_an_id_and_a_fragment() {
        assert_eq!(
            row_anchor("composition_create/ok-empty", 3),
            "row-composition_create/ok-empty-3"
        );
        assert_eq!(row_anchor("a b.é", 0), "row-a.20b.2E.C3.A9-0");
    }

    /// Every address keeps the listing and leaves the defaults out.
    #[test]
    fn an_address_keeps_the_listing_and_the_selection() {
        let base = Listing {
            view: View::Base,
            rows: Rows::Attention,
        };
        let all_options = Listing {
            view: View::Options,
            rows: Rows::All,
        };
        assert_eq!(page_href(base, None, None), "/run/results");
        assert_eq!(
            page_href(all_options, Some(("a/b", 2)), Some("detail")),
            "/run/results?case=a/b&row=2&view=options&rows=all#detail"
        );
        assert_eq!(
            page_href(all_options, None, Some("row-a/b-2")),
            "/run/results?view=options&rows=all#row-a/b-2"
        );
    }

    /// A group's counts read as words, every outcome that occurs and no other.
    #[test]
    fn a_group_counts_in_words() {
        let all = Tally {
            tests: 25,
            passed: 25,
            ..Tally::default()
        };
        assert_eq!(counts_line(&all), "All 25 rows passed");
        let mixed = Tally {
            tests: 25,
            passed: 23,
            skipped: 2,
            ..Tally::default()
        };
        assert_eq!(counts_line(&mixed), "25 rows: 23 passed, 2 skipped");
        let red = Tally {
            tests: 4,
            failed: 3,
            advisory: 1,
            ..Tally::default()
        };
        assert_eq!(counts_line(&red), "4 rows: 3 failed, 1 advisory");
        let one = Tally {
            tests: 1,
            passed: 1,
            ..Tally::default()
        };
        assert_eq!(counts_line(&one), "The one row passed");
        assert_eq!(counts_line(&Tally::default()), "no rows");
    }
}
