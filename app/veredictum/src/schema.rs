// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Deterministic JSON-Schema emission for every document the instrument
//! reads or writes.
//!
//! The party documents, the measured schedule and its records, the bench
//! documents, and the Arazzo catalogue's own formats.
//!
//! The published schema files are the norm; this module is their single
//! source. Emission is byte-deterministic: fixed insertion order (via
//! `serde_json`'s `preserve_order`), two-space pretty printing, trailing
//! newline. Every closed vocabulary in a schema derives from the compiled
//! enums in [`crate::vocab`], so schema and reference implementation cannot
//! drift. Draft: JSON Schema 2020-12. `$id`s are versioned URNs pending the
//! upstream repository's canonical URLs.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use serde::Serialize;
use serde_json::{Value, json};

use crate::party::{
    IdentitySource, InstrumentStatus, OutcomeStatus, SelectionBasis, TechProfileSource,
    VerificationPackStatus,
};
use crate::vocab::{FormatName, ItsName, SpecComponent, Tier};

/// The schema version stamped into every `$id` (bumps with the schedule
/// release, independently of the product version).
pub const SCHEMA_VERSION: &str = "0.1.0";

const DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Serde token of one enum variant.
fn token<T: Serialize>(v: &T) -> Value {
    match serde_json::to_value(v) {
        Ok(Value::String(s)) => Value::String(s),
        // Unreachable for the vocab enums (all serialize to strings); a
        // non-string token would be a defect in this module, surfaced by the
        // emission tests, never silently emitted.
        _ => Value::Null,
    }
}

/// Serde tokens of a whole vocabulary.
fn tokens<T: Serialize>(all: &[T]) -> Value {
    Value::Array(all.iter().map(token).collect())
}

/// The serde token of one enum variant, as an owned `String` (for building
/// property-name maps). Non-string tokens are a defect surfaced by the
/// emission tests, never silently emitted.
fn token_str<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(Value::String(s)) => s,
        _ => String::new(),
    }
}

fn urn(name: &str) -> String {
    format!("urn:openehr:cnf:schema:{name}:{SCHEMA_VERSION}")
}

const CASE_ID_PATTERN: &str = "^\\S+$";
const IDENT_PATTERN: &str = "^[A-Za-z_][A-Za-z0-9_]*$";
const CORPUS_KEY_PATTERN: &str = "^[a-z0-9_-]+(\\.[a-z0-9_-]+)*$";
const OPTION_TAG_PATTERN: &str = "^[a-z0-9_-]+$";

fn string_array(item_pattern: Option<&str>) -> Value {
    match item_pattern {
        Some(p) => json!({ "type": "array", "items": { "type": "string", "pattern": p } }),
        None => json!({ "type": "array", "items": { "type": "string" } }),
    }
}

/// A `{ its, formats }` technology-profile object schema, as a party DECLARES
/// one.
fn tech_profile_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["its"],
        "properties": {
            "its": { "enum": tokens(ItsName::ALL) },
            "formats": { "type": "array", "items": { "enum": tokens(FormatName::ALL) } }
        }
    })
}

/// The technology profile a results document RECORDS: the declared shape plus
/// the provenance of the recorded format list.
fn recorded_tech_profile_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["its"],
        "properties": {
            "its": { "enum": tokens(ItsName::ALL) },
            "formats": { "type": "array", "items": { "enum": tokens(FormatName::ALL) } },
            "source": {
                "enum": tokens(TechProfileSource::ALL),
                "description": "Where the recorded format list came from: `declared` when the party statement's tech_profiles entry for this ITS named it, `defaulted` when no declaration named this ITS and every format the instrument speaks was recorded instead. The recorded list is what the verdict pipeline selects gating records with, so the two carry different claims: a defaulted list keeps a red row in an unlisted format from vanishing behind a pass, and it is not a party's claim about the formats it serves. Absent only in a document written before the member existed, where absence is unknown and never either source."
            }
        }
    })
}

/// The declared spec-version object: fixed component keys, each a string.
fn spec_versions_def() -> Value {
    let mut props = serde_json::Map::new();
    for component in SpecComponent::ALL {
        props.insert(token_str(component), json!({ "type": "string" }));
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": Value::Object(props)
    })
}

/// `statement.json` — the party statement (ICS + `SDoC`).
#[must_use]
pub fn statement_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("statement"),
        "title": "CNF 2.0 party statement (ICS + SDoC)",
        "description": "The supplier's Implementation Conformance Statement (the verdict-bearing claims) plus the SDoC self-declaration. The canonical JSON interchange artifact the verdict machinery consumes; verdicts are computed from it, never asserted here.",
        "type": "object",
        "additionalProperties": false,
        "required": ["product", "schedule_release", "claims"],
        "properties": {
            "product": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "version", "vendor", "identifier"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "version": { "type": "string", "minLength": 1 },
                    "vendor": { "type": "string", "minLength": 1 },
                    "identifier": { "type": "string", "minLength": 1 }
                }
            },
            "schedule_release": { "type": "string", "minLength": 1 },
            "spec_versions": spec_versions_def(),
            "claims": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "capabilities": {
                        "description": crate::arazzo::catalogue::CLAIMS_DESCRIPTION,
                        "type": "array",
                        "items": { "type": "string", "pattern": IDENT_PATTERN }
                    },
                    "profiles": { "type": "array", "items": { "enum": tokens(Tier::ALL) } }
                }
            },
            "tech_profiles": { "type": "array", "items": tech_profile_def() },
            "options": string_array(Some(OPTION_TAG_PATTERN)),
            "performance": {
                "type": "object",
                "additionalProperties": false,
                "required": ["class", "environment_ref"],
                "properties": {
                    "class": { "enum": perf_class_tokens() },
                    "environment_ref": { "type": "string", "minLength": 1 }
                }
            },
            "non_functional": { "type": "object" },
            "evidence": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["results_path", "sha256"],
                    "properties": {
                        "results_path": { "type": "string", "minLength": 1 },
                        "sha256": { "type": "string", "minLength": 1 }
                    }
                }
            },
            "attestation": {
                "type": "object",
                "additionalProperties": false,
                "required": ["signatory", "role", "date", "statement"],
                "properties": {
                    "signatory": { "type": "string", "minLength": 1 },
                    "role": { "type": "string", "minLength": 1 },
                    "date": { "type": "string", "minLength": 1 },
                    "statement": { "type": "string", "minLength": 1 }
                }
            }
        }
    })
}

/// One case×format outcome record, as the results document and the evidence
/// bundle both carry it.
fn outcome_record_def() -> Value {
    let row_record = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["row", "step", "reason"],
        "properties": {
            "row": { "type": "integer", "minimum": 0 },
            "step": { "type": "integer", "minimum": 0 },
            "reason": { "type": "string", "minLength": 1 }
        }
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["case", "status", "rows_driven", "rows_total"],
        "properties": {
            "case": { "type": "string", "pattern": CASE_ID_PATTERN },
            "format": { "enum": tokens(FormatName::ALL) },
            "status": { "enum": tokens(OutcomeStatus::ALL) },
            "rows_driven": { "type": "integer", "minimum": 0 },
            "rows_total": { "type": "integer", "minimum": 0 },
            "failing_step": { "type": "integer", "minimum": 0 },
            "reason": { "type": "string" },
            "citation": { "type": "string" },
            "failed_rows": { "type": "array", "items": row_record },
            "errored_rows": { "type": "array", "items": row_record }
        },
        "allOf": [
            { "if": { "properties": { "status": { "enum": ["skipped", "not_applicable"] } },
                      "required": ["status"] },
              "then": { "required": ["case", "status", "rows_driven", "rows_total", "citation"],
                        "properties": { "citation": { "type": "string", "minLength": 1 } } } }
        ]
    })
}

/// The approval status of the instrument that wrote a results document.
/// One recorded identity field: its value and who supplied it.
fn identity_field_def(sources: &[IdentitySource]) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["value", "source"],
        "properties": {
            "value": { "type": "string", "minLength": 1 },
            "source": { "enum": tokens(sources) }
        }
    })
}

/// The recorded SUT identity, each field with its source.
fn sut_def() -> Value {
    json!({
        "description": "The SUT identity. A field the server stated in its ITS-REST System OPTIONS document (specifications/operations/options.yaml, schemas/others/Options.yaml) carries the server's value with source `server`; the operator supplies only the name or version the server did not state (source `operator`). `vendor` and `restapi_specs_version` are present only when the server stated them.",
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "version"],
        "properties": {
            "name": identity_field_def(IdentitySource::ALL),
            "version": identity_field_def(IdentitySource::ALL),
            "vendor": identity_field_def(&[IdentitySource::Server]),
            "restapi_specs_version": identity_field_def(&[IdentitySource::Server])
        }
    })
}

