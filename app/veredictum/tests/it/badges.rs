// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The static badges of an Arazzo results document: every badge pinned as SVG
//! and as a Shields.io endpoint document for a passing, a failing and a
//! development-profile result; a failed or errored row never drawn in a
//! passing colour; a warning counted on its own badge, outside the ratio and
//! out of the colour; the full count breakdown in the title; `dev` in every
//! counting badge of a development result; no separator character in any
//! label or message; and the command writing the files and drawing the perf,
//! noise and instrument badges for a `perf-result`.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;
use veredictum::arazzo::results::{Engine, Explanation, ResultRow, Results, Status, Whose};
use veredictum::arazzo::rows::Row;
use veredictum::arazzo::spec_profile::{SnapshotComponent, Specification};
use veredictum::arazzo::structure::{
    ArmRef, ChapterDecl, FolderDecl, Outline, OutlineArm, OutlineOption, Place, SectionDecl,
    Standing, Theme, Tier,
};
use veredictum::badge::{Badge, BadgeColour, BadgeName, badges, perf_badges};
use veredictum::party::InstrumentStatus;
use veredictum::perf_result::PerfResult;
use veredictum::perf_result::capacity::Knee;
use veredictum::perf_result::compare::{AaFloor, CompareMetric};
use veredictum::perf_result::derive::BetweenRunNoise;
use veredictum::pipeline::export::badge_files;

fn row(case: &str, index: usize, status: Status, whose: Option<Whose>) -> ResultRow {
    ResultRow {
        case: case.to_owned(),
        row: index,
        status,
        inputs: Row::new(),
        generated: Vec::new(),
        skip: None,
        explanation: whose.map(|whose| Explanation {
            step: "read".to_owned(),
            what: "Read the EHR.".to_owned(),
            why: None,
            expected: "status 200".to_owned(),
            received: "status 500".to_owned(),
            exchange: None,
            whose,
        }),
    }
}

fn place(standing: Standing) -> Place {
    Place {
        chapter: "ehr".to_owned(),
        folder: "ehr".to_owned(),
        standing,
        trial: false,
        reports: Vec::new(),
        formalism: None,
    }
}

fn on_arm(option: &str, arm: &str) -> Place {
    place(Standing::Option {
        arms: vec![ArmRef {
            option: option.to_owned(),
            arm: arm.to_owned(),
        }],
    })
}

fn option(name: &str, selected: &str, unselected: &str) -> OutlineOption {
    OutlineOption {
        option: name.to_owned(),
        description: format!("Whether the server supports {name}."),
        theme: Theme::CanonicalXml,
        tier: Tier::Options,
        report: None,
        folders: vec!["ehr".to_owned()],
        arms: vec![
            OutlineArm {
                arm: selected.to_owned(),
                selected: true,
                documents: vec![selected.to_owned()],
            },
            OutlineArm {
                arm: unselected.to_owned(),
                selected: false,
                documents: vec![unselected.to_owned()],
            },
        ],
    }
}

/// BASE cases `ehr/create` and `ehr/read`, a case on the selected arm
/// `xml-supported` and one on the unselected arm `flat-supported`.
fn outline() -> Outline {
    let base = || {
        place(Standing::Base {
            section: "create".to_owned(),
        })
    };
    Outline {
        chapters: vec![ChapterDecl {
            chapter: "ehr".to_owned(),
            title: "EHR".to_owned(),
            folders: vec![FolderDecl {
                folder: "ehr".to_owned(),
                title: "EHR".to_owned(),
                formalism: false,
                sections: vec![SectionDecl {
                    section: "create".to_owned(),
                    title: "Create".to_owned(),
                }],
            }],
        }],
        options: vec![
            option("ehr-xml", "xml-supported", "xml-unsupported"),
            option("ehr-flat", "flat-unsupported", "flat-supported"),
        ],
        reports: Vec::new(),
        cases: BTreeMap::from([
            ("ehr/create".to_owned(), base()),
            ("ehr/read".to_owned(), base()),
            (
                "xml-supported/read".to_owned(),
                on_arm("ehr-xml", "xml-supported"),
            ),
            (
                "flat-supported/read".to_owned(),
                on_arm("ehr-flat", "flat-supported"),
            ),
        ]),
    }
}

fn results(rows: Vec<ResultRow>, specification: Specification) -> Results {
    Results {
        document: "catalogue".to_owned(),
        engine: Engine {
            name: "veredictum".to_owned(),
            version: "0.3.1".to_owned(),
        },
        instrument_status: InstrumentStatus::PreApproval,
        specification,
        base_url: "http://sut/openehr/v1".to_owned(),
        server: None,
        review: Vec::new(),
        rows,
        outline: Some(outline()),
    }
}

