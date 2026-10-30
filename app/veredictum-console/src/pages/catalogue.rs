// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S2 — the catalogue explorer (#61, #64).
//!
//! Capability folders → BASE sections and options → one workflow in full,
//! read through the published lib's catalogue listing and structure files. Filter, search and page state live in the URL, so a view is
//! shareable, refresh-safe and works without WASM loaded
//! (<https://book.leptos.dev/router/20_form.html>).

use leptos::prelude::{
    AddAnyAttr, AnyView, AriaAttributes, ClassAttribute, CollectView, ElementChild, Get,
    GlobalAttributes, IntoAny, IntoView, Memo, Resource, Suspend, Suspense, Transition, With,
    component, view,
};
use leptos_meta::Title;
use leptos_router::components::{A, Form};
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::catalogue_api::fns::{fetch_case_detail, fetch_chapter_page, fetch_chapters};
use crate::catalogue_api::{ArmBlock, BandRows, CaseDetail, ChapterPage};
use crate::components::data_table::{TABLE, TABLE_WRAP, TD, TH};
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

/// The small badge classes beside a heading or a case id.
const BADGE: &str = "ml-1.5 rounded-control border border-edge-strong px-1 py-px text-[10px] font-medium uppercase tracking-wide text-ink-muted";

/// A case count as a reader says it.
fn cases_label(cases: u64) -> String {
    if cases == 1 {
        String::from("1 case")
    } else {
        format!("{cases} cases")
    }
}

/// The table rows of one document: its header row, then one row per case.
fn document_rows(folder: &str, document: &BandRows) -> AnyView {
    let rows = document
        .cases
        .iter()
        .map(|row| {
            let href = format!("/catalogue/{folder}/{}", row.id);
            let id = row.id.clone();
            let badge = row
                .formalism
                .clone()
                .map(|label| view! { <span class=BADGE>{label}</span> });
            view! {
                <tr class="hover:bg-sunken">
                    <td class=TD>
                        <A href=href attr:class="font-mono text-xs text-accent hover:underline">
                            {id}
                        </A>
                        {badge}
                    </td>
                    <td class=TD>
                        <span class="line-clamp-2 text-sm">{row.summary.clone()}</span>
                    </td>
                </tr>
            }
        })
        .collect_view();
    let heading = format!(
        "{} · {}",
        document.band,
        cases_label(u64::try_from(document.cases.len()).unwrap_or(u64::MAX))
    );
    view! {
        <tbody>
            <tr>
                <th
                    colspan="2"
                    class="bg-sunken px-3 py-1.5 text-left text-xs font-semibold text-ink-heading"
                >
                    {heading}
                </th>
            </tr>
            {rows}
        </tbody>
    }
    .into_any()
}

/// A table of documents with their cases.
fn documents_table(folder: &str, documents: &[BandRows]) -> AnyView {
    let bodies = documents
        .iter()
        .map(|document| document_rows(folder, document))
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
                {bodies}
            </table>
        </div>
    }
    .into_any()
}

/// The table of contents: every section and option with its case count.
fn contents_view(page: &ChapterPage) -> AnyView {
    let entry = |anchor: &str, label: String, cases: u64| {
        view! {
            <li>
                <a href=format!("#{anchor}") class="text-accent hover:underline">
                    {label}
                </a>
                <span class="ml-1 tabular-nums text-ink-muted">{cases_label(cases)}</span>
            </li>
        }
    };
    let sections = page
        .sections
        .iter()
        .map(|s| entry(&s.anchor, s.title.clone(), s.cases))
        .collect_view();
    let others = (!page.unsectioned.is_empty()).then(|| {
        let cases = page
            .unsectioned
            .iter()
            .map(|d| u64::try_from(d.cases.len()).unwrap_or(u64::MAX))
            .fold(0, u64::saturating_add);
        entry("section-other", String::from("Other BASE documents"), cases)
    });
    let options = page
        .options
        .iter()
        .map(|o| entry(&o.anchor, o.option.clone(), o.cases))
        .collect_view();
    let no_options = page
        .options
        .is_empty()
        .then(|| view! { <li class="text-ink-muted">"No option touches this folder."</li> });
    view! {
        <nav aria-label="On this page" class=format!("{CARD_PAD} mb-4")>
            <div class="grid grid-cols-1 gap-4 text-sm md:grid-cols-2">
                <div>
                    <h2 class=CARD_TITLE>
                        <a href="#base" class="hover:underline">
                            "BASE"
                        </a>
                    </h2>
                    <ul class="space-y-0.5">{sections} {others}</ul>
                </div>
                <div>
                    <h2 class=CARD_TITLE>
                        <a href="#options" class="hover:underline">
                            "OPTIONS"
                        </a>
                    </h2>
                    <ul class="space-y-0.5">{options} {no_options}</ul>
                </div>
            </div>
        </nav>
    }
    .into_any()
}

