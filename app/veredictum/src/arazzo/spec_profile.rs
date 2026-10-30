// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The specification profile a run is judged under: a pinned snapshot of the
//! openEHR development line, the latest text, or the frozen release.
//!
//! A citation names the same source and section under both profiles; only
//! the tree its text is read from changes. Under [`SpecProfile::Released`]
//! that is the vendored release, the oracle of every released claim. Under
//! [`SpecProfile::Development`] it is the snapshot under
//! [`DEVELOPMENT_ROOT`], one directory per upstream repository at the
//! repository's own paths ([`SpecProfile::path_of`]). A development result
//! is a record of its own: it never builds a certificate and never counts
//! toward a released result. The command line runs `test` under the
//! development profile unless told otherwise; the library's [`Default`] is
//! the released profile, the one that needs no snapshot. No openEHR spec
//! governs the profiles — our own design.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// The root of the vendored development snapshot, relative to the
/// repository root.
pub const DEVELOPMENT_ROOT: &str = "specs/openehr-development";

/// The prefix every upstream specification repository's name carries, which
/// names its directory under [`DEVELOPMENT_ROOT`].
const REPOSITORY_PREFIX: &str = "specifications-";

/// The released vendored roots that are one directory of an upstream
/// repository, with the repository and the directory inside it.
const MIRRORED_ROOTS: [(&str, &str, &str); 2] = [
    ("specs/rest-oas/", "ITS-REST", "computable/OAS/"),
    ("specs/its-json-schemas/", "ITS-JSON", "components/"),
];

/// The specification text a run reads its citations against.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpecProfile {
    /// The vendored release under `specs/`: the oracle of a released claim.
    #[default]
    Released,
    /// The pinned development snapshot under [`DEVELOPMENT_ROOT`].
    Development,
}

impl SpecProfile {
    /// All variants, in vocabulary order.
    pub const ALL: &[Self] = &[Self::Released, Self::Development];

    /// The token the command line and the results spell.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Released => "released",
            Self::Development => "development",
        }
    }

    /// Where the text of the cited `source` sits under this profile, relative
    /// to the repository root; `None` when the profile holds no tree for the
    /// source's root.
    ///
    /// The released profile reads every source where it names. The
    /// development profile reads `specs/openehr/<C>/` from the snapshot of
    /// `specifications-<C>`, following the development line's Antora layout
    /// ([`antora_path`]), `specs/rest-oas/` from ITS-REST's `computable/OAS/`
    /// and `specs/its-json-schemas/` from ITS-JSON's `components/`. The
    /// ITS-XML bundles are vendored from release tags, so the development
    /// profile holds no counterpart for them.
    #[must_use]
    pub fn path_of(self, source: &str) -> Option<String> {
        match self {
            Self::Released => Some(source.to_owned()),
            Self::Development => {
                if let Some((component, inner)) = source
                    .strip_prefix("specs/openehr/")
                    .and_then(|rest| rest.split_once('/'))
                {
                    return Some(format!(
                        "{}/{}",
                        self.component_dir(component),
                        antora_path(inner)
                    ));
                }
                MIRRORED_ROOTS.iter().find_map(|(root, component, inside)| {
                    source
                        .strip_prefix(root)
                        .map(|rest| format!("{}/{inside}{rest}", self.component_dir(component)))
                })
            }
        }
    }

    /// The directory, relative to the repository root, that holds
    /// `component`'s manifest and documents under this profile.
    #[must_use]
    pub fn component_dir(self, component: &str) -> String {
        match self {
            Self::Released => format!("specs/openehr/{component}"),
            Self::Development => format!("{DEVELOPMENT_ROOT}/{REPOSITORY_PREFIX}{component}"),
        }
    }
}

/// The package prefix the released AM class tables carry in their file names
/// (`org.openehr.am.aom14.c_object.adoc`), which the Antora partials drop
/// (`aom14.c_object.adoc`).
const AM_PACKAGE_PREFIX: &str = "org.openehr.am.";

