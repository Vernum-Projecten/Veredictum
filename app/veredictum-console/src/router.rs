// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console's HTTP surface: every route the server answers on, the body
//! cap each one carries, and the layer that gives a malformed call a caller's
//! status.
//!
//! It lives in the lib rather than in the binary so the whole router can be
//! bound to a listener and driven over real HTTP by a test, which is the only
//! place a layer's actual reach is observable — the thin-main-over-testable-lib
//! shape the Book recommends
//! (<https://doc.rust-lang.org/book/ch12-03-improving-error-handling-and-testing.html>).

use axum::extract::DefaultBodyLimit;
use leptos::prelude::LeptosOptions;
use leptos_axum::{LeptosRoutes as _, generate_route_list};

use crate::app::{App, shell};
use crate::error::ConsoleError;
use crate::state::ConsoleState;

/// How far URL encoding can inflate the payload a server function judges.
///
/// A `#[server]` call posts its arguments URL-encoded, and percent-encoding
/// spends three bytes on every byte outside the unreserved set, so a JSON
/// statement arrives at up to three times its own length. Four is that worst
/// case with room above it.
const SERVER_FN_INFLATION: u64 = 4;

/// A cap the pages enforce that this host's `usize` cannot hold.
///
/// The server refuses to start on it: the one value a body cap must never
/// fall back to is "unlimited".
#[derive(Debug, thiserror::Error)]
#[error("the {route} body cap of {bytes} bytes does not fit this host's usize")]
pub struct BodyCapTooLarge {
    /// Which reach the cap belongs to.
    pub route: &'static str,
    /// The cap, in bytes.
    pub bytes: u64,
    /// The conversion's own refusal.
    #[source]
    pub source: std::num::TryFromIntError,
}

/// The transport body cap the router installs.
///
/// It is derived from the number the endpoints' own code enforces and sits
/// above it, so every payload an endpoint accepts reaches the code that judges
/// it, and a size refusal within the transport cap is that endpoint's own
/// sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyCaps {
    /// The cap every route carries, the server functions among them.
    pub server_fn: usize,
}

impl BodyCaps {
    /// Derives the cap from the number the pages enforce.
    ///
    /// # Errors
    /// [`BodyCapTooLarge`] when the cap does not fit this host's `usize`.
    pub fn derived() -> Result<Self, BodyCapTooLarge> {
        Ok(Self {
            server_fn: fits(
                "server function",
                SERVER_FN_INFLATION * crate::run_api::read::STATEMENT_CAP_BYTES,
            )?,
        })
    }
}

/// The one u64 to `usize` conversion, named so a refusal says which cap failed.
fn fits(route: &'static str, bytes: u64) -> Result<usize, BodyCapTooLarge> {
    usize::try_from(bytes).map_err(|source| BodyCapTooLarge {
        route,
        bytes,
        source,
    })
}

