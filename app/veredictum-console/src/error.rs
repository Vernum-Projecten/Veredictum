// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console's one error type, and the one module that words a refusal.
//!
//! Every `#[server]` fn answers with [`ConsoleError`], so a screen branches on
//! the variant and never on a message. The type implements
//! [`FromServerFnError`], encoded as JSON, so the variant a server function
//! raised is the variant the browser decodes
//! (<https://book.leptos.dev/server/25_server_functions.html>). The refusal
//! layer in the router reads the same encoding back to give each variant its
//! HTTP status (`ConsoleError::status`, compiled with the `ssr` feature).
//!
//! The wording a screen shows around a refusal lives here too: what the
//! refused act left unchanged ([`Act`]) and what to do next
//! ([`ConsoleError::next_step`]), so two screens never describe one refusal
//! two ways.

use leptos::prelude::{FromServerFnError, ServerFnErrorErr};
use leptos::server_fn::codec::JsonEncoding;
use serde::{Deserialize, Serialize};

/// Everything a console server function can answer instead of its value.
///
/// NOTE: no openEHR spec governs this — our own design; each variant is one
/// refusal family a screen words or routes differently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ConsoleError {
    /// This submitter's budget for a metered action is spent.
    #[error("{detail}")]
    RateBudget {
        /// How long until the budget reopens, in seconds.
        retry_in_secs: u64,
        /// The ledger's own sentence.
        detail: String,
    },
    /// The posture's target guard refused the address.
    #[error("{detail}")]
    Target {
        /// The guard's own sentence, naming the address family it refused.
        detail: String,
    },
    /// A value is longer than the console accepts for it.
    #[error("the {field} is {bytes} bytes; the cap is {cap}")]
    TooLong {
        /// What the value is, as a sentence names it.
        field: String,
        /// Its length, in bytes.
        bytes: u64,
        /// The cap, in bytes.
        cap: u64,
    },
    /// The pasted statement was refused: past its cap, not JSON, outside its
    /// published schema, or not a statement.
    #[error("{detail}")]
    Statement {
        /// The reader's finding, verbatim.
        detail: String,
    },
    /// An argument names nothing the console can use: an unknown profile or
    /// capability, an empty selection, a filter that selects nothing.
    #[error("{detail}")]
    Invalid {
        /// What is wrong with it.
        detail: String,
    },
    /// There is no connection draft to work on.
    #[error("no connection draft: complete the Connect step first")]
    NoDraft,
    /// This submitter already has a run in flight, so no other starts.
    #[error(
        "you already have a run in flight (run {run}); watch it or cancel it before starting another"
    )]
    RunInFlight {
        /// The run already in flight.
        run: String,
    },
    /// What the call names does not exist on this instance.
    #[error("{detail}")]
    NotFound {
        /// What was not found.
        detail: String,
    },
    /// The mounted catalogue did not load.
    #[error("{detail}")]
    CatalogueUnavailable {
        /// The load failure, verbatim.
        detail: String,
    },
    /// This instance failed: a poisoned lock, a file it could not write, an
    /// engine it could not start.
    ///
    /// The detail stays in the instance's log: it names paths on the host,
    /// which a public instance does not show its visitors.
    #[error(
        "This instance could not complete the request ({}); the detail is in its log.",
        cause.phrase()
    )]
    Internal {
        /// What kind of failure it was, never the detail itself.
        cause: InternalCause,
    },
    /// The call's body is larger than the transport cap, so none of its
    /// arguments were decoded.
    #[error(
        "This call could not be read. Its body is larger than the {cap} bytes this console reads for one call, so none of its arguments were decoded."
    )]
    CallTooLarge {
        /// The cap, in bytes.
        cap: u64,
    },
    /// The call's body was read, and an argument was missing or did not
    /// decode.
    #[error("{}", undecodable(missing.as_deref()))]
    CallUndecodable {
        /// The argument serde named as missing, when it named one.
        missing: Option<String>,
    },
    /// The server-function transport failed before or after the handler.
    #[error("{0}")]
    Transport(ServerFnErrorErr),
}

