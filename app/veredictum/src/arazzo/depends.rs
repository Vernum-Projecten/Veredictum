// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Following a workflow's `dependsOn`: one resolver and one walker.
//!
//! Arazzo 1.1.0 §Workflow Object `dependsOn` names a workflow of the same
//! document by its `workflowId`, or one of another document as
//! `$sourceDescriptions.<name>.<workflowId>` of a `type: arazzo` source. A
//! [`Library`] holds the documents a caller starts from and loads, at any
//! depth, every `type: arazzo` source they name, wherever its file lives.
//! Each document is keyed by its canonical path, so a source reached through
//! `../` and the same file listed under the catalogue are one document, and
//! two files that share a name are two.
//!
//! [`Library::resolve`] maps one entry to its [`Target`] or says why it is
//! [`Unresolvable`]. [`Library::walk`] visits a workflow and every workflow
//! it reaches, depth first in `dependsOn` order, each once per [`Seen`], and
//! passes an entry that does not resolve by: the document gate refuses that
//! entry on its own. No openEHR spec governs this — our own design.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::ControlFlow;
use std::path::{Component, Path, PathBuf};

use roas_arazzo::v1_1::Workflow;

use crate::arazzo::document::{ArazzoDocument, DocumentError};
use crate::arazzo::source::split_qualified;

/// A document's identity: its canonical path, or, for a file that does not
/// exist, its path with every `.` and `..` folded away.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DocumentKey(PathBuf);

impl DocumentKey {
    /// The key of the document at `path`.
    #[must_use]
    pub fn of(path: &Path) -> Self {
        // NOTE: no openEHR spec governs this — our own design; a path that names no file
        // on disk still identifies a document built in memory, so it folds lexically.
        Self(std::fs::canonicalize(path).unwrap_or_else(|_| lexical(path)))
    }

    /// The path the key holds.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// `path` with each `.` dropped and each `..` folding the component before it.
fn lexical(path: &Path) -> PathBuf {
    let mut folded = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir
                if matches!(folded.components().next_back(), Some(Component::Normal(_))) =>
            {
                folded.pop();
            }
            other => folded.push(other),
        }
    }
    folded
}

/// Why a `dependsOn` entry names no workflow.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unresolvable {
    /// A same-document entry names no workflow of the document.
    #[error("`{workflow_id}` is not a workflow of this document")]
    NoLocalWorkflow {
        /// The workflow the entry names.
        workflow_id: String,
    },
    /// A qualified entry names a source that is not a `type: arazzo` source
    /// of the document.
    #[error("`{name}` is not a `type: arazzo` source of this document")]
    NotArazzoSource {
        /// The source the entry names.
        name: String,
    },
    /// The source's file was never followed into the library.
    #[error("`{}` was not loaded", file.display())]
    NotFollowed {
        /// The source's file.
        file: PathBuf,
    },
    /// The source's file does not load as an Arazzo document.
    #[error("`{}` does not load: {reason}", file.display())]
    NotLoaded {
        /// The source's file.
        file: PathBuf,
        /// Why it does not load.
        reason: String,
    },
    /// The source's document holds no workflow of that id.
    #[error("`{workflow_id}` is not a workflow of `{}`", file.display())]
    NoWorkflow {
        /// The source's file.
        file: PathBuf,
        /// The workflow the entry names.
        workflow_id: String,
    },
}

/// The workflow a `dependsOn` entry names, with the document holding it.
#[derive(Debug, Clone, Copy)]
pub struct Target<'s> {
    /// The document holding the workflow.
    pub document: &'s ArazzoDocument,
    /// The workflow.
    pub workflow: &'s Workflow,
    /// The `type: arazzo` source a qualified entry names; `None` for an
    /// entry of the same document.
    pub source: Option<&'s str>,
}

/// One document the library holds.
#[derive(Debug)]
enum Held<'a> {
    /// A document the caller started from.
    Root(&'a ArazzoDocument),
    /// A source document the library loaded.
    Loaded(Box<ArazzoDocument>),
    /// A source document that does not load, with why.
    Failed(String),
}

/// The documents a set of `dependsOn` entries can name, keyed by
/// [`DocumentKey`].
#[derive(Debug, Default)]
pub struct Library<'a> {
    held: BTreeMap<DocumentKey, Held<'a>>,
    keys: BTreeMap<PathBuf, DocumentKey>,
}

impl<'a> Library<'a> {
    /// A library over `roots` and every `type: arazzo` source they name, at
    /// any depth. A source that does not load is kept with its reason, which
    /// [`Library::resolve`] reports.
    #[must_use]
    pub fn new(roots: impl IntoIterator<Item = &'a ArazzoDocument>) -> Self {
        let mut library = Self::default();
        let pending = library.hold_roots(roots);
        // NOTE: no openEHR spec governs this — our own design; a source that does not load
        // is the document gate's finding, and resolving an entry into it says why.
        library.load(pending);
        library
    }