fn instrument_status_def() -> Value {
    json!({
        "enum": tokens(InstrumentStatus::ALL),
        "description": "The approval status of the instrument that wrote this document, derived from its own version: pre-approval for every 0.x version, which the openEHR SEC has not approved, and sec-approved from 1.0.0, which marks that approval."
    })
}

/// `results.json` — the party results (the campaign outcomes).
#[must_use]
pub fn results_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("results"),
        "title": "CNF 2.0 party results",
        "description": "The campaign outcomes for one technology profile: per-case×format outcome records (with mandatory citations on skipped/not_applicable), the performance measurements, and provenance (SUT/runner/ixit digest).",
        "type": "object",
        "additionalProperties": false,
        "required": ["sut", "runner", "instrument_status", "schedule_release", "tech_profile", "ixit_digest"],
        "properties": {
            "sut": sut_def(),
            "runner": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "version", "verification_pack_status"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "version": { "type": "string", "minLength": 1 },
                    "verification_pack_status": { "enum": tokens(VerificationPackStatus::ALL) }
                }
            },
            "instrument_status": instrument_status_def(),
            "schedule_release": { "type": "string", "minLength": 1 },
            "tech_profile": recorded_tech_profile_def(),
            "ixit_digest": {
                "type": "string",
                "pattern": "^[0-9a-f]{16}$",
                "description": "The leading 8 bytes of the SHA-256 over the ixit declaration this campaign was driven under, lowercase hex, computed over the document's bytes with nothing canonicalized first. A reader holding that declaration re-derives it with `sha256sum ixit.json | cut -c1-16`."
            },
            "statement_digest": {
                "type": "string",
                "pattern": "^[0-9a-f]{16}$",
                "description": "The leading 8 bytes of the SHA-256 over the party statement this campaign was selected under, lowercase hex, computed over the document's bytes with nothing canonicalized first. A reader holding that statement re-derives it with `sha256sum statement.json | cut -c1-16`, which is what tells one claim from another claim declaring the same its-rest formats. Absent for a campaign no statement selected (selection_basis statement_blind), and absent in a document written before the member existed, where the identity is unknown and never a match."
            },
            "selection_basis": {
                "enum": tokens(SelectionBasis::ALL),
                "description": "What ISO/IEC 9646 test selection had to select this campaign with: `statement` when a party statement (ICS) was supplied, `statement_blind` when none was. A blind campaign is a sweep of the whole catalogue rather than a party-scoped record — nothing selected the party's option arms, extension routes, claimed capabilities or release floors, and the cases whose failure that absence would manufacture (an unselected arm of a mutually exclusive option_select branch, an extension route no released text governs) are recorded not_applicable with their citation. Absent only in a document written before the member existed, where absence is unknown and never either basis."
            },
            "restapi_specs_version": {
                "type": "string",
                "minLength": 1,
                "description": "The restapi_specs_version member the SUT's System OPTIONS manifest served during the campaign, when that exchange was driven (released OAS system.openapi.yaml Options — every member optional, so absence is normal). An independent confirmation of the statement's declared spec_versions.its_rest, never a source of truth: divergence from the declaration is a static-review finding, not a re-declaration."
            },
            "outcomes": {
                "type": "array",
                "items": outcome_record_def()
            },
            "measurements": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["case", "class", "environment", "offered_load_sustained",
                                  "warmup_s", "duration_s", "operations", "verdict"],
                    "properties": {
                        "case": { "type": "string", "pattern": CASE_ID_PATTERN },
                        "class": { "enum": perf_class_tokens() },
                        "environment": environment_def(),
                        "offered_load_sustained": { "type": "number", "minimum": 0.0 },
                        "warmup_s": { "type": "integer", "minimum": 0 },
                        "duration_s": { "type": "integer", "minimum": 1 },
                        "operations": {
                            "type": "array",
                            "minItems": 1,
                            "items": operation_measurement_def()
                        },
                        "verdict": { "enum": ["earned", "not-earned"] },
                        "violations": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                        "resources": resources_def()
                    }
                }
            }
        }
    })
}

/// The version-signing posture block — declared party-wide (the default) and
/// optionally per instance (the deployment that runs a different mode). One
/// definition, two placements, so the two can never drift apart.
fn signing_def(description: &str) -> Value {
    json!({
        "description": description,
        "type": "object",
        "required": ["mode"],
        "oneOf": [
            { "additionalProperties": false, "required": ["mode", "algorithm", "encoding"],
              "properties": { "mode": { "const": "digest" },
                              "algorithm": { "type": "string", "minLength": 1 },
                              "encoding": { "type": "string", "minLength": 1 },
                              "prefix": { "type": "string" } } },
            { "additionalProperties": false, "required": ["mode", "public_key"],
              "properties": { "mode": { "const": "pgp" },
                              "public_key": { "type": "string", "minLength": 1 } } }
        ]
    })
}

/// The openEHR specification generation set — declared party-wide (the
/// default) and optionally per instance (the deployment running the other
/// set). One definition, two placements, exactly like [`signing_def`], so the
/// two can never drift apart.
fn spec_profile_def(description: &str) -> Value {
    json!({
        "description": description,
        "enum": tokens(crate::ixit::SpecProfile::ALL)
    })
}

/// The terminology posture block — declared party-wide (the default) and
/// optionally per instance (the deployment that runs the other
/// unresolvable-value-set posture). One definition, two placements, exactly
/// like [`signing_def`], so the two can never drift apart.
fn terminology_def(description: &str) -> Value {
    json!({
        "description": description,
        "type": "object",
        "additionalProperties": false,
        "required": ["posture", "servers"],
        "properties": {
            "posture": {
                "description": "What the deployment does with a bound value set it cannot resolve. BASE architecture_overview master12 §Binding Terminology Value-sets to Archetypes puts the query in an external terminology query server and never says what happens when it cannot be reached, so the branch is a deployment fact, not a spec rule (the `terminology-binding-unresolvable` option in the catalogue's capabilities.yaml).",
                "enum": tokens(crate::ixit::TerminologyPosture::ALL)
            },
            "servers": {
                "description": "The terminology servers this deployment is wired to. A namespace is whatever key a case names for a terminology (code-system URI, value-set URL, terminology id); a DECLARED-unreachable server is how the terminology-server-down branch is exercised for the whole run, never by a mid-run reconfiguration.",
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name", "namespaces"],
                    "properties": {
                        "name": { "type": "string", "minLength": 1 },
                        "reachable": { "type": "boolean" },
                        "namespaces": { "type": "array", "minItems": 1, "items": { "type": "string", "minLength": 1 } }
                    }
                }
            }
        }
    })
}

/// `ixit.json` — the SUT topology the runner drives (ISO/IEC 9646 IXIT).
/// The schema validates exactly what [`crate::ixit::Ixit`] parses.
#[must_use]
pub fn ixit_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("ixit"),
        "title": "CNF 2.0 IXIT (implementation extra information for testing)",
        "description": "The SUT topology a runner drives: named instances (base URL + auth mode, credentials by env-var reference, never inline) plus the environment block. `veredictum test` takes its server from `--base-url` and reads no instance, so a conformance-only IXIT may omit `instances`; every instrument that addresses an instance refuses an IXIT without the default instance `sut`.",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "instances": ixit_instances_def(),
            "environment": declared_environment_def(),
            "containers": {
                "type": "object",
                "additionalProperties": false,
                "required": ["sut", "db"],
                "properties": {
                    "sut": { "type": "string", "minLength": 1 },
                    "db": { "type": "string", "minLength": 1 }
                }
            },
            "system_id": {
                "description": "The SUT's own configured system identifier — the value it stamps into AUDIT_DETAILS.system_id when the client supplies none (ITS-REST Requests_and_responses §openehr-version and openehr-audit-details) and into every OBJECT_VERSION_ID.creating_system_id it mints. Declared here because no released operation discloses it; absent => the cases reading ${ixit:system_id} are not-applicable with that citation.",
                "type": "string",
                "minLength": 1
            },
            "dump_location": {
                "description": "A location on the SUT's OWN file system the admin dump/load operations may write an archive to and read one back from (no released ITS-REST operation names one). Declared here because which paths a deployment can write is a property of its image and mounts that no operation discloses; absent => the cases reading ${ixit:dump_location} are not-applicable with that citation.",
                "type": "string",
                "minLength": 1
            },
            "signing": signing_def(
                "The party's DEFAULT version-signing posture (RM common master06 §Digital Signature). Present => the Signing capability is claimed and this block declares the mode every instance runs unless it declares its own. digest: self-describing plain digest (algorithm/encoding/prefix); pgp: openPGP verified against the public key."
            ),
            "terminology": terminology_def(
                "The party's DEFAULT terminology posture: the terminology query servers this deployment is wired to (BASE architecture_overview master12 §Binding Terminology Value-sets to Archetypes — the bound value set is resolved by a server outside the CDR), which namespaces each answers for, and the unresolvable-value-set branch it realizes. Declared here because released ITS-REST 1.1.0 surfaces no terminology resource, so nothing on the wire discloses any of it; absent => every terminology-dependent case is not-applicable with that citation."
            ),
            "spec_profile": spec_profile_def(
                "The party's DEFAULT openEHR specification generation set: `stable` = the latest RELEASED generations, `development` = the development generations, which admit surface no release defines yet. Declared here because no released operation discloses which set a deployment runs (the openEHR release strategy makes a minor release a compatible superset, so the sets differ only in accepted surface); absent => every case whose expectation rests on a generation set is not-applicable with that citation. An instance may override it."
            ),
            "smart": ixit_smart_def(),
            "scoped_tokens": ixit_scoped_tokens_def()
        }
    })
}

