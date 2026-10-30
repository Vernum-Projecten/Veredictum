// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Re-judging a `perf-result` record from a file.

use std::path::Path;

use crate::perf_result::judge::{Rejudgement, rejudge};
use crate::pipeline::Error;

/// Reads the `perf-result` at `path` and re-derives it from its raw part.
///
/// # Errors
/// [`Error::Read`] when the file cannot be read, and [`Error::Party`] naming
/// the file when it is not JSON, breaks the published schema, or its raw part
/// does not re-derive.
pub fn judge_perf_result(path: &Path) -> Result<Rejudgement, Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    rejudge(&text).map_err(|e| Error::Party(format!("{}: {e}", path.display())))
}
