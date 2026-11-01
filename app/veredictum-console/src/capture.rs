// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Documentation capture mode: the stand-ins the surfaces show for the facts
//! one run stamps.
//!
//! The book's screenshots photograph a live run's clock, the run's own
//! identity, the address the fixture server bound and the latency the connect
//! probe measured. Every one of those changes on every pass, and the chrome's
//! engine version changes at every release, so the `ui-screenshot-guard` job
//! cannot tell a re-run or a release from a real visual change.
//!
//! In capture mode the server functions answer with the fixed stand-ins
//! below, so an unchanged console photographs identically. Nothing else
//! moves: the results and the recording carry real values, because the
//! pinning happens where a value is SENT TO A BROWSER and nowhere else.
//!
//! What a pass measures is pinned; what a pass RECORDED is not. The case
//! counter, the case now driving, the engine's own output and the finished
//! tally stay verbatim, because a screenshot that redacted them would
//! document a run nobody drove. What those carry of the run's identity — the
//! output directory every path names — is rewritten to the pinned id, which
//! is the one fact in them a re-run moves.
//!
//! The engine also mints a fresh UUID for every generated workflow input,
//! such as a client-supplied `ehr_id`, and names it in its output and in the
//! judged exchange. Each distinct minted id is shown as a numbered stand-in
//! ([`MintedIds`]), so the screens keep which ids are the same and which
//! differ while a re-run moves no pixel.
//!
//! A moving frame is the same problem as a moving fact: the served document
//! carries [`CAPTURE_CLASS`] on its root element in capture mode, and
//! `style/tailwind.css` switches every transition and animation off under it,
//! so a screenshot photographs a settled frame instead of whatever phase of
//! the progress bar's width transition it happened to catch.

use crate::record_api::{ExchangeView, RecordingView, ResultDetail, ResultsScreen};
use crate::run_api::{DraftView, ProbeAnswer, RecordedResults, RecordedRun, RunScreen};
use crate::run_job::{FinishedView, JobView, RunId};

/// The environment variable that turns capture mode on.
pub const CAPTURE_ENV: &str = "VEREDICTUM_CAPTURE_MODE";

/// The class the served document's root element carries in capture mode.
///
/// `style/tailwind.css` matches it, unlayered so it outranks every Tailwind
/// utility, and switches transitions and animations off beneath it.
pub const CAPTURE_CLASS: &str = "capture";

/// Whether capture mode is on for this process.
///
/// The one read of [`CAPTURE_ENV`]: the state's flag and the served
/// document's root class are the same fact, so they cannot disagree about
/// which mode a pass is photographing.
#[must_use]
pub fn enabled() -> bool {
    std::env::var(CAPTURE_ENV).is_ok_and(|value| !value.is_empty())
}

/// The served document's root class: [`CAPTURE_CLASS`] under capture mode,
/// empty otherwise.
///
/// Server-side only by construction — the shell renders the document around
/// the hydration root and is never re-rendered in the browser — so the class
/// changes no hydrated view.
#[must_use]
pub fn root_class(capture: bool) -> &'static str {
    if capture { CAPTURE_CLASS } else { "" }
}

/// Every duration a captured surface measured, in milliseconds: the run
/// clock, the estimate beside it, and the connect probe's round trip.
///
/// Zero: no run and no request takes no time at all, so the reader can tell a
/// stand-in from a measurement.
pub const PINNED_ELAPSED_MS: u64 = 0;

/// The run id every captured live screen shows in place of a minted one.
///
/// A run id is a fresh UUID per run (#386), and the live screen prints it as
/// the run's own address, so without a stand-in every capture pass would
/// rewrite the screenshot with an id nobody changed.
pub const PINNED_RUN_ID: RunId = RunId::NIL;

/// The endpoint every captured surface shows in place of the address its run
/// drove: the scope screen's connection pane.
///
/// The harness's fixture server binds an ephemeral port, so the real value
/// moves on every pass; port zero is a port no server ever answers on, which
/// makes the stand-in unmistakable.
pub const PINNED_ENDPOINT: &str = "http://127.0.0.1:0";