/// The `instances` member of the ixit — the named SUT deployments/principals
/// (base URL + auth mode with credentials by env-var reference, never
/// inline), each optionally carrying its own `signing`/`terminology` posture
/// (instance-first resolution, the party default as fallback).
fn ixit_instances_def() -> Value {
    json!({
        "type": "object",
        "minProperties": 1,
        "propertyNames": { "pattern": IDENT_PATTERN },
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "required": ["base_url", "auth"],
            "properties": {
                "base_url": { "type": "string", "minLength": 1 },
                "auth": {
                    "type": "object",
                    "required": ["mode"],
                    "oneOf": [
                        { "additionalProperties": false, "required": ["mode"],
                          "properties": { "mode": { "const": "none" } } },
                        { "additionalProperties": false,
                          "required": ["mode", "user_env", "password_env"],
                          "properties": { "mode": { "const": "basic" },
                                          "user_env": { "type": "string", "minLength": 1 },
                                          "password_env": { "type": "string", "minLength": 1 } } },
                        { "additionalProperties": false, "required": ["mode", "token_env"],
                          "properties": { "mode": { "const": "bearer" },
                                          "token_env": { "type": "string", "minLength": 1 } } },
                        { "additionalProperties": false, "required": ["mode"],
                          "properties": { "mode": { "const": "bearer_mint" },
                                          "subject": { "type": "string", "minLength": 1 },
                                          "roles": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                                          "default_scopes": { "type": "array", "items": { "type": "string" } } } }
                    ]
                },
                "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                "signing": signing_def(
                    "THIS instance's version-signing posture, when it differs from the party default (RM common master06 §Digital Signature: the mode is a deployment fact, and a deployment runs one). A party claiming both modes declares two deployments as two instances, each with its own block; every signature check resolves instance-first, party default second. Absent => the top-level `signing` applies."
                ),
                "terminology": terminology_def(
                    "THIS instance's terminology posture, when it differs from the party default. The unresolvable-value-set branch is one switch per running deployment, so a party exercising both runs two deployments and declares each one's posture on its own instance — the same law the `signing` block follows. Absent => the top-level `terminology` applies."
                ),
                "spec_profile": spec_profile_def(
                    "THIS instance's openEHR specification generation set, when it differs from the party default. One running deployment implements exactly one set, so a party claiming both runs two deployments and declares each one's set on its own instance — the same law the `signing` and `terminology` blocks follow. Absent => the top-level `spec_profile` applies."
                ),
                "administrative": {
                    "description": "Whether this instance's principal holds administrative authorization (upstream report #757). No released operation discloses which roles a principal holds, so the posture is an IXIT declaration; a role-boundary case states the posture it needs in requires, and an undeclared or opposite declaration records it not-applicable with that citation. Absent => undeclared, never a default.",
                    "type": "boolean"
                }
            }
        }
    })
}

/// The `scoped_tokens` member of the IXIT: where `veredictum test` takes the
/// token a step that names a SMART scope set presents. No openEHR spec
/// governs it — our own design.
fn ixit_scoped_tokens_def() -> Value {
    use crate::arazzo::credential::{
        MINTED_CLAIMS, SCOPED_KEY_ENV, SCOPED_TOKEN_PREFIX, SCOPED_TOKENS_MEMBER,
    };
    json!({
        "description": format!("The tokens a step that names a SMART scope set (`x-veredictum-scope`) presents in `veredictum test`. `issued` maps a scope set, space-delimited scope tokens in any order (RFC 6749 §3.3), to the variable holding a token issued for it, read from the variable or from the file its `_FILE` variant names. `mint` declares a test issuer the deployment trusts, which signs an RS256 token for any scope set with the RSA private key in `{SCOPED_KEY_ENV}` (or its `_FILE`); a supplied token wins over a minted one. Every variable named here must be set when the run starts. Absent => every row presenting a scope set is skipped as `not-declared`. The `{SCOPED_TOKENS_MEMBER}` member names variables, never a token."),
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "issued": {
                "type": "object",
                "additionalProperties": {
                    "type": "string",
                    "pattern": format!("^{SCOPED_TOKEN_PREFIX}[A-Z0-9_]+$"),
                    "not": { "const": SCOPED_KEY_ENV }
                }
            },
            "mint": {
                "type": "object",
                "additionalProperties": false,
                "required": ["issuer", "subject", "kid", "ttl_seconds"],
                "properties": {
                    "issuer": { "description": "The `iss` claim.", "type": "string" },
                    "subject": { "description": "The `sub` claim.", "type": "string" },
                    "audience": { "description": "The `aud` claim, if the deployment checks one.", "type": "string" },
                    "kid": { "description": "The `kid` header parameter naming the key the deployment verifies with.", "type": "string" },
                    "ttl_seconds": { "description": "How long a minted token lives.", "type": "integer", "minimum": 1, "maximum": u32::MAX },
                    "claims": {
                        "description": "Further claims, such as the role claim the deployment's own access control reads.",
                        "type": "object",
                        "propertyNames": { "not": { "enum": MINTED_CLAIMS } }
                    }
                }
            }
        }
    })
}

/// The `smart` member of the ixit — the party's SMART App Launch lane
/// (ITS-REST `docs/smart_app_launch`): the Platform instance + the static
/// test issuer the runner mints per-step scoped tokens against.
fn ixit_smart_def() -> Value {
    json!({
        "description": "The party's SMART App Launch lane (ITS-REST docs/smart_app_launch). Present => the deployment runs the CDR's SMART resource-server role and trusts the declared static test issuer, so the runner may mint per-step scoped access tokens (the CDR never issues them — master06 §Supported Authentication Flows makes that the Authorization Server's duty, and the conformance stack runs none). Absent => every SMART case is not-applicable with that citation.",
        "type": "object",
        "additionalProperties": false,
        "required": ["platform_instance", "mint"],
        "properties": {
            "platform_instance": {
                "description": "The instance whose base_url is the SMART Platform base URL — master04 §Service Discovery serves /.well-known/smart-configuration relative to it, not to the openEHR REST base the other instances address.",
                "type": "string",
                "pattern": IDENT_PATTERN
            },
            "mint": {
                "description": "The static test issuer the `bearer_mint` instances sign RS256 access tokens with. Committed test material, never production key material.",
                "type": "object",
                "additionalProperties": false,
                "required": ["issuer", "subject", "key_file", "kid", "ttl_seconds"],
                "properties": {
                    "issuer": { "type": "string", "minLength": 1 },
                    "audience": { "type": "string", "minLength": 1 },
                    "subject": { "type": "string", "minLength": 1 },
                    "roles": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                    "key_file": { "type": "string", "minLength": 1 },
                    "kid": { "type": "string", "minLength": 1 },
                    "ttl_seconds": { "type": "integer", "minimum": 1 }
                }
            }
        }
    })
}

