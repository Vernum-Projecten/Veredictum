// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The pure assertion evaluators the Arazzo judges wrap.
//!
//! RM document equivalence, `ORIGINAL_VERSION.signature` facts and AQL
//! result-set comparison, each verdict logic over canonical-JSON values so a
//! re-judged recording computes the same verdict.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use serde_json::Value;
use std::collections::BTreeMap;

use crate::exec::resultset;
use crate::model::assertion::ColumnSpec;
use crate::vocab::{CellComparison, ResultSetMatch};

/// One assertion failure (stable, human-readable — lands in the outcome
/// record and the report).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionFailure(pub String);

/// Which channel an assertion failure belongs to.
///
/// A finding against the server needs a value the server actually served. An
/// assertion the run could not judge at all proves nothing about the SUT, so
/// it takes the inconclusive channel beside a transport fault (ISO/IEC 9646
/// *inconclusive*; interpreter law (c), [`crate::exec`]).
///
/// The two are distinguished as types rather than by reading the message,
/// because a classification branching on a substring changes the moment a
/// message is reworded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertionOutcome {
    /// The SUT served a value, and it differs from the asserted one — a
    /// conformance finding, so the row FAILS (law b).
    Mismatch(String),
    /// The assertion cannot be judged on this ITS or on this run: the fact
    /// has no released read, the container the assertion names resolves to
    /// no single family, the authored pattern carries a token outside its
    /// closed vocabulary, or a prerequisite the assertion reads was never
    /// bound. The row is INCONCLUSIVE (law c), attributed to the runner or
    /// the catalogue, never to the server.
    Unjudgeable(String),
}

impl AssertionOutcome {
    /// The one-line reason, whichever channel this outcome carries.
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            Self::Mismatch(reason) | Self::Unjudgeable(reason) => reason,
        }
    }
}

impl From<AssertionFailure> for AssertionOutcome {
    /// A pure judge compares a value the SUT SERVED against the authored one,
    /// so its failure is a conformance mismatch by construction. Only the
    /// wire-side resolution that precedes a judge can be unjudgeable, and
    /// those sites name the variant themselves.
    fn from(failure: AssertionFailure) -> Self {
        Self::Mismatch(failure.0)
    }
}

/// Resolve an RM path segment sequence (`context/setting`,
/// `content[0]/data/events[0]/...`) over a canonical-JSON value.
///
/// Supported addressing: object attributes and `[<index>]` list positions —
/// the subset the catalogue's field assertions use.
#[must_use]
pub fn resolve_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = root;
    for raw in path.split('/').filter(|s| !s.is_empty()) {
        let (attr, index) = match raw.split_once('[') {
            Some((attr, rest)) => {
                let index: usize = rest.strip_suffix(']')?.parse().ok()?;
                (attr, Some(index))
            }
            None => (raw, None),
        };
        if !attr.is_empty() {
            current = current.get(attr)?;
        }
        if let Some(i) = index {
            current = current.get(i)?;
        }
    }
    Some(current)
}

/// Strip the named ignore-sets and explicit paths from a value (top-level
/// path removal; nested server-assigned paths use `/`-separated forms).
///
/// A `**` segment matches zero or more intervening attribute steps, so
/// `**/uid` names one attribute wherever it occurs in a recursive RM
/// structure. Recursive containment is an RM shape, not a fixture depth:
/// `FOLDER.folders` is `List<FOLDER>` (RM common `folder.adoc`), so an
/// ignore-set that could only be written per depth would silently
/// under-cover a deeper tree.
#[must_use]
pub fn strip_ignored(value: &Value, ignored_paths: &[String]) -> Value {
    fn remove(value: &mut Value, segments: &[&str]) {
        let Some((head, rest)) = segments.split_first() else {
            return;
        };
        if *head == "**" {
            // Zero intervening steps: apply the remainder here …
            remove(value, rest);
            // … or one-or-more: descend and retry the same pattern.
            match value {
                Value::Object(map) => {
                    for child in map.values_mut() {
                        remove(child, segments);
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        remove(item, segments);
                    }
                }
                _ => {}
            }
            return;
        }
        match value {
            Value::Object(map) => {
                if rest.is_empty() {
                    map.remove(*head);
                } else if let Some(next) = map.get_mut(*head) {
                    remove(next, rest);
                }
            }
            Value::Array(items) => {
                for item in items {
                    remove(item, segments);
                }
            }
            _ => {}
        }
    }
    let mut out = value.clone();
    for path in ignored_paths {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        remove(&mut out, &segments);
    }
    out
}

/// A FLAT body: a one-level object whose keys are path-formed strings.
fn is_flat_map(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            !map.is_empty()
                && map.keys().any(|k| k.contains('/'))
                && map.values().all(|v| !matches!(v, Value::Object(_)))
        }
        _ => false,
    }
}

