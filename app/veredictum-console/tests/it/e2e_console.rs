// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The browser journeys (#69), driven by `scripts/ui-e2e.sh`.
//!
//! Rust-native `WebDriver` over headless Chromium, never Playwright, because
//! the console admits no hand-written JavaScript and its test suite is part of
//! the console. Every journey waits on explicit element and URL conditions,
//! and ends by reading the browser console and failing on any `SEVERE` entry,
//! hydration error or panic. Each test skips with a printed reason when
//! `UI_E2E_BASE_URL` is unset, so a plain `cargo nextest run` without a
//! composed console stays green.
//!
//! With `UI_E2E_DOCS_SHOTS=1` the same journeys also write the book's
//! screenshots (light and dark, one fixed viewport) into
//! `website/book/src/console/img/`, which is what makes every console change
//! produce a reviewable visual diff.

#![allow(
    clippy::panic,
    clippy::expect_used,
    reason = "a browser journey asserts by panicking, and the harness panics when a configured console cannot be driven"
)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the skip-with-reason and capture lines ARE this suite's report"
)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use leptos::server_fn::ServerFn;
use thirtyfour::ChromiumLikeCapabilities;
use thirtyfour::prelude::{By, DesiredCapabilities, ElementQueryable, WebDriver, WebElement};
use veredictum_console::run_api::StartOutcome;
use veredictum_console::run_api::fns::{CancelRun, ProbeAndSave, SaveScope, StartRun};

/// The budget every ordinary element or URL wait allows.
const WAIT: Duration = Duration::from_secs(15);

/// The interval between polls inside a wait.
const POLL: Duration = Duration::from_millis(200);

/// Polls per wait, so a loop-shaped condition gets the same budget as
/// [`WAIT`].
const POLLS: u32 = 75;

/// The budget [`Harness::wait_hydrated`] allows. Four times [`WAIT`], because
/// the harness serves a debug-profile WASM bundle the browser has to fetch,
/// compile and run before the marker appears — an order of magnitude slower
/// than any wait that only observes rendered HTML.
const HYDRATION_WAIT: Duration = Duration::from_mins(1);

/// The one capture viewport, so a screenshot diff shows a UI change and never
/// a window-size change.
const VIEWPORT: (u32, u32) = (1440, 900);

/// The settle a themed capture needs: the console's surfaces carry
/// `transition-colors`, and a screenshot racing the token switch freezes a
/// half-themed frame. An animation wait, and the only one in this file.
const THEME_SETTLE: Duration = Duration::from_millis(700);

/// Reads a harness variable, treating an empty value as unset.
fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// The book's console screenshot directory, resolved from this crate's
/// manifest directory.
fn book_img_dir() -> PathBuf {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    Path::new(root)
        .join("website")
        .join("book")
        .join("src")
        .join("console")
        .join("img")
}

/// One browser session plus everything a journey needs from it.
struct Harness {
    /// The `WebDriver` session.
    driver: WebDriver,
    /// The console origin the journeys navigate against.
    base: String,
    /// Where failure evidence is written.
    shots_dir: PathBuf,
    /// Where the book's captures are written, `None` outside capture mode.
    book_dir: Option<PathBuf>,
    /// The journey's slug, used in evidence file names.
    journey: &'static str,
}

impl Harness {
    /// Starts a journey, or returns `None` with a printed reason when the
    /// harness environment is absent.
    ///
    /// # Panics
    /// When the environment IS set but the browser session cannot start —
    /// that is a failure, never a skip.
    async fn start(journey: &'static str) -> Option<Self> {
        let (Some(base), Some(webdriver_url)) =
            (env("UI_E2E_BASE_URL"), env("UI_E2E_WEBDRIVER_URL"))
        else {
            eprintln!(
                "SKIP {journey}: UI_E2E_BASE_URL/UI_E2E_WEBDRIVER_URL unset (run scripts/ui-e2e.sh)"
            );
            return None;
        };
        let shots_dir = PathBuf::from(
            env("UI_E2E_SHOTS_DIR").unwrap_or_else(|| String::from("target/ui-e2e/screenshots")),
        );
        std::fs::create_dir_all(&shots_dir).expect("the evidence directory");
        let book_dir = env("UI_E2E_DOCS_SHOTS").map(|_| {
            let dir = book_img_dir();
            std::fs::create_dir_all(&dir).expect("the book screenshot directory");
            dir
        });

        let mut caps = DesiredCapabilities::chrome();
        caps.add_arg("--headless=new").expect("headless");
        caps.add_arg(&format!("--window-size={},{}", VIEWPORT.0, VIEWPORT.1))
            .expect("window size");
        // A retina host would otherwise capture at 2x and every committed
        // screenshot would change with the machine that took it.
        caps.add_arg("--force-device-scale-factor=1")
            .expect("device scale");
        // Without this the browser log endpoint answers empty and the console
        // gate below silently passes on every page.
        caps.set_logging_prefs("browser", thirtyfour::LoggingPrefsLogLevel::All)
            .expect("logging prefs");
        let driver = WebDriver::new(&webdriver_url, caps)
            .await
            .expect("a WebDriver session (is the browser endpoint up?)");
        Some(Self {
            driver,
            base,
            shots_dir,
            book_dir,
            journey,
        })
    }

    /// Navigates to a console path and waits until the page has hydrated.
    ///
    /// # Panics
    /// On navigation failure, or when the page never hydrates.
    async fn goto(&self, path: &str) {
        self.goto_unhydrated(path).await;
        self.wait_hydrated().await;
    }

    /// Navigates without waiting for hydration — for the routes' 404
    /// fallback, whose assertion is the server-rendered body itself.
    ///
    /// # Panics
    /// On navigation failure.
    async fn goto_unhydrated(&self, path: &str) {
        self.driver
            .goto(format!("{}{path}", self.base))
            .await
            .expect("navigate");
    }

    /// Waits until the client has taken over the page (`app.rs` stamps
    /// `data-hydrated` on `<body>` from a browser-only effect).
    ///
    /// # Panics
    /// When the marker never appears within [`HYDRATION_WAIT`].
    async fn wait_hydrated(&self) {
        self.wait_css_for("body[data-hydrated]", HYDRATION_WAIT)
            .await;
    }

    /// Waits for the first element matching `css`, within [`WAIT`].
    ///
    /// # Panics
    /// When the element never appears — with the selector and the page it was
    /// waiting on in the message.
    async fn wait_css(&self, css: &str) -> WebElement {
        self.wait_css_for(css, WAIT).await
    }

    /// [`Self::wait_css`] with an explicit budget, so the hydration wait can
    /// be long without lengthening every other wait.
    ///
    /// # Panics
    /// When the element never appears.
    async fn wait_css_for(&self, css: &str, budget: Duration) -> WebElement {
        match self
            .driver
            .query(By::Css(css))
            .wait(budget, POLL)
            .first()
            .await
        {
            Ok(element) => element,
            Err(e) => panic!("waiting for `{css}` {}: {e}", self.evidence("wait").await),
        }
    }