/// The resource-telemetry block of one measurement record or stress step
/// — measured CONTEXT, never verdict-bearing: per-container CPU/RSS/I/O
/// series on a fixed cadence (run-clock offsets, phase-stamped) plus, on
/// measured class runs only, the database volume's four disk anchors
/// (stress steps stay anchor-free — exploration stays light). Optional:
/// absent when the ixit declares no `containers` block or the container
/// runtime was unreachable.
fn resources_def() -> Value {
    let byte_counter = json!({ "type": "integer", "minimum": 0 });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["sample_interval_s", "containers"],
        "properties": {
            "sample_interval_s": { "type": "integer", "minimum": 1 },
            "containers": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["role", "name", "samples"],
                    "properties": {
                        "role": { "enum": tokens(crate::perf::ContainerRole::ALL) },
                        "name": { "type": "string", "minLength": 1 },
                        "samples": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["offset_s", "phase", "cpu_pct", "rss_bytes",
                                              "blk_read_bytes", "blk_write_bytes",
                                              "net_rx_bytes", "net_tx_bytes"],
                                "properties": {
                                    "offset_s": { "type": "integer", "minimum": 0 },
                                    "phase": { "enum": tokens(crate::perf::ResourcePhase::ALL) },
                                    "cpu_pct": { "type": "number", "minimum": 0.0 },
                                    "rss_bytes": byte_counter.clone(),
                                    "blk_read_bytes": byte_counter.clone(),
                                    "blk_write_bytes": byte_counter.clone(),
                                    "net_rx_bytes": byte_counter.clone(),
                                    "net_tx_bytes": byte_counter.clone()
                                }
                            }
                        }
                    }
                }
            },
            "disk": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "before_scale_seed_bytes": byte_counter.clone(),
                    "after_scale_seed_bytes": byte_counter.clone(),
                    "after_ward_seed_bytes": byte_counter.clone(),
                    "after_window_bytes": byte_counter.clone(),
                    "seed_compositions": { "type": "integer", "minimum": 1 }
                }
            }
        }
    })
}

/// The ixit environment block as a party declares it: a conformance run
/// reads only `exclusive_server`, and a measured run refuses an IXIT lacking
/// any other member, naming it.
fn declared_environment_def() -> Value {
    let mut def = environment_def();
    if let Some(block) = def.as_object_mut() {
        block.remove("required");
        block.insert(
            "description".to_owned(),
            json!("`exclusive_server` is all a conformance run reads. `perf`, `stress` and `aql-probe` need every other member, and refuse an IXIT lacking one, naming it."),
        );
    }
    def
}

/// The environment a measured run was taken in (every measurement record
/// embeds it — an earned class is reported WITH its environment).
fn environment_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["hardware_class", "cores", "memory_gb", "storage_class", "topology"],
        "properties": {
            "exclusive_server": { "type": "boolean" },
            "hardware_class": { "type": "string", "minLength": 1 },
            "cores": { "type": "integer", "minimum": 0 },
            "memory_gb": { "type": "integer", "minimum": 0 },
            "storage_class": { "type": "string", "minLength": 1 },
            "topology": { "type": "string", "minLength": 1 }
        }
    })
}

/// One per-operation measurement record (shared by the results.json
/// `measurements` block and the stress report's steps): counts, summary
/// percentiles, and the re-checkable base64 HDR V2 histogram.
fn operation_measurement_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["operation", "requests", "errors",
                      "latency_ms_p50", "latency_ms_p90",
                      "latency_ms_p99", "hdr_v2_base64"],
        "properties": {
            "operation": { "type": "string", "minLength": 1 },
            "requests": { "type": "integer", "minimum": 0 },
            "errors": { "type": "integer", "minimum": 0 },
            "latency_ms_p50": { "type": "number", "minimum": 0.0 },
            "latency_ms_p90": { "type": "number", "minimum": 0.0 },
            "latency_ms_p99": { "type": "number", "minimum": 0.0 },
            "hdr_v2_base64": { "type": "string", "minLength": 1 }
        }
    })
}

/// `stress.json` — the step-load stress report (exploration only: the
/// maximum sustainable throughput, never a conformance record).
#[must_use]
pub fn stress_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("stress"),
        "title": "CNF 2.0 stress report",
        "description": "The step-load stress instrument's exploration artifact: geometric load steps (each with re-checkable HDR V2 records) to the maximum sustainable throughput — where the system breaks — run on a class-scale corpus, environment-bound; never a conformance record, and floor-free by design.",
        "type": "object",
        "additionalProperties": false,
        "required": ["corpus", "environment", "step_warmup_s", "step_hold_s",
                      "p99_budget_ms", "error_budget", "steps",
                      "max_sustainable_throughput_per_s", "ladder_capped",
                      "generator_bound", "remark"],
        "properties": {
            "corpus": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
            "environment": environment_def(),
            "step_warmup_s": { "type": "integer", "minimum": 0 },
            "step_hold_s": { "type": "integer", "minimum": 1 },
            "p99_budget_ms": { "type": "number", "exclusiveMinimum": 0.0 },
            "error_budget": { "type": "number", "minimum": 0.0 },
            "steps": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["rate", "offered_load_sustained", "operations",
                                  "stable", "generator_bound"],
                    "properties": {
                        "rate": { "type": "number", "exclusiveMinimum": 0.0 },
                        "offered_load_sustained": { "type": "number", "minimum": 0.0 },
                        "operations": {
                            "type": "array",
                            "minItems": 1,
                            "items": operation_measurement_def()
                        },
                        "stable": { "type": "boolean" },
                        "breaches": { "type": "array", "items": { "type": "string", "minLength": 1 } },
                        "generator_bound": { "type": "boolean" },
                        "resources": resources_def()
                    }
                }
            },
            "max_sustainable_throughput_per_s": { "type": "number", "minimum": 0.0 },
            "ladder_capped": { "type": "boolean" },
            "generator_bound": { "type": "boolean" },
            "remark": { "type": "string", "minLength": 1 }
        }
    })
}

/// `aql-probe.json` — the AQL optimization probe report (exploration
/// evidence: wire percentiles + per-statement DB attribution over the
/// seeded corpus; never a conformance record).
#[must_use]
pub fn aql_probe_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("aql-probe"),
        "title": "CNF 2.0 AQL probe report",
        "description": "The seeded-corpus AQL optimization probe: per-probe wire-latency percentiles and pg_stat_statements attribution through the container runtime, environment-bound. Exploration evidence for the optimization loop — never a conformance record; results.json is never touched.",
        "type": "object",
        "additionalProperties": false,
        "required": ["corpus", "environment", "requests_per_probe", "maintenance_settled",
                      "attribution", "probes", "remark"],
        "properties": {
            "corpus": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
            "environment": environment_def(),
            "requests_per_probe": { "type": "integer", "minimum": 1 },
            "maintenance_settled": { "type": "boolean" },
            "attribution": { "type": "string", "minLength": 1 },
            "probes": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name", "aql", "failures", "wire_ms"],
                    "properties": {
                        "name": { "type": "string", "minLength": 1 },
                        "aql": { "type": "string", "minLength": 1 },
                        "failures": { "type": "integer", "minimum": 0 },
                        "wire_ms": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["min_ms", "p50_ms", "p95_ms", "max_ms"],
                            "properties": {
                                "min_ms": { "type": "number", "minimum": 0.0 },
                                "p50_ms": { "type": "number", "minimum": 0.0 },
                                "p95_ms": { "type": "number", "minimum": 0.0 },
                                "max_ms": { "type": "number", "minimum": 0.0 }
                            }
                        },
                        "statements": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["sql", "calls", "mean_ms", "total_ms",
                                              "shared_blks_hit", "shared_blks_read"],
                                "properties": {
                                    "sql": { "type": "string", "minLength": 1 },
                                    "calls": { "type": "integer", "minimum": 0 },
                                    "mean_ms": { "type": "number", "minimum": 0.0 },
                                    "total_ms": { "type": "number", "minimum": 0.0 },
                                    "shared_blks_hit": { "type": "integer", "minimum": 0 },
                                    "shared_blks_read": { "type": "integer", "minimum": 0 }
                                }
                            }
                        }
                    }
                }
            },
            "remark": { "type": "string", "minLength": 1 }
        }
    })
}

/// The published performance-class tokens, ladder order.
fn perf_class_tokens() -> Vec<&'static str> {
    crate::perf::PerfClass::ALL
        .iter()
        .map(|c| c.token())
        .collect()
}

