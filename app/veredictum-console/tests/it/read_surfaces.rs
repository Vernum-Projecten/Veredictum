// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The #64 acceptance gates: the landing's counts are the catalogue listing's
//! own numbers, checked rather than eyeballed, and the catalogue-missing
//! state names the mounts it looked at.
//!
//! The capability, document and case-detail readers are pinned here too: they
//! are the whole of what S2 renders, they are component-free by
//! construction, and the committed catalogue is the only input they take.

use std::path::Path;

use veredictum_console::catalogue_api::{ArmPlace, CaseStanding, InstrumentView, OptionKind, read};
use veredictum_console::state::ConsoleState;

/// The repository root, two levels above this crate (#55).
fn repo_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The startup state over the committed catalogue, exactly as `main` builds
/// it — the env override keeps the test independent of the process cwd.
fn committed_state() -> ConsoleState {
    let root = repo_root().join("catalogue");
    let specs = repo_root().join("specs/openehr");
    let catalogue =
        veredictum::pipeline::listing::read(&root, Some(&specs)).map_err(|e| e.to_string());
    ConsoleState {
        root,
        specs,
        out: repo_root().join("out"),
        catalogue: std::sync::Arc::new(catalogue),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        jobs: veredictum_console::run_job::JobSlot::default(),
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    }
}

/// The worked example of the Arazzo design, a case the catalogue keeps.
const WORKED_CASE: &str = "ehr/create_ehr-same_ehr_twice-with_id";

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_landing_counts_are_the_listing_counts() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let listing = state
        .catalogue
        .as_ref()
        .as_ref()
        .map_err(|e| format!("the committed catalogue must load: {e}"))?;

    let expected = (
        u64::try_from(listing.workflows().count())?,
        u64::try_from(listing.documents.len())?,
        u64::try_from(listing.capabilities().len())?,
        u64::try_from(listing.findings.len())?,
    );

    match read::instrument_view(&state) {
        InstrumentView::Loaded(summary) => {
            assert_eq!(
                (
                    summary.cases,
                    summary.documents,
                    summary.capabilities,
                    summary.findings
                ),
                expected,
                "the landing shows numbers the listing does not carry"
            );
            assert!(summary.cases > 0, "the committed catalogue carries cases");
        }
        InstrumentView::Missing(missing) => {
            panic!(
                "the committed catalogue read as missing: {}",
                missing.reason
            );
        }
    }
    Ok(())
}

/// A state whose catalogue refused to load, carrying `reason` verbatim.
fn refused_state(reason: &str) -> ConsoleState {
    ConsoleState {
        root: "/nonexistent/catalogue".into(),
        specs: "/nonexistent/specs".into(),
        out: "/nonexistent/out".into(),
        catalogue: std::sync::Arc::new(Err(reason.to_owned())),
        draft: std::sync::Arc::new(std::sync::Mutex::new(
            veredictum_console::run_api::Drafts::new(),
        )),
        client_ip_header: None,
        jobs: veredictum_console::run_job::JobSlot::default(),
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
    }
}

#[test]
fn a_missing_catalogue_names_the_mounts_it_looked_at() {
    let state = refused_state("no such directory");
    match read::instrument_view(&state) {
        InstrumentView::Missing(missing) => {
            assert_eq!(missing.root, "/nonexistent/catalogue");
            assert_eq!(missing.specs, "/nonexistent/specs");
            assert_eq!(missing.reason, "no such directory");
        }
        InstrumentView::Loaded(_) => {
            panic!("a failed load must render as the missing state")
        }
    }
}

