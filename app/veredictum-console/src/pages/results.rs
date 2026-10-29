// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S6 — the results surface (#61, #67).
//!
//! The finished run's results document, red rows first, with a selected
//! row's four-part explanation and the exchange it was judged on. The
//! selected row lives in the URL (`?case=`, `?row=`), so a finding is
//! shareable.

use leptos::prelude::{
    AddAnyAttr, ClassAttribute, CollectView, ElementChild, Get, IntoAny, IntoView, Memo, Resource,
    Suspend, Suspense, With, component, view,
};
use leptos_meta::Title;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::components::data_table::{TABLE, TABLE_WRAP, TD, TH};
use crate::components::empty_state::EmptyState;
use crate::components::field::BTN_SECONDARY;
use crate::components::format_view::{Pane, inline_error};
use crate::components::page_header::{Crumb, PageHeader};
use crate::components::stat_card::StatCard;
use crate::components::surface::{CARD_PAD, CARD_TITLE};
use crate::pages::run::steps;
use crate::record_api::fns::{fetch_result_detail, fetch_results};
use crate::record_api::{ExchangeView, RecordingView, ResultDetail, ResultsScreen};

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

/// The status chip classes, verdict semantics only.
fn status_chip(status: &str) -> &'static str {
    match status {
        "failed" => "rounded-control bg-danger-subtle px-1.5 py-0.5 text-xs font-medium text-ink",
        "errored" => "rounded-control bg-warn-subtle px-1.5 py-0.5 text-xs font-medium text-ink",
        "passed" => "rounded-control bg-ok-subtle px-1.5 py-0.5 text-xs font-medium text-ink",
        _ => "rounded-control bg-sunken px-1.5 py-0.5 text-xs text-ink-muted",
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
            subtitle="The results, red rows first — a red row names a defect in exactly one of three suspects."
            crumbs=vec![Crumb::new("Run", "/run/connect")]
        >
            <div class="flex items-center gap-1">{steps("results")}</div>
        </PageHeader>
        <Suspense fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the results…"</p> }
        }>
            {move || Suspend::new(async move {
                match screen.await {
                    Ok(Some(results)) => results_view(&results).into_any(),
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
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                match detail.await {
                    Ok(Some(found)) => detail_view(&found).into_any(),
                    Ok(None) => ().into_any(),
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
    }
}

/// The tallies and the red-first table.
fn results_view(results: &ResultsScreen) -> impl IntoView + use<> {
    let (passed, failed, errored, skipped) = results.tallies;
    let rows = results
        .rows
        .iter()
        .cloned()
        .map(|row| {
            let href = format!("/run/results?case={}&row={}", row.case, row.row);
            let chip = status_chip(&row.status);
            view! {
                <tr class="hover:bg-sunken">
                    <td class=TD>
                        <A href=href attr:class="font-mono text-xs text-accent hover:underline">
                            {row.case}
                        </A>
                    </td>
                    <td class=TD>
                        <span class="tabular-nums">{row.row}</span>
                    </td>
                    <td class=TD>
                        <span class=chip>{row.status}</span>
                    </td>
                    <td class=TD>
                        <span class="line-clamp-2 font-mono text-xs">
                            {row.reason.unwrap_or_default()}
                        </span>
                    </td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <div class="mb-4 grid grid-cols-2 gap-4 xl:grid-cols-4">
            <StatCard label="Passed" value=passed.to_string() icon=icondata_lu::LuCheck />
            <StatCard label="Failed" value=failed.to_string() icon=icondata_lu::LuX />
            <StatCard
                label="Errored (inconclusive)"
                value=errored.to_string()
                icon=icondata_lu::LuCircleHelp
            />
            <StatCard
                label="Skipped with citation"
                value=skipped.to_string()
                icon=icondata_lu::LuFileText
            />
        </div>
        <p class="mb-2 text-sm text-ink-muted">{format!("Base URL: {}", results.base_url)}</p>
        <div class=TABLE_WRAP>
            <table class=TABLE>
                <thead>
                    <tr>
                        <th class=TH>"Case"</th>
                        <th class=TH>"Row"</th>
                        <th class=TH>"Status"</th>
                        <th class=TH>"Received or skip reason"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
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
/// exchange it was judged on, and the attribution law stated where a red row
/// is read.
fn detail_view(detail: &ResultDetail) -> impl IntoView + use<> {
    let report = detail.report.map(|report| {
        view! {
            <p class="text-sm text-ink">
                {format!("The cited text has a known spec gap: upstream report #{report}.")}
            </p>
        }
    });
    let wire = recording_view(&detail.recording);
    view! {
        <section class=format!("{CARD_PAD} mt-4")>
            <h2 class=CARD_TITLE>{format!("{} · row {}", detail.row.case, detail.row.row)}</h2>
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
}