/// The performance cases under the performance schedule's `classes/`
/// (conformance-by-measurement).
#[must_use]
pub fn performance_case_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("performance-case"),
        "title": "CNF 2.0 performance case",
        "description": "A kind: performance case — open-loop offered load against class thresholds; verdicts are measured (earned | not-earned), bound to the ixit environment block.",
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "kind", "component", "description", "test_purpose", "spec_refs", "class", "corpus", "workload", "thresholds"],
        "properties": {
            "id": { "type": "string", "pattern": CASE_ID_PATTERN },
            "kind": { "const": "performance" },
            "component": { "const": "PERFORMANCE" },
            "description": { "type": "string", "minLength": 1 },
            "test_purpose": { "type": "string", "minLength": 1 },
            "spec_refs": { "type": "array", "minItems": 1, "items": { "type": "string", "minLength": 1 } },
            "class": { "enum": ["POC", "S", "L", "R"] },
            "corpus": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
            "workload": {
                "type": "object",
                "additionalProperties": false,
                "required": ["arrival_rate", "warmup", "duration", "journeys"],
                "properties": {
                    "arrival_rate": { "type": "string", "pattern": "^[0-9.]+/s$" },
                    "warmup": { "type": "string", "pattern": "^PT([0-9]+H)?([0-9]+M)?([0-9]+S)?$" },
                    "duration": { "type": "string", "pattern": "^PT([0-9]+H)?([0-9]+M)?([0-9]+S)?$" },
                    "arrival_curve": { "enum": ["uniform", "diurnal"] },
                    "journeys": { "type": "object", "minProperties": 1,
                              "propertyNames": { "pattern": IDENT_PATTERN },
                              "additionalProperties": { "type": "string", "pattern": "^[0-9.]+%$" } }
                }
            },
            "thresholds": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["metric"],
                    "properties": {
                        "metric": { "enum": ["latency_p50", "latency_p90", "latency_p99", "error_rate", "offered_load_sustained"] },
                        "operation": { "type": "string" },
                        "max": { "type": "number" },
                        "min": { "type": "number" }
                    }
                }
            }
        }
    })
}

/// The performance schedule's `payloads.yaml` — every request body and
/// operational template the measured instruments send.
#[must_use]
pub fn perf_payloads_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("perf-payloads"),
        "title": "Performance schedule payload manifest",
        "description": "Every payload the measured instruments send, keyed by the name a journey stage or the seeder uses: its file under payloads/, the operational template's wire identifier where it has one, and its provenance. No openEHR spec governs measured performance (CNF guide master03-overview.adoc §Product Scope) — our own design.",
        "type": "object",
        "minProperties": 1,
        "propertyNames": { "pattern": CORPUS_KEY_PATTERN },
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "required": ["source", "provenance"],
            "properties": {
                "source": { "type": "string", "minLength": 1 },
                "template_id": { "type": "string", "minLength": 1 },
                "provenance": { "type": "string", "minLength": 1 }
            }
        }
    })
}

/// The published journey-operation tokens, vocabulary order.
fn perf_op_tokens() -> Vec<&'static str> {
    crate::perf::PerfOp::ALL
        .iter()
        .map(|op| op.as_str())
        .collect()
}

/// The performance schedule's `journeys.yaml` — the hospital-simulation
/// journey vocabulary the performance workloads decompose into.
#[must_use]
pub fn journey_catalogue_schema() -> Value {
    let duration_pattern = "^PT([0-9]+H)?([0-9]+M)?([0-9]+S)?$";
    let offset = json!({
        "oneOf": [
            { "type": "string", "pattern": duration_pattern },
            { "type": "object", "additionalProperties": false,
              "required": ["uniform"],
              "properties": { "uniform": {
                  "type": "array", "minItems": 2, "maxItems": 2,
                  "items": { "type": "string", "pattern": duration_pattern } } } },
            { "type": "object", "additionalProperties": false,
              "required": ["periodic"],
              "properties": { "periodic": {
                  "type": "object", "additionalProperties": false,
                  "required": ["interval", "count"],
                  "properties": {
                      "interval": { "type": "string", "pattern": duration_pattern },
                      "count": { "type": "integer", "minimum": 1 } } } } }
        ]
    });
    json!({
        "$schema": DRAFT,
        "$id": urn("journey-catalogue"),
        "title": "CNF 2.0 journey catalogue",
        "description": "The hospital-simulation vocabulary: clinical journeys as ordered, time-offset operation sequences over the closed operation vocabulary; commit/update stages name their payload in perf/payloads.yaml; each journey cites its activity-statistics derivation. No openEHR spec governs measured performance (CNF guide master03-overview.adoc §Product Scope) — our own design/extension.",
        "type": "object",
        "minProperties": 1,
        "propertyNames": { "pattern": IDENT_PATTERN },
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "required": ["description", "derivation", "stages"],
            "properties": {
                "description": { "type": "string", "minLength": 1 },
                "derivation": { "type": "string", "minLength": 1 },
                "stages": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["op", "at"],
                        "properties": {
                            "op": { "enum": perf_op_tokens() },
                            "template": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
                            "at": offset
                        }
                    }
                }
            }
        }
    })
}

/// The tokens of a closed bench vocabulary, as a JSON array.
fn bench_tokens(tokens: &[&'static str]) -> Value {
    Value::Array(tokens.iter().map(|token| json!(token)).collect())
}

/// One operation's per-repetition statistics inside a bench result.
fn bench_operation_stats_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["count", "errors", "errors_by_class", "throughput_ops_s",
                      "p50_us", "p75_us", "p90_us", "p99_us", "p999_us", "max_us",
                      "hdr_v2_base64"],
        "properties": {
            "count": { "type": "integer", "minimum": 0 },
            "errors": { "type": "integer", "minimum": 0 },
            "errors_by_class": {
                "type": "object",
                "propertyNames": {
                    "enum": bench_tokens(&crate::bench::result::ErrorClass::ALL
                        .iter().map(|class| class.as_str()).collect::<Vec<_>>())
                },
                "additionalProperties": { "type": "integer", "minimum": 0 }
            },
            "throughput_ops_s": { "type": "number", "minimum": 0.0 },
            "p50_us": { "type": "integer", "minimum": 0 },
            "p75_us": { "type": "integer", "minimum": 0 },
            "p90_us": { "type": "integer", "minimum": 0 },
            "p99_us": { "type": "integer", "minimum": 0 },
            "p999_us": { "type": "integer", "minimum": 0 },
            "max_us": { "type": "integer", "minimum": 0 },
            "hdr_v2_base64": { "type": "string", "minLength": 1 }
        }
    })
}

/// A cross-repetition median plus inter-quartile range.
fn bench_cross_stat_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["median", "iqr"],
        "properties": {
            "median": { "type": "number" },
            "iqr": { "type": "number", "minimum": 0.0 }
        }
    })
}

/// The bench operation vocabulary, as the property names every per-operation
/// map is keyed by.
fn bench_operation_names() -> Value {
    json!({
        "enum": bench_tokens(&crate::bench::pack::BenchOp::ALL
            .iter().map(|op| op.as_str()).collect::<Vec<_>>())
    })
}

/// One phase's cross-repetition summary, reused by the target's `cross` block
/// and by every baseline's.
fn bench_cross_phase_def(regimes: &Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": {
            "type": "object",
            "additionalProperties": false,
            "required": ["regime", "operations"],
            "properties": {
                "regime": { "enum": regimes },
                "operations": {
                    "type": "object",
                    "propertyNames": bench_operation_names(),
                    "additionalProperties": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["repetitions", "p50_us", "p75_us", "p90_us",
                                      "p99_us", "p999_us", "throughput_ops_s"],
                        "properties": {
                            "repetitions": { "type": "integer", "minimum": 1 },
                            "p50_us": bench_cross_stat_def(),
                            "p75_us": bench_cross_stat_def(),
                            "p90_us": bench_cross_stat_def(),
                            "p99_us": bench_cross_stat_def(),
                            "p999_us": bench_cross_stat_def(),
                            "throughput_ops_s": bench_cross_stat_def()
                        }
                    }
                }
            }
        }
    })
}