    /// Waits for an xpath under an explicit budget (a driven run's finish).
    ///
    /// # Panics
    /// When the element never appears — with the page and a failure
    /// screenshot in the message, exactly like the CSS waits, because a
    /// timeout with no picture is the one failure this suite cannot read.
    async fn wait_xpath_for(&self, xpath: &str, budget: Duration) -> WebElement {
        match self
            .driver
            .query(By::XPath(xpath))
            .wait(budget, POLL)
            .first()
            .await
        {
            Ok(element) => element,
            Err(e) => panic!(
                "waiting {budget:?} for xpath `{xpath}` {}: {e}",
                self.evidence("wait").await
            ),
        }
    }

    /// Waits for the first element matching `xpath`, within [`WAIT`].
    ///
    /// # Panics
    /// When the element never appears.
    async fn wait_xpath(&self, xpath: &str) -> WebElement {
        self.wait_xpath_for(xpath, WAIT).await
    }

    /// The path the browser is on, origin stripped, so a journey can navigate
    /// back to an address the console itself produced.
    ///
    /// # Panics
    /// When the current URL cannot be read.
    async fn current_path(&self) -> String {
        let url = self.driver.current_url().await.expect("current url");
        let mut path = String::from(url.path());
        if let Some(query) = url.query() {
            path.push('?');
            path.push_str(query);
        }
        path
    }

    /// Waits until the current URL contains `fragment`.
    ///
    /// # Panics
    /// When it never does within [`WAIT`].
    async fn wait_url_contains(&self, fragment: &str) {
        for _ in 0..POLLS {
            let url = self.driver.current_url().await.expect("current url");
            if url.as_str().contains(fragment) {
                return;
            }
            tokio::time::sleep(POLL).await;
        }
        let url = self.driver.current_url().await.expect("current url");
        panic!("the URL never contained `{fragment}` (last: {url})");
    }

    /// Clicks the shell's theme control and waits until the root class has
    /// caught up, returning once dark mode is on.
    ///
    /// # Panics
    /// When the control is absent or the class never appears.
    async fn enable_dark(&self) {
        self.wait_css("button[aria-label='Toggle dark mode']")
            .await
            .click()
            .await
            .expect("click the theme control");
        self.wait_css("html.dark").await;
        tokio::time::sleep(THEME_SETTLE).await;
    }

    /// Writes one book screenshot under `slug`, or does nothing outside
    /// capture mode.
    ///
    /// # Panics
    /// On capture or IO failure — a capture pass that silently wrote nothing
    /// would leave the book showing a stale UI.
    async fn capture(&self, slug: &str) {
        let Some(dir) = self.book_dir.as_ref() else {
            return;
        };
        let out = dir.join(format!("{slug}.png"));
        self.driver
            .screenshot(&out)
            .await
            .expect("write the book screenshot");
        println!("captured {slug} -> {}", out.display());
    }

    /// Where the browser is, plus a failure screenshot, as one line for a
    /// panic message.
    async fn evidence(&self, slug: &str) -> String {
        let url = self
            .driver
            .current_url()
            .await
            .map(|u| u.to_string())
            .unwrap_or_default();
        let path = self.shots_dir.join(format!("{}-{slug}.png", self.journey));
        drop(self.driver.screenshot(&path).await);
        format!("at {url} (evidence: {})", path.display())
    }

    /// The standing console gate: reads the browser log and fails on any
    /// `SEVERE` entry, and on any entry naming a hydration failure or a
    /// panic at any level.
    ///
    /// `allowed` waives an entry by substring, and exists only for a journey
    /// whose own deliberate negative step produces it — a browser logs a
    /// main-document 404 as an error, and the 404 is what that journey
    /// asserts. Every other journey passes an empty slice.
    ///
    /// # Panics
    /// When the log carries such an entry.
    async fn assert_console_clean(&self, allowed: &[&str]) {
        let entries = self
            .driver
            .get_log("browser")
            .await
            .expect("the browser log (the driver's legacy log endpoint)");
        let mut findings: Vec<String> = Vec::new();
        for entry in entries {
            if allowed.iter().any(|a| entry.message.contains(a)) {
                continue;
            }
            let lowered = entry.message.to_lowercase();
            let named = lowered.contains("hydrat") || lowered.contains("panic");
            if entry.level == "SEVERE" || named {
                findings.push(format!("[{}] {}", entry.level, entry.message));
            }
        }
        let at = self
            .driver
            .current_url()
            .await
            .map(|u| u.to_string())
            .unwrap_or_default();
        assert!(
            findings.is_empty(),
            "the browser console is not clean (last page: {at}):\n{}",
            findings.join("\n")
        );
    }

    /// Ends the session.
    ///
    /// # Panics
    /// When the session cannot be closed.
    async fn finish(self) {
        self.driver.quit().await.expect("quit the session");
    }
}