/// The engine version every captured surface shows in place of the linked
/// one: the chrome's footer.
///
/// The linked version moves at every release, so without a stand-in each cut
/// would stale every capture. `0.3.1` is the version the book's badge gallery
/// pins its fixtures at, so the book names one engine version throughout.
pub const PINNED_ENGINE_VERSION: &str = "0.3.1";

/// The engine version the chrome prints: [`PINNED_ENGINE_VERSION`] under
/// capture mode, the linked [`crate::ENGINE_PIN`] otherwise.
#[must_use]
pub fn engine_version(capture: bool) -> &'static str {
    if capture {
        PINNED_ENGINE_VERSION
    } else {
        crate::ENGINE_PIN
    }
}

/// The live screen as this console answers it.
#[cfg(feature = "ssr")]
#[must_use]
pub fn run_screen(state: &crate::state::ConsoleState, screen: RunScreen) -> RunScreen {
    if state.capture {
        pin_run_screen(screen)
    } else {
        screen
    }
}

/// The results screen as this console answers it.
#[cfg(feature = "ssr")]
#[must_use]
pub fn results(
    state: &crate::state::ConsoleState,
    screen: Option<ResultsScreen>,
) -> Option<ResultsScreen> {
    if state.capture {
        screen.map(pin_results)
    } else {
        screen
    }
}

/// One row's detail as this console answers it.
#[cfg(feature = "ssr")]
#[must_use]
pub fn detail(
    state: &crate::state::ConsoleState,
    detail: Option<ResultDetail>,
) -> Option<ResultDetail> {
    if state.capture {
        detail.map(pin_detail)
    } else {
        detail
    }
}

/// The connection draft as this console answers it.
#[cfg(feature = "ssr")]
#[must_use]
pub fn draft(state: &crate::state::ConsoleState, view: Option<DraftView>) -> Option<DraftView> {
    if state.capture {
        view.map(pin_draft)
    } else {
        view
    }
}

/// The probe's answer as this console answers it.
#[cfg(feature = "ssr")]
#[must_use]
pub fn probe_answer(state: &crate::state::ConsoleState, answer: ProbeAnswer) -> ProbeAnswer {
    if state.capture {
        pin_probe(answer)
    } else {
        answer
    }
}

/// One connection draft as a capture shows it.
///
/// The harness's fixture SUT binds an ephemeral port, and the scope screen
/// prints the drafted address verbatim, so the one fact a re-run moves is the
/// address itself.
#[must_use]
pub fn pin_draft(view: DraftView) -> DraftView {
    DraftView {
        base_url: String::from(PINNED_ENDPOINT),
        ..view
    }
}

/// One probe answer as a capture shows it.
///
/// The connect screen prints the measured round trip verbatim, so a stopwatch
/// is in the picture until it is pinned. An unreachable server carries the
/// transport's own words and no measurement.
#[must_use]
pub fn pin_probe(answer: ProbeAnswer) -> ProbeAnswer {
    match answer {
        ProbeAnswer::Answered { status, ok, .. } => ProbeAnswer::Answered {
            status,
            elapsed_ms: PINNED_ELAPSED_MS,
            ok,
        },
        other @ ProbeAnswer::Unreachable { .. } => other,
    }
}

/// The live screen as a capture shows it.
///
/// A streamed run and a run read back from its own directory both carry the
/// run's identity; the other two states name nothing a re-run moves.
#[must_use]
pub fn pin_run_screen(screen: RunScreen) -> RunScreen {
    match screen {
        RunScreen::Live(view) => RunScreen::Live(Box::new(pin_job(*view))),
        RunScreen::Recorded(run) => RunScreen::Recorded(Box::new(pin_recorded(*run))),
        other => other,
    }
}