    /// A library like [`Library::new`] that refuses a source that does not
    /// load.
    ///
    /// # Errors
    /// The first [`DocumentError`] of a `type: arazzo` source that does not
    /// load, breadth first from the roots and in source-name order.
    pub fn strict(
        roots: impl IntoIterator<Item = &'a ArazzoDocument>,
    ) -> Result<Self, DocumentError> {
        let mut library = Self::default();
        let pending = library.hold_roots(roots);
        match library.load(pending).into_iter().next() {
            Some(error) => Err(error),
            None => Ok(library),
        }
    }

    /// Loads every `type: arazzo` source `document` names, at any depth,
    /// that the library does not hold yet; `document` itself is not held.
    pub fn follow(&mut self, document: &ArazzoDocument) {
        let pending = document.workflow_sources().values().cloned().collect();
        // NOTE: no openEHR spec governs this — our own design; as in `new`, a source that
        // does not load is kept with its reason.
        self.load(pending);
    }

    /// The key of the document at `path`, without touching the disk when
    /// the library has met the path.
    #[must_use]
    pub fn key(&self, path: &Path) -> DocumentKey {
        self.keys
            .get(path)
            .cloned()
            .unwrap_or_else(|| DocumentKey::of(path))
    }

    /// Every document the library holds that loaded, roots and sources
    /// alike, in [`DocumentKey`] order.
    pub fn documents(&self) -> impl Iterator<Item = &ArazzoDocument> {
        self.held.values().filter_map(|held| match held {
            Held::Root(document) => Some(*document),
            Held::Loaded(document) => Some(&**document),
            Held::Failed(_) => None,
        })
    }

    /// The workflow the `dependsOn` entry `entry` of a workflow of `from`
    /// names.
    ///
    /// # Errors
    /// [`Unresolvable`] naming why the entry names no workflow.
    pub fn resolve<'s>(
        &'s self,
        from: &'s ArazzoDocument,
        entry: &str,
    ) -> Result<Target<'s>, Unresolvable> {
        let Some((source, workflow_id)) = split_qualified(entry) else {
            return from
                .workflow(entry)
                .map(|workflow| Target {
                    document: from,
                    workflow,
                    source: None,
                })
                .ok_or_else(|| Unresolvable::NoLocalWorkflow {
                    workflow_id: entry.to_owned(),
                });
        };
        let (name, file) = from
            .workflow_sources()
            .get_key_value(source)
            .ok_or_else(|| Unresolvable::NotArazzoSource {
                name: source.to_owned(),
            })?;
        let document = match self.keys.get(file).and_then(|key| self.held.get(key)) {
            Some(Held::Root(document)) => *document,
            Some(Held::Loaded(document)) => &**document,
            Some(Held::Failed(reason)) => {
                return Err(Unresolvable::NotLoaded {
                    file: file.clone(),
                    reason: reason.clone(),
                });
            }
            None => return Err(Unresolvable::NotFollowed { file: file.clone() }),
        };
        let workflow = document
            .workflow(workflow_id)
            .ok_or_else(|| Unresolvable::NoWorkflow {
                file: file.clone(),
                workflow_id: workflow_id.to_owned(),
            })?;
        Ok(Target {
            document,
            workflow,
            source: Some(name.as_str()),
        })
    }

    /// Visits `workflow` of `document`, then every workflow it reaches
    /// through `dependsOn`, depth first in `dependsOn` order, skipping each
    /// one `seen` already holds; the first [`ControlFlow::Break`] ends the
    /// walk and is returned.
    pub fn walk<'s, T>(
        &'s self,
        seen: &mut Seen,
        document: &'s ArazzoDocument,
        workflow: &'s Workflow,
        visit: &mut impl FnMut(Visit<'_, 's>) -> ControlFlow<T>,
    ) -> Option<T> {
        self.walk_from(seen, &mut Vec::new(), document, workflow, visit)
    }

    fn walk_from<'s, T>(
        &'s self,
        seen: &mut Seen,
        path: &mut Vec<(&'s ArazzoDocument, &'s Workflow)>,
        document: &'s ArazzoDocument,
        workflow: &'s Workflow,
        visit: &mut impl FnMut(Visit<'_, 's>) -> ControlFlow<T>,
    ) -> Option<T> {
        if !seen.insert(self.key(document.path()), &workflow.workflow_id) {
            return None;
        }
        let node = Visit {
            document,
            workflow,
            path,
        };
        if let ControlFlow::Break(found) = visit(node) {
            return Some(found);
        }
        path.push((document, workflow));
        for entry in &workflow.depends_on {
            // NOTE: no openEHR spec governs this — our own design; an entry that does not
            // resolve is the document gate's finding, so the walk passes it by.
            let Ok(target) = self.resolve(document, entry) else {
                continue;
            };
            let found = self.walk_from(seen, path, target.document, target.workflow, visit);
            if found.is_some() {
                return found;
            }
        }
        path.pop();
        None
    }

    /// Holds `roots`, returning the source files they name.
    fn hold_roots(
        &mut self,
        roots: impl IntoIterator<Item = &'a ArazzoDocument>,
    ) -> VecDeque<PathBuf> {
        let mut pending = VecDeque::new();
        for root in roots {
            let key = DocumentKey::of(root.path());
            self.keys.insert(root.path().to_owned(), key.clone());
            self.held.entry(key).or_insert(Held::Root(root));
            pending.extend(root.workflow_sources().values().cloned());
        }
        pending
    }

    /// Loads each of `pending` and every source it names, breadth first,
    /// that the library does not hold yet; returns each one that does not
    /// load.
    fn load(&mut self, mut pending: VecDeque<PathBuf>) -> Vec<DocumentError> {
        let mut failures = Vec::new();
        while let Some(file) = pending.pop_front() {
            let key = self.key(&file);
            self.keys.insert(file.clone(), key.clone());
            if self.held.contains_key(&key) {
                continue;
            }
            match ArazzoDocument::load(&file) {
                Ok(document) => {
                    pending.extend(document.workflow_sources().values().cloned());
                    self.held.insert(key, Held::Loaded(Box::new(document)));
                }
                Err(error) => {
                    self.held.insert(key, Held::Failed(error.to_string()));
                    failures.push(error);
                }
            }
        }
        failures
    }
}