/// The landing renders the four catalogue counts as filled stat cards, and
/// the numbers are the catalogue's own rather than placeholders.
#[tokio::test]
async fn e2e_landing_shows_the_catalogue_counts() {
    let Some(h) = Harness::start("landing").await else {
        return;
    };
    h.goto("/").await;
    h.wait_css("#instrument-stats").await;
    let cards = h
        .driver
        .find_all(By::Css("#instrument-stats > *"))
        .await
        .expect("the stat cards");
    assert_eq!(cards.len(), 4, "the landing renders four stat cards");
    for card in &cards {
        let text = card.text().await.expect("a stat card's text");
        let value = text.lines().next().unwrap_or_default().trim().to_owned();
        assert!(
            value.chars().all(|c| c.is_ascii_digit()) && !value.is_empty(),
            "a stat card must lead with its count, not `{value}`"
        );
    }
    // The case count is the catalogue's, so a placeholder zero is a failure.
    let cases = h
        .wait_xpath("//*[@id='instrument-stats']//a[@href='/catalogue']")
        .await
        .text()
        .await
        .expect("the case-count card");
    assert!(
        !cases.starts_with('0'),
        "the case count must come from the mounted catalogue (read `{cases}`)"
    );
    h.capture("landing-light").await;
    h.enable_dark().await;
    h.capture("landing-dark").await;
    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// The sidebar reaches the catalogue, a chapter listing, and one case in
/// full — the case detail naming the citations its expectation stands on.
#[tokio::test]
async fn e2e_sidebar_reaches_a_case_detail() {
    let Some(h) = Harness::start("catalogue").await else {
        return;
    };
    h.goto("/").await;
    h.wait_css("nav[aria-label='Primary'] a[href='/catalogue']")
        .await
        .click()
        .await
        .expect("click the Catalogue entry");
    h.wait_url_contains("/catalogue").await;
    let chapter = h.wait_css("a[href^='/catalogue/']").await;
    let chapter_href = chapter
        .attr("href")
        .await
        .expect("the chapter link's href")
        .expect("a chapter link carries an href");
    h.capture("catalogue-light").await;

    chapter.click().await.expect("open the chapter");
    h.wait_url_contains(&chapter_href).await;
    h.wait_css("table tbody tr").await;
    h.capture("chapter-light").await;

    h.wait_css("table tbody tr td a")
        .await
        .click()
        .await
        .expect("open the case");
    h.wait_xpath("//h2[normalize-space()='Spec citation']")
        .await;
    h.wait_xpath("//h2[normalize-space()='Steps']").await;
    let step = h
        .wait_css("table tbody tr td span")
        .await
        .text()
        .await
        .expect("the first step");
    assert!(
        !step.trim().is_empty(),
        "a case detail lists the steps its workflow drives"
    );
    h.capture("case-light").await;

    // Dark is captured walking the same trail backwards: the preference is
    // stored, so each navigation re-applies it after hydration.
    h.enable_dark().await;
    h.capture("case-dark").await;
    h.goto(&chapter_href).await;
    h.wait_css("html.dark").await;
    h.wait_css("table tbody tr").await;
    tokio::time::sleep(THEME_SETTLE).await;
    h.capture("chapter-dark").await;
    h.goto("/catalogue").await;
    h.wait_css("html.dark").await;
    h.wait_css("a[href^='/catalogue/']").await;
    tokio::time::sleep(THEME_SETTLE).await;
    h.capture("catalogue-dark").await;

    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// The theme control flips the root class, and the choice survives a full
/// reload — the persisted preference is re-applied after hydration.
#[tokio::test]
async fn e2e_dark_mode_persists_across_a_reload() {
    let Some(h) = Harness::start("dark-mode").await else {
        return;
    };
    h.goto("/").await;
    let roots = h
        .driver
        .find_all(By::Css("html.dark"))
        .await
        .expect("the root element");
    assert!(roots.is_empty(), "the first paint is light");
    h.enable_dark().await;
    h.goto("/").await;
    h.wait_css("html.dark").await;
    h.wait_css("button[aria-label='Toggle dark mode']")
        .await
        .click()
        .await
        .expect("click the theme control");
    // The mirror direction: turning it off must persist as well, or the
    // console would come back dark for a reader who chose light.
    for _ in 0..POLLS {
        if h.driver
            .find_all(By::Css("html.dark"))
            .await
            .unwrap_or_default()
            .is_empty()
        {
            break;
        }
        tokio::time::sleep(POLL).await;
    }
    h.goto("/").await;
    h.wait_css("#instrument-stats").await;
    let still_dark = h
        .driver
        .find_all(By::Css("html.dark"))
        .await
        .expect("the root element");
    assert!(
        still_dark.is_empty(),
        "turning dark mode off must survive a reload too"
    );
    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// An unrouted path renders the designed 404 inside the console's own chrome
/// (#84) rather than a blank document, a bare string, or a server error.
#[tokio::test]
async fn e2e_unknown_route_renders_the_fallback() {
    let Some(h) = Harness::start("not-found").await else {
        return;
    };
    h.goto_unhydrated("/no-such-surface").await;
    // The server-rendered heading, so the page is the assertion before any
    // WASM loads; the chrome and the route home prove it is the designed
    // surface rather than the old bare string.
    h.wait_xpath("//h1[normalize-space()='Page not found']")
        .await;
    h.wait_css("nav[aria-label='Primary'] a[href='/catalogue']")
        .await;
    h.wait_xpath("//a[normalize-space()='Back to the instrument']")
        .await;
    // The path that missed is named, so the reader knows what was asked for.
    h.wait_xpath("//body[contains(., '/no-such-surface')]")
        .await;
    h.wait_hydrated().await;
    h.capture("not-found-light").await;
    h.enable_dark().await;
    h.capture("not-found-dark").await;
    // The waived entry is the 404 this journey exists to provoke: the browser
    // logs a main-document 404 as a page error, and the server answering 404
    // for an unrouted path is the assertion.
    h.assert_console_clean(&["/no-such-surface - Failed to load resource"])
        .await;
    h.finish().await;
}

/// The wizard's first two steps render: /run redirects to Connect, the form
/// and the auth control exist, and Scope carries its own selection controls.
///
/// Scope is asserted by its OWN structure, never by the absence of a draft
/// (#135). The console now keeps a draft per submitter (#389), and every
/// journey drives the SAME browser, so they are one submitter sharing one
/// draft: an assertion on the empty-draft copy would still pass or fail by
/// which journey nextest happened to run first. The heading, the claim box,
/// the filter and the two controls are on the page whatever the draft holds.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_run_wizard_reaches_connect_and_scope() {
    let Some(h) = Harness::start("run-wizard").await else {
        return;
    };
    h.goto("/run/connect").await;
    h.wait_xpath("//h1[contains(., 'Grade a server')]").await;
    h.wait_xpath("//button[contains(., 'Probe connection')]")
        .await;
    h.wait_xpath("//button[contains(., 'Basic')]").await;
    h.capture("connect-light").await;
    h.enable_dark().await;
    h.capture("connect-dark").await;
    h.goto("/run/scope").await;
    h.wait_xpath("//h1[contains(., 'Scope')]").await;
    h.wait_css("textarea#statement-json").await;
    h.wait_css("input#filter").await;
    h.wait_xpath("//button[contains(., 'Preview selection')]")
        .await;
    h.wait_xpath("//button[contains(., 'Save scope')]").await;
    // #100: the capability row and its counts, and the refusal an empty
    // selection earns. Both are draft-independent: the composer refuses a
    // claim with no capability before it ever reads the draft, so this
    // journey stays order-free.
    for capability in ["ehr", "system", "query"] {
        h.wait_css(&format!("input#capability-{capability}")).await;
    }
    h.wait_xpath("//label[contains(., 'ehr') and contains(., 'cases')]")
        .await;
    h.wait_xpath("//button[contains(., 'Compose the claim')]")
        .await
        .click()
        .await
        .expect("compose with nothing checked");
    h.wait_xpath("//body[contains(., 'check at least one capability')]")
        .await;
    // The connection pane resolves either way — a draft summary labelled
    // `connection`, or the honest "No connection draft" answer — so what is
    // asserted is that the Suspense RESOLVED, never which of the two it
    // landed on.
    h.wait_xpath("//body[contains(., 'connection')]").await;
    // A refused server function answers 500, which the browser logs as a
    // failed resource: the refusal above is the assertion, and this ONE
    // endpoint's status line is what driving it deliberately costs.
    h.assert_console_clean(&["/api/compose_claim"]).await;
    h.finish().await;
}

/// A minimal fixture SUT for the driven-run journey: answers every request
/// `500` deterministically, in THIS test process — the console server on the
/// same host reaches it over loopback. The thread ends with the process.
fn fixture_sut() -> Result<u16, std::io::Error> {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut scratch = [0_u8; 4096];
            let _bytes_read = stream.read(&mut scratch);
            let _write = stream.write_all(
                b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 2\r\nconnection: close\r\n\r\nno",
            );
        }
    });
    Ok(port)
}

