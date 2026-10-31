// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! S3 Connect and S4 Scope (#61, #65): the run wizard's first half.
//!
//! Every step is a URL, so refresh and back never lose the user; the
//! wizard's memory is the server-side draft (`run_api`), and the secret
//! values the connect form collects reach only that draft.

use leptos::ev::SubmitEvent;
use leptos::form::FromFormData;
use leptos::prelude::{
    Action, AddAnyAttr, AnyView, ClassAttribute, CollectView, Effect, ElementChild, Get,
    GetUntracked, GlobalAttributes, IntoAny, IntoView, Memo, OnAttribute, OnTargetAttribute,
    PropAttribute, Read, Resource, RwSignal, Set, Show, Signal, StyleAttribute, Suspend, Suspense,
    Transition, Update, With, component, untrack, view,
};
use leptos::server_fn::ServerFn;
use leptos_meta::Title;
use leptos_router::components::{A, Redirect};
use leptos_router::hooks::{use_params_map, use_query_map};
use leptos_use_clipboard::{UseClipboardReturn, use_clipboard};

use crate::components::field::{BTN_PRIMARY, BTN_SECONDARY, INPUT, LABEL, TEXTAREA};
use crate::components::format_view::{Pane, inline_error, refusal_view};
use crate::components::page_header::{Crumb, PageHeader};
use crate::components::surface::{CARD_PAD, CARD_TITLE};
use crate::components::toast;
use crate::error::{Act, ConsoleError};
use crate::identity::{FieldEntry, IdentityAnswer, entered_label, resolve};
use crate::run_api::fns::{
    ProbeAndSave, SaveIdentity, cancel_run, compose_claim, fetch_capability_rows, fetch_connect,
    fetch_draft, fetch_run, fetch_scope_preview, probe_and_save, save_identity, save_profile,
    save_scope, start_run,
};
use crate::run_api::{
    AuthChoice, CapabilityRow, ClaimSummary, ConnectAnswer, ConnectRecord, ConnectRefusal,
    DigestEncoding, PostureForm, ProbeAnswer, REFUSED_KEY, RecordedRun, RunScreen, ScopePreview,
    SigningChoice, SpecProfileChoice, StartOutcome,
};
use crate::run_job::{JobStatus, JobView, RunId};

/// `/run` — the wizard's entry: always the first step.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Run() -> impl IntoView {
    view! { <Redirect path="/run/connect" /> }
}

/// The step indicator: one row, the active step accented — a URL per step.
pub(crate) fn steps(active: &'static str) -> impl IntoView + use<> {
    ["connect", "scope", "live", "results"]
        .into_iter()
        .map(|step| {
            let class = if step == active {
                "rounded-control bg-accent-subtle px-2 py-0.5 text-xs font-medium text-accent-ink"
            } else {
                "rounded-control px-2 py-0.5 text-xs text-ink-faint"
            };
            view! { <span class=class>{step}</span> }
        })
        .collect_view()
}

/// One principal's credential controls: the mode and its secret fields.
///
/// Every write marks the connection edited, because a credential is a fact
/// the probe stored and the run drives with.
#[derive(Debug, Clone, Copy)]
struct CredentialSignals {
    /// The authentication mode.
    auth: RwSignal<AuthChoice>,
    /// The Basic user.
    user: RwSignal<String>,
    /// The Basic password.
    password: RwSignal<String>,
    /// The bearer token.
    token: RwSignal<String>,
    /// The control ids' prefix: `sut` or `admin`.
    prefix: &'static str,
    /// The server-function argument the controls post as: `server` or
    /// `admin`.
    arg: &'static str,
}

impl CredentialSignals {
    /// Empty controls under `prefix`, in mode `none`, posting as `arg`.
    fn new(prefix: &'static str, arg: &'static str) -> Self {
        Self {
            auth: RwSignal::new(AuthChoice::None),
            user: RwSignal::new(String::new()),
            password: RwSignal::new(String::new()),
            token: RwSignal::new(String::new()),
            prefix,
            arg,
        }
    }

    /// The form field name of one member of this principal's argument.
    fn field(self, member: &str) -> String {
        format!("{}[{member}]", self.arg)
    }
}

/// One secret input, posting as `name`, writing `value` and marking the
/// connection edited.
fn secret_input(
    id: String,
    name: String,
    label: &'static str,
    kind: &'static str,
    value: RwSignal<String>,
    edited: RwSignal<bool>,
) -> AnyView {
    view! {
        <div>
            <label class=LABEL for=id.clone()>
                {label}
            </label>
            <input
                id=id
                name=name
                type=kind
                autocomplete="off"
                class=format!("{INPUT} mt-1 w-full")
                prop:value=move || value.get()
                on:input:target=move |ev| {
                    value.set(ev.target().value());
                    edited.set(true);
                }
            />
        </div>
    }
    .into_any()
}

/// An empty hidden field, so the posted form names every member of the
/// argument whatever the mode shows.
fn blank_field(name: String) -> AnyView {
    view! { <input type="hidden" name=name value="" /> }.into_any()
}

/// One principal's mode buttons and the secret fields its mode reads.
///
/// The form posts the mode through a hidden field and every secret member,
/// blank for the ones the mode does not read, so the argument deserializes
/// whichever mode is chosen. The hidden fields come first: a hidden element
/// takes no box, so the stack's spacing is the visible children's alone.
fn credential_controls(
    credential: CredentialSignals,
    label: &'static str,
    edited: RwSignal<bool>,
) -> AnyView {
    let prefix = credential.prefix;
    let button = move |choice: AuthChoice, text: &'static str| {
        let id = format!("{prefix}-auth-{}", choice.token());
        view! {
            <button
                id=id
                type="button"
                class=move || {
                    if credential.auth.get() == choice {
                        "rounded-control bg-accent px-3 py-1.5 text-sm font-medium text-on-accent"
                    } else {
                        "rounded-control border border-edge-strong bg-raised px-3 py-1.5 text-sm text-ink hover:bg-sunken"
                    }
                }
                on:click=move |_| {
                    if credential.auth.get_untracked() != choice {
                        credential.auth.set(choice);
                        edited.set(true);
                    }
                }
            >
                {text}
            </button>
        }
    };
    let hidden = move || {
        let auth = credential.auth.get();
        let blanks = match auth {
            AuthChoice::None => vec!["user", "password", "token"],
            AuthChoice::Basic => vec!["token"],
            AuthChoice::Bearer => vec!["user", "password"],
        }
        .into_iter()
        .map(|member| blank_field(credential.field(member)))
        .collect_view();
        view! {
            <input type="hidden" name=credential.field("auth") value=auth.form_value() />
            {blanks}
        }
        .into_any()
    };
    let fields = move || match credential.auth.get() {
        AuthChoice::None => ().into_any(),
        AuthChoice::Basic => view! {
            <div class="grid grid-cols-2 gap-3">
                {secret_input(
                    format!("{prefix}-user"),
                    credential.field("user"),
                    "User",
                    "text",
                    credential.user,
                    edited,
                )}
                {secret_input(
                    format!("{prefix}-pass"),
                    credential.field("password"),
                    "Password",
                    "password",
                    credential.password,
                    edited,
                )}
            </div>
        }
        .into_any(),
        AuthChoice::Bearer => secret_input(
            format!("{prefix}-token"),
            credential.field("token"),
            "Bearer token",
            "password",
            credential.token,
            edited,
        ),
    };
    view! {
        <div class="space-y-2">
            {hidden} <div>
                <span class=LABEL>{label}</span>
                <div class="mt-1 flex items-center gap-2">
                    {button(AuthChoice::None, "None")} {button(AuthChoice::Basic, "Basic")}
                    {button(AuthChoice::Bearer, "Bearer")}
                </div>
            </div> {fields}
        </div>
    }
    .into_any()
}

/// One identity label: the signal an edit writes, and the value the screen
/// shows and posts.
///
/// The two differ only on a page showing a probe stored by an earlier
/// request, before the client has taken the stored label over: the server
/// pass and a page without the client bundle show and post the stored label.
#[derive(Debug, Clone, Copy)]
struct Label {
    /// What an operator's edit writes.
    edit: RwSignal<String>,
    /// What the screen shows and the forms post.
    shown: Signal<String>,
}

