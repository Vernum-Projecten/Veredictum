// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The specification profile a run is judged under: a pinned snapshot of the
//! `master` of each openEHR specification repository, the line that carries
//! the newest text, or the frozen release.
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
///
/// The ITS-XML 2.0.0 bundle is the upstream `components/` tree, file for
/// file; the 1.0.2 bundle has no counterpart on `master`.
const MIRRORED_ROOTS: [(&str, &str, &str); 3] = [
    ("specs/rest-oas/", "ITS-REST", "computable/OAS/"),
    ("specs/its-json-schemas/", "ITS-JSON", "components/"),
    (
        "specs/its-xml-schemas/its-xml-2.0.0-nsv2/",
        "ITS-XML",
        "components/",
    ),
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
    /// development profile reads `specs/openehr/<C>/<path>` from
    /// `specifications-<C>/<path>` in the snapshot, `specs/rest-oas/` from
    /// ITS-REST's `computable/OAS/`, `specs/its-json-schemas/` from
    /// ITS-JSON's `components/` and the ITS-XML 2.0.0 bundle from ITS-XML's
    /// `components/`. The ITS-XML 1.0.2 bundle has no counterpart there.
    #[must_use]
    pub fn path_of(self, source: &str) -> Option<String> {
        match self {
            Self::Released => Some(source.to_owned()),
            Self::Development => {
                if let Some((component, inner)) = source
                    .strip_prefix("specs/openehr/")
                    .and_then(|rest| rest.split_once('/'))
                {
                    return Some(format!("{}/{inner}", self.component_dir(component)));
                }
                MIRRORED_ROOTS.iter().find_map(|(root, component, inside)| {
                    source
                        .strip_prefix(root)
                        .map(|rest| format!("{}/{inside}{rest}", self.component_dir(component)))
                })
            }
        }
    }

    /// The file this profile reads for the cited `source` under `repo_root`:
    /// [`SpecProfile::path_of`], or, when no file sits there, the class
    /// table the snapshot names by its package-qualified class
    /// ([`qualified_class_table`]). `None` when the profile holds no tree for
    /// the source's root.
    #[must_use]
    pub fn file_of(self, repo_root: &Path, source: &str) -> Option<String> {
        let path = self.path_of(source)?;
        if self == Self::Released || repo_root.join(&path).is_file() {
            return Some(path);
        }
        Some(qualified_class_table(repo_root, &path).unwrap_or(path))
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

/// Returns the released-shaped source a development snapshot file stands for.
///
/// It is the inverse of [`SpecProfile::path_of`] under
/// [`SpecProfile::Development`]; `None` when `path` is not under
/// [`DEVELOPMENT_ROOT`] in a `specifications-<component>` directory.
///
/// A file development adds has no released counterpart, and the answer
/// names where the release would hold it, so its maturity is read from the
/// snapshot's manifest of the same component.
#[must_use]
pub fn snapshot_source(path: &str) -> Option<String> {
    let (component, inner) = path
        .strip_prefix(DEVELOPMENT_ROOT)?
        .strip_prefix('/')?
        .strip_prefix(REPOSITORY_PREFIX)?
        .split_once('/')?;
    if component.is_empty() || inner.is_empty() {
        return None;
    }
    let mirrored = MIRRORED_ROOTS
        .iter()
        .filter(|(_, mirrored, _)| *mirrored == component)
        .find_map(|(root, _, inside)| {
            inner
                .strip_prefix(inside)
                .map(|rest| format!("{root}{rest}"))
        });
    Some(mirrored.unwrap_or_else(|| format!("specs/openehr/{component}/{inner}")))
}

/// The prefix of a package-qualified class-table file name.
const QUALIFIED_CLASS: &str = "org.openehr.";

/// Finds the package-qualified class table for the class `path` names.
///
/// The answer is the one table under the `docs/UML/classes/` directory of
/// `path`, relative to `repo_root`, whose qualified name ends with that
/// class; `None` when `path` names no unqualified class table, or when no
/// single qualified table matches.
///
/// The release names a class table by its class
/// (`docs/UML/classes/locatable.adoc`); the specification repositories'
/// `master` names it with its package
/// (`docs/UML/classes/org.openehr.rm.common.locatable.adoc`), since the
/// republication with the upgraded UML extractor that qualifies every class
/// (openEHR/specifications-RM commit `ad2435db479d`). No openEHR spec
/// governs file names — our own design reads the two as one table.
#[must_use]
pub fn qualified_class_table(repo_root: &Path, path: &str) -> Option<String> {
    let (dir, name) = path.rsplit_once('/')?;
    if !dir.ends_with("/docs/UML/classes") || name.starts_with(QUALIFIED_CLASS) {
        return None;
    }
    let suffix = format!(".{name}");
    // NOTE: no openEHR spec governs this — our own design; a directory or an entry that does
    // not list holds no qualified table, so the caller reports the class table absent.
    let entries = std::fs::read_dir(repo_root.join(dir)).ok()?;
    let mut found = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|file| file.starts_with(QUALIFIED_CLASS) && file.ends_with(&suffix));
    let one = found.next()?;
    found.next().is_none().then(|| format!("{dir}/{one}"))
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
                "specs/openehr-development/specifications-RM/docs/common/master06-change_control_package.adoc"
            )
        );
        assert_eq!(
            dev.path_of("specs/openehr/RM/docs/UML/classes/locatable.adoc")
                .as_deref(),
            Some("specs/openehr-development/specifications-RM/docs/UML/classes/locatable.adoc")
        );
        assert_eq!(
            dev.path_of("specs/openehr/AM/docs/UML/classes/org.openehr.am.aom14.c_object.adoc")
                .as_deref(),
            Some(
                "specs/openehr-development/specifications-AM/docs/UML/classes/org.openehr.am.aom14.c_object.adoc"
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
            dev.path_of("specs/its-xml-schemas/its-xml-2.0.0-nsv2/AM/Release-1.4/Template.xsd")
                .as_deref(),
            Some(
                "specs/openehr-development/specifications-ITS-XML/components/AM/Release-1.4/Template.xsd"
            )
        );
        assert_eq!(
            SpecProfile::Released
                .path_of("specs/its-json-schemas/x.json")
                .as_deref(),
            Some("specs/its-json-schemas/x.json")
        );
    }

    #[test]
    fn every_its_xml_2_file_has_its_snapshot_counterpart() -> Result<(), Box<dyn std::error::Error>>
    {
        let bundle = "specs/its-xml-schemas/its-xml-2.0.0-nsv2";
        let mut stack = vec![root().join(bundle)];
        let mut files = 0_usize;
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let relative = path.strip_prefix(root())?.to_string_lossy().into_owned();
                let mapped = SpecProfile::Development
                    .path_of(&relative)
                    .ok_or("an ITS-XML 2.0.0 file maps")?;
                assert!(root().join(&mapped).is_file(), "{relative} -> {mapped}");
                assert_eq!(snapshot_source(&mapped).as_deref(), Some(relative.as_str()));
                files += 1;
            }
        }
        assert!(files > 0, "the bundle is vendored");
        Ok(())
    }

    #[test]
    fn a_snapshot_file_reads_back_as_the_source_it_stands_for() {
        assert_eq!(
            snapshot_source(
                "specs/openehr-development/specifications-RM/docs/ehr/master04-ehr.adoc"
            )
            .as_deref(),
            Some("specs/openehr/RM/docs/ehr/master04-ehr.adoc")
        );
        assert_eq!(
            snapshot_source(
                "specs/openehr-development/specifications-ITS-REST/computable/OAS/ehr.openapi.yaml"
            )
            .as_deref(),
            Some("specs/rest-oas/ehr.openapi.yaml")
        );
        assert_eq!(
            snapshot_source(
                "specs/openehr-development/specifications-ITS-REST/specifications/docs/overview/Resources.md"
            )
            .as_deref(),
            Some("specs/openehr/ITS-REST/specifications/docs/overview/Resources.md")
        );
        assert_eq!(
            snapshot_source("specs/openehr-development/specifications-ITS-JSON/components/x.json")
                .as_deref(),
            Some("specs/its-json-schemas/x.json")
        );
        assert_eq!(snapshot_source("specs/openehr/RM/docs/x.adoc"), None);
        assert_eq!(snapshot_source("specs/openehr-development/RM/x.adoc"), None);
        assert_eq!(
            snapshot_source("specs/openehr-development/specifications-RM"),
            None
        );
    }

    #[test]
    fn a_class_table_is_found_under_its_package_qualified_name() {
        let dev = SpecProfile::Development;
        assert_eq!(
            dev.file_of(&root(), "specs/openehr/RM/docs/UML/classes/locatable.adoc")
                .as_deref(),
            Some(
                "specs/openehr-development/specifications-RM/docs/UML/classes/org.openehr.rm.common.locatable.adoc"
            )
        );
        assert_eq!(
            dev.file_of(&root(), "specs/openehr/RM/docs/UML/classes/event.adoc")
                .as_deref(),
            Some(
                "specs/openehr-development/specifications-RM/docs/UML/classes/org.openehr.rm.data_structures.event.adoc"
            ),
            "a dot-bounded suffix never matches point_event or interval_event"
        );
        assert_eq!(
            dev.file_of(
                &root(),
                "specs/openehr/RM/docs/UML/classes/no_such_class.adoc"
            )
            .as_deref(),
            Some("specs/openehr-development/specifications-RM/docs/UML/classes/no_such_class.adoc"),
            "an unmatched class keeps its mapped path, which the checker reports absent"
        );
        assert_eq!(
            SpecProfile::Released
                .file_of(&root(), "specs/openehr/RM/docs/UML/classes/locatable.adoc")
                .as_deref(),
            Some("specs/openehr/RM/docs/UML/classes/locatable.adoc")
        );
    }

    #[test]
    fn a_class_name_two_packages_share_matches_neither() -> Result<(), Box<dyn std::error::Error>> {
        let dir = assert_fs::TempDir::new()?;
        let classes = dir
            .path()
            .join("specs/openehr-development/specifications-RM/docs/UML/classes");
        std::fs::create_dir_all(&classes)?;
        std::fs::write(classes.join("org.openehr.rm.a.thing.adoc"), "x")?;
        std::fs::write(classes.join("org.openehr.rm.b.thing.adoc"), "x")?;
        assert_eq!(
            qualified_class_table(
                dir.path(),
                "specs/openehr-development/specifications-RM/docs/UML/classes/thing.adoc"
            ),
            None
        );
        Ok(())
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
            assert_eq!(component.branch, "master", "{component:?}");
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
            matches!(&refused, Err(NotReleased { label }) if label.starts_with("development snapshot (AM master@")),
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