/// Fold a committed FLAT document's `ctx/*` input-convenience keys onto the
/// RM-tree flat paths a read-back expresses them at, per ITS-REST
/// `simplified_formats` master06 §Context Information: `ctx/participation_*`
/// → `{root}/context/_participation:{i}|*`, `ctx/health_care_facility|*` →
/// `{root}/context/_health_care_facility|*`, and the declared
/// `ctx/id_namespace`/`ctx/id_scheme` defaults expand onto every folded
/// party that carries an `|id` ("default namespace/scheme for external
/// references" — master06 §ID Namespace and Scheme). Pure default-setters
/// whose targets the ignore-set covers (`ctx/time`, `ctx/setting`,
/// composer) pass through untouched for the ignore pass.
fn fold_flat_ctx(committed: &BTreeMap<String, Value>, root: &str) -> BTreeMap<String, Value> {
    let id_namespace = committed.get("ctx/id_namespace").cloned();
    let id_scheme = committed.get("ctx/id_scheme").cloned();
    let mut out: BTreeMap<String, Value> = BTreeMap::new();
    let mut folded_id_carriers: Vec<String> = Vec::new();
    for (key, value) in committed {
        if let Some(rest) = key.strip_prefix("ctx/participation_") {
            // participation_<field>[:i] — default index 0.
            let (field, index) = match rest.split_once(':') {
                Some((f, i)) => (f, i),
                None => (rest, "0"),
            };
            let target = format!("{root}/context/_participation:{index}|{field}");
            if field == "id" {
                folded_id_carriers.push(format!("{root}/context/_participation:{index}"));
            }
            out.insert(target, value.clone());
        } else if let Some(rest) = key.strip_prefix("ctx/health_care_facility|") {
            if rest == "id" {
                folded_id_carriers.push(format!("{root}/context/_health_care_facility"));
            }
            out.insert(
                format!("{root}/context/_health_care_facility|{rest}"),
                value.clone(),
            );
        } else if key == "ctx/id_namespace" || key == "ctx/id_scheme" {
            // consumed as qualifiers below
        } else {
            out.insert(key.clone(), value.clone());
        }
    }
    for carrier in folded_id_carriers {
        if let Some(ns) = &id_namespace {
            out.insert(format!("{carrier}|id_namespace"), ns.clone());
        }
        if let Some(scheme) = &id_scheme {
            out.insert(format!("{carrier}|id_scheme"), scheme.clone());
        }
    }
    out
}

/// Whether a FLAT key falls under an ignore path: the `ctx/*` default-setter
/// spellings map to their master06 targets (`ctx/time` →
/// `context/start_time`, `ctx/setting` → `context/setting`, `ctx/composer_*`
/// → `composer`); any other key matches when its post-root path starts with
/// the ignore path (`uid` also matches the flat `_uid` spelling).
fn flat_key_ignored(key: &str, ignored_paths: &[String]) -> bool {
    let effective: &str = match key {
        "ctx/time" => "context/start_time",
        "ctx/end_time" => "context/end_time",
        "ctx/setting" => "context/setting",
        k if k.starts_with("ctx/composer") => "composer",
        k => k.split_once('/').map_or(k, |(_, rest)| rest),
    };
    let effective = effective.replace("/_uid", "/uid");
    let effective = effective.strip_prefix('_').unwrap_or(&effective);
    ignored_paths.iter().any(|p| {
        effective == p.as_str()
            || effective.starts_with(&format!("{p}/"))
            || effective.starts_with(&format!("{p}|"))
    })
}

/// Canonicalize a FLAT key by eliding every `:0` first-element index
/// (`a:0/b`, `a:0|x`, trailing `a:0`) — the index is optional on the wire
/// (`simplified_formats` master04 §Field Identifiers), so both spellings
/// name the same datum.
fn dezero(key: &str) -> String {
    let inner = key.replace(":0/", "/").replace(":0|", "|");
    inner
        .strip_suffix(":0")
        .map_or(inner.clone(), ToOwned::to_owned)
}

/// Flatten a STRUCTURED body into its FLAT key form per the
/// `simplified_formats` master04 STRUCTURED->FLAT algorithm: object keys
/// join with `/`, array elements append `:{i}` to their segment,
/// `|`-prefixed attribute keys append without a separator, and the
/// empty-string key is the element's main value.
fn flatten_structured(value: &Value, prefix: &str, out: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let next = if key.is_empty() {
                    prefix.to_owned()
                } else if let Some(attr) = key.strip_prefix('|') {
                    format!("{prefix}|{attr}")
                } else if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}/{key}")
                };
                flatten_structured(child, &next, out);
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                flatten_structured(item, &format!("{prefix}:{i}"), out);
            }
        }
        leaf => {
            out.insert(prefix.to_owned(), leaf.clone());
        }
    }
}

/// STRUCTURED is recognized by its attribute-key shape: `|`-prefixed or
/// empty-string keys somewhere in the tree (master04 §Structured format) —
/// canonical JSON never carries either, so a canonical body is never
/// misread as simplified.
fn has_simplified_leaf_keys(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(k, v)| k.is_empty() || k.starts_with('|') || has_simplified_leaf_keys(v)),
        Value::Array(items) => items.iter().any(has_simplified_leaf_keys),
        _ => false,
    }
}

