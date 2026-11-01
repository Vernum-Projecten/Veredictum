// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Deterministic JSON-Schema emission for every document the instrument
//! reads or writes.
//!
//! The party documents, the performance workloads, host prescriptions and
//! records, and the Arazzo catalogue's own formats.
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

use crate::party::{IdentitySource, InstrumentStatus};
use crate::vocab::{FormatName, ItsName, SpecComponent};

/// The schema version stamped into every `$id` (bumps with the schedule
/// release, independently of the product version).
pub const SCHEMA_VERSION: &str = "0.1.0";

const DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// A closed vocabulary whose serde token is a JSON string.
///
/// Implemented only for fieldless enums with a derived `Serialize`, which
/// `serde_json` writes as the variant's (renamed) name, a JSON string
/// (<https://serde.rs/enum-representations.html>). The list below is the whole
/// set, so a type outside it does not compile at a call site.
trait VocabToken: Serialize {}

macro_rules! vocab_tokens {
    ($($vocabulary:ty),* $(,)?) => {
        $(impl VocabToken for $vocabulary {})*
    };
}

vocab_tokens!(
    ItsName,
    FormatName,
    SpecComponent,
    IdentitySource,
    InstrumentStatus,
    crate::ixit::SpecProfile,
    crate::ixit::TerminologyPosture,
    crate::workload::CorpusScale,
    crate::perf_gen::arrivals::ArrivalProcess,
    crate::arazzo::results::Status,
    crate::arazzo::results::SkipReason,
    crate::arazzo::results::Whose,
    crate::arazzo::results::ReviewKind,
    crate::arazzo::structure::Theme,
    crate::arazzo::structure::Tier,
    crate::arazzo::structure::ReportState,
    crate::arazzo::structure::Formalism,
    crate::certificate::Outcome,
);

/// Returns the serde token of one vocabulary member as an owned `String`.
///
/// # Panics
/// Never for a [`VocabToken`]: each implementor serializes as a string.
#[expect(
    clippy::expect_used,
    reason = "every VocabToken implementor is a fieldless enum with a derived Serialize, which \
              serde_json serializes as the variant name, a string; the module tests drive every \
              call site through emit_all"
)]
fn token_str<T: VocabToken>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(Value::String(s)) => Some(s),
        Ok(_) | Err(_) => None,
    }
    .expect("a VocabToken should serialize to a JSON string")
}

/// Returns the serde token of one vocabulary member as a JSON string.
fn token<T: VocabToken>(v: &T) -> Value {
    Value::String(token_str(v))
}

/// Returns the serde tokens of a whole vocabulary.
fn tokens<T: VocabToken>(all: &[T]) -> Value {
    Value::Array(all.iter().map(token).collect())
}

fn urn(name: &str) -> String {
    format!("urn:openehr:cnf:schema:{name}:{SCHEMA_VERSION}")
}

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
                "required": ["capabilities"],
                "properties": {
                    "capabilities": {
                        "description": crate::arazzo::catalogue::CLAIMS_DESCRIPTION,
                        "type": "array",
                        "minItems": 1,
                        "items": { "type": "string", "pattern": IDENT_PATTERN }
                    }
                }
            },
            "tech_profiles": {
                "description": crate::arazzo::catalogue::FORMATS_DESCRIPTION,
                "type": "array",
                "items": tech_profile_def()
            },
            "options": string_array(Some(OPTION_TAG_PATTERN)),
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
                    "db": { "type": "string", "minLength": 1 },
                    "database": { "type": "string", "minLength": 1, "description": "The PostgreSQL database the SUT keeps its data in, inside the `db` container. `perf` reads its sizes and its `pg_stat_statements` table through it; without it those readings are recorded absent." }
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

/// The ixit environment block as a party declares it: a conformance run
/// reads only `exclusive_server`, and a measured run refuses an IXIT lacking
/// any other member, naming it.
fn declared_environment_def() -> Value {
    let mut def = environment_def();
    if let Some(block) = def.as_object_mut() {
        block.remove("required");
        block.insert(
            "description".to_owned(),
            json!("`exclusive_server` is all a conformance run reads. `perf` and `perf-compare` need every other member, and refuse an IXIT lacking one, naming it."),
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

/// The performance schedule's `payloads.yaml` — every request body and
/// operational template the measured instruments send.
#[must_use]
pub fn perf_payloads_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("perf-payloads"),
        "title": "Performance schedule payload manifest",
        "description": "Every payload a workload sends, keyed by the name a workload or the seeder reads it by: its file under payloads/, the operational template's wire identifier where it has one, the digest a reproduced payload is pinned at, and its provenance. A template's `.example` skeleton and `.invalid_twin` sit beside it under the template's key. No openEHR spec governs measured performance (CNF guide master03-overview.adoc: \"Non-functional conformance (performance, etc) is not addressed by this guide\") — our own design.",
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
                "sha256": { "type": "string", "pattern": "^[0-9a-f]{64}$", "description": "The lowercase hex sha256 the file's bytes are pinned at, for a payload reproduced from a published source; loading refuses bytes that hash otherwise." },
                "provenance": { "type": "string", "minLength": 1 }
            }
        }
    })
}

/// A share of a whole, written as a percentage with at most four decimals.
const SHARE_PATTERN: &str = "^(100|[0-9]{1,2})(\\.[0-9]{1,4})?%$";

/// A map of names to shares.
fn share_map(name_pattern: &str) -> Value {
    json!({
        "type": "object",
        "minProperties": 1,
        "propertyNames": { "pattern": name_pattern },
        "additionalProperties": { "type": "string", "pattern": SHARE_PATTERN }
    })
}

/// One stage of a workload interaction.
fn workload_stage_def() -> Value {
    let ops: Vec<&str> = crate::workload::ops::WorkloadOp::ALL
        .iter()
        .map(|op| op.token())
        .collect();
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["op"],
        "properties": {
            "op": { "enum": ops },
            "payload": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
            "query": { "type": "string", "pattern": IDENT_PATTERN }
        }
    })
}

/// A workload document under `perf/workloads/`: the traffic a measured run
/// offers.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one literal JSON-Schema document")]
pub fn perf_workload_schema() -> Value {
    let ops: Vec<&str> = crate::workload::ops::WorkloadOp::ALL
        .iter()
        .map(|op| op.token())
        .collect();
    let transports: Vec<&str> = crate::workload::ops::QueryTransport::ALL
        .iter()
        .map(|t| t.token())
        .collect();
    let transport_pattern = format!("^({})$", transports.join("|"));
    let text = json!({ "type": "string", "minLength": 1 });
    let session = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["think_time_mean_ms", "stages"],
        "properties": {
            "think_time_mean_ms": { "type": "integer", "minimum": 0 },
            "stages": { "type": "array", "minItems": 2, "items": workload_stage_def() }
        }
    });
    let interaction = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["share", "description", "derivation"],
        "properties": {
            "share": { "type": "string", "pattern": SHARE_PATTERN },
            "description": text,
            "derivation": text,
            "op": { "enum": ops },
            "payload": { "type": "string", "pattern": CORPUS_KEY_PATTERN },
            "query": { "type": "string", "pattern": IDENT_PATTERN },
            "session": session
        },
        "oneOf": [
            { "required": ["op"], "not": { "required": ["session"] } },
            { "required": ["session"], "not": { "anyOf": [
                { "required": ["op"] }, { "required": ["payload"] }, { "required": ["query"] } ] } }
        ]
    });
    let parameter = json!({
        "oneOf": [
            { "enum": ["ehr", "other_ehr", "composition"] },
            { "type": "object", "additionalProperties": false, "required": ["int_range"],
              "properties": { "int_range": {
                  "type": "array", "minItems": 2, "maxItems": 2, "items": { "type": "integer" } } } }
        ]
    });
    let query = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["description", "text", "scope", "tier", "transport", "on_refusal", "derivation"],
        "properties": {
            "description": text,
            "text": text,
            "scope": { "enum": ["single_subject", "population"] },
            "tier": { "enum": ["metadata", "content"] },
            "parameters": { "type": "object",
                            "propertyNames": { "pattern": "^[A-Za-z0-9_]+$" },
                            "additionalProperties": parameter },
            "transport": share_map(&transport_pattern),
            "fetch": { "type": "integer", "minimum": 1 },
            "requires": { "type": "array", "items": { "type": "string", "pattern": "^[a-z0-9_-]+$" } },
            "on_refusal": { "enum": ["error", "count_apart"] },
            "derivation": text
        }
    });
    json!({
        "$schema": DRAFT,
        "$id": urn("perf-workload"),
        "title": "Performance workload",
        "description": "The traffic a measured run offers: a seed, a corpus recipe, an arrival process, the plateau qualification limits, the per-plateau sample target, the interactions with their shares of interaction arrivals (one operation, or a partly-open session on one EHR), and the AQL query classes with their transport mix. Every operation token names a released ITS-REST operation. Loading enforces the gates no schema can: shares summing to exactly 100 %, payloads present in perf/payloads.yaml, AQL that parses, requires tokens that resolve against the catalogue, the 10:1 to 50:1 read band, limits no looser than the ruled default, and a planned run of at most 600 s after seeding. No openEHR spec governs measured performance (CNF guide master03-overview.adoc: \"Non-functional conformance (performance, etc) is not addressed by this guide\") — our own design.",
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "version", "status", "description", "seed", "corpus", "interactions"],
        "properties": {
            "id": { "type": "string", "pattern": "^[a-z][a-z0-9-]*$" },
            "version": { "type": "integer", "minimum": 1 },
            "status": { "enum": ["provisional", "ratified"] },
            "description": text,
            "reproduces": {
                "type": "string",
                "minLength": 1,
                "description": "The published harness or benchmark the workload reproduces. Its mix is the harness's, so the population model's read band does not gate it, and every rendering says so."
            },
            "seed": { "type": "integer", "minimum": 0 },
            "corpus": {
                "type": "object",
                "additionalProperties": false,
                "required": ["scale", "compositions_per_ehr", "templates"],
                "properties": {
                    "scale": { "enum": tokens(crate::workload::CorpusScale::ALL) },
                    "ehrs": { "type": "integer", "minimum": 1, "description": "The EHRs to create when a reproduced harness pins a count other than the scale's." },
                    "compositions_per_ehr": { "type": "integer", "minimum": 1 },
                    "templates": share_map(CORPUS_KEY_PATTERN),
                    "commit": { "enum": ["stamped", "verbatim"], "description": "`stamped` (the default) varies event times, composer and the numeric leaves the template allows; `verbatim` commits the example as it is." },
                    "seed_workers": { "type": "integer", "minimum": 1, "description": "The closed-loop seeding workers a reproduced harness pins." },
                    "walk": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["reads"],
                        "description": "A closed-loop walk after seeding: these reads against every seeded composition, each sent after the previous answer, reported apart and labelled closed-loop.",
                        "properties": {
                            "reads": {
                                "type": "array",
                                "minItems": 1,
                                "items": { "enum": crate::workload::ops::WorkloadOp::ALL.iter().map(|op| op.token()).collect::<Vec<_>>() }
                            }
                        }
                    }
                }
            },
            "arrival": { "enum": tokens(crate::perf_gen::arrivals::ArrivalProcess::ALL) },
            "sample_target": { "type": "integer", "minimum": 1 },
            "qualification": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "operation_p99_ms": { "type": "integer", "minimum": 1 },
                    "population_aql_p99_ms": { "type": "integer", "minimum": 1 },
                    "max_error_share": { "type": "string", "pattern": SHARE_PATTERN },
                    "min_achieved_share": { "type": "string", "pattern": SHARE_PATTERN }
                }
            },
            "interactions": {
                "type": "object",
                "minProperties": 1,
                "propertyNames": { "pattern": IDENT_PATTERN },
                "additionalProperties": interaction
            },
            "queries": {
                "type": "object",
                "propertyNames": { "pattern": IDENT_PATTERN },
                "additionalProperties": query
            }
        }
    })
}