/// What kind of failure an [`ConsoleError::Internal`] was, as a visitor may
/// read it: the kind names no path, no address and no body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InternalCause {
    /// A lock a panicking thread poisoned.
    Lock,
    /// A file under the output mount that could not be written or read.
    Storage,
    /// The engine binary could not be found, started or read.
    Engine,
    /// A document this instance composes or reads did not serialize or parse.
    Document,
    /// The target guard could not run.
    TargetCheck,
}

impl InternalCause {
    /// The phrase the public sentence carries.
    #[must_use]
    pub fn phrase(self) -> &'static str {
        match self {
            Self::Lock => "its shared state is poisoned",
            Self::Storage => "a file under its output mount failed",
            Self::Engine => "the engine failed",
            Self::Document => "a document it composes or reads failed",
            Self::TargetCheck => "the target check could not run",
        }
    }
}

/// The sentence an undecodable call reads as.
fn undecodable(missing: Option<&str>) -> String {
    let named = missing.map_or_else(
        || String::from(" One of its arguments could not be decoded."),
        |argument| format!(" The argument `{argument}` is required and was not supplied."),
    );
    format!(
        "This call could not be read.{named} Every argument of a console endpoint is required: a value that is not being declared has its own spelling (an empty string, or the `Undeclared` member of its vocabulary), so that an omitted argument can never read as a declared absence."
    )
}

/// Writes an internal failure's detail to standard error, which the host's
/// log driver keeps, at warn level.
#[cfg(feature = "ssr")]
#[expect(
    clippy::print_stderr,
    reason = "the container's log stream is the instance's log; the line carries this instance's own diagnostic and never a body a server under test sent"
)]
fn log_warn(cause: InternalCause, detail: &str) {
    eprintln!("veredictum-console: warn: {}: {detail}", cause.phrase());
}

/// The browser keeps no log of the instance's own failures.
#[cfg(not(feature = "ssr"))]
fn log_warn(_cause: InternalCause, _detail: &str) {}

impl FromServerFnError for ConsoleError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        Self::Transport(value)
    }
}

impl ConsoleError {
    /// An [`ConsoleError::Internal`] of `cause`, with `detail` written to the
    /// instance's log at warn level and nowhere else.
    ///
    /// The detail is this instance's own diagnostic (a path, a lock, a parse
    /// failure), never a body a server under test sent.
    #[must_use]
    pub fn internal(cause: InternalCause, detail: impl std::fmt::Display) -> Self {
        log_warn(cause, &detail.to_string());
        Self::Internal { cause }
    }

    /// An [`ConsoleError::Invalid`] carrying `detail`.
    #[must_use]
    pub fn invalid(detail: impl std::fmt::Display) -> Self {
        Self::Invalid {
            detail: detail.to_string(),
        }
    }

    /// An [`ConsoleError::Statement`] carrying `detail`.
    #[must_use]
    pub fn statement(detail: impl std::fmt::Display) -> Self {
        Self::Statement {
            detail: detail.to_string(),
        }
    }

    /// A [`ConsoleError::TooLong`] for `field` at `bytes` against `cap`.
    #[must_use]
    pub fn too_long(field: &str, bytes: u64, cap: u64) -> Self {
        Self::TooLong {
            field: field.to_owned(),
            bytes,
            cap,
        }
    }

    /// Whether the call itself was the caller's mistake, before any handler
    /// judged its arguments.
    #[must_use]
    pub fn is_unreadable_call(&self) -> bool {
        matches!(
            self,
            Self::CallTooLarge { .. } | Self::CallUndecodable { .. }
        )
    }