/// One system under test the wizard drives end to end.
struct DrivenSut<'a> {
    /// The openEHR REST base URL the connect form receives.
    base_url: &'a str,
    /// The SUT identity typed into the form (name, version).
    identity: (&'a str, &'a str),
    /// Basic credentials, when the SUT wants them.
    basic: Option<(&'a str, &'a str)>,
    /// The document-id filter for the run.
    filter: &'a str,
    /// The capability folder the composed claim checks.
    capability: &'a str,
    /// The probe outcome the connect screen must show before continuing.
    probe_answer: &'a str,
    /// The continue control's label ("Continue" on a 2xx probe).
    continue_label: &'a str,
    /// A version label typed AFTER the probe, which the run must record
    /// (#670); `None` leaves the probed label alone.
    version_after_probe: Option<&'a str>,
    /// The finish budget for the live screen's poll.
    finish_budget: Duration,
}

/// Checks one capability and composes the claim into the paste box (#100).
///
/// This IS the claim the journey grades. Since #465 the console offers no
/// committed declaration to load over it: the support columns of an ICS
/// proforma belong to the supplier of the implementation (ISO/IEC 9646-7), so
/// pasting or composing are the only ways one enters.
async fn compose_claim(h: &Harness, capability: &str) {
    h.wait_css(&format!("input#capability-{capability}"))
        .await
        .click()
        .await
        .expect("check the capability");
    h.wait_xpath("//button[contains(., 'Compose the claim')]")
        .await
        .click()
        .await
        .expect("compose the claim");
    h.wait_xpath("//p[contains(., 'Composed from the checked capabilities')]")
        .await;
    // The box is driven by `prop:value`, so its DOM text never changes: the
    // live value is the property, which is what the operator reads.
    let composed = h
        .wait_css("textarea#statement-json")
        .await
        .prop("value")
        .await
        .expect("read the claim box");
    assert!(
        composed.is_some_and(|body| body.contains("urn:veredictum:console:")),
        "the composed claim must land in the paste box"
    );
}

/// Drives connect → probe → scope → start → live, returning the run's own
/// address with the run finished on the Live screen. Captures are the
/// caller's business.
async fn drive_wizard(h: &Harness, sut: &DrivenSut<'_>, scope_shot: Option<&str>) -> String {
    h.goto("/run/connect").await;
    let base = h.wait_css("input#base-url").await;
    base.clear().await.expect("clear the base URL");
    base.send_keys(sut.base_url)
        .await
        .expect("type the base URL");
    let (name, version) = sut.identity;
    let name_field = h.wait_css("input#sut-name").await;
    name_field.clear().await.expect("clear the name");
    name_field.send_keys(name).await.expect("type the name");
    let version_field = h.wait_css("input#sut-version").await;
    version_field.clear().await.expect("clear the version");
    version_field
        .send_keys(version)
        .await
        .expect("type the version");
    if let Some((user, password)) = sut.basic {
        h.wait_xpath("//button[contains(., 'Basic')]")
            .await
            .click()
            .await
            .expect("pick basic auth");
        h.wait_css("input#sut-user")
            .await
            .send_keys(user)
            .await
            .expect("type the user");
        h.wait_css("input#sut-pass")
            .await
            .send_keys(password)
            .await
            .expect("type the password");
    }
    h.wait_xpath("//button[contains(., 'Probe connection')]")
        .await
        .click()
        .await
        .expect("probe");
    h.wait_xpath(&format!("//body[contains(., '{}')]", sut.probe_answer))
        .await;
    if let Some(edited) = sut.version_after_probe {
        edit_after_probe(h, sut.base_url, edited).await;
    }
    h.wait_xpath(&format!(
        "//button[@id='connect-continue' and contains(., '{}')]",
        sut.continue_label
    ))
    .await
    .click()
    .await
    .expect("continue");

    h.wait_xpath("//h1[contains(., 'Scope')]").await;
    compose_claim(h, sut.capability).await;
    h.wait_css("input#filter")
        .await
        .send_keys(sut.filter)
        .await
        .expect("type the filter");
    if let Some(slug) = scope_shot {
        h.capture(slug).await;
    }
    h.wait_xpath("//button[contains(., 'Save scope')]")
        .await
        .click()
        .await
        .expect("save");
    h.wait_xpath("//p[contains(., 'Claim accepted')]").await;
    h.wait_xpath("//button[contains(., 'Start the run')]")
        .await
        .click()
        .await
        .expect("start");
    h.wait_xpath("//a[contains(., 'Watch it live')]")
        .await
        .click()
        .await
        .expect("to live");

    let address = follow_the_run(h).await;
    h.wait_xpath_for("//span[contains(., 'finished')]", sut.finish_budget)
        .await;
    address
}

/// Edits the connect step after its probe (#670): an address edit withholds
/// Continue and says a new probe is needed, and after that probe a version
/// label typed into the form is the one Continue carries to the run.
///
/// # Panics
/// When the screen offers Continue over an address the probe never tested.
async fn edit_after_probe(h: &Harness, base_url: &str, version: &str) {
    let base = h.wait_css("input#base-url").await;
    base.clear().await.expect("clear the base URL");
    base.send_keys(base_url).await.expect("retype the base URL");
    h.wait_css("p#connect-stale").await;
    assert!(
        h.driver
            .find_all(By::Css("button#connect-continue"))
            .await
            .expect("query the continue control")
            .is_empty(),
        "Continue must wait for a probe of the edited address"
    );
    h.wait_xpath("//button[contains(., 'Probe connection')]")
        .await
        .click()
        .await
        .expect("probe again");
    h.wait_css("button#connect-continue").await;
    let version_field = h.wait_css("input#sut-version").await;
    version_field.clear().await.expect("clear the version");
    version_field
        .send_keys(version)
        .await
        .expect("type the edited version");
}

/// Reads the run's own address off the live screen and reloads it mid-run,
/// returning that address.
///
/// A reload must rejoin the SAME run (#386): before the URL carried the id,
/// the page could only ask whether anything was in flight here, and answered
/// "no run is in flight" about a run that was still executing.
///
/// The permalink is asserted by its href rather than by its text, because
/// documentation capture mode pins the run id it DISPLAYS so an unchanged
/// console rewrites no screenshot; the address the page is actually serving
/// is the URL, and that is what a reload has to preserve.
///
/// # Panics
/// When the live link carries no id, when the reload lands on another run, or
/// when the screen offers no permalink at all.
async fn follow_the_run(h: &Harness) -> String {
    h.wait_xpath("//h1[contains(., 'Live run')]").await;
    let address = h.current_path().await;
    assert!(
        address.starts_with("/run/live/") && address.len() > "/run/live/".len(),
        "the live link must carry the run's id: {address}"
    );
    h.goto(&address).await;
    h.wait_xpath("//h1[contains(., 'Live run')]").await;
    h.wait_xpath("//a[starts-with(@href, '/run/live/')]").await;
    assert_eq!(
        h.current_path().await,
        address,
        "a reload mid-run follows the same run"
    );
    address
}

