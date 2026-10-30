// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The engine seam (#54): the one place the console touches the instrument.
//!
//! Runs execute through the pinned, published `veredictum` binary as a
//! subprocess, so a run's output stays byte-identical to the same run from a
//! terminal (`tests/it/engine_gate.rs` diffs the two paths' documents). Reads
//! parse through the published lib's typed model, so no schema is
//! reimplemented console-side. Nothing here speaks to a CDR, and the SUT
//! credentials the console holds in memory reach only the spawned process's
//! environment: never a file, a log line, or any client-visible state.

use std::io::BufRead as _;
use std::path::{Path, PathBuf};

/// The exact engine version this console is built against.
///
/// One fact with the manifest's crates.io pin and the lib-level
/// [`crate::ENGINE_PIN`] the chrome displays: `build.rs` emits the constant
/// from that manifest line, the unit test below re-reads the line
/// independently, and [`Engine::verified`] refuses a binary that reports
/// anything else.
pub const ENGINE_VERSION: &str = crate::ENGINE_PIN;

/// The exact line the pinned engine's `--version` prints: the program name,
/// [`ENGINE_VERSION`], and the instrument status, built by the engine's own
/// [`veredictum::party::version_line`].
#[must_use]
pub fn expected_version_report() -> String {
    format!(
        "veredictum {}",
        veredictum::party::version_line(ENGINE_VERSION)
    )
}

/// The environment variable that names the engine binary explicitly. Without
/// it, [`locate`] falls back to `veredictum` on `PATH`.
pub const ENGINE_ENV: &str = "VEREDICTUM_ENGINE";

/// A secret value on its way into a spawned run's environment.
///
/// The wrapper exists for what it refuses: `Debug` prints a redaction, the
/// value is never serialized, and the only read path is the environment of
/// the child process (`Engine::run`).
pub struct Secret(String);

impl Secret {
    /// Wraps a secret value.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(«redacted»)")
    }
}

/// One credential the ixit declares by environment-variable NAME and the
/// console supplies by value, in memory, for exactly one spawned run.
#[derive(Debug)]
pub struct Credential {
    /// The environment-variable name the ixit references (`user_env`,
    /// `password_env`, `token_env`).
    pub name: String,
    /// The value, redacted from every rendering.
    pub value: Secret,
}

/// What to drive, mirroring the `veredictum test` CLI surface one to one
/// and adding nothing to it.
#[derive(Debug)]
pub struct RunSpec {
    /// The catalogue root the run reads.
    pub root: PathBuf,
    /// The server's base URL, up to and including the openEHR REST base.
    pub base_url: String,
    /// The `--auth` token: `none`, `basic` or `bearer`.
    pub auth: &'static str,
    /// The `--admin-auth` token for the administrative principal: `none`,
    /// `basic` or `bearer`. With `none` the engine skips every workflow with
    /// an administrative step as `not-declared`.
    pub admin_auth: &'static str,
    /// The party statement, when test selection should apply.
    pub statement: Option<PathBuf>,
    /// The IXIT the rows files' `declared` inputs read (already written;
    /// carries no secret).
    pub ixit: Option<PathBuf>,
    /// Run only these documents, by id; empty runs every document.
    pub documents: Vec<String>,
    /// The output directory the run writes into.
    pub out_dir: PathBuf,
    /// The secret values under the variable names `test` reads
    /// ([`veredictum::identity::USER_ENV`] and its siblings for `--auth`,
    /// [`veredictum::arazzo::credential::ADMIN_USER_ENV`] and its siblings for
    /// `--admin-auth`). They reach the child process environment and nothing
    /// else.
    pub credentials: Vec<Credential>,
}

/// Whom to identify, mirroring the `veredictum identify` CLI surface one to
/// one and adding nothing to it.
#[derive(Debug)]
pub struct IdentifySpec {
    /// The CDR base URL.
    pub base_url: String,
    /// The `--auth` token: `none`, `basic` or `bearer`.
    pub auth: &'static str,
    /// The secret values under the variable names `identify` reads
    /// ([`veredictum::identity::USER_ENV`] and its siblings). They reach the
    /// child process environment and nothing else.
    pub credentials: Vec<Credential>,
}

/// One line of the running engine's own output, as it happens.
#[derive(Debug)]
pub enum Line {
    /// A stdout line.
    Out(String),
    /// A stderr line.
    Err(String),
}

