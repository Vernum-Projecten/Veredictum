// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Response validation against the `-validation` OpenAPI 3.0.3 sources.
//!
//! A response is validated only where a case asks for it, through the
//! `schema` judge of `x-veredictum-check` on a criterion with its own
//! citation: an OAS schema never outranks the docs text or the RM by default
//! (hard rule 1 puts the OAS below both). The response is validated against
//! the schema its operation declares for the served status and media type.
//! Only JSON bodies are validated (`application/json` or a `+json` suffix,
//! RFC 6839 §3.1). An OAS 3.0.3 Schema Object is a
//! subset of JSON Schema Wright draft 00 with its own adjustments (OAS 3.0.3
//! §Schema Object), so each schema is translated into JSON Schema 2020-12
//! before it is compiled. The supported subset:
//!
//! - kept as they are: `title`, `description`, `type`, `enum`, `required`,
//!   `default`, `format`, `pattern`, `multipleOf`, `minimum`, `maximum`,
//!   `minLength`, `maxLength`, `minItems`, `maxItems`, `uniqueItems`,
//!   `minProperties`, `maxProperties`;
//! - translated recursively: `properties`, `additionalProperties`, `items`,
//!   `allOf`, `oneOf`, `anyOf`, `not`;
//! - `exclusiveMinimum` and `exclusiveMaximum` as booleans become the 2020-12
//!   numeric form over `minimum` and `maximum`;
//! - `nullable: true` adds `"null"` to an explicit `type` (§Fixed Fields,
//!   `nullable`);
//! - a Reference Object to `#/components/schemas/<name>` becomes a `$ref` to
//!   `$defs`, and its siblings are dropped, because "any properties added
//!   SHALL be ignored" (§Reference Object);
//! - `discriminator` is dropped: a `oneOf` payload "MUST, by validation,
//!   match exactly one of the schemas", and the discriminator "MAY act as a
//!   hint" (§Discriminator Object), so `oneOf` alone decides;
//! - the documentation fields `example`, `externalDocs`, `xml`, `readOnly`,
//!   `writeOnly`, `deprecated` and every `x-` extension are dropped.
//!
//! Any other keyword, and a reference to anything but a local schema, is an
//! [`OasError`]: the schema is not silently narrowed. `format` is an
//! annotation only, since OAS 3.0.3 §Data Types makes it "an open string-valued
//! property" and the bundles use values JSON Schema does not define (`url`,
//! `datetime`). A body is matched to a schema by the media type its
//! `Content-Type` names, exactly; the response is found by its status code,
//! then its `1XX`…`5XX` range, then `default` (§Responses Object).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::arazzo::cite::Citation;
use crate::arazzo::document::ArazzoDocument;
use crate::arazzo::source::split_qualified;

/// Why a source's schemas cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OasError {
    /// The source file does not load.
    #[error("source `{path}`: {reason}")]
    Load {
        /// The file.
        path: PathBuf,
        /// The verbatim failure.
        reason: String,
    },
    /// A keyword outside the supported OAS 3.0.3 subset.
    #[error("`{pointer}` uses `{keyword}`, which the OAS 3.0.3 translation does not support")]
    UnsupportedKeyword {
        /// Where the keyword is.
        pointer: String,
        /// The keyword.
        keyword: String,
    },
    /// A reference to anything but a local component.
    #[error("`{pointer}` references `{target}`, which is not a local component")]
    UnsupportedRef {
        /// Where the reference is.
        pointer: String,
        /// The reference.
        target: String,
    },
    /// A value is not the shape OAS 3.0.3 gives it.
    #[error("`{0}` is not a well-formed OAS 3.0.3 object")]
    Malformed(String),
    /// The translated schema does not compile.
    #[error("the schema at `{pointer}` does not compile: {reason}")]
    Compile {
        /// The response's media-type schema.
        pointer: String,
        /// The verbatim failure.
        reason: String,
    },
}

/// What validating one response found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaVerdict {
    /// The operation declares no schema for this status and media type, or
    /// the response has no body.
    NotDeclared,
    /// The body validates.
    Valid,
    /// The body does not validate.
    Invalid {
        /// The response object, cited by its description.
        citation: Citation,
        /// The media-type schema the body was validated against.
        pointer: String,
        /// Every violation, `instance path: message`, in the validator's order.
        errors: Vec<String>,
    },
}

/// One source's raw OAS and its translated component schemas.
#[derive(Debug)]
struct Source {
    file: String,
    raw: Value,
    defs: Value,
    operations: BTreeMap<String, String>,
}

