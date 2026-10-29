// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The SMART Platform base URL: the second base a step can be sent to.
//!
//! SMART on openEHR serves its discovery document "relative to the
//! _Platform_ base URL" (ITS-REST
//! `docs/smart_app_launch/master04-service_discovery.adoc` §Service
//! Discovery), and the same section shows that base differing from the
//! openEHR REST base. A source whose root says `x-veredictum-base: platform`
//! ([`crate::arazzo::source::Base`]) has its paths appended to the Platform
//! base URL the party supplies; every workflow with a step on such a source
//! is skipped as `not-declared` when the party supplies none.

use std::collections::BTreeMap;
use std::path::Path;

use roas_arazzo::v1_1::Workflow;

use crate::arazzo::cite::{Citation, Level};
use crate::arazzo::document::ArazzoDocument;
use crate::arazzo::results::{Skip, SkipReason, Why};
use crate::arazzo::source::Base;

/// The released sentence a `not-declared` skip of a Platform workflow cites.
#[must_use]
pub fn undeclared_citation() -> Citation {
    Citation {
        source: "specs/openehr/ITS-REST/docs/smart_app_launch/master04-service_discovery.adoc"
            .to_owned(),
        section: "Service Discovery".to_owned(),
        quote: "The configuration endpoint should be always available relative to the _Platform_ base URL.".to_owned(),
        report: None,
        level: Level::Must,
    }
}

/// Whether any step of `workflow` is sent to the Platform base URL.
#[must_use]
pub fn addresses(document: &ArazzoDocument, workflow: &Workflow) -> bool {
    workflow.steps.iter().any(|step| {
        step.operation_id
            .as_deref()
            .and_then(|qualified| document.operation(qualified))
            .is_some_and(|operation| operation.base == Base::Platform)
    })
}

/// The skip of every workflow of `document` with a step sent to the Platform
/// base URL, by `workflowId`, for a party that supplies none.
#[must_use]
pub fn undeclared_skips(document: &ArazzoDocument, repo_root: &Path) -> BTreeMap<String, Skip> {
    let why = Why::of(&undeclared_citation(), repo_root);
    document
        .description()
        .workflows
        .iter()
        .filter(|workflow| addresses(document, workflow))
        .map(|workflow| {
            (
                workflow.workflow_id.clone(),
                Skip {
                    reason: SkipReason::NotDeclared,
                    why: why.clone(),
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::cite::{QuoteChecker, check_level};

    #[test]
    fn the_undeclared_citation_holds_against_the_vendored_text() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let citation = undeclared_citation();
        assert_eq!(check_level(&citation), Ok(()));
        assert_eq!(QuoteChecker::new(&root).check(&citation), Ok(()));
    }
}