/// The manifest `perf-workloads` writes: every pinned workload described.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one literal JSON-Schema document")]
pub fn perf_workloads_schema() -> Value {
    let text = json!({ "type": "string" });
    let share = json!({ "type": "string", "pattern": SHARE_PATTERN });
    let percent = json!({ "type": "string", "pattern": "^[0-9]+\\.[0-9]{2}%$" });
    let sha = json!({ "type": "string", "pattern": "^[0-9a-f]{64}$" });
    let released = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["operation_id", "method", "path", "citation"],
        "properties": {
            "operation_id": text, "method": text, "path": text, "citation": text
        }
    });
    let object = |required: &[&str], properties: Value| {
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": required,
            "properties": properties
        })
    };
    let workload = object(
        &[
            "id",
            "version",
            "label",
            "status",
            "file",
            "sha256",
            "description",
            "seed",
            "arrival",
            "sample_target",
            "qualification",
            "corpus",
            "planned_run",
            "read_write_ratio",
            "mean_operations_per_interaction",
            "interactions",
            "operations",
            "queries",
            "payloads",
        ],
        json!({
            "id": text,
            "version": { "type": "integer", "minimum": 1 },
            "label": text,
            "status": { "enum": ["provisional", "ratified"] },
            "file": text,
            "sha256": sha,
            "description": text,
            "reproduces": text,
            "seed": { "type": "integer", "minimum": 0 },
            "arrival": { "enum": tokens(crate::perf_gen::arrivals::ArrivalProcess::ALL) },
            "sample_target": { "type": "integer", "minimum": 1 },
            "qualification": object(
                &["operation_p99_ms", "population_aql_p99_ms", "max_error_share", "min_achieved_share"],
                json!({
                    "operation_p99_ms": { "type": "integer" },
                    "population_aql_p99_ms": { "type": "integer" },
                    "max_error_share": share,
                    "min_achieved_share": share
                })),
            "corpus": object(
                &["scale", "ehrs", "compositions_per_ehr", "templates"],
                json!({
                    "scale": { "enum": tokens(crate::workload::CorpusScale::ALL) },
                    "ehrs": { "type": "integer" },
                    "compositions_per_ehr": { "type": "integer" },
                    "templates": { "type": "array", "items": object(&["key", "share"],
                        json!({ "key": text, "share": share })) }
                })),
            "planned_run": object(
                &["min_s", "max_s", "session_tail_s", "limit_s"],
                json!({
                    "min_s": { "type": "integer" }, "max_s": { "type": "integer" },
                    "session_tail_s": { "type": "integer" }, "limit_s": { "type": "integer" }
                })),
            "read_write_ratio": { "type": "string", "pattern": "^([0-9]+\\.[0-9]{2}:1|reads only)$" },
            "mean_operations_per_interaction": { "type": "string", "pattern": "^[0-9]+\\.[0-9]{2}$" },
            "interactions": { "type": "array", "items": object(
                &["name", "share", "kind", "stages", "requires", "description", "derivation"],
                json!({
                    "name": text, "share": share, "kind": { "enum": ["single", "session"] },
                    "think_time_mean_ms": { "type": "integer" },
                    "stages": { "type": "array", "items": text },
                    "requires": { "type": "array", "items": text },
                    "description": text, "derivation": text
                })) },
            "operations": { "type": "array", "items": object(
                &["key", "share_of_operations", "write", "released"],
                json!({
                    "key": text, "share_of_operations": percent, "write": { "type": "boolean" },
                    "released": { "type": "array", "items": released }
                })) },
            "queries": { "type": "array", "items": object(
                &["name", "share_of_operations", "scope", "tier", "budget_ms", "text", "parameters",
                  "transport", "requires", "on_refusal", "description", "derivation"],
                json!({
                    "name": text, "share_of_operations": percent,
                    "scope": { "enum": ["single_subject", "population"] },
                    "tier": { "enum": ["metadata", "content"] },
                    "budget_ms": { "type": "integer" },
                    "text": text,
                    "parameters": { "type": "object" },
                    "transport": { "type": "array", "items": object(&["transport", "share", "released"],
                        json!({ "transport": text, "share": share, "released": released })) },
                    "requires": { "type": "array", "items": text },
                    "on_refusal": { "enum": ["error", "count_apart"] },
                    "description": text, "derivation": text
                })) },
            "payloads": { "type": "array", "items": object(
                &["key", "source", "bytes", "sha256", "provenance"],
                json!({
                    "key": text, "source": text, "template_id": text,
                    "bytes": { "type": "integer", "minimum": 0 }, "sha256": sha, "provenance": text
                })) }
        }),
    );
    json!({
        "$schema": DRAFT,
        "$id": urn("perf-workloads"),
        "title": "Performance workload manifest",
        "description": "What `veredictum perf-workloads` writes: every pinned workload with its digest, its expanded per-operation mix with the released ITS-REST operation each entry is sent as, its query classes, its read:write ratio, its planned run range after seeding, and every payload it sends or seeds with its size and digest. Byte-deterministic. No openEHR spec governs measured performance — our own design.",
        "type": "object",
        "additionalProperties": false,
        "required": ["workloads"],
        "properties": {
            "workloads": { "type": "array", "items": workload }
        }
    })
}

