// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The closed operation vocabulary a workload names, each token bound to the
//! released ITS-REST operation it is sent as.
//!
//! Every token resolves to one operation file under
//! `specs/openehr/ITS-REST/specifications/operations/`, reached from its
//! API document (`ehr.openapi.yaml`, `query.openapi.yaml`, …) at the path
//! and method recorded here, so a workload can name no wire the release does
//! not define. The `aql` token is one operation per declared query class;
//! which of the four released query executions carries an arrival is the
//! class's [`QueryTransport`] mix.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One released ITS-REST operation: the API document that routes it, its
/// `operationId` (the operation file's stem), the method and the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleasedOperation {
    /// The API document under `specs/openehr/ITS-REST/specifications/`,
    /// without its `.openapi.yaml` suffix.
    pub api: &'static str,
    /// The operation file's stem, which is its `operationId` except where the
    /// release spells that with a `.yaml` suffix
    /// (`operations/definition_query_version_store.yaml`).
    pub operation_id: &'static str,
    /// The HTTP method the API document routes it under.
    pub method: &'static str,
    /// The path template the API document routes it under.
    pub path: &'static str,
}

impl ReleasedOperation {
    /// Returns the citation of the operation: its file and its route.
    #[must_use]
    pub fn citation(&self) -> String {
        format!(
            "specs/openehr/ITS-REST/specifications/operations/{}.yaml ({} {}, {}.openapi.yaml)",
            self.operation_id, self.method, self.path, self.api
        )
    }
}

const fn released(
    api: &'static str,
    operation_id: &'static str,
    method: &'static str,
    path: &'static str,
) -> ReleasedOperation {
    ReleasedOperation {
        api,
        operation_id,
        method,
        path,
    }
}

/// What a stage of an operation carries besides its addressing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageInput {
    /// Nothing: the operation is addressed by the interaction's EHR and the
    /// objects it holds.
    None,
    /// A payload key of `perf/payloads.yaml` whose meaning is `role`.
    Payload(PayloadRole),
    /// A query class the workload declares.
    Query,
}

/// How a stage's payload key is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadRole {
    /// An operational template, committed through its `.example` skeleton:
    /// the key carries a `template_id`, and `<key>.example` is in the
    /// manifest too.
    Template,
    /// A Simplified Flat body, sent with the `openehr-template-id` header
    /// its manifest entry names, so the entry carries a `template_id`.
    Flat,
    /// A body sent as its bytes.
    Body,
}

