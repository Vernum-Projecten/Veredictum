// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S6 — the results surface (#61, #67, #867).
//!
//! The finished run's results document as the engine reads it: the chapter
//! bars, then every row grouped BASE chapter by chapter, then the option
//! tiers and the rows resting on reported text, red rows first in each
//! group. A selected row's four-part explanation and the exchange it was
//! judged on open at the top of the page. The selected row (`?case=`,
//! `?row=`) and the chart's part (`?view=base|options`) live in the URL, so a
//! finding is shareable and every control works without JavaScript.

use std::fmt::Write as _;

use leptos::prelude::{
    AddAnyAttr, AnyView, AriaAttributes, ClassAttribute, CollectView, ElementChild, Get,
    GlobalAttributes, InnerHtmlAttribute, IntoAny, IntoView, Memo, Resource, Suspend, Suspense,
    With, component, view,
};
use leptos_meta::Title;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::chart::{Palette, View, of, render};
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
    ReportView, ResultDetail, ResultRow, ResultsScreen, Tally, TierView,
};

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

/// The `?view=` part of the chart from the URL; BASE unless it names
/// `options`.
fn query_view() -> Memo<View> {
    let query = use_query_map();
    Memo::new(move |_| query.with(|q| View::from_token(&q.get("view").unwrap_or_default())))
}

/// The token a view travels as in the URL.
const fn view_token(view: View) -> &'static str {
    match view {
        View::Base => "base",
        View::Options => "options",
    }
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