/// Every BASE and selected-arm row passed, one BASE row a warning, and the
/// unselected arm skipped.
fn passing_rows() -> Vec<ResultRow> {
    vec![
        row("ehr/create", 0, Status::Passed, None),
        row("ehr/create", 1, Status::Passed, None),
        row("ehr/read", 0, Status::Failed, Some(Whose::Warning)),
        row("ehr/read", 1, Status::Passed, None),
        row("xml-supported/read", 0, Status::Passed, None),
        row("flat-supported/read", 0, Status::Skipped, None),
    ]
}

fn passing() -> Results {
    results(passing_rows(), Specification::Released)
}

/// A failed and an errored BASE row, and an errored selected-arm row.
fn failing() -> Results {
    results(
        vec![
            row("ehr/create", 0, Status::Passed, None),
            row("ehr/create", 1, Status::Failed, Some(Whose::Server)),
            row("ehr/read", 0, Status::Errored, Some(Whose::Instrument)),
            row("ehr/read", 1, Status::Passed, None),
            row(
                "xml-supported/read",
                0,
                Status::Errored,
                Some(Whose::Instrument),
            ),
            row("flat-supported/read", 0, Status::Skipped, None),
        ],
        Specification::Released,
    )
}

/// The passing rows, judged against the development snapshot.
fn development() -> Results {
    let component = |name: &str| SnapshotComponent {
        component: name.to_owned(),
        repository: format!("https://github.com/openEHR/specifications-{name}"),
        branch: "master".to_owned(),
        commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
    };
    results(
        passing_rows(),
        Specification::Development {
            snapshot: vec![component("BASE"), component("RM")],
        },
    )
}

fn badge(results: &Results, name: BadgeName) -> Option<Badge> {
    badges(results).into_iter().find(|b| b.name == name)
}

#[test]
fn every_badge_is_pinned_for_a_passing_a_failing_and_a_development_result() {
    for (kind, results) in [
        ("passing", passing()),
        ("failing", failing()),
        ("development", development()),
    ] {
        let drawn = badges(&results);
        assert_eq!(
            drawn.iter().map(|b| b.name).collect::<Vec<_>>(),
            BadgeName::CONFORMANCE.to_vec()
        );
        assert_eq!(drawn, badges(&results), "byte-deterministic");
        for badge in drawn {
            let name = badge.name.token();
            insta::assert_snapshot!(format!("{kind}-{name}.svg"), badge.svg());
            insta::assert_snapshot!(
                format!("{kind}-{name}.json"),
                badge.endpoint_json().unwrap()
            );
        }
    }
}

/// The documents the badges above are drawn from, pinned as the files
/// `scripts/render/book-badges.sh` hands to `veredictum badge` for the book's
/// gallery, so the gallery shows these fixtures and nothing hand-made.
#[test]
fn the_badge_fixtures_are_pinned_as_documents() {
    for (kind, results) in [
        ("passing", passing()),
        ("failing", failing()),
        ("development", development()),
    ] {
        insta::assert_snapshot!(format!("{kind}-results.json"), results.to_text().unwrap());
    }
    for (kind, record) in perf_records().unwrap() {
        insta::assert_snapshot!(format!("perf-{kind}-record.json"), record.render().unwrap());
    }
}

#[test]
fn a_failed_or_errored_row_is_never_drawn_in_a_passing_colour() {
    assert_eq!(
        badge(&passing(), BadgeName::Base).unwrap().colour,
        BadgeColour::Passing
    );
    assert_eq!(
        badge(&failing(), BadgeName::Base).unwrap().colour,
        BadgeColour::Failing
    );
    let mut errored_only = failing();
    errored_only.rows.retain(|r| r.status != Status::Failed);
    assert_eq!(
        badge(&errored_only, BadgeName::Base).unwrap().colour,
        BadgeColour::Errored
    );
    assert_eq!(
        badge(&failing(), BadgeName::Options).unwrap().colour,
        BadgeColour::Errored
    );
    assert_eq!(
        badge(&passing(), BadgeName::Options).unwrap().message,
        "1/1 passed",
        "the unselected arm's skipped row is left out"
    );
    assert_eq!(
        badge(&passing(), BadgeName::Base).unwrap().message,
        "3/3 passed, 1 warning",
        "the warning row is named after the ratio, never inside it"
    );
}

