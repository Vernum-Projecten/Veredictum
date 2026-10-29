// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Identifier newtypes for the schedule artifacts.
//!
//! Every identifier space in the CNF 2.0 artifact set is a distinct type so a
//! swapped-argument mistake is a compile error, and each carries its lexical
//! rule at parse time (CNF `platform_test_schedule/master03-overview.adoc`
//! defines the case-id families; the rest are this framework's own closed
//! grammars — no openEHR spec governs them: our own design).

use std::fmt;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

/// Lexical error for any identifier newtype in this module.
#[derive(Debug, Error)]
#[error("invalid {kind}: {value:?} ({rule})")]
pub struct IdError {
    kind: &'static str,
    value: String,
    rule: &'static str,
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

macro_rules! string_id {
    ($(#[$doc:meta])* $name:ident, $kind:literal, $rule:literal, $check:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Parse and lexically validate the identifier.
            ///
            /// # Errors
            /// Returns [`IdError`] when the value violates the identifier's
            /// lexical rule.
            pub fn parse(value: &str) -> Result<Self, IdError> {
                // The rule arrives either as a non-capturing closure literal or
                // as a fn item; binding it to a fn pointer accepts both and
                // keeps the expansion free of a directly-called closure.
                let rule_holds: fn(&str) -> bool = $check;
                if rule_holds(value) {
                    Ok(Self(value.to_owned()))
                } else {
                    Err(IdError { kind: $kind, value: value.to_owned(), rule: $rule })
                }
            }

            /// The identifier text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = IdError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let s = String::deserialize(deserializer)?;
                Self::parse(&s).map_err(D::Error::custom)
            }
        }
    };
}

string_id!(
    /// A global CNF case id.
    ///
    /// The families are `<SERVICE_COMPONENT>.<operation>-<variant>`,
    /// `CONT-<TYPE>-<variant>`, `SF-<FORM>-<variant>`, … . Family membership
    /// is checked by the validator; lexically an id is non-empty ASCII
    /// without whitespace.
    CaseId,
    "case id",
    "non-empty, ASCII, no whitespace",
    |s: &str| !s.is_empty() && s.is_ascii() && !s.contains(char::is_whitespace)
);

string_id!(
    /// A verdict-bearing capability name from the capability matrix
    /// (e.g. `EhrOperations`).
    CapabilityName,
    "capability name",
    "ASCII identifier",
    is_ident
);

string_id!(
    /// A corpus manifest key (`cnf.composition.minimal_event.v1`): lowercase
    /// dotted segments.
    CorpusKey,
    "corpus key",
    "dot-separated lowercase segments of [a-z0-9_-]",
    |s: &str| {
        !s.is_empty()
            && s.split('.').all(|seg| {
                !seg.is_empty()
                    && seg.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
            })
    }
);

string_id!(
    /// An option tag realizing one branch of an `option_select` ambiguity
    /// (e.g. `adl14-duplicate-conflict`).
    OptionTag,
    "option tag",
    "non-empty [a-z0-9_-]",
    |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
);

string_id!(
    /// A named SUT instance from `ixit.json` (the flow `on:` selector;
    /// default `sut`).
    InstanceName,
    "instance name",
    "ASCII identifier",
    is_ident
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_key_shape() {
        assert!(CorpusKey::parse("cnf.composition.minimal_event.v1").is_ok());
        assert!(CorpusKey::parse("cnf.set.bp-10").is_ok());
        assert!(CorpusKey::parse("Cnf.Upper").is_err());
        assert!(CorpusKey::parse("cnf..double_dot").is_err());
    }

    /// The string-id newtypes carry the same three seams, and the error names
    /// the identifier kind and its rule so a bad artifact says which space it
    /// violated.
    #[test]
    fn a_string_id_round_trips_and_its_error_names_the_rule() {
        let name: CapabilityName = "EhrOperations".parse().unwrap();
        assert_eq!(name.as_str(), "EhrOperations");
        assert_eq!(serde_json::to_string(&name).unwrap(), "\"EhrOperations\"");
        let error = CapabilityName::parse("2fast").expect_err("a leading digit is not an ident");
        assert_eq!(
            error.to_string(),
            "invalid capability name: \"2fast\" (ASCII identifier)"
        );
    }
}