/// Every listed workflow lands in exactly one capability row, so the counts
/// add up to the landing's own case count and none is double-counted.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_capability_counts_partition_the_catalogue() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let rows = read::chapter_rows(&state)?;
    assert!(!rows.is_empty(), "the committed catalogue has capabilities");

    let mut keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
    let sorted = {
        let mut copy = keys.clone();
        copy.sort_unstable();
        copy
    };
    assert_eq!(keys, sorted, "the capability list is name-sorted");
    keys.dedup();
    assert_eq!(keys.len(), rows.len(), "a capability appears once");
    assert!(
        !keys.contains(&"_setup"),
        "the shared setups are not a capability"
    );

    let listing = state
        .catalogue
        .as_ref()
        .as_ref()
        .map_err(|e| format!("the committed catalogue must load: {e}"))?;
    let total: u64 = rows.iter().map(|row| row.cases).sum();
    assert_eq!(
        total,
        u64::try_from(listing.workflows().count())?,
        "the capability counts must partition the listed workflows"
    );
    assert!(
        rows.iter().any(|row| row.key == "ehr"),
        "the ehr capability is committed: {keys:?}"
    );
    Ok(())
}

/// A catalogue that did not load is the SAME verbatim refusal on every
/// reader, never an empty listing that reads as "the catalogue is fine and
/// carries nothing".
#[test]
fn every_reader_reports_the_load_refusal_verbatim() {
    let state = refused_state("catalogue/cases: no such directory");
    assert_eq!(
        read::chapter_rows(&state).err().as_deref(),
        Some("catalogue/cases: no such directory")
    );
    assert_eq!(
        read::band_rows(&state, "ehr", "").err().as_deref(),
        Some("catalogue/cases: no such directory")
    );
    assert_eq!(
        read::case_detail(&state, "any").err().as_deref(),
        Some("catalogue/cases: no such directory")
    );
}

/// A capability listing has one section per document, and every row carries
/// the document it was filed under.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_capability_listing_is_sectioned_by_document() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let bands = read::band_rows(&state, "ehr", "")?;
    assert!(!bands.is_empty(), "the ehr capability carries cases");
    for band in &bands {
        assert!(!band.cases.is_empty(), "a rendered section carries rows");
        for case in &band.cases {
            assert_eq!(
                case.band, band.band,
                "a row must carry the document it was filed under"
            );
            assert!(
                case.id.starts_with(&format!("{}/", band.band)),
                "{} is not a workflow of {}",
                case.id,
                band.band
            );
            assert!(!case.summary.is_empty(), "{} has no summary", case.id);
        }
    }
    // A capability the catalogue does not carry is an empty listing, not an
    // error: the screen renders "nothing here", and the catalogue is intact.
    assert!(read::band_rows(&state, "no-such-capability", "")?.is_empty());
    Ok(())
}

/// The id filter narrows rather than reshapes: it keeps a subset of the
/// unfiltered listing, and a substring no case carries keeps nothing.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_listing_filter_narrows_the_same_rows() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let all: Vec<String> = read::band_rows(&state, "ehr", "")?
        .into_iter()
        .flat_map(|band| band.cases)
        .map(|case| case.id)
        .collect();

    let by_query: Vec<String> = read::band_rows(&state, "ehr", "create_ehr")?
        .into_iter()
        .flat_map(|band| band.cases)
        .map(|case| case.id)
        .collect();
    assert!(!by_query.is_empty(), "create_ehr cases are committed");
    assert!(by_query.len() < all.len(), "the substring must narrow");
    assert!(
        by_query
            .iter()
            .all(|id| id.contains("create_ehr") && all.contains(id)),
        "the substring filter kept a row that does not carry it"
    );
    assert!(
        read::band_rows(&state, "ehr", "no-such-substring")?.is_empty(),
        "a substring no case carries keeps nothing"
    );
    Ok(())
}