/// One identity field: the server's value read-only when it stated one, the
/// operator's input otherwise, each labelled with its source.
///
/// The field posts as `name` either way: a stated field posts blank, because
/// no operator entered it.
fn identity_cell(
    entry: FieldEntry,
    caption: &'static str,
    (id, name): (&'static str, &'static str),
    label: Label,
) -> AnyView {
    match entry {
        FieldEntry::Reported(stated) => {
            let reported_id = format!("{id}-reported");
            view! {
                <div>
                    <input type="hidden" name=name value="" />
                    <span class=LABEL>{caption}</span>
                    <p id=reported_id class="mt-1 text-sm font-medium text-ink">
                        {stated}
                    </p>
                    <p class="text-xs text-ink-muted">"reported by the server"</p>
                </div>
            }
            .into_any()
        }
        FieldEntry::Entered => view! {
            <div>
                <label class=LABEL for=id>
                    {caption}
                </label>
                <input
                    id=id
                    name=name
                    type="text"
                    class=format!("{INPUT} mt-1 w-full")
                    value=label.shown.get_untracked()
                    prop:value=move || label.shown.get()
                    on:input:target=move |ev| label.edit.set(ev.target().value())
                />
                <p class="text-xs text-ink-muted">"entered by the operator"</p>
            </div>
        }
        .into_any(),
    }
}

/// The identity block: name and version, then whatever else the server
/// stated, or why the operator enters the identity.
fn identity_fields(identity: Option<&IdentityAnswer>, labels: Labels) -> AnyView {
    let name = identity_cell(
        identity.map_or(FieldEntry::Entered, IdentityAnswer::name_entry),
        "Display name",
        ("sut-name", "sut_name"),
        labels.0,
    );
    let version = identity_cell(
        identity.map_or(FieldEntry::Entered, IdentityAnswer::version_entry),
        "Version label",
        ("sut-version", "sut_version"),
        labels.1,
    );
    let mut stated: Vec<(&'static str, &'static str, String)> = Vec::new();
    if let Some(vendor) = identity.and_then(IdentityAnswer::vendor) {
        stated.push(("Vendor", "sut-vendor-reported", vendor.to_owned()));
    }
    if let Some(release) = identity.and_then(IdentityAnswer::restapi_specs_version) {
        stated.push((
            "REST API specification",
            "sut-rest-version-reported",
            release.to_owned(),
        ));
    }
    let extras = stated
        .into_iter()
        .map(|(label, id, value)| {
            view! {
                <div>
                    <span class=LABEL>{label}</span>
                    <p id=id class="mt-1 text-sm font-medium text-ink">
                        {value}
                    </p>
                    <p class="text-xs text-ink-muted">"reported by the server"</p>
                </div>
            }
        })
        .collect_view();
    let reason = identity
        .and_then(IdentityAnswer::fallback_reason)
        .map(|line| view! { <p class="text-sm text-ink-muted">{line}</p> });
    view! {
        <div class="space-y-2">
            <div class="grid grid-cols-2 gap-3">{name}{version}{extras}</div>
            {reason}
        </div>
    }
    .into_any()
}

/// The display name and the version label, as the screen shows them.
type Labels = (Label, Label);

/// What the connect step's answer views share.
#[derive(Clone, Copy)]
struct ConnectControls {
    /// The identity labels the continue control carries.
    labels: Labels,
    /// What withholds Continue.
    gates: ConnectGates,
    /// Continue: stores the labels, then the server redirects to Scope.
    save: Action<SaveIdentity, ()>,
    /// Why the last Continue was refused, when it was.
    save_refused: RwSignal<Option<ConsoleError>>,
}

/// What the connect step offers after a probe the server answered: the
/// continue control over the labels as the screen shows them now, or why
/// there is none yet.
///
/// The identity labels are carried to the run when the operator continues,
/// so they stay editable after the probe. The address and the credentials are
/// facts the probe tested and stored, so an edit to one of them asks for a
/// new probe before the run can use it.
///
/// Continue is a form posting `save_identity`, whose server function
/// redirects to Scope, so it works before the client bundle loads; with the
/// bundle the submit dispatches an action instead, whose continuation reports
/// a refusal (the shape `<ActionForm>` itself takes, in leptos `form.rs`). A
/// label the server stated posts blank.
fn continue_view(identity: &IdentityAnswer, ok: bool, controls: ConnectControls) -> AnyView {
    let ConnectControls {
        labels: (name, version),
        gates,
        save,
        save_refused,
    } = controls;
    if gates.probing.get() {
        return view! { <p class="text-sm text-ink-muted">"Probing the connection…"</p> }
            .into_any();
    }
    if gates.edited.get() {
        return view! {
            <p id="connect-stale" class="text-sm text-ink">
                "The address or a credential changed since the probe. Probe again before continuing, so the run drives what this screen shows."
            </p>
        }
        .into_any();
    }
    let answer = identity.clone();
    let missing = Memo::new(move |_| {
        name.shown.with(|name| {
            version
                .shown
                .with(|version| resolve(Some(&answer), name, version).missing())
        })
    });
    let name_entry = identity.name_entry();
    let version_entry = identity.version_entry();
    let sent_name = move || entered_label(&name_entry, name.shown.get());
    let sent_version = move || entered_label(&version_entry, version.shown.get());
    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        match SaveIdentity::from_event(&ev) {
            Ok(input) => {
                save.dispatch(input);
            }
            Err(e) => report_save_refusal(save_refused, unreadable_form(e)),
        }
    };
    let blocked = move || {
        let body = format!(
            "The server did not state its {}: enter it above.",
            missing.read().join(" or "),
        );
        view! { <p class="text-sm text-ink">{body}</p> }
    };
    let refusal = move || save_refused.get().map(|e| refusal_view(&e));
    view! {
        <Show when=move || missing.with(Vec::is_empty) fallback=blocked>
            <form action=SaveIdentity::url() method="post" on:submit=on_submit>
                <input
                    type="hidden"
                    name="sut_name"
                    value=untrack(&sent_name)
                    prop:value=sent_name.clone()
                />
                <input
                    type="hidden"
                    name="sut_version"
                    value=untrack(&sent_version)
                    prop:value=sent_version.clone()
                />
                <button
                    id="connect-continue"
                    type="submit"
                    class=BTN_PRIMARY
                    prop:disabled=move || save.pending().get()
                >
                    {move || {
                        if save.pending().get() {
                            "Saving…"
                        } else if ok {
                            "Continue"
                        } else {
                            "Continue anyway"
                        }
                    }}
                </button>
            </form>
        </Show>
        {refusal}
    }
    .into_any()
}

/// What withholds Continue on the connect step.
#[derive(Debug, Clone, Copy)]
struct ConnectGates {
    /// A probe is in flight, and the draft it stores is not there yet.
    probing: Memo<bool>,
    /// The address or a credential changed since the last probe.
    edited: RwSignal<bool>,
}

/// The probe's answer: the status line and what comes next.
fn answer_view(result: Result<ConnectAnswer, ConsoleError>, controls: ConnectControls) -> AnyView {
    match result {
        Ok(ConnectAnswer {
            probe: ProbeAnswer::Answered {
                status,
                elapsed_ms,
                ok,
            },
            identity,
        }) => {
            let line = format!("{status} · {elapsed_ms} ms · GET …/definition/template/adl1.4");
            let badge = if ok {
                view! { <p class="text-sm text-ink">"The server answered. Continue when ready."</p> }
                    .into_any()
            } else {
                view! {
                    <p class="text-sm text-ink">
                        "The server answered, though not 2xx — a refusal of the unauthenticated probe is legitimate on locked-down deployments. Check the credentials, or continue anyway."
                    </p>
                }
                .into_any()
            };
            let next = move || continue_view(&identity, ok, controls);
            view! {
                <div class="space-y-2">
                    <Pane label="probe answer" body=line />
                    {badge}
                    {next}
                </div>
            }
            .into_any()
        }
        Ok(ConnectAnswer {
            probe: ProbeAnswer::Unreachable { error },
            ..
        }) => view! {
            <div class="space-y-2">
                {inline_error(&error)}
                <p class="text-sm text-ink-muted">
                    "Nothing answered: check the URL, the network path from this console to the server, and that the CDR is up."
                </p>
            </div>
        }
        .into_any(),
        Err(e) => {
            // The same sentence the page states for a refusal posted without
            // the client bundle, chosen by the refusal's variant.
            let sentence = ConnectRefusal::from(&e).sentence();
            view! {
                <div class="space-y-2">
                    {inline_error(&e.to_string())} <p class="text-sm text-ink-muted">{sentence}</p>
                </div>
            }
            .into_any()
        }
    }
}