/// A finished run: the exit status, where the documents landed, and the
/// results document parsed through the published lib's typed model.
#[derive(Debug)]
pub struct Finished {
    /// Whether the engine reported a green run (exit status success).
    pub clean_exit: bool,
    /// The results document, typed by the published `veredictum` lib — never
    /// a console-side re-parse.
    pub results: veredictum::arazzo::results::Results,
    /// Where `results.json` landed.
    pub results_path: PathBuf,
    /// Where the HAR recording landed.
    pub recording_path: PathBuf,
}

/// Everything the seam can refuse, typed at the boundary that branches.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No engine binary was found at the override path or on `PATH`.
    #[error("no engine binary: {0}")]
    NotFound(String),
    /// The version probe could not run.
    #[error("the engine version probe failed: {0}")]
    Probe(#[source] std::io::Error),
    /// The binary answered the probe with a different identity.
    #[error(
        "engine version mismatch: expected `{}`, the binary reports `{reported}`",
        expected_version_report()
    )]
    VersionMismatch {
        /// What `--version` printed, trimmed.
        reported: String,
    },
    /// The run could not be spawned or streamed.
    #[error("the engine run failed to execute: {0}")]
    Execute(#[source] std::io::Error),
    /// The run finished but left no readable results document.
    #[error("the run left no readable results document at {path}: {source}")]
    NoResults {
        /// Where the document was expected.
        path: PathBuf,
        /// Why it could not be read.
        #[source]
        source: std::io::Error,
    },
    /// The identify subcommand exited non-zero; the field is the engine's own
    /// diagnostic, verbatim.
    #[error("the engine could not read the server's identity: {diagnostic}")]
    Identify {
        /// What the engine printed, trimmed.
        diagnostic: String,
    },
    /// The identity document did not parse as the published type.
    #[error("the identity document does not parse as the published type: {0}")]
    IdentityMalformed(#[source] serde_json::Error),
    /// The results document did not parse as the published record.
    #[error("the results document at {path} does not parse as the published record: {source}")]
    Malformed {
        /// The document that failed.
        path: PathBuf,
        /// The parse failure.
        #[source]
        source: serde_json::Error,
    },
}

/// The located, version-verified engine binary.
#[derive(Debug, Clone)]
pub struct Engine {
    binary: PathBuf,
}

impl Engine {
    /// Verifies the binary at `path` identifies as the pinned engine and
    /// wraps it.
    ///
    /// # Errors
    /// [`Error::Probe`] when `--version` cannot run, and
    /// [`Error::VersionMismatch`] when it reports anything other than
    /// [`expected_version_report`].
    pub fn verified(path: &Path) -> Result<Self, Error> {
        let output = std::process::Command::new(path)
            .arg("--version")
            .output()
            .map_err(Error::Probe)?;
        let reported = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if reported == expected_version_report() {
            Ok(Self {
                binary: path.to_path_buf(),
            })
        } else {
            Err(Error::VersionMismatch { reported })
        }
    }

    /// The verified binary's path.
    #[must_use]
    pub fn binary(&self) -> &Path {
        &self.binary
    }

    /// Runs the engine with `args` in the foreground, its standard streams
    /// and environment inherited, and returns its exit code.
    ///
    /// The container image's entry point is the console binary, so this is
    /// how `docker run <image> <subcommand> …` reaches the engine.
    ///
    /// # Errors
    /// [`Error::Execute`] when the process cannot be spawned, or when it ends
    /// without an exit code (stopped by a signal).
    pub fn forward(&self, args: impl IntoIterator<Item = std::ffi::OsString>) -> Result<u8, Error> {
        let status = std::process::Command::new(&self.binary)
            .args(args)
            .status()
            .map_err(Error::Execute)?;
        status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .ok_or_else(|| {
                Error::Execute(std::io::Error::other(format!(
                    "the engine ended without an exit code: {status}"
                )))
            })
    }

    /// Drives one run through the engine, streaming each output line to
    /// `on_line` as it happens, then parses the results document through the
    /// published lib.
    ///
    /// The credentials reach the child process environment and nothing
    /// else; the child's environment is otherwise inherited, which is what
    /// lets an operator pass proxy settings the ordinary way.
    ///
    /// # Errors
    /// [`Error::Execute`] when the process cannot be spawned or its output
    /// cannot be read, [`Error::NoResults`] when the run leaves no results
    /// document, and [`Error::Malformed`] when that document does not parse
    /// as the published record.
    pub fn run(&self, spec: &RunSpec, on_line: impl FnMut(Line)) -> Result<Finished, Error> {
        let running = self.spawn(spec)?;
        running.stream(on_line)
    }

    /// Reads the server's identity by running the pinned binary's `identify`
    /// subcommand to completion and parsing its document through the
    /// published lib.
    ///
    /// # Errors
    /// [`Error::Execute`] when the process cannot run, [`Error::Identify`]
    /// carrying the engine's own diagnostic when it exits non-zero, and
    /// [`Error::IdentityMalformed`] when its document does not parse.
    pub fn identify(
        &self,
        spec: &IdentifySpec,
    ) -> Result<veredictum::identity::IdentityRead, Error> {
        let mut command = std::process::Command::new(&self.binary);
        command
            .args(identify_args(spec))
            .stdin(std::process::Stdio::null());
        for credential in &spec.credentials {
            command.env(&credential.name, &credential.value.0);
        }
        let output = command.output().map_err(Error::Execute)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(Error::Identify { diagnostic: stderr });
        }
        serde_json::from_slice(&output.stdout).map_err(Error::IdentityMalformed)
    }

    /// Spawns one run and hands back the handle that streams it — the split
    /// [`Self::run`] composes, exposed so a job supervisor can hold the
    /// [`RunningEngine::canceller`] while another thread streams (#66).
    ///
    /// # Errors
    /// [`Error::Execute`] when the process cannot be spawned.
    pub fn spawn(&self, spec: &RunSpec) -> Result<RunningEngine, Error> {
        let mut command = std::process::Command::new(&self.binary);
        command
            .args(args(spec))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for credential in &spec.credentials {
            command.env(&credential.name, &credential.value.0);
        }
        let child = command.spawn().map_err(Error::Execute)?;
        Ok(RunningEngine {
            child: std::sync::Arc::new(std::sync::Mutex::new(child)),
            out_dir: spec.out_dir.clone(),
        })
    }
}