/// Walks the finished run's results with one URL-addressed detail, capturing
/// it under the given slug. `wire` asserts the drawer shows the judged
/// exchange rather than the absence line.
async fn read_record(h: &Harness, case_needle: &str, wire: bool, results_shot: &str) {
    h.goto("/run/results").await;
    h.wait_xpath("//h1[contains(., 'Results')]").await;
    h.wait_xpath(&format!("//td//a[contains(., '{case_needle}')]"))
        .await
        .click()
        .await
        .expect("open the detail");
    h.wait_xpath("//body[contains(., 'The wire')]").await;
    if wire {
        h.wait_xpath("//body[contains(., 'recording entry')]").await;
        h.wait_xpath("//span[contains(., 'response body')]").await;
    }
    h.capture(results_shot).await;
}

/// Reads a finished run whose results hold failed AND errored rows (#655):
/// both appear in the one table under their own status chips, and an errored
/// row opens to the reason the instrument could not judge it.
///
/// # Panics
/// When either status is missing from the table, or the errored row's detail
/// carries no reason.
async fn read_errored_beside_failed(h: &Harness, results_shot: &str) {
    h.goto("/run/results").await;
    h.wait_xpath("//h1[contains(., 'Results')]").await;
    let failed = h
        .wait_xpath("//td//span[normalize-space()='failed']")
        .await
        .attr("class")
        .await
        .expect("read the failed chip");
    let errored = h
        .wait_xpath("//td//span[normalize-space()='errored']")
        .await
        .attr("class")
        .await
        .expect("read the errored chip");
    assert_ne!(
        failed, errored,
        "an errored row must be visibly distinct from a failed one"
    );
    h.wait_xpath("//tr[.//span[normalize-space()='errored']]//td//a")
        .await
        .click()
        .await
        .expect("open the errored row");
    h.wait_xpath("//h2[contains(., '· row')]").await;
    h.wait_xpath("//span[contains(., 'received')]").await;
    h.wait_xpath("//body[contains(., 'The wire')]").await;
    h.capture(results_shot).await;
}

/// A fixture SUT that states its identity: `OPTIONS /` answers a 200
/// `Options` document (ITS-REST `specifications/operations/options.yaml`),
/// and every other request answers `500`, in THIS test process.
fn options_fixture_sut() -> Result<u16, std::io::Error> {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    std::thread::spawn(move || {
        let body = br#"{"solution":"fixture-cdr","solution_version":"9.8.7","vendor":"Fixture Vendor","restapi_specs_version":"1.1.0"}"#;
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut scratch = [0_u8; 4096];
            let read = stream.read(&mut scratch).unwrap_or(0);
            let head = scratch.get(..read).unwrap_or_default();
            let answer = if head.starts_with(b"OPTIONS / ") {
                let mut answer = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                )
                .into_bytes();
                answer.extend_from_slice(body);
                answer
            } else {
                b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 2\r\nconnection: close\r\n\r\nno"
                    .to_vec()
            };
            let _write = stream.write_all(&answer);
        }
    });
    Ok(port)
}

/// A server that states its identity through System OPTIONS shows it
/// read-only, labelled as reported by the server, with the vendor and the REST
/// specification version beside it; the operator's inputs give way.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_connect_shows_the_identity_the_server_states() {
    let Some(h) = Harness::start("connect-identity").await else {
        return;
    };
    let Ok(port) = options_fixture_sut() else {
        panic!("the fixture SUT could not bind");
    };
    h.goto("/run/connect").await;
    let base = h.wait_css("input#base-url").await;
    base.clear().await.expect("clear the base URL");
    base.send_keys(&format!("http://127.0.0.1:{port}"))
        .await
        .expect("type the base URL");
    h.wait_xpath("//button[contains(., 'Probe connection')]")
        .await
        .click()
        .await
        .expect("probe");
    h.wait_xpath("//p[@id='sut-name-reported' and normalize-space()='fixture-cdr']")
        .await;
    h.wait_xpath("//p[@id='sut-version-reported' and normalize-space()='9.8.7']")
        .await;
    h.wait_xpath("//p[@id='sut-vendor-reported' and normalize-space()='Fixture Vendor']")
        .await;
    h.wait_xpath("//p[@id='sut-rest-version-reported' and normalize-space()='1.1.0']")
        .await;
    h.wait_xpath("//body[contains(., 'reported by the server')]")
        .await;
    h.wait_xpath("//button[contains(., 'Continue anyway')]")
        .await;
    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// The whole pipeline through the real UI: connect → probe → scope with a