/// The address of one row's detail on this page, keeping the chart's part.
fn row_href(row: &ResultRow, view: View) -> String {
    let mut href = format!(
        "/run/results?case={}&row={}",
        query_value(&row.case),
        row.row
    );
    if view == View::Options {
        href.push_str("&view=options");
    }
    href
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

/// One group's counts as one line of glyphs.
fn tally_line(tally: &Tally) -> String {
    format!(
        "{} rows · {} ✓ · {} ✕ · {} ! · {} ? · {} ○",
        tally.tests, tally.passed, tally.failed, tally.advisory, tally.errored, tally.skipped
    )
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
    let view = query_view();
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
            subtitle="The results by chapter, red rows first — a red row names a defect in exactly one of three suspects."
            crumbs=vec![Crumb::new("Run", "/run/connect")]
        >
            <div class="flex items-center gap-1">{steps("results")}</div>
        </PageHeader>
        // The detail renders ABOVE the row list: a navigation scrolls to the
        // top, so a detail below thousands of rows opens out of sight (#869).
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                match detail.await {
                    Ok(Some(found)) => detail_view(&found, view.get()).into_any(),
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
                    Ok(Some(results)) => results_view(&results, view.get()).into_any(),
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
        <div class="mb-4 grid grid-cols-2 gap-4 lg:grid-cols-3 xl:grid-cols-5">
            <StatCard label="Passed" value=total.passed.to_string() icon=icondata_lu::LuCheck />
            <StatCard label="Failed" value=total.failed.to_string() icon=icondata_lu::LuX />
            <StatCard
                label="Advisory (a SHOULD not followed)"
                value=total.advisory.to_string()
                icon=icondata_lu::LuCircleAlert
            />
            <StatCard
                label="Errored (inconclusive)"
                value=total.errored.to_string()
                icon=icondata_lu::LuCircleHelp
            />
            <StatCard
                label="Skipped with citation"
                value=total.skipped.to_string()
                icon=icondata_lu::LuFileText
            />
        </div>
    }
    .into_any()
}

/// The chapter bars, both theme variants, with the two links choosing BASE
/// or OPTIONS.
fn chart_view(reading: &ReadingView, view: View) -> AnyView {
    let chart = of(reading, view);
    let light = render(&chart, Palette::Light);
    let dark = render(&chart, Palette::Dark);
    let link = |target: View, label: &'static str| {
        let current = target == view;
        let class = if current {
            "rounded-control bg-accent-subtle px-2 py-1 font-medium text-accent-ink"
        } else {
            "rounded-control px-2 py-1 text-accent hover:underline"
        };
        view! {
            <a
                href=format!("/run/results?view={}", view_token(target))
                class=class
                aria-current=current.then_some("true")
            >
                {label}
            </a>
        }
    };
    let title = match view {
        View::Base => "BASE outcomes by chapter",
        View::Options => "OPTIONS outcomes by tier",
    };
    view! {
        <section id="outcome-chart" class=format!("{CARD_PAD} mb-4")>
            <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
                <h2 class="text-sm font-semibold text-ink-heading">{title}</h2>
                <nav aria-label="Chart part" class="flex items-center gap-1 text-sm">
                    {link(View::Base, "BASE")}
                    {link(View::Options, "OPTIONS")}
                </nav>
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

/// The table body rows of one group.
fn row_cells(rows: &[ResultRow], view: View) -> AnyView {
    rows.iter()
        .map(|row| {
            let href = row_href(row, view);
            let chip = mark_chip(&row.mark);
            let badges = badges(row);
            view! {
                <tr class="hover:bg-sunken">
                    <td class=TD>
                        <a
                            href=href
                            class="result-row font-mono text-xs text-accent hover:underline"
                        >
                            {row.case.clone()}
                        </a>
                        {badges}
                    </td>
                    <td class=TD>
                        <span class="tabular-nums">{row.row}</span>
                    </td>
                    <td class=TD>
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
        })
        .collect_view()
        .into_any()
}

/// The column heads every group's table shares.
fn table_head() -> AnyView {
    view! {
        <thead>
            <tr>
                <th class=TH>"Case"</th>
                <th class=TH>"Row"</th>
                <th class=TH>"Outcome"</th>
                <th class=TH>"Received or skip reason"</th>
            </tr>
        </thead>
    }
    .into_any()
}

/// One labelled block of a table: a header row carrying the anchor, then
/// the rows.
fn table_block(anchor: String, heading: AnyView, rows: &[ResultRow], view: View) -> AnyView {
    let cells = row_cells(rows, view);
    view! {
        <tbody id=anchor>
            <tr>
                <th
                    colspan="4"
                    class="bg-sunken px-3 py-1.5 text-left text-xs font-semibold text-ink-heading"
                >
                    {heading}
                </th>
            </tr>
            {cells}
        </tbody>
    }
    .into_any()
}

/// A one-line heading of a block: its title and its counts.
fn block_heading(title: String, extra: Option<AnyView>, tally: &Tally) -> AnyView {
    let line = tally_line(tally);
    view! {
        <span>{title}</span>
        {extra}
        <span class="ml-2 font-normal text-ink-muted">{line}</span>
    }
    .into_any()
}

/// One BASE band: its sections as blocks of one table, or "no cases".
fn band_view(band: &BandView, view: View) -> AnyView {
    if band.sections.is_empty() {
        return view! {
            <p id=band.anchor.clone() class="mb-3 text-sm text-ink-muted">
                <span class="font-medium text-ink">{band.title.clone()}</span>
                ": no cases"
            </p>
        }
        .into_any();
    }
    let blocks = band
        .sections
        .iter()
        .map(|section| {
            let extra = section
                .formalism
                .clone()
                .map(|label| view! { <span class=BADGE>{label}</span> }.into_any());
            table_block(
                section.anchor.clone(),
                block_heading(section.title.clone(), extra, &section.tally),
                &section.rows,
                view,
            )
        })
        .collect_view();
    let head = table_head();
    view! {
        <div id=band.anchor.clone() class="mb-4">
            <h4 class="mb-1 text-sm font-semibold text-ink-heading">
                {band.title.clone()}
                <span class="ml-2 font-normal text-ink-muted">{tally_line(&band.tally)}</span>
            </h4>
            <div class=TABLE_WRAP>
                <table class=TABLE>{head} {blocks}</table>
            </div>
        </div>
    }
    .into_any()
}

/// One BASE chapter with its bands.
fn chapter_view(chapter: &ChapterView, view: View) -> AnyView {
    let bands = chapter
        .bands
        .iter()
        .map(|band| band_view(band, view))
        .collect_view();
    view! {
        <section id=chapter.anchor.clone() class="mb-6">
            <h3 class="mb-2 text-base font-semibold text-ink-heading">
                <a href=chapter.href.clone() class="hover:underline">
                    {chapter.title.clone()}
                </a>
                <span class="ml-2 text-sm font-normal text-ink-muted">
                    {tally_line(&chapter.tally)}
                </span>
            </h3>
            {bands}
        </section>
    }
    .into_any()
}

/// One option: its description and one block per arm.
fn option_view(option: &OptionView, view: View) -> AnyView {
    let blocks = option
        .arms
        .iter()
        .map(|arm| {
            let state = if arm.empty {
                "nothing to run"
            } else if arm.selected {
                "selected"
            } else {
                "not selected"
            };
            let extra = Some(view! { <span class=BADGE>{state}</span> }.into_any());
            table_block(
                format!("{}-{}", option.anchor, arm.arm),
                block_heading(arm.arm.clone(), extra, &arm.tally),
                &arm.rows,
                view,
            )
        })
        .collect_view();
    let report = option.report.map(|report| {
        view! {
            <a href=format!("#report-{report}") class=format!("{BADGE} hover:text-accent")>
                {format!("report #{report}")}
            </a>
        }
    });
    let head = table_head();
    view! {
        <div id=option.anchor.clone() class="mb-4">
            <h4 class="text-sm font-semibold text-ink-heading">
                <span class="font-mono">{option.option.clone()}</span>
                <span class=BADGE>{option.theme.clone()}</span>
                {report}
                <span class="ml-2 font-normal text-ink-muted">{tally_line(&option.tally)}</span>
            </h4>
            <p class="mb-1 text-sm text-ink-muted">{option.description.clone()}</p>
            <div class=TABLE_WRAP>
                <table class=TABLE>{head} {blocks}</table>
            </div>
        </div>
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
    view: View,
) -> AnyView {
    let head = table_head();
    let cells = row_cells(rows, view);
    let note = note.map(|note| view! { <p class="mb-1 text-sm text-ink-muted">{note}</p> });
    view! {
        <div id=anchor class="mb-4">
            <h4 class="text-sm font-semibold text-ink-heading">
                {title} <span class="ml-2 font-normal text-ink-muted">{tally_line(tally)}</span>
            </h4>
            {note}
            <div class=TABLE_WRAP>
                <table class=TABLE>{head} <tbody>{cells}</tbody></table>
            </div>
        </div>
    }
    .into_any()
}

/// One report's rows, with its title and state from the committed snapshot.
fn report_view(report: &ReportView, view: View) -> AnyView {
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
        view,
    )
}

/// One option tier: its options, the TRIAL block on OPTIONS, and on the spec
/// boundary the rows grouped by the report their text rests on.
fn tier_view(tier: &TierView, reports: &[ReportView], view: View, boundary: bool) -> AnyView {
    let options = if tier.options.is_empty() {
        view! { <p class="mb-3 text-sm text-ink-muted">"No option of this tier."</p> }.into_any()
    } else {
        tier.options
            .iter()
            .map(|option| option_view(option, view))
            .collect_view()
            .into_any()
    };
    let trial = tier.trial.as_ref().map(|trial| {
        rows_block(
            trial.anchor.clone(),
            trial.title.clone(),
            Some(String::from(
                "BASE cases whose citations rest on text the release marks TRIAL, read with the options.",
            )),
            &trial.tally,
            &trial.rows,
            view,
        )
    });
    let reports = boundary.then(|| {
        let blocks = reports
            .iter()
            .map(|report| report_view(report, view))
            .collect_view();
        view! {
            <h4 class="mb-2 mt-4 text-sm font-semibold text-ink-heading">
                "Rows resting on text reported as defective"
            </h4>
            {if reports.is_empty() {
                view! {
                    <p class="mb-3 text-sm text-ink-muted">
                        "No row rests on text an upstream report records."
                    </p>
                }
                    .into_any()
            } else {
                blocks.into_any()
            }}
        }
    });
    view! {
        <section id=tier.anchor.clone() class="mb-6">
            <h3 class="mb-2 text-base font-semibold text-ink-heading">
                {tier.title.clone()}
                <span class="ml-2 text-sm font-normal text-ink-muted">
                    {tally_line(&tier.tally)}
                </span>
            </h3>
            {options}
            {trial}
            {reports}
        </section>
    }
    .into_any()
}

/// Every row, grouped as the engine reads the results.
fn grouped_view(reading: &ReadingView, view: View) -> AnyView {
    let chapters = reading
        .chapters
        .iter()
        .map(|chapter| chapter_view(chapter, view))
        .collect_view();
    let last = reading.tiers.len().saturating_sub(1);
    let tiers = reading
        .tiers
        .iter()
        .enumerate()
        .map(|(at, tier)| tier_view(tier, &reading.reports, view, at == last))
        .collect_view();
    let unplaced = (!reading.unplaced.is_empty()).then(|| {
        rows_block(
            String::from("unplaced"),
            String::from("Placed nowhere"),
            Some(String::from(
                "Rows the results document places in no chapter or option.",
            )),
            &Tally::default(),
            &reading.unplaced,
            view,
        )
    });
    view! {
        <div id="result-rows">
            <h2 class="mb-3 text-lg font-semibold text-ink-heading">"BASE"</h2>
            {chapters}
            {tiers}
            {unplaced}
        </div>
    }
    .into_any()
}

/// The tallies, the chart and the grouped rows.
fn results_view(results: &ResultsScreen, view: View) -> AnyView {
    let stats = stats_view(&results.reading.total);
    let certificate = certificate_view(&results.certificate);
    let chart = chart_view(&results.reading, view);
    let rows = grouped_view(&results.reading, view);
    view! {
        {stats}
        {certificate}
        <p class="mb-2 text-sm text-ink-muted">{format!("Base URL: {}", results.base_url)}</p>
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
/// exchange it was judged on, the catalogue case it came from, and the
/// attribution law stated where a red row is read.
fn detail_view(detail: &ResultDetail, view: View) -> AnyView {
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
    let close = match view {
        View::Base => String::from("/run/results"),
        View::Options => String::from("/run/results?view=options"),
    };
    let chip = mark_chip(&detail.row.mark);
    let badges = badges(&detail.row);
    let wire = recording_view(&detail.recording);
    view! {
        <section id="result-detail" class=format!("{CARD_PAD} mb-4")>
            <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
                <h2 class="text-sm font-semibold text-ink-heading">
                    {format!("{} · row {}", detail.row.case, detail.row.row)}
                </h2>
                <div class="flex items-center gap-3">
                    {catalogue} <a href=close class="text-sm text-ink-muted hover:underline">
                        "Close"
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
    use super::query_value;

    /// A case id keeps its `/` and loses nothing that would split a query.
    #[test]
    fn a_case_id_travels_whole_in_a_query() {
        assert_eq!(
            query_value("agent_tags_get/agent_tags_get-empty"),
            "agent_tags_get/agent_tags_get-empty"
        );
        assert_eq!(query_value("a&b=c d+é"), "a%26b%3Dc%20d%2B%C3%A9");
    }
}