/// What the URL says about the connect step: whether a probe was stored for
/// this page to show, and the refusal a form posted without the client bundle
/// came back with.
///
/// A helper rather than the component body, for the reason [`requested_run`]
/// gives.
fn connect_from_url() -> (Memo<bool>, Memo<Option<String>>) {
    let query = use_query_map();
    let probed = Memo::new(move |_| query.with(|map| map.get("probed").is_some()));
    let refused = Memo::new(move |_| query.with(|map| map.get(REFUSED_KEY)));
    (probed, refused)
}

/// The sentence a refusal named by the URL reads as.
fn refusal_sentence(token: &str) -> &'static str {
    ConnectRefusal::from_token(token).map_or(
        "The connection was refused for a reason this address does not name. Probe the connection again.",
        ConnectRefusal::sentence,
    )
}

/// A form the browser could not turn into the server function's arguments,
/// as the error the action reports (the way `<ActionForm>` reports it, in
/// leptos `form.rs`).
fn unreadable_form(error: impl std::fmt::Display) -> ConsoleError {
    ConsoleError::Transport(leptos::prelude::ServerFnErrorErr::Serialization(
        error.to_string(),
    ))
}

/// Reports a refused probe: the notification and the inline answer, for a
/// refusal from the server and for a form that would not parse alike.
fn report_probe_refusal(
    answer: RwSignal<Option<Result<ConnectAnswer, ConsoleError>>>,
    error: ConsoleError,
) {
    toast::refused(Act::Probe, &error);
    answer.set(Some(Err(error)));
}

/// Reports a refused Continue: the notification and the inline line, for a
/// refusal from the server and for a form that would not parse alike.
fn report_save_refusal(save_refused: RwSignal<Option<ConsoleError>>, error: ConsoleError) {
    toast::refused(Act::SaveIdentity, &error);
    save_refused.set(Some(error));
}

/// S3 — the connection form and the probe.
///
/// Probe and Continue are plain forms posting their server functions, so the
/// step works before the client bundle loads. A probe posted without it is
/// redirected back here with `?probed`, and the page reads the stored answer;
/// a refused form comes back with `?refused`, and the page states the
/// refusal and offers no Continue; Continue's server function redirects to
/// Scope. With the bundle each submit dispatches an action whose continuation
/// writes the answer and reports a refusal as a notification.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[expect(
    clippy::too_many_lines,
    reason = "the connect form's fields, both credentials, the probe and the continue gate — one cohesive screen, its sections erased per rules §1"
)]
#[component]
pub fn Connect() -> impl IntoView {
    let base_url = RwSignal::new(String::new());
    let sut_name = RwSignal::new(String::from("my-cdr"));
    let sut_version = RwSignal::new(String::from("unknown"));
    let server = CredentialSignals::new("sut", "server");
    let admin = CredentialSignals::new("admin", "admin");
    // Whether the address or a credential changed since the last probe: those
    // are the facts the probe stored, so the run cannot drive the edit until
    // a new probe stores it.
    let edited = RwSignal::new(false);
    let answer = RwSignal::new(None::<Result<ConnectAnswer, ConsoleError>>);
    let probe = Action::new(move |input: &ProbeAndSave| {
        let input = input.clone();
        async move {
            match probe_and_save(
                input.base_url,
                input.sut_name,
                input.sut_version,
                input.server,
                input.admin,
            )
            .await
            {
                Ok(connected) => answer.set(Some(Ok(connected))),
                Err(e) => report_probe_refusal(answer, e),
            }
        }
    });
    // A success needs no answer here: the server function's redirect moves
    // the router to Scope.
    let save_refused = RwSignal::new(None::<ConsoleError>);
    let save = Action::new(move |input: &SaveIdentity| {
        let input = input.clone();
        async move {
            if let Err(e) = save_identity(input.sut_name, input.sut_version).await {
                report_save_refusal(save_refused, e);
            }
        }
    });
    let (probed, refused) = connect_from_url();
    let stored = Resource::new(
        move || probed.get() && refused.with(Option::is_none),
        |probed| async move {
            if probed {
                fetch_connect().await
            } else {
                Ok(None)
            }
        },
    );
    // Seeds the editable labels from a probe an earlier request stored, once
    // it has loaded: they render in the form and in the stored answer's
    // Continue, outside any one Suspend, which is the sanctioned one-shot
    // resource-seeding effect (`reactivity/working_with_signals`). Until it
    // runs, the shown labels read the stored record itself.
    let seeded = RwSignal::new(false);
    Effect::new(move |_| {
        if seeded.get_untracked() {
            return;
        }
        if let Some(loaded) = stored.get() {
            if let Ok(Some(record)) = loaded {
                sut_name.set(record.sut_name);
                sut_version.set(record.sut_version);
            }
            seeded.set(true);
        }
    });
    let stored_label = move |pick: fn(&ConnectRecord) -> &String| {
        stored.with(|loaded| {
            loaded
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .and_then(Option::as_ref)
                .map(|record| pick(record).clone())
        })
    };
    let shown = move |edit: RwSignal<String>, pick: fn(&ConnectRecord) -> &String| {
        Signal::derive(move || {
            if seeded.get() {
                edit.get()
            } else {
                stored_label(pick).unwrap_or_else(|| edit.get())
            }
        })
    };
    let labels: Labels = (
        Label {
            edit: sut_name,
            shown: shown(sut_name, |record| &record.sut_name),
        },
        Label {
            edit: sut_version,
            shown: shown(sut_version, |record| &record.sut_version),
        },
    );
    let controls = ConnectControls {
        labels,
        gates: ConnectGates {
            probing: probe.pending(),
            edited,
        },
        save,
        save_refused,
    };

    // The identity block reads the stored probe too, so it renders under its
    // own Suspense and the server pass and hydration agree on it.
    let identity = view! {
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                let live = answer
                    .with(|answered| {
                        answered
                            .as_ref()
                            .and_then(|result| result.as_ref().ok())
                            .map(|done| done.identity.clone())
                    });
                let loaded = stored.await;
                let identity = live
                    .or_else(|| { loaded.ok().flatten().map(|record| record.answer.identity) });
                identity_fields(identity.as_ref(), labels)
            })}
        </Suspense>
    }
    .into_any();
    let credentials = view! {
        {credential_controls(server, "Authentication", edited)}
        <div class="space-y-2 border-t border-line pt-4">
            {credential_controls(admin, "Administrative credential (optional)", edited)}
            <p class="text-xs text-ink-muted">
                "The principal the administrative workflows log in as, separate from the server credential. Without one, every workflow with an administrative step is skipped as not-declared."
            </p>
        </div>
    }
    .into_any();
    let answered = move || answer.get().map(|result| answer_view(result, controls));
    let refusal = move || {
        refused
            .get()
            .filter(|_| answer.with(Option::is_none))
            .map(|token| {
                view! { <div id="connect-refused">{inline_error(refusal_sentence(&token))}</div> }
            })
    };
    // A probe stored by an earlier request, shown until this page probes
    // again. Read under its own Suspense so the server pass and hydration
    // agree on it.
    let recalled = view! {
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                let superseded = answer.with(Option::is_some);
                let record = stored.await;
                match record {
                    Ok(Some(record)) if !superseded => answer_view(Ok(record.answer), controls),
                    Ok(_) => ().into_any(),
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
    }
    .into_any();
    let on_probe = move |ev: SubmitEvent| {
        ev.prevent_default();
        edited.set(false);
        match ProbeAndSave::from_event(&ev) {
            Ok(input) => {
                probe.dispatch(input);
            }
            Err(e) => report_probe_refusal(answer, unreadable_form(e)),
        }
    };
    let form = view! {
        <form action=ProbeAndSave::url() method="post" class="space-y-4" on:submit=on_probe>
            <div>
                <label class=LABEL for="base-url">
                    "CDR base URL"
                </label>
                <input
                    id="base-url"
                    name="base_url"
                    type="url"
                    class=format!("{INPUT} mt-1 w-full")
                    placeholder="https://cdr.example/ehrbase/rest/openehr/v1"
                    prop:value=move || base_url.get()
                    on:input:target=move |ev| {
                        base_url.set(ev.target().value());
                        edited.set(true);
                    }
                />
            </div>
            {identity}
            {credentials}
            <div class="flex items-center gap-2">
                <button
                    type="submit"
                    class=BTN_SECONDARY
                    prop:disabled=move || probe.pending().get()
                >
                    {move || {
                        if probe.pending().get() { "Probing…" } else { "Probe connection" }
                    }}
                </button>
            </div>
        </form>
    }
    .into_any();

    view! {
        <Title text="Connect · Run · Veredictum console" />
        <PageHeader
            title="Grade a server"
            subtitle="Point the instrument at a reachable CDR. Credential values stay in memory; only the spawned run's environment ever sees them."
            crumbs=vec![Crumb::new("Run", "/run/connect")]
        >
            <div class="flex items-center gap-1">{steps("connect")}</div>
        </PageHeader>
        <section class=format!("{CARD_PAD} max-w-2xl")>
            <div class="space-y-4">{form}{refusal}{answered}{recalled}</div>
        </section>
    }
}

