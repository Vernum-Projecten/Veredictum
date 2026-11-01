// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Party-declared extension sources: vendor routes that prepare ground for a
//! case and are never judged.
//!
//! Some preconditions can only be established through a route no released
//! ITS-REST OAS declares, such as an EHR import. The catalogue names such a
//! route by an Arazzo source description carrying `x-veredictum-extension`,
//! whose value is an [`ExtensionKey`]; its `url` is a placeholder the runner
//! never reads. A party that offers the route declares it in its statement's
//! `extensions`, naming its own OpenAPI document and the `operationId` in it
//! that realises the extension, and the run sends the step to that
//! operation. Only a step of a shared setup under `cases/_setup/` may call an
//! extension operation, its response only decides whether the setup
//! proceeds and feeds outputs, and a case whose setups need an extension the
//! statement does not declare is skipped as `extension-not-offered`.
//! No openEHR spec governs this — our own design.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use roas_arazzo::v1_1::{Step, Workflow};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::arazzo::catalogue::STATEMENT_SCHEMA;
use crate::arazzo::depends::{Library, Seen};
use crate::arazzo::document::ArazzoDocument;
use crate::arazzo::gates::workflow_label;
use crate::arazzo::results::{Skip, SkipReason, Why};
use crate::arazzo::source::{Operation, OperationIndex, SourceError};
use crate::party::DeclaredExtension;

/// The specification-extension key, on an Arazzo source description, that
/// names the extension the source stands for.
pub const EXTENSION_KEY: &str = "x-veredictum-extension";

/// The section of the statement schema an `extension-not-offered` skip
/// cites.
pub const EXTENSIONS_SECTION: &str = "#/properties/extensions";

/// The rule an `extension-not-offered` skip quotes, verbatim from
/// [`EXTENSIONS_DESCRIPTION`].
pub const EXTENSIONS_RULE: &str = "Every case whose setups call an extension the statement does not declare here is skipped extension-not-offered.";

/// The description the statement schema publishes for `extensions`, which
/// carries [`EXTENSIONS_RULE`]. No openEHR spec governs this — our own
/// design.
pub const EXTENSIONS_DESCRIPTION: &str = "The extension routes the party offers, each keyed by an extension the catalogue defines: the party's own OpenAPI document, a path relative to the statement, and the operationId in it that realises the extension. An extension route only prepares ground for a case in a shared setup, and no verdict ever judges its response. Every case whose setups call an extension the statement does not declare here is skipped extension-not-offered.";

/// An extension the catalogue defines: a precondition only a route outside
/// the released ITS-REST OAS can establish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionKey {
    /// An EHR import: places an EHR with versions the server did not create
    /// through its own commit operations.
    EhrImport,
}

impl ExtensionKey {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::EhrImport];

    /// The token a source description and a statement spell.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::EhrImport => "ehr-import",
        }
    }

    /// Reads a token; an unknown one is refused, never defaulted.
    ///
    /// # Errors
    /// [`ExtensionError::Unknown`] for a token outside the vocabulary.
    pub fn parse(token: &str) -> Result<Self, ExtensionError> {
        Self::ALL
            .iter()
            .copied()
            .find(|key| key.token() == token)
            .ok_or_else(|| ExtensionError::Unknown(token.to_owned()))
    }
}

impl fmt::Display for ExtensionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.token())
    }
}

/// Why a use of an extension source does not hold.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExtensionError {
    /// The value of `x-veredictum-extension` is not a string.
    #[error("`{EXTENSION_KEY}` is not a string")]
    NotAString,
    /// The token names no extension the catalogue defines.
    #[error("`{EXTENSION_KEY}` names `{0}`, which is not an extension the catalogue defines")]
    Unknown(String),
    /// The extension source is not a `type: openapi` source.
    #[error("an extension source stands for an OpenAPI document and declares `type: openapi`")]
    NotOpenapi,
    /// A step outside a shared setup calls an extension operation.
    #[error(
        "`{operation}` is an extension operation, which only a workflow of a shared setup under `cases/_setup/` may call"
    )]
    OutsideSetup {
        /// The qualified operation the step calls.
        operation: String,
    },
    /// A step addresses an extension source by another operation than its
    /// extension key.
    #[error(
        "`{operation}` calls an extension source by another name than its extension `{key}`, which is the only operation it offers"
    )]
    NotTheKey {
        /// The qualified operation the step calls.
        operation: String,
        /// The extension the source stands for.
        key: ExtensionKey,
    },
    /// An extension step carries its own citation.
    #[error(
        "an extension step carries a `x-veredictum-cite`, while its response only decides whether the setup proceeds"
    )]
    CitedStep,
    /// A criterion of an extension step carries its own citation.
    #[error(
        "a criterion of an extension step carries a `x-veredictum-cite`, while its response only decides whether the setup proceeds"
    )]
    CitedCriterion,
    /// A criterion of an extension step carries a judge.
    #[error(
        "a criterion of an extension step carries a `x-veredictum-check` judge, while its response only decides whether the setup proceeds"
    )]
    CheckedCriterion,
    /// A rows file feeds a workflow that calls an extension operation.
    #[error(
        "a rows file feeds a workflow that calls the extension `{0}`, while an extension step only prepares ground"
    )]
    Rows(ExtensionKey),
}