/// One same-machine baseline block: a full per-operation summary beside the
/// provenance a submitter must disclose for it to be re-composable.
fn bench_baseline_def(regimes: &Value, repetitions: &Value, seed_phases: &Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["cdr", "display_name", "images", "recipe", "resources", "base_url",
                      "started_at", "finished_at", "seed_phases", "repetitions", "cross",
                      "posture"],
        "properties": {
            "cdr": {
                "enum": bench_tokens(&crate::bench::baselines::ReferenceCdr::ALL
                    .iter().map(|cdr| cdr.as_str()).collect::<Vec<_>>())
            },
            "display_name": { "type": "string", "minLength": 1 },
            "images": {
                "type": "object",
                "minProperties": 1,
                "additionalProperties": {
                    "type": "string",
                    "pattern": "^[^@]+@sha256:[0-9a-f]{64}$"
                }
            },
            "recipe": {
                "type": "object",
                "additionalProperties": false,
                "required": ["repository", "git_ref", "file"],
                "properties": {
                    "repository": { "type": "string", "minLength": 1 },
                    "git_ref": { "type": "string", "minLength": 1 },
                    "file": { "type": "string", "minLength": 1 }
                }
            },
            "resources": {
                "type": "object",
                "additionalProperties": false,
                "required": ["server_cpus", "server_memory", "database_cpus",
                              "database_memory", "database_shm_size"],
                "properties": {
                    "server_cpus": { "type": "string", "minLength": 1 },
                    "server_memory": { "type": "string", "minLength": 1 },
                    "database_cpus": { "type": "string", "minLength": 1 },
                    "database_memory": { "type": "string", "minLength": 1 },
                    "database_shm_size": { "type": "string", "minLength": 1 }
                }
            },
            "base_url": { "type": "string", "minLength": 1 },
            "sut_version": { "type": "string", "minLength": 1 },
            "started_at": { "type": "string", "minLength": 1 },
            "finished_at": { "type": "string", "minLength": 1 },
            "seed_phases": seed_phases,
            "repetitions": repetitions,
            "cross": bench_cross_phase_def(regimes),
            "posture": bench_posture_def()
        }
    })
}

/// The target measured against one baseline: the dimensionless ratio, its two
/// inputs, and every place no ratio could be formed.
fn bench_relative_def(regimes: &Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["baseline", "display_name", "derivation", "phases", "gaps"],
        "properties": {
            "baseline": {
                "enum": bench_tokens(&crate::bench::baselines::ReferenceCdr::ALL
                    .iter().map(|cdr| cdr.as_str()).collect::<Vec<_>>())
            },
            "display_name": { "type": "string", "minLength": 1 },
            "derivation": { "const": crate::bench::relative::RELATIVE_DERIVATION },
            "phases": {
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["regime", "operations"],
                    "properties": {
                        "regime": { "enum": regimes },
                        "operations": {
                            "type": "object",
                            "propertyNames": bench_operation_names(),
                            "additionalProperties": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["metrics"],
                                "properties": {
                                    "metrics": {
                                        "type": "object",
                                        "propertyNames": {
                                            "enum": bench_tokens(&crate::bench::compare::Metric::ALL
                                                .iter().map(|metric| metric.as_str())
                                                .collect::<Vec<_>>())
                                        },
                                        "additionalProperties": {
                                            "type": "object",
                                            "additionalProperties": false,
                                            "required": ["target_median", "baseline_median", "index"],
                                            "properties": {
                                                "target_median": { "type": "number" },
                                                "baseline_median": { "type": "number", "exclusiveMinimum": 0.0 },
                                                "index": { "type": "number", "minimum": 0.0 }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "gaps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["phase", "operation", "reason"],
                    "properties": {
                        "phase": { "type": "string", "minLength": 1 },
                        "operation": { "type": "string", "minLength": 1 },
                        "metric": {
                            "enum": bench_tokens(&crate::bench::compare::Metric::ALL
                                .iter().map(|metric| metric.as_str()).collect::<Vec<_>>())
                        },
                        "reason": {
                            "enum": bench_tokens(&crate::bench::relative::GapReason::ALL
                                .iter().map(|reason| reason.as_str()).collect::<Vec<_>>())
                        }
                    }
                }
            }
        }
    })
}

/// `bench-result.json` — one universal-benchmark run's record.
///
/// A comparative SPEED record, and nothing else: [`crate::bench::BOUNDARY_STATEMENT`]
/// is a schema-required constant, so a document that drops it is invalid.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one literal JSON-Schema document")]
pub fn bench_result_schema() -> Value {
    let regimes = bench_tokens(
        &crate::bench::result::LoopRegime::ALL
            .iter()
            .map(|regime| regime.as_str())
            .collect::<Vec<_>>(),
    );
    let seed_phases = json!({
        "type": "array",
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["name", "regime", "ehrs", "compositions_per_ehr", "workers",
                          "elapsed_s", "bulk_load_writes_per_s",
                          "whole_loop_ms_per_composition"],
            "properties": {
                "name": { "type": "string", "minLength": 1 },
                "regime": { "enum": regimes },
                "ehrs": { "type": "integer", "minimum": 0 },
                "compositions_per_ehr": { "type": "integer", "minimum": 0 },
                "workers": { "type": "integer", "minimum": 1 },
                "elapsed_s": { "type": "number", "minimum": 0.0 },
                "bulk_load_writes_per_s": { "type": "number", "minimum": 0.0 },
                "whole_loop_ms_per_composition": { "type": "number", "minimum": 0.0 }
            }
        }
    });
    let repetitions = json!({
        "type": "array",
        "minItems": 1,
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["repetition", "phases"],
            "properties": {
                "repetition": { "type": "integer", "minimum": 1 },
                "phases": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["regime", "rate_per_s", "warmup_s", "duration_s",
                                      "planned_measured_arrivals",
                                      "dispatched_measured_arrivals", "warmup_arrivals",
                                      "offered_load_sustained_per_s", "generator_bound",
                                      "operations"],
                        "properties": {
                            "regime": { "enum": regimes },
                            "rate_per_s": { "type": "number", "minimum": 0.0 },
                            "warmup_s": { "type": "integer", "minimum": 0 },
                            "duration_s": { "type": "integer", "minimum": 0 },
                            "planned_measured_arrivals": { "type": "integer", "minimum": 0 },
                            "dispatched_measured_arrivals": { "type": "integer", "minimum": 0 },
                            "warmup_arrivals": { "type": "integer", "minimum": 0 },
                            "offered_load_sustained_per_s": { "type": "number", "minimum": 0.0 },
                            "generator_bound": { "type": "boolean" },
                            "operations": {
                                "type": "object",
                                "propertyNames": bench_operation_names(),
                                "additionalProperties": bench_operation_stats_def()
                            }
                        }
                    }
                },
                "sweeps": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["name", "regime", "workers", "compositions",
                                      "requests_per_composition", "requests", "elapsed_s",
                                      "whole_loop_us_per_request", "operations"],
                        "properties": {
                            "name": { "type": "string", "minLength": 1 },
                            "regime": { "enum": regimes },
                            "workers": { "type": "integer", "minimum": 1 },
                            "compositions": { "type": "integer", "minimum": 0 },
                            "requests_per_composition": { "type": "integer", "minimum": 0 },
                            "requests": { "type": "integer", "minimum": 0 },
                            "elapsed_s": { "type": "number", "minimum": 0.0 },
                            "whole_loop_us_per_request": { "type": "number", "minimum": 0.0 },
                            "operations": {
                                "type": "object",
                                "propertyNames": bench_operation_names(),
                                "additionalProperties": bench_operation_stats_def()
                            }
                        }
                    }
                }
            }
        }
    });
    json!({
        "$schema": DRAFT,
        "$id": urn("bench-result"),
        "title": "Veredictum bench result",
        "description": "One universal-benchmark run: an embedded pack with pinned fixtures, driven against a base URL, seeded once and measured N times open-loop with latency taken from the planned arrival instant. Comparative speed only — never a conformance record, a certificate, or a performance-class rating.",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "boundary_statement", "pack", "target", "environment",
                      "started_at", "finished_at", "scale", "seed_phases", "repetitions", "cross",
                      "methodology", "submittable", "posture"],
        "properties": {
            "schema_version": { "type": "string", "minLength": 1 },
            "boundary_statement": { "const": crate::bench::BOUNDARY_STATEMENT },
            "label": { "type": "string", "minLength": 1 },
            "pack": {
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "version", "description", "max_failed_share", "seed",
                              "fixtures"],
                "properties": {
                    "id": { "type": "string", "pattern": OPTION_TAG_PATTERN },
                    "version": { "type": "string", "minLength": 1 },
                    "description": { "type": "string", "minLength": 1 },
                    "max_failed_share": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "seed": { "type": "integer", "minimum": 0 },
                    "fixtures": {
                        "type": "object",
                        "additionalProperties": { "type": "string", "pattern": "^[0-9a-f]{64}$" }
                    }
                }
            },
            "target": {
                "type": "object",
                "additionalProperties": false,
                "required": ["base_url"],
                "properties": {
                    "base_url": { "type": "string", "minLength": 1 },
                    "sut_version": { "type": "string", "minLength": 1 }
                }
            },
            "environment": {
                "type": "object",
                "additionalProperties": false,
                "required": ["arch", "os"],
                "properties": {
                    "arch": { "type": "string", "minLength": 1 },
                    "os": { "type": "string", "minLength": 1 },
                    "available_parallelism": { "type": "integer", "minimum": 1 },
                    "cpu_model": { "type": "string", "minLength": 1 },
                    "total_memory_bytes": { "type": "integer", "minimum": 1 }
                }
            },
            "started_at": { "type": "string", "minLength": 1 },
            "finished_at": { "type": "string", "minLength": 1 },
            "scale": {
                "type": "object",
                "additionalProperties": false,
                "required": ["factor", "declared_workers", "reference_configuration"],
                "properties": {
                    "factor": { "type": "number", "exclusiveMinimum": 0.0 },
                    "declared_workers": { "type": "boolean" },
                    "reference_configuration": { "type": "boolean" }
                }
            },
            "version_at_time": { "type": "string", "minLength": 1 },
            "seed_phases": seed_phases,
            "repetitions": repetitions,
            "cross": bench_cross_phase_def(&regimes),
            "baselines": {
                "type": "array",
                "items": bench_baseline_def(&regimes, &repetitions, &seed_phases)
            },
            "relative": {
                "type": "array",
                "items": bench_relative_def(&regimes)
            },
            "methodology": {
                "type": "object",
                "additionalProperties": false,
                "required": ["statement", "open_loop", "coordinated_omission_free",
                              "seed_once_measure_n", "repetitions"],
                "properties": {
                    "statement": { "const": crate::bench::METHODOLOGY },
                    "open_loop": { "const": true },
                    "coordinated_omission_free": { "const": true },
                    "seed_once_measure_n": { "const": true },
                    "repetitions": { "type": "integer", "minimum": 1 }
                }
            },
            "submittable": { "type": "boolean" },
            "submittable_unmet": {
                "type": "array",
                "uniqueItems": true,
                "items": {
                    "enum": bench_tokens(&crate::bench::result::SubmissionRequirement::ALL
                        .iter().map(|requirement| requirement.as_str()).collect::<Vec<_>>())
                }
            },
            "posture": bench_posture_def()
        }
    })
}