/// The BASE part: one anchored section per declared section.
fn base_view(page: &ChapterPage) -> AnyView {
    let folder = page.folder.clone();
    let sections = page
        .sections
        .iter()
        .map(|section| {
            let table = documents_table(&folder, &section.documents);
            view! {
                <section id=section.anchor.clone() class="mb-5">
                    <h3 class="mb-2 text-sm font-semibold text-ink-heading">
                        {section.title.clone()}
                        <span class="ml-2 font-normal text-ink-muted">
                            {cases_label(section.cases)}
                        </span>
                    </h3>
                    {table}
                </section>
            }
        })
        .collect_view();
    let others = (!page.unsectioned.is_empty()).then(|| {
        let table = documents_table(&folder, &page.unsectioned);
        view! {
            <section id="section-other" class="mb-5">
                <h3 class="mb-2 text-sm font-semibold text-ink-heading">"Other BASE documents"</h3>
                {table}
            </section>
        }
    });
    let empty = (page.sections.is_empty() && page.unsectioned.is_empty())
        .then(|| view! { <p class="mb-4 text-sm text-ink-muted">"No BASE case matches."</p> });
    view! {
        <h2 id="base" class="mb-3 text-lg font-semibold text-ink-heading">
            "BASE"
        </h2>
        {sections}
        {others}
        {empty}
    }
    .into_any()
}

/// One arm of an option, as a card listing its documents.
fn arm_view(folder: &str, arm: &ArmBlock) -> AnyView {
    let body = if arm.empty {
        view! { <p class="text-sm text-ink-muted">"nothing to run"</p> }.into_any()
    } else if arm.documents.is_empty() {
        view! { <p class="text-sm text-ink-muted">"No case of this arm in this folder."</p> }
            .into_any()
    } else {
        documents_table(folder, &arm.documents)
    };
    view! {
        <div class="min-w-0">
            <h4 class="mb-1 font-mono text-xs font-semibold text-ink-heading">{arm.arm.clone()}</h4>
            {body}
        </div>
    }
    .into_any()
}

/// The OPTIONS part: one block per option, its arms side by side.
fn options_view(page: &ChapterPage) -> AnyView {
    let folder = page.folder.clone();
    let blocks = page
        .options
        .iter()
        .map(|option| {
            let tier = option
                .tier
                .clone()
                .map(|tier| view! { <span class=BADGE>{tier}</span> });
            let theme = option
                .theme
                .clone()
                .map(|theme| view! { <span class=BADGE>{theme}</span> });
            let arms = option
                .arms
                .iter()
                .map(|arm| arm_view(&folder, arm))
                .collect_view();
            view! {
                <section id=option.anchor.clone() class=format!("{CARD_PAD} mb-4")>
                    <h3 class="text-sm font-semibold text-ink-heading">
                        <span class="font-mono">{option.option.clone()}</span>
                        {tier}
                        {theme}
                        <span class="ml-2 font-normal text-ink-muted">
                            {cases_label(option.cases)}
                        </span>
                    </h3>
                    <p class="mb-3 mt-1 text-sm text-ink-muted">{option.description.clone()}</p>
                    <div class="grid grid-cols-1 gap-3 lg:grid-cols-2">{arms}</div>
                </section>
            }
        })
        .collect_view();
    let empty = page
        .options
        .is_empty()
        .then(|| view! { <p class="text-sm text-ink-muted">"No option touches this folder."</p> });
    view! {
        <h2 id="options" class="mb-3 mt-6 text-lg font-semibold text-ink-heading">
            "OPTIONS"
        </h2>
        {blocks}
        {empty}
    }
    .into_any()
}

/// The loaded chapter page: contents, BASE, OPTIONS.
fn chapter_page_view(page: &ChapterPage) -> AnyView {
    if page.cases() == 0 {
        return view! {
            <EmptyState
                icon=icondata_lu::LuSearchX
                message="No case matches"
                hint="Loosen the filter, or check the capability folder in the URL."
            />
        }
        .into_any();
    }
    let contents = contents_view(page);
    let base = base_view(page);
    let options = options_view(page);
    view! {
        {contents}
        {base}
        {options}
    }
    .into_any()
}

/// One capability folder: its BASE sections in declared order, then the
/// options touching it, over a URL-state filter.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Chapter() -> impl IntoView {
    let chapter = param("chapter");
    let q = query_q();
    // Reactive inputs in the SOURCE; the fetcher is untracked by design.
    let page = Resource::new(
        move || (chapter.get(), q.get()),
        |(chapter, q)| fetch_chapter_page(chapter, q),
    );

    view! {
        <Title text=move || format!("{} · Catalogue · Veredictum console", chapter.get()) />
        <PageHeader
            title=chapter
            subtitle="BASE behaviour section by section, then every option that touches this folder, one small isolated workflow per behaviour."
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
        // Transition keeps the old groups visible while a filter reloads —
        // no fallback flash on every submit.
        <Transition fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the chapter…"</p> }
        }>
            {move || Suspend::new(async move {
                match page.await {
                    Ok(page) => chapter_page_view(&page),
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
