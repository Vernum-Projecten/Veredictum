// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Whether a cited `AsciiDoc` file of a component's documents is published:
//! reachable through the active `include::` directives of the `master.adoc`
//! of a current document its manifest lists.
//!
//! A file no published document includes is text the release does not
//! publish, such as RM `docs/common/master07-tags.adoc`, whose include the
//! released Common IM `master.adoc` comments out. No openEHR spec governs how
//! the instrument reads the includes — our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::arazzo::maturity::{Maturities, Maturity, MaturityError, SpecStatus};
use crate::arazzo::spec_profile::{SpecProfile, snapshot_source};

/// The document attribute every component's documents name their UML class
/// tables by, which the published build sets to the component's `docs/UML`.
const UML_EXPORT_DIR: &str = "{uml_export_dir}/";

/// The published files of each component directory, read once.
#[derive(Debug, Default)]
pub struct Publication {
    published: BTreeMap<String, BTreeSet<String>>,
}

/// The component a file of a component's documents belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsFile {
    /// The profile whose tree holds the file.
    pub profile: SpecProfile,
    /// The component, as its folder under `specs/openehr/` names it.
    pub component: String,
}

impl DocsFile {
    /// The component of `path`, a repository-relative file, when it is an
    /// `AsciiDoc` file under a component's `docs/`: of the release under
    /// `specs/openehr/`, or of the development snapshot. `None` for any
    /// other file.
    #[must_use]
    pub fn of(path: &str) -> Option<Self> {
        let is_adoc = Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("adoc"));
        if !is_adoc {
            return None;
        }
        let (profile, shaped) = match snapshot_source(path) {
            Some(shaped) => (SpecProfile::Development, shaped),
            None => (SpecProfile::Released, path.to_owned()),
        };
        let (component, inner) = shaped.strip_prefix("specs/openehr/")?.split_once('/')?;
        inner.starts_with("docs/").then(|| Self {
            profile,
            component: component.to_owned(),
        })
    }
}

impl Publication {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `path`, a repository-relative `AsciiDoc` file under `docs`
    /// of `docs`'s component, is reachable from the `master.adoc` of a
    /// document the component's manifest lists and does not mark RETIRED or
    /// SUPERSEDED.
    ///
    /// # Errors
    /// The [`MaturityError`] of the component's manifest or of one of its
    /// documents' statuses, [`MaturityError::NoManifest`] included.
    pub fn is_published(
        &mut self,
        repo_root: &Path,
        maturities: &mut Maturities,
        docs: &DocsFile,
        path: &str,
    ) -> Result<bool, MaturityError> {
        let dir = docs.profile.component_dir(&docs.component);
        if !self.published.contains_key(&dir) {
            let mut masters = Vec::new();
            for id in maturities.document_ids(docs.profile, &docs.component)? {
                let master = format!("{dir}/docs/{id}/master.adoc");
                if !repo_root.join(&master).is_file() {
                    continue;
                }
                let current = !matches!(
                    maturities.of(&master)?,
                    Maturity::Document {
                        status: SpecStatus::Retired | SpecStatus::Superseded,
                        ..
                    }
                );
                if current {
                    masters.push(master);
                }
            }
            let reached = reachable(repo_root, &dir, &masters);
            self.published.insert(dir.clone(), reached);
        }
        Ok(self
            .published
            .get(&dir)
            .is_some_and(|reached| reached.contains(path)))
    }
}

/// Every file the `masters` reach through their active `include::`
/// directives, the masters included, as repository-relative paths.
fn reachable(repo_root: &Path, component_dir: &str, masters: &[String]) -> BTreeSet<String> {
    let mut reached = BTreeSet::new();
    for master in masters {
        let mut attributes = BTreeMap::new();
        walk(
            repo_root,
            component_dir,
            master,
            &mut attributes,
            &mut reached,
        );
    }
    reached
}

