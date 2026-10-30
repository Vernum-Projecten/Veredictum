// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `x-veredictum-scope` extension: a step names the scope set of the
//! access token its request presents.
//!
//! SMART on openEHR scopes "are evaluated by the _Platform_ when issuing
//! access tokens" (ITS-REST `docs/smart_app_launch/master08-scopes.adoc`
//! §Scopes), so a token carrying a given scope set is something the party
//! supplies, or a test issuer the party's deployment trusts mints. A step
//! that names a scope set is sent with a marker naming that set as its
//! `Authorization` value ([`marker`]), and the credential transport puts the
//! token in the marker's place on the wire
//! ([`crate::arazzo::credential::CredentialedTransport`]); a row whose scope
//! set the party covers neither way is skipped as `not-declared`.
//! No openEHR spec governs the extension itself — our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use roas_arazzo::v1_1::Workflow;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::arazzo::cite::Citation;
use crate::arazzo::results::{Skip, SkipReason, Why};
use crate::arazzo::rows::Row;

/// The specification-extension key of a step's scope set.
pub const SCOPE_KEY: &str = "x-veredictum-scope";

/// The prefix of the `Authorization` value a scoped step carries until the
/// transport puts the token in its place; the scope set and `]` follow it.
pub const MARKER_PREFIX: &str = "[a token whose scope claim is: ";

/// The runtime-expression prefix a step's scope set may be read from.
const INPUTS_PREFIX: &str = "$inputs.";

/// One OAuth 2.0 scope value: a set of scope tokens.
///
/// RFC 6749 §3.3 (<https://www.rfc-editor.org/rfc/rfc6749#section-3.3>)
/// makes the value "a list of space-delimited, case-sensitive strings" whose
/// "order does not matter", so two spellings of one set are one set: the
/// tokens are kept sorted and printed joined by one space. The empty set is
/// a token with no scope at all.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScopeSet(BTreeSet<String>);

impl ScopeSet {
    /// Reads a space-delimited scope value.
    ///
    /// # Errors
    /// [`ScopeError::Token`] for a token with a character RFC 6749 §3.3's
    /// `scope-token` grammar excludes (`%x21 / %x23-5B / %x5D-7E`).
    pub fn parse(text: &str) -> Result<Self, ScopeError> {
        let mut tokens = BTreeSet::new();
        for token in text.split(' ').filter(|token| !token.is_empty()) {
            let legal = |c: char| matches!(c, '\x21' | '\x23'..='\x5B' | '\x5D'..='\x7E');
            if !token.chars().all(legal) {
                return Err(ScopeError::Token(token.to_owned()));
            }
            tokens.insert(token.to_owned());
        }
        Ok(Self(tokens))
    }

    /// Whether the set holds no scope token.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for ScopeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let joined: Vec<&str> = self.0.iter().map(String::as_str).collect();
        f.write_str(&joined.join(" "))
    }
}

impl Serialize for ScopeSet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ScopeSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// Why an `x-veredictum-scope` value, or a scope value, does not hold.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScopeError {
    /// The extension's value is not a string.
    #[error("`{SCOPE_KEY}` is not a string")]
    NotAString,
    /// A scope token carries a character the scope grammar excludes.
    #[error("scope token `{0}` carries a character RFC 6749 §3.3 excludes from a scope-token")]
    Token(String),
    /// The `$inputs.` form names no input.
    #[error("`{SCOPE_KEY}` reads `{0}`, which names no input")]
    BadInput(String),
    /// The step names a principal as well, and a request presents one
    /// credential.
    #[error("a step names both `{SCOPE_KEY}` and `x-veredictum-principal`")]
    WithPrincipal,
}

/// Where a step's scope set comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeRef {
    /// This scope set, written on the step.
    Literal(ScopeSet),
    /// The named workflow input, a string of each row.
    Input(String),
}

impl ScopeRef {
    /// The scope set this reference names for `inputs`.
    ///
    /// # Errors
    /// A message when the input is absent, not a string, or not a scope
    /// value.
    pub fn resolve(&self, inputs: &Row) -> Result<ScopeSet, String> {
        match self {
            Self::Literal(set) => Ok(set.clone()),
            Self::Input(name) => match inputs.get(name) {
                Some(Value::String(text)) => ScopeSet::parse(text).map_err(|e| e.to_string()),
                Some(other) => Err(format!(
                    "the scope set input `{name}` is {other}, not a string"
                )),
                None => Err(format!("the scope set input `{name}` is absent")),
            },
        }
    }
}

