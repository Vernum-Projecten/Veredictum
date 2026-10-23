// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The condition profile's regex dialect: the syntax ECMA-262 (with the `u`
//! flag) and the `regex` crate share, with ECMA-262 meaning.
//!
//! Arazzo 1.1.0 §Regex Conditions names no dialect, so the profile pins one.
//! [`check`] refuses every construct outside the shared syntax: lookaround
//! and backreferences (ECMA-262 has them, `regex` does not), and inline
//! flags, `(?P<…>)`, `\A`, `\z`, the extended word boundaries, `\p{…}`,
//! POSIX classes, nested classes, class set operations, `\x{…}`, `\U…` and
//! `\a` (`regex` has them, ECMA-262 does not). [`compile`] then rewrites the
//! four constructs whose meaning differs between the two (`.`, `\d`, `\w`,
//! `\s` and their negations, `\b`, `\B`) into the ECMA-262 meaning before the
//! pattern is compiled, so a pattern matches what an ECMA-262 engine matches.
//! The ECMA-262 meanings are the `CharacterClassEscape` and `Atom` productions,
//! <https://tc39.es/ecma262/#sec-characterclassescape> and
//! <https://tc39.es/ecma262/#sec-atom>, with the `WhiteSpace` and
//! `LineTerminator` code points they name.

use std::fmt;

use regex_syntax::ast::{
    self, AssertionKind, Ast, ClassPerl, ClassPerlKind, ClassSetItem, ErrorKind, GroupKind,
    HexLiteralKind, LiteralKind, SpecialLiteralKind, Visitor,
};

/// A construct of a pattern that lies outside the profile's shared syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegexConstruct {
    /// `(?=…)`, `(?!…)`, `(?<=…)` or `(?<!…)`.
    LookAround,
    /// `\1` or `\k<name>`.
    Backreference,
    /// `(?i)` or `(?i:…)`: a flag set inside the pattern.
    InlineFlags,
    /// `(?P<name>…)`; the shared spelling is `(?<name>…)`.
    PythonNamedGroup,
    /// `\A` or `\z`; the shared spelling is `^` or `$`.
    TextAnchor,
    /// `\b{start}`, `\<` and the other extended word boundaries.
    ExtendedWordBoundary,
    /// `\p{…}` or `\P{…}`.
    UnicodeClass,
    /// `[[:alpha:]]`.
    PosixClass,
    /// A bracket class nested in a bracket class.
    NestedClass,
    /// `&&`, `--` or `~~` inside a bracket class.
    ClassSetOperation,
    /// A negated class escape (`\D`, `\W`, `\S`) inside a bracket class.
    NegatedClassEscapeInClass,
    /// A bracket class that opens with `]`, which the two dialects read
    /// differently.
    LeadingBracketInClass,
    /// An escape form only one dialect has (`\x{…}`, `\U…`, `\a`, `\#`, …).
    Escape(String),
    /// A `]` or `}` outside a class, which ECMA-262 with `u` refuses.
    LoneBracket(char),
    /// The pattern does not parse at all.
    Syntax(String),
}

impl fmt::Display for RegexConstruct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LookAround => f.write_str("lookaround"),
            Self::Backreference => f.write_str("a backreference"),
            Self::InlineFlags => f.write_str("inline flags"),
            Self::PythonNamedGroup => f.write_str("a `(?P<name>…)` group"),
            Self::TextAnchor => f.write_str("a `\\A` or `\\z` anchor"),
            Self::ExtendedWordBoundary => f.write_str("an extended word boundary"),
            Self::UnicodeClass => f.write_str("a `\\p` Unicode class"),
            Self::PosixClass => f.write_str("a POSIX class"),
            Self::NestedClass => f.write_str("a nested bracket class"),
            Self::ClassSetOperation => f.write_str("a class set operation"),
            Self::NegatedClassEscapeInClass => {
                f.write_str("a negated class escape inside a bracket class")
            }
            Self::LeadingBracketInClass => f.write_str("a bracket class opening with `]`"),
            Self::Escape(form) => write!(f, "the escape `{form}`"),
            Self::LoneBracket(c) => write!(f, "a lone `{c}`"),
            Self::Syntax(reason) => write!(f, "invalid syntax ({reason})"),
        }
    }
}

/// Why a pattern does not compile once substituted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the regex `{pattern}` does not compile: {reason}")]
pub struct CompileError {
    /// The pattern as substituted.
    pub pattern: String,
    /// The verbatim reason.
    pub reason: String,
}

