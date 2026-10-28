// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The OpenAPI source descriptions a document names, indexed by
//! `operationId`.
//!
//! An `operationId` is an address only: it yields the method and the path
//! template a step is sent to, and never an expectation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::SystemTime;

use serde_json::Value;

use crate::vocab::HttpMethod;

/// A file as last seen: its path, modification time and length.
type Stamp = (PathBuf, SystemTime, u64);

/// Returns the index of the file at `path`, built once per process for each
/// version of the file a [`Stamp`] tells apart; a file whose metadata cannot
/// be read is indexed afresh every time.
// NOTE: no openEHR spec governs this — our own design: every catalogue
// document names the same OpenAPI files, and parsing them per document dominated `validate`.
fn memoized<T: Clone, E>(
    cache: &Mutex<BTreeMap<Stamp, T>>,
    path: &Path,
    build: fn(&Path) -> Result<T, E>,
) -> Result<T, E> {
    let stamp = std::fs::metadata(path)
        .ok()
        .and_then(|meta| Some((path.to_owned(), meta.modified().ok()?, meta.len())));
    let Some(stamp) = stamp else {
        return build(path);
    };
    if let Some(hit) = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&stamp)
    {
        return Ok(hit.clone());
    }
    let index = build(path)?;
    cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(stamp, index.clone());
    Ok(index)
}

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
        static INDEXED: LazyLock<Mutex<BTreeMap<Stamp, OperationIndex>>> =
            LazyLock::new(Mutex::default);
        memoized(&INDEXED, path, Self::index)
    }

    fn index(path: &Path) -> Result<Self, SourceError> {
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

/// Where an OpenAPI 3.0 parameter is carried (OAS 3.0.3 §Parameter Object,
/// `in`: "Possible values are "query", "header", "path" or "cookie"").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ParameterIn {
    /// `path`.
    Path,
    /// `query`.
    Query,
    /// `header`.
    Header,
    /// `cookie`.
    Cookie,
}

impl ParameterIn {
    /// Parses the OAS `in` token; any other token is `None`.
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        Some(match token {
            "path" => Self::Path,
            "query" => Self::Query,
            "header" => Self::Header,
            "cookie" => Self::Cookie,
            _ => return None,
        })
    }

    /// The OAS `in` token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Query => "query",
            Self::Header => "header",
            Self::Cookie => "cookie",
        }
    }
}

/// One parameter an operation declares, after every `$ref` is followed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeclaredParameter {
    /// Where the parameter is carried.
    pub location: ParameterIn,
    /// The parameter name, as the OAS spells it.
    pub name: String,
}

impl DeclaredParameter {
    /// Whether a step parameter named `name` in `location` addresses this
    /// declaration. Header names compare case-insensitively (RFC 9110
    /// §5.1: "Field names are case-insensitive"); every other name exactly.
    #[must_use]
    pub fn matches(&self, location: ParameterIn, name: &str) -> bool {
        self.location == location
            && match location {
                ParameterIn::Header => self.name.eq_ignore_ascii_case(name),
                ParameterIn::Path | ParameterIn::Query | ParameterIn::Cookie => self.name == name,
            }
    }
}

/// Why an operation's parameters cannot be read.
#[derive(Debug, thiserror::Error)]
pub enum ParameterError {
    /// The file does not index as a source at all.
    #[error(transparent)]
    Source(#[from] SourceError),
    /// A `$ref` that is not a local JSON Pointer, or names nothing.
    #[error("source `{path}`: `$ref` `{reference}` does not resolve inside the document")]
    UnresolvedRef {
        /// The file.
        path: PathBuf,
        /// The reference.
        reference: String,
    },
    /// A chain of `$ref`s longer than any the released bundles use.
    #[error("source `{path}`: `$ref` `{reference}` does not settle within {MAX_REF_HOPS} hops")]
    RefChain {
        /// The file.
        path: PathBuf,
        /// The reference the chain started at.
        reference: String,
    },
    /// A parameter whose `name` or `in` is missing, or whose `in` is outside
    /// the four OAS 3.0 locations.
    #[error("source `{path}`: operation `{operation_id}` declares a malformed parameter: {reason}")]
    Malformed {
        /// The file.
        path: PathBuf,
        /// The operation.
        operation_id: String,
        /// What is wrong.
        reason: String,
    },
}

/// The longest `$ref` chain [`ParameterIndex`] follows before refusing.
const MAX_REF_HOPS: usize = 16;

/// The parameters every operation of one OpenAPI document declares, by
/// `operationId`, with path-item parameters merged in.
///
/// OAS 3.0.3 §Operation Object, `parameters`: "If a parameter is already
/// defined at the Path Item, the new definition will override it but can
/// never remove it", so an operation's set is the union of both levels.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParameterIndex {
    parameters: BTreeMap<String, Vec<DeclaredParameter>>,
}