/// The scope set a step's extensions name, if any.
///
/// The value is a scope value, or `$inputs.<name>` to read it from a row.
///
/// # Errors
/// [`ScopeError`] when the extension is present and is not one of the two
/// forms, or the step also names a principal.
pub fn scope_of(
    extensions: Option<&BTreeMap<String, Value>>,
) -> Result<Option<ScopeRef>, ScopeError> {
    let Some(value) = extensions.and_then(|e| e.get(SCOPE_KEY)) else {
        return Ok(None);
    };
    if extensions.is_some_and(|e| e.contains_key(crate::arazzo::principal::PRINCIPAL_KEY)) {
        return Err(ScopeError::WithPrincipal);
    }
    let Value::String(text) = value else {
        return Err(ScopeError::NotAString);
    };
    match text.strip_prefix(INPUTS_PREFIX) {
        Some(name) => {
            let named = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
            if named {
                Ok(Some(ScopeRef::Input(name.to_owned())))
            } else {
                Err(ScopeError::BadInput(text.clone()))
            }
        }
        None => ScopeSet::parse(text).map(|set| Some(ScopeRef::Literal(set))),
    }
}

/// The `Authorization` value a step presenting `set` carries until the
/// transport puts a token in its place.
#[must_use]
pub fn marker(set: &ScopeSet) -> String {
    format!("{MARKER_PREFIX}{set}]")
}

/// The scope set a marker names; `None` for a value that is not a marker.
///
/// # Errors
/// [`ScopeError`] when the value is a marker whose scope set does not parse.
#[must_use]
pub fn from_marker(value: &str) -> Option<Result<ScopeSet, ScopeError>> {
    let set = value.strip_prefix(MARKER_PREFIX)?.strip_suffix(']')?;
    Some(ScopeSet::parse(set))
}

/// Which scope sets the party covers: the sets it supplies a token for, and
/// whether a test issuer it declares mints a token for any set.
///
/// A recording carries it, so a re-judgement skips the rows the live run
/// skipped; it names no token.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Coverage {
    /// The scope sets a token is supplied for.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub issued: BTreeSet<ScopeSet>,
    /// Whether a declared test issuer mints a token for any scope set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub minted: bool,
}

impl Coverage {
    /// Whether the party covers `set`.
    #[must_use]
    pub fn covers(&self, set: &ScopeSet) -> bool {
        self.minted || self.issued.contains(set)
    }

    /// Whether the party covers no scope set at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.minted && self.issued.is_empty()
    }
}

/// The first scope set a row of `workflow` presents that `coverage` does not
/// cover; `None` when every scoped step's set is covered or cannot be read,
/// which the step itself then reports when it is driven.
#[must_use]
pub fn uncovered(workflow: &Workflow, inputs: &Row, coverage: &Coverage) -> Option<ScopeSet> {
    workflow.steps.iter().find_map(|step| {
        // NOTE: an unreadable scope is not "uncovered"; the drive reports it as errored.
        let set = scope_of(step.extensions.as_ref())
            .ok()??
            .resolve(inputs)
            .ok()?;
        (!coverage.covers(&set)).then_some(set)
    })
}

/// The released sentence a `not-declared` skip of a scope set cites.
#[must_use]
pub fn undeclared_citation() -> Citation {
    Citation {
        source: "specs/openehr/ITS-REST/docs/smart_app_launch/master08-scopes.adoc".to_owned(),
        section: "Scopes".to_owned(),
        quote: "scopes are passed during the authorization request and are evaluated by the _Platform_ when issuing access tokens.".to_owned(),
        report: None,
        level: None,
    }
}

