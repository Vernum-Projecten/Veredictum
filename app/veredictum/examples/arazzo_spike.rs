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
//!     --user alice --password secret --out target/arazzo-run
//! cargo run --example arazzo_spike -- --rejudge target/arazzo-run/recording.har
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use base64::Engine;
use clap::Parser;
use veredictum::arazzo::document::ArazzoDocument;
use veredictum::arazzo::evaluate::{Plan, RunConfig, rejudge, run_document};
use veredictum::arazzo::har::Har;
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
    /// The Arazzo document to drive.
    #[arg(long, default_value = "artifacts/workflows/ehr/ehr.arazzo.yaml")]
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
    let mut plan = Plan::default();
    for workflow in &document.description().workflows {
        let path = rows_path(&args.document, &workflow.workflow_id);
        if path.is_file() {
            let case_id = format!("{name}/{}", workflow.workflow_id);
            let rows = rows::load(&path, &case_id, workflow)?;
            plan.rows.insert(workflow.workflow_id.clone(), rows);
        }
    }
    let (results, recording) = if let Some(path) = &args.rejudge {
        let recording = Har::read(path)?;
        let results = rejudge(&document, name, &args.repo_root, None, &plan, &recording)?;
        (results, None)
    } else {
        let authorization = args.user.as_ref().map(|user| {
            let pair = format!("{user}:{}", args.password);
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(pair)
            )
        });
        let config = RunConfig {
            base_url: args.base_url.clone().ok_or("--base-url is required")?,
            authorization,
            signing: None,
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
