// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The helpers every topic module shares: the one repository root every path
//! in the suite derives from, and the one test [`ConsoleState`] (#534).
//!
//! A field added to the state is added here once, and a test that needs a
//! different value overrides it with struct update syntax
//! (<https://doc.rust-lang.org/reference/expressions/struct-expr.html#functional-update-syntax>).

use std::path::Path;

#[cfg(feature = "ssr")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "ssr")]
use veredictum_console::state::ConsoleState;

/// The repository root, two levels above this crate's manifest directory.
pub(crate) fn repo_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// What the state's one startup read of the catalogue answered.
#[cfg(feature = "ssr")]
pub(crate) type Catalogue = Arc<Result<veredictum::pipeline::listing::Listing, String>>;

/// A catalogue that did not load, for `reason`: a test that reads no
/// catalogue stays off the validation pass this way.
#[cfg(feature = "ssr")]
pub(crate) fn absent(reason: &str) -> Catalogue {
    Arc::new(Err(reason.to_owned()))
}

/// The committed catalogue, listed without resolving its citations.
#[cfg(feature = "ssr")]
pub(crate) fn committed_listing() -> Catalogue {
    Arc::new(
        veredictum::pipeline::listing::read(&repo_root().join("catalogue"), None)
            .map_err(|e| e.to_string()),
    )
}

/// The committed catalogue, read exactly as the server's startup reads it:
/// citations resolved against the vendored specification tree.
#[cfg(feature = "ssr")]
pub(crate) fn validated_listing() -> Catalogue {
    let root = repo_root().join("catalogue");
    let specs = repo_root().join("specs/openehr");
    Arc::new(veredictum::pipeline::listing::read(&root, Some(&specs)).map_err(|e| e.to_string()))
}

/// The default test state: the repository's own catalogue and specification
/// mounts, run output under `out`, the given catalogue read, and every other
/// field at the value the server starts with (the local posture, the default
/// rates and job limits, no client-address header, capture mode off).
#[cfg(feature = "ssr")]
pub(crate) fn state(out: &Path, catalogue: Catalogue) -> ConsoleState {
    ConsoleState {
        root: repo_root().join("catalogue"),
        specs: repo_root().join("specs/openehr"),
        out: out.to_path_buf(),
        catalogue,
        draft: Arc::new(Mutex::new(veredictum_console::run_api::Drafts::new())),
        jobs: veredictum_console::run_job::JobSlot::default(),
        client_ip_header: None,
        posture: veredictum_console::posture::Posture::Local,
        rates: veredictum_console::rate_limit::RateLimiter::default(),
        capture: false,
        metrics: veredictum_console::metrics::Metrics::default(),
    }
}