macro_rules! workload_ops {
    ($( $(#[$doc:meta])* $variant:ident = $token:literal, $folder:literal, $write:literal, $input:expr, $op:expr; )*) => {
        /// One operation of the workload vocabulary.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum WorkloadOp {
            $( $(#[$doc])* $variant, )*
        }

        impl WorkloadOp {
            /// Every operation, in vocabulary order (the schema enum and the
            /// manifest read this order).
            pub const ALL: &[WorkloadOp] = &[ $( WorkloadOp::$variant, )* ];

            /// Returns the token a workload document names the operation by.
            #[must_use]
            pub fn token(self) -> &'static str {
                match self { $( WorkloadOp::$variant => $token, )* }
            }

            /// Returns the capability folder under `catalogue/cases/` the
            /// operation belongs to, which a statement must claim for an
            /// interaction sending it to run.
            #[must_use]
            pub fn folder(self) -> &'static str {
                match self { $( WorkloadOp::$variant => $folder, )* }
            }

            /// Returns whether the operation changes the server's state,
            /// which is what the read band of the expanded mix counts.
            #[must_use]
            pub fn is_write(self) -> bool {
                match self { $( WorkloadOp::$variant => $write, )* }
            }

            /// Returns what a stage of this operation carries.
            #[must_use]
            pub fn input(self) -> StageInput {
                match self { $( WorkloadOp::$variant => $input, )* }
            }

            /// Returns the released operation the token is sent as, or
            /// `None` for `aql`, whose operation is its transport's.
            #[must_use]
            pub fn released(self) -> Option<ReleasedOperation> {
                match self { $( WorkloadOp::$variant => $op, )* }
            }
        }
    };
}

workload_ops! {
    /// `POST /ehr`.
    EhrCreate = "ehr_create", "ehr", true, StageInput::None,
        Some(released("ehr", "ehr_create", "POST", "/ehr"));
    /// `GET /ehr/{ehr_id}`.
    EhrRead = "ehr_read", "ehr", false, StageInput::None,
        Some(released("ehr", "ehr_get_by_id", "GET", "/ehr/{ehr_id}"));
    /// `GET /ehr?subject_id=…&subject_namespace=…`, how a front end finds the
    /// record of the patient in front of it.
    EhrReadBySubject = "ehr_read_by_subject", "ehr", false, StageInput::None,
        Some(released("ehr", "ehr_get_by_subject", "GET", "/ehr"));
    /// `GET /ehr/{ehr_id}/ehr_status`.
    EhrStatusRead = "ehr_status_read", "ehr_status", false, StageInput::None,
        Some(released("ehr", "ehr_status_get_at_time", "GET", "/ehr/{ehr_id}/ehr_status"));
    /// `PUT /ehr/{ehr_id}/ehr_status` with `If-Match`.
    EhrStatusUpdate = "ehr_status_update", "ehr_status", true, StageInput::None,
        Some(released("ehr", "ehr_status_update", "PUT", "/ehr/{ehr_id}/ehr_status"));
    /// `POST /ehr/{ehr_id}/composition` in canonical JSON.
    CompositionCommit = "composition_commit", "composition", true,
        StageInput::Payload(PayloadRole::Template),
        Some(released("ehr", "composition_create", "POST", "/ehr/{ehr_id}/composition"));
    /// `POST /ehr/{ehr_id}/composition` in the Simplified Flat format.
    CompositionCommitFlat = "composition_commit_flat", "simplified_formats", true,
        StageInput::Payload(PayloadRole::Flat),
        Some(released("ehr", "composition_create", "POST", "/ehr/{ehr_id}/composition"));
    /// `GET /ehr/{ehr_id}/composition/{uid_based_id}` at a version uid.
    CompositionRead = "composition_read", "composition", false, StageInput::None,
        Some(released("ehr", "composition_get", "GET", "/ehr/{ehr_id}/composition/{uid_based_id}"));
    /// `GET /ehr/{ehr_id}/composition/{uid_based_id}` at a versioned object
    /// uid, which answers the latest version.
    CompositionReadCurrent = "composition_read_current", "composition", false, StageInput::None,
        Some(released("ehr", "composition_get", "GET", "/ehr/{ehr_id}/composition/{uid_based_id}"));
    /// `PUT /ehr/{ehr_id}/composition/{uid_based_id}` with `If-Match`.
    CompositionUpdate = "composition_update", "composition", true,
        StageInput::Payload(PayloadRole::Template),
        Some(released("ehr", "composition_update", "PUT", "/ehr/{ehr_id}/composition/{uid_based_id}"));
    /// `DELETE /ehr/{ehr_id}/composition/{uid_based_id}`.
    CompositionDelete = "composition_delete", "composition", true, StageInput::None,
        Some(released("ehr", "composition_delete", "DELETE", "/ehr/{ehr_id}/composition/{uid_based_id}"));
    /// `GET …/versioned_composition/{versioned_object_uid}/revision_history`.
    CompositionRevisionHistory = "composition_revision_history", "composition", false, StageInput::None,
        Some(released("ehr", "versioned_composition_revision_history", "GET",
            "/ehr/{ehr_id}/versioned_composition/{versioned_object_uid}/revision_history"));
    /// `GET …/versioned_composition/{versioned_object_uid}/version/{version_uid}`.
    CompositionVersionRead = "composition_version_read", "composition", false, StageInput::None,
        Some(released("ehr", "versioned_composition_version_get_by_id", "GET",
            "/ehr/{ehr_id}/versioned_composition/{versioned_object_uid}/version/{version_uid}"));
    /// `POST /ehr/{ehr_id}/directory`.
    DirectoryCreate = "directory_create", "directory", true, StageInput::None,
        Some(released("ehr", "directory_create", "POST", "/ehr/{ehr_id}/directory"));
    /// `GET /ehr/{ehr_id}/directory`.
    DirectoryRead = "directory_read", "directory", false, StageInput::None,
        Some(released("ehr", "directory_get_at_time", "GET", "/ehr/{ehr_id}/directory"));
    /// `PUT /ehr/{ehr_id}/directory` with `If-Match`.
    DirectoryUpdate = "directory_update", "directory", true, StageInput::None,
        Some(released("ehr", "directory_update", "PUT", "/ehr/{ehr_id}/directory"));
    /// `POST /ehr/{ehr_id}/contribution` carrying one composition version.
    ContributionCommit = "contribution_commit", "contribution", true,
        StageInput::Payload(PayloadRole::Template),
        Some(released("ehr", "contribution_create", "POST", "/ehr/{ehr_id}/contribution"));
    /// `GET /ehr/{ehr_id}/contribution/{contribution_uid}`.
    ContributionRead = "contribution_read", "contribution", false, StageInput::None,
        Some(released("ehr", "contribution_get", "GET", "/ehr/{ehr_id}/contribution/{contribution_uid}"));
    /// `PUT /ehr/{ehr_id}/composition/{uid_based_id}/tags`.
    TagsPut = "tags_put", "item_tags", true, StageInput::None,
        Some(released("ehr", "composition_tags_update", "PUT", "/ehr/{ehr_id}/composition/{uid_based_id}/tags"));
    /// `GET /ehr/{ehr_id}/composition/{uid_based_id}/tags`.
    TagsRead = "tags_read", "item_tags", false, StageInput::None,
        Some(released("ehr", "composition_tags_get", "GET", "/ehr/{ehr_id}/composition/{uid_based_id}/tags"));
    /// `GET /definition/template/adl1.4`.
    TemplateList = "template_list", "definition", false, StageInput::None,
        Some(released("definition", "definition_template_adl1.4_list", "GET", "/definition/template/adl1.4"));
    /// `GET /definition/template/adl1.4/{template_id}`.
    TemplateGet = "template_get", "definition", false, StageInput::None,
        Some(released("definition", "definition_template_adl1.4_get", "GET", "/definition/template/adl1.4/{template_id}"));
    /// `GET /definition/template/adl1.4/{template_id}/example`.
    TemplateExample = "template_example", "definition", false, StageInput::None,
        Some(released("definition", "definition_template_adl1.4_example_get", "GET",
            "/definition/template/adl1.4/{template_id}/example"));
    /// `GET /definition/template/adl2`.
    TemplateAdl2List = "template_adl2_list", "definition", false, StageInput::None,
        Some(released("definition", "definition_template_adl2_list", "GET", "/definition/template/adl2"));
    /// `POST /demographic/person`.
    PartyCreate = "party_create", "demographic", true, StageInput::Payload(PayloadRole::Body),
        Some(released("demographic", "person_create", "POST", "/demographic/person"));
    /// `GET /demographic/person/{uid_based_id}`.
    PartyRead = "party_read", "demographic", false, StageInput::None,
        Some(released("demographic", "person_get", "GET", "/demographic/person/{uid_based_id}"));
    /// `PUT /demographic/person/{uid_based_id}` with `If-Match`.
    PartyUpdate = "party_update", "demographic", true, StageInput::Payload(PayloadRole::Body),
        Some(released("demographic", "person_update", "PUT", "/demographic/person/{uid_based_id}"));
    /// `OPTIONS /`, the System API's one operation.
    SystemOptions = "system_options", "system", false, StageInput::None,
        Some(released("system", "options", "OPTIONS", "/"));
    /// One arrival of a declared query class, sent over its transport mix.
    Aql = "aql", "query", false, StageInput::Query, None;
}

impl WorkloadOp {
    /// Parses a token of the vocabulary.
    ///
    /// # Errors
    /// The unknown token, named: the vocabulary is closed.
    pub fn parse(token: &str) -> Result<Self, String> {
        Self::ALL
            .iter()
            .copied()
            .find(|op| op.token() == token)
            .ok_or_else(|| format!("{token:?} is not an operation of the workload vocabulary"))
    }
}

impl fmt::Display for WorkloadOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.token())
    }
}