/// A simplified body in either wire form, as a FLAT key map: a FLAT body
/// verbatim, a STRUCTURED body via the master04 flattening. `None` for
/// canonical/other bodies (a canonical COMPOSITION carries `_type`).
fn simplified_as_flat(value: &Value) -> Option<BTreeMap<String, Value>> {
    let Value::Object(map) = value else {
        return None;
    };
    if is_flat_map(value) {
        return Some(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    }
    if !map.is_empty()
        && map.keys().all(|k| !k.contains('/'))
        && !map.contains_key("_type")
        && (map.contains_key("ctx") || has_simplified_leaf_keys(value))
    {
        let mut out = BTreeMap::new();
        flatten_structured(value, "", &mut out);
        return Some(out);
    }
    None
}

/// FLAT round-trip equivalence: fold the committed side's ctx keys onto the
/// read-back's RM-path forms (master06), drop ignore-set keys from both sides,
/// then require every committed entry to appear in the read-back with an equal
/// value (keys compared in their `:0`-elided canonical form, master04 §Field
/// Identifiers). Read-back surplus is tolerated, because the export is the full
/// RM projection of the committed data (`simplified_formats` master04), so the
/// round-trip guarantee is that no committed datum is lost or altered. (No
/// openEHR spec defines a round-trip comparator — our own design over the
/// master04 and master06 semantics.)
fn flat_equivalent(
    actual: &BTreeMap<String, Value>,
    committed: &BTreeMap<String, Value>,
    ignored_paths: &[String],
) -> bool {
    let root = actual
        .keys()
        .find(|k| !k.starts_with("ctx/"))
        .and_then(|k| k.split(['/', ':']).next())
        .unwrap_or_default()
        .to_owned();
    let normalized: BTreeMap<String, &Value> = actual.iter().map(|(k, v)| (dezero(k), v)).collect();
    let folded = fold_flat_ctx(committed, &root);
    folded
        .iter()
        .filter(|(k, _)| !flat_key_ignored(k, ignored_paths))
        .all(|(k, want)| {
            normalized
                .get(&dezero(k))
                .is_some_and(|got| resultset::cells_equal(got, want))
        })
}

/// The `equivalent` comparison.
///
/// Structural equality after stripping the resolved ignore paths from both
/// sides, with numeric leaves compared by value through the result-set cell
/// rule and canonical `_type` self-tag presence normalized (`rm_cells_equal`).
/// FLAT bodies take the master06-aware round-trip rule (`flat_equivalent`).
#[must_use]
pub fn equivalent(actual: &Value, expected: &Value, ignored_paths: &[String]) -> bool {
    if let (Some(a), Some(e)) = (simplified_as_flat(actual), simplified_as_flat(expected)) {
        return flat_equivalent(&a, &e, ignored_paths);
    }
    let a = strip_ignored(actual, ignored_paths);
    let b = strip_ignored(expected, ignored_paths);
    rm_cells_equal(&a, &b)
}

/// Canonical-RM structural equality for `equivalent`: the result-set cell rule
/// everywhere, except that a `_type` self-tag present on only one side of an
/// object is not a content difference. ITS-REST overview Resources.md §JSON
/// Format makes `_type` presence conditional ("should be used to specify the RM
/// type whenever polymorphism is involved, or when the underlying definition in
/// RM type is abstract") while the MUST governs its value, so a codec that
/// self-tags every object and a sparsely tagged committed twin describe the same
/// RM content. Present on both sides, the tags must be equal, which keeps a
/// genuine polymorphic-type substitution detectable.
fn rm_cells_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<&str> = x
                .keys()
                .chain(y.keys())
                .map(String::as_str)
                .filter(|k| *k != "_type")
                .collect();
            let type_tags_agree = match (x.get("_type"), y.get("_type")) {
                (Some(ta), Some(tb)) => ta == tb,
                _ => true,
            };
            type_tags_agree
                && keys.iter().all(|k| match (x.get(*k), y.get(*k)) {
                    (Some(va), Some(vb)) => rm_cells_equal(va, vb),
                    _ => false,
                })
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(va, vb)| rm_cells_equal(va, vb))
        }
        _ => resultset::cells_equal(a, b),
    }
}

/// The equivalence-failure diagnostic: the first differing paths.
///
/// Each phrase names the path, what was served and what was wanted, so a red
/// row carries triage-usable evidence — two 80-char head previews forced the
/// 2026-07-28 composition-XML triage to reconstruct the diff offline from the
/// codec instead of reading it from `results.json`.
#[must_use]
pub fn equivalence_mismatch(body: &Value, expected: &Value) -> AssertionFailure {
    let mut diffs: Vec<String> = Vec::new();
    diff_paths(body, expected, "$", &mut diffs);
    let shown = 6;
    let suffix = if diffs.len() > shown {
        format!(" … and {} more differing path(s)", diffs.len() - shown)
    } else {
        String::new()
    };
    AssertionFailure(format!(
        "equivalent: retrieved content differs from committed (modulo the normative ignore-set); {}{}",
        diffs
            .iter()
            .take(shown)
            .cloned()
            .collect::<Vec<_>>()
            .join("; "),
        suffix
    ))
}

/// A value rendered for a diagnostic, clipped to its first 60 characters.
fn brief(value: &Value) -> String {
    let rendered = value.to_string();
    if rendered.chars().count() > 60 {
        let head: String = rendered.chars().take(60).collect();
        format!("{head}…")
    } else {
        rendered
    }
}

/// Every path at which two RM values differ, deepest-first, as
/// `path: got … want …` phrases.
fn diff_paths(got: &Value, want: &Value, path: &str, out: &mut Vec<String>) {
    match (got, want) {
        (Value::Object(x), Value::Object(y)) => diff_object_paths(x, y, path, out),
        (Value::Array(x), Value::Array(y)) => diff_array_paths(x, y, path, out),
        _ => {
            if !resultset::cells_equal(got, want) {
                out.push(format!("{path}: got {} want {}", brief(got), brief(want)));
            }
        }
    }
}

