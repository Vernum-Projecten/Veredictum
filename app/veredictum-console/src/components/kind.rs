// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The option kind badge, the `@SpecDiverge` tag and the closing divergence
//! note, one look on every surface that reads a result or the catalogue.
//!
//! An option carries its kind beside its name: Recommended (SHOULD) or
//! Optional (MAY) as a badge, and an option of the spec-boundary tier the
//! `@SpecDiverge` tag in that place. A case resting on reported text carries
//! the same quiet tag, and the surface closes with one muted note counting
//! the tagged cases and linking each report.

use leptos::prelude::{
    AnyView, ClassAttribute, CollectView, ElementChild, GlobalAttributes, IntoAny, view,
};

use crate::spec_diverge::{Divergence, TAG, issue_href, tag_title};

/// The kind badge: Recommended (SHOULD), Optional (MAY).
const KIND: &str = "inline-flex shrink-0 items-center rounded-control border border-edge-strong bg-sunken px-1.5 py-px font-sans text-[11px] font-medium text-ink";

/// The theme label: quieter than the kind badge, no frame.
pub const THEME: &str =
    "shrink-0 font-sans text-[10px] font-normal uppercase tracking-wide text-ink-faint";

/// The `@SpecDiverge` tag: quiet and monospace, like the theme label.
const TAG_CLASS: &str = "shrink-0 font-mono text-[10px] font-normal text-ink-faint";

/// The `@SpecDiverge` tag over `reports`, its hover text naming them.
#[must_use]
pub fn diverge_tag(reports: &[u64]) -> AnyView {
    view! {
        <span class=TAG_CLASS title=tag_title(reports)>
            {TAG}
        </span>
    }
    .into_any()
}

/// The kind badge of an option, or for a spec-boundary option the
/// `@SpecDiverge` tag over its report.
#[must_use]
pub fn kind_badge(kind: &str, diverge: bool, report: Option<u64>) -> AnyView {
    if diverge {
        let reports: Vec<u64> = report.into_iter().collect();
        diverge_tag(&reports)
    } else {
        view! { <span class=KIND>{kind.to_owned()}</span> }.into_any()
    }
}

/// The closing note over the tagged cases of `found`; nothing when none is.
#[must_use]
pub fn divergence_note(found: &Divergence) -> Option<AnyView> {
    let cases = found.count();
    if cases == 0 {
        return None;
    }
    let lead = if cases == 1 {
        String::from(
            "1 case rests on released text that is unclear or contradictory, and is tagged ",
        )
    } else {
        format!(
            "{cases} cases rest on released text that is unclear or contradictory, and are tagged "
        )
    };
    let last = found.reports.len().saturating_sub(1);
    let links = found
        .reports
        .iter()
        .enumerate()
        .map(|(at, report)| {
            let after = if at == last { "." } else { ", " };
            view! {
                <a href=issue_href(*report) rel="external" class="text-accent hover:underline">
                    {format!("#{report}")}
                </a>
                {after}
            }
        })
        .collect_view();
    Some(
        view! {
            <p id="spec-divergence" class="mt-8 max-w-3xl text-xs text-ink-muted">
                "Spec divergence: "
                {lead}
                <span class="font-mono">{TAG}</span>
                ". Until the specification resolves it, Veredictum judges them as the linked upstream reports describe: "
                {links}
            </p>
        }
        .into_any(),
    )
}
