// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S2 — the catalogue explorer (#61, #64).
//!
//! Capability folders → workflows → one workflow in full, read through the
//! published lib's catalogue listing. Filter, search and page state live in the URL, so a view is
//! shareable, refresh-safe and works without WASM loaded
//! (<https://book.leptos.dev/router/20_form.html>).

use leptos::prelude::{
    AddAnyAttr, ClassAttribute, CollectView, ElementChild, Get, IntoAny, IntoView, Memo, Resource,
    Suspend, Suspense, Transition, With, component, view,
};
use leptos_meta::Title;
use leptos_router::components::{A, Form};
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::catalogue_api::CaseDetail;
use crate::catalogue_api::fns::{fetch_case_detail, fetch_chapter_bands, fetch_chapters};
use crate::components::data_table::{
    TABLE, TABLE_WRAP, TD, TH, TableFooter, page_from_url, page_window,
};
use crate::components::empty_state::EmptyState;
use crate::components::field::INPUT;
use crate::components::format_view::inline_error;
use crate::components::page_header::{Crumb, PageHeader};
use crate::components::surface::{CARD_PAD, CARD_TITLE, WELL};

/// A router param by name, empty when absent — params are user input
/// (rules §9), and the URL reads live in helper fns (the `must_use` trap,
/// rules §2).
fn param(name: &'static str) -> Memo<String> {
    let params = use_params_map();
    Memo::new(move |_| params.with(|p| p.get(name).unwrap_or_default()))
}

/// The `?q=` search filter from the URL.
fn query_q() -> Memo<String> {
    let query = use_query_map();
    Memo::new(move |_| query.with(|q| q.get("q").unwrap_or_default()))
}

/// One labeled fact list on the case card; an empty list says so instead of
/// vanishing.
fn fact_list(label: &'static str, values: Vec<String>) -> impl IntoView + use<> {
    let body = if values.is_empty() {
        view! { <p class="text-sm text-ink-faint">"none"</p> }.into_any()
    } else {
        values
            .into_iter()
            .map(|value| view! { <li class="font-mono text-xs text-ink">{value}</li> })
            .collect_view()
            .into_any()
    };
    view! {
        <div>
            <h3 class="mb-1 text-xs font-semibold uppercase tracking-wide text-ink-muted">
                {label}
            </h3>
            <ul class="space-y-0.5">{body}</ul>
        </div>
    }
}