/// Checks that `pattern` stays inside the profile's shared syntax.
///
/// # Errors
/// The first [`RegexConstruct`] outside it.
pub fn check(pattern: &str) -> Result<(), RegexConstruct> {
    let tree = parse(pattern)?;
    ast::visit(
        &tree,
        Walker {
            pattern,
            refuse: true,
            rewrites: Vec::new(),
        },
    )
    .map(drop)
}

/// Compiles a substituted pattern with ECMA-262 meaning.
///
/// # Errors
/// [`CompileError`] when the pattern does not parse or compile.
pub fn compile(pattern: &str) -> Result<regex::Regex, CompileError> {
    let fail = |reason: String| CompileError {
        pattern: pattern.to_owned(),
        reason,
    };
    let tree = parse(pattern).map_err(|construct| fail(construct.to_string()))?;
    let rewrites = ast::visit(
        &tree,
        Walker {
            pattern,
            refuse: false,
            rewrites: Vec::new(),
        },
    )
    .map_err(|construct| fail(construct.to_string()))?;
    let translated = apply(pattern, rewrites).ok_or_else(|| fail("a rewrite span".to_owned()))?;
    regex::Regex::new(&translated).map_err(|error| fail(error.to_string()))
}

fn parse(pattern: &str) -> Result<Ast, RegexConstruct> {
    ast::parse::Parser::new()
        .parse(pattern)
        .map_err(|error| match error.kind() {
            ErrorKind::UnsupportedLookAround => RegexConstruct::LookAround,
            ErrorKind::UnsupportedBackreference => RegexConstruct::Backreference,
            other => RegexConstruct::Syntax(other.to_string()),
        })
}

/// Splices every rewrite into the pattern; `None` when a span is not on a
/// character boundary, which the parser's spans never are.
fn apply(pattern: &str, mut rewrites: Vec<(usize, usize, String)>) -> Option<String> {
    rewrites.sort_by_key(|(start, _, _)| *start);
    let mut out = String::new();
    let mut at = 0;
    for (start, end, text) in rewrites {
        out.push_str(pattern.get(at..start)?);
        out.push_str(&text);
        at = end;
    }
    out.push_str(pattern.get(at..)?);
    Some(out)
}

/// ECMA-262 `WhiteSpace` and `LineTerminator`, as class items.
const ECMA_SPACE: &str =
    r"\t\n\x0B\x0C\r \xA0\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

/// The class items an ECMA-262 class escape denotes.
fn ecma_items(kind: &ClassPerlKind) -> &'static str {
    match kind {
        ClassPerlKind::Digit => "0-9",
        ClassPerlKind::Word => "0-9A-Za-z_",
        ClassPerlKind::Space => ECMA_SPACE,
    }
}

struct Walker<'p> {
    pattern: &'p str,
    refuse: bool,
    rewrites: Vec<(usize, usize, String)>,
}

impl Walker<'_> {
    fn refuse(&self, construct: RegexConstruct) -> Result<(), RegexConstruct> {
        if self.refuse { Err(construct) } else { Ok(()) }
    }

    fn text(&self, span: &ast::Span) -> &str {
        self.pattern
            .get(span.start.offset..span.end.offset)
            .unwrap_or_default()
    }

    fn rewrite(&mut self, span: &ast::Span, text: String) {
        self.rewrites
            .push((span.start.offset, span.end.offset, text));
    }

    fn literal(&self, literal: &ast::Literal, in_class: bool) -> Result<(), RegexConstruct> {
        let escape = || RegexConstruct::Escape(self.text(&literal.span).to_owned());
        match &literal.kind {
            LiteralKind::Verbatim if !in_class && matches!(literal.c, ']' | '}') => {
                self.refuse(RegexConstruct::LoneBracket(literal.c))
            }
            LiteralKind::Meta if matches!(literal.c, '#' | '&' | '~') => self.refuse(escape()),
            LiteralKind::Meta if literal.c == '-' && !in_class => self.refuse(escape()),
            LiteralKind::Superfluous if !(literal.c == '/' || (in_class && literal.c == '-')) => {
                self.refuse(escape())
            }
            LiteralKind::Octal
            | LiteralKind::HexFixed(HexLiteralKind::UnicodeLong)
            | LiteralKind::HexBrace(HexLiteralKind::X | HexLiteralKind::UnicodeLong)
            | LiteralKind::Special(SpecialLiteralKind::Bell | SpecialLiteralKind::Space) => {
                self.refuse(escape())
            }
            _ => Ok(()),
        }
    }

    fn perl(&mut self, class: &ClassPerl, in_class: bool) -> Result<(), RegexConstruct> {
        let items = ecma_items(&class.kind);
        if in_class {
            if class.negated {
                return self.refuse(RegexConstruct::NegatedClassEscapeInClass);
            }
            self.rewrite(&class.span, items.to_owned());
        } else {
            let caret = if class.negated { "^" } else { "" };
            self.rewrite(&class.span, format!("[{caret}{items}]"));
        }
        Ok(())
    }
}