/// The case detail is the workflow's own fields: its document, summary,
/// description, citation and steps.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_case_detail_is_the_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let detail = read::case_detail(&state, WORKED_CASE)?
        .ok_or("the committed catalogue carries the case")?;

    assert_eq!(detail.id, WORKED_CASE);
    assert_eq!(detail.chapter, "ehr");
    assert_eq!(detail.document, "ehr");
    assert_eq!(
        detail.summary,
        "An ehr_id already in use is refused on create"
    );
    assert!(!detail.description.is_empty());
    assert!(
        detail.citation.as_ref().is_some_and(|citation| citation
            .source
            .contains("409_EHR_with_id.yaml")
            && !citation.section.is_empty()
            && !citation.quote.is_empty()),
        "every expectation cites the section it comes from: {:?}",
        detail.citation
    );
    // The vendored ITS-REST tree records its upstream, so the source links to
    // the released file at the pinned commit.
    assert!(
        detail
            .citation
            .as_ref()
            .and_then(|c| c.link.as_deref())
            .is_some_and(|link| {
                link.starts_with("https://github.com/openEHR/specifications-ITS-REST/blob/")
                    && link.ends_with("/409_EHR_with_id.yaml")
            }),
        "the citation links upstream: {:?}",
        detail.citation
    );
    assert_eq!(detail.chapter_key.as_deref(), Some("ehr"));
    assert_eq!(detail.chapter_title.as_deref(), Some("EHR"));
    assert_eq!(detail.band_title.as_deref(), Some("EHR"));
    assert_eq!(
        detail.standing,
        CaseStanding::Base {
            section: Some(String::from("create-and-read")),
            title: Some(String::from("Create and read")),
        },
        "the case is BASE under its declared section"
    );
    assert!(
        detail.steps.len() >= 2,
        "the case creates twice: {:?}",
        detail.steps
    );
    assert!(
        detail
            .steps
            .iter()
            .all(|step| !step.id.is_empty() && !step.operation.is_empty()),
        "every step names its operation"
    );
    assert_eq!(detail.option, None, "the case is not option-gated");
    Ok(())
}

/// A workflow of a document an option arm names carries that arm.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn an_option_gated_case_names_its_arm() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let detail = read::case_detail(&state, "ehr_xml/get_ehr-xml")?
        .ok_or("the committed catalogue carries the case")?;
    assert_eq!(
        detail.option.as_deref(),
        Some("ehr-xml-read: ehr-xml-supported")
    );
    assert_eq!(
        detail.standing,
        CaseStanding::Option {
            arms: vec![ArmPlace {
                option: String::from("ehr-xml-read"),
                arm: String::from("ehr-xml-supported"),
                kind: Some(OptionKind {
                    label: String::from("Optional (MAY)"),
                    diverges: false,
                }),
                report: None,
            }],
        },
        "the case sits on its arm, labelled with the kind the structure file places the option in"
    );
    Ok(())
}

/// An id the catalogue does not carry is a legitimately absent page, never an
/// error the screen has to translate.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn an_unknown_case_id_is_absence_rather_than_failure() -> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    assert!(read::case_detail(&state, "no_document/no_workflow")?.is_none());
    assert!(read::case_detail(&state, "")?.is_none());
    Ok(())
}