/// Builds the console's router: the server-owned routes, the Leptos route
/// tree, and the body cap they carry.
///
/// # Errors
/// [`BodyCapTooLarge`] when a cap does not fit this host's `usize`. The caller
/// stops startup on it.
pub fn router(
    state: &ConsoleState,
    leptos_options: LeptosOptions,
) -> Result<axum::Router, BodyCapTooLarge> {
    let caps = BodyCaps::derived()?;
    let routes = generate_route_list(App);

    Ok(axum::Router::new()
        // Outside the Leptos route tree so it answers even when the WASM
        // bundle or the app shell is broken: it claims only "the server
        // accepts connections".
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        // The operator's counters (#489), outside the route tree for the same
        // reason, and behind the operator's bearer token.
        .route(
            crate::metrics::ROUTE,
            axum::routing::get({
                let state = state.clone();
                move |headers: axum::http::HeaderMap| {
                    let state = state.clone();
                    async move { crate::metrics::answer(&state, &headers).await }
                }
            }),
        )
        // The verifying keys, outside the Leptos route tree for the same
        // reason: a verifier needs them whatever state the app shell is in.
        .route(
            crate::certificates::JWKS_ROUTE,
            axum::routing::get(crate::certificates::ssr::jwks),
        )
        .route(
            &format!("{}/{{run}}", crate::certificates::CERTIFICATE_ROUTE),
            axum::routing::get({
                let out = state.out.clone();
                move |axum::extract::Path(run): axum::extract::Path<String>| {
                    let out = out.clone();
                    async move { crate::certificates::ssr::download(&out, &run) }
                }
            }),
        )
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let state = state.clone();
                move || leptos::prelude::provide_context(state.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        // The 404 page renders the shell too, and the shell's status line
        // reads the state, so the fallback provides it like the routes do.
        .fallback(leptos_axum::file_and_error_handler_with_context(
            {
                let state = state.clone();
                move || leptos::prelude::provide_context(state.clone())
            },
            shell,
        ))
        // AFTER the server functions and the fallback, which is the whole
        // point: "the middleware is only applied to existing routes"
        // (<https://docs.rs/axum/latest/axum/routing/struct.Router.html#method.layer>),
        // so this is the first placement from which a `#[server]` call's body
        // limit is anything but axum's 2 MiB default (#496).
        .layer(DefaultBodyLimit::max(caps.server_fn))
        // Also after the server functions, for the same reason: a layer placed
        // higher up would cover the three routes above and none of the
        // endpoints whose refusals it exists to rewrite (#484).
        .layer(axum::middleware::from_fn_with_state(
            caps.server_fn,
            caller_faults_are_4xx,
        ))
        // Outside the refusal layer, so a server function's response is
        // counted with the status its caller reads (#489).
        .layer(axum::middleware::from_fn_with_state(
            state.metrics.clone(),
            crate::metrics::count_server_fns,
        ))
        .with_state(leptos_options))
}

/// Gives every server-function refusal its own status (#484, #401).
///
/// `server_fn` answers every error it raises with 500, argument decoding
/// included, and offers no hook to change it
/// (<https://docs.rs/leptos/latest/leptos/server_fn/response/trait.Res.html>).
/// So the encoded [`ConsoleError`] is read back here: a decoding failure
/// becomes the caller's refusal (a body past the transport cap
/// [`ConsoleError::CallTooLarge`], an argument that would not decode
/// [`ConsoleError::CallUndecodable`]), and every variant then answers with
/// [`ConsoleError::status`]. The two caller faults are told apart by the body
/// bytes counted on the way in, below the framework's limited reader.
async fn caller_faults_are_4xx(
    axum::extract::State(cap): axum::extract::State<usize>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse as _;
    use http_body_util::BodyExt as _;
    use leptos::prelude::FromServerFnError as _;

    let read = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let request = request.map(|body| {
        let read = std::sync::Arc::clone(&read);
        axum::body::Body::new(body.map_frame(move |frame| {
            if let Some(data) = frame.data_ref() {
                read.fetch_add(data.len(), std::sync::atomic::Ordering::Relaxed);
            }
            frame
        }))
    });

    let response = next.run(request).await;
    if response.status() != axum::http::StatusCode::INTERNAL_SERVER_ERROR {
        return response;
    }

    let (parts, body) = response.into_parts();
    // NOTE: no openEHR spec governs this — our own design; a body too large
    // to read back is not an encoded refusal, so it keeps the framework's own
    // status and says nothing more.
    let Ok(bytes) = axum::body::to_bytes(body, MAX_REFUSAL_BYTES).await else {
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    // NOTE: a body that is not the console's own JSON encoding is no refusal
    // this crate raised, so it legitimately passes through as it came.
    let Ok(error) = serde_json::from_slice::<ConsoleError>(&bytes) else {
        return (parts, bytes).into_response();
    };
    let body_read = crate::arg_refusal::BodyRead {
        bytes: read.load(std::sync::atomic::Ordering::Relaxed),
        cap,
    };
    let error = crate::arg_refusal::caller_fault(&error, body_read).unwrap_or(error);
    let mut parts = parts;
    parts.status = error.status();
    parts.headers.remove(axum::http::header::CONTENT_LENGTH);
    (parts, error.ser()).into_response()
}

/// The largest error body the refusal layer reads back before giving up.
///
/// A server-function error is one encoded sentence, so anything past this is
/// not one and is answered as the framework wrote it.
const MAX_REFUSAL_BYTES: usize = 64 * 1024;

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
mod tests {
    use super::{BodyCapTooLarge, BodyCaps, SERVER_FN_INFLATION};

    #[test]
    fn every_cap_is_derived_from_the_number_its_own_endpoint_enforces()
    -> Result<(), BodyCapTooLarge> {
        let caps = BodyCaps::derived()?;
        assert_eq!(
            u64::try_from(caps.server_fn),
            Ok(SERVER_FN_INFLATION * crate::run_api::read::STATEMENT_CAP_BYTES),
            "the server-function cap is the statement cap with room for URL encoding"
        );
        Ok(())
    }

    #[test]
    fn the_caps_are_the_numbers_this_host_installs() -> Result<(), BodyCapTooLarge> {
        let caps = BodyCaps::derived()?;
        assert_eq!(caps.server_fn, 4_194_304, "4 MiB");
        Ok(())
    }

    #[test]
    fn every_cap_sits_above_the_payload_its_endpoint_accepts() -> Result<(), BodyCapTooLarge> {
        let caps = BodyCaps::derived()?;
        assert!(
            u64::try_from(caps.server_fn)
                .is_ok_and(|cap| cap > crate::run_api::read::STATEMENT_CAP_BYTES),
            "a statement at the page's own cap must reach the handler"
        );
        Ok(())
    }

    #[test]
    fn the_statement_cap_is_the_largest_input_any_server_function_judges() {
        let largest = crate::run_api::SERVER_FN_INPUT_CAPS.iter().copied().max();
        assert_eq!(
            largest,
            Some(crate::run_api::read::STATEMENT_CAP_BYTES),
            "one transport cap covers every `#[server]` fn and it is derived from the \
             statement cap alone; an endpoint now judging a larger input needs that \
             derivation re-decided, or its callers meet the generic transport refusal"
        );
    }

    #[test]
    fn a_cap_that_does_not_fit_this_host_carries_the_conversion_as_its_source() {
        let source = u8::try_from(300_u32).expect_err("300 does not fit a u8");
        let refusal = BodyCapTooLarge {
            route: "server function",
            bytes: u64::MAX,
            source,
        };
        let cause = std::error::Error::source(&refusal).expect("the conversion is the cause");
        assert!(
            cause.downcast_ref::<std::num::TryFromIntError>().is_some(),
            "the cause downcasts to the conversion's own error, not to a wrapper"
        );
    }
}