#[test]
fn a_base_warning_leaves_the_ratio_all_passed_and_the_badge_green() {
    let base = badge(&passing(), BadgeName::Base).unwrap();
    assert_eq!(base.message, "3/3 passed, 1 warning");
    assert_eq!(base.colour, BadgeColour::Passing);
    let mut unwarned = passing();
    unwarned.rows.retain(|r| r.explanation.is_none());
    let plain = badge(&unwarned, BadgeName::Base).unwrap();
    assert_eq!(plain.message, "3/3 passed");
    assert_eq!(plain.colour, base.colour, "a warning changes no colour");
}

#[test]
fn an_option_warning_is_counted_on_the_options_badge_only() {
    let mut results = passing();
    results.rows.retain(|r| r.explanation.is_none());
    results.rows.push(row(
        "xml-supported/read",
        1,
        Status::Failed,
        Some(Whose::Warning),
    ));
    let options = badge(&results, BadgeName::Options).unwrap();
    assert_eq!(options.message, "1/1 passed, 1 warning");
    assert_eq!(options.colour, BadgeColour::Passing);
    assert_eq!(
        badge(&results, BadgeName::Base).unwrap().message,
        "3/3 passed"
    );
}

#[test]
fn warnings_in_base_and_options_are_counted_on_each_badge_apart() {
    let mut results = failing();
    for (case, index) in [("ehr/read", 2), ("ehr/read", 3), ("xml-supported/read", 1)] {
        results
            .rows
            .push(row(case, index, Status::Failed, Some(Whose::Warning)));
    }
    let base = badge(&results, BadgeName::Base).unwrap();
    assert_eq!(base.message, "2/4 passed, 2 warnings");
    assert_eq!(base.detail, "2/4 passed, 1 failed, 1 errored, 2 warnings");
    assert_eq!(base.colour, BadgeColour::Failing);
    let options = badge(&results, BadgeName::Options).unwrap();
    assert_eq!(options.message, "0/1 passed, 1 warning");
    assert_eq!(options.detail, "0/1 passed, 1 errored, 1 warning");
    assert_eq!(options.colour, BadgeColour::Errored);
}

#[test]
fn a_server_attributed_errored_row_makes_the_base_badge_red() {
    for whose in [Some(Whose::Server), None] {
        let mut results = passing();
        results
            .rows
            .push(row("ehr/create", 2, Status::Errored, whose));
        let base = badge(&results, BadgeName::Base).unwrap();
        assert_eq!(base.colour, BadgeColour::Failing, "{whose:?}");
        assert_eq!(base.message, "3/4 passed, 1 warning", "{whose:?}");
        assert_eq!(base.detail, "3/4 passed, 1 errored, 1 warning", "{whose:?}");
    }
    let mut inconclusive = passing();
    inconclusive.rows.push(row(
        "ehr/create",
        2,
        Status::Errored,
        Some(Whose::Instrument),
    ));
    assert_eq!(
        badge(&inconclusive, BadgeName::Base).unwrap().colour,
        BadgeColour::Errored,
        "an inconclusive row alone stays orange"
    );
}

#[test]
fn a_red_row_the_outline_places_nowhere_never_leaves_the_base_badge_green() {
    for (status, whose, colour) in [
        (Status::Failed, Whose::Server, BadgeColour::Failing),
        (Status::Errored, Whose::Instrument, BadgeColour::Errored),
    ] {
        let mut results = passing();
        results
            .rows
            .push(row("stray/unplaced", 0, status, Some(whose)));
        let base = badge(&results, BadgeName::Base).unwrap();
        assert_eq!(base.colour, colour, "{status:?}");
        assert_eq!(
            base.message.split(' ').next(),
            Some("3/4"),
            "{}",
            base.message
        );
    }
    let mut passed = passing();
    passed
        .rows
        .push(row("stray/unplaced", 0, Status::Passed, None));
    assert_eq!(
        badge(&passed, BadgeName::Base).unwrap().colour,
        BadgeColour::Passing
    );
}

