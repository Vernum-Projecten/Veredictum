// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Surface coverage for the Arazzo catalogue.
//!
//! Inside the operation families the catalogue has converted, every released
//! operation, every response it declares and every value of its negotiation
//! headers is covered by a step, or is a gap with the issue that tracks it.
//!
//! The surface is enumerated from the decomposed ITS-REST 1.1.0 OAS under
//! `specs/openehr/ITS-REST/specifications/`: each API document's `paths`
//! name the operation files, and each operation file declares its tags, its
//! responses and its parameters. An OAS response list is not exhaustive
//! (OAS 3.0.3 §Responses Object: "The documentation is not necessarily
//! expected to cover all possible HTTP response codes"), so this gate is a
//! floor: a status the docs text assigns is covered by its own case, and is
//! never a gap here.
//!
//! The scope is declared, never inferred: `cases/surface.yaml` names each
//! converted family as an API document and an OAS tag, so an unconverted
//! family cannot fail the build and a converted one cannot quietly shrink.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use roas_arazzo::common::reusable::ReusableOr;
use roas_arazzo::v1_1::{CriterionKind, CriterionType, ParameterLocation, ValueOrSelector};
use serde::Deserialize;
use serde_json::Value;

use crate::arazzo::document::ArazzoDocument;
use crate::arazzo::expr::{CmpOp, Expr, Operand, SimpleCondition};
use crate::arazzo::gates::published::surface_schema;
use crate::arazzo::source::split_qualified;

/// Where the surface file sits, relative to the cases directory.
pub const SURFACE_FILE: &str = "surface.yaml";

/// The request headers whose declared values are negotiation branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum NegotiationHeader {
    /// `Accept`: the response format.
    #[serde(rename = "Accept")]
    Accept,
    /// `Content-Type`: the request format.
    #[serde(rename = "Content-Type")]
    ContentType,
    /// `Prefer`: the representation returned.
    #[serde(rename = "Prefer")]
    Prefer,
}

impl NegotiationHeader {
    /// Every negotiation header, in vocabulary order.
    pub const ALL: &[Self] = &[Self::Accept, Self::ContentType, Self::Prefer];

    /// The field name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Accept => "Accept",
            Self::ContentType => "Content-Type",
            Self::Prefer => "Prefer",
        }
    }

    /// The negotiation header named `name`, compared case-insensitively
    /// (RFC 9110 §5.1: "Field names are case-insensitive").
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|header| header.name().eq_ignore_ascii_case(name))
    }
}

/// One converted family: an ITS-REST API document and an OAS tag.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    /// The API document, `specs/openehr/ITS-REST/specifications/<api>.openapi.yaml`.
    pub api: String,
    /// The OAS tag.
    pub tag: String,
}

/// One uncovered element, with the issue that tracks covering it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gap {
    /// The API document.
    pub api: String,
    /// The `operationId`.
    pub operation: String,
    /// A declared response status, when the gap is a response branch.
    pub status: Option<String>,
    /// A negotiation header, when the gap is a negotiation branch.
    pub header: Option<NegotiationHeader>,
    /// The header value, beside `header`.
    pub value: Option<String>,
    /// The issue in this repository that tracks covering the element.
    pub issue: NonZeroU64,
    /// Why it is not covered yet.
    pub note: Option<String>,
}

impl Gap {
    fn element(&self) -> Option<Element> {
        let api = self.api.clone();
        let operation = self.operation.clone();
        match (&self.status, self.header, &self.value) {
            (None, None, None) => Some(Element::Operation { api, operation }),
            (Some(status), None, None) => Some(Element::Response {
                api,
                operation,
                status: status.clone(),
            }),
            (None, Some(header), Some(value)) => Some(Element::Negotiation {
                api,
                operation,
                header,
                value: value.clone(),
            }),
            _ => None,
        }
    }
}

/// The surface file: what is in scope, and the tracked gaps inside it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceFile {
    /// The converted families.
    pub scope: Vec<Scope>,
    /// The tracked gaps.
    #[serde(default)]
    pub gaps: Vec<Gap>,
}