/// The workflows a walk has visited, by document and workflow id.
#[derive(Debug, Default)]
pub struct Seen(BTreeSet<(DocumentKey, String)>);

impl Seen {
    /// Records one workflow; `false` when it was recorded before.
    fn insert(&mut self, key: DocumentKey, workflow_id: &str) -> bool {
        self.0.insert((key, workflow_id.to_owned()))
    }
}

/// One workflow a walk visits.
#[derive(Debug, Clone, Copy)]
pub struct Visit<'v, 's> {
    /// The document holding the workflow.
    pub document: &'s ArazzoDocument,
    /// The workflow.
    pub workflow: &'s Workflow,
    /// The workflows between the walk's start and this one, the start first;
    /// empty for the start itself.
    pub path: &'v [(&'s ArazzoDocument, &'s Workflow)],
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;

    const OAS: &str = "openapi: 3.0.3\npaths:\n  /ehr:\n    post:\n      operationId: ehr_create\n";

    /// A document over the seeded OAS whose `type: arazzo` sources are
    /// `sources` (`name: url`) and whose workflows are `(id, dependsOn)`.
    fn document(sources: &[(&str, &str)], workflows: &[(&str, &str)]) -> String {
        use std::fmt::Write as _;
        let mut listed = String::new();
        for (name, url) in sources {
            writeln!(listed, "  - {{ name: {name}, url: {url}, type: arazzo }}").unwrap();
        }
        let sources = listed;
        let mut listed = String::new();
        for (id, depends_on) in workflows {
            write!(
                listed,
                "  - workflowId: {id}\n    dependsOn: [{depends_on}]\n    steps:\n      - stepId: create\n        operationId: $sourceDescriptions.ehr.ehr_create\n        successCriteria:\n          - condition: $statusCode == 201\n"
            )
            .unwrap();
        }
        let workflows = listed;
        format!(
            "arazzo: 1.1.0\ninfo: {{ title: t, version: '1' }}\nsourceDescriptions:\n  - {{ name: ehr, url: /OAS, type: openapi }}\n{sources}workflows:\n{workflows}"
        )
    }

    /// Writes `text` at `at`, its OAS source `oas` resolving from there.
    fn write(dir: &assert_fs::TempDir, at: &str, oas: &str, text: &str) {
        dir.child(at).write_str(&text.replace("/OAS", oas)).unwrap();
    }

