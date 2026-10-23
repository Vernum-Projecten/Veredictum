// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Drives every workflow of one Arazzo document against a running server
//! under test, and prints one row per workflow.
//!
//! The document is gated first, and a document with any finding is not run.
//! A failed or errored row prints its four-part explanation with a short
//! excerpt of the response body.
//!
//! ```text
//! cargo run --example arazzo_spike -- \
//!     --base-url http://127.0.0.1:8080/ehrbase/rest/openehr/v1 \
//!     --user alice --password secret
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use base64::Engine;
use clap::Parser;
use veredictum::arazzo::document::ArazzoDocument;
use veredictum::arazzo::evaluate::{RowStatus, RunConfig, run_workflow};
use veredictum::exec::transport::HttpTransport;

/// The command line of the example.
#[derive(Debug, Parser)]
struct Args {
    /// The base URL the OAS paths are appended to.
    #[arg(long)]
    base_url: String,
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
        .unwrap_or("document");
    let authorization = args.user.as_ref().map(|user| {
        let pair = format!("{user}:{}", args.password);
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(pair)
        )
    });
    let config = RunConfig {
        base_url: args.base_url.clone(),
        authorization,
    };
    let mut transport = HttpTransport::new()?;
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    for workflow in &document.description().workflows {
        let run = run_workflow(
            &document,
            name,
            &workflow.workflow_id,
            &BTreeMap::new(),
            &config,
            &mut transport,
        )?;
        let label = match run.status {
            RowStatus::Skipped(_) => "SKIPPED".to_owned(),
            other => other.to_string(),
        };
        *tally.entry(label).or_default() += 1;
        println!("{run}");
        if run.status != RowStatus::Passed {
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
    Ok(())
}