/// S4 — the scope: the tier row that builds a claim, the pasted claim, the
/// filter, and the honest preview.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[expect(
    clippy::too_many_lines,
    reason = "the capability row, the statement pick, the filter, the preview and the save answer — one cohesive screen, its sections erased per rules §1"
)]
#[component]
pub fn Scope() -> impl IntoView {
    let draft = Resource::new(|| (), |()| fetch_draft());
    let statement_json = RwSignal::new(String::new());
    let capability_rows = Resource::new(|| (), |()| fetch_capability_rows());
    let checked = RwSignal::new(Vec::<String>::new());
    let composed_note = RwSignal::new(None::<Result<String, ConsoleError>>);
    let compose = Action::new(move |selection: &Vec<String>| {
        let selection = selection.clone();
        async move {
            match compose_claim(Some(selection)).await {
                Ok(document) => {
                    statement_json.set(document);
                    composed_note.set(Some(Ok(String::from(
                        "Composed from the checked capabilities. Read it, edit it if you need to, then save the scope.",
                    ))));
                }
                Err(e) => composed_note.set(Some(Err(e))),
            }
        }
    });
    let filter = RwSignal::new(String::new());
    let postures = PostureSignals::new();
    let profile = RwSignal::new(DEVELOPMENT_PROFILE);
    let set_profile = Action::new(move |token: &&'static str| {
        let token = *token;
        async move {
            match save_profile(token.to_owned()).await {
                Ok(()) => toast::success(
                    "Specification saved",
                    &format!("The run is judged against the {token} text."),
                ),
                Err(e) => toast::refused(Act::SaveProfile, &e),
            }
        }
    });
    // The preview is a pure read, so it answers inline only, in its own
    // pane. Every mutation below reports BOTH outcomes as a toast and keeps
    // its inline pane. The answers land in each action's own async
    // continuation, so no signal-writing Effect mediates
    // (<https://book.leptos.dev/reactivity/working_with_signals.html>).
    let previewed = RwSignal::new(None::<Result<ScopePreview, ConsoleError>>);
    let preview = Action::new(move |filter: &String| {
        let filter = filter.clone();
        async move {
            previewed.set(Some(fetch_scope_preview(filter).await));
        }
    });
    let saved = RwSignal::new(None::<Result<Option<ClaimSummary>, ConsoleError>>);
    let save = Action::new(move |input: &ScopeInput| {
        let input = input.clone();
        async move {
            match save_scope(
                Some(input.statement_json),
                Some(input.filter),
                input.postures,
            )
            .await
            {
                Ok(claim) => {
                    toast::success("Scope saved", &saved_body(claim.as_ref()));
                    saved.set(Some(Ok(claim)));
                }
                Err(e) => {
                    toast::refused(Act::SaveScope, &e);
                    saved.set(Some(Err(e)));
                }
            }
        }
    });
    let started = RwSignal::new(None::<Result<StartOutcome, ConsoleError>>);
    let start = Action::new(move |(): &()| async move {
        match start_run().await {
            Ok(StartOutcome::Accepted(id)) => {
                toast::success(
                    "Run accepted",
                    &format!(
                        "Run {id} has its own address; the Live screen says whether it is driving or queued."
                    ),
                );
                started.set(Some(Ok(StartOutcome::Accepted(id))));
            }
            Ok(StartOutcome::AlreadyInFlight(id)) => {
                toast::error(
                    "You already have a run in flight",
                    &format!(
                        "Nothing was started: run {id} is still going. Watch it, or cancel it and start again."
                    ),
                );
                started.set(Some(Ok(StartOutcome::AlreadyInFlight(id))));
            }
            Err(e) => {
                toast::refused(Act::Start, &e);
                started.set(Some(Err(e)));
            }
        }
    });

    view! {
        <Title text="Scope · Run · Veredictum console" />
        <PageHeader
            title="Scope"
            subtitle="Build the claim from tiers or paste the vendor's own, then preview what will process before anything starts."
            crumbs=vec![Crumb::new("Run", "/run/connect")]
        >
            <div class="flex items-center gap-1">{steps("scope")}</div>
        </PageHeader>
        <Suspense fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the draft…"</p> }
        }>
            {move || Suspend::new(async move {
                match draft.await {
                    Ok(Some(view_draft)) => {
                        let connection = format!(
                            "{} · {} {} · auth {} · admin auth {}{}",
                            view_draft.base_url,
                            view_draft.sut_name,
                            view_draft.sut_version,
                            view_draft.auth,
                            view_draft.admin_auth,
                            if view_draft.probed_ok {
                                " · probed ✓"
                            } else {
                                " · probe not 2xx"
                            },
                        );
                        view! { <Pane label="connection" body=connection /> }.into_any()
                    }
                    Ok(None) => {
                        view! {
                            <div class="space-y-2">
                                <p class="text-sm text-ink">
                                    "No connection draft: the wizard starts at Connect."
                                </p>
                                <A href="/run/connect" attr:class=BTN_SECONDARY>
                                    "Back to Connect"
                                </A>
                            </div>
                        }
                            .into_any()
                    }
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Suspense>
        <section class=format!("{CARD_PAD} mt-4 max-w-2xl")>
            <h2 class=CARD_TITLE>"Selection"</h2>
            <div class="space-y-4">
                {capability_picker(capability_rows, checked, compose, composed_note)}
                {statement_picker(statement_json)} <div>
                    <label class=LABEL for="filter">
                        "Document-id filter (optional)"
                    </label>
                    <input
                        id="filter"
                        type="text"
                        class=format!("{INPUT} mt-1 w-full font-mono")
                        placeholder="ehr_status"
                        prop:value=move || filter.get()
                        on:input:target=move |ev| filter.set(ev.target().value())
                    />
                </div> {profile_picker(profile)} {posture_picker(postures)}
                <div class="flex items-center gap-2">
                    <button
                        type="button"
                        class=BTN_SECONDARY
                        on:click=move |_| {
                            preview.dispatch(filter.get());
                        }
                    >
                        "Preview selection"
                    </button>
                    <button
                        type="button"
                        class=BTN_PRIMARY
                        on:click=move |_| {
                            save.dispatch(ScopeInput {
                                statement_json: statement_json.get(),
                                filter: filter.get(),
                                postures: postures.read(),
                            });
                            set_profile.dispatch(profile.get());
                            preview.dispatch(filter.get());
                        }
                    >
                        "Save scope"
                    </button>
                </div>
                {move || {
                    previewed
                        .get()
                        .map(|result| match result {
                            Ok(scope) => {
                                let chapters = scope
                                    .chapters
                                    .iter()
                                    .map(|(chapter, cases)| format!("{chapter} {cases}"))
                                    .collect::<Vec<_>>()
                                    .join(" · ");
                                view! {
                                    <div class="space-y-1">
                                        <p class="text-sm font-medium text-ink">
                                            {format!("{} cases in scope", scope.total)}
                                        </p>
                                        <p class="text-sm text-ink-muted">{chapters}</p>
                                        <p class="text-sm text-ink-muted">
                                            "Every case in scope lands as an outcome or a recorded exception; the statement excuses out-of-claim cases at drive time."
                                        </p>
                                    </div>
                                }
                                    .into_any()
                            }
                            Err(e) => inline_error(&e.to_string()).into_any(),
                        })
                }}
                {move || {
                    saved
                        .get()
                        .map(|result| match result {
                            Ok(claim) => {
                                let line = saved_body(claim.as_ref());
                                view! {
                                    <div class="space-y-2">
                                        <p class="text-sm text-ink">{line}</p>
                                        <button
                                            type="button"
                                            class=BTN_PRIMARY
                                            on:click=move |_| {
                                                start.dispatch(());
                                            }
                                        >
                                            "Start the run"
                                        </button>
                                    </div>
                                }
                                    .into_any()
                            }
                            Err(e) => refusal_view(&e).into_any(),
                        })
                }}
                {move || {
                    started
                        .get()
                        .map(|result| match result {
                            Ok(StartOutcome::Accepted(id)) => {
                                view! {
                                    <div class="flex items-center gap-2">
                                        <p class="text-sm text-ink">
                                            {format!("Run accepted ({id}).")}
                                        </p>
                                        <A href=live_path(id) attr:class=BTN_SECONDARY>
                                            "Watch it live"
                                        </A>
                                    </div>
                                }
                                    .into_any()
                            }
                            Ok(StartOutcome::AlreadyInFlight(id)) => {
                                view! {
                                    <div class="flex items-center gap-2">
                                        <p class="text-sm text-ink">
                                            {format!(
                                                "Nothing was started: run {id} is already in flight from this address.",
                                            )}
                                        </p>
                                        <A href=live_path(id) attr:class=BTN_SECONDARY>
                                            "Go to that run"
                                        </A>
                                    </div>
                                }
                                    .into_any()
                            }
                            Err(e) => refusal_view(&e).into_any(),
                        })
                }}
            </div>
        </section>
    }
}

/// The capability picker: each folder's counts, the checkboxes that build the
/// claim, and the compose button with its note.
fn capability_picker(
    capability_rows: Resource<Result<Vec<CapabilityRow>, ConsoleError>>,
    checked: RwSignal<Vec<String>>,
    compose: Action<Vec<String>, ()>,
    composed_note: RwSignal<Option<Result<String, ConsoleError>>>,
) -> impl IntoView + use<> {
    view! {
        <div>
            <span class=LABEL>"Build the claim from capabilities"</span>
            <p class="mt-1 text-sm text-ink-muted">
                "Checking a capability claims its folder of the catalogue; the counts say how many documents and cases it holds. Composing writes the claim into the box below, where you read it before anything runs. Options stay undeclared, because only the party running the server knows which arm it realizes."
            </p>
            <Suspense fallback=|| {
                view! { <p class="text-sm text-ink-muted">"Counting the capabilities…"</p> }
            }>
                {move || Suspend::new(async move {
                    match capability_rows.await {
                        Ok(rows) => {
                            let boxes = rows
                                .into_iter()
                                .map(|row| {
                                    let control = format!("capability-{}", row.name);
                                    let labelled = control.clone();
                                    let label = format!(
                                        "{} · {} documents · {} cases",
                                        row.name,
                                        row.documents,
                                        row.cases,
                                    );
                                    let held = row.name.clone();
                                    let toggled = row.name;
                                    view! {
                                        <label
                                            class="flex items-center gap-2 text-sm text-ink"
                                            for=labelled
                                        >
                                            <input
                                                id=control
                                                type="checkbox"
                                                class="size-4 accent-accent"
                                                prop:checked=move || {
                                                    checked.with(|names| names.contains(&held))
                                                }
                                                on:change:target=move |ev| {
                                                    let on = ev.target().checked();
                                                    checked
                                                        .update(|names| {
                                                            names.retain(|name| *name != toggled);
                                                            if on {
                                                                names.push(toggled.clone());
                                                            }
                                                        });
                                                }
                                            />
                                            {label}
                                        </label>
                                    }
                                })
                                .collect_view();
                            view! { <div class="mt-2 grid gap-2 sm:grid-cols-2">{boxes}</div> }
                                .into_any()
                        }
                        Err(e) => inline_error(&e.to_string()).into_any(),
                    }
                })}
            </Suspense>
            <div class="mt-3 flex items-center gap-2">
                <button
                    type="button"
                    class=BTN_SECONDARY
                    on:click=move |_| {
                        compose.dispatch(checked.get());
                    }
                >
                    "Compose the claim"
                </button>
            </div>
            {move || {
                composed_note
                    .get()
                    .map(|note| match note {
                        Ok(line) => {
                            view! { <p class="mt-2 text-sm text-ink-muted">{line}</p> }.into_any()
                        }
                        Err(e) => refusal_view(&e).into_any(),
                    })
            }}
        </div>
    }
}

/// The statement pane: the box a submitter's own declaration is pasted into.
///
/// ISO/IEC 9646-7 assigns an ICS proforma's support and supported-values
/// columns to the supplier of the implementation, so the console collects the
/// declaration and offers none of its own.
fn statement_picker(statement_json: RwSignal<String>) -> impl IntoView + use<> {
    view! {
        <div>
            <label class=LABEL for="statement-json">
                "Party statement (ICS) — the claim this run grades"
            </label>
            <p class="mt-1 text-sm text-ink-muted">
                "Paste your own statement.json, or compose one from the capabilities above and edit it. The claim is the supplier's to make, so this instrument holds none about any product. Leave the box empty for an honest no-claim run: everything applicable drives, nothing is certified."
            </p>
            <textarea
                id="statement-json"
                class=format!("{TEXTAREA} mt-2 h-48")
                placeholder="{ \"product\": { \"name\": …, \"version\": … }, \"claims\": { \"capabilities\": [\"ehr\"] }, … }"
                prop:value=move || statement_json.get()
                on:input:target=move |ev| statement_json.set(ev.target().value())
            ></textarea>
        </div>
    }
}

/// The deployment postures the form collects, held as one bundle so the
/// picker takes one argument instead of seven.
#[derive(Debug, Clone, Copy)]
struct PostureSignals {
    /// The deployment's configured system identifier.
    system_id: RwSignal<String>,
    /// A location on the SUT's own file system for the dump/load operations.
    dump_location: RwSignal<String>,
    /// The signing mode.
    signing: RwSignal<SigningChoice>,
    /// The digest encoding, read only for the digest mode.
    digest_encoding: RwSignal<DigestEncoding>,
    /// The prefix a digest-mode signature carries.
    digest_prefix: RwSignal<String>,
    /// The armored openPGP public key.
    pgp_public_key: RwSignal<String>,
    /// The openEHR generation set.
    spec_profile: RwSignal<SpecProfileChoice>,
}

impl PostureSignals {
    /// Fresh signals, every posture undeclared.
    fn new() -> Self {
        Self {
            system_id: RwSignal::new(String::new()),
            dump_location: RwSignal::new(String::new()),
            signing: RwSignal::new(SigningChoice::Undeclared),
            digest_encoding: RwSignal::new(DigestEncoding::Base64),
            digest_prefix: RwSignal::new(String::new()),
            pgp_public_key: RwSignal::new(String::new()),
            spec_profile: RwSignal::new(SpecProfileChoice::Undeclared),
        }
    }

    /// What the form says right now, for one Save-scope dispatch.
    fn read(self) -> PostureForm {
        PostureForm {
            system_id: self.system_id.get(),
            dump_location: self.dump_location.get(),
            signing: self.signing.get(),
            digest_encoding: self.digest_encoding.get(),
            digest_prefix: self.digest_prefix.get(),
            pgp_public_key: self.pgp_public_key.get(),
            spec_profile: self.spec_profile.get(),
        }
    }
}

/// One button of a segmented control: the choice it selects, and whether it
/// is the selected one — the Connect step's auth control is the precedent, and
/// it keeps the closed vocabulary in the type system rather than in a parsed
/// string.
fn choice_button<A, S>(
    id: &'static str,
    label: &'static str,
    active: A,
    select: S,
) -> impl IntoView + use<A, S>
where
    A: Fn() -> bool + Send + Sync + 'static,
    S: Fn() + 'static,
{
    view! {
        <button
            id=id
            type="button"
            class=move || {
                if active() {
                    "rounded-control bg-accent px-3 py-1.5 text-sm font-medium text-on-accent"
                } else {
                    "rounded-control border border-line px-3 py-1.5 text-sm text-ink"
                }
            }
            on:click=move |_| select()
        >
            {label}
        </button>
    }
}

/// The two scalar postures: the configured system identifier, and the dump
/// location on the server's own file system.
fn posture_scalars(postures: PostureSignals) -> impl IntoView + use<> {
    view! {
        <div class="space-y-4">
            <div class="space-y-1">
                <h3 class="text-sm font-semibold text-ink-heading">"Deployment postures"</h3>
                <p class="text-sm text-ink-muted">
                    "Facts about your deployment that no openEHR operation discloses, so the instrument can only read them from a declaration. Leave one undeclared and the cases that need it come back not-applicable with the citation, never judged against a guess."
                </p>
            </div>
            <div>
                <label class=LABEL for="posture-system-id">
                    "System id (optional)"
                </label>
                <input
                    id="posture-system-id"
                    type="text"
                    class=format!("{INPUT} mt-1 w-full font-mono")
                    placeholder="cdr.example.org"
                    prop:value=move || postures.system_id.get()
                    on:input:target=move |ev| postures.system_id.set(ev.target().value())
                />
                <p class="mt-1 text-sm text-ink-muted">
                    "The identifier the server stamps into the version uids it mints."
                </p>
            </div>
            <div>
                <label class=LABEL for="posture-dump-location">
                    "Dump location (optional)"
                </label>
                <input
                    id="posture-dump-location"
                    type="text"
                    class=format!("{INPUT} mt-1 w-full font-mono")
                    placeholder="/var/lib/openehr/dump"
                    prop:value=move || postures.dump_location.get()
                    on:input:target=move |ev| postures.dump_location.set(ev.target().value())
                />
                <p class="mt-1 text-sm text-ink-muted">
                    "A directory on the SERVER's own file system the admin dump and load operations may write to. The console never opens it: it travels to the server in those requests."
                </p>
            </div>
        </div>
    }
}

/// The digest posture's own parameters: the encoding, and the fixed prefix the
/// wire form carries.
fn posture_digest_fields(postures: PostureSignals) -> impl IntoView + use<> {
    let encoding = postures.digest_encoding;
    let encoding_buttons = move || {
        DigestEncoding::ALL
            .into_iter()
            .map(|choice| {
                choice_button(
                    choice.control_id(),
                    choice.token(),
                    move || encoding.get() == choice,
                    move || encoding.set(choice),
                )
            })
            .collect_view()
    };
    view! {
        <div class="space-y-2 border-l border-line pl-3">
            <p class="text-sm text-ink-muted">
                "Digest: sha256, the algorithm this instrument's verifier implements. Another hash is not declarable here, and its rows stay not-applicable."
            </p>
            <div class="flex flex-wrap items-center gap-2">{encoding_buttons}</div>
            <div>
                <label class=LABEL for="posture-digest-prefix">
                    "Signature prefix (optional)"
                </label>
                <input
                    id="posture-digest-prefix"
                    type="text"
                    class=format!("{INPUT} mt-1 w-full font-mono")
                    placeholder="sha256:"
                    prop:value=move || postures.digest_prefix.get()
                    on:input:target=move |ev| postures.digest_prefix.set(ev.target().value())
                />
            </div>
        </div>
    }
}

/// The openPGP posture's own parameter: the armored public half.
fn posture_pgp_field(postures: PostureSignals) -> impl IntoView + use<> {
    view! {
        <div class="space-y-2 border-l border-line pl-3">
            <label class=LABEL for="posture-pgp-key">
                "openPGP public key"
            </label>
            <textarea
                id="posture-pgp-key"
                class=format!("{TEXTAREA} mt-1 h-40")
                placeholder="-----BEGIN PGP PUBLIC KEY BLOCK-----"
                prop:value=move || postures.pgp_public_key.get()
                on:input:target=move |ev| postures.pgp_public_key.set(ev.target().value())
            ></textarea>
            <p class="text-sm text-ink-muted">
                "The armored public half the detached signatures verify against. Public key material only, never a private key."
            </p>
        </div>
    }
}

/// The version-signing posture: the mode, and whichever parameters that mode
/// carries.
fn posture_signing(postures: PostureSignals) -> impl IntoView + use<> {
    let signing = postures.signing;
    let signing_buttons = move || {
        SigningChoice::ALL
            .into_iter()
            .map(|choice| {
                choice_button(
                    choice.control_id(),
                    choice.label(),
                    move || signing.get() == choice,
                    move || signing.set(choice),
                )
            })
            .collect_view()
    };
    let digest_fields = move || posture_digest_fields(postures).into_any();
    let pgp_field = move || posture_pgp_field(postures).into_any();
    view! {
        <div class="space-y-2">
            <span class=LABEL>"Version signing"</span>
            <div class="flex flex-wrap items-center gap-2">{signing_buttons}</div>
            <p class="text-sm text-ink-muted">
                "Signing needs public-key or equivalent infrastructure in place, so it is a property of your deployment rather than of openEHR. The stored-signature cases run either way; only the verifying ones need the mode."
            </p>
            <Show when=move || signing.get() == SigningChoice::Digest>{digest_fields}</Show>
            <Show when=move || signing.get() == SigningChoice::Pgp>{pgp_field}</Show>
        </div>
    }
}

/// The openEHR generation set the deployment runs.
fn posture_profile(postures: PostureSignals) -> impl IntoView + use<> {
    let profile = postures.spec_profile;
    let profile_buttons = move || {
        SpecProfileChoice::ALL
            .into_iter()
            .map(|choice| {
                choice_button(
                    choice.control_id(),
                    choice.label(),
                    move || profile.get() == choice,
                    move || profile.set(choice),
                )
            })
            .collect_view()
    };
    view! {
        <div class="space-y-2">
            <span class=LABEL>"openEHR generation set"</span>
            <div class="flex flex-wrap items-center gap-2">{profile_buttons}</div>
            <p class="text-sm text-ink-muted">
                "Which generation set the server implements. A minor openEHR release is a compatible superset, so the two accept different surface while looking identical on the wire."
            </p>
        </div>
    }
}

/// What this console does NOT supply, and what each omission costs a run.
///
/// Stated rather than guessed: a fact the wizard cannot collect honestly is
/// named here, so a reader knows why those rows come back not-applicable.
fn posture_gaps() -> impl IntoView + use<> {
    let body = String::from(
        "SMART App Launch: minting a scope-carrying token needs a signing key on this host, and a public endpoint that names a file is a file-read primitive. The SMART cases stay not-applicable.\n\
         Terminology servers: which servers answer for which namespaces, and what the server does with a value set it cannot resolve, is a topology this form does not collect.\n\
         Exclusive server: the flag travels in one block with the hardware facts a perf-result records as the environment it measured, and this console collects no hardware facts.\n\
         A second principal or a second deployment (a read-only role, a second signing posture): each needs its own credentials or its own guarded target, and this wizard composes one.",
    );
    view! { <Pane label="postures this console does not supply" body=body /> }
}

/// The deployment-posture section: the facts the console CAN collect, and a
/// plain statement of the ones it cannot.
///
/// Every fact here is a deployment declaration no released openEHR operation
/// discloses, which is why the instrument reads it from the ixit. Undeclared
/// is a real answer: the run records the cases that need it not-applicable
/// with the engine's own citation, and no value here is ever guessed.
fn posture_picker(postures: PostureSignals) -> impl IntoView + use<> {
    let scalars = posture_scalars(postures).into_any();
    let signing = posture_signing(postures).into_any();
    let profile = posture_profile(postures).into_any();
    let gaps = posture_gaps().into_any();
    view! { <div class="space-y-4">{scalars}{signing}{profile}{gaps}</div> }.into_any()
}

/// The token of the development specification profile, the default.
const DEVELOPMENT_PROFILE: &str = "development";

/// The token of the frozen release's specification profile.
const RELEASED_PROFILE: &str = "released";

/// The specification text the run is judged against: the development
/// snapshot by default, or the frozen release, which alone backs a
/// certificate.
fn profile_picker(profile: RwSignal<&'static str>) -> AnyView {
    let buttons = [
        (
            "profile-development",
            DEVELOPMENT_PROFILE,
            "development snapshot",
        ),
        ("profile-released", RELEASED_PROFILE, "release"),
    ]
    .into_iter()
    .map(|(id, token, label)| {
        choice_button(
            id,
            label,
            move || profile.get() == token,
            move || profile.set(token),
        )
    })
    .collect_view();
    view! {
        <div class="space-y-2">
            <h3 class="text-sm font-semibold text-ink-heading">"Specification text"</h3>
            <p class="text-sm text-ink-muted">
                "The development snapshot is the latest openEHR text. Only a run against the release backs a released claim or a certificate."
            </p>
            <div class="flex flex-wrap items-center gap-2">{buttons}</div>
        </div>
    }
    .into_any()
}

/// What one Save-scope dispatch carries: the pasted claim, the filter, and
/// the wire-recording choice, read off the form at click time.
#[derive(Debug, Clone)]
struct ScopeInput {
    /// The pasted statement document, empty for a no-claim run.
    statement_json: String,
    /// The document-id filter, empty for the whole catalogue.
    filter: String,
    /// The deployment postures the operator declared.
    postures: PostureForm,
}

/// The one sentence a saved scope is reported by, in the toast and inline —
/// one reader for one claim.
fn saved_body(claim: Option<&ClaimSummary>) -> String {
    claim.map_or_else(
        || {
            String::from(
                "Scope saved without a claim: everything applicable drives, nothing is certified.",
            )
        },
        |summary| {
            format!(
                "Claim accepted: {} — {} capabilities.",
                summary.product, summary.capabilities,
            )
        },
    )
}

/// Renders milliseconds as `mm:ss` — display truncation is the intent of
/// each division here.
#[expect(
    clippy::integer_division,
    reason = "clock display truncates by definition; the remainder is shown in the seconds field"
)]
fn mmss(ms: u64) -> String {
    let seconds = ms / 1000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

/// What `/run/live/{run_id}` names, read out of the URL.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Requested {
    /// The bare `/run/live`: ask this process which run it holds.
    Nothing,
    /// A readable run id.
    Named(RunId),
    /// A path segment that is not a run id; the field is the parse reason.
    Unreadable(String),
}

