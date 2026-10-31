// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The refusal a malformed server-function call gets back.
//!
//! Every `#[server]` fn here is a publicly reachable HTTP endpoint, so its
//! refusals are part of the interface. A call whose arguments cannot be
//! decoded never reaches its handler: `server_fn` builds that response itself
//! and documents the status it uses, which is 500 for every error it carries
//! (<https://docs.rs/leptos/latest/leptos/server_fn/response/trait.Res.html>:
//! "sets the HTTP status code to `500`"). A caller's mistake therefore reads
//! as the server breaking, and on a public instrument that is the difference
//! between "you sent the wrong thing" and "this thing is down".
//!
//! The framework offers no hook for it, so the rewrite happens in a layer this
//! crate owns, over the decoded [`ConsoleError`] and the number of body bytes
//! the transport read.

use leptos::prelude::ServerFnErrorErr;

use crate::error::ConsoleError;

/// How much of a call's body the transport read, against the cap it holds.
///
/// The count is taken below the framework's limited reader, so it is the
/// number of bytes the request yielded before the read stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyRead {
    /// The bytes of the body the server read.
    pub bytes: usize,
    /// The transport cap the server reads a call's body under.
    pub cap: usize,
}

impl BodyRead {
    /// Returns whether the body ran past the cap, which ends the read before
    /// any argument is decoded.
    #[must_use]
    pub fn overran(self) -> bool {
        self.bytes > self.cap
    }
}

/// Reads a server-function error as the caller's mistake, when it is one.
///
/// `Some` when the transport failed to decode the call: an argument that will
/// not deserialize, one missing outright, or a body that is not the expected
/// encoding at all, the three [`ServerFnErrorErr`] variants raised before a
/// handler runs
/// (<https://docs.rs/leptos/latest/leptos/prelude/enum.ServerFnErrorErr.html>).
/// The body's own size decides between the two refusals: the framework's
/// limited reader reports an overrun as a decoding failure, so a body that ran
/// past the cap is [`ConsoleError::CallTooLarge`] whatever the message says.
/// `None` for every other error, which keeps its own variant.
#[must_use]
pub fn caller_fault(error: &ConsoleError, read: BodyRead) -> Option<ConsoleError> {
    let ConsoleError::Transport(
        ServerFnErrorErr::Args(detail)
        | ServerFnErrorErr::MissingArg(detail)
        | ServerFnErrorErr::Deserialization(detail),
    ) = error
    else {
        return None;
    };
    if read.overran() {
        return Some(ConsoleError::CallTooLarge {
            cap: cap_bytes(read.cap),
        });
    }
    Some(ConsoleError::CallUndecodable {
        missing: missing_field(detail).map(str::to_owned),
    })
}

/// The transport cap as the refusal carries it.
#[expect(
    clippy::expect_used,
    reason = "usize is at most 64 bits on every target this console builds for, so a usize always fits a u64"
)]
fn cap_bytes(cap: usize) -> u64 {
    u64::try_from(cap).expect("a usize cap should fit a u64")
}

/// The field name out of serde's own phrasing, when it carries one.
///
/// serde writes the phrase "missing field" followed by the name in backticks
/// for an absent member, which is the only shape worth reading: anything else
/// is reported as an argument that could not be decoded, rather than guessed
/// at.
fn missing_field(detail: &str) -> Option<&str> {
    let rest = detail.split_once("missing field `")?.1;
    let name = rest.split_once('`')?.0;
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A body that stayed inside a 4 MiB cap.
    const READ: BodyRead = BodyRead {
        bytes: 512,
        cap: 4_194_304,
    };

    fn transport(error: ServerFnErrorErr) -> ConsoleError {
        ConsoleError::Transport(error)
    }

    #[test]
    fn a_missing_argument_is_the_callers_fault_and_names_itself() {
        let fault = caller_fault(
            &transport(ServerFnErrorErr::Args(String::from(
                "missing field `postures`",
            ))),
            READ,
        )
        .expect("a decoding failure is the caller's fault");
        assert_eq!(
            fault,
            ConsoleError::CallUndecodable {
                missing: Some(String::from("postures"))
            }
        );
        let answer = fault.to_string();
        assert!(answer.contains("`postures`"), "{answer}");
        assert!(answer.contains("required"), "{answer}");
        assert!(
            !answer.contains("Args"),
            "the serializer's own phrasing does not travel: {answer}"
        );
    }

    #[test]
    fn an_undecodable_argument_says_so_without_inventing_a_name() {
        let fault = caller_fault(
            &transport(ServerFnErrorErr::Deserialization(String::from(
                "invalid type: string \"x\", expected u32",
            ))),
            READ,
        )
        .expect("a decoding failure is the caller's fault");
        assert_eq!(fault, ConsoleError::CallUndecodable { missing: None });
        let answer = fault.to_string();
        assert!(answer.contains("could not be decoded"), "{answer}");
        assert!(
            !answer.contains("The argument `"),
            "nothing is named when serde named nothing: {answer}"
        );
    }

    #[test]
    fn a_missing_arg_variant_is_read_the_same_way() {
        assert!(
            caller_fault(
                &transport(ServerFnErrorErr::MissingArg(String::from(
                    "no argument named `filter`"
                ))),
                READ
            )
            .is_some()
        );
    }

    #[test]
    fn the_servers_own_errors_are_left_alone() {
        assert_eq!(
            caller_fault(
                &transport(ServerFnErrorErr::ServerError(String::from(
                    "the engine exited 2"
                ))),
                READ
            ),
            None
        );
        assert_eq!(caller_fault(&ConsoleError::NoDraft, READ), None);
        assert_eq!(
            caller_fault(
                &ConsoleError::Internal {
                    cause: crate::error::InternalCause::Lock
                },
                READ
            ),
            None
        );
    }

    /// The overrun is told apart by the bytes the transport read, never by
    /// the reader's message, and its sentence names the cap.
    #[test]
    fn a_body_past_the_cap_is_too_large_whatever_the_message_says() {
        let past = BodyRead {
            bytes: 4_194_305,
            cap: 4_194_304,
        };
        let fault = caller_fault(
            &transport(ServerFnErrorErr::Deserialization(String::from(
                "anything at all",
            ))),
            past,
        )
        .expect("a decoding failure is the caller's fault");
        assert_eq!(fault, ConsoleError::CallTooLarge { cap: 4_194_304 });
        let answer = fault.to_string();
        assert!(answer.contains("larger than the 4194304 bytes"), "{answer}");
        assert!(!answer.contains("could not be decoded"), "{answer}");
    }

    #[test]
    fn a_body_exactly_at_the_cap_is_judged_by_its_content() {
        let at = BodyRead {
            bytes: 4_194_304,
            cap: 4_194_304,
        };
        assert_eq!(
            caller_fault(
                &transport(ServerFnErrorErr::Deserialization(String::from(
                    "expected u32"
                ))),
                at
            ),
            Some(ConsoleError::CallUndecodable { missing: None })
        );
    }

    #[test]
    fn the_servers_own_error_stays_its_own_after_an_overrun() {
        let past = BodyRead {
            bytes: 9_000_000,
            cap: 4_194_304,
        };
        assert_eq!(
            caller_fault(
                &transport(ServerFnErrorErr::ServerError(String::from(
                    "the engine exited 2"
                ))),
                past
            ),
            None
        );
    }
}