static VALIDATOR: LazyLock<Result<jsonschema::Validator, String>> =
    LazyLock::new(|| jsonschema::validator_for(&surface_schema()).map_err(|e| e.to_string()));

/// Reads a surface file's parsed YAML through the published schema.
///
/// # Errors
/// One message per schema violation, or the typed-parse failure.
pub fn parse(value: &Value) -> Result<SurfaceFile, Vec<String>> {
    let validator = VALIDATOR
        .as_ref()
        .map_err(|error| vec![format!("the surface schema does not compile: {error}")])?;
    let violations: Vec<String> = validator
        .iter_errors(value)
        .map(|error| format!("#{}: {error}", error.instance_path()))
        .collect();
    if !violations.is_empty() {
        return Err(violations);
    }
    serde_json::from_value(value.clone()).map_err(|error| vec![error.to_string()])
}

/// One element of the released surface.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Element {
    /// The operation is called at all.
    Operation {
        /// The API document.
        api: String,
        /// The `operationId`.
        operation: String,
    },
    /// A response status the operation declares.
    Response {
        /// The API document.
        api: String,
        /// The `operationId`.
        operation: String,
        /// The status key, as the OAS spells it.
        status: String,
    },
    /// A value the operation declares for a negotiation header.
    Negotiation {
        /// The API document.
        api: String,
        /// The `operationId`.
        operation: String,
        /// The header.
        header: NegotiationHeader,
        /// The declared value.
        value: String,
    },
}

impl fmt::Display for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation { api, operation } => write!(f, "{api}/{operation}"),
            Self::Response {
                api,
                operation,
                status,
            } => write!(f, "{api}/{operation} response {status}"),
            Self::Negotiation {
                api,
                operation,
                header,
                value,
            } => write!(f, "{api}/{operation} {}: {value}", header.name()),
        }
    }
}

/// Why the released surface cannot be enumerated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SurfaceError {
    /// A file of the decomposed OAS does not load.
    #[error("`{file}`: {reason}")]
    Unreadable {
        /// The file.
        file: PathBuf,
        /// The verbatim failure.
        reason: String,
    },
    /// A scope entry names no API document.
    #[error("scope `{api}`: no `{api}.openapi.yaml` under the decomposed ITS-REST OAS")]
    NoApi {
        /// The API name.
        api: String,
    },
    /// A scope entry's tag tags no operation of its API document.
    #[error("scope `{api}`/`{tag}`: no operation of `{api}` carries the tag")]
    EmptyScope {
        /// The API name.
        api: String,
        /// The tag.
        tag: String,
    },
    /// An operation file without an `operationId`.
    #[error("`{0}` declares no operationId")]
    NoOperationId(PathBuf),
}

/// Enumerates the released surface of `scope` from the decomposed OAS under
/// `its_rest` (`specs/openehr/ITS-REST/specifications`).
///
/// # Errors
/// The [`SurfaceError`]s of the scope entries that do not enumerate; the
/// entries that do still contribute their elements.
#[must_use]
pub fn enumerate(its_rest: &Path, scope: &[Scope]) -> (BTreeSet<Element>, Vec<SurfaceError>) {
    let mut elements = BTreeSet::new();
    let mut errors = Vec::new();
    for entry in scope {
        match enumerate_one(its_rest, entry) {
            Ok(found) if found.is_empty() => errors.push(SurfaceError::EmptyScope {
                api: entry.api.clone(),
                tag: entry.tag.clone(),
            }),
            Ok(found) => elements.extend(found),
            Err(error) => errors.push(error),
        }
    }
    (elements, errors)
}

/// The OAS 3.0.3 §Path Item Object fields that hold an operation.
const OPERATION_KEYS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

fn load(file: &Path) -> Result<Value, SurfaceError> {
    crate::load::yaml_file_to_value(file).map_err(|error| SurfaceError::Unreadable {
        file: file.to_owned(),
        reason: error.detail(),
    })
}