/// The startup read takes every mount from the environment, and with none of
/// the variables set it falls back to the documented working-tree defaults —
/// never to an empty path, and never to a catalogue it silently invented.
///
/// A catalogue that is not there is a first-class state, so `load` still
/// answers: the landing renders the named-mount explanation rather than the
/// server refusing to start.
#[test]
fn the_startup_read_falls_back_to_the_documented_mounts() {
    use veredictum_console::posture::{POSTURE_ENV, Posture};
    use veredictum_console::state::{OUT_ENV, ROOT_ENV, SPECS_ENV};

    // The variables are process-wide and `set_var` is unsafe, which this crate
    // forbids outright, so the unset case is read rather than arranged. An
    // ambient value is somebody's real console configuration: skip loudly
    // instead of asserting against it.
    let set: Vec<&str> = [ROOT_ENV, SPECS_ENV, OUT_ENV, POSTURE_ENV]
        .into_iter()
        .filter(|name| std::env::var_os(name).is_some())
        .collect();
    if !set.is_empty() {
        eprintln!("SKIPPED({set:?} set in this environment): the defaults cannot be read");
        return;
    }

    let state = ConsoleState::load().expect("an unset posture is the local one, and it loads");
    assert_eq!(
        state.posture,
        Posture::Local,
        "an unset posture is the local one, which refuses no target"
    );
    assert_eq!(state.root, Path::new("catalogue"));
    assert_eq!(state.specs, Path::new("specs/openehr"));
    assert_eq!(state.out, Path::new("out"));
    // A fresh console holds nothing for ANY submitter (#389): the job map and
    // the drafts map are both empty, and the question is asked per visitor
    // because that is the only shape either one has now.
    let who = veredictum_console::submitter::Submitter::Unknown;
    assert!(
        state
            .jobs
            .latest_of(who, veredictum_console::run_job::Latest::Any)
            .is_ok_and(|run| run.is_none()),
        "a fresh console has no job"
    );
    assert!(
        state
            .draft
            .lock()
            .is_ok_and(|drafts| drafts.get(who).is_none()),
        "a fresh console has no run draft"
    );
    assert_eq!(
        state.client_ip_header, None,
        "no forwarded header is trusted until the operator names one"
    );
    // The suite's own working directory carries no `catalogue` tree, so the
    // load refuses and the refusal is the state — not a panic, not a default.
    match read::instrument_view(&state) {
        InstrumentView::Missing(missing) => {
            assert_eq!(missing.root, "catalogue");
            assert!(!missing.reason.is_empty());
        }
        InstrumentView::Loaded(summary) => {
            assert_eq!(summary.root, "catalogue");
        }
    }
}
/// Every section counts its rows, and every row is a summarised workflow of
/// the document it names.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
fn assert_sections_list_workflows(
    sections: &[veredictum_console::catalogue_api::SectionBlock],
) -> Result<(), Box<dyn std::error::Error>> {
    for section in sections {
        assert_eq!(
            section.cases,
            u64::try_from(section.rows.len())?,
            "a section's count is its rows"
        );
        for row in &section.rows {
            assert!(
                row.id.starts_with(&format!("{}/", row.band)),
                "{} is not a workflow of {}",
                row.id,
                row.band
            );
            assert!(!row.summary.is_empty(), "{} has no summary", row.id);
        }
    }
    Ok(())
}

/// A chapter page groups `composition` into its declared BASE sections, in
/// declared order, then the options of `capabilities.yaml` touching it; every
/// document of the folder is listed exactly once (#868).
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_chapter_page_groups_composition_into_base_and_options()
-> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let page = read::chapter_page(&state, "composition", "")?;
    assert_eq!(page.title.as_deref(), Some("COMPOSITION"));
    assert_eq!(page.chapter.as_deref(), Some("EHR"));
    let titles: Vec<&str> = page.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        titles.first().copied(),
        Some("Create"),
        "BASE opens with the first declared section: {titles:?}"
    );
    let declared = [
        "Create",
        "Read",
        "Update",
        "Delete",
        "Versioning and history",
        "Refusals",
    ];
    let mut order = titles.iter().map(|t| declared.iter().position(|d| d == t));
    assert!(
        order.all(|at| at.is_some()),
        "only declared sections: {titles:?}"
    );
    let positions: Vec<usize> = titles
        .iter()
        .filter_map(|t| declared.iter().position(|d| d == t))
        .collect();
    assert!(
        positions.windows(2).all(|w| w.first() < w.get(1)),
        "sections keep declared order: {titles:?}"
    );
    let create = page.sections.first().ok_or("a Create section")?;
    assert!(
        create.rows.iter().any(|r| r.band == "composition_create"),
        "composition_create is a Create document"
    );
    assert_sections_list_workflows(&page.sections)?;
    assert!(
        page.unsectioned.is_empty(),
        "every BASE document names a section"
    );

    let xml = page
        .options
        .iter()
        .find(|o| o.option == "composition-xml-read")
        .ok_or("the composition-xml-read option touches composition")?;
    assert!(!xml.description.is_empty());
    assert!(xml.kind.is_some() && xml.theme.is_some());
    let arms: Vec<&str> = xml.arms.iter().map(|a| a.arm.as_str()).collect();
    assert_eq!(
        arms,
        ["composition-xml-supported", "composition-xml-unsupported"]
    );
    assert!(
        xml.arms
            .iter()
            .flat_map(|a| a.rows.iter())
            .any(|r| r.band == "composition_read_xml")
    );

    let listing = state
        .catalogue
        .as_ref()
        .as_ref()
        .map_err(|e| format!("the committed catalogue must load: {e}"))?;
    let folder: Vec<&str> = listing
        .documents
        .iter()
        .filter(|d| d.capability == "composition" && !d.workflows.is_empty())
        .map(|d| d.id.as_str())
        .collect();
    let in_options: Vec<&str> = page
        .options
        .iter()
        .flat_map(|o| o.arms.iter())
        .flat_map(|a| a.rows.iter())
        .map(|r| r.band.as_str())
        .collect();
    for document in &folder {
        // The sections a document's cases are listed under: one at most.
        let listed = page
            .sections
            .iter()
            .filter(|s| s.rows.iter().any(|r| r.band == *document))
            .count();
        assert!(
            listed + usize::from(in_options.contains(document)) >= 1,
            "{document} is listed nowhere"
        );
        assert!(listed <= 1, "{document} is listed twice under BASE");
        assert!(
            !(listed == 1 && in_options.contains(document)),
            "{document} is both BASE and OPTION"
        );
    }
    Ok(())
}

