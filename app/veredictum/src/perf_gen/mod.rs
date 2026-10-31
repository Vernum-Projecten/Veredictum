// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The open-loop load generator.
//!
//! Interactions arrive on a seeded process laid over a rate profile
//! ([`arrivals`]), each one a single operation or a partly-open session: the
//! session's first stage is intended at its arrival instant and every later
//! stage at the previous answer plus a drawn think time (Schroeder, Wierman
//! and Harchol-Balter, "Open Versus Closed: A Cautionary Tale", NSDI 2006).
//! Every latency runs from the INTENDED instant, never from the send, so a
//! stalled server cannot hide behind the requests it delayed (the
//! coordinated-omission correction of wrk2,
//! <https://github.com/giltene/wrk2#readme>). The arrivals are tasks on a
//! `tokio` runtime with a bounded in-flight budget ([`drive`]), and the
//! generator measures its own lag (actual send minus intended) so a plateau
//! it could not hold is named rather than published.
//!
//! The generator knows operations only by [`OperationId`]: a caller supplies
//! a [`Performer`] that turns one [`Call`] into one wire exchange and says
//! how it ended. No openEHR specification governs this: our own design.

pub mod arrivals;
pub mod drive;
pub mod pool;

use std::fmt;
use std::future::Future;
use std::time::Duration;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The name of one operation the workload offers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OperationId(String);

impl OperationId {
    /// Builds an operation id, refusing an empty name or one with
    /// whitespace or a control character.
    ///
    /// # Errors
    /// [`MixError::OperationId`] naming the refused text.
    pub fn new(name: &str) -> Result<Self, MixError> {
        if name.is_empty() || name.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(MixError::OperationId(name.to_owned()));
        }
        Ok(Self(name.to_owned()))
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for OperationId {
    type Error = MixError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<OperationId> for String {
    fn from(id: OperationId) -> Self {
        id.0
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The name of one interaction in a [`Mix`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InteractionId(String);

impl InteractionId {
    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A later stage of a session: the mean think time before it, and its
/// operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUp {
    /// The mean of the exponential think time drawn before this stage.
    pub think_mean: Duration,
    /// The operation this stage performs.
    pub operation: OperationId,
}

/// One kind of interaction: a single operation, or a session of stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interaction {
    id: InteractionId,
    weight: u32,
    first: OperationId,
    follow_ups: Vec<FollowUp>,
}

impl Interaction {
    /// An interaction of one operation.
    ///
    /// # Errors
    /// [`MixError::Weight`] for a zero weight, [`MixError::InteractionId`]
    /// for an empty id.
    pub fn single(id: &str, operation: OperationId, weight: u32) -> Result<Self, MixError> {
        Self::session(id, weight, operation, Vec::new())
    }

    /// A partly-open session: `first` at the arrival instant, then each
    /// follow-up after the previous answer and its think time.
    ///
    /// # Errors
    /// [`MixError::Weight`] for a zero weight, [`MixError::InteractionId`]
    /// for an empty id.
    pub fn session(
        id: &str,
        weight: u32,
        first: OperationId,
        follow_ups: Vec<FollowUp>,
    ) -> Result<Self, MixError> {
        if id.is_empty() {
            return Err(MixError::InteractionId);
        }
        if weight == 0 {
            return Err(MixError::Weight(id.to_owned()));
        }
        Ok(Self {
            id: InteractionId(id.to_owned()),
            weight,
            first,
            follow_ups,
        })
    }

    /// The interaction's id.
    #[must_use]
    pub fn id(&self) -> &InteractionId {
        &self.id
    }

    /// The interaction's weight in its mix.
    #[must_use]
    pub fn weight(&self) -> u32 {
        self.weight
    }

    /// The operation of stage `stage`, from zero.
    #[must_use]
    pub fn operation(&self, stage: u32) -> Option<&OperationId> {
        match stage.checked_sub(1) {
            None => Some(&self.first),
            Some(later) => usize::try_from(later)
                .ok()
                .and_then(|i| self.follow_ups.get(i))
                .map(|f| &f.operation),
        }
    }

    /// The follow-up stages, in order.
    #[must_use]
    pub fn follow_ups(&self) -> &[FollowUp] {
        &self.follow_ups
    }
}

/// The weighted interactions a run draws from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mix {
    interactions: Vec<Interaction>,
    total_weight: u64,
}

impl Mix {
    /// Builds a mix, refusing an empty one and a repeated interaction id.
    ///
    /// # Errors
    /// [`MixError::Empty`] or [`MixError::Duplicate`].
    pub fn new(interactions: Vec<Interaction>) -> Result<Self, MixError> {
        if interactions.is_empty() {
            return Err(MixError::Empty);
        }
        let mut seen = std::collections::BTreeSet::new();
        for interaction in &interactions {
            if !seen.insert(interaction.id.as_str()) {
                return Err(MixError::Duplicate(interaction.id.0.clone()));
            }
        }
        let total_weight = interactions.iter().map(|i| u64::from(i.weight)).sum();
        Ok(Self {
            interactions,
            total_weight,
        })
    }

    /// The interactions, in declaration order.
    #[must_use]
    pub fn interactions(&self) -> &[Interaction] {
        &self.interactions
    }

    /// The interaction a 64-bit draw selects, by cumulative weight.
    fn pick(&self, bits: u64) -> usize {
        let scaled = (u128::from(bits) * u128::from(self.total_weight)) >> 64;
        let mut target = u64::try_from(scaled).unwrap_or(u64::MAX);
        for (index, interaction) in self.interactions.iter().enumerate() {
            let weight = u64::from(interaction.weight);
            if target < weight {
                return index;
            }
            target -= weight;
        }
        self.interactions.len().saturating_sub(1)
    }
}

/// A mix the generator refuses.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MixError {
    /// An operation id that is empty or carries whitespace.
    #[error("operation id {0:?} is empty or carries whitespace")]
    OperationId(String),
    /// An interaction with an empty id.
    #[error("an interaction id is empty")]
    InteractionId,
    /// An interaction with weight zero.
    #[error("interaction {0} has weight zero")]
    Weight(String),
    /// A mix with no interaction.
    #[error("a mix needs at least one interaction")]
    Empty,
    /// Two interactions with one id.
    #[error("interaction {0} is declared twice")]
    Duplicate(String),
}

/// Why one exchange did not succeed (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    /// No answer within the client's timeout.
    Timeout,
    /// The connection failed or broke before an answer.
    Transport,
    /// `429 Too Many Requests`: the server rate-limited the instrument.
    RateLimited,
    /// Any other `4xx`.
    ClientError,
    /// A `5xx`.
    ServerError,
    /// A status the operation does not expect that is neither `4xx` nor `5xx`.
    UnexpectedStatus,
    /// Still unanswered when the drain ended; its latency is recorded as the
    /// time it had waited by then, a lower bound.
    Unfinished,
    /// The instrument had nothing to address: an update or delete found no
    /// free target. A defect of the run, never of the server, and it still
    /// counts against the plateau so a starved run cannot qualify.
    NoTarget,
}

impl ErrorClass {
    /// Every class, in serialization order.
    pub const ALL: &[ErrorClass] = &[
        ErrorClass::Timeout,
        ErrorClass::Transport,
        ErrorClass::RateLimited,
        ErrorClass::ClientError,
        ErrorClass::ServerError,
        ErrorClass::UnexpectedStatus,
        ErrorClass::Unfinished,
        ErrorClass::NoTarget,
    ];