impl Visitor for Walker<'_> {
    type Output = Vec<(usize, usize, String)>;
    type Err = RegexConstruct;

    fn finish(self) -> Result<Self::Output, Self::Err> {
        Ok(self.rewrites)
    }

    fn visit_pre(&mut self, node: &Ast) -> Result<(), RegexConstruct> {
        match node {
            Ast::Flags(_) => self.refuse(RegexConstruct::InlineFlags),
            Ast::Literal(literal) => self.literal(literal, false),
            Ast::Dot(span) => {
                self.rewrite(span, r"[^\n\r\x{2028}\x{2029}]".to_owned());
                Ok(())
            }
            Ast::Assertion(assertion) => match assertion.kind {
                AssertionKind::StartLine | AssertionKind::EndLine => Ok(()),
                AssertionKind::WordBoundary => {
                    self.rewrite(&assertion.span, r"(?-u:\b)".to_owned());
                    Ok(())
                }
                AssertionKind::NotWordBoundary => {
                    self.rewrite(&assertion.span, r"(?-u:\B)".to_owned());
                    Ok(())
                }
                AssertionKind::StartText | AssertionKind::EndText => {
                    self.refuse(RegexConstruct::TextAnchor)
                }
                _ => self.refuse(RegexConstruct::ExtendedWordBoundary),
            },
            Ast::ClassUnicode(_) => self.refuse(RegexConstruct::UnicodeClass),
            Ast::ClassPerl(class) => self.perl(class, false),
            Ast::ClassBracketed(class) => {
                let text = self.text(&class.span);
                if text.starts_with("[]") || text.starts_with("[^]") {
                    return self.refuse(RegexConstruct::LeadingBracketInClass);
                }
                if matches!(class.kind, ast::ClassSet::BinaryOp(_)) {
                    return self.refuse(RegexConstruct::ClassSetOperation);
                }
                Ok(())
            }
            Ast::Group(group) => match &group.kind {
                GroupKind::CaptureName {
                    starts_with_p: true,
                    ..
                } => self.refuse(RegexConstruct::PythonNamedGroup),
                GroupKind::NonCapturing(flags) if !flags.items.is_empty() => {
                    self.refuse(RegexConstruct::InlineFlags)
                }
                _ => Ok(()),
            },
            Ast::Repetition(_) | Ast::Empty(_) | Ast::Alternation(_) | Ast::Concat(_) => Ok(()),
        }
    }

    fn visit_class_set_item_pre(&mut self, item: &ClassSetItem) -> Result<(), RegexConstruct> {
        match item {
            ClassSetItem::Literal(literal) => self.literal(literal, true),
            ClassSetItem::Range(range) => {
                self.literal(&range.start, true)?;
                self.literal(&range.end, true)
            }
            ClassSetItem::Ascii(_) => self.refuse(RegexConstruct::PosixClass),
            ClassSetItem::Unicode(_) => self.refuse(RegexConstruct::UnicodeClass),
            ClassSetItem::Perl(class) => self.perl(class, true),
            ClassSetItem::Bracketed(_) => self.refuse(RegexConstruct::NestedClass),
            ClassSetItem::Empty(_) | ClassSetItem::Union(_) => Ok(()),
        }
    }

    fn visit_class_set_binary_op_pre(
        &mut self,
        _op: &ast::ClassSetBinaryOp,
    ) -> Result<(), RegexConstruct> {
        self.refuse(RegexConstruct::ClassSetOperation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_syntax_is_accepted() {
        for pattern in [
            r"^[0-9a-f]{8}-[0-9a-f]{4}$",
            r"^2\d{2}$",
            r"^(?<major>\d+)\.(?:\d+)$",
            r"a|b*?c+",
            r"[\w.\-]+",
            r"\u0041\x41\u{1F600}",
            r"\/x\b\B",
            "",
        ] {
            assert_eq!(check(pattern), Ok(()), "{pattern}");
        }
    }

    #[test]
    fn lookaround_and_backreferences_are_refused() {
        assert_eq!(check("a(?=b)"), Err(RegexConstruct::LookAround));
        assert_eq!(check("a(?!b)"), Err(RegexConstruct::LookAround));
        assert_eq!(check("(?<=a)b"), Err(RegexConstruct::LookAround));
        assert_eq!(check("(?<!a)b"), Err(RegexConstruct::LookAround));
        assert_eq!(check(r"(a)\1"), Err(RegexConstruct::Backreference));
    }

    #[test]
    fn constructs_only_one_dialect_has_are_refused() {
        let cases = [
            ("(?i)abc", RegexConstruct::InlineFlags),
            ("(?i:abc)", RegexConstruct::InlineFlags),
            ("(?P<n>a)", RegexConstruct::PythonNamedGroup),
            (r"\Aabc\z", RegexConstruct::TextAnchor),
            (r"\b{start}a", RegexConstruct::ExtendedWordBoundary),
            (r"\pL", RegexConstruct::UnicodeClass),
            ("[[:alpha:]]", RegexConstruct::PosixClass),
            ("[a[b]]", RegexConstruct::NestedClass),
            ("[a-z&&b]", RegexConstruct::ClassSetOperation),
            (r"[\D]", RegexConstruct::NegatedClassEscapeInClass),
            ("[]a]", RegexConstruct::LeadingBracketInClass),
            (r"\x{41}", RegexConstruct::Escape(r"\x{41}".to_owned())),
            (
                r"\U00000041",
                RegexConstruct::Escape(r"\U00000041".to_owned()),
            ),
            (r"\a", RegexConstruct::Escape(r"\a".to_owned())),
            (r"\#", RegexConstruct::Escape(r"\#".to_owned())),
            (r"a\-b", RegexConstruct::Escape(r"\-".to_owned())),
            ("a]", RegexConstruct::LoneBracket(']')),
        ];
        for (pattern, construct) in cases {
            assert_eq!(check(pattern), Err(construct), "{pattern}");
        }
        assert!(matches!(check("a("), Err(RegexConstruct::Syntax(_))));
        assert!(
            matches!(check("a{,3}"), Err(RegexConstruct::Syntax(_))),
            "ECMA-262 does not read `{{,n}}` as a repetition, and `regex` refuses it"
        );
    }

    #[test]
    fn the_class_escapes_and_dot_carry_their_ecma_meaning() {
        let digit = compile(r"^\d$").unwrap();
        assert!(digit.is_match("7"));
        assert!(
            !digit.is_match("\u{0663}"),
            "ARABIC-INDIC DIGIT THREE is no ECMA digit"
        );
        let word = compile(r"^\w+$").unwrap();
        assert!(word.is_match("a_9"));
        assert!(!word.is_match("é"), "ECMA \\w is ASCII");
        let space = compile(r"^\s$").unwrap();
        assert!(space.is_match("\u{FEFF}"), "ECMA \\s includes U+FEFF");
        assert!(!space.is_match("\u{0085}"), "ECMA \\s excludes NEL");
        let dot = compile("^a.b$").unwrap();
        assert!(
            dot.is_match("a\u{1F600}b"),
            "with `u` a dot is one code point"
        );
        assert!(
            !dot.is_match("a\rb"),
            "an ECMA dot excludes line terminators"
        );
        let in_class = compile(r"^[\d.]+$").unwrap();
        assert!(in_class.is_match("1.2"));
        assert!(!in_class.is_match("1x2"));
        let boundary = compile(r"\bé").unwrap();
        assert!(boundary.is_match("aé"), "an ECMA word boundary is ASCII");
        assert!(!boundary.is_match(" é"), "é is no ECMA word character");
        assert!(compile(r"a\.b").unwrap().is_match("a.b"));
        assert!(!compile(r"a\.b").unwrap().is_match("axb"));
    }

    #[test]
    fn an_escaped_substitution_compiles_as_a_literal() {
        let pattern = format!("^{}$", regex::escape("a.b#c&d"));
        let compiled = compile(&pattern).unwrap();
        assert!(compiled.is_match("a.b#c&d"));
        assert!(!compiled.is_match("aXb#c&d"));
    }
}
