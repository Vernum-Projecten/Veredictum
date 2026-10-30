// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The posture canaries that bracket a performance run, against a fake
//! server, and the refusals `submission_defects` makes over a record's
//! posture block.
//!
//! A canary reading that contradicts the declared profile refuses the run
//! with the item named. Every item a canary can observe must read `verified`
//! on both brackets, and an item nothing on the wire discloses must never
//! claim it was verified. Each fixture below changes one item of a passing
//! block, so a refusal names exactly the item it states.

#![expect(
    clippy::unwrap_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken harness must abort the test loudly, Book ch11"
)]

use serde_json::json;
use veredictum::perf_emulate::canary::CanaryClient;
use veredictum::perf_emulate::posture::{
    Assurance, AuditSink, AuthnMode, Bracket, CanaryOutcome, CanaryReading, CanaryTarget,
    CompressionMode, InvalidTwin, MINIMAL, PostureDefectKind, PostureDisclosure, PostureError,
    PostureItem, PostureProfile, PostureRecord, SigningScheme, Tenancy, ValidationDepth,
    VersionSample, bracket, settle, submission_defects, tls_of,
};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

use crate::fake_sut::FakeSut;

/// One canary reading, for the fixtures below.
fn reading(bracket: Bracket, outcome: CanaryOutcome, observed: &str) -> CanaryReading {
    CanaryReading {
        bracket,
        outcome,
        observed: observed.to_owned(),
        evidence: "a reading this fixture states rather than takes".to_owned(),
    }
}

/// A posture block shaped exactly as a passing record carries one: every
/// observable item verified on both brackets, the two nothing discloses
/// carried as claims.
fn publishable_posture() -> PostureRecord {
    PostureRecord {
        profile: "minimal".to_owned(),
        summary: "the fixture profile".to_owned(),
        items: PostureItem::ALL
            .iter()
            .copied()
            .map(|item| {
                let (outcome, assurance) = if item.is_observable() {
                    (CanaryOutcome::Confirmed, Assurance::Verified)
                } else {
                    (CanaryOutcome::NotObservable, Assurance::DeclaredOnly)
                };
                PostureDisclosure {
                    item,
                    declared: "off".to_owned(),
                    assurance,
                    readings: vec![
                        reading(Bracket::Before, outcome, "off"),
                        reading(Bracket::After, outcome, "off"),
                    ],
                }
            })
            .collect(),
        comparability: Vec::new(),
    }
}

/// The same block with one item rewritten, which is how each refusal fixture
/// below states the single thing it changed.
fn with_item(
    posture: &PostureRecord,
    item: PostureItem,
    outcome: CanaryOutcome,
    assurance: Assurance,
) -> PostureRecord {
    let mut rewritten = posture.clone();
    for line in &mut rewritten.items {
        if line.item == item {
            line.assurance = assurance;
            line.readings = vec![
                reading(Bracket::Before, outcome, "(not observable)"),
                reading(Bracket::After, outcome, "(not observable)"),
            ];
        }
    }
    rewritten
}

/// The shape a passing record carries produces no finding at all, so the
/// fixtures below fail for the reason each one states and not for a shape the
/// gate refuses anyway.
#[test]
fn the_publishable_fixture_carries_no_finding() {
    assert_eq!(submission_defects(&publishable_posture()), Vec::new());
}

/// Signing has an observable that always exists — the run commits versions of
/// its own and reads them back — so a bracket that saw nothing is a canary
/// that did not run, and the record is refused rather than published with the
/// declaration standing unchecked.
#[test]
fn a_signing_canary_that_observed_nothing_is_refused() {
    let posture = with_item(
        &publishable_posture(),
        PostureItem::VersionSigning,
        CanaryOutcome::NotObservable,
        Assurance::DeclaredOnly,
    );
    let defects = submission_defects(&posture);
    assert_eq!(defects.len(), 1, "{defects:?}");
    let defect = defects.first().expect("one defect was just asserted");
    assert_eq!(defect.item, PostureItem::VersionSigning);
    assert_eq!(defect.kind, PostureDefectKind::Unverified);
    assert!(defect.to_string().contains("version_signing"), "{defect}");
}

/// Commit validation has an observable that always exists too: every pack
/// embeds the invalid twin the canary offers, so a declared-only validation
/// item is a check that was skipped.
#[test]
fn a_declared_only_validation_item_is_refused() {
    let posture = with_item(
        &publishable_posture(),
        PostureItem::CommitValidation,
        CanaryOutcome::NotObservable,
        Assurance::DeclaredOnly,
    );
    let defects = submission_defects(&posture);
    assert_eq!(defects.len(), 1, "{defects:?}");
    let defect = defects.first().expect("one defect was just asserted");
    assert_eq!(defect.item, PostureItem::CommitValidation);
    assert_eq!(defect.kind, PostureDefectKind::Unverified);
}