/// The live run as a capture shows it.
///
/// Capture mode changes only what a surface DISPLAYS. The run's id is both a
/// displayed fact and the address every control on the screen posts back, so
/// the view carries the two apart: [`JobView::shown_id`] is pinned, and
/// [`JobView::id`] stays the run's own, so Cancel reaches the run it names.
///
/// The clock, the estimate and the displayed id are stand-ins. The
/// counter, the case now driving, the engine's output and the finished tally
/// are the run's own, with the minted id every path in them names rewritten
/// to the pinned one: the output directory carries that id, so the tail and
/// the results path move on every pass. The ids the engine mints for
/// generated inputs are numbered stand-ins in the tail ([`MintedIds`]).
#[must_use]
pub fn pin_job(view: JobView) -> JobView {
    let minted = view.id.to_string();
    JobView {
        shown_id: PINNED_RUN_ID,
        elapsed_ms: PINNED_ELAPSED_MS,
        eta_ms: view.eta_ms.map(|_| PINNED_ELAPSED_MS),
        tail: {
            let mut ids = MintedIds::default();
            view.tail
                .iter()
                .map(|line| ids.pin(&pin_loopback_port(&pin_run_paths(line, &minted))))
                .collect()
        },
        finished: view.finished.map(|summary| FinishedView {
            results_path: pin_run_paths(&summary.results_path, &minted),
            ..summary
        }),
        ..view
    }
}

/// One recorded run's facts as a capture shows them.
///
/// Its directory and the document inside it are named by the run's own id, so
/// both are re-derived from the pinned one; the tally the record carries is
/// the record's own. As for [`pin_job`], only the displayed id is pinned: the
/// permalink still links to and copies the run's own address.
#[must_use]
pub fn pin_recorded(run: RecordedRun) -> RecordedRun {
    let minted = run.id.to_string();
    RecordedRun {
        shown_id: PINNED_RUN_ID,
        dir: pin_run_paths(&run.dir, &minted),
        results: run.results.map(|results| RecordedResults {
            base_url: String::from(PINNED_ENDPOINT),
            results_path: pin_run_paths(&results.results_path, &minted),
            ..results
        }),
        ..run
    }
}

/// The results screen as a capture shows it.
///
/// The screen prints the address the run drove, which is the fixture
/// server's ephemeral port; the tallies and the rows are the run's own. A
/// certificate's download path is an address the link follows and the screen
/// never prints, so it stays the run's own.
#[must_use]
pub fn pin_results(screen: ResultsScreen) -> ResultsScreen {
    ResultsScreen {
        base_url: String::from(PINNED_ENDPOINT),
        ..screen
    }
}

/// One row's detail as a capture shows it.
///
/// The judged exchange names the fixture server's address in its request
/// line, its `Host` header and any URL the server answered with; that
/// address is rewritten to the pinned one, every id the engine minted is
/// shown by its [`MintedIds`] stand-in in the exchange and the explanation
/// beside it, and every other byte is the recording's own.
#[must_use]
pub fn pin_detail(detail: ResultDetail) -> ResultDetail {
    let RecordingView::Exchange(exchange) = &detail.recording else {
        return detail;
    };
    let Some(authority) = authority_of(&exchange.request_line) else {
        return detail;
    };
    let pinned = PINNED_ENDPOINT
        .split_once("://")
        .map_or(PINNED_ENDPOINT, |(_, authority)| authority);
    let mut ids = MintedIds::default();
    let what = detail.what.as_deref().map(|text| ids.pin(text));
    let expected = detail.expected.as_deref().map(|text| ids.pin(text));
    let received = detail.received.as_deref().map(|text| ids.pin(text));
    let mut swap = |text: &str| ids.pin(&text.replace(&authority, pinned));
    let exchange = ExchangeView {
        entry: exchange.entry,
        request_line: swap(&exchange.request_line),
        request_headers: swap(&exchange.request_headers),
        request_body: exchange.request_body.as_deref().map(&mut swap),
        status_line: exchange.status_line.clone(),
        response_headers: swap(&exchange.response_headers),
        response_body: exchange.response_body.as_deref().map(&mut swap),
    };
    ResultDetail {
        what,
        expected,
        received,
        recording: RecordingView::Exchange(exchange),
        ..detail
    }
}

/// Every distinct id one captured surface names, numbered in the order the
/// surface first names it.
///
/// The engine mints a fresh UUID for every generated workflow input, so the
/// ids in its output and in a judged exchange move on every pass. The nil
/// UUID is already the pinned run id and is never renumbered, so stand-in
/// `n` reads `00000000-0000-0000-0000-` followed by `n` in twelve digits.
#[derive(Debug, Default)]
pub struct MintedIds {
    seen: Vec<String>,
}

