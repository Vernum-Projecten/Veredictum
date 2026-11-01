// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Every seeded-defect fixture is rejected with the expected gate — the
//! validator's negative battery over the Arazzo catalogue, one fixture per
//! gate.
//!
//! **One test per defect.** nextest runs each test in its own process, so the
//! battery fans out across the whole process pool, and a red row names
//! exactly one fixture instead of aborting a loop. The fixture set and the
//! per-defect tests are generated from ONE list by `defects!`, so they cannot
//! drift apart; [`every_defect_fixture_has_a_test`] then compares that list
//! against the fixture directory, so a fixture added on disk with no test is a
//! failing check, never a silent skip.
//!
//! **The world each defect validates against is materialized, not copied.**
//! [`overlay_world`] symlinks the pristine catalogue and materializes real
//! directories only along the overlaid file's own path, so the gates see a
//! byte-identical tree with one file replaced or added. Nothing in the
//! battery writes to the tree, and the overlaid name is never symlinked, so
//! the pristine catalogue can never be written through a link.

#![expect(
    clippy::expect_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken fixture must abort the test loudly, Book ch11"
)]

use std::path::{Path, PathBuf};

use veredictum::validate::validate;

/// The seeded-defect battery: one row per fixture, and one `#[test]` per row.
///
/// Each row is `<test name> => (<fixture file>, <overlay destination inside
/// the world>, <the gate that must reject it>)`. The gate is spelled with the
/// stable token `veredictum::validate::CheckId::token` publishes — that enum
/// is the vocabulary these strings come from.
macro_rules! defects {
    ($($test:ident => ($fixture:literal, $dest:literal, $gate:literal)),+ $(,)?) => {
        /// (fixture file, overlay destination inside the world, expected gate)
        const DEFECTS: &[(&str, &str, &str)] = &[$(($fixture, $dest, $gate)),+];

        $(
            #[test]
            fn $test() {
                assert_rejected($fixture, $dest, $gate);
            }
        )+
    };
}

defects! {
    // The Arazzo catalogue's gates, each refusing one seeded document, rows
    // file or stray file through the same `validate` pass.
    arazzo_wrong_version =>
        ("arazzo-wrong-version.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-schema"),
    arazzo_unknown_key =>
        ("arazzo-unknown-key.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-keys"),
    arazzo_undeclared_header =>
        ("arazzo-undeclared-header.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-source"),
    arazzo_missing_citation =>
        ("arazzo-missing-citation.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-citation"),
    arazzo_level_mismatch =>
        ("arazzo-level-mismatch.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-level"),
    arazzo_level_unmarked =>
        ("arazzo-level-unmarked.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-level"),
    arazzo_simple_string =>
        ("arazzo-simple-string.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-profile"),
    arazzo_json_type_unknown =>
        ("arazzo-json-type-unknown.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-profile"),
    arazzo_rows_unknown_input =>
        ("arazzo-rows-unknown-input.rows.yaml", "cases/ehr/create_ehr-with_id.rows.yaml", "arazzo-rows"),
    arazzo_rows_bad_literal =>
        ("arazzo-rows-bad-literal.rows.yaml", "cases/content/commit-dv_count_out_of_range.rows.yaml", "arazzo-rows"),
    arazzo_stale_gap =>
        ("arazzo-stale-gap.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-surface"),
    arazzo_stray_file =>
        ("arazzo-stray-file.yaml", "cases/zz-notes.yaml", "arazzo-load"),
    arazzo_aql_unparsable =>
        ("arazzo-aql-unparsable.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-aql"),
    // A document that sends canonical JSON without stating the format.
    arazzo_json_format_unstated =>
        ("arazzo-json-format-unstated.arazzo.yaml", "cases/system/zz-json-unstated.arazzo.yaml", "arazzo-identity"),
    // The claim gates: a capability folder below its committed floor, a
    // folder verified only over extension routes and not marked so, a
    // statement claiming a folder in which nothing it declares can run, and
    // a statement selecting an arm whose documents need an unclaimed format.
    arazzo_floor_below =>
        ("arazzo-floor-below.arazzo.yaml", "cases/system/system_options.arazzo.yaml", "arazzo-floor"),
    arazzo_extension_only_unmarked =>
        ("arazzo-extension-only-unmarked.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-realization-scope"),
    // A released case resting on a setup that cites development text alone.
    arazzo_development_setup =>
        ("arazzo-development-setup.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-development-setup"),
    // The reading structure: a folder in no chapter or in two, a BASE
    // document with no section or an unknown one, a section nothing uses,
    // an OPTION document with a section, an option with no theme, a spec
    // boundary naming no report, and a report snapshot out of step with the
    // citations. A citation into RETIRED text is the citation gate's.
    structure_folder_in_no_chapter =>
        ("structure-folder-in-no-chapter.arazzo.yaml", "cases/zz/zz-defect.arazzo.yaml", "arazzo-structure"),
    structure_folder_twice =>
        ("structure-folder-twice.chapters.yaml", "chapters.yaml", "arazzo-structure"),
    structure_base_without_section =>
        ("structure-base-without-section.arazzo.yaml", "cases/system/system_options.arazzo.yaml", "arazzo-structure"),
    structure_unknown_section =>
        ("structure-unknown-section.arazzo.yaml", "cases/system/system_options.arazzo.yaml", "arazzo-structure"),
    structure_unused_section =>
        ("structure-unused-section.chapters.yaml", "chapters.yaml", "arazzo-structure"),
    structure_option_with_section =>
        ("structure-option-with-section.arazzo.yaml", "cases/security/version_signature.arazzo.yaml", "arazzo-structure"),
    structure_option_unplaced =>
        ("structure-option-unplaced.chapters.yaml", "chapters.yaml", "arazzo-structure"),
    structure_boundary_without_report =>
        ("structure-boundary-without-report.chapters.yaml", "chapters.yaml", "arazzo-structure"),
    structure_report_missing =>
        ("structure-report-missing.reports.yaml", "reports.yaml", "arazzo-structure"),
    structure_report_uncited =>
        ("structure-report-uncited.reports.yaml", "reports.yaml", "arazzo-structure"),
    citation_retired =>
        ("citation-retired.arazzo.yaml", "cases/system/system_options.arazzo.yaml", "arazzo-citation"),
    statement_claim_incomplete =>
        ("statement-claim-incomplete.json", "--statement", "claim-completeness"),
    statement_arm_format_unclaimed =>
        ("statement-arm-format-unclaimed.json", "--statement", "arm-format"),
}

