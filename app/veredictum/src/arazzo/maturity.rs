// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The maturity of the released text a citation names, read from the
//! vendored release itself.
//!
//! Every vendored component carries a `manifest.json` whose `specifications`
//! list states each document's `spec_status`, and a document's own
//! `docs/<id>/manifest_vars.adoc` states it as `:spec_status:`. A document
//! whose status is DEVELOPMENT inside a release is released text. A TRIAL
//! document is released for trial use, so behaviour grounded on it is read
//! with the options; a RETIRED or SUPERSEDED one is no longer current text,
//! so no citation may name it. No openEHR spec governs how the instrument reads
//! the statuses — our own design.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::arazzo::spec_profile::{SpecProfile, snapshot_source};

/// The status a vendored manifest gives one document of a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpecStatus {
    /// `STABLE`: released.
    Stable,
    /// `DEVELOPMENT` inside a release: released text.
    Development,
    /// `TRIAL`: released for trial use.
    Trial,
    /// `RETIRED`: no longer part of the release.
    Retired,
    /// `SUPERSEDED`: replaced by another document of the same text.
    Superseded,
}

impl SpecStatus {
    /// The status a manifest spells as `token`; `None` for any other token.
    #[must_use]
    pub fn of_token(token: &str) -> Option<Self> {
        match token.trim() {
            "STABLE" => Some(Self::Stable),
            "DEVELOPMENT" => Some(Self::Development),
            "TRIAL" => Some(Self::Trial),
            "RETIRED" => Some(Self::Retired),
            "SUPERSEDED" => Some(Self::Superseded),
            _ => None,
        }
    }
}

impl fmt::Display for SpecStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Stable => "STABLE",
            Self::Development => "DEVELOPMENT",
            Self::Trial => "TRIAL",
            Self::Retired => "RETIRED",
            Self::Superseded => "SUPERSEDED",
        })
    }
}

/// The maturity of one cited file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Maturity {
    /// The file belongs to one document of a component's manifest.
    Document {
        /// The component, as its folder under `specs/openehr/` names it.
        component: String,
        /// The document id its manifest lists.
        document: String,
        /// The status the release gives it.
        status: SpecStatus,
    },
    /// The file is part of a released component and of no single document
    /// in it: the component's UML, or the ITS-REST fragments every API
    /// document renders.
    Component {
        /// The component, as its folder under `specs/openehr/` names it.
        component: String,
    },
}

impl Maturity {
    /// Whether the cited text is TRIAL.
    #[must_use]
    pub fn is_trial(&self) -> bool {
        matches!(
            self,
            Self::Document {
                status: SpecStatus::Trial,
                ..
            }
        )
    }
}

/// Why a cited file has no maturity.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MaturityError {
    /// The file sits in no vendored tree whose manifest states statuses.
    #[error("`{0}` sits in no vendored component whose manifest states a status")]
    NoManifest(String),
    /// The manifest lists the document without a status the release
    /// vocabulary knows.
    #[error("the vendored document `{component}/{document}` states no known status: `{found}`")]
    NoStatus {
        /// The component.
        component: String,
        /// The document id.
        document: String,
        /// What the manifest states, or `(none)`.
        found: String,
    },
    /// The manifest cannot be read or parsed.
    #[error("`{path}` cannot be read: {reason}")]
    Unreadable {
        /// The manifest file.
        path: String,
        /// The verbatim failure.
        reason: String,
    },
}

/// The XSD bundle of the ITS-XML release 1.0.2, the stable `ITS-XML-v1`.
const XML_V1_BUNDLE: &str = "specs/its-xml-schemas/its-xml-1.0.2-nsv1/";

/// The XSD bundle of the ITS-XML release 2.0.0, one TRIAL document per
/// component folder.
const XML_V2_BUNDLE: &str = "specs/its-xml-schemas/its-xml-2.0.0-nsv2/";

/// One component's `manifest.json`, the fields this module reads.
#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    specifications: Vec<ManifestEntry>,
}

