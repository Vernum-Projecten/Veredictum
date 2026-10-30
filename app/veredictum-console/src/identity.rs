// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! What the connect step learned about the server's identity, and which
//! identity fields the operator still enters.
//!
//! The engine reads the identity (`veredictum identify`, over the ITS-REST
//! System OPTIONS operation of `specifications/operations/options.yaml`); this
//! module only decides, for each field, whether the screen shows the server's
//! value read-only or the operator's input, and which operator values a run
//! passes on. The rule is the engine's own merge: a field the server stated
//! is the server's, and the operator supplies only the rest. Plain Rust on
//! both targets, so the decision has ordinary unit tests.

use serde::{Deserialize, Serialize};

/// The identity members the server stated in its `Options` document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatedIdentity {
    /// The `solution` member.
    pub name: Option<String>,
    /// The `solution_version` member.
    pub version: Option<String>,
    /// The `vendor` member.
    pub vendor: Option<String>,
    /// The `restapi_specs_version` member.
    pub restapi_specs_version: Option<String>,
}

/// What the server answered to the identity read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdentityAnswer {
    /// A 200 `Options` document, with whatever members it carried.
    Stated(StatedIdentity),
    /// A status outside 2xx: the server does not offer the document, which
    /// the operation's SHOULD permits.
    NotOffered {
        /// The status code.
        status: u16,
    },
    /// A 2xx that is not an `Options` document.
    NotOptionsDocument {
        /// The status code.
        status: u16,
        /// Why, in the engine's words.
        reason: String,
    },
    /// A code from 600 to 999, which RFC 9110 §15 makes invalid.
    InvalidStatus {
        /// The status code.
        status: u16,
    },
    /// The read did not complete; the field is the diagnostic, verbatim.
    Unread {
        /// The diagnostic.
        error: String,
    },
}

/// How the connect step shows one identity field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldEntry {
    /// The server stated the value: shown read-only, labelled as reported by
    /// the server.
    Reported(String),
    /// The server did not state it: the operator's input, labelled as entered
    /// by the operator.
    Entered,
}

/// A value that carries something: blank text identifies nothing, exactly as
/// the engine's merge reads it.
fn stated(value: Option<&String>) -> Option<&str> {
    value
        .map(String::as_str)
        .filter(|text| !text.trim().is_empty())
}

impl IdentityAnswer {
    /// The stated members, when the server offered the document.
    #[must_use]
    pub fn stated(&self) -> Option<&StatedIdentity> {
        match self {
            Self::Stated(identity) => Some(identity),
            Self::NotOffered { .. }
            | Self::NotOptionsDocument { .. }
            | Self::InvalidStatus { .. }
            | Self::Unread { .. } => None,
        }
    }

    /// How the name is entered.
    #[must_use]
    pub fn name_entry(&self) -> FieldEntry {
        entry(self.stated().and_then(|s| stated(s.name.as_ref())))
    }

    /// How the version is entered.
    #[must_use]
    pub fn version_entry(&self) -> FieldEntry {
        entry(self.stated().and_then(|s| stated(s.version.as_ref())))
    }

    /// The vendor the server stated, when it stated one.
    #[must_use]
    pub fn vendor(&self) -> Option<&str> {
        self.stated().and_then(|s| stated(s.vendor.as_ref()))
    }

    /// The REST specification version the server stated, when it stated one.
    #[must_use]
    pub fn restapi_specs_version(&self) -> Option<&str> {
        self.stated()
            .and_then(|s| stated(s.restapi_specs_version.as_ref()))
    }

    /// One line saying why the operator enters the identity, or `None` when
    /// the server stated the document.
    #[must_use]
    pub fn fallback_reason(&self) -> Option<String> {
        match self {
            Self::Stated(_) => None,
            Self::NotOffered { status } => Some(format!(
                "The server does not offer System OPTIONS (HTTP {status}), so the name and version are the operator's."
            )),
            Self::NotOptionsDocument { status, reason } => Some(format!(
                "The server's System OPTIONS answer (HTTP {status}) is not an Options document ({reason}), so the name and version are the operator's."
            )),
            Self::InvalidStatus { status } => Some(format!(
                "The server's System OPTIONS answer carries {status}, which is not an HTTP status, so the name and version are the operator's."
            )),
            Self::Unread { error } => Some(format!(
                "The server's identity could not be read ({error}), so the name and version are the operator's."
            )),
        }
    }
}

fn entry(server: Option<&str>) -> FieldEntry {
    server.map_or(FieldEntry::Entered, |value| {
        FieldEntry::Reported(value.to_owned())
    })
}

/// The identity a run records, field by field, with the operator value a run
/// passes on for each field the server did not state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The name the record will carry, from whichever side supplies it.
    pub name: Option<String>,
    /// The version the record will carry.
    pub version: Option<String>,
    /// `--sut-name`: the operator's name, only when the server stated none.
    pub operator_name: Option<String>,
    /// `--sut-version`: the operator's version, only when the server stated
    /// none.
    pub operator_version: Option<String>,
}