/// The tokens of a closed vocabulary, as a JSON array.
fn token_list(tokens: &[&'static str]) -> Value {
    Value::Array(tokens.iter().map(|token| json!(token)).collect())
}

/// The posture block: the profile a run declared, every disclosed item's
/// declared value, and the bracketing canary evidence behind its
/// verified-or-declared-only label.
#[must_use]
fn posture_def() -> Value {
    use crate::perf_emulate::posture::{
        Assurance, AuditSink, AuthnMode, Bracket, CanaryOutcome, CompressionMode, PostureItem,
        SigningScheme, Tenancy, TlsMode, ValidationDepth,
    };
    let tokens = |values: Vec<&'static str>| token_list(&values);
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
        "description": "What was switched on behind these numbers. A run declares exactly one named posture profile, and black-box canaries bracket the run, after seeding and after the last plateau: every item is labelled `verified` when both brackets observed the declared value first-hand, and `declared-only` when nothing on the wire discloses it. A canary that contradicted the declaration, or a pair of brackets that disagreed, refused the run, so no such record exists.",
        "required": ["profile", "summary", "items"],
        "properties": {
            "profile": { "type": "string", "minLength": 1 },
            "summary": { "type": "string", "minLength": 1 },
            "comparability": {
                "description": "Every item on which the measured deployment's own configuration departs from the profile named above. A same-host baseline is composed from somebody else's pinned recipe, so where that recipe configures an item differently the run declares, and the canaries check, what the deployment actually does; this block is how the record says so. Absent for a run that declared its profile as the instrument defines it.",
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
#[expect(clippy::too_many_lines, reason = "one literal JSON-Schema document")]
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
            "specification": arazzo_specification_def(),
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
                                "why": arazzo_why_def(),
                                "detail": {
                                    "type": "string",
                                    "minLength": 1,
                                    "description": "What the reason leaves to say: for `cited-text-absent`, the specification profile and what of the cited text it lacks (the file, the section or the quote); for `development-only`, the development citation the case rests on."
                                }
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
            },
            "server": arazzo_server_def(),
            "outline": arazzo_outline_def(),
            "review": {
                "type": "array",
                "minItems": 1,
                "description": "The run's static review: the statement against what the server stated. No openEHR spec governs the comparison; it is the instrument's own design. A note changes no row's status and no exit status.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "declared", "stated"],
                    "properties": {
                        "kind": {
                            "enum": tokens(crate::arazzo::results::ReviewKind::ALL),
                            "description": "`restapi-version-differs`: the statement's `spec_versions.its_rest` differs, as text, from the `restapi_specs_version` the server stated."
                        },
                        "declared": { "type": "string", "minLength": 1, "description": "The value the statement declares." },
                        "stated": { "type": "string", "minLength": 1, "description": "The value the server stated." }
                    }
                }
            }
        }
    })
}

/// The `specification` member of the Arazzo results: the profile whose text
/// the run's citations were read against.
fn arazzo_specification_def() -> Value {
    use crate::arazzo::spec_profile::SpecProfile;
    let text = || json!({ "type": "string", "minLength": 1 });
    let profiles: Vec<&str> = SpecProfile::ALL.iter().map(|p| p.token()).collect();
    json!({
        "type": "object",
        "required": ["profile"],
        "description": "The specification text the run's citations were read against. `released` is the vendored release, the oracle of every released claim. `development` is the pinned `master` of each openEHR specification repository, the line that carries the newest text, named component by component with its repository, branch and commit; a development result builds no certificate and is never compared with or counted toward a released result. A results document without this member was judged against the release. No openEHR spec governs the profiles; they are the instrument's own design.",
        "properties": {
            "profile": { "enum": profiles },
            "snapshot": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["component", "repository", "branch", "commit"],
                    "properties": {
                        "component": text(),
                        "repository": text(),
                        "branch": text(),
                        "commit": { "type": "string", "pattern": "^[0-9a-f]{40}$" }
                    }
                }
            }
        },
        "oneOf": [
            {
                "additionalProperties": false,
                "properties": { "profile": { "const": "released" } }
            },
            {
                "additionalProperties": false,
                "required": ["snapshot"],
                "properties": { "profile": { "const": "development" }, "snapshot": true }
            }
        ]
    })
}

/// The `outline` member of the Arazzo results: the reading structure every
/// view groups the rows by, read from `catalogue/chapters.yaml`,
/// `capabilities.yaml` and `catalogue/reports.yaml` when the run was planned.
#[expect(clippy::too_many_lines, reason = "one literal JSON-Schema document")]
fn arazzo_outline_def() -> Value {
    use crate::arazzo::structure::{Formalism, ReportState, Theme, Tier};
    let text = || json!({ "type": "string", "minLength": 1 });
    let key = || json!({ "type": "string", "pattern": "^[a-z0-9]+(-[a-z0-9]+)*$" });
    let report = || json!({ "type": "integer", "minimum": 1 });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["chapters", "options", "cases"],
        "description": "The reading structure the catalogue declares, so every view groups the rows without reading the catalogue: the chapters with their capability folders and each folder's BASE sections in reading order, every option with its theme, its tier and the arms the statement selected, the upstream reports the catalogue cites, and where each case sits. A case is BASE when no option arm names its document, and OPTION otherwise. No openEHR spec governs the structure; it is the instrument's own design.",
        "properties": {
            "chapters": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["chapter", "title", "folders"],
                    "properties": {
                        "chapter": key(),
                        "title": text(),
                        "folders": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["folder", "title", "sections"],
                                "properties": {
                                    "folder": text(),
                                    "title": text(),
                                    "formalism": { "type": "boolean" },
                                    "sections": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "additionalProperties": false,
                                            "required": ["section", "title"],
                                            "properties": { "section": key(), "title": text() }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "options": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["option", "description", "theme", "tier", "folders", "arms"],
                    "properties": {
                        "option": text(),
                        "description": text(),
                        "theme": { "enum": tokens(Theme::ALL) },
                        "tier": {
                            "enum": tokens(Tier::ALL),
                            "description": "`recommended`: support is a SHOULD. `options`: a MAY, or behaviour the release leaves to the implementation. `spec-boundary`: an option only because the released text is defective, recorded in the upstream report `report` names."
                        },
                        "report": report(),
                        "folders": { "type": "array", "items": text() },
                        "arms": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["arm", "selected", "documents"],
                                "properties": {
                                    "arm": text(),
                                    "selected": { "type": "boolean" },
                                    "documents": { "type": "array", "items": text() }
                                }
                            }
                        }
                    }
                }
            },
            "reports": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["report", "title", "state"],
                    "properties": {
                        "report": report(),
                        "title": text(),
                        "state": { "enum": tokens(ReportState::ALL) }
                    }
                }
            },
            "cases": {
                "type": "object",
                "propertyNames": { "pattern": crate::arazzo::gates::published::CASE_ID_PATTERN },
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["chapter", "folder", "standing"],
                    "properties": {
                        "chapter": key(),
                        "folder": text(),
                        "standing": {
                            "oneOf": [
                                {
                                    "type": "object",
                                    "additionalProperties": false,
                                    "required": ["kind", "section"],
                                    "properties": { "kind": { "const": "base" }, "section": key() }
                                },
                                {
                                    "type": "object",
                                    "additionalProperties": false,
                                    "required": ["kind", "arms"],
                                    "properties": {
                                        "kind": { "const": "option" },
                                        "arms": {
                                            "type": "array",
                                            "minItems": 1,
                                            "items": {
                                                "type": "object",
                                                "additionalProperties": false,
                                                "required": ["option", "arm"],
                                                "properties": { "option": text(), "arm": text() }
                                            }
                                        }
                                    }
                                }
                            ]
                        },
                        "trial": {
                            "const": true,
                            "description": "Present when a citation of the case rests on a document the vendored release marks TRIAL; every view reads such a case with the options."
                        },
                        "reports": {
                            "type": "array",
                            "minItems": 1,
                            "items": report(),
                            "description": "The upstream reports the case's citations, or the option arm it runs on, name."
                        },
                        "formalism": {
                            "enum": tokens(Formalism::ALL),
                            "description": "The template formalism the case's setups upload, on a folder that badges it."
                        }
                    }
                }
            }
        }
    })
}

/// The `server` member of the Arazzo results: what the server stated through
/// System OPTIONS, and where the answer was read.
fn arazzo_server_def() -> Value {
    json!({
        "type": "object",
        "description": "What the server stated about itself through the ITS-REST System OPTIONS operation (specifications/operations/options.yaml, which says services SHOULD answer), on a catalogue run. `exchange` is the run's own System OPTIONS exchange at recording entry `entry`; `read` is the read the instrument made after the catalogue ran because the run exchanged none, kept in the recording's log; `unanswered` is that read failing at the transport; `not_recorded` is a re-judged recording carrying no answer. The server's `restapi_specs_version` is `answer.options.restapi_specs_version`, present only when the server stated it. Nothing here counts toward conformance.",
        "required": ["read"],
        "oneOf": [
            {
                "additionalProperties": false,
                "required": ["read", "entry", "answer"],
                "properties": {
                    "read": { "const": "exchange" },
                    "entry": { "type": "integer", "minimum": 0 },
                    "answer": identity_read_def()
                }
            },
            {
                "additionalProperties": false,
                "required": ["read", "answer"],
                "properties": {
                    "read": { "const": "read" },
                    "answer": identity_read_def()
                }
            },
            {
                "additionalProperties": false,
                "required": ["read", "error"],
                "properties": {
                    "read": { "const": "unanswered" },
                    "error": { "type": "string", "description": "The transport's own failure." }
                }
            },
            {
                "additionalProperties": false,
                "required": ["read"],
                "properties": { "read": { "const": "not_recorded" } }
            }
        ]
    })
}

/// One classified answer to ITS-REST System OPTIONS
/// (`specifications/operations/options.yaml`; the `Options` document is
/// `specifications/schemas/others/Options.yaml`, every member optional).
fn identity_read_def() -> Value {
    json!({
        "type": "object",
        "required": ["outcome", "status"],
        "oneOf": identity_read_outcomes()
    })
}