/// Where the development line's Antora layout keeps the document a released
/// path names, given relative to its component's directory.
///
/// The released tree publishes a document as `docs/<doc>/masterNN-<name>.adoc`
/// and a class table as `docs/UML/classes/<class>.adoc`; the Antora
/// component keeps them as `modules/<doc>/pages/<name>.adoc` and
/// `modules/ROOT/partials/classes/<class>.adoc`
/// (<https://docs.antora.org/antora/latest/standard-directories/>), an AM
/// class without its `org.openehr.am.` package prefix. Any other path is
/// kept as it is.
#[must_use]
pub fn antora_path(inner: &str) -> String {
    let Some(docs) = inner.strip_prefix("docs/") else {
        return inner.to_owned();
    };
    let Some((document, file)) = docs.split_once('/') else {
        return inner.to_owned();
    };
    let adoc = Path::new(file)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("adoc"));
    if file.contains('/') || !adoc {
        if document == "UML"
            && let Some(class) = file.strip_prefix("classes/")
            && !class.contains('/')
        {
            let class = class.strip_prefix(AM_PACKAGE_PREFIX).unwrap_or(class);
            return format!("modules/ROOT/partials/classes/{class}");
        }
        return inner.to_owned();
    }
    let page = file
        .strip_prefix("master")
        .and_then(|numbered| numbered.split_once('-'))
        .filter(|(number, _)| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
        .map_or(file, |(_, name)| name);
    format!("modules/{document}/pages/{page}")
}

impl fmt::Display for SpecProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.token())
    }
}

/// A token that names no specification profile.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is no specification profile; use `released` or `development`")]
pub struct UnknownProfile(pub String);

impl FromStr for SpecProfile {
    type Err = UnknownProfile;

    fn from_str(token: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|profile| profile.token() == token)
            .ok_or_else(|| UnknownProfile(token.to_owned()))
    }
}

/// One component of the development snapshot, as its `PROVENANCE.md` names
/// it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotComponent {
    /// The component (`RM`), whose repository directory under
    /// [`DEVELOPMENT_ROOT`] is `specifications-RM`.
    pub component: String,
    /// The upstream repository.
    pub repository: String,
    /// The branch the commit was taken from.
    pub branch: String,
    /// The pinned commit.
    pub commit: String,
}

/// The specification text a results document was judged against.
///
/// Serialized under the `profile` tag: `{"profile": "released"}`, or
/// `{"profile": "development", "snapshot": [...]}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "profile", rename_all = "snake_case", deny_unknown_fields)]
pub enum Specification {
    /// The vendored release.
    #[default]
    Released,
    /// The pinned development snapshot, component by component.
    Development {
        /// Every component of the snapshot, ordered by directory name.
        snapshot: Vec<SnapshotComponent>,
    },
}

impl Specification {
    /// The record of `profile`, reading the development snapshot's
    /// provenance under `repo_root` when the profile is development.
    ///
    /// # Errors
    /// [`SnapshotError`] when the development snapshot is missing, or a
    /// component's `PROVENANCE.md` does not name its repository, branch and
    /// commit.
    pub fn of(profile: SpecProfile, repo_root: &Path) -> Result<Self, SnapshotError> {
        match profile {
            SpecProfile::Released => Ok(Self::Released),
            SpecProfile::Development => Ok(Self::Development {
                snapshot: snapshot(repo_root)?,
            }),
        }
    }

    /// The profile the record names.
    #[must_use]
    pub fn profile(&self) -> SpecProfile {
        match self {
            Self::Released => SpecProfile::Released,
            Self::Development { .. } => SpecProfile::Development,
        }
    }

    /// Whether the record was judged against the released text.
    #[must_use]
    pub fn is_released(&self) -> bool {
        matches!(self, Self::Released)
    }

    /// The snapshot components of a development record; empty for a
    /// released one.
    #[must_use]
    pub fn snapshot(&self) -> &[SnapshotComponent] {
        match self {
            Self::Released => &[],
            Self::Development { snapshot } => snapshot,
        }
    }

    /// One line naming the profile and, for development, every snapshot
    /// component with the first twelve characters of its commit.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Released => "released".to_owned(),
            Self::Development { snapshot } => {
                let parts: Vec<String> = snapshot
                    .iter()
                    .map(|c| {
                        let short: String = c.commit.chars().take(12).collect();
                        format!("{} {}@{short}", c.component, c.branch)
                    })
                    .collect();
                format!("development snapshot ({})", parts.join(", "))
            }
        }
    }

    /// Refuses a record judged under any profile but the released one.
    ///
    /// # Errors
    /// [`NotReleased`] for a development record.
    pub fn ensure_released(&self) -> Result<(), NotReleased> {
        match self {
            Self::Released => Ok(()),
            Self::Development { .. } => Err(NotReleased {
                label: self.label(),
            }),
        }
    }
}

