// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The static badges of an Arazzo results document: every badge pinned as SVG
//! and as a Shields.io endpoint document for a passing, a failing and a
//! development-profile result; a failed or errored row never drawn in a
//! passing colour; the profile named in every counting badge; and the command
//! writing the files and refusing a `perf-result`.

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
use veredictum::badge::{Badge, BadgeColour, BadgeName, badges, perf_badge};
use veredictum::party::InstrumentStatus;
use veredictum::perf_result::PerfResult;
use veredictum::perf_result::capacity::Knee;
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
        badge(&passing(), BadgeName::Warnings).unwrap().message,
        "1",
        "the warning row counts as a warning"
    );
    assert_eq!(
        badge(&passing(), BadgeName::Base).unwrap().message,
        "3/4 passed"
    );
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
        assert_eq!(base.message, "3/5 passed \u{b7} 1 errored", "{whose:?}");
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
            Some("3/5"),
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
fn every_counting_badge_names_its_profile_and_the_profiles_never_draw_alike() {
    for name in [
        BadgeName::Base,
        BadgeName::Options,
        BadgeName::Warnings,
        BadgeName::Profile,
    ] {
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
            assert!(released.label.ends_with("\u{b7} released"), "{released:?}");
            assert!(
                development.label.ends_with("\u{b7} development"),
                "{development:?}"
            );
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
    assert_eq!(json["label"], "openEHR BASE \u{b7} released");
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
    assert_eq!(files.len(), 10);
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

/// The committed example, whose knee is a bracket and whose workload is
/// custom, and the same record read as a lower bound of a pinned workload.
fn perf_records() -> Result<[(&'static str, PerfResult); 2], Box<dyn std::error::Error>> {
    let bracket = PerfResult::read(&std::fs::read_to_string(perf_example())?)?;
    let mut lower = bracket.clone();
    lower.derived.knee = Knee::LowerBound {
        qualified_rate: 1_250.0,
        achieved_per_s: 1_249.5,
    };
    "ehr-mix@2".clone_into(&mut lower.raw.workload.label);
    Ok([("bracket", bracket), ("lower-bound", lower)])
}

#[test]
fn the_perf_badge_is_pinned_for_a_bracket_and_a_lower_bound() {
    for (kind, record) in perf_records().unwrap() {
        let badge = perf_badge(&record);
        assert_eq!(badge.name, BadgeName::Perf);
        assert_eq!(
            badge.colour,
            BadgeColour::Reading,
            "a knee is never pass or fail"
        );
        assert_eq!(badge, perf_badge(&record), "byte-deterministic");
        insta::assert_snapshot!(format!("perf-{kind}.svg"), badge.svg());
        insta::assert_snapshot!(format!("perf-{kind}.json"), badge.endpoint_json().unwrap());
    }
    let [(_, bracket), (_, lower)] = perf_records().unwrap();
    assert_eq!(perf_badge(&bracket).label, "openEHR perf \u{b7} custom");
    assert_eq!(
        perf_badge(&bracket).message,
        "knee 180.8\u{2013}210.9/s \u{b7} self-run \u{b7} 1 pass, noise unmeasured"
    );
    assert_eq!(perf_badge(&lower).label, "openEHR perf \u{b7} ehr-mix@2");
    assert!(
        perf_badge(&lower)
            .message
            .starts_with("knee \u{2265} 1,250.0/s"),
        "{}",
        perf_badge(&lower).message
    );
}

#[test]
fn the_command_draws_only_the_perf_badge_for_a_perf_result() {
    let dir = assert_fs::TempDir::new().unwrap();
    let output = badge_command(&perf_example(), dir.path()).unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut written: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    written.sort();
    assert_eq!(written, ["perf.json", "perf.svg"]);
    let [(_, bracket), _] = perf_records().unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("perf.svg")).unwrap(),
        perf_badge(&bracket).svg()
    );
}