/// What the live screen renders, once the URL and the server have both had
/// their say.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum LiveScreen {
    /// The URL named something this build cannot read as a run id.
    Unreadable(String),
    /// The server's own answer for the named run, or for no name at all.
    Answered(RunScreen),
}

/// The run the URL names, re-read on every navigation.
///
/// A helper rather than the component body: reading the router's params
/// inside a `#[component]` fn silences `clippy::must_use_candidate` there,
/// turning the crate's `#[expect]` idiom into an unfulfilled-expectation
/// build failure.
fn requested_run() -> Memo<Requested> {
    let params = use_params_map();
    Memo::new(move |_| {
        params.with(|map| match map.get("run_id") {
            None => Requested::Nothing,
            Some(text) if text.is_empty() => Requested::Nothing,
            Some(text) => match text.parse::<RunId>() {
                Ok(id) => Requested::Named(id),
                Err(e) => Requested::Unreadable(e.to_string()),
            },
        })
    })
}

/// S5 — the live run: progress from the engine's own stream, the honest
/// estimate, the output tail, and cancel. The page polls and therefore
/// rejoins the running job on refresh.
///
/// The URL carries the run's identity (#386), so a reload, a bookmark and a
/// shared link all follow the same run. A bare `/run/live` asks about the run
/// this process holds and renders it exactly as the addressed URL does.
#[expect(
    clippy::must_use_candidate,
    reason = "a Leptos component is mounted by the framework, never consumed as a value"
)]
#[component]
pub fn Live() -> impl IntoView {
    let requested = requested_run();
    // A browser-side tick drives the poll; on the server the resource loads
    // once and the timer never runs (pause/resume are browser-only).
    let tick = RwSignal::new(0_u64);
    let screen = Resource::new(
        move || (tick.get(), requested.get()),
        |(_, requested)| async move {
            match requested {
                Requested::Unreadable(reason) => Ok(LiveScreen::Unreadable(reason)),
                Requested::Nothing => fetch_run(None).await.map(LiveScreen::Answered),
                Requested::Named(id) => fetch_run(Some(id)).await.map(LiveScreen::Answered),
            }
        },
    );
    // Cancel is a mutation, so both outcomes are notifications: the live
    // screen's own status chip is the standing record, and a silent refusal
    // below the fold would read as "nothing happened".
    let cancel = Action::new(move |id: &RunId| {
        let id = *id;
        async move {
            match cancel_run(id).await {
                Ok(()) => toast::success(
                    "Cancel requested",
                    "The engine process was signalled; the job reports cancelled once it exits.",
                ),
                Err(e) => toast::refused(Act::Cancel, &e),
            }
        }
    });
    Effect::new(move |_| {
        let _pausable = leptos_use_interval_fn::use_interval_fn(
            move || tick.update(|t| *t = t.wrapping_add(1)),
            1_000,
        );
    });

    view! {
        <Title text="Live run · Veredictum console" />
        <PageHeader
            title="Live run"
            subtitle="Progress is the engine's own stream; the estimate is a moving median, labelled as such."
            crumbs=vec![Crumb::new("Run", "/run/connect")]
        >
            <div class="flex items-center gap-1">{steps("live")}</div>
        </PageHeader>
        <Transition fallback=|| {
            view! { <p class="text-sm text-ink-muted">"Reading the run…"</p> }
        }>
            {move || Suspend::new(async move {
                match screen.await {
                    Ok(LiveScreen::Unreadable(reason)) => {
                        unknown_view(None, Some(reason)).into_any()
                    }
                    Ok(LiveScreen::Answered(RunScreen::Live(job))) => {
                        live_view(&job, cancel).into_any()
                    }
                    Ok(LiveScreen::Answered(RunScreen::Recorded(run))) => {
                        recorded_view(&run).into_any()
                    }
                    Ok(LiveScreen::Answered(RunScreen::Unknown(id))) => {
                        unknown_view(Some(id.to_string()), None).into_any()
                    }
                    Ok(LiveScreen::Answered(RunScreen::NoRunNamed)) => idle_view().into_any(),
                    Err(e) => inline_error(&e.to_string()).into_any(),
                }
            })}
        </Transition>
        {ephemeral_note()}
    }
}

