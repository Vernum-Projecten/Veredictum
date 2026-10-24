// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! `openehr-query`, the AQL parser `validate` checks catalogue queries with,
//! judged against every worked query of the QUERY release this repository
//! pins (`specs/openehr/QUERY/docs/AQL_examples/`, Release-1.1.0), with the
//! released grammar `specs/openehr/QUERY/docs/AQL/grammar/AqlParser.g4` as
//! the judge.
//!
//! **Extraction.** Every `AsciiDoc` listing block is collected: an opening line
//! of four or more hyphens up to the identical closing line (`AsciiDoc`'s
//! delimited listing block). The examples carry no `[source]` attribute, and
//! two delimiter lengths occur (`----` and `--------`), so a fixed-length
//! match would silently skip blocks; [`every_listing_block_is_collected`]
//! pins the count per file. A block that is a bare `WHERE` fragment (the
//! TERMINOLOGY examples) is completed by ONE declared shell,
//! [`WHERE_SHELL`]: `SELECT e/ehr_id/value FROM EHR e WHERE <fragment>`. No
//! other example text is rewritten.
//!
//! **Expectations.** Each block is classified against the released grammar
//! itself, `AqlLexer.g4` and `AqlParser.g4`: one the grammar derives must
//! parse and survive `printer::to_aql` back to an equal AST; one the grammar
//! refuses must be refused by the same pass, at the position where the cited
//! rule has no continuation. One test per block, so a red row names one
//! example.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) fail loudly on a broken fixture or a wrong verdict, Book ch11"
)]

use std::path::{Path, PathBuf};

use openehr_query::parser::{ParseError, parse_str};
use openehr_query::printer::to_aql;

/// The worked examples, relative to the repository root.
const EXAMPLES: &str = "specs/openehr/QUERY/docs/AQL_examples";

/// The one shell a bare `WHERE` fragment is completed with: the fragment's
/// own text follows the `WHERE`. No openEHR spec governs this — our own
/// design, so the fragments' operators reach the parser at all.
const WHERE_SHELL: &str = "SELECT e/ehr_id/value FROM EHR e WHERE ";

/// The number of listing blocks per example file; every other file holds none.
const BLOCKS_PER_FILE: &[(&str, usize)] = &[
    ("master03-syntax-function.adoc", 8),
    ("master03-syntax-operator.adoc", 9),
    ("master04-single_subject.adoc", 1),
];

/// Grammar-accepted examples `openehr-query` refuses or mis-parses: each one
/// is a defect in the crate, fixed there, never worked around here. Each
/// entry is `(file, block ordinal, the parse error as the crate reports it)`.
const KNOWN_CRATE_DEFECTS: &[(&str, usize, &str)] = &[];