/// The posture block: the profile a run declared, every disclosed item's
/// declared value, and the bracketing canary evidence behind its
/// verified-or-declared-only label.
#[must_use]
fn bench_posture_def() -> Value {
    use crate::bench::posture::{
        Assurance, AuditSink, AuthnMode, Bracket, CanaryOutcome, CompressionMode, PostureItem,
        SigningScheme, Tenancy, TlsMode, ValidationDepth,
    };
    let tokens = |values: Vec<&'static str>| bench_tokens(&values);
    // The seven item vocabularies share tokens (`none` and `off` each occur in
    // three), and a JSON Schema `enum` holds a SET, so their union is sorted
    // and deduplicated rather than concatenated.
    let union: std::collections::BTreeSet<&'static str> = [
        AuditSink::ALL
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>(),
        SigningScheme::ALL
            .iter()
            .map(|value| value.as_str())
            .collect(),
        ValidationDepth::ALL
            .iter()
            .map(|value| value.as_str())
            .collect(),
        AuthnMode::ALL.iter().map(|value| value.as_str()).collect(),
        TlsMode::ALL.iter().map(|value| value.as_str()).collect(),
        CompressionMode::ALL
            .iter()
            .map(|value| value.as_str())
            .collect(),
        Tenancy::ALL.iter().map(|value| value.as_str()).collect(),
    ]
    .concat()
    .into_iter()
    .collect();
    let declared = tokens(union.into_iter().collect());
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "What was switched on behind these numbers. A run declares exactly one named posture profile out of the set its pack defines, and black-box canaries bracket the measured window: every item is labelled `verified` when both brackets observed the declared value first-hand, and `declared-only` when nothing on the wire discloses it. A canary that contradicted the declaration, or a pair of brackets that disagreed, refused the run, so no such record exists.",
        "required": ["profile", "summary", "items"],
        "properties": {
            "profile": { "type": "string", "minLength": 1 },
            "summary": { "type": "string", "minLength": 1 },
            "comparability": {
                "description": "Every item on which the measured deployment's own configuration departs from the profile named above. A same-machine baseline is composed from somebody else's pinned recipe, so where that recipe configures an item differently the run declares, and the canaries check, what the deployment actually does; this block is how the record says so. Absent for a run that declared its profile as the pack defines it.",
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["item", "profile_declares", "deployment_configures", "source"],
                    "properties": {
                        "item": {
                            "enum": tokens(PostureItem::ALL.iter().map(|item| item.as_str()).collect())
                        },
                        "profile_declares": { "enum": declared.clone() },
                        "deployment_configures": { "enum": declared.clone() },
                        "source": { "type": "string", "minLength": 1 }
                    }
                }
            },
            "items": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["item", "declared", "assurance", "readings"],
                    "properties": {
                        "item": {
                            "enum": tokens(PostureItem::ALL.iter().map(|item| item.as_str()).collect())
                        },
                        "declared": { "enum": declared },
                        "assurance": {
                            "enum": tokens(Assurance::ALL.iter().map(|value| value.as_str()).collect())
                        },
                        "readings": {
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["bracket", "outcome", "observed", "evidence"],
                                "properties": {
                                    "bracket": {
                                        "enum": tokens(Bracket::ALL.iter().map(|value| value.as_str()).collect())
                                    },
                                    "outcome": {
                                        "enum": tokens(CanaryOutcome::ALL.iter().map(|value| value.as_str()).collect())
                                    },
                                    "observed": { "type": "string", "minLength": 1 },
                                    "evidence": { "type": "string", "minLength": 1 }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

/// `bench-packs.json` — the embedded benchmark packs, as the binary describes
/// them.
///
/// The document a rendered legend is generated from: every phase with its
/// discipline, every mix entry with what it probes, every fixture pin with its
/// provenance, and the requirements a record meets before it may be ranked.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one literal JSON-Schema document")]
pub fn bench_packs_schema() -> Value {
    let fixtures = json!({
        "type": "array",
        "minItems": 1,
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["key", "kind", "media_type", "bytes", "sha256", "provenance"],
            "properties": {
                "key": { "type": "string", "minLength": 1 },
                "kind": {
                    "enum": bench_tokens(&crate::bench::pack::FixtureKind::ALL
                        .iter().map(|kind| kind.as_str()).collect::<Vec<_>>())
                },
                "media_type": { "type": "string", "minLength": 1 },
                "bytes": { "type": "integer", "minimum": 1 },
                "sha256": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                "provenance": { "type": "string", "minLength": 1 }
            }
        }
    });
    let seed_phase = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "name", "discipline", "ehrs", "compositions_per_ehr",
                      "compositions", "workers", "fixtures"],
        "properties": {
            "kind": { "const": "seed" },
            "name": { "type": "string", "minLength": 1 },
            "discipline": { "const": "closed-loop" },
            "ehrs": { "type": "integer", "minimum": 0 },
            "compositions_per_ehr": { "type": "integer", "minimum": 0 },
            "compositions": { "type": "integer", "minimum": 0 },
            "workers": { "type": "integer", "minimum": 1 },
            "fixtures": {
                "type": "array",
                "minItems": 1,
                "items": { "type": "string", "minLength": 1 }
            }
        }
    });
    let sweep_phase = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "name", "discipline", "workers", "requests_per_composition",
                      "operations"],
        "properties": {
            "kind": { "const": "sweep" },
            "name": { "type": "string", "minLength": 1 },
            "discipline": { "const": "closed-loop" },
            "workers": { "type": "integer", "minimum": 1 },
            "requests_per_composition": { "type": "integer", "minimum": 1 },
            "operations": {
                "type": "array",
                "minItems": 1,
                "items": bench_operation_names()
            }
        }
    });
    let measure_phase = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "name", "discipline", "rate_per_s", "warmup_s", "duration_s",
                      "planned_arrivals", "planned_measured_arrivals", "mix"],
        "properties": {
            "kind": { "const": "measure" },
            "name": { "type": "string", "minLength": 1 },
            "discipline": { "const": "open-loop" },
            "rate_per_s": { "type": "number", "exclusiveMinimum": 0.0 },
            "warmup_s": { "type": "integer", "minimum": 0 },
            "duration_s": { "type": "integer", "minimum": 1 },
            "planned_arrivals": { "type": "integer", "minimum": 1 },
            "planned_measured_arrivals": { "type": "integer", "minimum": 1 },
            "mix": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["op", "share", "rate_per_s", "rationale"],
                    "properties": {
                        "op": bench_operation_names(),
                        "share": { "type": "integer", "minimum": 1 },
                        "rate_per_s": { "type": "number", "minimum": 0.0 },
                        "rationale": { "type": "string", "minLength": 1 }
                    }
                }
            }
        }
    });
    json!({
        "$schema": DRAFT,
        "$id": urn("bench-packs"),
        "title": "Veredictum benchmark pack manifest",
        "description": "Every benchmark pack this binary embeds: the phases with their load discipline, the operation mix with what each entry probes, the pinned fixtures with their provenance, the seed the arrival streams draw from, and the requirements a record meets before it may be ranked. Emitted by `veredictum bench-packs`, and the only source a published description of a pack may be generated from.",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "boundary_statement", "methodology", "relative_index",
                      "seed_disclosure", "posture_disclosure", "submission_requirements",
                      "operations", "packs"],
        "properties": {
            "schema_version": { "type": "string", "minLength": 1 },
            "boundary_statement": { "const": crate::bench::BOUNDARY_STATEMENT },
            "methodology": { "const": crate::bench::METHODOLOGY },
            "relative_index": { "const": crate::bench::relative::RELATIVE_DERIVATION },
            "seed_disclosure": { "const": crate::bench::manifest::SEED_DISCLOSURE },
            "posture_disclosure": { "const": crate::bench::manifest::POSTURE_DISCLOSURE },
            "submission_requirements": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["token", "statement"],
                    "properties": {
                        "token": {
                            "enum": bench_tokens(&crate::bench::result::SubmissionRequirement::ALL
                                .iter().map(|requirement| requirement.as_str()).collect::<Vec<_>>())
                        },
                        "statement": { "type": "string", "minLength": 1 }
                    }
                }
            },
            "operations": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["token", "wire"],
                    "properties": {
                        "token": bench_operation_names(),
                        "wire": { "type": "string", "pattern": "^(GET|POST) /" }
                    }
                }
            },
            "packs": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id", "version", "description", "max_failed_share", "seed",
                                  "fixtures", "phases", "profiles", "probe_rationales"],
                    "properties": {
                        "id": { "type": "string", "pattern": OPTION_TAG_PATTERN },
                        "version": { "type": "string", "minLength": 1 },
                        "description": { "type": "string", "minLength": 1 },
                        "max_failed_share": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "seed": { "type": "integer", "minimum": 0 },
                        "fixtures": fixtures,
                        "phases": {
                            "type": "array",
                            "minItems": 1,
                            "items": { "oneOf": [seed_phase, sweep_phase, measure_phase] }
                        },
                        "profiles": {
                            "type": "array",
                            "minItems": 1,
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["name", "summary", "default", "declares"],
                                "properties": {
                                    "name": { "type": "string", "pattern": OPTION_TAG_PATTERN },
                                    "summary": { "type": "string", "minLength": 1 },
                                    "default": { "type": "boolean" },
                                    "declares": {
                                        "type": "object",
                                        "minProperties": 1,
                                        "propertyNames": {
                                            "enum": bench_tokens(&crate::bench::posture::PostureItem::ALL
                                                .iter().map(|item| item.as_str()).collect::<Vec<_>>())
                                        },
                                        "additionalProperties": { "type": "string", "minLength": 1 }
                                    }
                                }
                            }
                        },
                        "probe_rationales": {
                            "type": "object",
                            "propertyNames": bench_operation_names(),
                            "additionalProperties": { "type": "string", "minLength": 1 }
                        }
                    }
                }
            }
        }
    })
}