fn enumerate_one(its_rest: &Path, scope: &Scope) -> Result<BTreeSet<Element>, SurfaceError> {
    let api_file = its_rest.join(format!("{}.openapi.yaml", scope.api));
    if !api_file.is_file() {
        return Err(SurfaceError::NoApi {
            api: scope.api.clone(),
        });
    }
    let api = load(&api_file)?;
    let mut elements = BTreeSet::new();
    let paths = api.get("paths").and_then(Value::as_object);
    for item in paths.into_iter().flat_map(|paths| paths.values()) {
        let Some(item) = item.as_object() else {
            continue;
        };
        for (method, operation) in item {
            if !OPERATION_KEYS.contains(&method.as_str()) {
                continue;
            }
            let (operation, op_file) = match operation.get("$ref").and_then(Value::as_str) {
                Some(reference) => {
                    let op_file = its_rest.join(reference);
                    (load(&op_file)?, op_file)
                }
                None => (operation.clone(), api_file.clone()),
            };
            let tagged = operation
                .get("tags")
                .and_then(Value::as_array)
                .is_some_and(|tags| tags.iter().any(|t| t.as_str() == Some(scope.tag.as_str())));
            if !tagged {
                continue;
            }
            let Some(operation_id) = operation.get("operationId").and_then(Value::as_str) else {
                return Err(SurfaceError::NoOperationId(op_file));
            };
            let base = op_file.parent().unwrap_or(its_rest);
            elements.extend(operation_elements(
                &scope.api,
                operation_id,
                &operation,
                base,
            )?);
        }
    }
    Ok(elements)
}

fn operation_elements(
    api: &str,
    operation_id: &str,
    operation: &Value,
    base: &Path,
) -> Result<Vec<Element>, SurfaceError> {
    let api = api.to_owned();
    let operation_id = operation_id.to_owned();
    let mut out = vec![Element::Operation {
        api: api.clone(),
        operation: operation_id.clone(),
    }];
    for status in operation
        .get("responses")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|responses| responses.keys())
    {
        out.push(Element::Response {
            api: api.clone(),
            operation: operation_id.clone(),
            status: status.clone(),
        });
    }
    for parameter in operation
        .get("parameters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let parameter = match parameter.get("$ref").and_then(Value::as_str) {
            Some(reference) => load(&base.join(reference))?,
            None => parameter.clone(),
        };
        if parameter.get("in").and_then(Value::as_str) != Some("header") {
            continue;
        }
        let Some(header) = parameter
            .get("name")
            .and_then(Value::as_str)
            .and_then(NegotiationHeader::parse)
        else {
            continue;
        };
        // NOTE: OAS 3.0.3 §Parameter Object: a header without a `schema.enum` declares no
        // closed value set, so it adds no negotiation branch.
        for value in parameter
            .pointer("/schema/enum")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            out.push(Element::Negotiation {
                api: api.clone(),
                operation: operation_id.clone(),
                header,
                value: value.to_owned(),
            });
        }
    }
    Ok(out)
}

/// The released API a source description names, when it is one of the
/// released bundles: `specs/rest-oas/<api>-<variant>.openapi.yaml`.
#[must_use]
pub fn released_api(url: &str) -> Option<&str> {
    let (_, file) = url.rsplit_once("specs/rest-oas/")?;
    let stem = file.strip_suffix(".openapi.yaml")?;
    stem.rsplit_once('-').map(|(api, _variant)| api)
}