/// The differing paths of two RM objects: the wanted members first, then the
/// members the retrieved document carries in surplus.
///
/// `_type` presence on one side only is tolerated by the comparator
/// (`rm_cells_equal`), so the diagnostic stays aligned with it and a red row
/// never lists only tolerated diffs.
fn diff_object_paths(
    got: &serde_json::Map<String, Value>,
    want: &serde_json::Map<String, Value>,
    path: &str,
    out: &mut Vec<String>,
) {
    for (k, vw) in want {
        match got.get(k) {
            Some(vg) => diff_paths(vg, vw, &format!("{path}/{k}"), out),
            None if k == "_type" => {}
            None => out.push(format!(
                "{path}/{k}: absent in retrieved (want {})",
                brief(vw)
            )),
        }
    }
    for (k, vg) in got {
        if !want.contains_key(k) && k != "_type" {
            out.push(format!("{path}/{k}: surplus in retrieved ({})", brief(vg)));
        }
    }
}

/// The differing paths of two RM arrays: the length when it diverges, then
/// each index the two sides both carry.
fn diff_array_paths(got: &[Value], want: &[Value], path: &str, out: &mut Vec<String>) {
    if got.len() != want.len() {
        out.push(format!(
            "{path}: array length {} vs {}",
            got.len(),
            want.len()
        ));
    }
    for (i, (vg, vw)) in got.iter().zip(want).enumerate() {
        diff_paths(vg, vw, &format!("{path}[{i}]"), out);
    }
}

/// Evaluate the `signature` facts one assertion declares, against the
/// `ORIGINAL_VERSION` envelope the caller holds.
///
/// `present`/`equals`/`distinct_from` are mode-agnostic and read the served
/// envelope alone; `verifiable` reconstructs the agreed canonical form and
/// verifies it under `signing`, the posture of the addressed instance (RM
/// common `master06-change_control_package.adoc` §Digital Signature). Both
/// comparands arrive RESOLVED, because where a comparand comes from is the
/// caller's business and what it means is not.
///
/// # Errors
/// [`AssertionOutcome::Mismatch`] for every fact judged against a value the
/// SUT served, and [`AssertionOutcome::Unjudgeable`] for a `verifiable` fact
/// no declared posture covers.
pub fn eval_signature(
    body: &Value,
    present: Option<bool>,
    verifiable: Option<bool>,
    equals: Option<&str>,
    distinct_from: Option<&str>,
    signing: Option<&crate::exec::signature::SigningMode>,
) -> Result<(), AssertionOutcome> {
    let signature = body.get("signature").and_then(Value::as_str);
    if present == Some(true) && signature.is_none_or(str::is_empty) {
        return Err(AssertionFailure(
            "signature: expected present, the ORIGINAL_VERSION envelope carries no signature"
                .into(),
        )
        .into());
    }
    if let Some(want) = distinct_from {
        // The canonical form the signature covers includes `uid`, so two
        // distinct versions carry distinct signatures (RM common master06
        // §Digital Signature; version.adoc `canonical_form`: all attributes
        // except signature). Both sides must be non-empty.
        let Some(sig) = signature.filter(|s| !s.is_empty()) else {
            return Err(AssertionFailure(
                "signature: distinct_from requested but the envelope carries no signature".into(),
            )
            .into());
        };
        if want.is_empty() {
            return Err(AssertionFailure(
                "signature: distinct_from comparand is empty (the earlier signature capture failed)"
                    .into(),
            )
            .into());
        }
        if sig == want {
            return Err(AssertionFailure(
                "signature: identical to the compared version's signature — the signature must be a function of the version's canonical content (RM common master06 §Digital Signature)"
                    .into(),
            )
            .into());
        }
    }
    if let Some(want) = equals
        && signature != Some(want)
    {
        return Err(AssertionFailure(format!(
            "signature: stored {signature:?} is not the client-supplied {want:?} (must be stored verbatim)"
        ))
        .into());
    }
    if verifiable == Some(true) {
        let Some(sig) = signature else {
            return Err(AssertionFailure(
                "signature: verifiable requested but the envelope carries no signature".into(),
            )
            .into());
        };
        // The mode is a DEPLOYMENT fact the party declares in its ixit (RM
        // common master06 §Digital Signature: a deployment runs digest or
        // openPGP, one at a time). Undeclared, the run holds no agreed
        // canonical form and no key material, so it never asked.
        let Some(mode) = signing else {
            return Err(AssertionOutcome::Unjudgeable(
                "signature: verifiable requested but the ixit declares no `signing` posture for \
                 the addressed instance — the mode is a deployment fact (RM common \
                 master06-change_control_package.adoc §Digital Signature), so this run cannot \
                 judge the fact"
                    .into(),
            ));
        };
        match crate::exec::signature::verify(body, sig, mode) {
            Ok(true) => {}
            Ok(false) => {
                return Err(AssertionFailure(
                    "signature: does not verify over the agreed canonical form (RFC 8785 JCS of the version minus signature)"
                        .into(),
                )
                .into());
            }
            Err(e) => return Err(AssertionFailure(format!("signature verify: {e}")).into()),
        }
    }
    Ok(())
}