/// What the released grammar says about one example.
#[derive(Debug, Clone, Copy)]
enum Grammar {
    /// `selectQuery` derives it.
    Derives,
    /// `AqlLexer.g4` has no token rule matching the character that starts
    /// the first occurrence of `at`. The lexer pass refuses before the parser
    /// runs, so this is the first refusal even where `AqlParser.g4` would
    /// also refuse earlier text.
    LexerRefuses {
        /// Why no `AqlLexer.g4` rule matches there.
        rule: &'static str,
        /// The source text the refusal begins at.
        at: &'static str,
    },
    /// The tokens lex, and `rule` of `AqlParser.g4` has no alternative that
    /// continues at the first occurrence of `at`.
    ParserRefuses {
        /// The `AqlParser.g4` rule whose alternatives all fail there.
        rule: &'static str,
        /// The source text the refusal begins at.
        at: &'static str,
    },
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every delimited listing block of an `AsciiDoc` document, in order: an
/// opening line of four or more hyphens to the identical closing line.
fn listing_blocks(doc: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut open: Option<&str> = None;
    let mut body = String::new();
    for line in doc.lines() {
        let delimiter = line.trim_end();
        let is_delimiter = delimiter.len() >= 4 && delimiter.chars().all(|c| c == '-');
        match open {
            None if is_delimiter => {
                open = Some(delimiter);
                body.clear();
            }
            Some(opened) if delimiter == opened => {
                blocks.push(std::mem::take(&mut body));
                open = None;
            }
            Some(_) => {
                body.push_str(line);
                body.push('\n');
            }
            None => {}
        }
    }
    assert!(open.is_none(), "an unterminated listing block");
    blocks
}

/// The query of block `ordinal` of `file`, a `WHERE` fragment completed by
/// [`WHERE_SHELL`].
fn example(file: &str, ordinal: usize) -> String {
    let path = repo_root().join(EXAMPLES).join(file);
    let doc = std::fs::read_to_string(&path).unwrap();
    let block = listing_blocks(&doc)
        .into_iter()
        .nth(ordinal)
        .unwrap_or_else(|| panic!("{file} has no block {ordinal}"));
    match block.trim().strip_prefix("WHERE") {
        Some(fragment) => format!("{WHERE_SHELL}{}", fragment.trim()),
        None => block,
    }
}

/// The byte offset of the first occurrence of `at` in `query`.
fn offset_of(file: &str, ordinal: usize, query: &str, at: &str) -> usize {
    query
        .find(at)
        .unwrap_or_else(|| panic!("{file} block {ordinal}: `{at}` is not in the query"))
}

fn judge(file: &str, ordinal: usize, grammar: Grammar) {
    let query = example(file, ordinal);
    let defect = KNOWN_CRATE_DEFECTS
        .iter()
        .find(|(f, o, _)| *f == file && *o == ordinal);
    let parsed = parse_str(&query);
    if let Some((_, _, error)) = defect {
        let reported = parsed.as_ref().err().map(ToString::to_string);
        assert_eq!(
            reported.as_deref(),
            Some(*error),
            "{file} block {ordinal}: a known crate defect changed; re-adjudicate it"
        );
        return;
    }
    match grammar {
        Grammar::Derives => {
            let ast = parsed.unwrap_or_else(|e| {
                panic!("{file} block {ordinal}: the grammar derives it, the crate refuses: {e}\n{query}")
            });
            let printed = to_aql(&ast);
            let again = parse_str(&printed).unwrap_or_else(|e| {
                panic!("{file} block {ordinal}: printed form refused: {e}\n{printed}")
            });
            assert_eq!(
                again, ast,
                "{file} block {ordinal}: round trip drifted\n{printed}"
            );
        }
        Grammar::LexerRefuses { rule, at } => {
            let expected = offset_of(file, ordinal, &query, at);
            match parsed {
                Err(ParseError::Lex(lex)) => assert_eq!(
                    lex.span.start, expected,
                    "{file} block {ordinal}: lexed elsewhere than {rule}: {lex}"
                ),
                other => panic!("{file} block {ordinal}: {rule}, and the crate gives {other:?}"),
            }
        }
        Grammar::ParserRefuses { rule, at } => {
            let expected = offset_of(file, ordinal, &query, at);
            match parsed {
                Err(ParseError::Syntax { faults }) => {
                    let first = faults.first().and_then(|fault| fault.bytes.clone());
                    assert_eq!(
                        first.map(|bytes| bytes.start),
                        Some(expected),
                        "{file} block {ordinal}: refused elsewhere than `{rule}` refuses it: {faults:?}"
                    );
                }
                other => panic!(
                    "{file} block {ordinal}: `{rule}` refuses it, and the crate gives {other:?}"
                ),
            }
        }
    }
}

/// The lexer refusal of a cADL interval: `AqlLexer.g4` matches `|` only
/// inside a `TERM_CODE` after `::`, and no other rule starts with it.
const LONE_BAR: &str = "no AqlLexer.g4 rule matches a lone `|`";

macro_rules! examples {
    ($($test:ident => ($file:literal, $ordinal:literal, $grammar:expr)),+ $(,)?) => {
        /// Every block the tests below cover, as `(file, ordinal)`.
        const COVERED: &[(&str, usize)] = &[$(($file, $ordinal)),+];
        $(
            #[test]
            fn $test() {
                judge($file, $ordinal, $grammar);
            }
        )+
    };
}

examples! {
    // §Aggregate functions, Examples 1-3: `aggregateFunctionCall`.
    aggregate_count_and_min => ("master03-syntax-function.adoc", 0, Grammar::Derives),
    aggregate_count_distinct => ("master03-syntax-function.adoc", 1, Grammar::Derives),
    aggregate_max_min_avg => ("master03-syntax-function.adoc", 2, Grammar::Derives),
    // §TERMINOLOGY, Examples 1-4: `terminologyFunction` as a `matchesOperand`
    // and as the `functionCall` of an `identifiedExpr`.
    terminology_expand => ("master03-syntax-function.adoc", 3, Grammar::Derives),
    terminology_validate => ("master03-syntax-function.adoc", 4, Grammar::Derives),
    terminology_lookup => ("master03-syntax-function.adoc", 5, Grammar::Derives),
    terminology_map => ("master03-syntax-function.adoc", 6, Grammar::Derives),
    // §TERMINOLOGY, Example 5: `terminologyFunction` takes three STRING
    // operands, and the third here is a CONCAT `functionCall`.
    terminology_subsumes_with_concat => ("master03-syntax-function.adoc", 7,
        Grammar::ParserRefuses { rule: "terminologyFunction", at: "CONCAT(" }),
    // §matches: an ADL 1.4 cADL block is no `matchesOperand` (a value list,
    // a terminologyFunction, or a URI).
    // The cADL interval `<|>=11|>` does not lex, and `matchesOperand`
    // would refuse `C_DV_QUANTITY<` before it.
    matches_cadl_quantity_list => ("master03-syntax-operator.adoc", 0,
        Grammar::LexerRefuses { rule: LONE_BAR, at: "|>=11|" }),
    matches_value_lists => ("master03-syntax-operator.adoc", 1, Grammar::Derives),
    // §matches: the cADL intervals do not lex, and `selectQuery` would
    // refuse the missing FROM before them.
    matches_cadl_interval_without_from => ("master03-syntax-operator.adoc", 2,
        Grammar::LexerRefuses { rule: LONE_BAR, at: "|5.0..6.0|" }),
    matches_value_lists_alternative => ("master03-syntax-operator.adoc", 3, Grammar::Derives),
    matches_cadl_proportion_without_from => ("master03-syntax-operator.adoc", 4,
        Grammar::LexerRefuses { rule: LONE_BAR, at: "|>7.0|" }),
    comparison_on_numerator => ("master03-syntax-operator.adoc", 5, Grammar::Derives),
    matches_cadl_cholesterol_without_from => ("master03-syntax-operator.adoc", 6,
        Grammar::LexerRefuses { rule: LONE_BAR, at: "|>=5.0|" }),
    // §Arithmetic operators: `containsExpr AND containsExpr` needs a class
    // expression after AND, and the example puts CONTAINS there.
    arithmetic_and_contains => ("master03-syntax-operator.adoc", 7,
        Grammar::ParserRefuses { rule: "containsExpr", at: "CONTAINS (COMPOSITION c2" }),
    // §Nest Query and 'Not in': `identifiedExpr` has no IN alternative.
    nested_not_in => ("master03-syntax-operator.adoc", 8,
        Grammar::ParserRefuses { rule: "identifiedExpr", at: "not in (" }),
    // Single-Subject AQL Examples: ORDER BY and LIMIT.
    single_subject_latest_five => ("master04-single_subject.adoc", 0, Grammar::Derives),
}

/// Every listing block of every example file is collected, and every one
/// has its test: a block the extractor skips, or one with no expectation,
/// fails here.
#[test]
fn every_listing_block_is_collected() {
    let dir = repo_root().join(EXAMPLES);
    let mut counts = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|x| x == "adoc") {
            let blocks = listing_blocks(&std::fs::read_to_string(&path).unwrap()).len();
            if blocks > 0 {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                counts.push((name, blocks));
            }
        }
    }
    counts.sort();
    let expected: Vec<(String, usize)> = BLOCKS_PER_FILE
        .iter()
        .map(|(file, n)| ((*file).to_owned(), *n))
        .collect();
    assert_eq!(counts, expected);
    let mut covered: Vec<(String, usize)> = COVERED
        .iter()
        .map(|(file, ordinal)| ((*file).to_owned(), *ordinal))
        .collect();
    covered.sort();
    let mut all: Vec<(String, usize)> = expected
        .iter()
        .flat_map(|(file, n)| (0..*n).map(move |i| (file.clone(), i)))
        .collect();
    all.sort();
    assert_eq!(covered, all);
    assert_eq!(all.len(), 18);
}
