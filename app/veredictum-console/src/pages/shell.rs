// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The persistent chrome (#61 §Look and feel).
//!
//! The static sidebar (seal, one entry per surface, the versions in the
//! footer) and the instrument status banner around the routed `<Outlet/>`. The chrome renders exactly once
//! outside any Suspense: a `Suspend` closure re-runs on every notification of
//! the resources it awaits and re-creates everything inside it, so a resource
//! owned there gets a different id on the server than on the client and
//! hydration reads the wrong serialized slot. Dark mode is re-applied after
//! hydration inside an `Effect` for the same reason
//! (<https://book.leptos.dev/ssr/24_hydration_bugs.html>).

use std::time::Duration;

use leptos::prelude::{
    AddAnyAttr, AriaAttributes, Children, ClassAttribute, CollectView, Effect, ElementChild, Get,
    GlobalAttributes, IntoAny, IntoView, OnAttribute, Resource, RwSignal, Set, Suspend, Suspense,
    Update, component, document, on_cleanup, set_timeout, view, window_event_listener,
};
use leptos_icons::Icon;
use leptos_router::components::{A, Outlet};
use leptos_router::hooks::use_location;

use crate::components::toast::{self, ToastHost};
use crate::status_api::fns::fetch_status_line;
use crate::theme;

/// One sidebar entry.
struct NavEntry {
    key: &'static str,
    label: &'static str,
    icon: icondata_core::Icon,
}

/// The three surfaces (#61): the run pipeline lives under one entry.
fn nav_entries() -> [NavEntry; 3] {
    [
        NavEntry {
            key: "/",
            label: "Instrument",
            icon: icondata_lu::LuGauge,
        },
        NavEntry {
            key: "/catalogue",
            label: "Catalogue",
            icon: icondata_lu::LuLibrary,
        },
        NavEntry {
            key: "/run",
            label: "Run",
            icon: icondata_lu::LuPlay,
        },
    ]
}

/// Maps a full URL path to the top-level nav key it belongs under, so the
/// sidebar highlights the active surface.
fn nav_key(path: &str) -> &'static str {
    if path.starts_with("/catalogue") {
        "/catalogue"
    } else if path.starts_with("/run") {
        "/run"
    } else {
        "/"
    }
}

/// The application shell: the chrome around the routed `<Outlet/>`.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Shell() -> impl IntoView {
    view! {
        <Chrome>
            <Outlet />
        </Chrome>
    }
}

/// Stamps `data-hydrated` on `<body>`: the marker every browser journey
/// waits on before it drives a control.
fn stamp_hydrated() {
    if let Some(body) = document().body() {
        let outcome = body.set_attribute("data-hydrated", "");
        // NOTE: no openEHR spec governs this — our own design; a failed
        // attribute write on a live document has nothing to recover from.
        drop(outcome);
    }
}

/// Sets the hydration marker once every hydrated listener is attached.
///
/// The ordering comes from the deferral, not from where the effect is
/// created: `<Outlet/>` renders the routed page lazily, and a `Suspend` whose
/// data is still streaming hydrates later from a task of its own, so neither
/// is guaranteed to have attached its listeners when an effect first runs.
/// The stamp therefore waits for the document's `load` event, which fires
/// only once the whole streamed response has arrived, and then one timer
/// turn, after the tasks that data woke
/// (<https://html.spec.whatwg.org/multipage/parsing.html#the-end>).
fn stamp_when_settled() {
    let stamp = || set_timeout(stamp_hydrated, Duration::ZERO);
    if document().ready_state() == "complete" {
        stamp();
    } else {
        let handle = window_event_listener(leptos::ev::load, move |_| stamp());
        on_cleanup(move || handle.remove());
    }
}

