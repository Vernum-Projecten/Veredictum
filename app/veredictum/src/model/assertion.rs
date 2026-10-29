// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `result_set` column expectation a query row asserts.

use serde::Deserialize;

/// A `result_set` column expectation (identity: the `AS` alias, else the
/// 0-based index — ITS-REST `ResultSetColumn.yaml`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnSpec {
    /// The expected column name (alias).
    pub name: String,
}