/// statement → start → live to finished → results with a detail — capturing
/// every record surface, light and dark, against the hermetic in-process
/// fixture (every request answered 500).
#[tokio::test(flavor = "multi_thread")]
async fn e2e_wizard_drives_a_run_to_its_results() {
    let Some(h) = Harness::start("driven-run").await else {
        return;
    };
    let Ok(port) = fixture_sut() else {
        panic!("the fixture SUT could not bind");
    };
    let base_url = format!("http://127.0.0.1:{port}");
    let sut = DrivenSut {
        base_url: &base_url,
        identity: ("my-cdr", "1.0.0"),
        basic: None,
        filter: "ehr_status",
        capability: "ehr_status",
        probe_answer: "HTTP 500",
        continue_label: "Continue anyway",
        version_after_probe: Some("1.0.1"),
        finish_budget: Duration::from_mins(2),
    };
    let address = drive_wizard(&h, &sut, Some("scope-light")).await;
    // The run records the label as the screen showed it at Continue (#670).
    h.wait_xpath("//span[@id='live-sut' and normalize-space()='my-cdr 1.0.1']")
        .await;
    h.capture("live-light").await;
    read_errored_beside_failed(&h, "results-light").await;

    // A link to the finished run's own id shows that run, after the reader
    // walked away to the record surfaces (#386). The permalink is asserted by
    // its href: capture mode pins the id the screen DISPLAYS, and the address
    // being served is the URL.
    h.goto(&address).await;
    h.wait_xpath("//span[contains(., 'finished')]").await;
    h.wait_xpath("//a[starts-with(@href, '/run/live/')]").await;

    // The dark pass re-walks the finished run's surfaces: the job state
    // persists, so each page renders the same record in the other theme.
    h.enable_dark().await;
    h.goto("/run/scope").await;
    h.wait_xpath("//h1[contains(., 'Scope')]").await;
    h.capture("scope-dark").await;
    h.goto(&address).await;
    h.wait_xpath("//span[contains(., 'finished')]").await;
    h.capture("live-dark").await;
    h.goto("/run/results").await;
    h.wait_xpath("//h1[contains(., 'Results')]").await;
    h.capture("results-dark").await;

    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// A run id this console never drove says so about the RUN, and never "no run
/// is in flight", which is only true of a request that named none (#386). An
/// address that is not a run id at all lands in the same state with the parse
/// reason beside it, rather than a blank page or a panic.
#[tokio::test]
async fn e2e_an_unknown_run_id_says_so_in_its_own_words() {
    let Some(h) = Harness::start("unknown-run").await else {
        return;
    };
    // Addressed by id, so this journey is independent of whatever run another
    // journey may be driving through the same console.
    h.goto("/run/live/3f2504e0-4f89-41d3-9a0c-0305e82c3301")
        .await;
    h.wait_xpath("//body[contains(., 'knows nothing about run')]")
        .await;
    h.wait_xpath("//body[contains(., '3f2504e0-4f89-41d3-9a0c-0305e82c3301')]")
        .await;

    h.goto("/run/live/not-a-run-id").await;
    h.wait_xpath("//body[contains(., 'does not name a run')]")
        .await;
    h.wait_xpath("//h1[contains(., 'Live run')]").await;
    // The caveat is page furniture on every state of this screen (#388).
    h.wait_xpath("//body[contains(., 'keeps nothing durable')]")
        .await;

    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// The side-by-side grading (#99): the same wizard against two REAL CDRs —
/// FerroEHR's quickstart and EHRbase's official pairing, both latest — so the
/// captures show two records over one catalogue. Skips unless the harness
/// composed the SUTs (`UI_E2E_REAL_SUTS=1 scripts/ui-e2e.sh`).
#[tokio::test(flavor = "multi_thread")]
async fn e2e_wizard_grades_the_real_cdrs() {
    let ferroehr = std::env::var("UI_E2E_FERROEHR_URL").unwrap_or_default();
    let ehrbase = std::env::var("UI_E2E_EHRBASE_URL").unwrap_or_default();
    if ferroehr.is_empty() || ehrbase.is_empty() {
        println!(
            "skipping: UI_E2E_FERROEHR_URL / UI_E2E_EHRBASE_URL are unset (run with UI_E2E_REAL_SUTS=1)"
        );
        return;
    }
    let Some(h) = Harness::start("real-cdrs").await else {
        return;
    };
    let suts = [
        DrivenSut {
            base_url: &ferroehr,
            identity: ("FerroEHR", "latest"),
            basic: Some(("ferroehr", "ferroehr")),
            filter: "ehr",
            capability: "ehr",
            probe_answer: "The server answered",
            continue_label: "Continue",
            version_after_probe: None,
            finish_budget: Duration::from_mins(5),
        },
        DrivenSut {
            base_url: &ehrbase,
            identity: ("EHRbase", "latest"),
            basic: Some(("ehrbase-user", "SuperSecretPassword")),
            filter: "ehr",
            capability: "ehr",
            probe_answer: "The server answered",
            continue_label: "Continue",
            version_after_probe: None,
            finish_budget: Duration::from_mins(5),
        },
    ];
    for sut in &suts {
        let slug = sut.identity.0.to_lowercase();
        drive_wizard(&h, sut, None).await;
        h.capture(&format!("live-{slug}-light")).await;
        read_record(&h, "ehr/", false, &format!("results-{slug}-light")).await;
    }

    h.assert_console_clean(&[]).await;
    h.finish().await;
}
/// A fixture SUT that answers `500` after `delay`, so a run driven against it
/// is still in flight while a journey navigates.
///
/// The concurrency journey asserts that two runs are executing AT THE SAME
/// TIME, and a run that ends before the next navigation cannot show that.
fn slow_fixture_sut(delay: Duration) -> Result<u16, std::io::Error> {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut scratch = [0_u8; 4096];
                let _bytes_read = stream.read(&mut scratch);
                std::thread::sleep(delay);
                let _write = stream.write_all(
                    b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 2\r\nconnection: close\r\n\r\nno",
                );
            });
        }
    });
    Ok(port)
}

/// A visitor beside the browser: the same public server-function endpoints
/// the browser posts to, called under a client address of this journey's own.
///
/// The console reads a forwarded address only from the header its operator
/// named, so this identity exists only because `scripts/ui-e2e.sh` names one.
/// The browser sends no such header and keeps its socket peer, which is what
/// makes these calls a different submitter on one process.
struct ApiVisitor {
    /// The console's host-side origin (the browser's own origin may name a
    /// host only the browser container can resolve).
    origin: String,
    /// The header the console trusts for the client address.
    header: String,
    /// The address this visitor claims.
    address: &'static str,
    /// The HTTP client.
    client: reqwest::Client,
}

impl ApiVisitor {
    /// Builds a visitor claiming `address`, or returns `None` with a printed
    /// reason when the harness did not configure a trusted header.
    fn new(address: &'static str) -> Option<Self> {
        let (Some(origin), Some(header)) = (env("UI_E2E_HOST_URL"), env("UI_E2E_CLIENT_IP_HEADER"))
        else {
            eprintln!(
                "SKIP two-visitors: UI_E2E_HOST_URL/UI_E2E_CLIENT_IP_HEADER unset (run scripts/ui-e2e.sh)"
            );
            return None;
        };
        Some(Self {
            origin,
            header,
            address,
            client: reqwest::Client::new(),
        })
    }

    /// Posts one URL-encoded server-function call and returns its JSON body.
    ///
    /// # Panics
    /// On any transport failure or non-success status — a journey that
    /// silently skipped its second visitor would assert nothing.
    async fn call(&self, path: &str, form: &str) -> String {
        let url = format!("{}{path}", self.origin);
        let response = self
            .client
            .post(&url)
            .header(self.header.as_str(), self.address)
            .header("accept", "application/json")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(form.to_owned())
            .send()
            .await
            .unwrap_or_else(|e| panic!("POST {url}: {e}"));
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        assert!(status.is_success(), "POST {url} answered {status}: {body}");
        body
    }

    /// Posts `form` and returns the status with the body, refusing nothing.
    ///
    /// The sibling `call` asserts success, which is right for every step of a
    /// journey; a refusal is the subject here rather than a failure.
    async fn call_expecting_a_refusal(
        &self,
        path: &str,
        form: &str,
    ) -> (reqwest::StatusCode, String) {
        let url = format!("{}{path}", self.origin);
        let response = self
            .client
            .post(&url)
            .header(self.header.as_str(), self.address)
            .header("accept", "application/json")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(form.to_owned())
            .send()
            .await
            .unwrap_or_else(|e| panic!("POST {url}: {e}"));
        let status = response.status();
        (status, response.text().await.unwrap_or_default())
    }

