// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The static half of the condition profile: the request headers a step may
//! send whether or not its operation declares them, and the regex lint.
//!
//! The profile's `simple` rules (integers, booleans and `null` only, never a
//! string) are parsed by [`crate::arazzo::expr`]; this module adds what that
//! parser does not decide.

use crate::arazzo::cite::Citation;

/// A request header the profile lets a step send whether or not the
/// operation declares it.
///
/// Each one is a request header the ITS-REST 1.1.0 docs text names for
/// clients to send, and [`ProfileHeader::citation`] quotes the sentence that
/// names it. OAS 3.0.3 §Parameter Object also tells a tool to ignore a
/// declared `Accept`, `Content-Type` or `Authorization` parameter, so a step
/// cannot rely on the OAS to list those three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProfileHeader {
    /// `Accept`.
    Accept,
    /// `Content-Type`.
    ContentType,
    /// `Authorization`.
    Authorization,
    /// `Proxy-Authorization`.
    ProxyAuthorization,
    /// `Prefer`.
    Prefer,
    /// `If-Match`.
    IfMatch,
    /// `openehr-version`.
    OpenehrVersion,
    /// `openehr-audit-details`.
    OpenehrAuditDetails,
    /// `openehr-item-tag`.
    OpenehrItemTag,
    /// `openehr-version-item-tag`.
    OpenehrVersionItemTag,
    /// `openehr-template-id`.
    OpenehrTemplateId,
    /// `openehr-ehr-id`.
    OpenehrEhrId,
}

const RESOURCES: &str = "specs/openehr/ITS-REST/specifications/docs/overview/Resources.md";
const REQUESTS: &str =
    "specs/openehr/ITS-REST/specifications/docs/overview/Requests_and_responses.md";
const QUERY_REQUEST: &str = "specs/openehr/ITS-REST/specifications/docs/query/Request.md";

impl ProfileHeader {
    /// Every allowed header, in vocabulary order.
    pub const ALL: &[Self] = &[
        Self::Accept,
        Self::ContentType,
        Self::Authorization,
        Self::ProxyAuthorization,
        Self::Prefer,
        Self::IfMatch,
        Self::OpenehrVersion,
        Self::OpenehrAuditDetails,
        Self::OpenehrItemTag,
        Self::OpenehrVersionItemTag,
        Self::OpenehrTemplateId,
        Self::OpenehrEhrId,
    ];

    /// The header's field name, as the ITS-REST overview spells it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Accept => "Accept",
            Self::ContentType => "Content-Type",
            Self::Authorization => "Authorization",
            Self::ProxyAuthorization => "Proxy-Authorization",
            Self::Prefer => "Prefer",
            Self::IfMatch => "If-Match",
            Self::OpenehrVersion => "openehr-version",
            Self::OpenehrAuditDetails => "openehr-audit-details",
            Self::OpenehrItemTag => "openehr-item-tag",
            Self::OpenehrVersionItemTag => "openehr-version-item-tag",
            Self::OpenehrTemplateId => "openehr-template-id",
            Self::OpenehrEhrId => "openehr-ehr-id",
        }
    }

    /// The allowed header named `name`, compared case-insensitively (RFC 9110
    /// §5.1: "Field names are case-insensitive").
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|header| header.name().eq_ignore_ascii_case(name))
    }

    /// The released sentence that names the header as one a client sends.
    #[must_use]
    pub fn citation(self) -> Citation {
        let (source, section, quote) = match self {
            Self::Accept => (
                RESOURCES,
                "Data representation",
                "The client SHOULD use the `Accept: application/json` request header to specify the expected JSON response format.",
            ),
            Self::ContentType => (
                RESOURCES,
                "Data representation",
                "A client MAY use the header `Content-Type: application/json` in the requests to specify the JSON payload format.",
            ),
            Self::Authorization | Self::ProxyAuthorization => (
                REQUESTS,
                "Authentication and authorization",
                "Clients MUST send valid `Authorization` and `Proxy-Authorization` headers in their requests when required.",
            ),
            Self::Prefer => (
                REQUESTS,
                "Prefer",
                "The `Prefer` request header MAY be used by clients to negotiate the format or content of the resource representation in the response.",
            ),
            Self::IfMatch => (
                REQUESTS,
                "If-Match and accidental overwrites",
                "The `If-Match` request header SHOULD be used by the clients with state-changing methods (e.g., `PUT`) to prevent accidental overwrites",
            ),
            Self::OpenehrVersion | Self::OpenehrAuditDetails => (
                REQUESTS,
                "openehr-version and openehr-audit-details",
                "In order to allow clients to provide committal metadata, services MUST accept `openehr-version` and `openehr-audit-details` custom request headers.",
            ),
            Self::OpenehrItemTag | Self::OpenehrVersionItemTag => (
                REQUESTS,
                "Usage in Requests",
                "When used as request headers for creation or update operations (`PUT`, `POST`), the header instructs the system which ITEM_TAG list should be associated with the target VERSION or VERSIONED_OBJECT resource (depending on the header name).",
            ),
            Self::OpenehrTemplateId => (
                REQUESTS,
                "openehr-template-id",
                "The `openehr-template-id` request header MUST be used whenever committing COMPOSITION (via `PUT` or `POST` methods)",
            ),
            Self::OpenehrEhrId => (
                QUERY_REQUEST,
                "About the ehr_id parameter",
                "Depending on the needs, clients MAY supply it as a query parameter `ehr_id` or alternatively as a request header named `openehr-ehr-id`.",
            ),
        };
        Citation {
            source: source.to_owned(),
            section: section.to_owned(),
            quote: quote.to_owned(),
            report: None,
            level: None,
        }
    }
}