    /// The class of an answer whose status the operation did not expect.
    #[must_use]
    pub fn for_status(status: StatusCode) -> Self {
        if status == StatusCode::TOO_MANY_REQUESTS {
            Self::RateLimited
        } else if status.is_client_error() {
            Self::ClientError
        } else if status.is_server_error() {
            Self::ServerError
        } else {
            Self::UnexpectedStatus
        }
    }

    /// The class's serialized token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Transport => "transport",
            Self::RateLimited => "rate_limited",
            Self::ClientError => "client_error",
            Self::ServerError => "server_error",
            Self::UnexpectedStatus => "unexpected_status",
            Self::Unfinished => "unfinished",
            Self::NoTarget => "no_target",
        }
    }
}

impl fmt::Display for ErrorClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.token())
    }
}

/// How many bytes of a failed answer's body a sample keeps.
pub const FAILED_BODY_LIMIT: usize = 512;

/// How many bytes of a request line a sample keeps.
pub const REQUEST_LINE_LIMIT: usize = 1_024;

/// The evidence one failed exchange leaves: its status, request line and
/// the start of its body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailedExchange {
    /// The answer's status, absent when no answer came.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// The request line, `METHOD path`, at most [`REQUEST_LINE_LIMIT`] bytes.
    pub request_line: String,
    /// The start of the answer's body, at most [`FAILED_BODY_LIMIT`] bytes
    /// and cut on a character boundary.
    pub body: String,
    /// Whether the body was cut.
    pub truncated: bool,
}