#[test]
fn the_title_carries_the_full_count_breakdown() {
    let base = badge(&failing(), BadgeName::Base).unwrap();
    assert_eq!(base.message, "2/4 passed");
    let full = "openEHR BASE: 2/4 passed, 1 failed, 1 errored";
    assert_eq!(base.title(), full);
    let svg = base.svg();
    assert!(svg.contains(&format!(r#"aria-label="{full}""#)), "{svg}");
    assert!(svg.contains(&format!("<title>{full}</title>")), "{svg}");
    let json: Value = serde_json::from_str(&base.endpoint_json().unwrap()).unwrap();
    assert_eq!(
        json["message"], "2/4 passed",
        "the endpoint keeps the message"
    );
}

#[test]
fn every_counting_badge_of_a_development_result_says_dev() {
    let mut warned_options = development();
    warned_options.rows.push(row(
        "xml-supported/read",
        1,
        Status::Failed,
        Some(Whose::Warning),
    ));
    let mut unplaced = development();
    unplaced.outline = None;
    for results in [development(), warned_options, unplaced] {
        let drawn = badges(&results);
        let counting: Vec<&Badge> = drawn
            .iter()
            .filter(|b| matches!(b.name, BadgeName::Base | BadgeName::Options))
            .collect();
        assert_eq!(counting.len(), 2);
        for badge in counting {
            assert!(badge.label.ends_with(" dev"), "{badge:?}");
        }
    }
    for results in [passing(), failing()] {
        for badge in badges(&results) {
            assert!(!badge.label.contains("dev"), "{badge:?}");
        }
    }
}

#[test]
fn no_label_or_message_carries_a_separator() {
    let mut drawn = Vec::new();
    for results in [passing(), failing(), development()] {
        drawn.extend(badges(&results));
    }
    for (_, record) in perf_records().unwrap() {
        drawn.extend(perf_badges(&record));
    }
    for badge in drawn {
        for text in [&badge.label, &badge.message, &badge.detail] {
            assert!(!text.contains('\u{b7}'), "{badge:?}");
            assert!(!text.contains('|'), "{badge:?}");
        }
    }
}

#[test]
fn every_counting_badge_names_its_profile_and_the_profiles_never_draw_alike() {
    for name in [BadgeName::Base, BadgeName::Options, BadgeName::Profile] {
        let released = badge(&passing(), name).unwrap();
        let development = badge(&development(), name).unwrap();
        assert_ne!(released.svg(), development.svg(), "{}", name.token());
        assert_ne!(
            released.endpoint_json().unwrap(),
            development.endpoint_json().unwrap(),
            "{}",
            name.token()
        );
        if name != BadgeName::Profile {
            assert!(!released.label.ends_with(" dev"), "{released:?}");
            assert_eq!(development.label, format!("{} dev", released.label));
        }
    }
    assert_eq!(
        badge(&development(), BadgeName::Profile).unwrap().message,
        "development (master)"
    );
}

#[test]
fn a_record_without_the_specification_member_reads_as_released() {
    let mut document: Value = serde_json::from_str(&passing().to_text().unwrap()).unwrap();
    document
        .as_object_mut()
        .unwrap()
        .remove("specification")
        .unwrap();
    let read: Results = serde_json::from_value(document).unwrap();
    assert_eq!(badges(&read), badges(&passing()));
}

#[test]
fn the_endpoint_document_follows_the_shields_schema_with_the_icon() {
    let json: Value = serde_json::from_str(
        &badge(&failing(), BadgeName::Base)
            .unwrap()
            .endpoint_json()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["schemaVersion"], 1);
    assert_eq!(json["label"], "openEHR BASE");
    assert_eq!(json["color"], "e05d44");
    let logo = json["logoSvg"].as_str().unwrap();
    assert!(logo.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(logo.contains("stroke=\"#fff\""));
    assert!(logo.contains("M18.5 27 L29 45.5 L46.5 17.5"));
    assert_eq!(
        json.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["schemaVersion", "label", "message", "color", "logoSvg"]
    );
}

fn badge_command(results: &Path, out: &Path) -> std::io::Result<std::process::Output> {
    std::process::Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .arg("badge")
        .arg("--results")
        .arg(results)
        .arg("--out")
        .arg(out)
        .output()
}

#[test]
fn the_command_writes_every_badge_the_seam_draws() {
    let dir = assert_fs::TempDir::new().unwrap();
    let path = dir.path().join("results.json");
    std::fs::write(&path, development().to_text().unwrap()).unwrap();
    let out = dir.path().join("badges");
    let output = badge_command(&path, &out).unwrap();
    assert!(output.status.success(), "{output:?}");
    let files = badge_files(&path).unwrap();
    assert_eq!(files.len(), 8);
    for file in &files {
        assert_eq!(
            std::fs::read_to_string(out.join(&file.name)).unwrap(),
            file.body,
            "{}",
            file.name
        );
    }
}

fn perf_example() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/perf-result.example.json")
}

/// The committed example, whose knee is a bracket, whose workload is custom
/// and which carries no A/A floor, and the same record read as a lower bound
/// of a pinned workload with a floor attached. Both name the fixtures'
/// instrument version, 0.3.1, so a release that moves the example's version
/// leaves the pinned badges alone.
fn perf_records() -> Result<[(&'static str, PerfResult); 2], Box<dyn std::error::Error>> {
    let mut bracket = PerfResult::read(&std::fs::read_to_string(perf_example())?)?;
    "0.3.1".clone_into(&mut bracket.raw.instrument.version);
    let mut lower = bracket.clone();
    lower.derived.knee = Knee::LowerBound {
        qualified_rate: 1_250.0,
        achieved_per_s: 1_249.5,
    };
    "ehr-mix@2".clone_into(&mut lower.raw.workload.label);
    lower.derived.between_run_noise = BetweenRunNoise::AaFloor(AaFloor {
        ratios: BTreeMap::from([(CompareMetric::P95, 1.04), (CompareMetric::Achieved, 1.01)]),
        pairs: 12,
    });
    Ok([("bracket", bracket), ("lower-bound", lower)])
}

fn perf_badge(record: &PerfResult, name: BadgeName) -> Option<Badge> {
    perf_badges(record).into_iter().find(|b| b.name == name)
}

#[test]
fn the_perf_badges_are_pinned_for_a_bracket_and_a_lower_bound() {
    for (kind, record) in perf_records().unwrap() {
        let drawn = perf_badges(&record);
        assert_eq!(
            drawn.iter().map(|b| b.name).collect::<Vec<_>>(),
            BadgeName::PERF.to_vec()
        );
        assert_eq!(drawn, perf_badges(&record), "byte-deterministic");
        for badge in drawn {
            let name = badge.name.token();
            insta::assert_snapshot!(format!("perf-{kind}-{name}.svg"), badge.svg());
            insta::assert_snapshot!(
                format!("perf-{kind}-{name}.json"),
                badge.endpoint_json().unwrap()
            );
        }
    }
    let [(_, bracket), (_, lower)] = perf_records().unwrap();
    let knee = perf_badge(&bracket, BadgeName::Perf).unwrap();
    assert_eq!(knee.label, "perf custom");
    assert_eq!(knee.message, "knee 180\u{2013}211/s", "rounded outward");
    assert_eq!(knee.detail, "knee 180.8\u{2013}210.9/s");
    assert_eq!(
        knee.colour,
        BadgeColour::Reading,
        "a knee is never pass or fail"
    );
    let knee = perf_badge(&lower, BadgeName::Perf).unwrap();
    assert_eq!(knee.label, "perf ehr-mix@2");
    assert_eq!(knee.message, "knee \u{2265} 1,250/s");
    assert_eq!(knee.colour, BadgeColour::Reading);
}

#[test]
fn the_noise_badge_says_whether_an_aa_floor_is_attached() {
    let [(_, bracket), (_, lower)] = perf_records().unwrap();
    let unmeasured = perf_badge(&bracket, BadgeName::Noise).unwrap();
    assert_eq!(
        (unmeasured.label.as_str(), unmeasured.message.as_str()),
        ("A/A floor", "not measured")
    );
    assert_eq!(unmeasured.colour, BadgeColour::Empty);
    let attached = perf_badge(&lower, BadgeName::Noise).unwrap();
    assert_eq!(attached.message, "attached");
    assert_eq!(attached.colour, BadgeColour::Reading);
}

#[test]
fn the_perf_instrument_badge_names_the_version_that_wrote_the_record() {
    let [(_, mut bracket), _] = perf_records().unwrap();
    "9.8.7".clone_into(&mut bracket.raw.instrument.version);
    let instrument = perf_badge(&bracket, BadgeName::Instrument).unwrap();
    assert_eq!(instrument.label, "veredictum");
    assert_eq!(instrument.message, "self-run v9.8.7");
    assert_eq!(instrument.colour, BadgeColour::Instrument);
    assert_eq!(
        badge(&passing(), BadgeName::Instrument).unwrap().message,
        "self-run v0.3.1"
    );
}

#[test]
fn the_command_draws_the_perf_noise_and_instrument_badges_for_a_perf_result() {
    let dir = assert_fs::TempDir::new().unwrap();
    let output = badge_command(&perf_example(), dir.path()).unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut written: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    written.sort();
    assert_eq!(
        written,
        [
            "instrument.json",
            "instrument.svg",
            "noise.json",
            "noise.svg",
            "perf.json",
            "perf.svg"
        ]
    );
    let example = PerfResult::read(&std::fs::read_to_string(perf_example()).unwrap()).unwrap();
    for badge in perf_badges(&example) {
        let name = badge.name.token();
        assert_eq!(
            std::fs::read_to_string(dir.path().join(format!("{name}.svg"))).unwrap(),
            badge.svg(),
            "{name}"
        );
    }
}