/// Marks `file` reached and walks the `AsciiDoc` it includes, in document
/// order, with the attributes its lines define before each include.
fn walk(
    repo_root: &Path,
    component_dir: &str,
    file: &str,
    attributes: &mut BTreeMap<String, String>,
    reached: &mut BTreeSet<String>,
) {
    if !reached.insert(file.to_owned()) {
        return;
    }
    let is_adoc = Path::new(file)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("adoc"));
    if !is_adoc {
        return;
    }
    // NOTE: no openEHR spec governs this — our own design; a file that does not read includes
    // nothing, so what only it would include stays unpublished.
    let Ok(text) = std::fs::read_to_string(repo_root.join(file)) else {
        return;
    };
    let dir = file.rsplit_once('/').map_or("", |(dir, _)| dir);
    for line in active_lines(&text) {
        if let Some((name, value)) = attribute_entry(line) {
            attributes.insert(name.to_owned(), value.to_owned());
            continue;
        }
        let Some(target) = line
            .strip_prefix("include::")
            .and_then(|rest| rest.split_once('['))
            .map(|(target, _)| target)
        else {
            continue;
        };
        let (base, target) = match target.strip_prefix(UML_EXPORT_DIR) {
            Some(rest) => (format!("{component_dir}/docs/UML"), rest),
            None => (dir.to_owned(), target),
        };
        let Some(target) = substitute(target, attributes) else {
            continue;
        };
        if let Some(included) = normalize(&format!("{base}/{target}")) {
            walk(repo_root, component_dir, &included, attributes, reached);
        }
    }
}

/// The lines of `text` outside every comment: a `//` line comment and a
/// `////` comment block (<https://docs.asciidoctor.org/asciidoc/latest/comments/>).
fn active_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut in_block = false;
    text.lines().filter(move |line| {
        let line = line.trim_end();
        if line.len() >= 4 && line.chars().all(|c| c == '/') {
            in_block = !in_block;
            return false;
        }
        !in_block && !line.starts_with("//")
    })
}

/// The name and value of an attribute entry line, `:name: value`
/// (<https://docs.asciidoctor.org/asciidoc/latest/attributes/attribute-entries/>).
fn attribute_entry(line: &str) -> Option<(&str, &str)> {
    let (name, value) = line.strip_prefix(':')?.split_once(':')?;
    let named = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    named.then(|| (name, value.trim()))
}

/// `target` with every `{name}` replaced by its attribute's value; `None`
/// when it names an attribute no line defines, which leaves the include
/// pointing outside the vendored tree.
fn substitute(target: &str, attributes: &BTreeMap<String, String>) -> Option<String> {
    let mut out = String::with_capacity(target.len());
    let mut rest = target;
    while let Some((before, after)) = rest.split_once('{') {
        out.push_str(before);
        let (name, tail) = after.split_once('}')?;
        out.push_str(attributes.get(name)?);
        rest = tail;
    }
    out.push_str(rest);
    Some(out)
}

