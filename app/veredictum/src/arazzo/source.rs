// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The OpenAPI source descriptions a document names, indexed by
//! `operationId`.
//!
//! An `operationId` is an address only: it yields the method and the path
//! template a step is sent to, and never an expectation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::vocab::HttpMethod;

/// One addressable operation of a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    /// The HTTP method.
    pub method: HttpMethod,
    /// The path template, as the OAS `paths` key spells it.
    pub path: String,
}

/// Why a source cannot be indexed.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// The source `url` is not a local relative file path.
    #[error("source `{0}` is not a local relative path")]
    NotLocal(String),
    /// The file does not load as YAML.
    #[error("source `{path}`: {reason}")]
    Load {
        /// The file.
        path: PathBuf,
        /// The verbatim failure.
        reason: String,
    },
    /// The OAS has no `paths` object.
    #[error("source `{0}` has no `paths` object")]
    NoPaths(PathBuf),
    /// An operation uses a method outside the closed set.
    #[error("source `{path}`: `{method}` on `{route}` is outside the supported method set")]
    UnsupportedMethod {
        /// The file.
        path: PathBuf,
        /// The route.
        route: String,
        /// The method key.
        method: String,
    },
    /// Two operations share one `operationId`.
    #[error("source `{path}`: operationId `{operation_id}` is declared twice")]
    DuplicateOperation {
        /// The file.
        path: PathBuf,
        /// The repeated id.
        operation_id: String,
    },
}

/// The operations of one OpenAPI document, by `operationId`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperationIndex {
    operations: BTreeMap<String, Operation>,
}

/// The OAS 3.0 path-item keys that are not operations.
const PATH_ITEM_FIELDS: [&str; 5] = ["$ref", "summary", "description", "servers", "parameters"];

impl OperationIndex {
    /// Indexes the OpenAPI document at `path`.
    ///
    /// # Errors
    /// [`SourceError`] when the file does not load, has no `paths`, uses a
    /// method outside [`HttpMethod`], or repeats an `operationId`.
    pub fn load(path: &Path) -> Result<Self, SourceError> {
        let document =
            crate::load::yaml_file_to_value(path).map_err(|error| SourceError::Load {
                path: path.to_owned(),
                reason: error.detail(),
            })?;
        let Some(Value::Object(paths)) = document.get("paths") else {
            return Err(SourceError::NoPaths(path.to_owned()));
        };
        let mut operations = BTreeMap::new();
        for (route, item) in paths {
            let Value::Object(item) = item else {
                return Err(SourceError::Load {
                    path: path.to_owned(),
                    reason: format!("path item `{route}` is not an object"),
                });
            };
            for (key, operation) in item {
                if PATH_ITEM_FIELDS.contains(&key.as_str()) || key.starts_with("x-") {
                    continue;
                }
                let method = match key.as_str() {
                    "get" => HttpMethod::Get,
                    "post" => HttpMethod::Post,
                    "put" => HttpMethod::Put,
                    "delete" => HttpMethod::Delete,
                    "head" => HttpMethod::Head,
                    "options" => HttpMethod::Options,
                    other => {
                        return Err(SourceError::UnsupportedMethod {
                            path: path.to_owned(),
                            route: route.clone(),
                            method: other.to_owned(),
                        });
                    }
                };
                // NOTE: OAS 3.0.3 §Operation Object makes `operationId` optional; an
                // operation without one is legitimately absent, since no step can name it.
                let Some(Value::String(operation_id)) = operation.get("operationId") else {
                    continue;
                };
                let previous = operations.insert(
                    operation_id.clone(),
                    Operation {
                        method,
                        path: route.clone(),
                    },
                );
                if previous.is_some() {
                    return Err(SourceError::DuplicateOperation {
                        path: path.to_owned(),
                        operation_id: operation_id.clone(),
                    });
                }
            }
        }
        Ok(Self { operations })
    }

    /// The operation named `operation_id`, if the source declares it.
    #[must_use]
    pub fn get(&self, operation_id: &str) -> Option<&Operation> {
        self.operations.get(operation_id)
    }
}

/// Splits a qualified `$sourceDescriptions.<name>.<operationId>` reference.
///
/// Returns `None` for an unqualified `operationId`, which the catalogue does
/// not use because two released sources share operation ids.
#[must_use]
pub fn split_qualified(operation_id: &str) -> Option<(&str, &str)> {
    operation_id
        .strip_prefix("$sourceDescriptions.")?
        .split_once('.')
        .filter(|(name, id)| !name.is_empty() && !id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_released_ehr_bundle_indexes_its_create_and_read_operations() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../specs/rest-oas/ehr-validation.openapi.yaml");
        let index = OperationIndex::load(&path).unwrap();
        assert_eq!(
            index.get("ehr_create_with_id"),
            Some(&Operation {
                method: HttpMethod::Put,
                path: "/ehr/{ehr_id}".to_owned()
            })
        );
        assert_eq!(
            index.get("ehr_create").map(|o| o.method),
            Some(HttpMethod::Post)
        );
        assert_eq!(index.get("no_such_operation"), None);
    }

    #[test]
    fn qualified_references_split_and_bare_ones_do_not() {
        assert_eq!(
            split_qualified("$sourceDescriptions.ehr.ehr_get_by_id"),
            Some(("ehr", "ehr_get_by_id"))
        );
        assert_eq!(split_qualified("ehr_get_by_id"), None);
        assert_eq!(split_qualified("$sourceDescriptions.ehr."), None);
    }
}
