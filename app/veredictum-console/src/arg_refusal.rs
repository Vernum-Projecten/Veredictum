// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The status and the sentence a malformed server-function call gets back.
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
//! crate owns, over the encoded error and the number of body bytes the
//! transport read.

use std::fmt::Write as _;

/// The decoding failures that are the CALLER's mistake.
///
/// `server_fn` encodes an error as its variant name, a `|`, and the message
/// (`ServerFnError::ser`), and these three variants are the ones raised before
/// a handler runs: an argument that will not deserialize, one that is missing
/// outright, and a body that is not the expected encoding at all
/// (<https://docs.rs/leptos/latest/leptos/prelude/enum.ServerFnErrorErr.html>).
const CALLER_FAULT: [&str; 3] = ["Args", "MissingArg", "Deserialization"];

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

/// Why a call is the caller's mistake, each cause with its own sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallerFault {
    /// The body was larger than the transport cap, so it was never decoded.
    TooLarge {
        /// The cap, in bytes.
        cap: usize,
    },
    /// The body was read, and an argument was missing or did not decode.
    Undecodable {
        /// The argument serde named as missing, when it named one.
        missing: Option<String>,
    },
}

impl CallerFault {
    /// Returns the sentence a caller gets instead of the serializer's own
    /// words.
    #[must_use]
    pub fn sentence(&self) -> String {
        match self {
            Self::TooLarge { cap } => format!(
                "This call could not be read. Its body is larger than the \
                 {cap} bytes this console reads for one call, so none of its \
                 arguments were decoded."
            ),
            Self::Undecodable { missing } => {
                let mut sentence = String::from("This call could not be read.");
                if let Some(argument) = missing {
                    let _ = write!(
                        sentence,
                        " The argument `{argument}` is required and was not supplied."
                    );
                } else {
                    sentence.push_str(" One of its arguments could not be decoded.");
                }
                sentence.push_str(
                    " Every argument of a console endpoint is required: a value that is \
                     not being declared has its own spelling — an empty string, or the \
                     `Undeclared` member of its vocabulary — so that an omitted argument \
                     can never read as a declared absence.",
                );
                sentence
            }
        }
    }
}

/// Classifies an encoded server-function error as the caller's mistake.
///
/// `Some` when the encoded error names a decoding failure. The body's own
/// size decides between the two causes: the framework's limited reader
/// reports an overrun as a decoding failure, so a body that ran past the cap
/// is [`CallerFault::TooLarge`] whatever the message says. `None` when the
/// error is the server's own, which keeps its status and its body untouched.
#[must_use]
pub fn caller_fault(encoded: &str, read: BodyRead) -> Option<CallerFault> {
    let (variant, detail) = encoded.split_once('|')?;
    if !CALLER_FAULT.contains(&variant) {
        return None;
    }
    if read.overran() {
        return Some(CallerFault::TooLarge { cap: read.cap });
    }
    Some(CallerFault::Undecodable {
        missing: missing_field(detail).map(str::to_owned),
    })
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

    #[test]
    fn a_missing_argument_is_the_callers_fault_and_names_itself() {
        let answer = caller_fault("Args|missing field `postures`", READ)
            .expect("a decoding failure is the caller's fault")
            .sentence();
        assert!(answer.contains("`postures`"), "{answer}");
        assert!(answer.contains("required"), "{answer}");
        assert!(
            !answer.contains("Args|"),
            "the serializer's own phrasing does not travel: {answer}"
        );
    }

    #[test]
    fn an_undecodable_argument_says_so_without_inventing_a_name() {
        let answer = caller_fault(
            "Deserialization|invalid type: string \"x\", expected u32",
            READ,
        )
        .expect("a decoding failure is the caller's fault")
        .sentence();
        assert!(answer.contains("could not be decoded"), "{answer}");
        assert!(
            !answer.contains("The argument `"),
            "nothing is named when serde named nothing: {answer}"
        );
    }

    #[test]
    fn a_missing_arg_variant_is_read_the_same_way() {
        assert!(caller_fault("MissingArg|no argument named `filter`", READ).is_some());
    }

    #[test]
    fn the_servers_own_errors_are_left_alone() {
        assert_eq!(caller_fault("ServerError|the engine exited 2", READ), None);
        assert_eq!(caller_fault("WrappedServerError|refused", READ), None);
        assert_eq!(caller_fault("no separator at all", READ), None);
    }

    /// The overrun is told apart by the bytes the transport read, never by
    /// the reader's message, and its sentence names the cap.
    #[test]
    fn a_body_past_the_cap_is_too_large_whatever_the_message_says() {
        let past = BodyRead {
            bytes: 4_194_305,
            cap: 4_194_304,
        };
        let fault = caller_fault("Deserialization|anything at all", past)
            .expect("a decoding failure is the caller's fault");
        assert_eq!(fault, CallerFault::TooLarge { cap: 4_194_304 });
        let answer = fault.sentence();
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
            caller_fault("Deserialization|expected u32", at),
            Some(CallerFault::Undecodable { missing: None })
        );
    }

    #[test]
    fn the_servers_own_error_stays_its_own_after_an_overrun() {
        let past = BodyRead {
            bytes: 9_000_000,
            cap: 4_194_304,
        };
        assert_eq!(caller_fault("ServerError|the engine exited 2", past), None);
    }
}