/// `path` with its `.` and `..` segments resolved; `None` when it climbs
/// above the repository root.
fn normalize(path: &str) -> Option<String> {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            segment => segments.push(segment),
        }
    }
    Some(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn published(root: &Path, path: &str) -> Result<bool, MaturityError> {
        let docs = DocsFile::of(path).ok_or(MaturityError::NoManifest(path.to_owned()))?;
        let mut maturities = Maturities::in_profile(root, docs.profile);
        Publication::new().is_published(root, &mut maturities, &docs, path)
    }

    fn vendored() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn the_released_tags_chapter_and_its_class_table_are_unpublished() {
        let root = vendored();
        for path in [
            "specs/openehr/RM/docs/UML/classes/item_tag.adoc",
            "specs/openehr/RM/docs/common/master07-tags.adoc",
        ] {
            assert_eq!(published(&root, path), Ok(false), "{path}");
        }
    }

    #[test]
    fn an_included_chapter_and_a_class_table_it_includes_are_published() {
        let root = vendored();
        for path in [
            "specs/openehr/RM/docs/common/master.adoc",
            "specs/openehr/RM/docs/common/master06-change_control_package.adoc",
            "specs/openehr/RM/docs/UML/classes/versioned_object.adoc",
            "specs/openehr/AM/docs/UML/classes/org.openehr.am.aom14.archetype.adoc",
        ] {
            assert_eq!(published(&root, path), Ok(true), "{path}");
        }
    }

    #[test]
    fn the_snapshot_publishes_the_tags_chapter_through_its_package_attribute() {
        let root = vendored();
        assert_eq!(
            published(
                &root,
                "specs/openehr-development/specifications-RM/docs/UML/classes/org.openehr.rm.common.item_tag.adoc"
            ),
            Ok(true)
        );
    }

    #[test]
    fn only_adoc_under_a_components_docs_is_docs_text() {
        assert_eq!(
            DocsFile::of("specs/openehr/RM/docs/UML/classes/item_tag.adoc"),
            Some(DocsFile {
                profile: SpecProfile::Released,
                component: "RM".to_owned()
            })
        );
        assert_eq!(
            DocsFile::of("specs/openehr-development/specifications-QUERY/docs/AQL/master.adoc")
                .map(|d| d.profile),
            Some(SpecProfile::Development)
        );
        for other in [
            "specs/openehr/ITS-REST/specifications/docs/overview/Resources.md",
            "specs/openehr/ITS-REST/specifications/ehr.openapi.yaml",
            "specs/rest-oas/ehr.openapi.yaml",
            "specs/openehr/RM/README.adoc",
        ] {
            assert_eq!(DocsFile::of(other), None, "{other}");
        }
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    fn a_seeded_tree_publishes_only_what_an_active_include_reaches()
    -> Result<(), Box<dyn std::error::Error>> {
        use assert_fs::prelude::*;
        let dir = assert_fs::TempDir::new()?;
        let c = "specs/openehr/X";
        dir.child(format!("{c}/manifest.json")).write_str(
            r#"{"specifications": [{"id": "doc", "spec_status": "STABLE"}, {"id": "old", "spec_status": "RETIRED"}]}"#,
        )?;
        dir.child(format!("{c}/docs/doc/master.adoc")).write_str(
            "= Doc\n:pkg: org.x.\ninclude::ch1.adoc[leveloffset=+1]\n// include::ch2.adoc[]\n////\ninclude::ch3.adoc[]\n////\ninclude::{ref_dir}/x.adoc[]\n",
        )?;
        dir.child(format!("{c}/docs/doc/ch1.adoc")).write_str(
            "== One\ninclude::{uml_export_dir}/classes/{pkg}a.adoc[]\ninclude::../shared/s.adoc[]\n",
        )?;
        for file in [
            "docs/doc/ch2.adoc",
            "docs/doc/ch3.adoc",
            "docs/UML/classes/org.x.a.adoc",
            "docs/UML/classes/org.x.b.adoc",
            "docs/shared/s.adoc",
            "docs/old/only.adoc",
        ] {
            dir.child(format!("{c}/{file}")).write_str("== T\n")?;
        }
        dir.child(format!("{c}/docs/old/master.adoc")).write_str(
            "include::{uml_export_dir}/classes/org.x.b.adoc[]\ninclude::only.adoc[]\n",
        )?;
        let root = dir.path();
        for (file, want) in [
            ("docs/doc/master.adoc", true),
            ("docs/doc/ch1.adoc", true),
            ("docs/UML/classes/org.x.a.adoc", true),
            ("docs/shared/s.adoc", true),
            ("docs/doc/ch2.adoc", false),
            ("docs/doc/ch3.adoc", false),
            ("docs/UML/classes/org.x.b.adoc", false),
            ("docs/old/only.adoc", false),
        ] {
            assert_eq!(published(root, &format!("{c}/{file}"))?, want, "{file}");
        }
        assert!(matches!(
            published(root, "specs/openehr/Y/docs/doc/master.adoc"),
            Err(MaturityError::NoManifest(_))
        ));
        Ok(())
    }
}