/// One `result_set` expectation whose rows are already resolved to values.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedResultSet<'a> {
    /// How the expected rows are compared against the served ones.
    pub match_mode: ResultSetMatch,
    /// The expected rows; `None` where the assertion carries none.
    pub rows: Option<&'a [Value]>,
    /// The exact row count, for `match: count`.
    pub count: Option<u64>,
    /// The expected column aliases, when the row asserts them.
    pub columns: Option<&'a [ColumnSpec]>,
    /// The declared cell comparison; the default is the exact lexeme.
    pub cells: CellComparison,
}

/// Compare a served `RESULT_SET` against a resolved expectation.
///
/// Returns the non-gating divergences the declared cell mode tolerated, one
/// line each — empty under the default exact comparison.
///
/// # Errors
/// [`AssertionFailure`] describing the first divergence that gates, or the
/// absence of any comparable expectation.
pub fn eval_result_set_against(
    body: &Value,
    expectation: ResolvedResultSet<'_>,
) -> Result<Vec<String>, AssertionFailure> {
    let ResolvedResultSet {
        match_mode,
        rows,
        count,
        columns,
        cells,
    } = expectation;
    let mut cmp = resultset::CellComparator::new(cells);
    if let Some(cols) = columns {
        let names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
        resultset::compare_columns(body, &names).map_err(|e| AssertionFailure(e.0))?;
    }
    let outcome = match (match_mode, rows, count) {
        (ResultSetMatch::Count, _, Some(n)) => resultset::compare_count(body, n),
        (ResultSetMatch::Ordered, Some(rows), _) => {
            resultset::compare_ordered(body, rows, &mut cmp)
        }
        (ResultSetMatch::Set, Some(rows), _) => resultset::compare_bag(body, rows, &mut cmp),
        (ResultSetMatch::Contains, Some(rows), _) => {
            resultset::compare_contains(body, rows, &mut cmp)
        }
        _ => {
            return Err(AssertionFailure(
                "result_set: no comparable expectation resolved".into(),
            ));
        }
    };
    outcome.map_err(|e| AssertionFailure(e.0))?;
    Ok(recorded_divergences(&cmp))
}