/// A cancel handle onto a running engine, safe to hold on another thread.
#[derive(Debug, Clone)]
pub struct Canceller {
    child: std::sync::Arc<std::sync::Mutex<std::process::Child>>,
}

impl Canceller {
    /// Kills the subprocess; the streaming side then observes the exit.
    ///
    /// # Errors
    /// [`Error::Execute`] when the kill itself fails (an already-exited
    /// child kills cleanly, so this is rare).
    pub fn cancel(&self) -> Result<(), Error> {
        let mut child = self
            .child
            .lock()
            .map_err(|poison| Error::Execute(std::io::Error::other(poison.to_string())))?;
        child.kill().map_err(Error::Execute)
    }
}

/// One spawned run: stream it to completion, or cancel it from elsewhere.
#[derive(Debug)]
pub struct RunningEngine {
    child: std::sync::Arc<std::sync::Mutex<std::process::Child>>,
    out_dir: PathBuf,
}

impl RunningEngine {
    /// The cancel handle, cloneable across threads.
    #[must_use]
    pub fn canceller(&self) -> Canceller {
        Canceller {
            child: std::sync::Arc::clone(&self.child),
        }
    }

    /// Streams the run to completion, then parses the results record through
    /// the published lib.
    ///
    /// # Errors
    /// [`Error::Execute`] when the output cannot be read, [`Error::NoResults`]
    /// when the run leaves no results document (a cancelled run's ordinary
    /// shape), and [`Error::Malformed`] when that document does not parse as
    /// the published record.
    pub fn stream(self, mut on_line: impl FnMut(Line)) -> Result<Finished, Error> {
        // The pipes leave the child under a short lock; streaming itself
        // never holds it, so a canceller can take the lock and kill.
        let (stdout, stderr) = {
            let mut child = self
                .child
                .lock()
                .map_err(|poison| Error::Execute(std::io::Error::other(poison.to_string())))?;
            (child.stdout.take(), child.stderr.take())
        };
        let status = drain_to_completion(&self.child, stdout, stderr, &mut on_line)?;
        read_finished(&self.out_dir, status.success())
    }
}