/// A development record offered where only a released result is accepted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the results were judged under the {label}, and a development result never enters a released result, a certificate or a comparison"
)]
pub struct NotReleased {
    /// The profile and snapshot the record names.
    pub label: String,
}

/// Why the development snapshot's provenance does not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SnapshotError {
    /// The snapshot directory does not list.
    #[error(
        "the development snapshot `{path}` cannot be listed: {reason}; vendor it with scripts/vendor/spec-docs-development.sh"
    )]
    Missing {
        /// The directory.
        path: String,
        /// The verbatim failure.
        reason: String,
    },
    /// The snapshot holds no component.
    #[error("the development snapshot `{0}` holds no component")]
    Empty(String),
    /// A directory of the snapshot is not named for an upstream specification
    /// repository.
    #[error("`{0}` in the development snapshot is not a `specifications-<component>` directory")]
    Stray(String),
    /// A component's `PROVENANCE.md` does not read.
    #[error("`{path}` cannot be read: {reason}")]
    Unreadable {
        /// The provenance file.
        path: String,
        /// The verbatim failure.
        reason: String,
    },
    /// A component's `PROVENANCE.md` lacks a field.
    #[error("`{path}` does not name the component's {field}")]
    Provenance {
        /// The provenance file.
        path: String,
        /// The field it lacks: `repository`, `branch` or `commit`.
        field: &'static str,
    },
}