/// The extension a source description's extensions name, if any.
///
/// # Errors
/// [`ExtensionError`] when `x-veredictum-extension` is present and does not
/// name an extension of the vocabulary.
pub fn extension_of(
    extensions: Option<&BTreeMap<String, Value>>,
) -> Result<Option<ExtensionKey>, ExtensionError> {
    match extensions.and_then(|e| e.get(EXTENSION_KEY)) {
        None => Ok(None),
        Some(Value::String(token)) => ExtensionKey::parse(token).map(Some),
        Some(_) => Err(ExtensionError::NotAString),
    }
}

/// The extension `step` of `document` calls, if any.
#[must_use]
pub fn called(document: &ArazzoDocument, step: &Step) -> Option<ExtensionKey> {
    let (source, _) = step
        .operation_id
        .as_deref()
        .and_then(crate::arazzo::source::split_qualified)?;
    document.extension_source(source)
}

/// The first extension a step of `workflow` of `document` calls, if any.
#[must_use]
pub fn called_by(document: &ArazzoDocument, workflow: &Workflow) -> Option<ExtensionKey> {
    workflow
        .steps
        .iter()
        .find_map(|step| called(document, step))
}

/// The extension operation the party offers for each extension it declares.
pub type Offered = BTreeMap<ExtensionKey, Operation>;

/// Why a statement's `extensions` does not bind.
#[derive(Debug, thiserror::Error)]
pub enum BindError {
    /// `extensions` is not a map from an extension key to a declared route.
    #[error("the statement's `extensions` is not a map of extension keys to routes: {0}")]
    Shape(String),
    /// The party's OpenAPI document does not index.
    #[error("the statement's `extensions.{key}` names a document that does not load")]
    Document {
        /// The extension.
        key: ExtensionKey,
        /// Why the document does not index.
        #[source]
        source: SourceError,
    },
    /// The party's document does not declare the named operation.
    #[error(
        "the statement's `extensions.{key}` names operation `{operation_id}`, which `{}` does not declare",
        document.display()
    )]
    NoOperation {
        /// The extension.
        key: ExtensionKey,
        /// The party's document.
        document: PathBuf,
        /// The operation the statement names.
        operation_id: String,
    },
}

/// The operations the statement's `extensions` offer, each resolved in the
/// party's document, whose path is read relative to `statement_dir`.
///
/// No statement, or one without `extensions`, offers none.
///
/// # Errors
/// [`BindError`] when `extensions` does not parse, names an unknown key,
/// names a document that does not index, or an operation the document does
/// not declare.
pub fn offered(statement: Option<&Value>, statement_dir: &Path) -> Result<Offered, BindError> {
    let Some(declared) = statement.and_then(|s| s.get("extensions")) else {
        return Ok(Offered::new());
    };
    let declared: BTreeMap<ExtensionKey, DeclaredExtension> =
        serde_json::from_value(declared.clone())
            .map_err(|error| BindError::Shape(error.to_string()))?;
    let mut offered = Offered::new();
    for (key, route) in declared {
        let document = statement_dir.join(&route.document);
        let index = OperationIndex::load(&document)
            .map_err(|source| BindError::Document { key, source })?;
        let operation =
            index
                .get(&route.operation_id)
                .cloned()
                .ok_or_else(|| BindError::NoOperation {
                    key,
                    document: document.clone(),
                    operation_id: route.operation_id.clone(),
                })?;
        offered.insert(key, operation);
    }
    Ok(offered)
}