/// Drains both pipes to the child's exit, delivering every line to `on_line`.
///
/// A child that fills an un-read pipe's buffer blocks forever, so stderr
/// drains on its own thread while this one reads stdout, and the lines merge
/// over a channel in arrival order.
///
/// # Errors
/// [`Error::Execute`] when the child's lock is poisoned, when a stdout line
/// cannot be read, or when waiting on the child fails.
fn drain_to_completion(
    child: &std::sync::Mutex<std::process::Child>,
    stdout: Option<std::process::ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    on_line: &mut impl FnMut(Line),
) -> Result<std::process::ExitStatus, Error> {
    let (sender, receiver) = std::sync::mpsc::channel::<Line>();
    let stderr_reader = std::thread::spawn({
        let sender = sender.clone();
        move || {
            if let Some(stderr) = stderr {
                for line in std::io::BufReader::new(stderr).lines() {
                    let Ok(line) = line else { break };
                    if sender.send(Line::Err(line)).is_err() {
                        break;
                    }
                }
            }
        }
    });
    if let Some(stdout) = stdout {
        for line in std::io::BufReader::new(stdout).lines() {
            let line = line.map_err(Error::Execute)?;
            on_line(Line::Out(line));
            // Drain what stderr produced meanwhile, so the merged stream stays
            // close to arrival order.
            while let Ok(err_line) = receiver.try_recv() {
                on_line(err_line);
            }
        }
    }
    let status = {
        let mut child = child
            .lock()
            .map_err(|poison| Error::Execute(std::io::Error::other(poison.to_string())))?;
        child.wait().map_err(Error::Execute)?
    };
    // Joined, never dropped mid-stream: the remaining stderr lines are
    // delivered before the run is reported finished.
    let joined = stderr_reader.join();
    drop(sender);
    while let Ok(err_line) = receiver.try_recv() {
        on_line(err_line);
    }
    if joined.is_err() {
        on_line(Line::Err(String::from(
            "console: the stderr reader thread panicked; its tail was lost",
        )));
    }
    Ok(status)
}

/// Reads the results document the finished run left in `out_dir`, parsed
/// through the published lib.
///
/// # Errors
/// [`Error::NoResults`] when the run leaves no results document (a cancelled
/// run's ordinary shape), and [`Error::Malformed`] when that document does
/// not parse as the published record.
fn read_finished(out_dir: &Path, clean_exit: bool) -> Result<Finished, Error> {
    let results_path = out_dir.join(veredictum::pipeline::arazzo_run::RESULTS_FILE);
    let recording_path = out_dir.join(veredictum::pipeline::arazzo_run::RECORDING_FILE);
    let body = std::fs::read_to_string(&results_path).map_err(|source| Error::NoResults {
        path: results_path.clone(),
        source,
    })?;
    let results: veredictum::arazzo::results::Results =
        serde_json::from_str(&body).map_err(|source| Error::Malformed {
            path: results_path.clone(),
            source,
        })?;
    Ok(Finished {
        clean_exit,
        results,
        results_path,
        recording_path,
    })
}

/// Locates the engine: the [`ENGINE_ENV`] override when set, otherwise
/// `veredictum` on `PATH` — verified either way.
///
/// # Errors
/// [`Error::NotFound`] when neither names an existing binary, plus
/// everything [`Engine::verified`] refuses.
pub fn locate() -> Result<Engine, Error> {
    locate_with(std::env::var(ENGINE_ENV).ok().as_deref())
}

/// Resolves the engine from an explicit override, or from `PATH` when there is
/// none.
///
/// Separate from [`locate`] so the override branch is testable: reading the
/// variable happens once, in the caller, and `std::env::set_var` is unsafe in
/// this edition while `unsafe` is forbidden in this crate.
///
/// # Errors
/// [`Error::NotFound`] when neither the override nor `PATH` yields a file,
/// naming which of the two was looked at; otherwise whatever
/// [`Engine::verified`] reports.
pub fn locate_with(explicit: Option<&str>) -> Result<Engine, Error> {
    if let Some(explicit) = explicit {
        // A missing override is a typo in an operator's configuration, so the
        // diagnostic names the variable and the path it held. Without this the
        // failure surfaced as a bare "No such file or directory (os error 2)"
        // that named neither.
        return match Engine::verified(Path::new(explicit)) {
            Err(Error::Probe(source)) if source.kind() == std::io::ErrorKind::NotFound => {
                Err(Error::NotFound(format!(
                    "{ENGINE_ENV} names `{explicit}`, and there is no file there"
                )))
            }
            other => other,
        };
    }
    match Engine::verified(Path::new("veredictum")) {
        Err(Error::Probe(source)) if source.kind() == std::io::ErrorKind::NotFound => {
            Err(Error::NotFound(format!(
                "`veredictum` is not on PATH and {ENGINE_ENV} is unset"
            )))
        }
        other => other,
    }
}