/// One document of a component's manifest.
#[derive(Debug, Deserialize)]
struct ManifestEntry {
    id: String,
    #[serde(default)]
    spec_status: Option<String>,
}

/// The `(id, spec_status)` entries of one manifest, or why it did not read.
type ManifestEntries = Result<Vec<(String, Option<String>)>, MaturityError>;

/// The maturities of the vendored release under one repository root,
/// reading each manifest once.
#[derive(Debug)]
pub struct Maturities {
    repo_root: PathBuf,
    profile: SpecProfile,
    manifests: BTreeMap<String, ManifestEntries>,
}

impl Maturities {
    /// The maturities of the release vendored under `repo_root`.
    #[must_use]
    pub fn new(repo_root: &Path) -> Self {
        Self::in_profile(repo_root, SpecProfile::Released)
    }

    /// The maturities `profile` gives the documents vendored under
    /// `repo_root`: the release's manifests, or the development snapshot's.
    #[must_use]
    pub fn in_profile(repo_root: &Path, profile: SpecProfile) -> Self {
        Self {
            repo_root: repo_root.to_owned(),
            profile,
            manifests: BTreeMap::new(),
        }
    }

    /// The maturity of the cited file `source`, a path relative to the
    /// repository root.
    ///
    /// `specs/rest-oas/` is the ITS-REST `REST_OAS_files` document,
    /// `specs/its-json-schemas/` the ITS-JSON `ITS-JSON` document, and the
    /// ITS-XML bundles the `ITS-XML-v1` document (1.0.2) or the
    /// `ITS-XML-<component>` document (2.0.0). Under `specs/openehr/<C>/`
    /// the first path segment naming a document, by an id or a file stem
    /// `C`'s manifest lists or by a `docs/<id>/manifest_vars.adoc` of its
    /// own, is the document; its status is its
    /// `docs/<id>/manifest_vars.adoc` `:spec_status:` when that file exists,
    /// else the manifest's. A file under a component naming no document is
    /// [`Maturity::Component`]. A file of the development snapshot
    /// ([`snapshot_source`]) is read the same way from the snapshot's own
    /// manifests, under every profile.
    ///
    /// # Errors
    /// [`MaturityError`] when no manifest covers the file, the document
    /// states no known status, or the manifest does not read.
    pub fn of(&mut self, source: &str) -> Result<Maturity, MaturityError> {
        match snapshot_source(source) {
            Some(shaped) => self.of_in(&shaped, SpecProfile::Development),
            None => self.of_in(source, self.profile),
        }
    }

    /// The maturity of `source` in `profile`'s manifests.
    fn of_in(&mut self, source: &str, profile: SpecProfile) -> Result<Maturity, MaturityError> {
        if source.starts_with("specs/rest-oas/") {
            return self.document(profile, "ITS-REST", "REST_OAS_files");
        }
        if source.starts_with("specs/its-json-schemas/") {
            return self.document(profile, "ITS-JSON", "ITS-JSON");
        }
        if source.starts_with(XML_V1_BUNDLE) {
            return self.document(profile, "ITS-XML", "ITS-XML-v1");
        }
        if let Some(rest) = source.strip_prefix(XML_V2_BUNDLE) {
            let folder = rest
                .split('/')
                .next()
                .filter(|folder| !folder.is_empty() && rest.contains('/'))
                .ok_or_else(|| MaturityError::NoManifest(source.to_owned()))?;
            return self.document(profile, "ITS-XML", &format!("ITS-XML-{folder}"));
        }
        let Some((component, inner)) = source
            .strip_prefix("specs/openehr/")
            .and_then(|rest| rest.split_once('/'))
        else {
            return Err(MaturityError::NoManifest(source.to_owned()));
        };
        let entries = self.manifest(profile, component)?.to_vec();
        let named = inner.split('/').find_map(|segment| {
            let stem = segment.split('.').next().unwrap_or(segment);
            let listed = entries
                .iter()
                .find(|(id, _)| id == segment || id == stem)
                .map(|(id, _)| id.clone());
            listed.or_else(|| {
                self.vars_path(profile, component, segment)
                    .is_file()
                    .then(|| segment.to_owned())
            })
        });
        match named {
            Some(document) => self.document(profile, component, &document),
            None => Ok(Maturity::Component {
                component: component.to_owned(),
            }),
        }
    }

