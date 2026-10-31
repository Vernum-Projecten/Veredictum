// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console server binary (`ssr` feature): serves the Leptos routes, the
//! compiled client bundle, and a `/healthz` liveness endpoint. A featureless
//! build compiles a stub so plain `cargo build --all-targets` over the
//! workspace stays green; the shipped binary is always the `ssr` shape
//! (`bin-features` in `Cargo.toml`).
#![allow(
    clippy::print_stderr,
    reason = "the server binary's startup diagnostics belong on stderr, where an operator tailing the container sees them; library code stays restricted"
)]

//! The image is distroless and this binary is PID 1, so it handles SIGTERM
//! itself for `docker stop` to end it gracefully, and it doubles as its own
//! health probe (`veredictum-console healthcheck`) because the image carries
//! no shell and no curl. Any other first argument names an engine subcommand,
//! which the binary hands to the pinned engine, so `docker run <image> test …`
//! drives the catalogue the image ships.

/// Serves the console with no argument, probes the running server's
/// `/healthz` as `veredictum-console healthcheck`, and otherwise runs the
/// pinned engine with every argument and exits with the engine's own status.
///
/// # Errors
/// Returns an error when the console cannot serve, or the health probe does
/// not answer healthy.
#[cfg(feature = "ssr")]
fn main() -> anyhow::Result<std::process::ExitCode> {
    let mut args = std::env::args_os().skip(1);
    match args.next() {
        None => serve().map(|()| std::process::ExitCode::SUCCESS),
        Some(mode) if mode == "healthcheck" => {
            healthcheck().map(|()| std::process::ExitCode::SUCCESS)
        }
        Some(subcommand) => Ok(engine_subcommand(std::iter::once(subcommand).chain(args))),
    }
}

/// Runs the pinned engine with `args` and exits with its status. A failure
/// to start it exits `2`, the engine's own code for a run that could not
/// start, so a red run's `1` keeps its meaning.
#[cfg(feature = "ssr")]
fn engine_subcommand(args: impl IntoIterator<Item = std::ffi::OsString>) -> std::process::ExitCode {
    let status = veredictum_console::engine::locate().and_then(|engine| engine.forward(args));
    match status {
        Ok(code) => std::process::ExitCode::from(code),
        Err(e) => {
            eprintln!("veredictum-console: {e}");
            std::process::ExitCode::from(2)
        }
    }
}