/// Assembles the exact `veredictum test` argument vector for a spec.
#[must_use]
pub fn args(spec: &RunSpec) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = vec![
        "test".into(),
        "--root".into(),
        spec.root.clone().into(),
        "--base-url".into(),
        spec.base_url.clone().into(),
        "--auth".into(),
        spec.auth.into(),
        "--admin-auth".into(),
        spec.admin_auth.into(),
    ];
    if let Some(statement) = &spec.statement {
        args.push("--statement".into());
        args.push(statement.clone().into());
    }
    if let Some(ixit) = &spec.ixit {
        args.push("--ixit".into());
        args.push(ixit.clone().into());
    }
    for document in &spec.documents {
        args.push("--document".into());
        args.push(document.clone().into());
    }
    args.push("--out".into());
    args.push(spec.out_dir.clone().into());
    args
}

/// Assembles the exact `veredictum identify` argument vector for a spec.
#[must_use]
pub fn identify_args(spec: &IdentifySpec) -> Vec<std::ffi::OsString> {
    vec![
        "identify".into(),
        "--base-url".into(),
        spec.base_url.clone().into(),
        "--auth".into(),
        spec.auth.into(),
    ]
}

/// The console's view of the engine's identity document.
#[must_use]
pub fn identity_answer(
    read: veredictum::identity::IdentityRead,
) -> crate::identity::IdentityAnswer {
    use veredictum::identity::IdentityRead;

    match read {
        IdentityRead::Offered { options, .. } => {
            crate::identity::IdentityAnswer::Stated(crate::identity::StatedIdentity {
                name: options.solution,
                version: options.solution_version,
                vendor: options.vendor,
                restapi_specs_version: options.restapi_specs_version,
            })
        }
        IdentityRead::NotOffered { status } => {
            crate::identity::IdentityAnswer::NotOffered { status }
        }
        IdentityRead::NotOptionsDocument { status, reason } => {
            crate::identity::IdentityAnswer::NotOptionsDocument { status, reason }
        }
        IdentityRead::InvalidStatus { status } => {
            crate::identity::IdentityAnswer::InvalidStatus { status }
        }
    }
}

/// Returns a value's published token: the lib's own serialization, without
/// its JSON quotes.
///
/// The console mirrors no vocabulary of its own. A serialization failure is
/// returned, never absorbed: a blank token on a results row is a wrong answer
/// nobody can see.
///
/// # Errors
/// The value's `serde_json` failure, verbatim.
pub fn token<T: serde::Serialize>(value: &T) -> Result<String, serde_json::Error> {
    Ok(serde_json::to_string(value)?.trim_matches('"').to_owned())
}

/// Returns an optional value's published token, absent for an absent value.
///
/// # Errors
/// The value's `serde_json` failure, verbatim.
pub fn opt_token<T: serde::Serialize>(
    value: Option<&T>,
) -> Result<Option<String>, serde_json::Error> {
    value.map(token).transpose()
}

#[cfg(test)]
mod tests {
    use super::{
        Credential, ENGINE_ENV, ENGINE_VERSION, Error, RunSpec, Secret, args, locate_with,
        opt_token, token,
    };

    /// An override naming a file that is not there is an operator's typo, and
    /// the diagnostic has to name the variable it came from. Before #379 this
    /// surfaced as a bare "No such file or directory (os error 2)".
    #[test]
    fn an_override_naming_nothing_names_the_variable_it_came_from() {
        let missing = "/nonexistent/veredictum-engine-that-is-not-there";
        match locate_with(Some(missing)) {
            Err(Error::NotFound(message)) => {
                assert!(
                    message.contains(ENGINE_ENV),
                    "the diagnostic must name the variable: {message}"
                );
                assert!(
                    message.contains(missing),
                    "the diagnostic must name the path it held: {message}"
                );
            }
            other => panic!("expected a typed NotFound, got {other:?}"),
        }
    }

    /// The manifest line `build.rs` reads, re-read here by hand: the lock on
    /// what the build script substituted into [`ENGINE_VERSION`].
    #[test]
    fn the_pin_and_the_constant_agree() {
        let manifest = include_str!("../Cargo.toml");
        let line = manifest
            .lines()
            .find(|l| l.starts_with("veredictum = "))
            .expect("the manifest pins the engine");
        assert!(
            line.contains(&format!("\"={ENGINE_VERSION}\"")),
            "ENGINE_VERSION ({ENGINE_VERSION}) must equal the manifest's exact pin: {line}"
        );
    }