    /// The maturity of `document` in `component`'s manifest.
    fn document(
        &mut self,
        profile: SpecProfile,
        component: &str,
        document: &str,
    ) -> Result<Maturity, MaturityError> {
        let from_vars = self.manifest_vars(profile, component, document);
        let entries = self.manifest(profile, component)?;
        let listed = entries.iter().find(|(id, _)| id == document);
        let found = from_vars.or_else(|| listed.and_then(|(_, status)| status.clone()));
        let no_status = |found: String| MaturityError::NoStatus {
            component: component.to_owned(),
            document: document.to_owned(),
            found,
        };
        let token = found.ok_or_else(|| no_status("(none)".to_owned()))?;
        let status = SpecStatus::of_token(&token).ok_or_else(|| no_status(token.clone()))?;
        Ok(Maturity::Document {
            component: component.to_owned(),
            document: document.to_owned(),
            status,
        })
    }

    /// The `:spec_status:` a document's `manifest_vars.adoc` states, when
    /// the file exists and states one.
    fn manifest_vars(
        &self,
        profile: SpecProfile,
        component: &str,
        document: &str,
    ) -> Option<String> {
        let path = self.vars_path(profile, component, document);
        // NOTE: no openEHR spec governs this — our own design; a document without its own
        // manifest_vars.adoc takes the status its component manifest lists.
        let text = std::fs::read_to_string(path).ok()?;
        text.lines().find_map(|line| {
            line.strip_prefix(":spec_status:")
                .map(|status| status.trim().to_owned())
        })
    }

    /// Where `document` of `component` states its own status.
    fn vars_path(&self, profile: SpecProfile, component: &str, document: &str) -> PathBuf {
        self.repo_root
            .join(profile.component_dir(component))
            .join("docs")
            .join(document)
            .join("manifest_vars.adoc")
    }

    /// The document ids `component`'s manifest in `profile`'s tree lists, in
    /// manifest order.
    ///
    /// # Errors
    /// [`MaturityError::NoManifest`] when the component has no manifest, and
    /// [`MaturityError::Unreadable`] when it does not read.
    pub fn document_ids(
        &mut self,
        profile: SpecProfile,
        component: &str,
    ) -> Result<Vec<String>, MaturityError> {
        Ok(self
            .manifest(profile, component)?
            .iter()
            .map(|(id, _)| id.clone())
            .collect())
    }