    /// What to do next, as every screen words it.
    #[must_use]
    pub fn next_step(&self) -> &'static str {
        match self {
            Self::RateBudget { .. } => "Wait until the budget reopens, then try again.",
            Self::Target { .. } => {
                "Name a target reachable from this instance, or run the console yourself next to a private one."
            }
            Self::TooLong { .. } => "Shorten it and try again.",
            Self::Statement { .. } => "Correct the statement and save it again.",
            Self::Invalid { .. } => "Correct it and try again.",
            Self::NoDraft => "Probe the connection on the Connect step first.",
            Self::RunInFlight { .. } => "Watch that run, or cancel it and start again.",
            Self::NotFound { .. } => "Check the address it was asked for.",
            Self::CatalogueUnavailable { .. } => "Check the mounted catalogue root.",
            Self::Internal { .. } => "Try again; if it persists, the console needs a restart.",
            Self::CallTooLarge { .. } => "Send less in one call.",
            Self::CallUndecodable { .. } => "Reload the page and try again.",
            Self::Transport(_) => "Check the connection to this console, then try again.",
        }
    }

    /// The HTTP status this refusal answers with.
    ///
    /// NOTE: RFC 9110 §15.5 — a caller's mistake is a 4xx; only this
    /// instance's own failure, and the transport's, stays 500.
    #[cfg(feature = "ssr")]
    #[must_use]
    pub fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::RateBudget { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Target { .. }
            | Self::TooLong { .. }
            | Self::Statement { .. }
            | Self::Invalid { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::NoDraft | Self::RunInFlight { .. } => StatusCode::CONFLICT,
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::CallTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::CallUndecodable { .. } => StatusCode::BAD_REQUEST,
            Self::CatalogueUnavailable { .. } | Self::Internal { .. } | Self::Transport(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

/// A mutation a screen reports the refusal of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// The connect step's probe.
    Probe,
    /// The connect step's Continue, which stores the identity labels.
    SaveIdentity,
    /// The scope step's save.
    SaveScope,
    /// The choice of specification profile.
    SaveProfile,
    /// The composed claim.
    Compose,
    /// The start of a run.
    Start,
    /// The cancel of a run.
    Cancel,
}

impl Act {
    /// The notification's title.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Probe => "The connection was refused",
            Self::SaveIdentity => "The identity was not saved",
            Self::SaveScope => "The scope was refused",
            Self::SaveProfile => "The specification was refused",
            Self::Compose => "The claim was not composed",
            Self::Start => "The run was refused",
            Self::Cancel => "The cancel was refused",
        }
    }

    /// What the refusal left as it was.
    #[must_use]
    pub fn unchanged(self) -> &'static str {
        match self {
            Self::Probe => "No connection was stored",
            Self::SaveIdentity => "The connection draft is unchanged",
            Self::SaveScope => "The claim was not stored",
            Self::SaveProfile => "The run keeps the specification it had",
            Self::Compose => "Nothing was composed",
            Self::Start => "Nothing was driven",
            Self::Cancel => "The run is as it was",
        }
    }

    /// The notification's body for `error`: what is unchanged, the
    /// diagnostic verbatim, and what to do next.
    #[must_use]
    pub fn refusal(self, error: &ConsoleError) -> String {
        let diagnostic = error.to_string();
        format!(
            "{}: {}. {}",
            self.unchanged(),
            diagnostic.trim_end_matches('.'),
            error.next_step()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Act, ConsoleError, FromServerFnError as _, ServerFnErrorErr};

    /// One of every variant, so a test covers each.
    fn every() -> Vec<ConsoleError> {
        vec![
            ConsoleError::RateBudget {
                retry_in_secs: 30,
                detail: String::from("too many probes"),
            },
            ConsoleError::Target {
                detail: String::from("loopback"),
            },
            ConsoleError::too_long("display name", 2_000, 1_024),
            ConsoleError::statement("not JSON"),
            ConsoleError::invalid("no capability"),
            ConsoleError::NoDraft,
            ConsoleError::RunInFlight {
                run: String::from("3f2504e0-4f89-41d3-9a0c-0305e82c3301"),
            },
            ConsoleError::NotFound {
                detail: String::from("no run is in flight"),
            },
            ConsoleError::CatalogueUnavailable {
                detail: String::from("no catalogue"),
            },
            ConsoleError::Internal {
                cause: super::InternalCause::Lock,
            },
            ConsoleError::CallTooLarge { cap: 4_194_304 },
            ConsoleError::CallUndecodable {
                missing: Some(String::from("postures")),
            },
            ConsoleError::Transport(ServerFnErrorErr::Request(String::from("offline"))),
        ]
    }

    /// The variant a server function raises is the variant the browser
    /// decodes: the encoding round-trips every one.
    #[test]
    fn every_variant_round_trips_through_its_encoding() {
        for error in every() {
            assert_eq!(ConsoleError::de(error.ser()), error);
        }
    }

    /// A body that is not this encoding decodes as the transport's own
    /// failure, never as a domain refusal.
    #[test]
    fn a_foreign_body_decodes_as_a_transport_failure() {
        let decoded = ConsoleError::de(leptos::server_fn::Bytes::from_static(b"not json"));
        assert!(
            matches!(
                decoded,
                ConsoleError::Transport(ServerFnErrorErr::Deserialization(_))
            ),
            "{decoded:?}"
        );
    }

    /// An internal failure tells a visitor its kind and nothing it carried:
    /// the detail, a host path here, goes to the instance's log only.
    #[test]
    fn an_internal_failure_carries_no_detail_to_the_browser() {
        let error = ConsoleError::internal(
            super::InternalCause::Storage,
            "/srv/veredictum/out/3f25/ixit.json: permission denied",
        );
        let shown = error.to_string();
        let encoded = String::from_utf8_lossy(&error.ser()).into_owned();
        for leaked in ["/srv", "ixit.json", "permission denied"] {
            assert!(!shown.contains(leaked), "{shown}");
            assert!(!encoded.contains(leaked), "{encoded}");
        }
        assert!(shown.contains("output mount"), "{shown}");
    }

    #[test]
    fn a_too_long_value_names_its_field_size_and_cap() {
        assert_eq!(
            ConsoleError::too_long("display name", 2_000, 1_024).to_string(),
            "the display name is 2000 bytes; the cap is 1024"
        );
    }

    #[test]
    fn an_undecodable_call_names_the_missing_argument_only_when_serde_did() {
        let named = ConsoleError::CallUndecodable {
            missing: Some(String::from("postures")),
        }
        .to_string();
        assert!(named.contains("`postures`"), "{named}");
        assert!(named.contains("required"), "{named}");
        let unnamed = ConsoleError::CallUndecodable { missing: None }.to_string();
        assert!(unnamed.contains("could not be decoded"), "{unnamed}");
        assert!(!unnamed.contains("The argument `"), "{unnamed}");
    }

    /// The refusal body says what is unchanged, quotes the diagnostic once,
    /// and ends on the variant's own next step.
    #[test]
    fn a_refusal_body_is_unchanged_diagnostic_and_next_step() {
        let body = Act::Start.refusal(&ConsoleError::NoDraft);
        assert_eq!(
            body,
            "Nothing was driven: no connection draft: complete the Connect step first. Probe the connection on the Connect step first."
        );
        let trailing = Act::Probe.refusal(&ConsoleError::CallTooLarge { cap: 8 });
        assert!(!trailing.contains(".."), "{trailing}");
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn every_caller_refusal_is_a_4xx_and_only_failures_are_5xx() {
        for error in every() {
            let status = error.status();
            let failure = matches!(
                error,
                ConsoleError::CatalogueUnavailable { .. }
                    | ConsoleError::Internal { .. }
                    | ConsoleError::Transport(_)
            );
            assert_eq!(status.is_server_error(), failure, "{error:?} -> {status}");
            assert_eq!(status.is_client_error(), !failure, "{error:?} -> {status}");
        }
    }
}