/// The capability folder list.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Catalogue() -> impl IntoView {
    let rows = Resource::new(|| (), |()| fetch_chapters());

    view! {
        <Title text="Catalogue · Veredictum console" />
        <PageHeader
            title="Catalogue"
            subtitle="Every capability folder, every workflow, and the citations each expectation stands on."
        />
        <Suspense fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the catalogue…"</p> }
        }>
            {move || Suspend::new(async move {
                match rows.await {
                    Ok(chapters) => {
                        view! {
                            <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3">
                                {chapters
                                    .into_iter()
                                    .map(|chapter| {
                                        let href = format!("/catalogue/{}", chapter.key);
                                        view! {
                                            <A
                                                href=href
                                                attr:class="flex items-center justify-between rounded-card border border-edge bg-raised p-4 shadow-card transition-colors hover:border-accent"
                                            >
                                                <span class="font-mono text-sm text-ink">
                                                    {chapter.key}
                                                </span>
                                                <span class="tabular-nums text-sm text-ink-muted">
                                                    {format!("{} cases", chapter.cases)}
                                                </span>
                                            </A>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        }
                            .into_any()
                    }
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
    }
}

/// One capability folder's case listing: one section per Arazzo document,
/// over URL-state search and paging.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[expect(
    clippy::too_many_lines,
    reason = "the filter form and the document sections — one cohesive screen, its sections erased per the hydration rules"
)]
#[component]
pub fn Chapter() -> impl IntoView {
    let chapter = param("chapter");
    let q = query_q();
    let page = page_from_url();
    // Reactive inputs in the SOURCE; the fetcher is untracked by design.
    let rows = Resource::new(
        move || (chapter.get(), q.get()),
        |(chapter, q)| fetch_chapter_bands(chapter, q),
    );

    view! {
        <Title text=move || format!("{} · Catalogue · Veredictum console", chapter.get()) />
        <PageHeader
            title=chapter
            subtitle="One small isolated workflow per behaviour, grouped by the Arazzo document it is in."
            crumbs=vec![Crumb::new("Catalogue", "/catalogue")]
        />
        <div class="mb-4 flex flex-wrap items-center gap-3">
            <Form method="GET" action="">
                <div class="flex items-center gap-2">
                    <input
                        type="search"
                        name="q"
                        value=move || q.get()
                        placeholder="Filter by case id…"
                        class=INPUT
                    />
                    <button type="submit" class="text-sm text-accent hover:underline">
                        "filter"
                    </button>
                </div>
            </Form>
        </div>
        // Transition keeps the old rows visible while a filter reloads —
        // no fallback flash on every submit.
        <Transition fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the chapter…"</p> }
        }>
            {move || Suspend::new(async move {
                let current_page = page.get();
                let chapter_key = chapter.get();
                match rows.await {
                    Ok(bands) => {
                        let total: usize = bands.iter().map(|band| band.cases.len()).sum();
                        if total == 0 {
                            return view! {
                                <EmptyState
                                    icon=icondata_lu::LuSearchX
                                    message="No case matches"
                                    hint="Loosen the filter, or check the capability folder in the URL."
                                />
                            }
                                .into_any();
                        }
                        let (start, end) = page_window(current_page, total);
                        let mut index = 0_usize;
                        let sections = bands
                            .into_iter()
                            .filter_map(|band| {
                                let band_len = band.cases.len();
                                let band_start = index;
                                index += band_len;
                                let visible_from = start.max(band_start);
                                let visible_to = end.min(band_start + band_len);
                                if visible_from >= visible_to {
                                    return None;
                                }
                                let rows = band
                                    .cases
                                    .get(visible_from - band_start..visible_to - band_start)
                                    .unwrap_or_default()
                                    .iter()
                                    .cloned()
                                    .map(|row| {
                                        let href = format!("/catalogue/{chapter_key}/{}", row.id);
                                        // Paging windows over the FLAT case sequence; document
                                        // headers render wherever their first visible case
                                        // lands, so the two-level reading survives paging.
                                        view! {
                                            <tr class="hover:bg-sunken">
                                                <td class=TD>
                                                    <A
                                                        href=href
                                                        attr:class="font-mono text-xs text-accent hover:underline"
                                                    >
                                                        {row.id}
                                                    </A>
                                                </td>
                                                <td class=TD>
                                                    <span class="line-clamp-2 text-sm">{row.summary}</span>
                                                </td>
                                            </tr>
                                        }
                                    })
                                    .collect_view();
                                Some(
                                    view! {
                                        <tbody>
                                            <tr>
                                                <th
                                                    colspan="2"
                                                    class="bg-sunken px-3 py-1.5 text-left text-xs font-semibold text-ink-heading"
                                                >
                                                    {format!("{} · {band_len} case(s)", band.band)}
                                                </th>
                                            </tr>
                                            {rows}
                                        </tbody>
                                    },
                                )
                            })
                            .collect_view();
                        view! {
                            <div class=TABLE_WRAP>
                                <table class=TABLE>
                                    <thead>
                                        <tr>
                                            <th class=TH>"Case id"</th>
                                            <th class=TH>"Summary"</th>
                                        </tr>
                                    </thead>
                                    {sections}
                                </table>
                                <TableFooter
                                    base=format!("/catalogue/{chapter_key}")
                                    page=current_page
                                    total=total
                                />
                            </div>
                        }
                            .into_any()
                    }
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Transition>
    }
}

/// One case in full: summary, description, citation and steps.
///
/// No `must_use_candidate` expect here: the URL-reading helpers keep that
/// lint from firing on this fn (the rules §2 toolchain trap, inverted).
#[component]
pub fn Case() -> impl IntoView {
    let chapter = param("chapter");
    let document = param("document");
    let workflow = param("workflow");
    let id = Memo::new(move |_| format!("{}/{}", document.get(), workflow.get()));
    let detail = Resource::new(move || id.get(), fetch_case_detail);

    view! {
        <Title text=move || format!("{} · Catalogue · Veredictum console", id.get()) />
        <Suspense fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the case…"</p> }
        }>
            {move || Suspend::new(async move {
                let chapter_key = chapter.get();
                match detail.await {
                    Ok(Some(case)) => case_view(&chapter_key, case).into_any(),
                    Ok(None) => {
                        view! {
                            <EmptyState
                                icon=icondata_lu::LuFileX
                                message="No case carries this id"
                                hint="A case id is the document and the workflow it names — check the URL."
                            />
                        }
                            .into_any()
                    }
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
    }
}

/// The loaded case's sections — plain assembly, erased per section
/// (rules §1).
fn case_view(chapter_key: &str, case: CaseDetail) -> impl IntoView + use<> {
    let citation = case.citation.map_or_else(
        || {
            view! {
                <p class="text-sm text-ink-muted">
                    "No workflow-level citation: each criterion carries its own."
                </p>
            }
            .into_any()
        },
        |citation| view! { <p class="font-mono text-xs text-ink">{citation}</p> }.into_any(),
    );
    let steps = case
        .steps
        .into_iter()
        .map(|step| {
            view! {
                <tr>
                    <td class=TD>
                        <span class="font-mono text-xs">{step.id}</span>
                    </td>
                    <td class=TD>
                        <span class="font-mono text-xs">{step.operation}</span>
                    </td>
                    <td class=TD>
                        <span class="text-sm">{step.description}</span>
                    </td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <PageHeader
            title=case.id
            subtitle=case.summary
            crumbs=vec![
                Crumb::new("Catalogue", "/catalogue"),
                Crumb::new(chapter_key.to_owned(), format!("/catalogue/{chapter_key}")),
            ]
        />
        <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
            <section class=format!("{CARD_PAD} lg:col-span-2")>
                <h2 class=CARD_TITLE>"Description"</h2>
                <p class="whitespace-pre-wrap text-sm text-ink">{case.description}</p>
                <div class="mt-3 flex flex-wrap items-center gap-3 text-sm text-ink-muted">
                    <span>{format!("document: {}", case.document)}</span>
                </div>
            </section>
            <section class=CARD_PAD>
                <h2 class=CARD_TITLE>"Spec citation"</h2>
                <div class=WELL>{citation}</div>
                <p class="mt-2 text-sm text-ink-muted">
                    "An expectation is refuted by a better reading of the cited text, and by nothing else."
                </p>
            </section>
            <section class=CARD_PAD>
                <h2 class=CARD_TITLE>"Selection facts"</h2>
                <div class="grid grid-cols-1 gap-3 text-sm">
                    {fact_list("Capability folder", vec![case.chapter])}
                    {fact_list(
                        "Option arm",
                        case.option.map(|option| vec![option]).unwrap_or_default(),
                    )}
                </div>
            </section>
            <section class=format!("{CARD_PAD} lg:col-span-2")>
                <h2 class=CARD_TITLE>"Steps"</h2>
                <div class=TABLE_WRAP>
                    <table class=TABLE>
                        <thead>
                            <tr>
                                <th class=TH>"Step"</th>
                                <th class=TH>"Operation"</th>
                                <th class=TH>"Description"</th>
                            </tr>
                        </thead>
                        <tbody>{steps}</tbody>
                    </table>
                </div>
            </section>
        </div>
    }
}