/// The skip every workflow of `document` carries whose setups, at any depth
/// through `dependsOn`, call an extension `offered` does not hold, by
/// `workflowId`.
#[must_use]
pub fn not_offered_skips(document: &ArazzoDocument, offered: &Offered) -> BTreeMap<String, Skip> {
    let library = Library::new([document]);
    let mut skips = BTreeMap::new();
    for workflow in &document.description().workflows {
        let missing =
            library.walk(
                &mut Seen::default(),
                document,
                workflow,
                &mut |visit| match called_by(visit.document, visit.workflow) {
                    Some(key) if !offered.contains_key(&key) => {
                        ControlFlow::Break((key, workflow_label(visit.document, visit.workflow)))
                    }
                    _ => ControlFlow::Continue(()),
                },
            );
        if let Some((key, setup)) = missing {
            skips.insert(workflow.workflow_id.clone(), not_offered(key, &setup));
        }
    }
    skips
}

/// The skip of a case whose setup `setup` calls `key`, which the statement
/// does not declare, citing the statement schema's `extensions` clause.
#[must_use]
pub fn not_offered(key: ExtensionKey, setup: &str) -> Skip {
    Skip {
        reason: SkipReason::ExtensionNotOffered,
        why: Why {
            source: STATEMENT_SCHEMA.to_owned(),
            section: EXTENSIONS_SECTION.to_owned(),
            quote: EXTENSIONS_RULE.to_owned(),
            link: None,
            report: None,
        },
        detail: Some(format!(
            "the setup `{setup}` calls the `{key}` extension, which the statement does not declare"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use serde_json::json;
    use std::error::Error as _;

    #[test]
    fn an_extension_token_round_trips_and_an_unknown_one_is_refused() {
        for key in ExtensionKey::ALL {
            assert_eq!(ExtensionKey::parse(key.token()), Ok(*key));
            assert_eq!(
                serde_json::to_value(key).ok(),
                Some(Value::String(key.token().to_owned()))
            );
        }
        assert_eq!(
            ExtensionKey::parse("ehr_import"),
            Err(ExtensionError::Unknown("ehr_import".to_owned()))
        );
        let mut extensions = BTreeMap::new();
        extensions.insert(EXTENSION_KEY.to_owned(), json!(1));
        assert_eq!(
            extension_of(Some(&extensions)),
            Err(ExtensionError::NotAString)
        );
        assert_eq!(extension_of(None), Ok(None));
    }

    #[test]
    fn the_rule_is_quoted_verbatim_from_the_published_statement_schema() {
        assert!(EXTENSIONS_DESCRIPTION.contains(EXTENSIONS_RULE));
        let schema = crate::schema::statement_schema();
        assert_eq!(
            schema
                .pointer("/properties/extensions/description")
                .and_then(Value::as_str),
            Some(EXTENSIONS_DESCRIPTION)
        );
        let skip = not_offered(ExtensionKey::EhrImport, "s/w");
        assert_eq!(skip.reason, SkipReason::ExtensionNotOffered);
        assert_eq!(skip.why.section, EXTENSIONS_SECTION);
    }

    const PARTY_OAS: &str = "openapi: 3.0.3\npaths:\n  /admin/import/{ehr_id}:\n    post:\n      operationId: import_ehr\n";

    #[test]
    fn a_declared_route_binds_to_the_party_operation() {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("party/import.yaml").write_str(PARTY_OAS).unwrap();
        let statement = json!({ "extensions": {
            "ehr-import": { "document": "party/import.yaml", "operation_id": "import_ehr" }
        }});
        let offered = offered(Some(&statement), dir.path()).unwrap();
        let operation = offered.get(&ExtensionKey::EhrImport).unwrap();
        assert_eq!(operation.path, "/admin/import/{ehr_id}");
        assert!(super::offered(None, dir.path()).unwrap().is_empty());
        assert!(
            super::offered(Some(&json!({})), dir.path())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_route_that_does_not_bind_is_refused_with_its_cause() {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("party/import.yaml").write_str(PARTY_OAS).unwrap();
        let unknown = json!({ "extensions": {
            "ehr-export": { "document": "party/import.yaml", "operation_id": "import_ehr" }
        }});
        assert!(matches!(
            offered(Some(&unknown), dir.path()),
            Err(BindError::Shape(_))
        ));
        let missing_operation = json!({ "extensions": {
            "ehr-import": { "document": "party/import.yaml", "operation_id": "nope" }
        }});
        assert!(matches!(
            offered(Some(&missing_operation), dir.path()),
            Err(BindError::NoOperation {
                key: ExtensionKey::EhrImport,
                ..
            })
        ));
        let missing_document = json!({ "extensions": {
            "ehr-import": { "document": "party/absent.yaml", "operation_id": "import_ehr" }
        }});
        let error = offered(Some(&missing_document), dir.path()).unwrap_err();
        assert!(
            error
                .source()
                .and_then(|source| source.downcast_ref::<SourceError>())
                .is_some(),
            "{error}"
        );
    }
}