/// The standing note about what this instance keeps.
///
/// Page furniture on every state, because a reader deciding whether to close
/// the tab needs it whether the run is driving or already finished.
fn ephemeral_note() -> impl IntoView + use<> {
    view! {
        <p class="mt-4 text-xs text-ink-muted">
            "This instance keeps nothing durable. A run's artifacts live long enough to be judged and shown, and a redeploy ends the runs in flight along with them."
        </p>
    }
}

/// The run's own address: what a reader copies to come back or to share.
///
/// The link and the copy carry the run's real address; the text is what the
/// screen displays, which capture mode pins.
#[component]
fn Permalink(
    /// The run's path under this console.
    #[prop(into)]
    path: String,
    /// The path as the screen prints it.
    #[prop(into)]
    shown: String,
) -> impl IntoView {
    let UseClipboardReturn { copy, copied, .. } = use_clipboard();
    let source = path.clone();
    view! {
        <div class="mt-2 flex flex-wrap items-center gap-2">
            <span class="text-xs font-medium uppercase tracking-wide text-ink-muted">
                "this run"
            </span>
            <a href=path class="font-mono text-xs text-accent hover:underline">
                {shown}
            </a>
            <button
                type="button"
                class="text-xs text-accent hover:text-accent-hover hover:underline"
                on:click=move |_| copy(&source)
            >
                {move || if copied.get() { "copied" } else { "copy" }}
            </button>
        </div>
    }
}