impl MintedIds {
    /// Returns `text` with every UUID in it replaced by its stand-in.
    #[must_use]
    pub fn pin(&mut self, text: &str) -> String {
        let mut pinned = String::with_capacity(text.len());
        let mut rest = text;
        let mut after_word = false;
        while let Some(first) = rest.chars().next() {
            if !after_word && let Some(id) = uuid_at(rest) {
                pinned.push_str(&self.stand_in(id));
                rest = rest.get(UUID_LEN..).unwrap_or_default();
                after_word = false;
                continue;
            }
            pinned.push(first);
            after_word = first.is_ascii_alphanumeric();
            rest = rest.get(first.len_utf8()..).unwrap_or_default();
        }
        pinned
    }

    /// The stand-in for one id, numbering it on first sight.
    fn stand_in(&mut self, id: &str) -> String {
        let id = id.to_ascii_lowercase();
        if id == PINNED_RUN_ID.to_string() {
            return id;
        }
        let number = if let Some(position) = self.seen.iter().position(|seen| *seen == id) {
            position.saturating_add(1)
        } else {
            self.seen.push(id);
            self.seen.len()
        };
        format!("00000000-0000-0000-0000-{number:012}")
    }
}

/// The length of a hyphenated UUID.
const UUID_LEN: usize = 36;

/// The hyphenated UUID `text` opens with, when it opens with one that no
/// further word character extends.
// NOTE: no openEHR spec governs this — our own design; RFC 9562 §4 gives the 8-4-4-4-12 hex form.
fn uuid_at(text: &str) -> Option<&str> {
    let candidate = text.get(..UUID_LEN)?;
    let shaped = candidate.bytes().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            byte == b'-'
        } else {
            byte.is_ascii_hexdigit()
        }
    });
    let extended = text
        .get(UUID_LEN..)
        .and_then(|after| after.chars().next())
        .is_some_and(|next| next.is_ascii_alphanumeric());
    (shaped && !extended).then_some(candidate)
}

/// One engine output line with every loopback port rewritten to port zero.
///
/// The engine names each red row's exchange by its URL, and the harness's
/// fixture server is a loopback address on an ephemeral port, so the port is
/// the one fact in those lines a re-run moves.
#[must_use]
fn pin_loopback_port(line: &str) -> String {
    const LOOPBACK: &str = "127.0.0.1:";
    let mut pinned = String::with_capacity(line.len());
    let mut rest = line;
    while let Some((before, after)) = rest.split_once(LOOPBACK) {
        pinned.push_str(before);
        pinned.push_str(LOOPBACK);
        let past_port = after.trim_start_matches(|c: char| c.is_ascii_digit());
        if past_port.len() < after.len() {
            pinned.push('0');
        }
        rest = past_port;
    }
    pinned.push_str(rest);
    pinned
}