    /// The `(id, spec_status)` entries of `component`'s manifest in
    /// `profile`'s tree.
    fn manifest(
        &mut self,
        profile: SpecProfile,
        component: &str,
    ) -> Result<&[(String, Option<String>)], MaturityError> {
        let repo_root = &self.repo_root;
        let directory = profile.component_dir(component);
        let entry = self.manifests.entry(directory.clone()).or_insert_with(|| {
            let path = repo_root.join(&directory).join("manifest.json");
            if !path.is_file() {
                return Err(MaturityError::NoManifest(format!("{directory}/")));
            }
            let unreadable = |reason: String| MaturityError::Unreadable {
                path: path.display().to_string(),
                reason,
            };
            let text = std::fs::read_to_string(&path).map_err(|e| unreadable(e.to_string()))?;
            let manifest: Manifest =
                serde_json::from_str(&text).map_err(|e| unreadable(e.to_string()))?;
            Ok(manifest
                .specifications
                .into_iter()
                .map(|entry| (entry.id, entry.spec_status))
                .collect())
        });
        entry.as_deref().map_err(Clone::clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn maturities() -> Maturities {
        Maturities::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
    }

    fn status(source: &str) -> SpecStatus {
        match maturities().of(source) {
            Ok(Maturity::Document { status, .. }) => status,
            other => panic!("{source}: {other:?}"),
        }
    }

    #[test]
    fn each_status_is_read_from_the_vendored_release() {
        assert_eq!(
            status("specs/openehr/ITS-REST/specifications/docs/overview/Resources.md"),
            SpecStatus::Stable
        );
        assert_eq!(
            status("specs/openehr/ITS-REST/specifications/docs/admin/Description.md"),
            SpecStatus::Development
        );
        assert_eq!(
            status("specs/rest-oas/query-validation.openapi.yaml"),
            SpecStatus::Trial
        );
        assert_eq!(
            status("specs/openehr/ITS-REST/docs/simplified_data_template/master.adoc"),
            SpecStatus::Retired
        );
        assert_eq!(
            status("specs/openehr/AM/docs/OPT2/master.adoc"),
            SpecStatus::Development,
            "a document's own manifest_vars states its status"
        );
        assert_eq!(
            status("specs/its-xml-schemas/its-xml-2.0.0-nsv2/RM/x.xsd"),
            SpecStatus::Trial
        );
        assert_eq!(
            status("specs/its-xml-schemas/its-xml-1.0.2-nsv1/ALL/x.xsd"),
            SpecStatus::Stable
        );
        assert_eq!(
            status("specs/openehr/ITS-REST/specifications/ehr.openapi.yaml"),
            SpecStatus::Stable,
            "a file stem names its document"
        );
    }

    #[test]
    fn a_document_the_snapshot_supersedes_is_read_as_superseded_and_never_trial() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let found = Maturities::in_profile(&root, SpecProfile::Development)
            .of("specs/openehr/AM/docs/Identification/master07-referencing.adoc")
            .unwrap();
        assert_eq!(
            found,
            Maturity::Document {
                component: "AM".to_owned(),
                document: "Identification".to_owned(),
                status: SpecStatus::Superseded,
            }
        );
        assert!(!found.is_trial());
    }

    #[test]
    fn a_snapshot_file_reads_its_status_from_the_snapshot_under_every_profile() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = "specs/openehr-development/specifications-AM/docs/Identification/master07-referencing.adoc";
        for profile in SpecProfile::ALL {
            assert_eq!(
                Maturities::in_profile(&root, *profile).of(source),
                Ok(Maturity::Document {
                    component: "AM".to_owned(),
                    document: "Identification".to_owned(),
                    status: SpecStatus::Superseded,
                }),
                "{profile}"
            );
        }
        assert_eq!(
            maturities()
                .of("specs/openehr-development/specifications-ITS-XML/components/AM/Release-1.4/Template.xsd")
                .map(|m| m.is_trial()),
            Ok(true)
        );
    }

    #[test]
    fn a_file_of_no_single_document_is_component_text_and_never_trial() {
        let found = maturities()
            .of("specs/openehr/ITS-REST/specifications/responses/201_EHR.yaml")
            .unwrap();
        assert_eq!(
            found,
            Maturity::Component {
                component: "ITS-REST".to_owned()
            }
        );
        assert!(!found.is_trial());
        assert!(
            maturities()
                .of("specs/rest-oas/ehr-validation.openapi.yaml")
                .unwrap()
                .is_trial()
        );
    }

    #[test]
    fn a_file_outside_every_manifest_has_no_maturity() {
        assert!(matches!(
            maturities().of("specs/openehr/NOPE/docs/x.adoc"),
            Err(MaturityError::NoManifest(_))
        ));
        assert!(matches!(
            maturities().of("schemas/statement.schema.json"),
            Err(MaturityError::NoManifest(_))
        ));
        assert_eq!(SpecStatus::of_token("OBSOLETE"), None);
        assert_eq!(
            SpecStatus::of_token("SUPERSEDED"),
            Some(SpecStatus::Superseded)
        );
    }
}
