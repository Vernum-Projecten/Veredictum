// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S2 — the catalogue explorer (#61, #64).
//!
//! Chapters → capability folders → BASE sections and options → one workflow
//! in full, read through the published lib's catalogue listing and structure
//! files. The filter lives in the URL, so a view is shareable, refresh-safe
//! and works without WASM loaded
//! (<https://book.leptos.dev/router/20_form.html>). Sections fold with native
//! `<details>`, which needs no script at all.

use leptos::prelude::{
    AddAnyAttr, AnyView, AriaAttributes, ClassAttribute, CollectView, ElementChild, Get,
    GlobalAttributes, IntoAny, IntoView, Memo, Resource, Suspend, Suspense, Transition, With,
    component, view,
};
use leptos_meta::Title;
use leptos_router::components::{A, Form};
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::catalogue_api::fns::{fetch_case_detail, fetch_catalogue_home, fetch_chapter_page};
use crate::catalogue_api::{
    ArmBlock, ArmPlace, BandCard, CaseDetail, CaseRow, CaseStanding, ChapterGroup, ChapterPage,
    CitationRow, OptionBlock, OptionKind,
};
use crate::components::data_table::{TABLE, TABLE_WRAP, TD, TH};
use crate::components::empty_state::EmptyState;
use crate::components::field::INPUT;
use crate::components::format_view::inline_error;
use crate::components::kind::{THEME, diverge_tag, divergence_note, kind_badge};
use crate::components::page_header::{Crumb, PageHeader};
use crate::components::surface::{CARD_PAD, CARD_TITLE, WELL};
use crate::spec_diverge::Divergence;

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

/// A case count as a reader says it.
fn cases_label(cases: u64) -> String {
    if cases == 1 {
        String::from("1 case")
    } else {
        format!("{cases} cases")
    }
}

/// The small badge classes beside a heading or a case id.
const BADGE: &str = "ml-1.5 rounded-control border border-edge-strong px-1 py-px font-sans text-[10px] font-medium uppercase tracking-wide text-ink-muted";

/// The anchor of a chapter on the catalogue home.
fn chapter_anchor(key: &str) -> String {
    format!("chapter-{key}")
}

/// One folder card on the catalogue home.
fn band_card(card: BandCard) -> AnyView {
    let href = format!("/catalogue/{}", card.folder);
    let split = format!("{} BASE · {} in options", card.base, card.options);
    view! {
        <A
            href=href
            attr:class="flex flex-col gap-1 rounded-card border border-edge bg-raised p-4 shadow-card transition-colors hover:border-accent"
        >
            <span class="flex items-baseline justify-between gap-3">
                <span class="font-semibold text-ink-heading">{card.title}</span>
                <span class="shrink-0 tabular-nums text-sm text-ink-muted">
                    {cases_label(card.cases)}
                </span>
            </span>
            <span class="font-mono text-xs text-ink-muted">{card.folder}</span>
            <span class="mt-1 tabular-nums text-xs text-ink-muted">{split}</span>
        </A>
    }
    .into_any()
}

/// One chapter on the catalogue home: its heading, then its folders.
///
/// A chapter spans as many columns of the home grid as it has folders, up
/// to three, so the one-folder chapters share a row instead of each taking
/// a full line.
fn chapter_group(group: ChapterGroup) -> AnyView {
    let (span, inner) = match group.bands.len() {
        0 | 1 => ("min-w-0", "grid grid-cols-1 gap-3"),
        2 => (
            "min-w-0 sm:col-span-2",
            "grid grid-cols-1 gap-3 sm:grid-cols-2",
        ),
        _ => (
            "min-w-0 col-span-full",
            "grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3",
        ),
    };
    let cards = group.bands.into_iter().map(band_card).collect_view();
    view! {
        <section id=chapter_anchor(&group.key) class=format!("{span} scroll-mt-4")>
            <h2 class="mb-3 flex items-baseline gap-2 border-b border-edge pb-1.5 text-base font-semibold text-ink-heading">
                {group.title}
                <span class="tabular-nums text-sm font-normal text-ink-muted">
                    {cases_label(group.cases)}
                </span>
            </h2>
            <div class=inner>{cards}</div>
        </section>
    }
    .into_any()
}