/// The response schemas of every source a document names, compiled on first
/// use.
#[derive(Debug)]
pub struct ResponseSchemas {
    sources: BTreeMap<String, Source>,
    compiled: BTreeMap<String, jsonschema::Validator>,
}

impl ResponseSchemas {
    /// Loads every source `document` names. `repo_root` is the root a cited
    /// source path is written relative to.
    ///
    /// # Errors
    /// [`OasError`] when a source does not load or a component schema falls
    /// outside the supported subset.
    pub fn load(document: &ArazzoDocument, repo_root: &Path) -> Result<Self, OasError> {
        let base = document.path().parent().unwrap_or_else(|| Path::new("."));
        let mut sources = BTreeMap::new();
        for source in &document.description().source_descriptions {
            let path = base.join(&source.url);
            sources.insert(source.name.clone(), Source::load(&path, repo_root)?);
        }
        Ok(Self {
            sources,
            compiled: BTreeMap::new(),
        })
    }

    /// Validates one response of the operation `qualified`
    /// (`$sourceDescriptions.<name>.<operationId>`).
    ///
    /// # Errors
    /// [`OasError`] when the declared schema cannot be translated or compiled.
    pub fn validate(
        &mut self,
        qualified: &str,
        status: u16,
        content_type: Option<&str>,
        body: Option<&Value>,
    ) -> Result<SchemaVerdict, OasError> {
        let (Some(body), Some(content_type)) = (body, content_type) else {
            return Ok(SchemaVerdict::NotDeclared);
        };
        let Some((name, operation_id)) = split_qualified(qualified) else {
            return Ok(SchemaVerdict::NotDeclared);
        };
        let Some(source) = self.sources.get(name) else {
            return Ok(SchemaVerdict::NotDeclared);
        };
        let Some(found) = source.response_schema(operation_id, status, content_type)? else {
            return Ok(SchemaVerdict::NotDeclared);
        };
        let key = format!("{name}#{}", found.schema_pointer);
        if !self.compiled.contains_key(&key) {
            let schema = source.schema_document(&found.schema_pointer)?;
            let validator = jsonschema::options()
                .with_draft(jsonschema::Draft::Draft202012)
                .should_validate_formats(false)
                .build(&schema)
                .map_err(|error| OasError::Compile {
                    pointer: found.schema_pointer.clone(),
                    reason: error.to_string(),
                })?;
            self.compiled.insert(key.clone(), validator);
        }
        let Some(validator) = self.compiled.get(&key) else {
            return Err(OasError::Malformed(key));
        };
        let errors: Vec<String> = validator
            .iter_errors(body)
            .map(|e| {
                let path = e.instance_path().to_string();
                let at = if path.is_empty() { "/" } else { path.as_str() };
                clip(&format!("{at}: {e}"))
            })
            .collect();
        if errors.is_empty() {
            return Ok(SchemaVerdict::Valid);
        }
        Ok(SchemaVerdict::Invalid {
            citation: Citation {
                source: source.file.clone(),
                section: found.response_pointer,
                quote: found.quote,
                report: None,
                level: crate::arazzo::cite::Level::Must,
            },
            pointer: found.schema_pointer,
            errors,
        })
    }
}

/// Whether a `Content-Type` names JSON: `application/json` or a `+json`
/// structured syntax suffix (RFC 6839 §3.1).
#[must_use]
pub fn is_json_media(content_type: &str) -> bool {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence == "application/json" || essence.ends_with("+json")
}

/// How many characters of one violation an explanation keeps; a `oneOf`
/// violation quotes the whole instance.
const VIOLATION_CHARS: usize = 300;