/// The path one run is addressed by.
fn live_path(id: RunId) -> String {
    format!("/run/live/{id}")
}

/// The no-run state, said only about a request that named no run.
fn idle_view() -> impl IntoView + use<> {
    view! {
        <div class="space-y-2">
            <p class="text-sm text-ink">"No run is in flight. Start one from Connect."</p>
            <A href="/run/connect" attr:class=BTN_SECONDARY>
                "Go to Connect"
            </A>
        </div>
    }
}

/// A run this instance never heard of, or an address it cannot read.
fn unknown_view(id: Option<String>, reason: Option<String>) -> impl IntoView + use<> {
    let named = id.map_or_else(
        || String::from("That address does not name a run."),
        |id| format!("This console knows nothing about run {id}."),
    );
    view! {
        <div class="space-y-2">
            <p class="text-sm text-ink">{named}</p>
            <p class="text-sm text-ink-muted">
                "It never ran here, or the instance that drove it is gone. Nothing was lost that this console can recover."
            </p>
            {reason
                .map(|reason| {
                    view! { <p class="font-mono text-xs text-ink-faint">{reason}</p> }
                })}
            <A href="/run/connect" attr:class=BTN_SECONDARY>
                "Go to Connect"
            </A>
        </div>
    }
}

/// A run read back from its own directory: this process never drove it, and
/// the screen says so before it states a number.
fn recorded_view(run: &RecordedRun) -> impl IntoView + use<> {
    let outcome = run.results.as_ref().map_or_else(
        || {
            view! {
                <p class="text-sm text-ink">
                    "The run left no results document, so there is no outcome to state. It was cancelled, or it stopped before the engine wrote one."
                </p>
            }
                .into_any()
        },
        |results| {
            view! {
                <div class="space-y-1">
                    <p class="text-sm text-ink">{format!("{} · finished", results.base_url)}</p>
                    <p class="tabular-nums text-sm text-ink">
                        {format!(
                            "{} passed · {} failed · {} errored · {} skipped",
                            results.passed,
                            results.failed,
                            results.errored,
                            results.skipped,
                        )}
                    </p>
                    <p class="font-mono text-xs text-ink-muted">
                        {format!("results: {}", results.results_path)}
                    </p>
                </div>
            }
                .into_any()
        },
    );
    let path = live_path(run.id);
    let shown = live_path(run.shown_id);
    view! {
        <section class=CARD_PAD>
            <h2 class=CARD_TITLE>{format!("Run {}", run.shown_id)}</h2>
            <p class="text-sm text-ink-muted">
                "This console did not drive this run. Everything below is read from the artifacts it left in the mounted output tree."
            </p>
            <div class="mt-3">{outcome}</div>
            <p class="mt-2 font-mono text-xs text-ink-muted">{format!("artifacts: {}", run.dir)}</p>
            <Permalink path=path shown=shown />
        </section>
    }
}