    /// `cases/a/a.arazzo.yaml` naming `cases/_setup/s.arazzo.yaml` through
    /// `../_setup/` and `outside/s.arazzo.yaml`, both with the stem `s`.
    fn world() -> assert_fs::TempDir {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("oas.yaml").write_str(OAS).unwrap();
        write(
            &dir,
            "cases/a/a.arazzo.yaml",
            "../../oas.yaml",
            &document(
                &[
                    ("inside", "../_setup/s.arazzo.yaml"),
                    ("outside", "../../outside/s.arazzo.yaml"),
                    ("broken", "../../missing.arazzo.yaml"),
                ],
                &[
                    ("case", "local, $sourceDescriptions.inside.w"),
                    ("local", "$sourceDescriptions.outside.far"),
                    ("cycle", "cycle"),
                ],
            ),
        );
        write(
            &dir,
            "cases/_setup/s.arazzo.yaml",
            "../../oas.yaml",
            &document(
                &[("back", "../a/a.arazzo.yaml")],
                &[("w", "$sourceDescriptions.back.case")],
            ),
        );
        write(
            &dir,
            "outside/s.arazzo.yaml",
            "../oas.yaml",
            &document(&[], &[("far", "")]),
        );
        dir
    }

    #[test]
    fn an_entry_resolves_by_canonical_path_or_names_why_it_does_not() {
        let dir = world();
        let a = ArazzoDocument::load(&dir.path().join("cases/a/a.arazzo.yaml")).unwrap();
        let library = Library::new([&a]);
        let local = library.resolve(&a, "local").unwrap();
        assert!(std::ptr::eq(local.document, &raw const a));
        assert_eq!(local.source, None);
        let inside = library.resolve(&a, "$sourceDescriptions.inside.w").unwrap();
        assert_eq!(inside.source, Some("inside"));
        assert!(inside.document.path().ends_with("_setup/s.arazzo.yaml"));
        let outside = library
            .resolve(&a, "$sourceDescriptions.outside.far")
            .unwrap();
        assert!(outside.document.path().ends_with("outside/s.arazzo.yaml"));
        let back = library
            .resolve(inside.document, "$sourceDescriptions.back.case")
            .unwrap();
        assert!(
            std::ptr::eq(back.document, &raw const a),
            "a source naming a root by `../` is the root"
        );
        assert_eq!(
            library.resolve(&a, "nothing").unwrap_err(),
            Unresolvable::NoLocalWorkflow {
                workflow_id: "nothing".to_owned()
            }
        );
        assert_eq!(
            library
                .resolve(&a, "$sourceDescriptions.ehr.ehr_create")
                .unwrap_err(),
            Unresolvable::NotArazzoSource {
                name: "ehr".to_owned()
            }
        );
        assert!(matches!(
            library.resolve(&a, "$sourceDescriptions.outside.near").unwrap_err(),
            Unresolvable::NoWorkflow { workflow_id, .. } if workflow_id == "near"
        ));
        assert!(matches!(
            library
                .resolve(&a, "$sourceDescriptions.broken.w")
                .unwrap_err(),
            Unresolvable::NotLoaded { .. }
        ));
        assert!(Library::strict([&a]).is_err());
        let unfollowed = Library::default();
        assert!(matches!(
            unfollowed
                .resolve(&a, "$sourceDescriptions.inside.w")
                .unwrap_err(),
            Unresolvable::NotFollowed { .. }
        ));
    }

    #[test]
    fn a_walk_visits_each_reached_workflow_once_and_ends_on_a_cycle() {
        let dir = world();
        let a = ArazzoDocument::load(&dir.path().join("cases/a/a.arazzo.yaml")).unwrap();
        let library = Library::new([&a]);
        let visited = |id: &str| {
            let mut out = Vec::new();
            let start = a.workflow(id).unwrap();
            let none: Option<()> = library.walk(&mut Seen::default(), &a, start, &mut |v| {
                out.push((v.workflow.workflow_id.clone(), v.path.len()));
                ControlFlow::Continue(())
            });
            assert!(none.is_none());
            out
        };
        assert_eq!(
            visited("case"),
            [
                ("case".to_owned(), 0),
                ("local".to_owned(), 1),
                ("far".to_owned(), 2),
                ("w".to_owned(), 1),
            ]
        );
        assert_eq!(visited("cycle"), [("cycle".to_owned(), 0)]);
        let start = a.workflow("case").unwrap();
        let found = library.walk(&mut Seen::default(), &a, start, &mut |v| {
            if v.workflow.workflow_id == "far" {
                ControlFlow::Break(
                    v.path
                        .iter()
                        .map(|(_, w)| w.workflow_id.clone())
                        .collect::<Vec<_>>(),
                )
            } else {
                ControlFlow::Continue(())
            }
        });
        assert_eq!(found, Some(vec!["case".to_owned(), "local".to_owned()]));
    }

    #[test]
    fn a_missing_file_keys_by_its_folded_path() {
        assert_eq!(
            DocumentKey::of(Path::new("/nowhere/a/./b/../c.arazzo.yaml")).path(),
            Path::new("/nowhere/a/c.arazzo.yaml")
        );
    }
}