/// One violation, clipped to [`VIOLATION_CHARS`] characters.
fn clip(text: &str) -> String {
    if text.chars().count() <= VIOLATION_CHARS {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(VIOLATION_CHARS).collect();
    out.push('…');
    out
}

/// The schema found for one response.
struct Found {
    response_pointer: String,
    schema_pointer: String,
    quote: String,
}

/// Escapes one JSON Pointer reference token (RFC 6901 §3).
fn escape_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// The HTTP methods a path item may carry (OAS 3.0.3 §Path Item Object).
const METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

impl Source {
    fn load(path: &Path, repo_root: &Path) -> Result<Self, OasError> {
        let raw = crate::load::yaml_file_to_value(path).map_err(|error| OasError::Load {
            path: path.to_owned(),
            reason: error.detail(),
        })?;
        let file = cited_path(path, repo_root);
        let mut defs = Map::new();
        if let Some(Value::Object(schemas)) = raw.pointer("/components/schemas") {
            for (name, schema) in schemas {
                let at = format!("/components/schemas/{}", escape_token(name));
                defs.insert(name.clone(), translate(schema, &at)?);
            }
        }
        let mut operations = BTreeMap::new();
        if let Some(Value::Object(paths)) = raw.get("paths") {
            for (route, item) in paths {
                for method in METHODS {
                    if let Some(id) = item
                        .get(method)
                        .and_then(|op| op.get("operationId"))
                        .and_then(Value::as_str)
                    {
                        operations.insert(
                            id.to_owned(),
                            format!("/paths/{}/{method}", escape_token(route)),
                        );
                    }
                }
            }
        }
        Ok(Self {
            file,
            raw,
            defs: Value::Object(defs),
            operations,
        })
    }

    /// Follows a local `$ref` once; returns the object and its pointer.
    fn follow<'a>(&'a self, value: &'a Value, at: String) -> Result<(&'a Value, String), OasError> {
        match value.get("$ref").and_then(Value::as_str) {
            None => Ok((value, at)),
            Some(target) => {
                let Some(pointer) = target.strip_prefix('#') else {
                    return Err(OasError::UnsupportedRef {
                        pointer: at,
                        target: target.to_owned(),
                    });
                };
                let resolved = self
                    .raw
                    .pointer(pointer)
                    .ok_or_else(|| OasError::Malformed(target.to_owned()))?;
                Ok((resolved, pointer.to_owned()))
            }
        }
    }

    fn response_schema(
        &self,
        operation_id: &str,
        status: u16,
        content_type: &str,
    ) -> Result<Option<Found>, OasError> {
        let Some(operation_pointer) = self.operations.get(operation_id) else {
            return Ok(None);
        };
        let Some(responses) = self
            .raw
            .pointer(&format!("{operation_pointer}/responses"))
            .and_then(Value::as_object)
        else {
            return Ok(None);
        };
        let exact = status.to_string();
        let range = format!("{}XX", exact.chars().next().unwrap_or('0'));
        let Some((key, response)) = [exact.as_str(), range.as_str(), "default"]
            .into_iter()
            .find_map(|key| responses.get(key).map(|r| (key, r)))
        else {
            return Ok(None);
        };
        let (response, response_pointer) = self.follow(
            response,
            format!("{operation_pointer}/responses/{}", escape_token(key)),
        )?;
        let essence = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let Some(content) = response.get("content").and_then(Value::as_object) else {
            return Ok(None);
        };
        let Some((media, entry)) = content
            .iter()
            .find(|(media, _)| media.to_ascii_lowercase() == essence)
        else {
            return Ok(None);
        };
        if entry.get("schema").is_none() {
            return Ok(None);
        }
        let quote = response
            .get("description")
            .and_then(Value::as_str)
            .and_then(|text| text.lines().map(str::trim).find(|l| !l.is_empty()))
            .unwrap_or_default()
            .to_owned();
        Ok(Some(Found {
            schema_pointer: format!("{response_pointer}/content/{}/schema", escape_token(media)),
            response_pointer,
            quote,
        }))
    }

    /// The JSON Schema 2020-12 document for the schema at `pointer`.
    fn schema_document(&self, pointer: &str) -> Result<Value, OasError> {
        let schema = self
            .raw
            .pointer(pointer)
            .ok_or_else(|| OasError::Malformed(pointer.to_owned()))?;
        Ok(json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$defs": self.defs,
            "allOf": [translate(schema, pointer)?]
        }))
    }
}