/// The destination a statement fixture names: it is not overlaid on the
/// catalogue, and the pristine catalogue is validated with it as the
/// supplied statement.
const STATEMENT: &str = "--statement";

/// The repository root, two levels above this package (#55): the catalogue
/// and the vendored specs live there, never inside the crate.
fn crate_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The seeded-defect fixture directory, which moved WITH the crate.
fn fixture_dir() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/defects"
    ))
}

/// The vendored spec tree the citation-resolution gates read.
fn spec_root() -> PathBuf {
    crate_dir().join("specs/openehr")
}

/// Build the world one defect validates against: the pristine catalogue, with
/// `dest` replaced (or added) from `fixture`. The world sits one level under
/// a temporary root whose `specs` links the vendored tree, because a
/// document names its OAS sources relative to the repository root.
///
/// Every entry the overlaid path does not pass through is a symlink to the
/// pristine tree; only the directories along `dest` are real, and the overlaid
/// name itself is never linked, so the copy at the end always creates a fresh
/// file and can never write through into the committed tree.
fn overlay_world(dest: &str, fixture: &str) -> assert_fs::TempDir {
    let temp = assert_fs::TempDir::new().expect("temp dir");
    std::os::unix::fs::symlink(crate_dir().join("specs"), temp.path().join("specs"))
        .expect("symlink the specs");
    let mut source = crate_dir().join("catalogue");
    let mut target = temp.path().join("catalogue");

    for component in Path::new(dest) {
        std::fs::create_dir_all(&target).expect("mkdir");
        // A destination directory the pristine tree lacks (`cases/zz/`,
        // where the Arazzo defects land) has nothing to link beside the overlay.
        let siblings: Vec<std::io::Result<std::fs::DirEntry>> = if source.is_dir() {
            std::fs::read_dir(&source).expect("read_dir").collect()
        } else {
            Vec::new()
        };
        for entry in siblings {
            let entry = entry.expect("entry");
            if entry.file_name() == component {
                continue; // the overlaid path: materialized, never linked
            }
            std::os::unix::fs::symlink(entry.path(), target.join(entry.file_name()))
                .expect("symlink");
        }
        source = source.join(component);
        target = target.join(component);
    }

    assert!(
        !target.exists(),
        "{dest}: the overlay destination must not exist yet"
    );
    std::fs::copy(fixture_dir().join(fixture), &target).expect("overlay");
    temp
}

/// Load the world holding `fixture` at `dest`, or the pristine catalogue
/// with `fixture` as the statement when `dest` is [`STATEMENT`], and assert
/// the `expected` gate rejected it.
fn assert_rejected(fixture: &str, dest: &str, expected: &str) {
    let specs = spec_root();
    let findings = if dest == STATEMENT {
        let statement = fixture_dir().join(fixture);
        validate(
            &crate_dir().join("catalogue"),
            Some(&specs),
            Some(&statement),
        )
    } else {
        let world = overlay_world(dest, fixture);
        validate(&world.path().join("catalogue"), Some(&specs), None)
    }
    .expect("the catalogue lists")
    .findings;

    // The clean catalogue produces zero findings (`validate.rs`'s own test),
    // so any finding at all is attributable to the seeded defect; the
    // specific-gate assertion pins WHICH gate caught it.
    assert!(
        !findings.is_empty(),
        "{fixture}: defect produced no findings at all"
    );
    assert!(
        findings.iter().any(|f| f.check.token() == expected),
        "{fixture}: expected a `{expected}` finding, got:\n{}",
        findings
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Every fixture on disk has a test — a fixture nobody exercises is a silent
/// hole in the negative battery, so it fails here rather than passing unseen.
#[test]
fn every_defect_fixture_has_a_test() {
    let mut on_disk: Vec<String> = std::fs::read_dir(fixture_dir())
        .expect("defects dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut in_table: Vec<String> = DEFECTS.iter().map(|(f, _, _)| (*f).to_owned()).collect();
    in_table.sort();
    in_table.dedup();
    assert_eq!(
        on_disk, in_table,
        "defect fixtures and the expectation table diverge"
    );
}