/// The loaded job's sections — plain assembly, erased per section.
#[expect(
    clippy::too_many_lines,
    reason = "the progress header, the tail pane and the finished summary — one cohesive assembly, each section already erased"
)]
fn live_view(job: &JobView, cancel: Action<RunId, ()>) -> impl IntoView + use<> {
    let id = job.id;
    // Display truncation is intended: a bar at 99.9% shows 99.
    let percent = job
        .completed
        .saturating_mul(100)
        .checked_div(job.total)
        .unwrap_or(0);
    let counter = if job.total > 0 {
        format!("{} / {} cases", job.completed, job.total)
    } else {
        String::from("progress stream not available from this engine build")
    };
    let eta = job.eta_ms.map(|ms| match job.status {
        JobStatus::Queued { .. } => format!("~{} until a slot frees (estimate)", mmss(ms)),
        _ => format!("~{} remaining (estimate)", mmss(ms)),
    });
    let current = job.current_case.clone().map(
        |case| view! { <p class="font-mono text-xs text-ink-muted">{format!("now: {case}")}</p> },
    );
    let status_line = match &job.status {
        JobStatus::Queued { position } => view! {
            <span class="rounded-control bg-warn-subtle px-2 py-0.5 text-xs font-medium text-ink">
                {format!("queued · position {position}")}
            </span>
        }
        .into_any(),
        JobStatus::Running => view! {
            <span class="rounded-control bg-run-subtle px-2 py-0.5 text-xs font-medium text-run-ink">
                "running"
            </span>
        }
        .into_any(),
        JobStatus::Finished => view! {
            <span class="rounded-control bg-ok-subtle px-2 py-0.5 text-xs font-medium text-ink">
                "finished"
            </span>
        }
        .into_any(),
        JobStatus::Cancelled => view! {
            <span class="rounded-control bg-warn-subtle px-2 py-0.5 text-xs font-medium text-ink">
                "cancelled"
            </span>
        }
        .into_any(),
        JobStatus::Expired => view! {
            <span class="rounded-control bg-warn-subtle px-2 py-0.5 text-xs font-medium text-ink">
                "ended by the 30-minute cap"
            </span>
        }
        .into_any(),
        JobStatus::Failed(reason) => view! {
            <span class="rounded-control bg-danger-subtle px-2 py-0.5 text-xs font-medium text-ink">
                {format!("failed: {reason}")}
            </span>
        }
        .into_any(),
    };
    let queued = matches!(job.status, JobStatus::Queued { .. });
    let note = match &job.status {
        JobStatus::Queued { .. } => Some(
            "The instance is full, so this run is waiting its turn. The address above is already the run's own: leave it open, or come back to it.",
        ),
        JobStatus::Expired => Some(
            "This run passed the wall clock a hosted instance allows, so the console ended it and discarded what it had written. Narrow the filter and run it again.",
        ),
        _ => None,
    }
    .map(|line| view! { <p class="mt-2 text-sm text-ink-muted">{line}</p> });
    let stoppable = queued || job.status == JobStatus::Running;
    let tail = job.tail.join("\n");
    let finished = job.finished.clone().map(|summary| {
        view! {
            <section class=format!("{CARD_PAD} mt-4")>
                <h2 class=CARD_TITLE>"Outcome"</h2>
                <p class="tabular-nums text-sm text-ink">
                    {format!(
                        "{} passed · {} failed · {} errored · {} skipped",
                        summary.passed,
                        summary.failed,
                        summary.errored,
                        summary.skipped,
                    )}
                </p>
                <p class="mt-1 font-mono text-xs text-ink-muted">
                    {format!("results: {}", summary.results_path)}
                </p>
                <div class="mt-2 flex items-center gap-2">
                    <A href="/run/results" attr:class=BTN_PRIMARY>
                        "Read the results"
                    </A>
                </div>
            </section>
        }
    });
    view! {
        <section class=CARD_PAD>
            <div class="flex flex-wrap items-center justify-between gap-3">
                <div class="flex items-center gap-2">
                    <h2 class="text-sm font-semibold text-ink-heading">
                        <span id="live-sut">{format!("{} {}", job.sut_name, job.sut_version)}</span>
                        {format!(" · {}", job.shown_id)}
                    </h2>
                    {status_line}
                </div>
                {stoppable
                    .then(|| {
                        view! {
                            <button
                                type="button"
                                class=crate::components::field::BTN_DANGER
                                on:click=move |_| {
                                    cancel.dispatch(id);
                                }
                            >
                                {if queued { "Leave the queue" } else { "Cancel run" }}
                            </button>
                        }
                    })}
            </div>
            <Permalink path=live_path(id) shown=live_path(job.shown_id) />
            {note}
            <div class="mt-3 h-2 w-full overflow-hidden rounded-control bg-sunken">
                <div
                    class="h-full rounded-control bg-run transition-all"
                    style=format!("width: {percent}%")
                ></div>
            </div>
            <div class="mt-2 flex flex-wrap items-center gap-3 text-sm text-ink-muted">
                <span class="tabular-nums">{counter}</span>
                <span class="tabular-nums">{format!("elapsed {}", mmss(job.elapsed_ms))}</span>
                {eta.map(|eta| view! { <span class="tabular-nums">{eta}</span> })}
            </div>
            {current}
        </section>
        <div class="mt-4">
            <Pane label="engine output (tail)" body=tail />
        </div>
        {finished}
    }
}