    /// One spec with every optional argument set, so the mapping is pinned
    /// argument for argument.
    fn full_spec() -> RunSpec {
        RunSpec {
            root: "catalogue".into(),
            base_url: String::from("http://cdr.test/openehr/v1"),
            auth: "basic",
            admin_auth: "bearer",
            statement: Some("mine/statement.json".into()),
            ixit: Some("out/ixit.json".into()),
            documents: vec![String::from("ehr"), String::from("ehr_status")],
            out_dir: "out".into(),
            credentials: vec![Credential {
                name: String::from("VEREDICTUM_CDR_PASSWORD"),
                value: Secret::new(String::from("hunter2")),
            }],
        }
    }

    fn rendered(spec: &RunSpec) -> Vec<String> {
        args(spec)
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_argument_vector_mirrors_the_cli_surface_and_carries_no_secret() {
        let rendered = rendered(&full_spec());
        assert_eq!(
            rendered,
            [
                "test",
                "--root",
                "catalogue",
                "--base-url",
                "http://cdr.test/openehr/v1",
                "--auth",
                "basic",
                "--admin-auth",
                "bearer",
                "--statement",
                "mine/statement.json",
                "--ixit",
                "out/ixit.json",
                "--document",
                "ehr",
                "--document",
                "ehr_status",
                "--out",
                "out",
            ]
        );
        assert!(!rendered.iter().any(|a| a.contains("hunter2")));
    }

    /// A run with no statement, no IXIT and no document narrowing passes none
    /// of those flags, so the engine's own defaults govern.
    #[test]
    fn an_unnarrowed_run_passes_no_optional_flag() {
        let spec = RunSpec {
            statement: None,
            ixit: None,
            documents: Vec::new(),
            ..full_spec()
        };
        let rendered = rendered(&spec);
        for flag in ["--statement", "--ixit", "--document"] {
            assert!(!rendered.contains(&String::from(flag)), "{rendered:?}");
        }
    }

    /// A run with no administrative credential still names the mode, so the
    /// engine skips the administrative workflows as `not-declared` by the
    /// console's own statement rather than by a default (#483).
    #[test]
    fn a_run_without_an_admin_credential_passes_admin_auth_none() {
        let spec = RunSpec {
            admin_auth: "none",
            credentials: Vec::new(),
            ..full_spec()
        };
        let rendered = rendered(&spec);
        let at = rendered
            .iter()
            .position(|a| a == "--admin-auth")
            .expect("the vector names --admin-auth");
        assert_eq!(rendered.get(at + 1).map(String::as_str), Some("none"));
    }

    #[test]
    fn the_identify_vector_mirrors_the_cli_surface_and_carries_no_secret() {
        let spec = super::IdentifySpec {
            base_url: String::from("http://cdr.test/openehr/v1"),
            auth: "basic",
            credentials: vec![Credential {
                name: String::from("VEREDICTUM_CDR_PASSWORD"),
                value: Secret::new(String::from("hunter2")),
            }],
        };
        let rendered: Vec<String> = super::identify_args(&spec)
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            rendered,
            [
                "identify",
                "--base-url",
                "http://cdr.test/openehr/v1",
                "--auth",
                "basic"
            ]
        );
        assert!(!rendered.iter().any(|a| a.contains("hunter2")));
    }

    /// A token is locked to the lib's own serialization, never a mirrored
    /// table.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_token_is_the_libs_own_spelling_without_its_quotes() -> Result<(), serde_json::Error> {
        assert_eq!(
            token(&veredictum::arazzo::results::SkipReason::NotClaimed)?,
            "not-claimed"
        );
        assert_eq!(token(&veredictum::vocab::Tier::SecBasic)?, "SEC-BASIC");
        assert_eq!(token(&veredictum::vocab::FormatName::WtFlat)?, "wt-flat");
        assert_eq!(
            token(&veredictum::perf::ClassVerdict::NotEarned)?,
            "not-earned"
        );
        Ok(())
    }

    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn an_absent_value_carries_no_token() -> Result<(), serde_json::Error> {
        assert_eq!(
            opt_token(Some(&veredictum::vocab::FormatName::Wt))?.as_deref(),
            Some("wt")
        );
        assert_eq!(opt_token::<veredictum::vocab::FormatName>(None)?, None);
        Ok(())
    }

    #[test]
    fn a_secret_debug_rendering_carries_no_value() {
        let credential = Credential {
            name: String::from("SUT_PASS"),
            value: Secret::new(String::from("hunter2")),
        };
        let rendered = format!("{credential:?}");
        assert!(!rendered.contains("hunter2"), "leaked: {rendered}");
        assert!(rendered.contains("redacted"));
    }
}
