// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `x-veredictum-principal` extension: a step names the principal its
//! request is sent as when that is not the run's ordinary credential.
//!
//! ITS-REST `docs/overview/Requests_and_responses.md` §Authentication and
//! authorization leaves the scheme open ("this specification does not mandate
//! a specific authentication scheme"), so which credential a service grants an
//! administrative operation to is something the party supplies, never
//! something the instrument assumes. A step that names a principal is sent
//! with that principal's credential; a workflow with such a step is skipped
//! as `not-declared` when the party supplies none.
//! No openEHR spec governs the extension itself — our own design.

use std::collections::BTreeMap;
use std::path::Path;

use roas_arazzo::v1_1::Workflow;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::arazzo::cite::{Citation, Level};
use crate::arazzo::document::ArazzoDocument;
use crate::arazzo::results::{Skip, SkipReason, Why};

/// The specification-extension key of a step's principal.
pub const PRINCIPAL_KEY: &str = "x-veredictum-principal";

/// A principal a step may be sent as, beside the run's ordinary credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Principal {
    /// A principal the service authorizes for its administrative operations.
    Administrative,
}

impl Principal {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::Administrative];

    /// The token a step spells.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Administrative => "administrative",
        }
    }

    /// Reads a token; an unknown one is refused, never defaulted.
    ///
    /// # Errors
    /// [`PrincipalError::Unknown`] for a token outside the vocabulary.
    pub fn parse(token: &str) -> Result<Self, PrincipalError> {
        Self::ALL
            .iter()
            .copied()
            .find(|principal| principal.token() == token)
            .ok_or_else(|| PrincipalError::Unknown(token.to_owned()))
    }
}

/// Why an `x-veredictum-principal` value does not hold.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrincipalError {
    /// The value is not a string.
    #[error("`{PRINCIPAL_KEY}` is not a string")]
    NotAString,
    /// The token names no principal.
    #[error("`{PRINCIPAL_KEY}` names `{0}`, which is not a principal of the vocabulary")]
    Unknown(String),
}

/// The principal a step's extensions name, if any.
///
/// # Errors
/// [`PrincipalError`] when the extension is present and does not name a
/// principal of the vocabulary.
pub fn principal_of(
    extensions: Option<&BTreeMap<String, Value>>,
) -> Result<Option<Principal>, PrincipalError> {
    match extensions.and_then(|e| e.get(PRINCIPAL_KEY)) {
        None => Ok(None),
        Some(Value::String(token)) => Principal::parse(token).map(Some),
        Some(_) => Err(PrincipalError::NotAString),
    }
}

/// Whether any step of `workflow` is sent as `principal`.
#[must_use]
pub fn names(workflow: &Workflow, principal: Principal) -> bool {
    workflow
        .steps
        .iter()
        .any(|step| principal_of(step.extensions.as_ref()) == Ok(Some(principal)))
}

/// The released sentence a `not-declared` skip of a principal cites.
#[must_use]
pub fn undeclared_citation() -> Citation {
    Citation {
        source: "specs/openehr/ITS-REST/specifications/docs/overview/Requests_and_responses.md"
            .to_owned(),
        section: "Authentication and authorization".to_owned(),
        quote: "Services SHOULD implement and support an HTTP Authentication and Authorization framework, though this specification does not mandate a specific authentication scheme.".to_owned(),
        report: None,
        level: Level::Should,
    }
}

/// The skip of every workflow of `document` with a step sent as
/// `principal`, by `workflowId`, for a party that supplies no credential for
/// it.
#[must_use]
pub fn undeclared_skips(
    document: &ArazzoDocument,
    principal: Principal,
    repo_root: &Path,
) -> BTreeMap<String, Skip> {
    let why = Why::of(&undeclared_citation(), repo_root);
    document
        .description()
        .workflows
        .iter()
        .filter(|workflow| names(workflow, principal))
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

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn a_principal_token_round_trips_and_an_unknown_one_is_refused() {
        for principal in Principal::ALL {
            assert_eq!(Principal::parse(principal.token()), Ok(*principal));
            assert_eq!(
                serde_json::to_value(principal).ok(),
                Some(Value::String(principal.token().to_owned()))
            );
        }
        assert_eq!(
            Principal::parse("admin"),
            Err(PrincipalError::Unknown("admin".to_owned()))
        );
        let mut extensions = BTreeMap::new();
        extensions.insert(PRINCIPAL_KEY.to_owned(), Value::Bool(true));
        assert_eq!(
            principal_of(Some(&extensions)),
            Err(PrincipalError::NotAString)
        );
        assert_eq!(principal_of(None), Ok(None));
    }

    #[test]
    fn the_undeclared_citation_holds_against_the_vendored_text() {
        let citation = undeclared_citation();
        assert_eq!(check_level(&citation), Ok(()));
        assert_eq!(QuoteChecker::new(&repo_root()).check(&citation), Ok(()));
    }
}