/// The comparator's tolerated divergences, one reportable line each.
fn recorded_divergences(cmp: &resultset::CellComparator) -> Vec<String> {
    cmp.divergences()
        .iter()
        .map(|d| {
            format!(
                "result_set: {d} — ITS-REST docs/overview/Resources.md §Datetime format puts the \
                 served spelling at SHOULD strength, so the row still passes"
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The equivalence diagnostic names the differing PATHS — a missing key,
    /// a surplus key, an array-length difference and an unequal leaf — and
    /// caps the list, so a red row carries triage-usable evidence instead of
    /// two truncated previews.
    #[test]
    fn the_equivalence_diagnostic_names_the_paths_that_differ() {
        let got = serde_json::json!({
            "_type": "COMPOSITION",
            "name": { "value": "served" },
            "surplus": 1,
            "items": [{ "a": 1 }]
        });
        let want = serde_json::json!({
            "name": { "value": "committed" },
            "missing": "gone",
            "items": [{ "a": 1 }, { "a": 2 }]
        });
        let message = equivalence_mismatch(&got, &want).0;
        assert!(
            message.contains("$/name/value: got \"served\" want \"committed\""),
            "{message}"
        );
        assert!(
            message.contains("$/missing: absent in retrieved"),
            "{message}"
        );
        assert!(
            message.contains("$/surplus: surplus in retrieved (1)"),
            "{message}"
        );
        assert!(
            message.contains("$/items: array length 1 vs 2"),
            "{message}"
        );
        // `_type` on one side only is what the comparator tolerates, so the
        // diagnostic never lists it.
        assert!(!message.contains("_type"), "{message}");

        // Past the shown budget the count is stated rather than the list run
        // on: a leaf preview is truncated to keep one line readable.
        let wide_got = serde_json::json!({ "k": "x" });
        let wide_want: Value = Value::Object(
            (0..9)
                .map(|k| (format!("k{k}"), Value::String("y".repeat(80))))
                .collect(),
        );
        let wide = equivalence_mismatch(&wide_got, &wide_want).0;
        assert!(wide.contains("and 4 more differing path(s)"), "{wide}");
        assert!(
            wide.contains('…'),
            "a long leaf is previewed, not dumped: {wide}"
        );
    }

    #[test]
    fn path_resolution_addresses_objects_and_lists() {
        let body = json!({
            "context": { "setting": { "value": "other care" } },
            "content": [ { "data": { "events": [ { "time": "t0" } ] } } ]
        });
        assert_eq!(
            resolve_path(&body, "context/setting/value").unwrap(),
            &json!("other care")
        );
        assert_eq!(
            resolve_path(&body, "content[0]/data/events[0]/time").unwrap(),
            &json!("t0")
        );
        assert!(resolve_path(&body, "content[1]").is_none());
    }

    #[test]
    fn recursive_ignore_segment_strips_every_depth() {
        // A FOLDER tree (RM common `folder.adoc`: `folders: List<FOLDER>`),
        // with a uid on the root and on each nested node.
        let tree = json!({
            "_type": "FOLDER",
            "uid": { "_type": "OBJECT_VERSION_ID", "value": "r::s::1" },
            "folders": [
                {
                    "_type": "FOLDER",
                    "uid": { "_type": "HIER_OBJECT_ID", "value": "a" },
                    "name": { "value": "emergency" },
                    "folders": [
                        { "_type": "FOLDER", "uid": { "value": "b" }, "name": { "value": "episode" } }
                    ]
                }
            ]
        });
        // A depth-anchored path reaches only the root.
        let shallow = strip_ignored(&tree, &["uid".to_owned()]);
        assert!(shallow.get("uid").is_none());
        assert!(shallow["folders"][0].get("uid").is_some());
        // `**/uid` reaches the root and every nested node.
        let deep = strip_ignored(&tree, &["**/uid".to_owned()]);
        assert!(deep.get("uid").is_none());
        assert!(deep["folders"][0].get("uid").is_none());
        assert!(deep["folders"][0]["folders"][0].get("uid").is_none());
        // Nothing else is touched.
        assert_eq!(deep["folders"][0]["name"]["value"], json!("emergency"));
        assert_eq!(
            deep["folders"][0]["folders"][0]["name"]["value"],
            json!("episode")
        );
    }

    #[test]
    fn equivalence_strips_normative_ignore_sets_only() {
        let committed = json!({ "name": { "value": "v1" }, "content": [{"x": 1}] });
        let served = json!({
            "uid": { "value": "generated::sut::1" },
            "name": { "value": "v1" },
            "content": [{"x": 1}]
        });
        assert!(equivalent(&served, &committed, &["uid".to_owned()]));
        assert!(!equivalent(&served, &committed, &[])); // nothing stripped -> uid differs
    }

    /// A FLAT body is "key-value pairs at a single level in JSON where …
    /// keys are full WT paths" and "context fields MUST use `ctx/` prefix"
    /// (ITS-REST `docs/simplified_formats/master04-basic_concepts.adoc`
    /// §Format variants → Flat format), which is exactly the shape this
    /// predicate reads: path-formed keys, no nested object under any of them.
    #[test]
    fn a_flat_body_is_recognized_by_its_single_level_of_path_keys() {
        let flat = json!({
            "ctx/language": "en",
            "vital_signs/body_temperature:0/any_event:0/temperature|magnitude": 37.5,
            "vital_signs/body_temperature:0/any_event:0/temperature|unit": "°C"
        });
        assert!(is_flat_map(&flat));
        // A canonical body nests, and carries no path-formed key.
        assert!(!is_flat_map(&json!({
            "_type": "COMPOSITION",
            "name": { "value": "Vital signs" }
        })));
        // One path-formed key is not enough if a value still nests.
        assert!(!is_flat_map(&json!({
            "vital_signs/x|magnitude": 1,
            "nested": { "a": 1 }
        })));
        assert!(!is_flat_map(&json!({})), "an empty object is not a body");
        assert!(!is_flat_map(&json!([])), "an array is not a FLAT map");
    }

    /// The STRUCTURED→FLAT algorithm: "build path by concatenating property
    /// names with forward slash", "for properties with a pipe prefix, append
    /// to a parent path with pipe", "unwrap arrays" and "preserve instance
    /// indices" (ITS-REST
    /// `docs/simplified_formats/master04-basic_concepts.adoc` §Conversion
    /// Between Formats → Structured to Flat). The empty-string key is the
    /// element's own main value, so it flattens onto the parent path itself.
    #[test]
    fn a_structured_body_flattens_onto_its_flat_key_form() {
        let structured = json!({
            "vital_signs": [ {
                "body_temperature": [ {
                    "any_event": [ {
                        "temperature": [ { "|magnitude": 37.5, "|unit": "°C" } ],
                        "time": [ { "": "2026-07-21T10:00:00Z" } ]
                    } ]
                } ]
            } ]
        });
        let mut flat = BTreeMap::new();
        flatten_structured(&structured, "", &mut flat);
        let keys: Vec<&str> = flat.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec![
                "vital_signs:0/body_temperature:0/any_event:0/temperature:0|magnitude",
                "vital_signs:0/body_temperature:0/any_event:0/temperature:0|unit",
                "vital_signs:0/body_temperature:0/any_event:0/time:0",
            ]
        );
        assert_eq!(
            flat.get("vital_signs:0/body_temperature:0/any_event:0/temperature:0|magnitude"),
            Some(&json!(37.5))
        );
    }

    /// The attribute-key shape is what tells a simplified body from a
    /// canonical one: `|`-prefixed and empty-string keys are STRUCTURED
    /// spellings (master04 §Format variants), and canonical JSON carries
    /// neither — it carries `_type`.
    #[test]
    fn a_canonical_body_is_never_read_as_a_simplified_one() {
        let canonical = json!({
            "_type": "COMPOSITION",
            "name": { "_type": "DV_TEXT", "value": "Vital signs" }
        });
        assert!(!has_simplified_leaf_keys(&canonical));
        assert!(
            simplified_as_flat(&canonical).is_none(),
            "a canonical body has no FLAT reading"
        );
        // A STRUCTURED body does, and it comes back flattened.
        let structured = json!({ "vital_signs": [ { "temperature": [ { "|magnitude": 1 } ] } ] });
        assert!(has_simplified_leaf_keys(&structured));
        let flat = simplified_as_flat(&structured).expect("a STRUCTURED body reads as FLAT");
        assert_eq!(
            flat.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["vital_signs:0/temperature:0|magnitude"]
        );
        // A FLAT body reads as itself, verbatim.
        let already_flat = json!({ "ctx/language": "en", "vitals/temp|magnitude": 37.5 });
        assert_eq!(
            simplified_as_flat(&already_flat).map(|m| m.len()),
            Some(2),
            "a FLAT body is its own key map"
        );
        assert!(simplified_as_flat(&json!("text")).is_none());
    }

    /// The committed side's `ctx/*` input keys name the same data the
    /// read-back expresses at RM paths: `ctx/participation_<field>:<i>` and
    /// `ctx/health_care_facility|<attr>` (ITS-REST
    /// `docs/simplified_formats/master06-context_information.adoc`
    /// §Participation, §`health_care_facility`), with `ctx/id_namespace` and
    /// `ctx/id_scheme` as the declared defaults for external references
    /// (§ID Namespace and Scheme) rather than data of their own.
    #[test]
    fn the_ctx_input_keys_fold_onto_the_paths_a_read_back_uses() {
        let committed: BTreeMap<String, Value> = [
            ("ctx/participation_name:1".to_owned(), json!("Lara Markham")),
            ("ctx/participation_id:1".to_owned(), json!("198")),
            ("ctx/participation_function".to_owned(), json!("performer")),
            ("ctx/health_care_facility|id".to_owned(), json!("9091")),
            ("ctx/id_namespace".to_owned(), json!("HOSPITAL-NS")),
            ("ctx/id_scheme".to_owned(), json!("HOSPITAL-NS")),
            ("ctx/language".to_owned(), json!("en")),
            ("vitals/temperature|magnitude".to_owned(), json!(37.5)),
        ]
        .into_iter()
        .collect();

        let folded = fold_flat_ctx(&committed, "vitals");
        assert_eq!(
            folded.get("vitals/context/_participation:1|name"),
            Some(&json!("Lara Markham"))
        );
        assert_eq!(
            folded.get("vitals/context/_participation:0|function"),
            Some(&json!("performer")),
            "an index-free participation key is the first participation"
        );
        assert_eq!(
            folded.get("vitals/context/_health_care_facility|id"),
            Some(&json!("9091"))
        );
        // The declared defaults expand onto every folded party carrying an id,
        // and never survive as keys of their own.
        assert_eq!(
            folded.get("vitals/context/_participation:1|id_namespace"),
            Some(&json!("HOSPITAL-NS"))
        );
        assert_eq!(
            folded.get("vitals/context/_health_care_facility|id_scheme"),
            Some(&json!("HOSPITAL-NS"))
        );
        assert!(!folded.contains_key("ctx/id_namespace"));
        assert!(!folded.contains_key("ctx/id_scheme"));
        // Everything else passes through untouched, ctx defaults included:
        // the ignore pass is what excuses them.
        assert_eq!(folded.get("ctx/language"), Some(&json!("en")));
        assert_eq!(
            folded.get("vitals/temperature|magnitude"),
            Some(&json!(37.5))
        );
    }

    /// The ignore pass reads a FLAT key at the RM path it names: the
    /// `ctx/*` default-setter spellings map onto their master06 targets
    /// (`ctx/time` → `context/start_time`, `ctx/setting` → `context/setting`,
    /// `ctx/composer_*` → `composer`), and any other key matches when its
    /// post-root path falls under the ignored path.
    #[test]
    fn a_flat_key_is_ignored_at_the_rm_path_it_names() {
        let ignored = [
            "context/start_time".to_owned(),
            "context/setting".to_owned(),
            "composer".to_owned(),
            "uid".to_owned(),
        ];
        assert!(flat_key_ignored("ctx/time", &ignored));
        assert!(flat_key_ignored("ctx/setting", &ignored));
        assert!(flat_key_ignored("ctx/composer_name", &ignored));
        assert!(flat_key_ignored("ctx/composer_id", &ignored));
        // The post-root path is what is compared, so the root segment of a
        // read-back key never has to be enumerated.
        assert!(flat_key_ignored("vitals/context/start_time", &ignored));
        assert!(flat_key_ignored("vitals/uid|value", &ignored));
        // The flat `_uid` spelling of the same RM attribute is the same datum.
        assert!(flat_key_ignored("vitals/_uid", &ignored));
        // A datum nobody ignored stays compared.
        assert!(!flat_key_ignored("vitals/temperature|magnitude", &ignored));
        assert!(!flat_key_ignored("ctx/end_time", &ignored));
        assert!(flat_key_ignored(
            "ctx/end_time",
            &["context/end_time".to_owned()]
        ));
    }

    /// Keys are compared in one canonical spelling, so a first-element index
    /// written out and one left off name the same datum.
    #[test]
    fn the_first_element_index_is_elided_before_keys_are_compared() {
        assert_eq!(
            dezero("vitals:0/temperature:0|magnitude"),
            "vitals/temperature|magnitude"
        );
        assert_eq!(dezero("vitals/events:0"), "vitals/events");
        assert_eq!(
            dezero("vitals:1/temperature:2|magnitude"),
            "vitals:1/temperature:2|magnitude",
            "only the FIRST element's index is elidable"
        );
        assert_eq!(
            dezero("vitals/temperature|magnitude"),
            "vitals/temperature|magnitude"
        );
    }

    /// The FLAT round-trip rule: every committed datum must come back with an
    /// equal value, read-back surplus is tolerated (the export is the full RM
    /// projection of the committed data, master04 §Format variants), and an
    /// ignored path is compared on neither side.
    #[test]
    fn a_flat_round_trip_loses_no_committed_datum_and_tolerates_surplus() {
        let committed = json!({
            "ctx/language": "en",
            "ctx/time": "2026-07-21T10:00:00Z",
            "vitals/temperature|magnitude": 37.5,
            "vitals/temperature|unit": "°C"
        });
        let read_back = json!({
            "vitals/temperature:0|magnitude": 37.5,
            "vitals/temperature:0|unit": "°C",
            "vitals/context/start_time": "2026-07-21T10:00:04Z",
            "vitals/category|code_string": "433",
            "vitals/_uid": "8849182c-82ad-4088-a07f-48ead4180515::sut::1",
            "ctx/language": "en"
        });
        let ignored = ["context/start_time".to_owned(), "uid".to_owned()];
        assert!(
            equivalent(&read_back, &committed, &ignored),
            "the read-back carries every committed datum"
        );

        // A changed datum is a failure, surplus or not.
        let altered = json!({
            "vitals/temperature:0|magnitude": 38.5,
            "vitals/temperature:0|unit": "°C",
            "ctx/language": "en"
        });
        assert!(!equivalent(&altered, &committed, &ignored));

        // A dropped datum is a failure too.
        let lossy = json!({
            "vitals/temperature:0|magnitude": 37.5,
            "ctx/language": "en"
        });
        assert!(!equivalent(&lossy, &committed, &ignored));

        // Without the ignore set the server-set start_time is compared, and
        // the committed `ctx/time` no longer matches it.
        assert!(!equivalent(&read_back, &committed, &[]));
    }

    /// ITS-REST overview `Resources.md` §JSON Format makes the `_type`
    /// self-tag CONDITIONAL while the requirement governs its VALUE, so a
    /// fully self-tagging codec and a sparsely tagged committed twin describe
    /// the same RM content — but two DIFFERENT tags are a real polymorphic
    /// substitution and stay detectable.
    #[test]
    fn a_type_self_tag_present_on_one_side_only_is_not_a_content_difference() {
        let served = json!({
            "_type": "COMPOSITION",
            "name": { "_type": "DV_TEXT", "value": "Vital signs" }
        });
        let committed = json!({ "name": { "value": "Vital signs" } });
        assert!(equivalent(&served, &committed, &[]));

        let substituted = json!({
            "_type": "COMPOSITION",
            "name": { "_type": "DV_CODED_TEXT", "value": "Vital signs" }
        });
        let tagged_committed = json!({
            "name": { "_type": "DV_TEXT", "value": "Vital signs" }
        });
        assert!(
            !equivalent(&substituted, &tagged_committed, &[]),
            "two different concrete types are not the same content"
        );
        // Array length is content, never padding.
        assert!(!equivalent(
            &json!({ "content": [1, 2] }),
            &json!({ "content": [1] }),
            &[]
        ));
    }

    /// A malformed index step addresses nothing rather than panicking or
    /// silently dropping the step: an assertion over a mis-authored path
    /// reports "resolves to nothing" instead of passing over the whole body.
    #[test]
    fn a_malformed_index_step_resolves_to_nothing() {
        let body = json!({ "versions": [{ "uid": "a" }, { "uid": "b" }] });
        assert_eq!(
            resolve_path(&body, "versions[1]/uid"),
            Some(&json!("b")),
            "the well-formed index addresses its element"
        );
        // A non-numeric index, an unterminated one, and an index past the end.
        for path in ["versions[x]/uid", "versions[0/uid", "versions[9]/uid"] {
            assert_eq!(resolve_path(&body, path), None, "{path}");
        }
        // A bare index step with no attribute indexes the CURRENT value.
        assert_eq!(resolve_path(&body, "versions/[0]/uid"), Some(&json!("a")));
        // A leading and a doubled separator are empty steps, which are skipped.
        assert_eq!(resolve_path(&body, "//versions[0]//uid"), Some(&json!("a")));
    }

    /// An ignore path descends through objects and lists, and an empty path
    /// strips nothing — an ignore set can never quietly blank a whole body.
    #[test]
    fn an_ignore_path_descends_and_an_empty_one_strips_nothing() {
        let body = json!({
            "context": { "start_time": "t", "setting": "s" },
            "versions": [
                { "uid": "a", "commit_audit": { "time_committed": "t1" } },
                { "uid": "b", "commit_audit": { "time_committed": "t2" } }
            ]
        });

        let nested = strip_ignored(&body, &["context/start_time".to_owned()]);
        assert_eq!(nested["context"], json!({ "setting": "s" }));
        assert_eq!(nested["versions"], body["versions"], "untouched elsewhere");

        // Through a list: every element loses the named leaf.
        let through_list =
            strip_ignored(&body, &["versions/commit_audit/time_committed".to_owned()]);
        for version in through_list["versions"].as_array().unwrap() {
            assert_eq!(version["commit_audit"], json!({}));
            assert!(version["uid"].is_string(), "siblings survive");
        }

        // An empty path names nothing, so the body comes back whole.
        assert_eq!(strip_ignored(&body, &[String::new()]), body);
        assert_eq!(strip_ignored(&body, &["/".to_owned()]), body);
        // A path through a scalar leaf strips nothing rather than failing.
        assert_eq!(
            strip_ignored(&body, &["context/setting/deeper".to_owned()]),
            body
        );
    }
}