impl ParameterIndex {
    /// Indexes the declared parameters of the OpenAPI document at `path`,
    /// following every local `$ref`.
    ///
    /// # Errors
    /// [`ParameterError`] when the file does not load, a `$ref` does not
    /// resolve inside the document, or a parameter is malformed.
    pub fn load(path: &Path) -> Result<Self, ParameterError> {
        static INDEXED: LazyLock<Mutex<BTreeMap<Stamp, ParameterIndex>>> =
            LazyLock::new(Mutex::default);
        memoized(&INDEXED, path, Self::index)
    }

    fn index(path: &Path) -> Result<Self, ParameterError> {
        let document =
            crate::load::yaml_file_to_value(path).map_err(|error| SourceError::Load {
                path: path.to_owned(),
                reason: error.detail(),
            })?;
        let Some(Value::Object(paths)) = document.get("paths") else {
            return Err(SourceError::NoPaths(path.to_owned()).into());
        };
        let mut parameters = BTreeMap::new();
        for item in paths.values() {
            let item = resolve(&document, item, path)?;
            let shared = item.get("parameters");
            let Value::Object(item) = item else {
                continue;
            };
            for (key, operation) in item {
                if PATH_ITEM_FIELDS.contains(&key.as_str()) || key.starts_with("x-") {
                    continue;
                }
                let operation = resolve(&document, operation, path)?;
                let Some(Value::String(operation_id)) = operation.get("operationId") else {
                    continue;
                };
                let mut declared = Vec::new();
                for list in [shared, operation.get("parameters")].into_iter().flatten() {
                    let Value::Array(list) = list else {
                        return Err(malformed(path, operation_id, "`parameters` is not a list"));
                    };
                    for parameter in list {
                        let parameter = resolve(&document, parameter, path)?;
                        let name = parameter.get("name").and_then(Value::as_str);
                        let location = parameter
                            .get("in")
                            .and_then(Value::as_str)
                            .map(|token| (token, ParameterIn::parse(token)));
                        let (name, location) = match (name, location) {
                            (Some(name), Some((_, Some(location)))) => (name, location),
                            (_, Some((token, None))) => {
                                return Err(malformed(
                                    path,
                                    operation_id,
                                    &format!("`in: {token}` is not an OAS 3.0 location"),
                                ));
                            }
                            _ => {
                                return Err(malformed(
                                    path,
                                    operation_id,
                                    "a parameter without `name` or `in`",
                                ));
                            }
                        };
                        declared.push(DeclaredParameter {
                            location,
                            name: name.to_owned(),
                        });
                    }
                }
                declared.sort();
                declared.dedup();
                parameters.insert(operation_id.clone(), declared);
            }
        }
        Ok(Self { parameters })
    }

    /// The parameters operation `operation_id` declares, if the document
    /// declares that operation.
    #[must_use]
    pub fn get(&self, operation_id: &str) -> Option<&[DeclaredParameter]> {
        self.parameters.get(operation_id).map(Vec::as_slice)
    }
}

fn malformed(path: &Path, operation_id: &str, reason: &str) -> ParameterError {
    ParameterError::Malformed {
        path: path.to_owned(),
        operation_id: operation_id.to_owned(),
        reason: reason.to_owned(),
    }
}

