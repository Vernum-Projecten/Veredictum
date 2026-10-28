// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Drives every workflow of one Arazzo document against a running server
//! under test, or re-judges a recording of an earlier run, and prints one row
//! per workflow row.
//!
//! The document is gated first, and a document with any finding is not run.
//! A failed or errored row prints its four-part explanation. With `--out`,
//! the results and the HAR recording are written there; the recording holds
//! response bodies verbatim, so it belongs where the operator keeps the run.
//!
//! ```text
//! cargo run --example arazzo_spike -- \
//!     --base-url http://127.0.0.1:8080/ehrbase/rest/openehr/v1 \
//!     --user alice --password secret --admin-user root --admin-password secret \
//!     --out target/arazzo-run
//! cargo run --example arazzo_spike -- --rejudge target/arazzo-run/recording.har
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use base64::Engine;
use clap::Parser;
use veredictum::arazzo::capabilities::Capabilities;
use veredictum::arazzo::document::ArazzoDocument;
use veredictum::arazzo::evaluate::{Plan, RunConfig, rejudge, run_document};
use veredictum::arazzo::har::Har;
use veredictum::arazzo::principal::{self, Principal};
use veredictum::arazzo::rows::{self, rows_path};
use veredictum::exec::transport::HttpTransport;

/// The command line of the example.
#[derive(Debug, Parser)]
struct Args {
    /// The base URL the OAS paths are appended to.
    #[arg(long, required_unless_present = "rejudge")]
    base_url: Option<String>,
    /// The basic-auth user, if the server needs one.
    #[arg(long)]
    user: Option<String>,
    /// The basic-auth password.
    #[arg(long, default_value = "")]
    password: String,
    /// The basic-auth user of the administrative principal; without it,
    /// every workflow with an administrative step is skipped as not declared.
    #[arg(long)]
    admin_user: Option<String>,
    /// The basic-auth password of the administrative principal.
    #[arg(long, default_value = "")]
    admin_password: String,
    /// The Arazzo document to drive.
    #[arg(long, default_value = "catalogue/cases/ehr/ehr.arazzo.yaml")]
    document: PathBuf,
    /// The repository root citations are relative to.
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,
    /// Where to write `results.json` and `recording.har`.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Re-judge this recording instead of driving a server.
    #[arg(long)]
    rejudge: Option<PathBuf>,
    /// The party's statement: its `options` select the arm of every option
    /// in `capabilities.yaml`, and a document of another arm is skipped.
    #[arg(long)]
    statement: Option<PathBuf>,
    /// The party's IXIT, which declared inputs read.
    #[arg(long)]
    ixit: Option<PathBuf>,
    /// The capabilities file the options come from.
    #[arg(long, default_value = "catalogue/capabilities.yaml")]
    capabilities: PathBuf,
}

/// The plan of one run: the party's declarations, the arms its statement
/// does not select, and every rows file beside the document.
fn plan_for(
    args: &Args,
    document: &ArazzoDocument,
    name: &str,
) -> Result<Plan, Box<dyn std::error::Error>> {
    let mut plan = Plan::default();
    let read_json = |path: &PathBuf| -> Result<_, Box<dyn std::error::Error>> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    };
    if let Some(path) = &args.ixit {
        plan.ixit = Some(read_json(path)?);
    }
    if let Some(path) = &args.statement {
        plan.statement = Some(read_json(path)?);
    }
    if args.capabilities.is_file() {
        let capabilities = Capabilities::load(&args.capabilities)?;
        let selected: Vec<String> = plan
            .statement
            .as_ref()
            .and_then(|s| s.get("options"))
            .and_then(|options| options.as_array())
            .map(|options| {
                options
                    .iter()
                    .filter_map(|option| option.as_str())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        if let Some(skip) = capabilities.skip_for(name, &selected, &args.repo_root) {
            for workflow in &document.description().workflows {
                plan.skips
                    .insert(workflow.workflow_id.clone(), skip.clone());
            }
        }
    }
    // A recording ran the administrative workflows exactly when the recorded
    // run was given the administrative principal.
    let administrative = match &args.rejudge {
        Some(path) => {
            let recording = Har::read(path)?;
            let prefix = format!("{name}/");
            recording.log.pages.iter().any(|page| {
                page.veredictum
                    .case
                    .strip_prefix(&prefix)
                    .and_then(|id| document.workflow(id))
                    .is_some_and(|w| principal::names(w, Principal::Administrative))
            })
        }
        None => args.admin_user.is_some(),
    };
    if !administrative {
        for (workflow, skip) in
            principal::undeclared_skips(document, Principal::Administrative, &args.repo_root)
        {
            plan.skips.entry(workflow).or_insert(skip);
        }
    }
    for workflow in &document.description().workflows {
        let path = rows_path(&args.document, &workflow.workflow_id);
        if path.is_file() {
            let case_id = format!("{name}/{}", workflow.workflow_id);
            let rows = rows::load(&path, &case_id, workflow)?;
            plan.rows.insert(workflow.workflow_id.clone(), rows);
        }
    }
    Ok(plan)
}

#[expect(
    clippy::print_stdout,
    reason = "an example's whole output is its report to the operator"
)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let document = ArazzoDocument::load(&args.document)?;
    let findings = document.check(&args.repo_root);
    if !findings.is_empty() {
        for finding in &findings {
            println!("finding  {finding}");
        }
        return Err(format!("{} gate finding(s); nothing was run", findings.len()).into());
    }
    let name = args
        .document
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .and_then(|file| file.strip_suffix(".arazzo.yaml"))
        .ok_or("the document's file name does not end in .arazzo.yaml")?;
    let plan = plan_for(&args, &document, name)?;
    let (results, recording) = if let Some(path) = &args.rejudge {
        let recording = Har::read(path)?;
        let results = rejudge(&document, name, &args.repo_root, None, &plan, &recording)?;
        (results, None)
    } else {
        let basic = |user: &String, password: &str| {
            let pair = format!("{user}:{password}");
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(pair)
            )
        };
        let config = RunConfig {
            base_url: args.base_url.clone().ok_or("--base-url is required")?,
            authorization: args.user.as_ref().map(|user| basic(user, &args.password)),
            signing: None,
            administrative: args
                .admin_user
                .as_ref()
                .map(|user| basic(user, &args.admin_password)),
        };
        let mut transport = HttpTransport::new()?;
        let (results, recording) = run_document(
            &document,
            name,
            config,
            &args.repo_root,
            &plan,
            &mut transport,
        )?;
        (results, Some(recording))
    };
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    for row in &results.rows {
        *tally.entry(row.status.to_string()).or_default() += 1;
        println!("{row}");
        if row.explanation.is_some() || row.skip.is_some() {
            println!();
        }
    }
    println!(
        "{}",
        tally
            .iter()
            .map(|(status, count)| format!("{status}: {count}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if let Some(out) = &args.out {
        std::fs::create_dir_all(out)?;
        std::fs::write(out.join("results.json"), results.to_text()?)?;
        if let Some(recording) = recording {
            std::fs::write(out.join("recording.har"), recording.to_text()?)?;
        }
    }
    Ok(())
}