/// The catalogue home: every chapter in reading order, each folder a card.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Catalogue() -> impl IntoView {
    let home = Resource::new(|| (), |()| fetch_catalogue_home());

    view! {
        <Title text="Catalogue · Veredictum console" />
        <PageHeader
            title="Catalogue"
            subtitle="Every chapter and capability, every workflow, and the citations each expectation stands on."
        />
        <Suspense fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the catalogue…"</p> }
        }>
            {move || Suspend::new(async move {
                match home.await {
                    Ok(groups) => {
                        let chapters = groups.into_iter().map(chapter_group).collect_view();
                        view! {
                            <div class="grid grid-cols-1 gap-x-3 gap-y-8 sm:grid-cols-2 xl:grid-cols-3">
                                {chapters}
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

/// One case in a section or an arm: the summary first, the id beneath, with
/// the `@SpecDiverge` tag when its text rests on an upstream report.
fn case_item(folder: &str, row: &CaseRow) -> AnyView {
    let href = format!("/catalogue/{folder}/{}", row.id);
    let summary = if row.summary.is_empty() {
        row.id.clone()
    } else {
        row.summary.clone()
    };
    let formalism = row.formalism.clone();
    let id = row.id.clone();
    let tag = (!row.reports.is_empty())
        .then(|| view! { <span class="ml-1.5">{diverge_tag(&row.reports)}</span> });
    view! {
        <li>
            <A href=href attr:class="group/case block px-4 py-2 hover:bg-sunken">
                <span class="block text-sm text-ink group-hover/case:text-accent">
                    {summary.clone()}
                </span>
                <span class="mt-0.5 block font-mono [overflow-wrap:anywhere] text-[11px] text-ink-muted">
                    {id.clone()}
                    {formalism.clone().map(|label| view! { <span class=BADGE>{label}</span> })}
                    {tag}
                </span>
            </A>
        </li>
    }
    .into_any()
}

/// A list of cases.
fn case_list(folder: &str, rows: &[CaseRow]) -> AnyView {
    let items = rows.iter().map(|row| case_item(folder, row)).collect_view();
    view! { <ul class="divide-y divide-edge">{items}</ul> }.into_any()
}

/// The summary classes of a folding section.
const SUMMARY: &str = "flex min-h-11 cursor-pointer list-none items-center gap-2 rounded-card px-4 py-2 hover:bg-sunken [&::-webkit-details-marker]:hidden";

/// The folding card a section or an option renders in.
const FOLD: &str = "group mb-2 scroll-mt-4 rounded-card border border-edge bg-raised shadow-card";

/// The disclosure chevron, turned when its `<details>` is open.
///
/// It is drawn with two borders rather than a glyph: some platforms render a
/// triangle codepoint from a colour emoji font.
fn chevron() -> AnyView {
    view! {
        <span
            aria-hidden="true"
            class="mx-0.5 inline-block size-1.5 -rotate-45 border-r-2 border-b-2 border-ink-muted transition-transform group-open:rotate-45"
        ></span>
    }
    .into_any()
}

/// One BASE section, folded unless the filter is active.
fn section_fold(
    folder: &str,
    anchor: String,
    title: String,
    rows: &[CaseRow],
    open: bool,
) -> AnyView {
    let cases = cases_label(u64::try_from(rows.len()).unwrap_or(u64::MAX));
    let list = case_list(folder, rows);
    view! {
        <details id=anchor open=open class=FOLD>
            <summary class=SUMMARY>
                {chevron()} <span class="font-medium text-ink-heading">{title}</span>
                <span class="ml-auto tabular-nums text-sm text-ink-muted">{cases}</span>
            </summary>
            <div class="border-t border-edge">{list}</div>
        </details>
    }
    .into_any()
}

/// What an empty options part says: under a filter, nothing matched; with
/// none, no option names a document of the folder.
const fn no_options_copy(filtering: bool) -> &'static str {
    if filtering {
        "No option case matches."
    } else {
        "No option touches this folder."
    }
}

/// The table of contents: every section and option with its case count,
/// each option with its kind.
fn contents_view(page: &ChapterPage, filtering: bool) -> AnyView {
    let entry = |anchor: &str, label: String, mono: bool, tag: Option<AnyView>, cases: u64| {
        let class = if mono {
            "font-mono text-xs text-accent hover:underline"
        } else {
            "text-accent hover:underline"
        };
        view! {
            <li class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                <a href=format!("#{anchor}") class=class>
                    {label}
                </a>
                {tag}
                <span class="shrink-0 tabular-nums text-xs text-ink-muted">{cases}</span>
            </li>
        }
    };
    let sections = page
        .sections
        .iter()
        .map(|s| entry(&s.anchor, s.title.clone(), false, None, s.cases))
        .collect_view();
    let others = (!page.unsectioned.is_empty()).then(|| {
        let cases = u64::try_from(page.unsectioned.len()).unwrap_or(u64::MAX);
        entry(
            "section-other",
            String::from("Other BASE cases"),
            false,
            None,
            cases,
        )
    });
    let no_base = (page.sections.is_empty() && page.unsectioned.is_empty())
        .then(|| view! { <li class="text-ink-muted">"No BASE case matches."</li> });
    let options = page
        .options
        .iter()
        .map(|o| {
            let kind = option_kind(o);
            entry(&o.anchor, o.option.clone(), true, kind, o.cases)
        })
        .collect_view();
    let no_options = page
        .options
        .is_empty()
        .then(|| view! { <li class="text-ink-muted">{no_options_copy(filtering)}</li> });
    view! {
        <nav aria-label="On this page" class=format!("{CARD_PAD} mb-6")>
            <div class="grid grid-cols-1 gap-x-8 gap-y-4 text-sm md:grid-cols-2">
                <div>
                    <h2 class=CARD_TITLE>
                        <a href="#base" class="hover:underline">
                            "BASE"
                        </a>
                    </h2>
                    <ul class="space-y-0.5">{sections} {others} {no_base}</ul>
                </div>
                <div>
                    <h2 class=CARD_TITLE>
                        <a href="#options" class="hover:underline">
                            "Options"
                        </a>
                    </h2>
                    <ul class="space-y-0.5">{options} {no_options}</ul>
                </div>
            </div>
        </nav>
    }
    .into_any()
}

/// The BASE part: one folding section per declared section.
fn base_view(page: &ChapterPage, open: bool) -> AnyView {
    let folder = page.folder.clone();
    let sections = page
        .sections
        .iter()
        .map(|section| {
            section_fold(
                &folder,
                section.anchor.clone(),
                section.title.clone(),
                &section.rows,
                open,
            )
        })
        .collect_view();
    let others = (!page.unsectioned.is_empty()).then(|| {
        section_fold(
            &folder,
            String::from("section-other"),
            String::from("Other BASE cases"),
            &page.unsectioned,
            open,
        )
    });
    let empty = (page.sections.is_empty() && page.unsectioned.is_empty())
        .then(|| view! { <p class="mb-4 text-sm text-ink-muted">"No BASE case matches."</p> });
    view! {
        <section class="mb-8">
            <h2 id="base" class="scroll-mt-4 text-lg font-semibold text-ink-heading">
                "BASE"
            </h2>
            <p class="mb-3 text-sm text-ink-muted">
                "Every claimant of this capability runs these, section by section."
            </p>
            {sections}
            {others}
            {empty}
        </section>
    }
    .into_any()
}

/// One arm of an option: its token, then its cases as a compact list.
fn arm_view(folder: &str, arm: &ArmBlock) -> AnyView {
    let body = if arm.empty {
        view! { <p class="px-4 py-2 text-sm text-ink-muted">"nothing to run"</p> }.into_any()
    } else if arm.rows.is_empty() {
        view! { <p class="px-4 py-2 text-sm text-ink-muted">"No case of this arm in this folder."</p> }
        .into_any()
    } else {
        case_list(folder, &arm.rows)
    };
    let count = (!arm.rows.is_empty()).then(|| {
        view! {
            <span class="ml-auto font-sans font-normal tabular-nums text-ink-muted">
                {cases_label(u64::try_from(arm.rows.len()).unwrap_or(u64::MAX))}
            </span>
        }
    });
    view! {
        <div class="min-w-0 overflow-hidden rounded-card border border-edge">
            <h4 class="flex items-baseline gap-2 border-b border-edge bg-sunken px-4 py-1.5 font-mono text-xs font-semibold text-ink-heading">
                {arm.arm.clone()} {count}
            </h4>
            {body}
        </div>
    }
    .into_any()
}

/// The kind badge of an option, the `@SpecDiverge` tag on a spec-boundary
/// one; nothing when the structure file places it nowhere.
fn option_kind(option: &OptionBlock) -> Option<AnyView> {
    option
        .kind
        .as_ref()
        .map(|kind| kind_badge(&kind.label, kind.diverges, option.report))
}

/// One option, folded unless the filter is active: its name, its kind and
/// its theme, then its arms side by side.
fn option_fold(folder: &str, option: &OptionBlock, open: bool) -> AnyView {
    let kind = option_kind(option);
    let theme = option
        .theme
        .clone()
        .map(|theme| view! { <span class=THEME>{theme}</span> });
    let arms = option
        .arms
        .iter()
        .map(|arm| arm_view(folder, arm))
        .collect_view();
    view! {
        <details id=option.anchor.clone() open=open class=FOLD>
            <summary class=SUMMARY>
                {chevron()}
                <span class="font-mono text-sm font-medium text-ink-heading">
                    {option.option.clone()}
                </span> {kind} {theme}
                <span class="ml-auto shrink-0 tabular-nums text-sm text-ink-muted">
                    {cases_label(option.cases)}
                </span>
            </summary>
            <div class="border-t border-edge px-4 py-3">
                <p class="mb-3 text-sm text-ink">{option.description.clone()}</p>
                <div class="grid grid-cols-1 items-start gap-3 lg:grid-cols-2">{arms}</div>
            </div>
        </details>
    }
    .into_any()
}

/// The options part: every option touching the folder, Recommended,
/// Optional, then the spec-boundary ones, each folding with its kind.
fn options_view(page: &ChapterPage, open: bool) -> AnyView {
    let folder = page.folder.clone();
    let blocks = page
        .options
        .iter()
        .map(|o| option_fold(&folder, o, open))
        .collect_view();
    let empty = page
        .options
        .is_empty()
        .then(|| view! { <p class="text-sm text-ink-muted">{no_options_copy(open)}</p> });
    view! {
        <section>
            <h2 id="options" class="scroll-mt-4 text-lg font-semibold text-ink-heading">
                "Options"
            </h2>
            <p class="mb-3 text-sm text-ink-muted">
                "A statement selects one arm of each; each option is labelled with its kind."
            </p>
            {blocks}
            {empty}
        </section>
    }
    .into_any()
}

/// The loaded chapter page: contents, BASE, options. Every fold opens while
/// a filter is active, so each match is in sight.
fn chapter_page_view(page: &ChapterPage, q: &str) -> AnyView {
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
    let open = !q.is_empty();
    let matched = open.then(|| {
        let line = format!("{} match “{q}”.", cases_label(page.cases()));
        view! {
            <p class="mb-4 text-sm text-ink-muted">
                {line} " "
                <A
                    href=format!("/catalogue/{}", page.folder)
                    attr:class="text-accent hover:underline"
                >
                    "Clear the filter"
                </A>
            </p>
        }
    });
    let contents = contents_view(page, open);
    let base = base_view(page, open);
    let options = options_view(page, open);
    view! {
        {matched}
        {contents}
        {base}
        {options}
        {divergence_note(&divergence(page))}
    }
    .into_any()
}

/// The chapter page's header: the band title with its chapter as context.
fn chapter_header(page: &ChapterPage) -> AnyView {
    let mut crumbs = vec![Crumb::new("Catalogue", "/catalogue")];
    if let (Some(title), Some(key)) = (&page.chapter, &page.chapter_key) {
        crumbs.push(Crumb::new(
            title.clone(),
            format!("/catalogue#{}", chapter_anchor(key)),
        ));
    }
    let title = page.title.clone().unwrap_or_else(|| page.folder.clone());
    view! {
        <PageHeader
            title=title
            tag=page.folder.clone()
            subtitle="BASE behaviour section by section, then every option that touches this folder, one small isolated workflow per behaviour."
            crumbs=crumbs
        />
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
        <Transition fallback=move || {
            view! {
                <PageHeader title=chapter crumbs=vec![Crumb::new("Catalogue", "/catalogue")] />
            }
        }>
            {move || Suspend::new(async move {
                match page.await {
                    Ok(page) => chapter_header(&page),
                    Err(_) => {
                        view! {
                            <PageHeader
                                title=chapter.get()
                                crumbs=vec![Crumb::new("Catalogue", "/catalogue")]
                            />
                        }
                            .into_any()
                    }
                }
            })}
        </Transition>
        <div class="mb-4 flex flex-wrap items-center gap-3">
            <Form method="GET" action="">
                <div class="flex items-center gap-2">
                    <input
                        type="search"
                        name="q"
                        value=move || q.get()
                        placeholder="Filter by case id…"
                        aria-label="Filter by case id"
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
                let filter = q.get();
                match page.await {
                    Ok(page) => chapter_page_view(&page, &filter),
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
                match detail.await {
                    Ok(Some(case)) => case_view(case).into_any(),
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

/// One labeled fact of the placement card.
fn fact(label: &'static str, body: AnyView) -> AnyView {
    view! {
        <div class="grid grid-cols-[8rem_1fr] items-baseline gap-3">
            <dt class="text-xs font-semibold uppercase tracking-wide text-ink-muted">{label}</dt>
            <dd class="min-w-0 text-sm text-ink">{body}</dd>
        </div>
    }
    .into_any()
}

/// The option arms a case sits on, each with its option's kind.
fn option_standing(folder: &str, arms: &[ArmPlace]) -> AnyView {
    let items = arms
        .iter()
        .map(|arm| {
            let kind = arm.kind.as_ref().map_or_else(
                || {
                    view! { <span class="text-xs text-ink-muted">"No kind placed"</span> }
                        .into_any()
                },
                |OptionKind { label, diverges }| kind_badge(label, *diverges, arm.report),
            );
            let option = arm.option.clone();
            view! {
                <li>
                    <A
                        href=format!("/catalogue/{folder}#option-{option}")
                        attr:class="font-mono text-xs text-accent hover:underline"
                    >
                        {option.clone()}
                    </A>
                    <span class="text-ink-muted">" arm "</span>
                    <span class="font-mono text-xs">{arm.arm.clone()}</span>
                    <span class="mt-1 block">{kind}</span>
                </li>
            }
        })
        .collect_view();
    fact(
        "Option",
        view! {
            <ul class="space-y-1.5">{items}</ul>
            <span class="mt-1 block text-xs text-ink-muted">
                "It runs only when a statement selects one of these arms."
            </span>
        }
        .into_any(),
    )
}

/// The divergence fact of a case whose cited text an upstream report
/// records; nothing when none does.
fn reports_fact(case: &CaseDetail) -> Option<AnyView> {
    (!case.reports.is_empty()).then(|| {
        fact(
            "Divergence",
            view! {
                {diverge_tag(&case.reports)}
                <span class="mt-1 block text-xs text-ink-muted">
                    "It rests on released text that is unclear or contradictory, judged as the upstream report describes."
                </span>
            }
            .into_any(),
        )
    })
}

/// The tagged cases a chapter page lists, each case once.
fn divergence(page: &ChapterPage) -> Divergence {
    let mut found = Divergence::default();
    let options = page
        .options
        .iter()
        .flat_map(|o| o.arms.iter())
        .flat_map(|a| a.rows.iter());
    let base = page.sections.iter().flat_map(|s| s.rows.iter());
    for row in base.chain(page.unsectioned.iter()).chain(options) {
        found.add(&row.id, &row.reports);
    }
    found
}

/// Where the case sits: chapter, capability, and BASE section or option arms.
fn placement_view(case: &CaseDetail) -> AnyView {
    let folder = case.chapter.clone();
    let chapter = case.chapter_title.clone().map_or_else(
        || view! { <span class="text-ink-muted">"No chapter declares it"</span> }.into_any(),
        |title| {
            let href = case.chapter_key.as_deref().map_or_else(
                || String::from("/catalogue"),
                |key| format!("/catalogue#{}", chapter_anchor(key)),
            );
            view! {
                <A href=href attr:class="text-accent hover:underline">
                    {title}
                </A>
            }
            .into_any()
        },
    );
    let band_title = case.band_title.clone().unwrap_or_else(|| folder.clone());
    let band = view! {
        <A href=format!("/catalogue/{folder}") attr:class="text-accent hover:underline">
            {band_title.clone()}
        </A>
        <span class="ml-2 font-mono text-xs text-ink-muted">{folder.clone()}</span>
    }
    .into_any();
    let standing = match &case.standing {
        CaseStanding::Base { section, title } => {
            let section = match (section, title) {
                (Some(key), Some(title)) => {
                    let title = title.clone();
                    view! {
                        " · "
                        <A
                            href=format!("/catalogue/{folder}#section-{key}")
                            attr:class="text-accent hover:underline"
                        >
                            {title.clone()}
                        </A>
                    }
                    .into_any()
                }
                _ => view! { <span class="text-ink-muted">" · no declared section"</span> }
                    .into_any(),
            };
            fact(
                "Standing",
                view! {
                    <span class="font-medium">"BASE"</span>
                    {section}
                    <span class="block text-xs text-ink-muted">
                        "Every claimant of the capability runs it."
                    </span>
                }
                .into_any(),
            )
        }
        CaseStanding::Option { arms } => option_standing(&folder, arms),
    };
    view! {
        <section class=CARD_PAD>
            <h2 class=CARD_TITLE>"Where it sits"</h2>
            <dl class="space-y-2.5">
                {fact("Chapter", chapter)} {fact("Capability", band)} {standing}
                {reports_fact(case)}
            </dl>
        </section>
    }
    .into_any()
}

/// A file path that may wrap after each `/`, never inside a file name.
fn path_breaks(path: &str) -> AnyView {
    path.split_inclusive('/')
        .map(|segment| {
            let segment = segment.to_owned();
            view! {
                {segment}
                <wbr />
            }
        })
        .collect_view()
        .into_any()
}

/// The citation card body: the source, linked upstream when known, its
/// section, and the quoted sentence.
fn citation_view(citation: Option<CitationRow>) -> AnyView {
    let Some(citation) = citation else {
        return view! {
            <p class="text-sm text-ink-muted">
                "No workflow-level citation: each criterion carries its own."
            </p>
        }
        .into_any();
    };
    let source = citation.link.map_or_else(
        || {
            view! {
                <span class="break-words font-mono text-xs text-ink">
                    {path_breaks(&citation.source)}
                </span>
            }
            .into_any()
        },
        |link| {
            view! {
                <a
                    href=link
                    rel="external noopener"
                    target="_blank"
                    class="break-words font-mono text-xs text-accent hover:underline"
                >
                    {path_breaks(&citation.source)}
                </a>
            }
            .into_any()
        },
    );
    view! {
        <div class=WELL>
            <div>{source}</div>
            <div class="mt-1 text-xs text-ink-muted">{citation.section}</div>
            <blockquote class="mt-2 border-l-2 border-edge-strong pl-3 text-sm text-ink">
                {citation.quote}
            </blockquote>
        </div>
    }
    .into_any()
}

/// The loaded case's sections — plain assembly, erased per section
/// (rules §1).
fn case_view(case: CaseDetail) -> impl IntoView + use<> {
    let mut crumbs = vec![Crumb::new("Catalogue", "/catalogue")];
    if let (Some(title), Some(key)) = (&case.chapter_title, &case.chapter_key) {
        crumbs.push(Crumb::new(
            title.clone(),
            format!("/catalogue#{}", chapter_anchor(key)),
        ));
    }
    crumbs.push(Crumb::new(
        case.band_title
            .clone()
            .unwrap_or_else(|| case.chapter.clone()),
        format!("/catalogue/{}", case.chapter),
    ));
    let placement = placement_view(&case);
    let citation = citation_view(case.citation);
    let title = if case.summary.is_empty() {
        case.id.clone()
    } else {
        case.summary
    };
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
        <PageHeader title=title tag=case.id crumbs=crumbs />
        <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
            <section class=format!("{CARD_PAD} lg:col-span-2")>
                <h2 class=CARD_TITLE>"Description"</h2>
                <p class="whitespace-pre-wrap text-sm text-ink">{case.description}</p>
                <div class="mt-3 flex flex-wrap items-center gap-3 text-sm text-ink-muted">
                    <span>"document: " <span class="font-mono text-xs">{case.document}</span></span>
                </div>
            </section>
            {placement}
            <section class=CARD_PAD>
                <h2 class=CARD_TITLE>"Spec citation"</h2>
                {citation}
                <p class="mt-2 text-sm text-ink-muted">
                    "An expectation is refuted by a better reading of the cited text, and by nothing else."
                </p>
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