/// The chrome itself: sidebar, toast host, and whatever content it frames.
///
/// Taking the content as `children` rather than rendering `<Outlet/>` itself
/// is what lets the routes' 404 fallback wear the same chrome (#84): the
/// fallback renders OUTSIDE the route tree, so it has no outlet to fill.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Chrome(
    /// The framed content — the routed `<Outlet/>`, or the 404 answer.
    children: Children,
) -> impl IntoView {
    toast::provide();
    let location = use_location();
    let nav_open = RwSignal::new(false);
    let dark = RwSignal::new(false);

    // Browser-only: re-apply the persisted theme after hydration. Reads the
    // outside world (localStorage), so an Effect is the correct home
    // (rules §2); the server pass and the first client paint stay light.
    Effect::new(move |_| {
        if let Some(stored) = theme::stored_dark() {
            dark.set(stored);
            theme::apply_dark(stored);
        }
    });

    let sidebar = move || {
        let active = nav_key(&location.pathname.get());
        nav_entries()
            .into_iter()
            .map(|entry| {
                let class = if entry.key == active {
                    "flex items-center gap-2.5 rounded-control bg-accent-subtle px-3 py-2 text-sm font-medium text-accent-ink"
                } else {
                    "flex items-center gap-2.5 rounded-control px-3 py-2 text-sm font-medium text-ink-muted hover:bg-sunken hover:text-ink"
                };
                view! {
                    <li>
                        <A href=entry.key attr:class=class on:click=move |_| nav_open.set(false)>
                            <Icon icon=entry.icon width="16" height="16" />
                            {entry.label}
                        </A>
                    </li>
                }
            })
            .collect_view()
    };

    let chrome = view! {
        <div class="flex min-h-screen bg-surface">
            // The scrim behind the mobile drawer; clicking it closes.
            <div
                class="fixed inset-0 z-30 bg-scrim lg:hidden"
                class:hidden=move || !nav_open.get()
                on:click=move |_| nav_open.set(false)
            ></div>
            <aside
                class="fixed inset-y-0 left-0 z-40 flex w-60 -translate-x-full flex-col border-r border-edge bg-raised transition-transform lg:static lg:translate-x-0"
                class:translate-x-0=move || nav_open.get()
            >
                <div class="flex items-center gap-2.5 px-4 py-4">
                    <img src="/seal.svg" alt="" class="h-8 w-8" />
                    <span class="text-base font-semibold tracking-tight text-ink-heading">
                        "Veredictum"
                    </span>
                </div>
                <nav aria-label="Primary" class="flex-1 overflow-y-auto px-3">
                    <ul class="flex flex-col gap-1">{sidebar}</ul>
                </nav>
                <div class="border-t border-edge px-4 py-3 text-xs text-ink-faint">
                    <div class="flex items-center justify-between">
                        <span class="tabular-nums">{format!("engine {}", crate::ENGINE_PIN)}</span>
                        <button
                            class="rounded-control px-2 py-1 text-ink-muted hover:bg-sunken hover:text-ink"
                            aria-label="Toggle dark mode"
                            on:click=move |_| {
                                let next = !dark.get();
                                dark.set(next);
                                theme::apply_dark(next);
                                theme::persist_dark(next);
                            }
                        >
                            {move || if dark.get() { "light" } else { "dark" }}
                        </button>
                    </div>
                </div>
            </aside>
            <div class="flex min-w-0 flex-1 flex-col">
                <StatusBanner />
                <header class="flex items-center gap-3 border-b border-edge bg-raised px-4 py-2 lg:hidden">
                    <button
                        class="rounded-control border border-edge-strong px-2 py-1 text-sm text-ink"
                        aria-label="Open navigation"
                        on:click=move |_| nav_open.update(|open| *open = !*open)
                    >
                        "☰"
                    </button>
                    <span class="text-sm font-semibold text-ink-heading">"Veredictum"</span>
                </header>
                <main class="min-w-0 flex-1 px-6 py-6 lg:px-8">{children()}</main>
            </div>
            <ToastHost />
        </div>
    };
    // The deferral inside orders the stamp after every hydrated listener
    // (#377); the 404 fallback wears this chrome too, so it carries the
    // marker as well.
    Effect::new(move |_| stamp_when_settled());
    chrome
}

/// The instrument status banner: the linked engine's approval status, read
/// through a server fn so the server pass and the hydrated client render the
/// same text.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn StatusBanner() -> impl IntoView {
    // Created in setup, never inside a Suspend (rules §4); the Suspend renders
    // only the resolved text.
    let status = Resource::new(|| (), |()| fetch_status_line());
    view! {
        <p
            id="instrument-status"
            class="border-b border-edge bg-sunken px-4 py-1.5 text-xs text-ink-muted lg:px-8"
        >
            <Suspense fallback=|| ()>
                {move || Suspend::new(async move {
                    match status.await {
                        Ok(line) => {
                            let notice = line
                                .issuance
                                .notice()
                                .map(|notice| {
                                    view! {
                                        " · "
                                        <span id="trial-mode" class="font-medium text-ink">
                                            {notice}
                                        </span>
                                    }
                                });
                            view! {
                                <span class="font-medium text-ink">{line.token}</span>
                                ": "
                                {line.summary}
                                {notice}
                            }
                                .into_any()
                        }
                        Err(e) => {
                            view! { <span>{format!("instrument status unavailable: {e}")}</span> }
                                .into_any()
                        }
                    }
                })}
            </Suspense>
        </p>
    }
}

#[cfg(test)]
mod tests {
    use super::nav_key;

    #[test]
    fn every_path_maps_to_exactly_one_surface() {
        assert_eq!(nav_key("/"), "/");
        assert_eq!(nav_key("/catalogue"), "/catalogue");
        assert_eq!(
            nav_key("/catalogue/ehr/I_EHR_SERVICE.create_ehr-main"),
            "/catalogue"
        );
        assert_eq!(nav_key("/run/connect"), "/run");
        assert_eq!(nav_key("/run/results"), "/run");
        assert_eq!(nav_key("/nonsense"), "/");
    }
}