/// The filter narrows every group of a chapter page alike, and a content
/// case carries the formalism its setup uploads (#870).
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_chapter_page_filters_across_groups_and_badges_content()
-> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let all = read::chapter_page(&state, "composition", "")?;
    let narrowed = read::chapter_page(&state, "composition", "xml")?;
    assert!(narrowed.cases() > 0 && narrowed.cases() < all.cases());
    let ids = narrowed.sections.iter().flat_map(|s| s.rows.iter()).chain(
        narrowed
            .options
            .iter()
            .flat_map(|o| o.arms.iter())
            .flat_map(|a| a.rows.iter()),
    );
    for case in ids {
        assert!(
            case.id.contains("xml"),
            "{} does not carry the filter",
            case.id
        );
    }

    let content = read::chapter_page(&state, "content", "")?;
    let badges: Vec<Option<&str>> = content
        .sections
        .iter()
        .flat_map(|s| s.rows.iter())
        .map(|c| c.formalism.as_deref())
        .collect();
    assert!(!badges.is_empty(), "content carries BASE cases");
    assert!(
        badges
            .iter()
            .all(|b| matches!(*b, Some("ADL 1.4" | "ADL 2"))),
        "every content case carries the formalism of the template it uploads"
    );
    for formalism in ["ADL 1.4", "ADL 2"] {
        assert!(
            badges.contains(&Some(formalism)),
            "the content chapter badges {formalism} cases"
        );
    }
    Ok(())
}

/// A chapter page lists every option in one group, Recommended, Optional,
/// then the spec-boundary ones naming their report, and a BASE case whose
/// citation names a report carries it.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_chapter_page_labels_each_option_and_marks_reported_cases()
-> Result<(), Box<dyn std::error::Error>> {
    let state = committed_state();
    let page = read::chapter_page(&state, "ehr_status", "")?;
    let flat = page
        .options
        .iter()
        .find(|o| o.option == "ehr-status-flat-write")
        .ok_or("the ehr-status-flat-write option touches ehr_status")?;
    assert_eq!(
        (flat.kind.as_ref().map(|k| k.diverges), flat.report),
        (Some(true), Some(352)),
        "the option exists only because the released text diverges"
    );
    let rank = |kind: Option<&OptionKind>| match kind {
        Some(k) if k.diverges => 2,
        Some(k) if k.label == "Recommended (SHOULD)" => 0,
        Some(k) if k.label == "Optional (MAY)" => 1,
        _ => 3,
    };
    let ranks: Vec<u8> = page.options.iter().map(|o| rank(o.kind.as_ref())).collect();
    assert!(
        ranks.windows(2).all(|w| w.first() <= w.get(1)),
        "Recommended, Optional, then the spec-boundary ones: {ranks:?}"
    );
    assert!(
        flat.arms
            .iter()
            .flat_map(|a| a.rows.iter())
            .all(|r| r.reports.contains(&352)),
        "every case of the option rests on its report, as the engine places it"
    );
    let reported: Vec<(&str, &[u64])> = page
        .sections
        .iter()
        .flat_map(|s| s.rows.iter())
        .filter(|r| !r.reports.is_empty())
        .map(|r| (r.id.as_str(), r.reports.as_slice()))
        .collect();
    assert!(
        reported.contains(&(
            "ehr_status_versioned/get_versioned_ehr_status-revision_history",
            &[661][..]
        )),
        "the revision-history case rests on #661: {reported:?}"
    );
    let detail = read::case_detail(
        &state,
        "ehr_status_versioned/get_versioned_ehr_status-revision_history",
    )?
    .ok_or("the committed catalogue carries the case")?;
    assert_eq!(detail.reports, vec![661]);
    Ok(())
}