impl FailedExchange {
    /// Builds a sample, cutting the request line and the body to their
    /// limits on character boundaries.
    #[must_use]
    pub fn new(status: Option<StatusCode>, request_line: &str, body: &str) -> Self {
        let (body_cut, truncated) = cut(body, FAILED_BODY_LIMIT);
        Self {
            status: status.map(|s| s.as_u16()),
            request_line: cut(request_line, REQUEST_LINE_LIMIT).0.to_owned(),
            body: body_cut.to_owned(),
            truncated,
        }
    }
}

/// The longest prefix of `text` within `limit` bytes that ends on a
/// character boundary, and whether anything was cut.
fn cut(text: &str, limit: usize) -> (&str, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text.get(..end).unwrap_or_default(), true)
}

/// How one exchange ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The answer was the one the operation expects.
    Success,
    /// The exchange failed.
    Failure {
        /// Why it failed.
        class: ErrorClass,
        /// The evidence, when the performer kept it.
        exchange: Option<FailedExchange>,
    },
    /// The server refused an operation the release leaves optional, and the
    /// workload counts that refusal apart from errors: neither a success nor
    /// a failure, and no latency is recorded for it.
    Refused,
}

/// One request the generator asks a [`Performer`] to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The arrival this call belongs to.
    pub arrival: u64,
    /// The stage of the interaction, from zero.
    pub stage: u32,
    /// The interaction's position in the mix.
    pub interaction: usize,
    /// The operation to perform.
    pub operation: OperationId,
}

/// The side that turns one [`Call`] into one wire exchange.
///
/// The future it returns runs on the generator's runtime, so it must not
/// block a worker thread: an HTTP performer uses an async client.
pub trait Performer: Send + Sync + 'static {
    /// Performs `call` and says how it ended.
    fn perform(&self, call: &Call) -> impl Future<Output = Outcome> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_maps_to_its_error_class() {
        assert_eq!(
            ErrorClass::for_status(StatusCode::TOO_MANY_REQUESTS),
            ErrorClass::RateLimited
        );
        assert_eq!(
            ErrorClass::for_status(StatusCode::NOT_FOUND),
            ErrorClass::ClientError
        );
        assert_eq!(
            ErrorClass::for_status(StatusCode::BAD_GATEWAY),
            ErrorClass::ServerError
        );
        assert_eq!(
            ErrorClass::for_status(StatusCode::OK),
            ErrorClass::UnexpectedStatus
        );
    }

    #[test]
    fn every_class_token_matches_its_serialization() {
        for class in ErrorClass::ALL {
            let json = serde_json::to_string(class).unwrap();
            assert_eq!(json, format!("\"{}\"", class.token()));
        }
    }

    #[test]
    fn a_failed_exchange_is_cut_on_a_character_boundary() {
        let body = "é".repeat(400);
        let sample = FailedExchange::new(Some(StatusCode::BAD_REQUEST), "POST /ehr", &body);
        assert!(sample.truncated);
        assert_eq!(sample.body.len(), 512);
        assert_eq!(sample.status, Some(400));
        let short = FailedExchange::new(None, "GET /x", "oops");
        assert!(!short.truncated);
        assert_eq!(short.status, None);
    }

    #[test]
    fn a_bad_mix_is_refused() {
        let op = OperationId::new("read").unwrap();
        assert_eq!(Mix::new(vec![]), Err(MixError::Empty));
        assert_eq!(
            Interaction::single("a", op.clone(), 0),
            Err(MixError::Weight("a".to_owned()))
        );
        let a = Interaction::single("a", op.clone(), 1).unwrap();
        assert_eq!(
            Mix::new(vec![a.clone(), a]),
            Err(MixError::Duplicate("a".to_owned()))
        );
        assert!(OperationId::new("has space").is_err());
        assert!(OperationId::new("").is_err());
    }

    #[test]
    fn a_session_names_the_operation_of_each_stage() {
        let first = OperationId::new("open").unwrap();
        let then = OperationId::new("read").unwrap();
        let session = Interaction::session(
            "s",
            1,
            first.clone(),
            vec![FollowUp {
                think_mean: Duration::from_secs(1),
                operation: then.clone(),
            }],
        )
        .unwrap();
        assert_eq!(session.operation(0), Some(&first));
        assert_eq!(session.operation(1), Some(&then));
        assert_eq!(session.operation(2), None);
    }
}