/// Every element the documents' steps cover.
#[must_use]
pub fn covered<'a>(documents: impl IntoIterator<Item = &'a ArazzoDocument>) -> BTreeSet<Element> {
    let mut out = BTreeSet::new();
    for document in documents {
        let apis: BTreeMap<&str, &str> = document
            .description()
            .source_descriptions
            .iter()
            .filter_map(|source| Some((source.name.as_str(), released_api(&source.url)?)))
            .collect();
        for step in document
            .description()
            .workflows
            .iter()
            .flat_map(|workflow| &workflow.steps)
        {
            let Some((source, operation)) = step.operation_id.as_deref().and_then(split_qualified)
            else {
                continue;
            };
            let Some(api) = apis.get(source) else {
                continue;
            };
            let (api, operation) = ((*api).to_owned(), operation.to_owned());
            out.insert(Element::Operation {
                api: api.clone(),
                operation: operation.clone(),
            });
            for criterion in &step.success_criteria {
                let simple = matches!(
                    criterion.type_,
                    None | Some(CriterionType::Simple(CriterionKind::Simple))
                );
                if let (true, Ok(condition)) =
                    (simple, SimpleCondition::parse(&criterion.condition))
                    && let SimpleCondition {
                        left: Operand::Expr(Expr::StatusCode),
                        op: CmpOp::Eq,
                        right: Operand::Integer(status),
                    } = condition
                {
                    out.insert(Element::Response {
                        api: api.clone(),
                        operation: operation.clone(),
                        status: status.to_string(),
                    });
                }
            }
            let mut sent: Vec<(NegotiationHeader, String)> = Vec::new();
            for parameter in &step.parameters {
                if let ReusableOr::Item(parameter) = parameter
                    && parameter.in_ == Some(ParameterLocation::Header)
                    && let Some(header) = NegotiationHeader::parse(&parameter.name)
                    && let ValueOrSelector::Literal(Value::String(value)) = &parameter.value
                {
                    sent.push((header, value.clone()));
                }
            }
            if let Some(content_type) = step
                .request_body
                .as_ref()
                .and_then(|body| body.content_type.clone())
            {
                sent.push((NegotiationHeader::ContentType, content_type));
            }
            for (header, value) in sent {
                out.insert(Element::Negotiation {
                    api: api.clone(),
                    operation: operation.clone(),
                    header,
                    value,
                });
            }
        }
    }
    out
}