/// One chapter of the home as `(key, title, [(folder, title)])`.
type HomeShape<'a> = (&'a str, &'a str, Vec<(&'a str, &'a str)>);

/// The catalogue home reads by chapter: the chapters and their folders in the
/// order `chapters.yaml` declares, each folder under its declared title, and
/// the counts partitioning the listing into BASE and option cases (#868).
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_catalogue_home_groups_folders_by_chapter() -> Result<(), Box<dyn std::error::Error>> {
    use veredictum::arazzo::structure::{CHAPTERS_FILE, Chapters};

    let state = committed_state();
    let groups = read::catalogue_home(&state)?;
    let declared = Chapters::load(&state.root.join(CHAPTERS_FILE))?;
    let listing = state
        .catalogue
        .as_ref()
        .as_ref()
        .map_err(|e| format!("the committed catalogue must load: {e}"))?;
    let listed = listing.capabilities();

    let expected: Vec<HomeShape<'_>> = declared
        .chapters
        .iter()
        .map(|chapter| {
            let folders: Vec<(&str, &str)> = chapter
                .folders
                .iter()
                .filter(|f| listed.contains_key(f.folder.as_str()))
                .map(|f| (f.folder.as_str(), f.title.as_str()))
                .collect();
            (chapter.chapter.as_str(), chapter.title.as_str(), folders)
        })
        .filter(|(_, _, folders)| !folders.is_empty())
        .collect();
    let actual: Vec<HomeShape<'_>> = groups
        .iter()
        .map(|group| {
            let folders = group
                .bands
                .iter()
                .map(|b| (b.folder.as_str(), b.title.as_str()))
                .collect();
            (group.key.as_str(), group.title.as_str(), folders)
        })
        .collect();
    assert_eq!(
        actual, expected,
        "the home keeps the declared chapter and folder order and titles, and every committed folder is declared"
    );
    assert_eq!(
        groups.first().map(|g| g.key.as_str()),
        Some("ehr"),
        "the home opens with the first declared chapter"
    );

    let mut total = 0_u64;
    for group in &groups {
        let sum: u64 = group.bands.iter().map(|b| b.cases).sum();
        assert_eq!(
            group.cases, sum,
            "{}: a chapter counts its folders",
            group.key
        );
        for band in &group.bands {
            assert_eq!(
                band.base + band.options,
                band.cases,
                "{}: BASE and option cases partition the folder",
                band.folder
            );
        }
        total += group.cases;
    }
    assert_eq!(
        total,
        u64::try_from(listing.workflows().count())?,
        "the chapters partition the listed workflows"
    );
    let composition = groups
        .iter()
        .flat_map(|g| g.bands.iter())
        .find(|b| b.folder == "composition")
        .ok_or("composition is a committed folder")?;
    assert_eq!(composition.title, "COMPOSITION");
    assert!(
        composition.base > 0 && composition.options > 0,
        "composition carries BASE cases and option cases: {composition:?}"
    );
    Ok(())
}