    /// Drives connect, scope and start as this visitor, returning the run's
    /// own address.
    ///
    /// # Panics
    /// When the console does not accept the run — the second visitor has no
    /// run in flight, so anything but an acceptance is a real failure.
    async fn start_a_run(&self, sut_base_url: &str, sut_name: &str, filter: &str) -> String {
        self.call(
            <ProbeAndSave as ServerFn>::PATH,
            &format!(
                "base_url={}&sut_name={sut_name}&sut_version=0.0.0-gate\
                 &server[auth]=None&server[user]=&server[password]=&server[token]=\
                 &admin[auth]=None&admin[user]=&admin[password]=&admin[token]=",
                encode(sut_base_url)
            ),
        )
        .await;
        // `postures` is a REQUIRED argument, so this visitor declares one like
        // any other caller: an undeclared posture is a stated absence rather
        // than an omitted field. A form that leaves it out is refused, which is
        // the behaviour the console wants and the reason this is spelled here
        // instead of the argument being made optional.
        self.call(
            <SaveScope as ServerFn>::PATH,
            &format!(
                "filter={}\
                 &postures[system_id]=&postures[dump_location]=\
                 &postures[signing]=Undeclared&postures[digest_encoding]=Base64\
                 &postures[digest_prefix]=&postures[pgp_public_key]=\
                 &postures[spec_profile]=Undeclared",
                encode(filter)
            ),
        )
        .await;
        let answer = self.call(<StartRun as ServerFn>::PATH, "").await;
        let outcome: StartOutcome = serde_json::from_str(&answer)
            .unwrap_or_else(|e| panic!("start answered `{answer}`: {e}"));
        match outcome {
            StartOutcome::Accepted(id) => format!("/run/live/{id}"),
            StartOutcome::AlreadyInFlight(id) => {
                panic!("the second visitor already had run {id} in flight")
            }
        }
    }

    /// Cancels the run at `address`, so the journey leaves no slot occupied.
    ///
    /// Cancel addresses a run by id and is not scoped to whoever started it,
    /// which is the same property that lets anyone holding a run's URL read
    /// it — so this ends both visitors' runs.
    async fn stop(&self, address: &str) {
        let Some(id) = address.rsplit('/').next() else {
            return;
        };
        drop(
            self.client
                .post(format!("{}{}", self.origin, <CancelRun as ServerFn>::PATH))
                .header(self.header.as_str(), self.address)
                .header("accept", "application/json")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(format!("id={id}"))
                .send()
                .await,
        );
    }
}