/// Compares the surface with what is covered and with the gaps.
///
/// Returns one message per uncovered element without a gap, per gap whose
/// element is covered, per gap outside the surface, and per gap listed twice.
#[must_use]
pub fn judge(
    surface: &BTreeSet<Element>,
    covered: &BTreeSet<Element>,
    gaps: &[Gap],
) -> Vec<String> {
    let mut out = Vec::new();
    let mut gapped = BTreeSet::new();
    for (index, gap) in gaps.iter().enumerate() {
        let Some(element) = gap.element() else {
            out.push(format!(
                "gaps[{index}]: name a status, a header with its value, or neither"
            ));
            continue;
        };
        if !surface.contains(&element) {
            out.push(format!(
                "gaps[{index}]: `{element}` is not an element of the scoped surface"
            ));
        } else if covered.contains(&element) {
            out.push(format!(
                "gaps[{index}]: `{element}` is covered, so the gap (#{}) is closed and must go",
                gap.issue
            ));
        }
        if !gapped.insert(element.clone()) {
            out.push(format!("gaps[{index}]: `{element}` is listed twice"));
        }
    }
    for element in surface {
        if !covered.contains(element) && !gapped.contains(element) {
            out.push(format!(
                "`{element}` has no covering step and no tracked gap"
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn its_rest() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../specs/openehr/ITS-REST/specifications")
    }

    fn ehr_scope() -> Vec<Scope> {
        vec![Scope {
            api: "ehr".to_owned(),
            tag: "EHR".to_owned(),
        }]
    }

    fn op(operation: &str) -> Element {
        Element::Operation {
            api: "ehr".to_owned(),
            operation: operation.to_owned(),
        }
    }

    fn response(operation: &str, status: &str) -> Element {
        Element::Response {
            api: "ehr".to_owned(),
            operation: operation.to_owned(),
            status: status.to_owned(),
        }
    }

    #[test]
    fn the_ehr_family_enumerates_its_four_operations_and_their_branches() {
        let (surface, errors) = enumerate(&its_rest(), &ehr_scope());
        assert_eq!(errors, Vec::new());
        let operations: Vec<String> = surface
            .iter()
            .filter_map(|e| match e {
                Element::Operation { operation, .. } => Some(operation.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            operations,
            [
                "ehr_create",
                "ehr_create_with_id",
                "ehr_get_by_id",
                "ehr_get_by_subject"
            ]
        );
        assert!(surface.contains(&response("ehr_create_with_id", "409")));
        assert!(surface.contains(&Element::Negotiation {
            api: "ehr".to_owned(),
            operation: "ehr_get_by_id".to_owned(),
            header: NegotiationHeader::Accept,
            value: "application/xml".to_owned(),
        }));
        assert!(
            !surface.contains(&op("admin_ehr_delete")),
            "the admin API's EHR-tagged operations are a different scope"
        );
    }

    #[test]
    fn a_seeded_scope_that_names_nothing_is_refused() {
        let (_, errors) = enumerate(
            &its_rest(),
            &[
                Scope {
                    api: "ehrs".to_owned(),
                    tag: "EHR".to_owned(),
                },
                Scope {
                    api: "ehr".to_owned(),
                    tag: "EHRS".to_owned(),
                },
            ],
        );
        assert!(matches!(
            errors.as_slice(),
            [SurfaceError::NoApi { .. }, SurfaceError::EmptyScope { .. }]
        ));
    }

    fn gap(operation: &str, status: Option<&str>) -> Gap {
        Gap {
            api: "ehr".to_owned(),
            operation: operation.to_owned(),
            status: status.map(str::to_owned),
            header: None,
            value: None,
            issue: NonZeroU64::MIN,
            note: None,
        }
    }

    #[test]
    fn an_uncovered_branch_needs_a_gap_and_a_covered_one_refuses_its_gap() {
        let surface: BTreeSet<Element> = [op("a"), response("a", "200"), response("a", "404")]
            .into_iter()
            .collect();
        let covered: BTreeSet<Element> = [op("a"), response("a", "200")].into_iter().collect();
        let seeded = judge(&surface, &covered, &[]);
        assert_eq!(
            seeded,
            ["`ehr/a response 404` has no covering step and no tracked gap"]
        );
        assert_eq!(
            judge(&surface, &covered, &[gap("a", Some("404"))]),
            Vec::<String>::new()
        );
        let stale = judge(
            &surface,
            &covered,
            &[gap("a", Some("404")), gap("a", Some("200"))],
        );
        assert_eq!(stale.len(), 1, "{stale:?}");
        assert!(stale.iter().all(|m| m.contains("is covered")), "{stale:?}");
        let outside = judge(&surface, &covered, &[gap("a", Some("404")), gap("b", None)]);
        assert!(
            outside.iter().any(|m| m.contains("not an element")),
            "{outside:?}"
        );
        let twice = judge(
            &surface,
            &covered,
            &[gap("a", Some("404")), gap("a", Some("404"))],
        );
        assert!(
            twice.iter().any(|m| m.contains("listed twice")),
            "{twice:?}"
        );
    }

    #[test]
    fn seeded_surface_files_are_refused() {
        for seeded in [
            json!({ "gaps": [] }),
            json!({ "scope": [] }),
            json!({ "scope": [{ "api": "ehr", "tag": "EHR" }], "gaps": [{ "api": "ehr", "operation": "a" }] }),
            json!({ "scope": [{ "api": "ehr", "tag": "EHR" }], "gaps": [{ "api": "ehr", "operation": "a", "issue": 1, "header": "Accept" }] }),
            json!({ "scope": [{ "api": "ehr", "tag": "EHR" }], "gaps": [{ "api": "ehr", "operation": "a", "issue": 1, "header": "If-Match", "value": "x" }] }),
            json!({ "scope": [{ "api": "ehr", "tag": "EHR" }], "gaps": [{ "api": "ehr", "operation": "a", "issue": 1, "status": "404", "header": "Accept", "value": "x" }] }),
        ] {
            assert!(parse(&seeded).is_err(), "{seeded}");
        }
        let good = parse(&json!({
            "scope": [{ "api": "ehr", "tag": "EHR" }],
            "gaps": [{ "api": "ehr", "operation": "a", "issue": 603, "header": "Accept", "value": "application/xml" }]
        }))
        .unwrap();
        assert_eq!(good.gaps.len(), 1);
    }

    #[test]
    fn only_released_bundles_name_an_api() {
        assert_eq!(
            released_api("../../../specs/rest-oas/ehr-validation.openapi.yaml"),
            Some("ehr")
        );
        assert_eq!(
            released_api("../sources/veredictum-extensions.openapi.yaml"),
            None
        );
        assert_eq!(released_api("specs/rest-oas/ehr.yaml"), None);
    }
}