/// The same rule holds for the three items the invocation settles: an
/// uncredentialed read, the base URL scheme and an encoded response are all
/// first-hand observations, so none of them may stand unchecked.
#[test]
fn an_unchecked_invocation_item_is_refused() {
    for item in [
        PostureItem::Authn,
        PostureItem::Tls,
        PostureItem::Compression,
    ] {
        let posture = with_item(
            &publishable_posture(),
            item,
            CanaryOutcome::NotObservable,
            Assurance::DeclaredOnly,
        );
        let defects = submission_defects(&posture);
        assert_eq!(defects.len(), 1, "{item}: {defects:?}");
        let defect = defects.first().expect("one defect was just asserted");
        assert_eq!(defect.item, item);
        assert_eq!(defect.kind, PostureDefectKind::Unverified);
    }
}

/// Nothing on the wire discloses an audit trail, so a block claiming one was
/// verified claims an observation the machinery cannot make, and that is
/// refused as loudly as a missing check.
#[test]
fn a_verified_claim_on_an_unobservable_item_is_refused() {
    let posture = with_item(
        &publishable_posture(),
        PostureItem::Audit,
        CanaryOutcome::Confirmed,
        Assurance::Verified,
    );
    let defects = submission_defects(&posture);
    assert_eq!(defects.len(), 1, "{defects:?}");
    let defect = defects.first().expect("one defect was just asserted");
    assert_eq!(defect.item, PostureItem::Audit);
    assert_eq!(defect.kind, PostureDefectKind::Unverifiable);
}

/// A block that simply omits an item is refused by name, so a shorter block
/// can never read as a passing one.
#[test]
fn a_posture_block_missing_an_item_is_refused() {
    let mut posture = publishable_posture();
    posture.items.retain(|line| line.item != PostureItem::Tls);
    let defects = submission_defects(&posture);
    assert_eq!(defects.len(), 1, "{defects:?}");
    let defect = defects.first().expect("one defect was just asserted");
    assert_eq!(defect.item, PostureItem::Tls);
    assert_eq!(defect.kind, PostureDefectKind::Missing);
}
/// Answers a composition commit the way a template-validating server does:
/// the invalid twin (a composition with the mandatory `COMPOSITION.composer`
/// gone) is refused `422`, everything else is created. ITS-REST
/// `specifications/responses/422.yaml` defines the refusal as the case where
/// the template "is not validating the supplied resource".
struct ValidatingCommit;

impl Respond for ValidatingCommit {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body = String::from_utf8_lossy(&request.body);
        if body.contains("\"composer\"") {
            ResponseTemplate::new(201).insert_header("ETag", "\"c-1::sut::1\"")
        } else {
            ResponseTemplate::new(422)
        }
    }
}

/// A commit path that accepts anything, including the invalid twin.
struct AcceptingCommit;

impl Respond for AcceptingCommit {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        ResponseTemplate::new(201).insert_header("ETag", "\"c-1::sut::1\"")
    }
}

/// A profile declaring no commit validation at all, so the same lenient
/// server that contradicts `minimal` confirms this one.
static UNVALIDATED: PostureProfile = PostureProfile {
    name: "test-unvalidated",
    summary: "A profile for this suite: nothing switched on, and commits unvalidated.",
    audit: AuditSink::Off,
    signing: SigningScheme::None,
    validation: ValidationDepth::None,
    compression: CompressionMode::Off,
    tenancy: Tenancy::Single,
};

/// Mounts the exchanges every canary sends, answered as a spec-conformant
/// `minimal` deployment answers them; a test mounts its deviation first,
/// because wiremock matches in registration order.
fn mount_minimal(sut: &FakeSut) {
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([]))),
    );
    sut.mount(Mock::given(method("POST")).and(path("/ehr")).respond_with(
        ResponseTemplate::new(201).insert_header("Location", "http://sut/ehr/EHR-1"),
    ));
    sut.mount(
        Mock::given(method("POST"))
            .and(path_regex(r"^/ehr/[^/]+/composition$"))
            .respond_with(ValidatingCommit),
    );
    sut.mount(
        Mock::given(method("GET"))
            .and(path_regex(
                r"^/ehr/[^/]+/versioned_composition/[^/]+/version/[^/]+$",
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "_type": "ORIGINAL_VERSION" })),
            ),
    );
}