/// The citation a results row names, with its upstream link.
fn arazzo_why_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["source", "section", "quote"],
        "properties": {
            "source": { "type": "string", "minLength": 1 },
            "section": { "type": "string" },
            "quote": { "type": "string" },
            "link": { "type": "string", "minLength": 1 },
            "report": { "type": "integer", "minimum": 1 }
        }
    })
}

/// `arazzo-results.schema.json` — the results of one Arazzo document's run,
/// or of a catalogue run over every document.
#[must_use]
pub fn arazzo_results_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("arazzo-results"),
        "title": "Arazzo run results",
        "description": "One row per workflow row of one Arazzo document, or of every document of a catalogue run, each passed, failed, errored or skipped. A failed or errored row carries the four-part explanation (what was checked, why it is required, expected against received with the exchange, whose problem it is); a skipped row carries one reason from the closed list and its citation. Re-judging the HAR recording of the run writes the same bytes.",
        "type": "object",
        "additionalProperties": false,
        "required": ["document", "engine", "instrument_status", "base_url", "rows"],
        "properties": {
            "document": {
                "type": "string",
                "minLength": 1,
                "description": "The document's name, the prefix of every case id; `catalogue` for a catalogue run, whose case ids each carry their own document's name."
            },
            "engine": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "version"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "version": { "type": "string", "minLength": 1 }
                }
            },
            "instrument_status": instrument_status_def(),
            "base_url": { "type": "string" },
            "rows": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["case", "row", "status", "inputs", "generated"],
                    "properties": {
                        "case": { "type": "string", "pattern": crate::arazzo::gates::published::CASE_ID_PATTERN },
                        "row": { "type": "integer", "minimum": 0 },
                        "status": { "enum": tokens(crate::arazzo::results::Status::ALL) },
                        "inputs": { "type": "object" },
                        "generated": { "type": "array", "items": { "type": "string" } },
                        "skip": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["reason", "why"],
                            "properties": {
                                "reason": { "enum": tokens(crate::arazzo::results::SkipReason::ALL) },
                                "why": arazzo_why_def()
                            }
                        },
                        "explanation": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["step", "what", "expected", "received", "whose"],
                            "properties": {
                                "step": { "type": "string", "minLength": 1 },
                                "what": { "type": "string" },
                                "why": arazzo_why_def(),
                                "expected": { "type": "string" },
                                "received": { "type": "string" },
                                "exchange": {
                                    "type": "object",
                                    "additionalProperties": false,
                                    "required": ["entry", "method", "url", "status"],
                                    "properties": {
                                        "entry": { "type": "integer", "minimum": 0 },
                                        "method": { "type": "string" },
                                        "url": { "type": "string" },
                                        "status": { "type": "integer", "minimum": 0 }
                                    }
                                },
                                "whose": { "enum": tokens(crate::arazzo::results::Whose::ALL) }
                            }
                        }
                    },
                    "allOf": [
                        { "if": { "properties": { "status": { "const": "skipped" } } },
                          "then": { "required": ["skip"] } },
                        { "if": { "properties": { "status": { "enum": ["failed", "errored"] } } },
                          "then": { "required": ["explanation"] } }
                    ]
                }
            }
        }
    })
}

/// The full published set: (file name, schema document).
#[must_use]
pub fn emit_all() -> Vec<(&'static str, Value)> {
    vec![
        ("statement.schema.json", statement_schema()),
        ("results.schema.json", results_schema()),
        ("ixit.schema.json", ixit_schema()),
        ("performance-case.schema.json", performance_case_schema()),
        ("journey-catalogue.schema.json", journey_catalogue_schema()),
        ("perf-payloads.schema.json", perf_payloads_schema()),
        ("stress.schema.json", stress_schema()),
        ("aql-probe.schema.json", aql_probe_schema()),
        ("bench-result.schema.json", bench_result_schema()),
        ("bench-packs.schema.json", bench_packs_schema()),
        (
            "arazzo-extensions.schema.json",
            crate::arazzo::gates::published::extensions_schema(),
        ),
        (
            "arazzo-profile.schema.json",
            crate::arazzo::gates::published::profile_schema(),
        ),
        (
            "arazzo-rows.schema.json",
            crate::arazzo::gates::published::rows_schema(),
        ),
        (
            "arazzo-surface.schema.json",
            crate::arazzo::gates::published::surface_schema(),
        ),
        (
            "arazzo-capabilities.schema.json",
            crate::arazzo::gates::published::capabilities_schema(),
        ),
        ("arazzo-results.schema.json", arazzo_results_schema()),
    ]
}

/// Render a schema document to its canonical published text
/// (two-space pretty print + trailing newline).
#[must_use]
pub fn render(schema: &Value) -> String {
    let mut text = serde_json::to_string_pretty(schema).unwrap_or_default();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_schema_compiles_under_2020_12() {
        for (name, schema) in emit_all() {
            jsonschema::validator_for(&schema)
                .unwrap_or_else(|e| panic!("{name} does not compile: {e}"));
        }
    }

    #[test]
    fn rendering_is_deterministic() {
        let a = render(&statement_schema());
        let b = render(&statement_schema());
        assert_eq!(a, b);
        assert!(a.ends_with('\n'));
    }

    #[test]
    fn no_null_tokens_leak() {
        for (name, schema) in emit_all() {
            let text = render(&schema);
            assert!(
                !text.contains(": null,"),
                "{name} leaked a null vocabulary token"
            );
        }
    }
}
