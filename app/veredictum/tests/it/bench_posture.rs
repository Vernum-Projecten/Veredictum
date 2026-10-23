// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The posture refusals `submission_defects` makes over a bench record's
//! posture block.
//!
//! Every item a canary can observe must read `verified` on both brackets, and
//! an item nothing on the wire discloses must never claim it was verified. Each
//! fixture below changes one item of a passing block, so a refusal names
//! exactly the item it states.

use veredictum::bench::posture::{
    Assurance, Bracket, CanaryOutcome, CanaryReading, PostureDefectKind, PostureDisclosure,
    PostureItem, PostureRecord, submission_defects,
};

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