/// The `host:port` of a `METHOD url` request line.
// NOTE: no openEHR spec governs this — our own design; a line naming no absolute URL has no address to pin.
fn authority_of(request_line: &str) -> Option<String> {
    let (_, url) = request_line.split_once(' ')?;
    let (_, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?']).next()?;
    (!authority.is_empty()).then(|| authority.to_owned())
}

/// One line with the run's minted id rewritten to the pinned one.
#[must_use]
fn pin_run_paths(line: &str, minted: &str) -> String {
    line.replace(minted, &PINNED_RUN_ID.to_string())
}

#[cfg(test)]
mod tests {
    use super::{PINNED_ELAPSED_MS, PINNED_RUN_ID, pin_job, pin_run_screen};
    use crate::run_api::{DraftView, ProbeAnswer, RecordedResults, RecordedRun, RunScreen};
    use crate::run_job::{FinishedView, JobStatus, JobView, RunId};

    /// The stylesheet's still-frame rule keys on this exact class, and an
    /// ordinary serve carries none of it: a console that photographed with
    /// transitions off would document a different product.
    #[test]
    fn only_a_capture_serve_stills_the_frame() {
        assert_eq!(super::root_class(true), super::CAPTURE_CLASS);
        assert_eq!(super::root_class(false), "");
    }

    /// A capture names the pinned engine, so a release restales no image;
    /// an ordinary serve names the engine it links.
    #[test]
    fn only_a_capture_pins_the_engine_version() {
        assert_eq!(super::engine_version(true), super::PINNED_ENGINE_VERSION);
        assert_eq!(super::PINNED_ENGINE_VERSION, "0.3.1");
        assert_eq!(super::engine_version(false), crate::ENGINE_PIN);
    }

    /// The minted id every fixture run carries in the harness.
    const MINTED: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";

    /// One driving job as the slot answers it.
    fn driving() -> JobView {
        JobView {
            id: RunId::NIL,
            shown_id: RunId::NIL,
            sut_name: String::from("example-cdr"),
            sut_version: String::from("1.0.0"),
            status: JobStatus::Running,
            completed: 7,
            total: 11,
            current_case: Some(String::from("I_EHR_SERVICE.create_ehr-main")),
            elapsed_ms: 91_337,
            eta_ms: Some(4_200),
            tail: vec![String::from("driving 11 cases")],
            finished: None,
        }
    }

    /// The live screen's clock, its estimate and the run's own address are
    /// pinned; the engine's own progress facts are not, because a screenshot
    /// that redacted them would document a run nobody drove.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_pinned_job_keeps_the_engine_s_own_progress() -> Result<(), crate::run_job::RunIdError> {
        let minted: RunId = MINTED.parse()?;
        let pinned = pin_job(JobView {
            id: minted,
            shown_id: minted,
            ..driving()
        });
        assert_eq!(pinned.elapsed_ms, PINNED_ELAPSED_MS);
        assert_eq!(pinned.eta_ms, Some(PINNED_ELAPSED_MS));
        assert_eq!(
            pinned.shown_id, PINNED_RUN_ID,
            "the displayed id is a fresh UUID"
        );
        assert_eq!(pinned.completed, 7);
        assert_eq!(pinned.total, 11);
        assert_eq!(
            pinned.current_case.as_deref(),
            Some("I_EHR_SERVICE.create_ehr-main")
        );
        Ok(())
    }

    /// Capture mode changes what is displayed, never an address a control
    /// posts back: the pinned view still names the run, so its Cancel reaches
    /// it (#404).
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_pinned_job_keeps_the_address_its_controls_post() -> Result<(), crate::run_job::RunIdError>
    {
        let minted: RunId = MINTED.parse()?;
        let pinned = pin_job(JobView {
            id: minted,
            shown_id: minted,
            ..driving()
        });
        assert_eq!(pinned.id, minted, "Cancel posts the run's own id");
        assert_ne!(pinned.id, pinned.shown_id, "only the display is pinned");
        Ok(())
    }

    /// The run's own directory is named by the id a run mints, so every path
    /// the live screen prints — the engine's output tail and the finished
    /// summary's results document — is re-derived from the pinned id, and the
    /// engine's own words around it survive.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_pinned_job_names_no_minted_directory() -> Result<(), crate::run_job::RunIdError> {
        let minted: RunId = MINTED.parse()?;
        let pinned = pin_job(JobView {
            id: minted,
            tail: vec![
                String::from("1 row(s): 0 passed, 0 failed, 1 errored, 0 skipped"),
                format!("wrote target/out/console-job-{MINTED}/recording.har"),
            ],
            finished: Some(FinishedView {
                passed: 0,
                failed: 0,
                errored: 1,
                skipped: 0,
                results_path: format!("target/out/console-job-{MINTED}/results.json"),
                certificate: crate::certificates::CertificateStatus::Trial,
            }),
            ..driving()
        });
        assert_eq!(
            pinned.tail,
            vec![
                String::from("1 row(s): 0 passed, 0 failed, 1 errored, 0 skipped"),
                format!("wrote target/out/console-job-{PINNED_RUN_ID}/recording.har"),
            ]
        );
        let Some(summary) = pinned.finished else {
            panic!("a finished run keeps its summary");
        };
        assert_eq!(
            summary.results_path,
            format!("target/out/console-job-{PINNED_RUN_ID}/results.json")
        );
        assert_eq!(summary.errored, 1, "the record's own tally is the record's");
        Ok(())
    }

    /// A run read back from its own directory carries the same identity in
    /// the same two places, and the address it drove is the fixture server's
    /// ephemeral port, so it is the pinned one; the tally is the record's own.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_pinned_recorded_run_names_no_minted_directory() -> Result<(), crate::run_job::RunIdError> {
        let minted: RunId = MINTED.parse()?;
        let RunScreen::Recorded(pinned) =
            pin_run_screen(RunScreen::Recorded(Box::new(RecordedRun {
                id: minted,
                shown_id: minted,
                dir: format!("target/out/console-job-{MINTED}"),
                results: Some(RecordedResults {
                    base_url: String::from("http://cdr.test/openehr/v1"),
                    passed: 0,
                    failed: 0,
                    errored: 1,
                    skipped: 0,
                    results_path: format!("target/out/console-job-{MINTED}/results.json"),
                }),
            })))
        else {
            panic!("a recorded run stays recorded");
        };
        assert_eq!(pinned.shown_id, PINNED_RUN_ID, "the displayed id is pinned");
        assert_eq!(pinned.id, minted, "the permalink links to the run itself");
        assert_eq!(
            pinned.dir,
            format!("target/out/console-job-{PINNED_RUN_ID}")
        );
        let Some(results) = pinned.results else {
            panic!("a recorded run keeps its results");
        };
        assert_eq!(
            results.results_path,
            format!("target/out/console-job-{PINNED_RUN_ID}/results.json")
        );
        assert_eq!(results.base_url, super::PINNED_ENDPOINT);
        assert_eq!(results.errored, 1, "the record's own tally is the record's");
        Ok(())
    }

    /// The scope screen prints the drafted address, and the harness's fixture
    /// SUT binds an ephemeral port, so the address is a stand-in and every
    /// selection fact the screen is about survives.
    #[test]
    fn a_pinned_draft_keeps_every_selection_fact() {
        let pinned = super::pin_draft(DraftView {
            base_url: String::from("http://127.0.0.1:54270"),
            sut_name: String::from("my-cdr"),
            sut_version: String::from("unknown"),
            auth: String::from("none"),
            admin_auth: String::from("none"),
            probed_ok: false,
            statement: Some(String::from("my-cdr unknown")),
            filter: Some(String::from("ehr_status")),
            postures: vec![String::from("system id: cdr.example.org")],
            profile: String::from("development"),
        });
        assert_eq!(pinned.base_url, super::PINNED_ENDPOINT);
        assert_eq!(pinned.sut_name, "my-cdr");
        assert_eq!(pinned.statement.as_deref(), Some("my-cdr unknown"));
        assert_eq!(pinned.filter.as_deref(), Some("ehr_status"));
        assert!(!pinned.probed_ok);
        assert_eq!(
            pinned.postures,
            vec![String::from("system id: cdr.example.org")]
        );
    }

    /// The connect screen prints the probe's measured round trip, so the
    /// measurement is a stand-in and the server's own answer is not.
    #[test]
    fn a_pinned_probe_keeps_the_server_s_own_answer() {
        let pinned = super::pin_probe(ProbeAnswer::Answered {
            status: String::from("HTTP 500 Internal Server Error"),
            elapsed_ms: 37,
            ok: false,
        });
        assert_eq!(
            pinned,
            ProbeAnswer::Answered {
                status: String::from("HTTP 500 Internal Server Error"),
                elapsed_ms: PINNED_ELAPSED_MS,
                ok: false,
            }
        );
        // Nothing answered, so there is no measurement to pin.
        let unreachable = ProbeAnswer::Unreachable {
            error: String::from("connection refused"),
        };
        assert_eq!(super::pin_probe(unreachable.clone()), unreachable);
    }

    /// A live screen with no streamed run carries nothing a re-run moves.
    #[test]
    fn only_a_streamed_run_is_pinned() {
        assert_eq!(pin_run_screen(RunScreen::NoRunNamed), RunScreen::NoRunNamed);
        assert_eq!(
            pin_run_screen(RunScreen::Unknown(RunId::NIL)),
            RunScreen::Unknown(RunId::NIL)
        );
        let RunScreen::Live(pinned) = pin_run_screen(RunScreen::Live(Box::new(driving()))) else {
            panic!("a streamed run stays streamed");
        };
        assert_eq!(pinned.elapsed_ms, PINNED_ELAPSED_MS);
    }

    /// The judged exchange's address is the fixture server's ephemeral port,
    /// so the request line, the `Host` header and a `Location` the server
    /// answered with show the pinned one; the status and every other byte
    /// stay the recording's own.
    #[test]
    fn a_pinned_detail_names_the_pinned_address() {
        use crate::record_api::{ExchangeView, RecordingView, ResultDetail, ResultRow};
        let detail = ResultDetail {
            row: ResultRow {
                case: String::from("ehr/create_ehr"),
                row: 0,
                status: String::from("failed"),
                mark: String::from("failed"),
                glyph: String::from("✕"),
                reason: Some(String::from("status 500")),
                formalism: None,
                trial: false,
                reports: Vec::new(),
            },
            summary: None,
            catalogue: None,
            step: None,
            what: None,
            why: None,
            expected: None,
            received: None,
            whose: None,
            report: None,
            skip: None,
            recording: RecordingView::Exchange(ExchangeView {
                entry: 3,
                request_line: String::from("POST http://127.0.0.1:36803/openehr/v1/ehr"),
                request_headers: String::from("host: 127.0.0.1:36803"),
                request_body: None,
                status_line: String::from("HTTP 201"),
                response_headers: String::from("location: http://127.0.0.1:36803/openehr/v1/ehr/1"),
                response_body: Some(String::from("{}")),
            }),
        };
        let pinned = super::pin_detail(detail);
        let RecordingView::Exchange(exchange) = pinned.recording else {
            panic!("an exchange stays an exchange");
        };
        assert_eq!(
            exchange.request_line,
            "POST http://127.0.0.1:0/openehr/v1/ehr"
        );
        assert_eq!(exchange.request_headers, "host: 127.0.0.1:0");
        assert_eq!(
            exchange.response_headers,
            "location: http://127.0.0.1:0/openehr/v1/ehr/1"
        );
        assert_eq!(exchange.status_line, "HTTP 201");
        assert_eq!(exchange.entry, 3);
    }

    /// The engine's own output names each red row's exchange by its URL; the
    /// loopback port moves on every pass and is pinned, and nothing else in
    /// the line is touched.
    #[test]
    fn an_output_line_keeps_everything_but_the_loopback_port() {
        assert_eq!(
            super::pin_loopback_port(
                "Exchange OPTIONS http://127.0.0.1:44439/ -> 500   (recording entry 0)"
            ),
            "Exchange OPTIONS http://127.0.0.1:0/ -> 500   (recording entry 0)"
        );
        assert_eq!(
            super::pin_loopback_port("driving http://cdr.test:8080/ and 127.0.0.1:"),
            "driving http://cdr.test:8080/ and 127.0.0.1:"
        );
    }

    /// Two EHR ids the engine minted for one run, as the fixture run's output
    /// names them.
    const EHR_A: &str = "147a1cb7-ddf0-4757-afe3-6edb50287ea6";
    /// The second minted EHR id.
    const EHR_B: &str = "73262d17-8fcb-457b-88aa-99250eccb356";

    /// The ids the engine mints for generated inputs move on every pass, so
    /// the live tail shows them as stand-ins numbered by first sight: the
    /// same id keeps one stand-in across lines, two ids keep two, and the
    /// pinned run id, a commit hash and the engine's own words are untouched.
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    #[test]
    fn a_pinned_tail_numbers_every_minted_id() -> Result<(), crate::run_job::RunIdError> {
        let minted: RunId = MINTED.parse()?;
        let commit =
            "https://github.com/openEHR/x/blob/9d25ea783f790512e05f908ef743626b4f7e8d2c/a.yaml";
        let pinned = pin_job(JobView {
            id: minted,
            tail: vec![
                format!(
                    "Exchange PUT http://127.0.0.1:41234/ehr/{EHR_A} -> 500   (recording entry 52)"
                ),
                String::from(commit),
                format!(
                    "Exchange PUT http://127.0.0.1:41234/ehr/{} -> 500",
                    EHR_B.to_uppercase()
                ),
                format!("Exchange GET http://127.0.0.1:41234/ehr/{EHR_A} -> 500"),
                format!("wrote target/out/console-job-{MINTED}/results.json"),
            ],
            ..driving()
        });
        assert_eq!(
            pinned.tail,
            vec![
                String::from(
                    "Exchange PUT http://127.0.0.1:0/ehr/00000000-0000-0000-0000-000000000001 -> 500   (recording entry 52)"
                ),
                String::from(commit),
                String::from(
                    "Exchange PUT http://127.0.0.1:0/ehr/00000000-0000-0000-0000-000000000002 -> 500"
                ),
                String::from(
                    "Exchange GET http://127.0.0.1:0/ehr/00000000-0000-0000-0000-000000000001 -> 500"
                ),
                format!("wrote target/out/console-job-{PINNED_RUN_ID}/results.json"),
            ]
        );
        Ok(())
    }

    /// Two passes mint different ids for the same run, and the pinned tails
    /// are identical: the property the committed captures depend on.
    #[test]
    fn two_passes_pin_to_the_same_tail() {
        let pass = |first: &str, second: &str| {
            pin_job(JobView {
                tail: vec![format!("PUT /ehr/{first}"), format!("PUT /ehr/{second}")],
                ..driving()
            })
            .tail
        };
        assert_eq!(
            pass(EHR_A, EHR_B),
            pass(MINTED, "0f8fad5b-d9cb-469f-a165-70867728950e")
        );
    }

    /// A UUID-shaped run inside a longer word is not an id, and the scan
    /// survives multi-byte text around the ids it does rewrite.
    #[test]
    fn only_a_whole_uuid_is_an_id() {
        let mut ids = super::MintedIds::default();
        assert_eq!(
            ids.pin(&format!("x{EHR_A} {EHR_A}0 é{EHR_B}")),
            format!("x{EHR_A} {EHR_A}0 é00000000-0000-0000-0000-000000000001")
        );
        assert_eq!(ids.pin("short 147a1cb7-ddf0"), "short 147a1cb7-ddf0");
    }

    /// The judged exchange and the explanation beside it name the same
    /// minted id by the same stand-in, so the detail still reads as one
    /// exchange.
    #[test]
    fn a_pinned_detail_numbers_every_minted_id() {
        use crate::record_api::{ExchangeView, RecordingView, ResultDetail, ResultRow};
        let detail = ResultDetail {
            row: ResultRow {
                case: String::from("ehr_status/get_ehr_status-at_time_malformed"),
                row: 0,
                status: String::from("errored"),
                mark: String::from("errored"),
                glyph: String::from("?"),
                reason: None,
                formalism: None,
                trial: false,
                reports: Vec::new(),
            },
            summary: None,
            catalogue: None,
            step: None,
            what: None,
            why: None,
            expected: None,
            received: Some(format!("location /ehr/{EHR_B}")),
            whose: None,
            report: None,
            skip: None,
            recording: RecordingView::Exchange(ExchangeView {
                entry: 5,
                request_line: format!("PUT http://127.0.0.1:36803/ehr/{EHR_A}"),
                request_headers: String::from("host: 127.0.0.1:36803"),
                request_body: Some(format!("{{\"ehr_id\":\"{EHR_A}\"}}")),
                status_line: String::from("HTTP 500"),
                response_headers: String::new(),
                response_body: Some(String::from("no")),
            }),
        };
        let pinned = super::pin_detail(detail);
        assert_eq!(
            pinned.received.as_deref(),
            Some("location /ehr/00000000-0000-0000-0000-000000000001")
        );
        let RecordingView::Exchange(exchange) = pinned.recording else {
            panic!("an exchange stays an exchange");
        };
        assert_eq!(
            exchange.request_line,
            "PUT http://127.0.0.1:0/ehr/00000000-0000-0000-0000-000000000002"
        );
        assert_eq!(
            exchange.request_body.as_deref(),
            Some("{\"ehr_id\":\"00000000-0000-0000-0000-000000000002\"}")
        );
        assert_eq!(exchange.entry, 5, "the recording's own entry is kept");
    }
}