fn twin() -> InvalidTwin {
    InvalidTwin {
        key: "test.twin".to_owned(),
        bytes: r#"{"_type":"COMPOSITION","name":{"value":"twin"}}"#.to_owned(),
    }
}

/// Takes both brackets against `sut` and settles them, the way a run does
/// after seeding and after its last plateau.
fn bracketed(sut: &FakeSut, profile: &PostureProfile) -> Result<PostureRecord, PostureError> {
    let client = CanaryClient::new(&sut.base_url(), None, Vec::new()).unwrap();
    let raw = client.without_decompression().unwrap();
    let anonymous = client.without_credential().unwrap();
    let samples = vec![VersionSample {
        ehr_id: "EHR-1".to_owned(),
        object_uid: "c-1".to_owned(),
        version_uid: "c-1::sut::1".to_owned(),
    }];
    let target = CanaryTarget {
        client: &client,
        raw: &raw,
        anonymous: &anonymous,
        profile,
        authn: AuthnMode::None,
        tls: tls_of(&sut.base_url()),
        invalid_twin: Some(twin()),
        samples: &samples,
    };
    let before = bracket(&target, Bracket::Before);
    let after = bracket(&target, Bracket::After);
    settle(profile, AuthnMode::None, target.tls, &before, &after)
}

/// A conformant `minimal` deployment is bracketed on both ends and every
/// observable item is verified first-hand.
#[test]
fn a_minimal_deployment_is_bracketed_and_verified() {
    let sut = FakeSut::start();
    mount_minimal(&sut);
    let record = bracketed(&sut, &MINIMAL).unwrap();
    assert_eq!(record.profile, "minimal");
    assert_eq!(submission_defects(&record), Vec::new());
    for line in &record.items {
        assert_eq!(line.readings.len(), 2, "{} is not bracketed", line.item);
        assert_eq!(line.readings[0].bracket, Bracket::Before);
        assert_eq!(line.readings[1].bracket, Bracket::After);
    }
}

/// A server that accepts the invalid twin contradicts a `minimal`
/// declaration, and the run is refused with the item named.
#[test]
fn a_lenient_server_contradicts_the_declared_validation_depth() {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path_regex(r"^/ehr/[^/]+/composition$"))
            .respond_with(AcceptingCommit),
    );
    mount_minimal(&sut);
    let error = bracketed(&sut, &MINIMAL).unwrap_err();
    assert!(matches!(error, PostureError::Contradiction(_)), "{error}");
    let text = error.to_string();
    assert!(text.contains("commit_validation"), "{text}");
    assert!(text.contains("accepts-the-invalid-twin"), "{text}");
}

/// A profile that declares no validation is verified against that same
/// lenient server: the canary reports what it saw.
#[test]
fn a_declared_absence_of_validation_is_verified_the_same_way() {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path_regex(r"^/ehr/[^/]+/composition$"))
            .respond_with(AcceptingCommit),
    );
    mount_minimal(&sut);
    let record = bracketed(&sut, &UNVALIDATED).unwrap();
    assert_eq!(record.declared(PostureItem::CommitValidation), Some("none"));
    assert!(
        record
            .verified_items()
            .contains(&PostureItem::CommitValidation)
    );
}

/// A server that signs the versions the run's own seeding committed
/// contradicts a declaration of `none`.
#[test]
fn a_signed_population_contradicts_a_declaration_of_no_signing() {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path_regex(
                r"^/ehr/[^/]+/versioned_composition/[^/]+/version/[^/]+$",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "_type": "ORIGINAL_VERSION",
                "signature": "sha256:6c1f2f8ac0d6dd1c8f0e2f0a0b1c2d3e4f5061728394a5b6c7d8e9f001122334"
            }))),
    );
    mount_minimal(&sut);
    let error = bracketed(&sut, &MINIMAL).unwrap_err().to_string();
    assert!(error.contains("posture canary contradicts"), "{error}");
    assert!(error.contains("version_signing"), "{error}");
    assert!(error.contains("digest"), "{error}");
}

/// A run names its profile by token, and an unknown name is refused rather
/// than read as `minimal`.
#[test]
fn a_profile_is_named_by_token_and_an_unknown_one_is_refused() {
    assert_eq!(
        PostureProfile::by_name("clinical-default").unwrap().name,
        "clinical-default"
    );
    let error = PostureProfile::by_name("fast").unwrap_err().to_string();
    assert!(error.contains("minimal, minimal-signed-digest"), "{error}");
}