/// The three outcomes of [`identity_read_def`], one branch each.
fn identity_read_outcomes() -> Value {
    let member = |description: &str| json!({ "type": "string", "description": description });
    json!([
            {
                "additionalProperties": false,
                "required": ["outcome", "status", "options"],
                "properties": {
                    "outcome": { "const": "offered" },
                    "status": { "const": 200 },
                    "options": {
                        "type": "object",
                        "additionalProperties": false,
                        "description": "The members of the `Options` document the server sent; a member the schema does not name is not carried.",
                        "properties": {
                            "solution": member("The product name."),
                            "solution_version": member("The product version."),
                            "vendor": member("The vendor."),
                            "restapi_specs_version": member("The ITS-REST release the server states it implements."),
                            "conformance_profile": member("The conformance profile the server states."),
                            "endpoints": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "The endpoints the server lists."
                            }
                        }
                    }
                }
            },
            {
                "additionalProperties": false,
                "required": ["outcome", "status"],
                "properties": {
                    "outcome": { "const": "not_offered" },
                    "status": { "type": "integer", "minimum": 100, "maximum": 599, "not": { "minimum": 200, "maximum": 299 } }
                }
            },
            {
                "additionalProperties": false,
                "required": ["outcome", "status", "reason"],
                "properties": {
                    "outcome": { "const": "not_options_document" },
                    "status": { "type": "integer", "minimum": 200, "maximum": 299 },
                    "reason": { "type": "string", "description": "Why the body is not an `Options` document, in the parser's own words." }
                }
            },
            {
                "additionalProperties": false,
                "required": ["outcome", "status"],
                "properties": {
                    "outcome": { "const": "invalid_status" },
                    "status": { "type": "integer", "minimum": 600, "maximum": 999, "description": "A three-digit code outside 100..599, which RFC 9110 §15 makes invalid." }
                }
            }
    ])
}

/// The `sha256:<64 lowercase hex>` digest spelling.
fn sha256_def(description: &str) -> Value {
    json!({
        "type": "string",
        "pattern": "^sha256:[0-9a-f]{64}$",
        "description": description
    })
}

/// A count of result rows.
fn row_count_def(description: &str) -> Value {
    json!({ "type": "integer", "minimum": 0, "description": description })
}

/// `certificate.schema.json` — the payload of a conformance certificate.
#[must_use]
pub fn certificate_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("certificate"),
        "title": "Conformance certificate payload",
        "description": "The payload of a conformance certificate: a JWS (RFC 7515) in the compact serialization, signed with Ed25519 (alg EdDSA, RFC 8037), whose protected header carries `alg`, `typ` (`veredictum-certificate`) and the `kid` of the signing key. The verifying keys are a JWK Set (RFC 7517) published beside the instrument (`jwks.schema.json`). The payload names what was measured and by which instrument, and the SHA-256 digests of the run's results document and HAR recording, so the recorded exchanges a certificate covers are checkable byte for byte.",
        "type": "object",
        "additionalProperties": false,
        "required": ["issued_at", "instrument", "sut", "base_url", "document", "catalogue", "verdict", "exchanges"],
        "properties": {
            "issued_at": {
                "type": "string",
                "format": "date-time",
                "description": "When the certificate was issued (RFC 3339). A key's `expires_at` is compared against it."
            },
            "instrument": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "version", "status"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "version": {
                        "type": "string",
                        "minLength": 1,
                        "description": "The engine version, which both judged the run and issued the certificate."
                    },
                    "status": instrument_status_def()
                }
            },
            "sut": sut_def(),
            "base_url": { "type": "string", "description": "The base URL the run drove." },
            "document": {
                "type": "string",
                "minLength": 1,
                "description": "The results document's own name: `catalogue` for a run over every document, or the one document a narrower run drove."
            },
            "catalogue": {
                "type": "object",
                "additionalProperties": false,
                "required": ["digest"],
                "properties": {
                    "digest": sha256_def("The catalogue revision: the SHA-256 of the `sha256sum` lines of every regular file under the catalogue root, each named by its `/`-separated relative path, sorted by the bytes of that path.")
                }
            },
            "verdict": {
                "type": "object",
                "additionalProperties": false,
                "required": ["outcome", "passed", "failed", "warning", "errored", "skipped"],
                "description": "The verdict summary. The five counts partition the rows of the results document.",
                "properties": {
                    "outcome": {
                        "enum": tokens(crate::certificate::Outcome::ALL),
                        "description": "`green` when no row failed outside the warning ones and no row errored, `red` otherwise."
                    },
                    "passed": row_count_def("Passed rows."),
                    "failed": row_count_def("Failed rows that bear on the verdict."),
                    "warning": row_count_def("Failed rows whose criterion is a SHOULD, which never count against a capability."),
                    "errored": row_count_def("Rows the instrument could not judge."),
                    "skipped": row_count_def("Skipped rows, each with its reason in the results document.")
                }
            },
            "exchanges": {
                "type": "object",
                "additionalProperties": false,
                "required": ["results", "recording"],
                "properties": {
                    "results": sha256_def("The SHA-256 of the run's `results.json`, byte for byte."),
                    "recording": sha256_def("The SHA-256 of the run's HAR recording, byte for byte.")
                }
            }
        }
    })
}

/// `jwks.schema.json` — the JWK Set certificates are verified against.
#[must_use]
pub fn jwks_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("jwks"),
        "title": "Certificate verifying keys",
        "description": "The JWK Set (RFC 7517 §5) a conformance certificate is verified against: Ed25519 public keys as octet key pairs (RFC 8037 §2). `expires_at` and `revoked_at` are this instrument's own members, which a generic JOSE library ignores (RFC 7517 §4); `veredictum verify` honours both. Any other member is refused, and so is the private member `d`, so a misspelled revocation stops the read rather than leaving a revoked key trusted.",
        "type": "object",
        "additionalProperties": false,
        "required": ["keys"],
        "properties": {
            "keys": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kty", "crv", "x", "kid"],
                    "properties": {
                        "kty": { "const": "OKP" },
                        "crv": { "const": "Ed25519" },
                        "x": {
                            "type": "string",
                            "pattern": "^[A-Za-z0-9_-]{43}$",
                            "description": "The 32-byte public key, base64url without padding."
                        },
                        "kid": {
                            "type": "string",
                            "pattern": "^[A-Za-z0-9._-]{1,64}$",
                            "description": "The key id a certificate's header names."
                        },
                        "alg": { "const": "EdDSA" },
                        "use": { "const": "sig" },
                        "expires_at": {
                            "type": "string",
                            "format": "date-time",
                            "description": "The last instant the key may sign (RFC 3339). A certificate whose `issued_at` is later is refused."
                        },
                        "revoked_at": {
                            "type": "string",
                            "format": "date-time",
                            "description": "When the key was revoked (RFC 3339). Every certificate the key signed is refused."
                        }
                    }
                }
            }
        }
    })
}

/// `identity.schema.json` — the document `veredictum identify` prints: what
/// the server answered to ITS-REST System OPTIONS
/// (`specifications/operations/options.yaml`), classified.
#[must_use]
pub fn identity_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("identity"),
        "title": "Server identity",
        "description": "What the server answered to `OPTIONS <base>/`, the ITS-REST System OPTIONS operation (specifications/operations/options.yaml, served on `/` by the released system.openapi.yaml). The operation says services SHOULD answer with a payload revealing details about themselves, so every outcome is an answer and none is a finding: `offered` carries the members of the `Options` document (specifications/schemas/others/Options.yaml, every member optional) the server sent, `not_offered` a status from 100 to 599 outside 2xx, `not_options_document` a 2xx whose body is not an `Options` document, and `invalid_status` a code from 600 to 999, which RFC 9110 §15 makes invalid.",
        "type": "object",
        "required": ["outcome", "status"],
        "oneOf": identity_read_outcomes()
    })
}

/// One quantile of a `perf-result`: resolved with its interval, or not
/// resolvable.
fn perf_quantile_def() -> Value {
    let ms = json!({ "type": "number", "minimum": 0.0 });
    let rank = json!({ "type": "integer", "minimum": 1 });
    json!({
        "type": "object",
        "required": ["state"],
        "oneOf": [
            {
                "additionalProperties": false,
                "required": ["state", "value_ms", "rank", "low_ms", "low_rank", "high_ms", "high_rank"],
                "properties": {
                    "state": { "const": "resolved" },
                    "value_ms": ms, "rank": rank,
                    "low_ms": ms, "low_rank": rank,
                    "high_ms": ms, "high_rank": rank
                }
            },
            {
                "additionalProperties": false,
                "required": ["state", "samples_above"],
                "properties": {
                    "state": { "const": "not_resolvable" },
                    "samples_above": { "type": "integer", "minimum": 0, "description": "Fewer than 10 samples lie above the quantile, so it is not reported." }
                }
            }
        ]
    })
}

fn perf_error_counts_def() -> Value {
    let mut properties = serde_json::Map::new();
    for class in crate::perf_gen::ErrorClass::ALL {
        let _replaced = properties.insert(
            class.token().to_owned(),
            json!({ "type": "integer", "minimum": 1 }),
        );
    }
    json!({ "type": "object", "additionalProperties": false, "properties": properties })
}

fn perf_latency_def() -> Value {
    let q = perf_quantile_def();
    let mut properties = serde_json::Map::new();
    for name in crate::perf_result::quantile::QuantileName::ALL {
        let _replaced = properties.insert(name.token().to_owned(), q.clone());
    }
    let _replaced = properties.insert(
        "max_ms".to_owned(),
        json!({ "type": "number", "minimum": 0.0 }),
    );
    let required: Vec<&str> = crate::perf_result::quantile::QuantileName::ALL
        .iter()
        .map(|n| n.token())
        .collect();
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties
    })
}