/// The skip of a row whose scope set the party does not cover.
#[must_use]
pub fn undeclared_skip(repo_root: &Path) -> Skip {
    Skip {
        detail: None,
        reason: SkipReason::NotDeclared,
        why: Why::of(&undeclared_citation(), repo_root),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::cite::{QuoteChecker, check_level};
    use serde_json::json;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn set(text: &str) -> ScopeSet {
        ScopeSet::parse(text).unwrap()
    }

    #[test]
    fn a_scope_value_is_a_set_whatever_its_order_or_spacing() {
        assert_eq!(
            set("user/template-*.r  openid"),
            set("openid user/template-*.r")
        );
        assert_eq!(
            set("openid user/template-*.r").to_string(),
            "openid user/template-*.r"
        );
        assert_eq!(set("a a").to_string(), "a");
        assert!(set("").is_empty());
        assert_eq!(
            ScopeSet::parse("a\"b"),
            Err(ScopeError::Token("a\"b".to_owned()))
        );
        assert_eq!(
            ScopeSet::parse("a\\b"),
            Err(ScopeError::Token("a\\b".to_owned()))
        );
        let round: ScopeSet = serde_json::from_value(json!("b a")).unwrap();
        assert_eq!(serde_json::to_value(&round).unwrap(), json!("a b"));
    }

    #[test]
    fn a_step_names_a_literal_or_an_input_and_nothing_else() {
        let ext = |value: Value| BTreeMap::from([(SCOPE_KEY.to_owned(), value)]);
        assert_eq!(scope_of(None), Ok(None));
        assert_eq!(
            scope_of(Some(&ext(json!("system/template-*.r")))),
            Ok(Some(ScopeRef::Literal(set("system/template-*.r"))))
        );
        assert_eq!(
            scope_of(Some(&ext(json!("$inputs.scope")))),
            Ok(Some(ScopeRef::Input("scope".to_owned())))
        );
        assert_eq!(
            scope_of(Some(&ext(json!("$inputs.")))),
            Err(ScopeError::BadInput("$inputs.".to_owned()))
        );
        assert_eq!(
            scope_of(Some(&ext(json!(["a"])))),
            Err(ScopeError::NotAString)
        );
        let mut both = ext(json!("a"));
        both.insert(
            crate::arazzo::principal::PRINCIPAL_KEY.to_owned(),
            json!("administrative"),
        );
        assert_eq!(scope_of(Some(&both)), Err(ScopeError::WithPrincipal));
    }

    #[test]
    fn an_input_reference_reads_the_row() {
        let input = ScopeRef::Input("scope".to_owned());
        let row = Row::from([("scope".to_owned(), json!("b a"))]);
        assert_eq!(input.resolve(&row), Ok(set("a b")));
        assert!(input.resolve(&Row::new()).is_err());
        let number = Row::from([("scope".to_owned(), json!(3))]);
        assert!(input.resolve(&number).is_err());
    }

    #[test]
    fn a_marker_round_trips_its_scope_set_and_other_values_are_not_markers() {
        for text in ["", "system/template-*.r", "a]b c"] {
            let marked = marker(&set(text));
            assert_eq!(from_marker(&marked), Some(Ok(set(text))));
        }
        assert_eq!(from_marker("Bearer x"), None);
        assert_eq!(
            from_marker(&format!("{MARKER_PREFIX}a\"]")),
            Some(Err(ScopeError::Token("a\"".to_owned())))
        );
    }

    #[test]
    fn coverage_is_the_issued_sets_or_any_set_when_minted() {
        let issued = Coverage {
            issued: BTreeSet::from([set("a")]),
            minted: false,
        };
        assert!(issued.covers(&set("a")));
        assert!(!issued.covers(&set("a b")));
        let minted = Coverage {
            issued: BTreeSet::new(),
            minted: true,
        };
        assert!(minted.covers(&set("anything at all")));
        assert!(Coverage::default().is_empty());
        assert_eq!(
            serde_json::to_value(&issued).unwrap(),
            json!({ "issued": ["a"] })
        );
    }

    #[test]
    fn the_first_uncovered_scope_set_of_a_row_is_found() {
        let workflow: Workflow = serde_json::from_value(json!({
            "workflowId": "w",
            "steps": [
                { "stepId": "a", "operationId": "x", "x-veredictum-scope": "a" },
                { "stepId": "b", "operationId": "x", "x-veredictum-scope": "$inputs.scope" }
            ]
        }))
        .unwrap();
        let coverage = Coverage {
            issued: BTreeSet::from([set("a")]),
            minted: false,
        };
        let row = |scope: &str| Row::from([("scope".to_owned(), json!(scope))]);
        assert_eq!(uncovered(&workflow, &row("a"), &coverage), None);
        assert_eq!(uncovered(&workflow, &row("b"), &coverage), Some(set("b")));
        assert_eq!(uncovered(&workflow, &Row::new(), &coverage), None);
    }

    #[test]
    fn the_undeclared_citation_holds_against_the_vendored_text() {
        let citation = undeclared_citation();
        assert_eq!(check_level(&citation), Ok(()));
        assert_eq!(QuoteChecker::new(&repo_root()).check(&citation), Ok(()));
    }
}