/// The source's path as a citation names it: relative to the repository
/// root, with `..` segments resolved.
fn cited_path(path: &Path, repo_root: &Path) -> String {
    let normal = |p: &Path| {
        let mut out = PathBuf::new();
        for component in p.components() {
            match component {
                std::path::Component::ParentDir => {
                    out.pop();
                }
                std::path::Component::CurDir => {}
                other => out.push(other),
            }
        }
        out
    };
    let path = normal(path);
    let root = normal(repo_root);
    path.strip_prefix(&root)
        .unwrap_or(&path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The OAS keywords kept verbatim.
const KEPT: [&str; 18] = [
    "title",
    "description",
    "type",
    "enum",
    "required",
    "default",
    "format",
    "pattern",
    "multipleOf",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "minItems",
    "maxItems",
    "uniqueItems",
    "minProperties",
    "maxProperties",
];

/// The OAS documentation fields dropped from validation.
const DROPPED: [&str; 7] = [
    "discriminator",
    "example",
    "externalDocs",
    "xml",
    "readOnly",
    "writeOnly",
    "deprecated",
];

/// Translates one OAS 3.0.3 Schema Object into JSON Schema 2020-12.
///
/// # Errors
/// [`OasError`] for a keyword or reference outside the supported subset.
pub fn translate(schema: &Value, at: &str) -> Result<Value, OasError> {
    let Value::Object(object) = schema else {
        return match schema {
            Value::Bool(_) => Ok(schema.clone()),
            _ => Err(OasError::Malformed(at.to_owned())),
        };
    };
    if let Some(target) = object.get("$ref") {
        let target = target
            .as_str()
            .ok_or_else(|| OasError::Malformed(at.to_owned()))?;
        let Some(name) = target.strip_prefix("#/components/schemas/") else {
            return Err(OasError::UnsupportedRef {
                pointer: at.to_owned(),
                target: target.to_owned(),
            });
        };
        return Ok(json!({ "$ref": format!("#/$defs/{name}") }));
    }
    let mut out = Map::new();
    for (key, value) in object {
        let child = |name: &str| format!("{at}/{}", escape_token(name));
        match key.as_str() {
            k if KEPT.contains(&k) => {
                out.insert(key.clone(), value.clone());
            }
            k if DROPPED.contains(&k) || k.starts_with("x-") || k == "nullable" => {}
            "items" | "not" | "additionalProperties" => {
                out.insert(key.clone(), translate(value, &child(key))?);
            }
            "allOf" | "oneOf" | "anyOf" => {
                let Value::Array(items) = value else {
                    return Err(OasError::Malformed(child(key)));
                };
                let translated = items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| translate(item, &format!("{}/{i}", child(key))))
                    .collect::<Result<Vec<_>, _>>()?;
                out.insert(key.clone(), Value::Array(translated));
            }
            "properties" => {
                let Value::Object(properties) = value else {
                    return Err(OasError::Malformed(child(key)));
                };
                let mut translated = Map::new();
                for (name, property) in properties {
                    translated.insert(
                        name.clone(),
                        translate(property, &format!("{}/{}", child(key), escape_token(name)))?,
                    );
                }
                out.insert(key.clone(), Value::Object(translated));
            }
            "exclusiveMinimum" | "exclusiveMaximum" => {}
            other => {
                return Err(OasError::UnsupportedKeyword {
                    pointer: at.to_owned(),
                    keyword: other.to_owned(),
                });
            }
        }
    }
    for (flag, bound) in [
        ("exclusiveMinimum", "minimum"),
        ("exclusiveMaximum", "maximum"),
    ] {
        match object.get(flag) {
            None | Some(Value::Bool(false)) => {}
            Some(Value::Bool(true)) => {
                let Some(limit) = out.remove(bound) else {
                    return Err(OasError::Malformed(format!("{at}/{flag}")));
                };
                out.insert(flag.to_owned(), limit);
            }
            Some(_) => return Err(OasError::Malformed(format!("{at}/{flag}"))),
        }
    }
    if object.get("nullable") == Some(&Value::Bool(true))
        && let Some(Value::String(kind)) = out.get("type").cloned()
    {
        out.insert("type".to_owned(), json!([kind, "null"]));
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;

    #[test]
    fn a_reference_drops_its_siblings_and_points_into_defs() {
        let schema = json!({ "$ref": "#/components/schemas/Ehr", "description": "ignored" });
        assert_eq!(
            translate(&schema, "#").unwrap(),
            json!({ "$ref": "#/$defs/Ehr" })
        );
        assert!(matches!(
            translate(&json!({ "$ref": "other.yaml#/X" }), "#"),
            Err(OasError::UnsupportedRef { .. })
        ));
    }

    #[test]
    fn nullable_exclusive_bounds_and_annotations_translate() {
        let schema = json!({
            "type": "integer", "nullable": true, "minimum": 0, "exclusiveMinimum": true,
            "maximum": 9, "exclusiveMaximum": false, "example": 3, "x-tag": 1,
            "discriminator": { "propertyName": "_type" }
        });
        assert_eq!(
            translate(&schema, "#").unwrap(),
            json!({ "type": ["integer", "null"], "exclusiveMinimum": 0, "maximum": 9 })
        );
    }

    #[test]
    fn a_keyword_outside_the_subset_is_refused() {
        assert!(matches!(
            translate(&json!({ "type": "object", "patternProperties": {} }), "#/s"),
            Err(OasError::UnsupportedKeyword { keyword, .. }) if keyword == "patternProperties"
        ));
        assert!(matches!(
            translate(&json!({ "properties": { "type": { "const": 1 } } }), "#/s"),
            Err(OasError::UnsupportedKeyword { keyword, .. }) if keyword == "const"
        ));
    }

    const OAS: &str = "openapi: 3.0.3
paths:
  /ehr/{ehr_id}:
    get:
      operationId: ehr_get_by_id
      responses:
        '200': { $ref: '#/components/responses/200_EHR' }
        4XX: { description: Any client error. }
components:
  responses:
    200_EHR:
      description: |
        `200 OK` is returned when the EHR is retrieved.
      content:
        application/json:
          schema: { $ref: '#/components/schemas/Ehr' }
  schemas:
    Ehr:
      type: object
      required: [ehr_id]
      properties:
        ehr_id: { $ref: '#/components/schemas/Id' }
    Id:
      type: object
      required: [value]
      properties: { value: { type: string, format: uuid } }
";

    fn schemas() -> (assert_fs::TempDir, ResponseSchemas) {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("specs/rest-oas/ehr.yaml").write_str(OAS).unwrap();
        let doc = "arazzo: 1.1.0\ninfo: { title: t, version: '1' }\nsourceDescriptions:\n  - { name: ehr, url: ../../specs/rest-oas/ehr.yaml, type: openapi }\nworkflows: []\n";
        dir.child("catalogue/cases/doc.yaml")
            .write_str(doc)
            .unwrap();
        let document = ArazzoDocument::load(&dir.path().join("catalogue/cases/doc.yaml")).unwrap();
        let schemas = ResponseSchemas::load(&document, dir.path()).unwrap();
        (dir, schemas)
    }

    const OP: &str = "$sourceDescriptions.ehr.ehr_get_by_id";

    #[test]
    fn a_declared_body_validates_or_names_its_violations_and_citation() {
        let (_dir, mut schemas) = schemas();
        let good = json!({ "ehr_id": { "value": "not-a-uuid-but-format-is-annotation" } });
        assert_eq!(
            schemas
                .validate(
                    OP,
                    200,
                    Some("application/json; charset=utf-8"),
                    Some(&good)
                )
                .unwrap(),
            SchemaVerdict::Valid
        );
        let bad = json!({ "ehr_id": { "id": 1 } });
        let SchemaVerdict::Invalid {
            citation,
            pointer,
            errors,
        } = schemas
            .validate(OP, 200, Some("application/json"), Some(&bad))
            .unwrap()
        else {
            panic!("the body lacks ehr_id.value");
        };
        assert_eq!(citation.source, "specs/rest-oas/ehr.yaml");
        assert_eq!(citation.section, "/components/responses/200_EHR");
        assert_eq!(
            citation.quote,
            "`200 OK` is returned when the EHR is retrieved."
        );
        assert_eq!(
            pointer,
            "/components/responses/200_EHR/content/application~1json/schema"
        );
        assert!(errors.iter().any(|e| e.contains("value")), "{errors:?}");
    }

    #[test]
    fn an_undeclared_status_media_type_or_empty_body_is_not_validated() {
        let (_dir, mut schemas) = schemas();
        let body = json!({});
        for (status, media) in [
            (404, Some("application/json")),
            (200, Some("application/xml")),
        ] {
            assert_eq!(
                schemas.validate(OP, status, media, Some(&body)).unwrap(),
                SchemaVerdict::NotDeclared
            );
        }
        assert_eq!(
            schemas.validate(OP, 200, None, Some(&body)).unwrap(),
            SchemaVerdict::NotDeclared
        );
        assert_eq!(
            schemas
                .validate(OP, 200, Some("application/json"), None)
                .unwrap(),
            SchemaVerdict::NotDeclared
        );
    }

    #[test]
    fn every_released_validation_bundle_translates() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for group in [
            "admin",
            "definition",
            "demographic",
            "ehr",
            "overview",
            "query",
            "system",
        ] {
            let path = root.join(format!("specs/rest-oas/{group}-validation.openapi.yaml"));
            Source::load(&path, &root).unwrap_or_else(|e| panic!("{group}: {e}"));
        }
    }
}