fn perf_raw_window_def() -> Value {
    let hdr = json!({ "type": "string", "minLength": 1, "description": "The standard HdrHistogram V2 serialization, base64, values in microseconds." });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["start_ms", "end_ms", "operations", "lag_hdr", "in_flight", "budget_wait_ms", "budget_exhausted"],
        "properties": {
            "start_ms": { "type": "integer", "minimum": 0 },
            "end_ms": { "type": "integer", "minimum": 1 },
            "operations": {
                "type": "object",
                "propertyNames": { "pattern": "^\\S+$" },
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["ok_hdr"],
                    "properties": {
                        "ok_hdr": hdr,
                        "failed_hdr": hdr,
                        "errors": perf_error_counts_def(),
                        "refused": { "type": "integer", "minimum": 1, "description": "Refusals of an optional operation the workload counts apart from errors (`on_refusal: count_apart`)." }
                    }
                }
            },
            "lag_hdr": hdr,
            "in_flight": { "type": "array", "items": { "type": "integer", "minimum": 0 } },
            "budget_wait_ms": { "type": "integer", "minimum": 0 },
            "budget_exhausted": { "type": "boolean" }
        }
    })
}

fn perf_throughput_def() -> Value {
    let rate = json!({ "type": "number", "minimum": 0.0 });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["offered_per_s", "achieved_per_s", "requests", "succeeded", "mean_in_flight"],
        "properties": {
            "offered_per_s": rate,
            "achieved_per_s": rate,
            "requests": { "type": "integer", "minimum": 0 },
            "succeeded": { "type": "integer", "minimum": 0 },
            "refused": { "type": "integer", "minimum": 1, "description": "Refusals counted apart from errors." },
            "errors": perf_error_counts_def(),
            "error_share": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
            "mean_in_flight": { "type": "number", "minimum": 0.0, "description": "Little's law: the rate of the timed exchanges times their mean latency." }
        }
    })
}

fn perf_generator_reading_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["budget_exhausted", "generator_bound"],
        "properties": {
            "lag_p99_ms": { "type": "number", "minimum": 0.0 },
            "lag_max_ms": { "type": "number", "minimum": 0.0 },
            "budget_exhausted": { "type": "boolean" },
            "generator_bound": { "type": "boolean", "description": "The lag p99 exceeded the generator's limit while the in-flight budget never ran out: the generator, not the server, failed to hold the rate." }
        }
    })
}

#[expect(clippy::too_many_lines, reason = "one literal JSON-Schema document")]
fn perf_result_raw_def() -> Value {
    let rate = json!({ "type": "number", "exclusiveMinimum": 0.0 });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["instrument", "target", "environment", "generator", "workload", "corpus", "seeding", "mix", "qualification", "seed", "plateaus"],
        "properties": {
            "instrument": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "version"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "version": { "type": "string", "minLength": 1 }
                }
            },
            "target": {
                "type": "object",
                "additionalProperties": false,
                "required": ["base_url", "options"],
                "properties": {
                    "base_url": { "type": "string", "minLength": 1 },
                    "options": {
                        "type": "object",
                        "required": ["read"],
                        "description": "The answer to ITS-REST System OPTIONS (specifications/operations/options.yaml), or the transport failure that prevented one.",
                        "oneOf": [
                            {
                                "additionalProperties": false,
                                "required": ["read", "answer"],
                                "properties": { "read": { "const": "answered" }, "answer": identity_read_def() }
                            },
                            {
                                "additionalProperties": false,
                                "required": ["read", "error"],
                                "properties": { "read": { "const": "unanswered" }, "error": { "type": "string" } }
                            }
                        ]
                    }
                }
            },
            "environment": environment_def(),
            "generator": {
                "type": "object",
                "additionalProperties": false,
                "required": ["host", "in_flight_budget", "lag_limit_us", "workers"],
                "properties": {
                    "host": {
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
                    "in_flight_budget": { "type": "integer", "minimum": 1 },
                    "lag_limit_us": { "type": "integer", "minimum": 0 },
                    "workers": { "type": "integer", "minimum": 1 }
                }
            },
            "host_prescription": host_prescription_def(),
            "network_leg": network_leg_def(),
            "interleaving": interleaving_def(),
            "aa_floor": aa_floor_def(),
            "posture": posture_def(),
            "workload": {
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "label", "version", "sha256", "document"],
                "properties": {
                    "id": { "type": "string", "minLength": 1 },
                    "label": { "type": "string", "pattern": "^(custom|[a-z0-9][a-z0-9-]*@[0-9]+)$", "description": "`<id>@<version>` for a workload the instrument pins by digest, `custom` for any other document." },
                    "version": { "type": "string", "minLength": 1 },
                    "sha256": sha256_def("The digest of `document`."),
                    "document": { "type": "string", "minLength": 1 }
                }
            },
            "corpus": {
                "type": "object",
                "additionalProperties": false,
                "required": ["scale", "recipe_sha256", "digest", "origin", "ehrs", "compositions"],
                "properties": {
                    "scale": { "enum": tokens(crate::workload::CorpusScale::ALL) },
                    "recipe_sha256": sha256_def("The digest of the recipe the corpus was seeded from."),
                    "digest": sha256_def("The digest of the seeded corpus listing."),
                    "origin": { "enum": ["seeded", "reused"] },
                    "ehrs": { "type": "integer", "minimum": 0, "description": "The EHRs the server held when the clock started, as read back." },
                    "compositions": { "type": "integer", "minimum": 0, "description": "The compositions the server held when the clock started, as read back." }
                }
            },
            "seeding": {
                "type": "object",
                "additionalProperties": false,
                "required": ["model", "workers", "requests", "wall_ms"],
                "description": "The seeding before the clock, a closed-loop bulk load reported apart from every measured number.",
                "properties": {
                    "model": { "const": "closed_loop" },
                    "workers": { "type": "integer", "minimum": 1 },
                    "requests": { "type": "integer", "minimum": 0 },
                    "wall_ms": { "type": "integer", "minimum": 0 }
                }
            },
            "walk": {
                "type": "object",
                "additionalProperties": false,
                "required": ["model", "reads", "requests", "wall_ms"],
                "description": "The closed-loop read walk after seeding, reported apart from every measured number.",
                "properties": {
                    "model": { "const": "closed_loop" },
                    "reads": { "type": "array", "minItems": 1, "items": { "type": "string", "minLength": 1 } },
                    "requests": { "type": "integer", "minimum": 0 },
                    "failed": { "type": "integer", "minimum": 1 },
                    "wall_ms": { "type": "integer", "minimum": 0 }
                }
            },
            "mix": {
                "type": "object",
                "additionalProperties": false,
                "required": ["operations_per_interaction"],
                "properties": {
                    "operations_per_interaction": { "type": "number", "exclusiveMinimum": 0.0, "description": "Planned rates count operation arrivals per second; the generator draws interactions at the rate divided by this." },
                    "dropped": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["interaction", "share", "missing"],
                            "properties": {
                                "interaction": { "type": "string", "minLength": 1 },
                                "share": { "type": "string", "minLength": 1 },
                                "missing": { "type": "array", "minItems": 1, "items": { "type": "string", "minLength": 1 } }
                            }
                        }
                    }
                }
            },
            "qualification": {
                "type": "object",
                "additionalProperties": false,
                "required": ["default_budget_ms", "max_error_ppm", "min_achieved_ppm"],
                "properties": {
                    "default_budget_ms": { "type": "integer", "minimum": 1 },
                    "budgets_ms": { "type": "object", "propertyNames": { "pattern": "^\\S+$" }, "additionalProperties": { "type": "integer", "minimum": 1 } },
                    "max_error_ppm": { "type": "integer", "minimum": 0, "maximum": 1_000_000 },
                    "min_achieved_ppm": { "type": "integer", "minimum": 0, "maximum": 1_000_000 }
                }
            },
            "seed": { "type": "integer", "minimum": 0 },
            "phases": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["phase", "planned_ms", "wall_ms", "exchanges"],
                    "properties": {
                        "phase": { "enum": ["settle", "warm_up", "scout", "drain", "ladder", "extra_plateau", "closing"] },
                        "planned_ms": { "type": "integer", "minimum": 0 },
                        "wall_ms": { "type": "integer", "minimum": 0 },
                        "exchanges": { "type": "integer", "minimum": 0 },
                        "errors": perf_error_counts_def()
                    }
                }
            },
            "scout": {
                "type": "object",
                "additionalProperties": false,
                "required": ["start_rate", "end_rate", "window_ms", "windows"],
                "properties": {
                    "start_rate": rate,
                    "end_rate": rate,
                    "halted_at_ms": { "type": "integer", "minimum": 0 },
                    "window_ms": { "type": "integer", "minimum": 1 },
                    "windows": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["planned_rate", "window"],
                            "properties": { "planned_rate": rate, "window": perf_raw_window_def() }
                        }
                    }
                }
            },
            "server_settings": perf_reading_def(
                "The database server's settings, read once before the settle: every setting not at its built-in default, and the planner, memory, WAL and parallelism settings whatever their source.",
                &json!({
                    "server_version": { "type": "string", "minLength": 1 },
                    "settings": {
                        "type": "object",
                        "additionalProperties": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["setting", "source"],
                            "properties": {
                                "setting": { "type": "string" },
                                "unit": { "type": "string" },
                                "source": { "type": "string", "minLength": 1 }
                            }
                        }
                    }
                }),
            ),
            "plateaus": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["planned_rate", "ramp_in_ms", "hold_ms", "halves"],
                    "properties": {
                        "planned_rate": rate,
                        "ramp_in_ms": { "type": "integer", "minimum": 0 },
                        "hold_ms": { "type": "integer", "minimum": 1 },
                        "halves": { "type": "array", "minItems": 2, "maxItems": 2, "items": perf_raw_window_def() },
                        "stopped_at_ms": { "type": "integer", "minimum": 0 },
                        "multiple": { "type": "number", "exclusiveMinimum": 0.0, "description": "The planned rate as a multiple of the scout's knee estimate." },
                        "server": perf_server_readings_def(),
                        "failed_exchanges": {
                            "type": "object",
                            "propertyNames": { "pattern": "^\\S+$" },
                            "additionalProperties": {
                                "type": "array",
                                "minItems": 1,
                                "maxItems": crate::perf_result::FAILED_EXCHANGES_PER_OPERATION,
                                "items": {
                                    "type": "object",
                                    "additionalProperties": false,
                                    "required": ["request_line", "body", "truncated"],
                                    "properties": {
                                        "status": { "type": "integer", "minimum": 100, "maximum": 999 },
                                        "request_line": { "type": "string", "maxLength": crate::perf_gen::REQUEST_LINE_LIMIT },
                                        "body": { "type": "string" },
                                        "truncated": { "type": "boolean" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

/// A server-side reading: `present` with `properties`, or `absent` with
/// the reason.
fn perf_reading_def(description: &str, properties: &Value) -> Value {
    let mut present = serde_json::Map::new();
    let _state = present.insert("state".to_owned(), json!({ "const": "present" }));
    let mut required = vec![json!("state")];
    if let Some(members) = properties.as_object() {
        for (name, def) in members {
            let _member = present.insert(name.clone(), def.clone());
            required.push(json!(name));
        }
    }
    json!({
        "description": description,
        "type": "object",
        "required": ["state"],
        "oneOf": [
            { "additionalProperties": false, "required": required, "properties": present },
            {
                "additionalProperties": false,
                "required": ["state", "reason"],
                "properties": { "state": { "const": "absent" }, "reason": { "type": "string", "minLength": 1 } }
            }
        ]
    })
}

/// The server-side readings of one plateau's hold.
fn perf_server_readings_def() -> Value {
    let count = json!({ "type": "integer", "minimum": 0 });
    let sizes = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["database_bytes", "schemas"],
        "properties": {
            "database_bytes": count,
            "schemas": { "type": "object", "additionalProperties": count }
        }
    });
    let ms = json!({ "type": "number", "minimum": 0.0 });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["containers", "database", "statements"],
        "description": "The server-side readings over the hold: context that never bears on qualification. Each is present, or absent with its reason.",
        "properties": {
            "containers": perf_reading_def(
                "The declared containers' cumulative counters from the Docker Engine API stats endpoint, read at a fixed interval from the hold's start and once at its end. A failed reading ends that container's series, with the reason.",
                &json!({
                    "series": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["role", "name", "interval_ms", "samples"],
                            "properties": {
                                "role": { "enum": ["sut", "db"] },
                                "name": { "type": "string", "minLength": 1 },
                                "interval_ms": { "type": "integer", "minimum": 1 },
                                "samples": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "additionalProperties": false,
                                        "required": ["offset_ms", "cpu_ns", "rss_bytes", "blk_read_bytes", "blk_write_bytes", "net_rx_bytes", "net_tx_bytes"],
                                        "properties": {
                                            "offset_ms": count, "cpu_ns": count, "rss_bytes": count,
                                            "blk_read_bytes": count, "blk_write_bytes": count,
                                            "net_rx_bytes": count, "net_tx_bytes": count
                                        }
                                    }
                                },
                                "truncated": {
                                    "type": "object",
                                    "additionalProperties": false,
                                    "required": ["at_ms", "reason"],
                                    "properties": { "at_ms": count, "reason": { "type": "string", "minLength": 1 } }
                                }
                            }
                        }
                    }
                }),
            ),
            "database": perf_reading_def(
                "The database's pg_database_size and per-schema pg_total_relation_size at the hold's start and end.",
                &json!({
                    "database": { "type": "string", "minLength": 1 },
                    "start": sizes,
                    "end": sizes
                }),
            ),
            "statements": perf_reading_def(
                "The statements pg_stat_statements counted over the hold, by execution and planning time, costliest first.",
                &json!({
                    "statements": {
                        "type": "array",
                        "maxItems": crate::perf_result::server::STATEMENTS_PER_PLATEAU,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["query", "calls", "total_exec_ms", "total_plan_ms", "rows", "shared_blks_hit", "shared_blks_read"],
                            "properties": {
                                "query": { "type": "string", "maxLength": crate::perf_result::server::STATEMENT_TEXT_LIMIT },
                                "calls": { "type": "integer", "minimum": 1 },
                                "total_exec_ms": ms, "total_plan_ms": ms,
                                "rows": count, "shared_blks_hit": count, "shared_blks_read": count
                            }
                        }
                    }
                }),
            )
        }
    })
}