/// Serves the console on the configured address (`site-addr`, overridable
/// through the standard `LEPTOS_SITE_ADDR` environment variable).
///
/// # Errors
/// Returns an error when the Leptos configuration cannot be read, the
/// listener cannot bind, or the server stops on a fault.
#[cfg(feature = "ssr")]
#[tokio::main]
async fn serve() -> anyhow::Result<()> {
    use leptos::prelude::get_configuration;

    let conf = get_configuration(None)?;
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;

    // The catalogue loads ONCE, before the listener binds: every request
    // shares the same startup read, and a missing mount is a first-class
    // state the screens explain rather than a crash (#64).
    // The posture is the ONE startup value that refuses rather than degrades
    // (#390): a public instance that read a typo as `local` would drive
    // whatever address a visitor named.
    let (state, setting) = veredictum_console::state::ConsoleState::load_with_setting()?;
    eprintln!("veredictum-console: {} posture", state.posture.token());
    // The setting's value names a mode and never a secret, so it is safe to
    // echo; an unrecognised one keeps trial mode, and says so here.
    if let veredictum_console::certificates::ssr::Setting::Unrecognised(value) = &setting {
        eprintln!(
            "veredictum-console: {}={value:?} is not `{}`, so certificate issuance stays off",
            veredictum_console::certificates::ISSUANCE_ENV,
            veredictum_console::certificates::ISSUE
        );
    }
    eprintln!(
        "veredictum-console: certificate issuance {}",
        match state.jobs.issuance_mode() {
            veredictum_console::certificates::IssuanceMode::Trial => "off (trial mode)",
            veredictum_console::certificates::IssuanceMode::Issuing => "on",
        }
    );
    // Whether the counters are served, never the token that opens them.
    eprintln!(
        "veredictum-console: metrics {}",
        if state.metrics.access().is_some() {
            "served at /metrics behind the operator's token"
        } else {
            "not served (no token set)"
        }
    );
    if let Err(reason) = state.catalogue.as_ref() {
        // A load diagnostic about a mount, never SUT data: no response body
        // reaches a log stream.
        eprintln!(
            "veredictum-console: no catalogue at {}: {reason}",
            state.root.display()
        );
    }

    // A cap that does not fit this host's usize stops startup: the one value
    // a body limit must never fall back to is "unlimited" (#492).
    let app = veredictum_console::router::router(&state, leptos_options)?;

    // The artifact sweeper (#412). The instrument now runs on a host that does
    // not restart, so the run directories a disposable filesystem used to
    // discard every few hours would otherwise grow until the disk is gone. It
    // lives here rather than in the job map so that constructing a state — which
    // every test does — spawns no thread.
    {
        let out = state.out.clone();
        let jobs = state.jobs.clone();
        tokio::spawn(async move {
            loop {
                let live = jobs.live_ids().unwrap_or_default();
                let out = out.clone();
                // Reading a directory tree is blocking I/O, and a runtime thread
                // is not where it belongs.
                let swept = tokio::task::spawn_blocking(move || {
                    veredictum_console::run_job::sweep_artifacts(
                        &out,
                        veredictum_console::run_job::ARTIFACTS_KEPT,
                        &live,
                    )
                })
                .await;
                if let Ok(swept) = swept
                    && (swept.removed > 0 || swept.refused > 0)
                {
                    // A count, never a path: an operator sees the shape without
                    // a run's identity reaching a shared log stream.
                    eprintln!(
                        "veredictum-console: swept {} expired run directory(ies), {} refused, {} kept live",
                        swept.removed, swept.refused, swept.live
                    );
                }
                tokio::time::sleep(veredictum_console::run_job::SWEEP_INTERVAL).await;
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    // The peer address is the only identity a console with no login has
    // (#389), and axum surfaces it as a `ConnectInfo` extension only when the
    // service is built with one, which `linger::serve` does. Every
    // connection also closes in stages, so a refusal written before the
    // caller finished sending still reaches it (#780).
    veredictum_console::linger::serve(listener, app, shutdown_signal()).await?;
    Ok(())
}

/// Resolves when the process receives SIGTERM (what `docker stop` and every
/// orchestrator send PID 1) or SIGINT (Ctrl-C in a terminal), so axum drains
/// in-flight requests instead of dying by SIGKILL at the grace-period
/// deadline.
#[cfg(feature = "ssr")]
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        // No signal handler means no graceful path exists; pending forever
        // leaves shutdown to the runtime's own termination.
        let Ok(mut sigterm) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        else {
            return std::future::pending().await;
        };
        tokio::select! {
            _ = sigterm.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Probes the running server's `/healthz` over one plain HTTP/1.1 exchange.
///
/// Hand-rolled over `std::net::TcpStream` because the probe runs inside the
/// distroless image where no curl exists, and an HTTP client would be the
/// heavier tool for one localhost GET. `Connection: close` keeps the read
/// finite.
///
/// # Errors
/// Returns an error when the connection, the write, the read, or the status
/// line fails — each is an unhealthy verdict, and the non-zero exit is what
/// the container HEALTHCHECK counts.
#[cfg(feature = "ssr")]
fn healthcheck() -> anyhow::Result<()> {
    use std::io::{Read, Write};

    let addr = std::env::var("LEPTOS_SITE_ADDR").unwrap_or_else(|_| String::from("127.0.0.1:3000"));
    // The server binds 0.0.0.0 in the container; the probe reaches it on
    // loopback.
    let addr = addr.replace("0.0.0.0", "127.0.0.1");
    let timeout = std::time::Duration::from_secs(3);

    let target = std::net::ToSocketAddrs::to_socket_addrs(addr.as_str())?
        .next()
        .ok_or_else(|| anyhow::anyhow!("LEPTOS_SITE_ADDR resolves to no address: {addr}"))?;
    let mut stream = std::net::TcpStream::connect_timeout(&target, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if response.starts_with("HTTP/1.1 200") {
        Ok(())
    } else {
        let status = response.lines().next().unwrap_or("no response");
        anyhow::bail!("unhealthy: {status}")
    }
}

// The featureless stub, so the bin target compiles under every feature set
// the workspace gates build; the client entry point is `lib.rs::hydrate`.
#[cfg(not(feature = "ssr"))]
fn main() {}