/// A `regex` condition the profile refuses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegexBreach {
    /// A lookahead or lookbehind group.
    #[error("`{0}` uses lookaround, which the profile's regex syntax excludes")]
    Lookaround(String),
    /// A numbered or named backreference.
    #[error("`{0}` uses a backreference, which the profile's regex syntax excludes")]
    Backreference(String),
    /// The pattern does not compile, with every embedded expression read as a
    /// literal.
    #[error("`{pattern}` does not compile: {reason}")]
    Syntax {
        /// The pattern.
        pattern: String,
        /// The compiler's message.
        reason: String,
    },
}

/// Checks a `regex` condition against the profile: no lookaround, no
/// backreference, and a pattern that compiles once every embedded `{$…}`
/// expression is replaced by a literal.
///
/// Arazzo 1.1.0 §Criterion Object leaves the regex dialect open; the profile
/// takes the syntax ECMA-262 and the evaluator's engine
/// (<https://docs.rs/regex/1>) share, and that engine supports neither
/// construct.
///
/// # Errors
/// The first [`RegexBreach`] the pattern commits.
pub fn lint_regex(pattern: &str) -> Result<(), RegexBreach> {
    let mut chars = pattern.chars().peekable();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('1'..='9') => return Err(RegexBreach::Backreference(pattern.to_owned())),
                Some('k') if chars.peek() == Some(&'<') => {
                    return Err(RegexBreach::Backreference(pattern.to_owned()));
                }
                _ => {}
            },
            '[' => in_class = true,
            ']' => in_class = false,
            '(' if !in_class && chars.peek() == Some(&'?') => {
                let rest: String = chars.clone().skip(1).take(2).collect();
                if rest.starts_with('=') || rest.starts_with('!') || rest == "<=" || rest == "<!" {
                    return Err(RegexBreach::Lookaround(pattern.to_owned()));
                }
            }
            _ => {}
        }
    }
    regex::Regex::new(&literal_embeddings(pattern))
        .map(drop)
        .map_err(|error| RegexBreach::Syntax {
            pattern: pattern.to_owned(),
            reason: error.to_string(),
        })
}

/// The pattern with every `{$…}` embedding replaced by the literal `x`, the
/// shape the evaluator compiles once each one is substituted and escaped.
fn literal_embeddings(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut rest = pattern;
    while let Some(start) = rest.find("{$") {
        out.push_str(rest.get(..start).unwrap_or_default());
        let tail = rest.get(start..).unwrap_or_default();
        if let Some(end) = tail.find('}') {
            out.push('x');
            rest = tail.get(end.saturating_add(1)..).unwrap_or_default();
        } else {
            out.push_str(tail);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::arazzo::cite::QuoteChecker;

    #[test]
    fn every_allowed_header_quotes_the_released_sentence_that_names_it() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut checker = QuoteChecker::new(&root);
        for header in ProfileHeader::ALL {
            assert_eq!(checker.check(&header.citation()), Ok(()), "{header:?}");
        }
    }

    #[test]
    fn header_names_match_case_insensitively_and_nothing_else_is_allowed() {
        assert_eq!(ProfileHeader::parse("accept"), Some(ProfileHeader::Accept));
        assert_eq!(
            ProfileHeader::parse("OPENEHR-AUDIT-DETAILS"),
            Some(ProfileHeader::OpenehrAuditDetails)
        );
        for seeded in ["If-None-Match", "X-Request-Id", "Acept", ""] {
            assert_eq!(ProfileHeader::parse(seeded), None, "{seeded}");
        }
    }

    #[test]
    fn plain_and_embedded_patterns_pass() {
        for pattern in [
            "^{$inputs.ehr_id}$",
            "^[0-9a-f]{8}-[0-9a-f]{4}$",
            r"^W/\x22.+\x22$",
            r"[(?=]",
            r"\\1",
        ] {
            assert_eq!(lint_regex(pattern), Ok(()), "{pattern}");
        }
    }

    #[test]
    fn seeded_lookaround_backreferences_and_bad_syntax_are_refused() {
        for pattern in ["^(?=a)", "a(?!b)", "(?<=a)b", "(?<!a)b"] {
            assert!(
                matches!(lint_regex(pattern), Err(RegexBreach::Lookaround(_))),
                "{pattern}"
            );
        }
        for pattern in [r"(a)\1", r"(?<n>a)\k<n>"] {
            assert!(
                matches!(lint_regex(pattern), Err(RegexBreach::Backreference(_))),
                "{pattern}"
            );
        }
        assert!(matches!(
            lint_regex("^(unclosed"),
            Err(RegexBreach::Syntax { .. })
        ));
    }
}
