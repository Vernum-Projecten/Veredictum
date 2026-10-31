// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The instrument's approval status, read from the linked engine for the
//! shell's status banner.
//!
//! The engine fixes the status at compile time from its own version
//! (`veredictum::party::InstrumentStatus`), so the console reads it there and
//! carries no copy of its own. No openEHR spec governs this — our own design.

use serde::{Deserialize, Serialize};

/// The status line the shell banner renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusLine {
    /// The status token, as the results document records it.
    pub token: String,
    /// The status as a plain sentence fragment.
    pub summary: String,
    /// Whether this console issues certificates, which every page states.
    pub issuance: crate::certificates::IssuanceMode,
}

#[cfg(feature = "ssr")]
impl StatusLine {
    /// The status of the engine this console is built against, and this
    /// console's issuance mode.
    #[must_use]
    pub fn of_engine(issuance: crate::certificates::IssuanceMode) -> Self {
        let status = veredictum::party::InstrumentStatus::CURRENT;
        Self {
            token: status.token().to_owned(),
            summary: status.summary().to_owned(),
            issuance,
        }
    }
}

pub mod fns {
    //! The `#[server]` endpoint, one module for one inner suppression.
    //!
    //! The suppression covers what the macro expands: `unused_async` and
    //! `unused_async_trait_impl` on the async fn it wraps in a trait impl, and
    //! `missing_docs` on the argument struct it mints. Both fire only in SOME
    //! expansions, so `#[expect]` would itself warn through
    //! `unfulfilled_lint_expectations` in the quiet ones
    //! (<https://doc.rust-lang.org/rustc/lints/levels.html#expect>).
    #![allow(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        missing_docs,
        reason = "fires only in some #[server] expansions; see the module doc"
    )]

    use super::StatusLine;
    use crate::error::ConsoleError;
    use leptos::prelude::server;

    /// The linked engine's approval status, and this console's issuance
    /// mode.
    ///
    /// # Errors
    /// The server-fn transport only; both are startup facts.
    #[server]
    pub async fn fetch_status_line() -> Result<StatusLine, ConsoleError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        Ok(StatusLine::of_engine(state.jobs.issuance_mode()))
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::StatusLine;

    #[test]
    fn the_line_is_the_engine_status() {
        let status = veredictum::party::InstrumentStatus::CURRENT;
        let line = StatusLine::of_engine(crate::certificates::IssuanceMode::Trial);
        assert_eq!(line.token, status.token());
        assert_eq!(line.summary, status.summary());
    }

    #[test]
    fn the_line_says_trial_mode_on_a_console_that_issues_nothing() {
        let trial = StatusLine::of_engine(crate::certificates::IssuanceMode::Trial);
        assert_eq!(trial.issuance, crate::certificates::IssuanceMode::Trial);
        assert!(
            trial
                .issuance
                .notice()
                .is_some_and(|notice| notice.contains("no certificate is issued"))
        );
        assert_eq!(crate::certificates::IssuanceMode::Issuing.notice(), None);
        assert_eq!(
            crate::run_job::JobSlot::default().issuance_mode(),
            crate::certificates::IssuanceMode::Trial,
            "a console is in trial mode unless startup switched issuance on"
        );
    }
}