/// A plateau's server-side readings, summarized.
fn perf_server_summary_def() -> Value {
    let count = json!({ "type": "integer", "minimum": 0 });
    let pct = json!({ "type": "number", "minimum": 0.0 });
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "The server-side readings summarized from the raw series alone: context that never bears on qualification.",
        "properties": {
            "containers": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["role", "name", "samples", "truncated"],
                    "properties": {
                        "role": { "enum": ["sut", "db"] },
                        "name": { "type": "string", "minLength": 1 },
                        "samples": count,
                        "truncated": { "type": "boolean" },
                        "cpu_mean_pct": pct, "cpu_max_pct": pct,
                        "rss_max_bytes": count,
                        "blk_read_bytes": count, "blk_write_bytes": count,
                        "net_rx_bytes": count, "net_tx_bytes": count
                    }
                }
            },
            "database_growth_bytes": { "type": "integer" }
        }
    })
}

fn perf_qualification_def() -> Value {
    let judged: Vec<&str> = vec!["p99", "p95", "p90", "p50", "none"];
    let share = json!({ "type": "number", "minimum": 0.0, "maximum": 1.0 });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["qualified"],
        "description": "A plateau qualifies when its successes are at least the achieved share of the exchanges not refused, its errors at most the error share, every operation within its budget at its judged quantile, and the generator neither bound nor stopped.",
        "properties": {
            "qualified": { "type": "boolean" },
            "shortfalls": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["criterion"],
                    "oneOf": [
                        { "additionalProperties": false, "properties": { "criterion": { "const": "no_exchanges" } } },
                        { "additionalProperties": false, "required": ["share"], "properties": { "criterion": { "const": "achieved" }, "share": share } },
                        { "additionalProperties": false, "required": ["share"], "properties": { "criterion": { "const": "error_share" }, "share": share } },
                        {
                            "additionalProperties": false,
                            "required": ["operation", "judged_at", "value_ms", "budget_ms"],
                            "properties": {
                                "criterion": { "const": "latency" },
                                "operation": { "type": "string", "pattern": "^\\S+$" },
                                "judged_at": { "enum": judged },
                                "value_ms": { "type": "number", "minimum": 0.0 },
                                "budget_ms": { "type": "integer", "minimum": 1 }
                            }
                        },
                        { "additionalProperties": false, "properties": { "criterion": { "const": "generator_bound" } } },
                        { "additionalProperties": false, "properties": { "criterion": { "const": "stopped" } } }
                    ]
                }
            }
        }
    })
}

fn perf_knee_def() -> Value {
    let rate = json!({ "type": "number", "exclusiveMinimum": 0.0 });
    let achieved = json!({ "type": "number", "minimum": 0.0 });
    json!({
        "description": "The highest offered rate whose plateau and every lower one qualify: a bracket up to the first failing plateau, a lower bound when the top plateau qualifies, or below the ladder when the lowest fails.",
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["bound", "qualified_rate", "achieved_per_s", "failing_rate"],
                "properties": { "bound": { "const": "bracket" }, "qualified_rate": rate, "achieved_per_s": achieved, "failing_rate": rate }
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["bound", "qualified_rate", "achieved_per_s"],
                "properties": { "bound": { "const": "lower_bound" }, "qualified_rate": rate, "achieved_per_s": achieved }
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["bound", "failing_rate"],
                "properties": { "bound": { "const": "below_ladder" }, "failing_rate": rate }
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["bound"],
                "properties": { "bound": { "const": "no_plateau" } }
            }
        ]
    })
}