impl Resolved {
    /// The fields neither side supplies, by the label the screen gives them.
    #[must_use]
    pub fn missing(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.name.is_none() {
            missing.push("display name");
        }
        if self.version.is_none() {
            missing.push("version label");
        }
        missing
    }
}

/// Resolves the identity from the answer, `None` when the identity was never
/// read, and the operator's inputs.
#[must_use]
pub fn resolve(
    answer: Option<&IdentityAnswer>,
    operator_name: &str,
    operator_version: &str,
) -> Resolved {
    let operator = |text: &str| {
        let trimmed = text.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    };
    let (name, operator_name) = match answer.map_or(FieldEntry::Entered, IdentityAnswer::name_entry)
    {
        FieldEntry::Reported(value) => (Some(value), None),
        FieldEntry::Entered => (operator(operator_name), operator(operator_name)),
    };
    let (version, operator_version) =
        match answer.map_or(FieldEntry::Entered, IdentityAnswer::version_entry) {
            FieldEntry::Reported(value) => (Some(value), None),
            FieldEntry::Entered => (operator(operator_version), operator(operator_version)),
        };
    Resolved {
        name,
        version,
        operator_name,
        operator_version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stated_answer(name: Option<&str>, version: Option<&str>) -> IdentityAnswer {
        IdentityAnswer::Stated(StatedIdentity {
            name: name.map(str::to_owned),
            version: version.map(str::to_owned),
            vendor: Some(String::from("Vendor")),
            restapi_specs_version: Some(String::from("1.1.0")),
        })
    }

    #[test]
    fn a_stated_identity_is_read_only_and_passes_no_operator_value() {
        let answer = stated_answer(Some("cdr"), Some("5.3.2"));
        assert_eq!(
            answer.name_entry(),
            FieldEntry::Reported(String::from("cdr"))
        );
        assert_eq!(
            answer.version_entry(),
            FieldEntry::Reported(String::from("5.3.2"))
        );
        let resolved = resolve(Some(&answer), "typed", "typed");
        assert_eq!(resolved.name.as_deref(), Some("cdr"));
        assert_eq!(resolved.version.as_deref(), Some("5.3.2"));
        assert_eq!(resolved.operator_name, None);
        assert_eq!(resolved.operator_version, None);
        assert!(resolved.missing().is_empty());
        assert_eq!(answer.vendor(), Some("Vendor"));
        assert_eq!(answer.restapi_specs_version(), Some("1.1.0"));
        assert_eq!(answer.fallback_reason(), None);
    }

    #[test]
    fn an_answer_without_the_document_leaves_both_fields_to_the_operator() {
        for answer in [
            IdentityAnswer::NotOffered { status: 404 },
            IdentityAnswer::NotOptionsDocument {
                status: 200,
                reason: String::from("not JSON"),
            },
            IdentityAnswer::InvalidStatus { status: 600 },
            IdentityAnswer::Unread {
                error: String::from("no engine"),
            },
        ] {
            assert_eq!(answer.name_entry(), FieldEntry::Entered);
            assert_eq!(answer.version_entry(), FieldEntry::Entered);
            let resolved = resolve(Some(&answer), "my-cdr", " 2.0 ");
            assert_eq!(resolved.operator_name.as_deref(), Some("my-cdr"));
            assert_eq!(resolved.operator_version.as_deref(), Some("2.0"));
            assert!(answer.fallback_reason().is_some());
            assert_eq!(answer.vendor(), None);
        }
    }

    #[test]
    fn a_partial_document_asks_the_operator_only_for_what_it_left_out() {
        let answer = stated_answer(Some("cdr"), None);
        let resolved = resolve(Some(&answer), "typed", "2.0");
        assert_eq!(resolved.name.as_deref(), Some("cdr"));
        assert_eq!(resolved.operator_name, None);
        assert_eq!(resolved.operator_version.as_deref(), Some("2.0"));
    }

    #[test]
    fn a_blank_stated_value_is_entered_by_the_operator() {
        let answer = stated_answer(Some("  "), Some("1"));
        assert_eq!(answer.name_entry(), FieldEntry::Entered);
    }

    #[test]
    fn a_field_nobody_supplies_is_named_as_missing() {
        let answer = IdentityAnswer::NotOffered { status: 404 };
        let resolved = resolve(Some(&answer), "", "1.0");
        assert_eq!(resolved.missing(), vec!["display name"]);
    }

    #[test]
    fn an_identity_never_read_is_the_operators() {
        let resolved = resolve(None, "my-cdr", "");
        assert_eq!(resolved.name.as_deref(), Some("my-cdr"));
        assert_eq!(resolved.operator_name.as_deref(), Some("my-cdr"));
        assert_eq!(resolved.missing(), vec!["version label"]);
    }
}