/// Follows `value`'s local `$ref` chain (`#/…`, a JSON Pointer into the same
/// document) to the object it names.
fn resolve<'a>(
    document: &'a Value,
    value: &'a Value,
    path: &Path,
) -> Result<&'a Value, ParameterError> {
    let mut current = value;
    let start = value.get("$ref").and_then(Value::as_str);
    for _ in 0..MAX_REF_HOPS {
        let Some(reference) = current.get("$ref").and_then(Value::as_str) else {
            return Ok(current);
        };
        current = reference
            .strip_prefix('#')
            .and_then(|pointer| document.pointer(pointer))
            .ok_or_else(|| ParameterError::UnresolvedRef {
                path: path.to_owned(),
                reference: reference.to_owned(),
            })?;
    }
    Err(ParameterError::RefChain {
        path: path.to_owned(),
        reference: start.unwrap_or_default().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_released_ehr_bundle_declares_its_parameters_through_refs() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../specs/rest-oas/ehr-validation.openapi.yaml");
        let index = ParameterIndex::load(&path).unwrap();
        let declared = index.get("ehr_create_with_id").unwrap();
        for (location, name) in [
            (ParameterIn::Path, "ehr_id"),
            (ParameterIn::Header, "prefer"),
            (ParameterIn::Header, "Accept"),
            (ParameterIn::Header, "content-type"),
        ] {
            assert!(
                declared.iter().any(|p| p.matches(location, name)),
                "{name} in {location:?}: {declared:?}"
            );
        }
        assert!(
            !declared
                .iter()
                .any(|p| p.matches(ParameterIn::Query, "ehr_id"))
        );
        assert!(
            !declared
                .iter()
                .any(|p| p.matches(ParameterIn::Path, "EHR_ID"))
        );
        assert_eq!(index.get("no_such_operation"), None);
    }

    #[test]
    fn a_seeded_dangling_or_cyclic_ref_is_refused() {
        use assert_fs::prelude::*;
        let dir = assert_fs::TempDir::new().unwrap();
        let file = dir.child("oas.yaml");
        file.write_str(
            "openapi: 3.0.3\npaths:\n  /a:\n    get:\n      operationId: a\n      parameters:\n        - $ref: '#/components/parameters/missing'\ncomponents: {}\n",
        )
        .unwrap();
        assert!(matches!(
            ParameterIndex::load(file.path()),
            Err(ParameterError::UnresolvedRef { .. })
        ));
        file.write_str(
            "openapi: 3.0.3\npaths:\n  /a:\n    get:\n      operationId: a\n      parameters:\n        - $ref: '#/components/parameters/x'\ncomponents:\n  parameters:\n    x: { $ref: '#/components/parameters/x' }\n",
        )
        .unwrap();
        assert!(matches!(
            ParameterIndex::load(file.path()),
            Err(ParameterError::RefChain { .. })
        ));
        file.write_str(
            "openapi: 3.0.3\npaths:\n  /a:\n    get:\n      operationId: a\n      parameters:\n        - { name: x, in: body }\n",
        )
        .unwrap();
        assert!(matches!(
            ParameterIndex::load(file.path()),
            Err(ParameterError::Malformed { .. })
        ));
    }

    #[test]
    fn a_rewritten_source_is_indexed_again() {
        use assert_fs::prelude::*;
        let dir = assert_fs::TempDir::new().unwrap();
        let file = dir.child("oas.yaml");
        file.write_str("openapi: 3.0.3\npaths:\n  /a:\n    get:\n      operationId: a\n")
            .unwrap();
        let first = OperationIndex::load(file.path()).unwrap();
        assert!(first.get("a").is_some());
        assert_eq!(OperationIndex::load(file.path()).unwrap(), first);
        file.write_str("openapi: 3.0.3\npaths:\n  /bb:\n    get:\n      operationId: bb\n")
            .unwrap();
        let second = OperationIndex::load(file.path()).unwrap();
        assert!(second.get("a").is_none());
        assert!(second.get("bb").is_some());
        let parameters = ParameterIndex::load(file.path()).unwrap();
        assert_eq!(parameters, ParameterIndex::load(file.path()).unwrap());
    }

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