fn perf_usl_def() -> Value {
    json!({
        "description": "The Universal Scalability Law X(N) = γN / (1 + α(N − 1) + βN(N − 1)) fitted to achieved throughput against mean in flight over the scout windows and the plateaus: an explanatory model, never a verdict. The fit is pinned arithmetic, so `perf-judge` reproduces it.",
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["state", "alpha", "beta", "gamma", "r_squared", "points", "iterations"],
                "properties": {
                    "state": { "const": "fitted" },
                    "alpha": { "type": "number", "minimum": 0.0, "exclusiveMaximum": 1.0 },
                    "beta": { "type": "number", "minimum": 0.0 },
                    "gamma": { "type": "number" },
                    "n_star": { "type": "number", "exclusiveMinimum": 0.0 },
                    "predicted_max_per_s": { "type": "number" },
                    "r_squared": { "type": "number" },
                    "points": { "type": "integer", "minimum": crate::perf_result::capacity::USL_MIN_POINTS },
                    "iterations": { "const": crate::perf_result::capacity::USL_ITERATIONS }
                }
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["state", "points", "reason"],
                "properties": {
                    "state": { "const": "not_fitted" },
                    "points": { "type": "integer", "minimum": 0 },
                    "reason": { "type": "string", "minLength": 1 }
                }
            }
        ]
    })
}

fn perf_population_def() -> Value {
    let pair = json!({ "type": "array", "minItems": 2, "maxItems": 2, "items": { "type": "number", "exclusiveMinimum": 0.0 } });
    json!({
        "description": "The population the knee could serve at its peak: the knee divided by 13 to 75 peak API arrivals per second per 100,000 population, read at the run's corpus scale and environment.",
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["state", "knee_rate", "lower_bound", "arrivals_per_100k", "population", "corpus_scale", "corpus_ehrs", "environment", "derivation"],
                "properties": {
                    "state": { "const": "read" },
                    "knee_rate": { "type": "number", "exclusiveMinimum": 0.0 },
                    "lower_bound": { "type": "boolean" },
                    "arrivals_per_100k": pair.clone(),
                    "population": pair,
                    "corpus_scale": { "enum": tokens(crate::workload::CorpusScale::ALL) },
                    "corpus_ehrs": { "type": "integer", "minimum": 1 },
                    "environment": { "type": "string", "minLength": 1 },
                    "derivation": { "const": crate::perf_result::capacity::POPULATION_DERIVATION }
                }
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["state", "reason"],
                "properties": { "state": { "const": "not_read" }, "reason": { "type": "string", "minLength": 1 } }
            }
        ]
    })
}

#[expect(clippy::too_many_lines, reason = "one literal JSON-Schema document")]
fn perf_result_derived_def() -> Value {
    let judged: Vec<&str> = vec!["p99", "p95", "p90", "p50", "none"];
    let ratio = json!({ "type": "number", "exclusiveMinimum": 0.0 });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["passes", "between_run_noise", "scope", "seeding", "plateaus", "knee", "usl", "population", "host"],
        "properties": {
            "passes": { "const": 1 },
            "between_run_noise": {
                "description": "`unmeasured` for one pass, which measures sampling intervals only; the A/A floor when an interleaved A/A comparison is attached.",
                "oneOf": [
                    { "const": "unmeasured" },
                    {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["aa_floor"],
                        "properties": { "aa_floor": aa_floor_def() }
                    }
                ]
            },
            "host": host_reading_def(),
            "scope": { "const": crate::perf_result::SCOPE },
            "seeding": {
                "type": "object",
                "additionalProperties": false,
                "required": ["model"],
                "properties": {
                    "model": { "const": "closed_loop" },
                    "requests_per_s": { "type": "number", "minimum": 0.0 }
                }
            },
            "walk": {
                "type": "object",
                "additionalProperties": false,
                "required": ["model"],
                "description": "The closed-loop walk's whole-loop average, the figure a single-client harness reports.",
                "properties": {
                    "model": { "const": "closed_loop" },
                    "mean_us_per_request": { "type": "number", "minimum": 0.0 }
                }
            },
            "knee": perf_knee_def(),
            "peak": {
                "type": "object",
                "additionalProperties": false,
                "required": ["achieved_per_s", "source", "index"],
                "description": "The highest achieved throughput of the run, whatever its latency.",
                "properties": {
                    "achieved_per_s": { "type": "number", "exclusiveMinimum": 0.0 },
                    "source": { "enum": ["plateau", "scout"] },
                    "index": { "type": "integer", "minimum": 0 }
                }
            },
            "usl": perf_usl_def(),
            "population": perf_population_def(),
            "scout_estimate": {
                "type": "object",
                "additionalProperties": false,
                "required": ["rate"],
                "description": "K̂, the knee estimate the ladder's rates are multiples of: the midpoint rate of the first breaching scout window, or the ramp's end rate as a lower bound when no window breached.",
                "properties": {
                    "rate": { "type": "number", "exclusiveMinimum": 0.0 },
                    "breached_window": { "type": "integer", "minimum": 0 }
                }
            },
            "scout": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["start_ms", "planned_rate", "throughput", "latency", "generator"],
                    "properties": {
                        "start_ms": { "type": "integer", "minimum": 0 },
                        "planned_rate": { "type": "number", "exclusiveMinimum": 0.0 },
                        "throughput": perf_throughput_def(),
                        "latency": perf_latency_def(),
                        "generator": perf_generator_reading_def(),
                        "qualification": perf_qualification_def()
                    }
                }
            },
            "plateaus": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["planned_rate", "hold_s", "throughput", "latency", "operations", "halves", "generator", "stopped", "qualification"],
                    "properties": {
                        "planned_rate": { "type": "number", "exclusiveMinimum": 0.0 },
                        "hold_s": { "type": "number", "exclusiveMinimum": 0.0 },
                        "throughput": perf_throughput_def(),
                        "latency": perf_latency_def(),
                        "operations": {
                            "type": "object",
                            "propertyNames": { "pattern": "^\\S+$" },
                            "additionalProperties": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["requests", "succeeded", "share", "latency", "judged_at", "budget_ms"],
                                "properties": {
                                    "requests": { "type": "integer", "minimum": 0 },
                                    "succeeded": { "type": "integer", "minimum": 0 },
                                    "refused": { "type": "integer", "minimum": 1 },
                                    "budget_ms": { "type": "integer", "minimum": 1, "description": "The latency budget at the judged quantile." },
                                    "errors": perf_error_counts_def(),
                                    "error_share": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                                    "share": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                                    "latency": perf_latency_def(),
                                    "judged_at": { "enum": judged, "description": "The highest of p99, p95, p90 and p50 that resolves." }
                                }
                            }
                        },
                        "halves": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["steadiness"],
                            "properties": {
                                "p50_ratio": ratio,
                                "p95_ratio": ratio,
                                "steadiness": { "enum": ["steady", "unsteady", "not_measurable"], "description": "Unsteady when the second half's overall p50 or p95 over the first's lies outside 0.8 to 1.25." }
                            }
                        },
                        "generator": perf_generator_reading_def(),
                        "stopped": { "type": "boolean" },
                        "qualification": perf_qualification_def(),
                        "server": perf_server_summary_def()
                    }
                }
            }
        }
    })
}

/// `perf-result.schema.json`: one performance run's record, its evidence
/// and every number derived from it.
#[must_use]
pub fn perf_result_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("perf-result"),
        "title": "Veredictum performance result",
        "description": "One performance run against one server: the `raw` evidence (the target's System OPTIONS answer, the environment, the generator, the embedded workload and corpus, the seed, the scout windows and per plateau the encoded HDR histograms per hold half and operation) and the `derived` numbers, which `veredictum perf-judge` recomputes from `raw` byte for byte. Percentiles come from summed histograms, never an average of percentiles. No openEHR specification governs performance; this is the instrument's own design.",
        "type": "object",
        "additionalProperties": false,
        "required": ["family", "raw", "derived"],
        "properties": {
            "family": { "const": "perf-result" },
            "raw": perf_result_raw_def(),
            "derived": perf_result_derived_def()
        }
    })
}

fn network_leg_def() -> Value {
    json!({
        "enum": ["same-host", "same-zone", "cross-region"],
        "description": "Where the load generator sits relative to the server; a record naming no leg reads as same-host."
    })
}

fn host_prescription_properties() -> Value {
    json!({
        "id": { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]*$", "description": "The prescription's id, its file stem under perf/hosts/." },
        "version": { "type": "integer", "minimum": 1 },
        "description": { "type": "string", "minLength": 1 },
        "derivation": { "type": "string", "minLength": 1, "description": "Where the prescription comes from; a field the source does not name is `unstated`." },
        "machine_class": { "enum": ["bare_metal", "virtual_machine", "hosted_ci_runner", "unstated"] },
        "vcpu": { "type": "integer", "minimum": 1 },
        "memory_gib": { "type": "integer", "minimum": 1 },
        "storage_class": { "enum": ["nvme", "ssd", "network_block", "hdd", "unstated"] },
        "container_ceilings": {
            "type": "object",
            "additionalProperties": false,
            "required": ["cpus", "memory_gib"],
            "properties": {
                "cpus": { "type": "number", "exclusiveMinimum": 0.0 },
                "memory_gib": { "type": "integer", "minimum": 1 }
            }
        },
        "network_leg": network_leg_def(),
        "idle": { "type": "boolean", "description": "Whether the host runs nothing but the measurement." }
    })
}

fn host_prescription_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "version", "description", "derivation", "machine_class", "vcpu", "memory_gib", "storage_class", "network_leg", "idle"],
        "properties": host_prescription_properties()
    })
}