/// Every component of the development snapshot under `repo_root`, ordered
/// by directory name, read from each repository directory's `PROVENANCE.md`.
///
/// # Errors
/// [`SnapshotError`] when the snapshot does not list, holds no component,
/// holds a directory not named for a specification repository, or a
/// provenance file lacks the repository, the branch or the commit.
pub fn snapshot(repo_root: &Path) -> Result<Vec<SnapshotComponent>, SnapshotError> {
    let root = repo_root.join(DEVELOPMENT_ROOT);
    let missing = |reason: String| SnapshotError::Missing {
        path: DEVELOPMENT_ROOT.to_owned(),
        reason,
    };
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&root).map_err(|e| missing(e.to_string()))? {
        let entry = entry.map_err(|e| missing(e.to_string()))?;
        if entry
            .file_type()
            .map_err(|e| missing(e.to_string()))?
            .is_dir()
        {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    if names.is_empty() {
        return Err(SnapshotError::Empty(DEVELOPMENT_ROOT.to_owned()));
    }
    names
        .into_iter()
        .map(|directory| {
            let component = directory
                .strip_prefix(REPOSITORY_PREFIX)
                .filter(|component| !component.is_empty())
                .ok_or_else(|| SnapshotError::Stray(directory.clone()))?
                .to_owned();
            let path = format!("{DEVELOPMENT_ROOT}/{directory}/PROVENANCE.md");
            let lacking = |field| SnapshotError::Provenance {
                path: path.clone(),
                field,
            };
            let text = std::fs::read_to_string(repo_root.join(&path)).map_err(|e| {
                SnapshotError::Unreadable {
                    path: path.clone(),
                    reason: e.to_string(),
                }
            })?;
            let field = |name: &str| {
                text.lines().find_map(|line| {
                    line.trim_start_matches("- ")
                        .strip_prefix(name)
                        .map(|value| value.trim().trim_matches('`').to_owned())
                        .filter(|value| !value.is_empty())
                })
            };
            Ok(SnapshotComponent {
                repository: field("Source:").ok_or_else(|| lacking("repository"))?,
                branch: field("Branch:").ok_or_else(|| lacking("branch"))?,
                commit: field("Commit:").ok_or_else(|| lacking("commit"))?,
                component,
            })
        })
        .collect()
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result-returning tests in the Book ch11 shape, each asserting; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn a_profile_is_read_from_its_token_and_nothing_else() {
        assert_eq!("released".parse(), Ok(SpecProfile::Released));
        assert_eq!("development".parse(), Ok(SpecProfile::Development));
        assert_eq!(
            "Development".parse::<SpecProfile>(),
            Err(UnknownProfile("Development".to_owned()))
        );
        assert_eq!(SpecProfile::default(), SpecProfile::Released);
    }

    #[test]
    fn the_development_profile_maps_the_released_roots_it_mirrors() {
        let dev = SpecProfile::Development;
        assert_eq!(
            dev.path_of("specs/openehr/RM/docs/common/master06-change_control_package.adoc")
                .as_deref(),
            Some(
                "specs/openehr-development/specifications-RM/modules/common/pages/change_control_package.adoc"
            )
        );
        assert_eq!(
            dev.path_of("specs/openehr/RM/docs/UML/classes/locatable.adoc")
                .as_deref(),
            Some(
                "specs/openehr-development/specifications-RM/modules/ROOT/partials/classes/locatable.adoc"
            )
        );
        assert_eq!(
            dev.path_of("specs/openehr/AM/docs/UML/classes/org.openehr.am.aom14.c_object.adoc")
                .as_deref(),
            Some(
                "specs/openehr-development/specifications-AM/modules/ROOT/partials/classes/aom14.c_object.adoc"
            )
        );
        assert_eq!(
            dev.path_of("specs/openehr/ITS-REST/specifications/docs/overview/Resources.md")
                .as_deref(),
            Some(
                "specs/openehr-development/specifications-ITS-REST/specifications/docs/overview/Resources.md"
            )
        );
        assert_eq!(
            dev.path_of("specs/openehr/QUERY/docs/AQL/grammar/AqlLexer.g4")
                .as_deref(),
            Some("specs/openehr-development/specifications-QUERY/docs/AQL/grammar/AqlLexer.g4")
        );
        assert_eq!(
            dev.path_of("specs/rest-oas/ehr-validation.openapi.yaml")
                .as_deref(),
            Some(
                "specs/openehr-development/specifications-ITS-REST/computable/OAS/ehr-validation.openapi.yaml"
            )
        );
        assert_eq!(
            dev.path_of("specs/its-json-schemas/x.json").as_deref(),
            Some("specs/openehr-development/specifications-ITS-JSON/components/x.json")
        );
        assert_eq!(
            dev.path_of("specs/its-xml-schemas/its-xml-1.0.2-nsv1/x.xsd"),
            None
        );
        assert_eq!(
            SpecProfile::Released
                .path_of("specs/its-json-schemas/x.json")
                .as_deref(),
            Some("specs/its-json-schemas/x.json")
        );
    }

    #[test]
    fn the_vendored_snapshot_names_every_component_by_commit() -> Result<(), SnapshotError> {
        let snapshot = snapshot(&root())?;
        let components: Vec<&str> = snapshot.iter().map(|c| c.component.as_str()).collect();
        assert_eq!(
            components,
            [
                "AM", "BASE", "ITS-JSON", "ITS-REST", "ITS-XML", "QUERY", "RM"
            ]
        );
        for component in &snapshot {
            let line = if component.component.starts_with("ITS-JSON")
                || component.component.starts_with("ITS-XML")
            {
                "master"
            } else {
                "development"
            };
            assert_eq!(component.branch, line, "{component:?}");
            assert_eq!(component.commit.len(), 40, "{component:?}");
            assert!(
                component
                    .repository
                    .starts_with("https://github.com/openEHR/")
            );
        }
        Ok(())
    }

    #[test]
    fn a_development_record_is_refused_where_only_a_released_one_is_accepted()
    -> Result<(), SnapshotError> {
        assert_eq!(Specification::Released.ensure_released(), Ok(()));
        let development = Specification::of(SpecProfile::Development, &root())?;
        let refused = development.ensure_released();
        assert!(
            matches!(&refused, Err(NotReleased { label }) if label.starts_with("development snapshot (AM development@")),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn the_record_serializes_under_its_profile_tag() -> Result<(), serde_json::Error> {
        assert_eq!(
            serde_json::to_value(Specification::Released)?,
            serde_json::json!({ "profile": "released" })
        );
        let development = Specification::Development {
            snapshot: vec![SnapshotComponent {
                component: "RM".to_owned(),
                repository: "https://github.com/openEHR/specifications-RM".to_owned(),
                branch: "development".to_owned(),
                commit: "c".to_owned(),
            }],
        };
        let value = serde_json::to_value(&development)?;
        assert_eq!(value["profile"], "development");
        assert_eq!(serde_json::from_value::<Specification>(value)?, development);
        assert!(
            serde_json::from_value::<Specification>(serde_json::json!({ "profile": "draft" }))
                .is_err(),
            "an unknown profile is refused"
        );
        Ok(())
    }
}