/// Percent-encodes a form value's reserved characters.
///
/// Only what these calls actually carry: a URL and a case-id filter. Anything
/// outside the unreserved set becomes `%XX`, which is always correct even
/// where it was not required.
fn encode(value: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Reads the whole rendered page as text.
///
/// # Panics
/// When the body cannot be read.
async fn page_text(h: &Harness) -> String {
    h.wait_css("body")
        .await
        .text()
        .await
        .expect("the page's text")
}

/// Several cases against a two-second SUT: both runs of the concurrency
/// journey stay in flight for its whole length, and both are cancelled before
/// it ends.
const TWO_VISITOR_FILTER: &str = "ehr";

/// The #389 journey: two runs drive at once on ONE console, each followed by
/// its own URL, neither screen showing the other's progress or outcome, and a
/// third start waiting in a queue that states its place.
///
/// The three visitors are the browser (its socket peer) and two sets of HTTP
/// calls under forwarded addresses the harness told the console to trust. The
/// screens are compared by the SUT display name and by the served URL, never
/// by the run id the page prints: documentation capture mode pins that id, so
/// every live screen displays the same one.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_two_visitors_drive_two_runs_at_once() {
    let Some(h) = Harness::start("two-visitors").await else {
        return;
    };
    let (Some(second), Some(third)) = (
        ApiVisitor::new("203.0.113.77"),
        ApiVisitor::new("203.0.113.78"),
    ) else {
        h.finish().await;
        return;
    };
    let Ok(port) = slow_fixture_sut(Duration::from_secs(2)) else {
        panic!("the fixture SUT could not bind");
    };
    let base_url = format!("http://127.0.0.1:{port}");

    // Visitor one is the browser, driving the wizard.
    let first = drive_to_live(&h, &base_url, "first-visitor", TWO_VISITOR_FILTER).await;
    h.wait_xpath("//span[contains(., 'running')]").await;

    // A malformed call is the CALLER's mistake and answers as one (#484).
    // Every `#[server]` fn is a public endpoint, so this is part of the
    // interface: the same form with `postures` left out, which server_fn
    // answers with 500 and its own serializer's phrasing unless something
    // rewrites it.
    let (status, body) = second
        .call_expecting_a_refusal(<SaveScope as ServerFn>::PATH, "filter=")
        .await;
    assert_eq!(
        status,
        reqwest::StatusCode::BAD_REQUEST,
        "a caller's mistake is 4xx, not 5xx: {body}"
    );
    assert!(
        body.contains("`postures`"),
        "the refusal names the argument: {body}"
    );
    assert!(
        !body.contains("Args|"),
        "the serializer's own phrasing does not reach a caller: {body}"
    );

    // Visitor two is this journey, over the same public endpoints.
    let other = second
        .start_a_run(&base_url, "second-visitor", TWO_VISITOR_FILTER)
        .await;
    assert_ne!(other, first, "two runs, two addresses");

    // Each address answers about its own run and says nothing of the other.
    h.goto(&other).await;
    assert_eq!(h.current_path().await, other, "the URL is the run served");
    h.wait_xpath("//h2[contains(., 'second-visitor')]").await;
    h.wait_xpath("//span[contains(., 'running')]").await;
    let second_page = page_text(&h).await;
    assert!(
        !second_page.contains("first-visitor"),
        "the second run's screen showed the first run:\n{second_page}"
    );

    h.goto(&first).await;
    assert_eq!(h.current_path().await, first, "the URL is the run served");
    h.wait_xpath("//h2[contains(., 'first-visitor')]").await;
    h.wait_xpath("//span[contains(., 'running')]").await;
    let first_page = page_text(&h).await;
    assert!(
        !first_page.contains("second-visitor"),
        "the first run's screen showed the second run:\n{first_page}"
    );
    // The permalink is asserted by its href SHAPE and the served address by
    // the URL: capture mode pins the id the page displays, and it pins the
    // same one for both runs.
    h.wait_xpath("//a[starts-with(@href, '/run/live/')]").await;

    // Visitor three arrives at a full instance: accepted, addressable at once,
    // and told its place rather than left on a spinner.
    let waiting = third
        .start_a_run(&base_url, "third-visitor", TWO_VISITOR_FILTER)
        .await;
    h.goto(&waiting).await;
    assert_eq!(h.current_path().await, waiting, "the URL is the run served");
    h.wait_xpath("//h2[contains(., 'third-visitor')]").await;
    h.wait_xpath("//span[contains(., 'queued') and contains(., 'position 1')]")
        .await;
    h.wait_xpath("//button[contains(., 'Leave the queue')]")
        .await;

    // No run may outlive the journey: the instance has two slots, and the
    // queued one is dropped first so freeing a slot promotes nothing. Every
    // run is ended by ADDRESS rather than through the screen's own control,
    // because documentation capture mode pins the id the live view carries
    // and that control would dispatch the pinned one.
    h.wait_xpath("//h1[contains(., 'Live run')]").await;
    third.stop(&waiting).await;
    second.stop(&first).await;
    second.stop(&other).await;

    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// Drives connect → probe → scope → start → live in the browser, returning
/// the run's own address with the run still driving.
///
/// A lean sibling of [`drive_wizard`]: no claim is composed and nothing is
/// waited on to finish, because what this journey needs is a run that is
/// still executing.
///
/// # Panics
/// On any control the wizard does not offer, or an address carrying no id.
async fn drive_to_live(h: &Harness, base_url: &str, sut_name: &str, filter: &str) -> String {
    h.goto("/run/connect").await;
    let base = h.wait_css("input#base-url").await;
    base.clear().await.expect("clear the base URL");
    base.send_keys(base_url).await.expect("type the base URL");
    let name_field = h.wait_css("input#sut-name").await;
    name_field.clear().await.expect("clear the name");
    name_field.send_keys(sut_name).await.expect("type the name");
    h.wait_xpath("//button[contains(., 'Probe connection')]")
        .await
        .click()
        .await
        .expect("probe");
    h.wait_xpath("//body[contains(., 'HTTP 500')]").await;
    h.wait_xpath("//button[contains(., 'Continue anyway')]")
        .await
        .click()
        .await
        .expect("continue");

    h.wait_xpath("//h1[contains(., 'Scope')]").await;
    let filter_field = h.wait_css("input#filter").await;
    filter_field.clear().await.expect("clear the filter");
    filter_field
        .send_keys(filter)
        .await
        .expect("type the filter");
    h.wait_xpath("//button[contains(., 'Save scope')]")
        .await
        .click()
        .await
        .expect("save");
    h.wait_xpath("//body[contains(., 'Scope saved')]").await;
    h.wait_xpath("//button[contains(., 'Start the run')]")
        .await
        .click()
        .await
        .expect("start");
    h.wait_xpath("//a[contains(., 'Watch it live')]")
        .await
        .click()
        .await
        .expect("to live");
    h.wait_xpath("//h1[contains(., 'Live run')]").await;
    let address = h.current_path().await;
    assert!(
        address.starts_with("/run/live/") && address.len() > "/run/live/".len(),
        "the live link must carry the run's id: {address}"
    );
    address
}

/// The row links on the results page.
const RESULT_ROW_LINKS: &str = "//td//a[starts-with(@href, '/run/results?case=')]";

/// The heading of the open result detail.
const RESULT_DETAIL_HEADING: &str = "//h2[contains(., '· row')]";

/// One click on a result row opens its detail where the reader is looking
/// (#869), "Back to the row" lands on that row again, marked as the one
/// opened (#867), and the detail reaches the catalogue case the row came
/// from.
///
/// The run covers a whole capability, so the row list runs far past the first
/// screen and the row clicked is the last one. The detail must sit inside the
/// first screen after the click (the router scrolls a navigation to the top),
/// and the same address loaded afresh must carry it in the server-rendered
/// page, which is what a reader without JavaScript receives.
///
/// # Panics
/// When the detail does not open, opens out of sight, leads back to anything
/// but the marked row, or names no catalogue case.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_a_result_row_opens_its_detail() {
    let Some(h) = Harness::start("result-detail").await else {
        return;
    };
    let Ok(port) = fixture_sut() else {
        panic!("the fixture SUT could not bind");
    };
    let base_url = format!("http://127.0.0.1:{port}");
    let sut = DrivenSut {
        base_url: &base_url,
        identity: ("my-cdr", "1.0.0"),
        basic: None,
        filter: "",
        capability: "ehr",
        probe_answer: "HTTP 500",
        continue_label: "Continue anyway",
        version_after_probe: None,
        finish_budget: Duration::from_mins(3),
    };
    drive_wizard(&h, &sut, None).await;
    h.goto("/run/results").await;
    h.wait_xpath("//h1[contains(., 'Results')]").await;
    h.wait_xpath(RESULT_ROW_LINKS).await;
    let rows = h
        .driver
        .find_all(By::XPath(RESULT_ROW_LINKS))
        .await
        .expect("the result row links");
    assert!(rows.len() > 30, "the run must fill more than one screen");
    let last = rows.last().expect("a result row");
    let href = last
        .attr("href")
        .await
        .expect("the row link's href")
        .expect("a row link carries an href");
    last.scroll_into_view()
        .await
        .expect("scroll to the last row");
    last.click().await.expect("open the last row");
    h.wait_url_contains("row=").await;
    let address = h.current_path().await;
    assert!(
        address.contains("&row=") && !address.contains("&amp;"),
        "the row link must carry its row as a query parameter: {address}"
    );
    let detail = h.wait_xpath(RESULT_DETAIL_HEADING).await;
    let rect = detail.rect().await.expect("the detail's position");
    assert!(
        rect.y < f64::from(VIEWPORT.1),
        "the detail must open inside the first screen, not {}px down the page {}",
        rect.y,
        h.evidence("detail-out-of-sight").await
    );
    h.wait_xpath("//body[contains(., 'The wire')]").await;

    go_back_to_the_row(&h, &href).await;

    h.goto_unhydrated(&href).await;
    h.wait_xpath(RESULT_DETAIL_HEADING).await;
    h.wait_hydrated().await;
    h.wait_css("a#result-catalogue-case")
        .await
        .click()
        .await
        .expect("open the catalogue case");
    h.wait_url_contains("/catalogue/").await;
    h.wait_xpath("//h2[normalize-space()='Spec citation']")
        .await;

    h.assert_console_clean(&[]).await;
    h.finish().await;
}

/// Follows the open detail's "Back to the row" (#867): the address it lands
/// on names the row's anchor as its fragment and no longer selects a row, and
/// the row there is marked as the one opened. `href` is the row link the
/// detail was opened from.
///
/// # Panics
/// When the way back lands anywhere else, or the row is not marked.
async fn go_back_to_the_row(h: &Harness, href: &str) {
    let query = href
        .split_once('?')
        .map(|(_, query)| query.split('#').next().unwrap_or_default())
        .unwrap_or_default();
    let param = |name: &str| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
            .unwrap_or_default()
            .to_owned()
    };
    let anchor = format!("row-{}-{}", param("case"), param("row"));
    h.wait_css("a#result-back")
        .await
        .click()
        .await
        .expect("go back to the row");
    h.wait_url_contains(&format!("#{anchor}")).await;
    let landed = h.driver.current_url().await.expect("current url");
    assert_eq!(
        landed.fragment(),
        Some(anchor.as_str()),
        "the way back must name the row's anchor: {landed}"
    );
    assert!(
        !landed.query().unwrap_or_default().contains("case="),
        "the way back closes the detail: {landed}"
    );
    let edge = h
        .wait_xpath(&format!("//tr[@id='{anchor}']/td[1]"))
        .await
        .css_value("box-shadow")
        .await
        .expect("read the row's edge");
    assert!(
        edge.contains("inset"),
        "the row landed on must be marked as the one opened, not `{edge}` {}",
        h.evidence("row-not-marked").await
    );
}