/// `perf-host.schema.json`: one host prescription under `perf/hosts/`.
#[must_use]
pub fn perf_host_schema() -> Value {
    json!({
        "$schema": DRAFT,
        "$id": urn("perf-host"),
        "title": "Veredictum host prescription",
        "description": "The machine a performance record claims to have run on: machine class, vCPU, memory, storage class, container ceilings and network leg. A record embeds the prescription it claims and its derived part reads the generator's measured fingerprint against it. No openEHR specification governs performance; this is the instrument's own design.",
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "version", "description", "derivation", "machine_class", "vcpu", "memory_gib", "storage_class", "network_leg", "idle"],
        "properties": host_prescription_properties()
    })
}

fn compare_metric_tokens() -> Vec<&'static str> {
    crate::perf_result::compare::CompareMetric::ALL
        .iter()
        .map(|m| m.token())
        .collect()
}

fn aa_floor_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ratios", "pairs"],
        "description": "The A/A floor: per metric, the largest ratio of the larger side over the smaller in an interleaved run of one target against itself.",
        "properties": {
            "ratios": {
                "type": "object",
                "propertyNames": { "enum": compare_metric_tokens() },
                "additionalProperties": { "type": "number", "minimum": 1.0 }
            },
            "pairs": { "type": "integer", "minimum": 0 }
        }
    })
}

fn slot_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["rung", "planned_rate", "side"],
        "properties": {
            "rung": { "type": "integer", "minimum": 0 },
            "planned_rate": { "type": "number", "exclusiveMinimum": 0.0 },
            "side": { "enum": ["a", "b"] }
        }
    })
}

fn interleaving_def() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["mode", "side", "partner", "shared_estimate", "sequence"],
        "description": "The interleaved run this record is one side of: both sides were offered the same rates and holds, and on each rung the plateaus alternate in ABBA order.",
        "properties": {
            "mode": { "enum": ["aa", "ab"] },
            "side": { "enum": ["a", "b"] },
            "partner": { "type": "string", "minLength": 1 },
            "shared_estimate": { "type": "number", "exclusiveMinimum": 0.0 },
            "sequence": { "type": "array", "items": slot_def() }
        }
    })
}

fn host_reading_def() -> Value {
    let id = json!({ "type": "string", "minLength": 1 });
    let list =
        json!({ "type": "array", "minItems": 1, "items": { "type": "string", "minLength": 1 } });
    json!({
        "type": "object",
        "required": ["state"],
        "description": "The generator's measured fingerprint and the record's network leg read against the host prescription the record claims.",
        "oneOf": [
            { "additionalProperties": false, "required": ["state"], "properties": { "state": { "const": "unclaimed" } } },
            { "additionalProperties": false, "required": ["state", "prescription"], "properties": { "state": { "const": "matched" }, "prescription": id } },
            { "additionalProperties": false, "required": ["state", "prescription", "mismatches"], "properties": { "state": { "const": "unmatched" }, "prescription": id, "mismatches": list } },
            { "additionalProperties": false, "required": ["state", "prescription", "missing"], "properties": { "state": { "const": "unverifiable" }, "prescription": id, "missing": list } }
        ]
    })
}

/// `perf-compare.schema.json`: two or more `perf-result` records aligned,
/// with their ratios when there are two.
#[must_use]
pub fn perf_compare_schema() -> Value {
    let fields: Vec<&str> = crate::perf_result::compare::AlignField::ALL
        .iter()
        .map(|f| f.token())
        .collect();
    json!({
        "$schema": DRAFT,
        "$id": urn("perf-compare"),
        "title": "Veredictum performance comparison",
        "description": "Two or more perf-result records aligned into one table, with every mismatch that makes their numbers describe different things named above it. Two records of one interleaved run carry ratios rung by rung, each with both values, judged against the declared tolerance and the attached A/A floor. No openEHR specification governs performance; this is the instrument's own design.",
        "type": "object",
        "additionalProperties": false,
        "required": ["family", "mode", "records", "table", "tolerance"],
        "properties": {
            "family": { "const": "perf-compare" },
            "mode": { "enum": ["aligned", "aa", "ab"] },
            "records": {
                "type": "array",
                "minItems": 2,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["label", "base_url", "fields"],
                    "properties": {
                        "label": { "type": "string", "minLength": 1 },
                        "base_url": { "type": "string", "minLength": 1 },
                        "fields": {
                            "type": "object",
                            "propertyNames": { "enum": fields },
                            "additionalProperties": { "type": "string" }
                        }
                    }
                }
            },
            "mismatches": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["field", "values"],
                    "properties": {
                        "field": { "enum": fields },
                        "values": { "type": "array", "items": { "type": "string" } }
                    }
                }
            },
            "table": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["label", "cells"],
                    "properties": {
                        "label": { "type": "string", "minLength": 1 },
                        "cells": { "type": "array", "items": { "type": "string" } }
                    }
                }
            },
            "tolerance": { "type": "number", "minimum": 0.0 },
            "floor": aa_floor_def(),
            "sequence": { "type": "array", "items": slot_def() },
            "rows": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["rung", "planned_rate", "operation", "metric", "reading"],
                    "properties": {
                        "rung": { "type": "integer", "minimum": 0 },
                        "planned_rate": { "type": "number", "exclusiveMinimum": 0.0 },
                        "operation": { "type": "string", "minLength": 1 },
                        "metric": { "enum": compare_metric_tokens() },
                        "a": { "type": "number", "minimum": 0.0 },
                        "b": { "type": "number", "minimum": 0.0 },
                        "ratio": { "type": "number", "exclusiveMinimum": 0.0 },
                        "overlap": { "type": "boolean" },
                        "reading": { "enum": ["difference", "within_noise", "not_resolvable", "different_rates"] }
                    }
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
        ("ixit.schema.json", ixit_schema()),
        ("perf-payloads.schema.json", perf_payloads_schema()),
        ("perf-workload.schema.json", perf_workload_schema()),
        ("perf-workloads.schema.json", perf_workloads_schema()),
        ("perf-result.schema.json", perf_result_schema()),
        ("perf-host.schema.json", perf_host_schema()),
        ("perf-compare.schema.json", perf_compare_schema()),
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
        (
            "arazzo-chapters.schema.json",
            crate::arazzo::gates::published::chapters_schema(),
        ),
        (
            "arazzo-reports.schema.json",
            crate::arazzo::gates::published::reports_schema(),
        ),
        ("arazzo-results.schema.json", arazzo_results_schema()),
        ("certificate.schema.json", certificate_schema()),
        ("jwks.schema.json", jwks_schema()),
        ("identity.schema.json", identity_schema()),
    ]
}

/// Renders a schema document to its canonical published text: a two-space
/// pretty print with a trailing newline.
///
/// # Errors
/// The serializer's own error, should `serde_json` refuse the document.
pub fn render(schema: &Value) -> Result<String, serde_json::Error> {
    let mut text = serde_json::to_string_pretty(schema)?;
    text.push('\n');
    Ok(text)
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
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    fn rendering_is_deterministic() -> Result<(), serde_json::Error> {
        let a = render(&statement_schema())?;
        let b = render(&statement_schema())?;
        assert_eq!(a, b);
        assert!(a.ends_with('\n'));
        Ok(())
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    fn no_null_tokens_leak() -> Result<(), serde_json::Error> {
        for (name, schema) in emit_all() {
            let text = render(&schema)?;
            assert!(
                !text.contains(": null,"),
                "{name} leaked a null vocabulary token"
            );
        }
        Ok(())
    }

    /// Collects every `enum` member and every property name that is not a
    /// non-empty string, each named by its JSON Pointer.
    fn non_string_tokens(value: &Value, pointer: &str, found: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    let at = format!("{pointer}/{key}");
                    if key == "enum" {
                        for (i, member) in child.as_array().into_iter().flatten().enumerate() {
                            if member.as_str().is_none_or(str::is_empty) {
                                found.push(format!("{at}/{i} = {member}"));
                            }
                        }
                    }
                    if key == "properties" {
                        for name in child.as_object().into_iter().flat_map(|m| m.keys()) {
                            if name.is_empty() {
                                found.push(format!("{at} has an empty property name"));
                            }
                        }
                    }
                    non_string_tokens(child, &at, found);
                }
            }
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    non_string_tokens(item, &format!("{pointer}/{i}"), found);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }

    #[test]
    fn every_emitted_vocabulary_token_is_a_non_empty_string() {
        for (name, schema) in emit_all() {
            let mut found = Vec::new();
            non_string_tokens(&schema, "", &mut found);
            assert_eq!(
                found,
                Vec::<String>::new(),
                "{name} carries a non-string token"
            );
        }
    }

    /// A vocabulary whose serde token is a number, which no real implementor is.
    #[derive(Serialize)]
    struct NumericToken(u8);

    impl VocabToken for NumericToken {}

    #[test]
    #[should_panic(expected = "a VocabToken should serialize to a JSON string")]
    fn a_non_string_token_stops_emission_instead_of_reaching_a_schema() {
        let emitted = tokens(&[NumericToken(7)]);
        assert_eq!(emitted, Value::Null, "unreachable: tokens panics first");
    }
}