impl Serialize for WorkloadOp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.token())
    }
}

impl<'de> Deserialize<'de> for WorkloadOp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let token = String::deserialize(deserializer)?;
        Self::parse(&token).map_err(serde::de::Error::custom)
    }
}

/// Which released query execution carries an arrival of a query class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryTransport {
    /// `POST /query/aql` with an `AdhocQueryExecute` body.
    AdhocPost,
    /// `GET /query/aql?q=…`.
    AdhocGet,
    /// `GET /query/{qualified_query_name}` of the stored class.
    StoredGet,
    /// `POST /query/{qualified_query_name}` with a `Query` body.
    StoredPost,
}

impl QueryTransport {
    /// Every transport, in vocabulary order.
    pub const ALL: &[QueryTransport] = &[
        QueryTransport::AdhocPost,
        QueryTransport::AdhocGet,
        QueryTransport::StoredGet,
        QueryTransport::StoredPost,
    ];

    /// Returns the token a workload document names the transport by.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            QueryTransport::AdhocPost => "adhoc_post",
            QueryTransport::AdhocGet => "adhoc_get",
            QueryTransport::StoredGet => "stored_get",
            QueryTransport::StoredPost => "stored_post",
        }
    }

    /// Returns whether the transport executes the class as a stored query,
    /// which the run registers before the clock starts.
    #[must_use]
    pub fn is_stored(self) -> bool {
        matches!(self, QueryTransport::StoredGet | QueryTransport::StoredPost)
    }

    /// Returns the released operation the transport is sent as.
    #[must_use]
    pub fn released(self) -> ReleasedOperation {
        match self {
            QueryTransport::AdhocPost => released(
                "query",
                "query_execute_adhoc_query_body",
                "POST",
                "/query/aql",
            ),
            QueryTransport::AdhocGet => {
                released("query", "query_execute_adhoc_query", "GET", "/query/aql")
            }
            QueryTransport::StoredGet => released(
                "query",
                "query_execute_stored_query",
                "GET",
                "/query/{qualified_query_name}",
            ),
            QueryTransport::StoredPost => released(
                "query",
                "query_execute_stored_query_body",
                "POST",
                "/query/{qualified_query_name}",
            ),
        }
    }

    /// Parses a token of the transport vocabulary.
    ///
    /// # Errors
    /// The unknown token, named.
    pub fn parse(token: &str) -> Result<Self, String> {
        Self::ALL
            .iter()
            .copied()
            .find(|t| t.token() == token)
            .ok_or_else(|| format!("{token:?} is not a query transport"))
    }
}

impl fmt::Display for QueryTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.token())
    }
}

impl Serialize for QueryTransport {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.token())
    }
}

impl<'de> Deserialize<'de> for QueryTransport {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let token = String::deserialize(deserializer)?;
        Self::parse(&token).map_err(serde::de::Error::custom)
    }
}

/// The released operation that registers a stored query class before a run:
/// `PUT /definition/query/{qualified_query_name}/{version}` with the AQL text
/// as a `text/plain` body.
pub const STORED_QUERY_STORE: ReleasedOperation = released(
    "definition",
    "definition_query_version_store",
    "PUT",
    "/definition/query/{qualified_query_name}/{version}",
);

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn specifications() -> PathBuf {
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../specs/openehr/ITS-REST/specifications"
        ))
        .to_owned()
    }

    /// Asserts the API document routes `op` at its path and method to its
    /// operation file, and that the file exists.
    fn assert_routed(op: &ReleasedOperation) {
        let root = specifications();
        let file = root.join(format!("operations/{}.yaml", op.operation_id));
        assert!(
            file.is_file(),
            "{} is not a released operation file",
            file.display()
        );
        let api = std::fs::read_to_string(root.join(format!("{}.openapi.yaml", op.api)))
            .expect("the API document reads");
        let mut in_path = false;
        let mut in_method = false;
        let mut found = false;
        let path_line = format!("  '{}':", op.path);
        let method_line = format!("    {}:", op.method.to_ascii_lowercase());
        let reference = format!("$ref: ./operations/{}.yaml", op.operation_id);
        for line in api.lines() {
            if line.starts_with("  '") {
                in_path = line == path_line;
                in_method = false;
            } else if in_path && line.starts_with("    ") && !line.starts_with("     ") {
                in_method = line == method_line;
            } else if in_path && in_method && line.trim() == reference {
                found = true;
            }
        }
        assert!(found, "{} does not route {}", op.api, op.citation());
    }

    /// Every token of the vocabulary is a released operation, routed where
    /// this module says it is.
    #[test]
    fn every_operation_is_a_released_route() {
        for op in WorkloadOp::ALL {
            if let Some(released) = op.released() {
                assert_routed(&released);
            } else {
                assert_eq!(*op, WorkloadOp::Aql);
            }
        }
        for transport in QueryTransport::ALL {
            assert_routed(&transport.released());
        }
        assert_routed(&STORED_QUERY_STORE);
    }

    /// Tokens round-trip, and an unknown token is refused by name.
    #[test]
    fn tokens_round_trip_and_unknown_tokens_are_refused() {
        for op in WorkloadOp::ALL {
            assert_eq!(WorkloadOp::parse(op.token()), Ok(*op));
        }
        for transport in QueryTransport::ALL {
            assert_eq!(QueryTransport::parse(transport.token()), Ok(*transport));
        }
        let error = WorkloadOp::parse("ehr_extract_export").expect_err("not released");
        assert!(error.contains("ehr_extract_export"), "{error}");
    }

    /// Every folder an operation names is a capability folder of the
    /// committed catalogue.
    #[test]
    fn every_operation_folder_is_a_catalogue_folder() {
        let cases = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../catalogue/cases"
        ));
        for op in WorkloadOp::ALL {
            assert!(
                cases.join(op.folder()).is_dir(),
                "{op}: {} is not a capability folder",
                op.folder()
            );
        }
    }
}
