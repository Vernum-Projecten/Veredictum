// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `xpath` criterion under the condition profile.
//!
//! Arazzo 1.1.0 §XPath Conditions makes XPath 3.1 the default language of a
//! `type: xpath` condition and converts its result to pass or fail by the
//! Effective Boolean Value of the version in use (XPath 3.1 §2.4.3,
//! <https://www.w3.org/TR/xpath-31/#id-ebv>). The profile pins three things
//! Arazzo leaves open: the only version is XPath 3.1, the only context is the
//! whole response body, and the openEHR namespaces are bound to the fixed
//! prefixes of [`NAMESPACES`], so a condition names an element the same way
//! whatever prefix the server chose. The engine is `oxixml-xpath`
//! (<https://docs.rs/oxixml-xpath/0.1.2>).

use oxixml_infoset::Tree;
use oxixml_xpath::{StaticContext, XPath, XPathVersion};

use crate::arazzo::expr::{
    EvalError, Expr, Judgement, ParseError, Scope, value_text, with_placeholders,
};

/// The namespace prefixes every `xpath` condition may use, each bound to a
/// fixed URI.
///
/// `oe` is the namespace of the stable ITS-XML Release-1.0.2 schemas, `oe2`
/// the one ITS-XML Release-2.0.0 moved to (`specs/openehr/ITS-XML/README.adoc`
/// §Releases and IM Versions: "the internal namespace used in the schemas is
/// also changed to `http://schemas.openehr.org/v2`"), and `xsi` the XML
/// Schema instance namespace an `xsi:type` attribute is read in (XML Schema
/// Part 1 §2.6.1). XPath 3.1 §2.1.1 binds `xs`, `fn`, `map`, `array` and
/// `math` already.
pub const NAMESPACES: [(&str, &str); 3] = [
    ("oe", "http://schemas.openehr.org/v1"),
    ("oe2", "http://schemas.openehr.org/v2"),
    ("xsi", "http://www.w3.org/2001/XMLSchema-instance"),
];

/// The static context every `xpath` condition compiles against.
fn static_context() -> StaticContext {
    StaticContext::new().with_namespaces(NAMESPACES)
}

/// Compiles `query` as XPath 3.1 under the profile's static context.
fn compile(query: &str) -> Result<oxixml_xpath::CompiledXPath, String> {
    XPath::compile(query, XPathVersion::XPath31, static_context()).map_err(|e| e.to_string())
}

/// An `xpath` criterion: an XPath 3.1 expression applied to the response body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XPathCriterion {
    /// The context expression, always the whole `$response.body`.
    pub context: Expr,
    /// The expression, with embedded `{$…}` expressions still in place.
    pub query: String,
}

impl XPathCriterion {
    /// Parses the context and checks the expression compiles as XPath 3.1
    /// once every embedded expression is replaced by an inert value.
    ///
    /// # Errors
    /// [`ParseError::XPathContext`] when the context is not the whole
    /// `$response.body`, and [`ParseError::XPath`] when the expression does
    /// not compile: a syntax error, an unknown function, or a prefix outside
    /// [`NAMESPACES`].
    pub fn parse(context: &str, query: &str) -> Result<Self, ParseError> {
        let context_expr = Expr::parse(context)?;
        if context_expr != Expr::ResponseBody(String::new()) {
            return Err(ParseError::XPathContext(context.to_owned()));
        }
        let checked = with_placeholders(query)?;
        compile(&checked).map_err(|reason| ParseError::XPath {
            query: query.to_owned(),
            reason,
        })?;
        Ok(Self {
            context: context_expr,
            query: query.to_owned(),
        })
    }

    /// Judges the criterion in `scope`: it holds when the Effective Boolean
    /// Value of the result is true (Arazzo 1.1.0 §XPath Conditions).
    ///
    /// An empty body does not hold: Arazzo 1.1.0 §XPath Conditions makes a
    /// context that "evaluates to `null` or `undefined`" fail.
    ///
    /// # Errors
    /// [`EvalError`] when the body is not well-formed XML, an embedded value
    /// cannot be placed, the substituted expression does not compile, or the
    /// evaluation raises a dynamic error, `FORG0006` (no Effective Boolean
    /// Value) among them. Each makes the row errored.
    pub fn judge(&self, scope: &Scope<'_>) -> Result<Judgement, EvalError> {
        let query = substitute(&self.query, scope)?;
        let compiled =
            compile(&query).map_err(|reason| EvalError::BadXPath(query.clone(), reason))?;
        let expected = format!("{} satisfies the XPath {query}", self.context);
        let Some((_, response)) = scope.exchange else {
            return Err(EvalError::NoExchange(self.context.to_string()));
        };
        if response.text.trim().is_empty() {
            return Ok(Judgement {
                holds: false,
                expected,
                received: format!("{} is absent", self.context),
            });
        }
        let tree = Tree::parse_str(&response.text)
            .map_err(|error| EvalError::UnreadableXml(error.to_string()))?;
        let value = compiled
            .evaluate_value_in(&compiled.context().with_context_item(tree.root()))
            .map_err(|error| EvalError::XPath(query.clone(), error.to_string()))?;
        let holds = value
            .effective_boolean_value()
            .map_err(|error| EvalError::XPath(query.clone(), error.to_string()))?;
        let received = match value.len() {
            0 => format!("{query} yields the empty sequence"),
            1 => format!("{query} yields an item whose effective boolean value is {holds}"),
            many => format!("{query} yields {many} items"),
        };
        Ok(Judgement {
            holds,
            expected,
            received,
        })
    }
}

/// Where an embedded expression stands in an XPath expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Literal {
    /// Outside any string literal.
    None,
    /// Inside an apostrophe-delimited string literal.
    Apos,
    /// Inside a quotation-mark-delimited string literal.
    Quot,
}

/// Substitutes every embedded `{$expression}` of `template` (Arazzo 1.1.0
/// §Condition Evaluation) as an inert value.
///
/// Inside a string literal the value's delimiter is doubled, the escape
/// XPath 3.1 §A.2.1 defines (`EscapeApos`, `EscapeQuot`). Outside a literal
/// only a numeric literal is placed, because any other text would be read as
/// expression syntax.
fn substitute(template: &str, scope: &Scope<'_>) -> Result<String, EvalError> {
    let mut out = String::with_capacity(template.len());
    let mut state = Literal::None;
    let mut rest = template;
    while let Some(c) = rest.chars().next() {
        if c == '{'
            && rest.get(1..).is_some_and(|tail| tail.starts_with('$'))
            && let Some(end) = rest.find('}')
        {
            let inner = rest.get(1..end).unwrap_or_default();
            let expr = Expr::parse(inner)?;
            let value = scope
                .eval(&expr)?
                .ok_or_else(|| EvalError::MissingOutput(expr.to_string()))?;
            let text = value_text(&value);
            match state {
                Literal::Apos => out.push_str(&text.replace('\'', "''")),
                Literal::Quot => out.push_str(&text.replace('"', "\"\"")),
                Literal::None if is_numeric_literal(&text) => out.push_str(&text),
                Literal::None => {
                    return Err(EvalError::BadXPath(
                        template.to_owned(),
                        format!(
                            "`{expr}` is `{text}`, which is placed outside a string literal and is not a number"
                        ),
                    ));
                }
            }
            rest = rest.get(end.saturating_add(1)..).unwrap_or_default();
            continue;
        }
        if c == '{' && rest.get(1..).is_some_and(|tail| tail.starts_with('$')) {
            return Err(ParseError::Unterminated(template.to_owned()).into());
        }
        state = match (state, c) {
            (Literal::None, '\'') => Literal::Apos,
            (Literal::None, '"') => Literal::Quot,
            (Literal::Apos, '\'') | (Literal::Quot, '"') => Literal::None,
            (unchanged, _) => unchanged,
        };
        out.push(c);
        rest = rest.get(c.len_utf8()..).unwrap_or_default();
    }
    Ok(out)
}

/// Whether `text` is an XPath 3.1 numeric literal, optionally negated
/// (XPath 3.1 §A.1 `NumericLiteral`, with the unary minus of §3.4).
fn is_numeric_literal(text: &str) -> bool {
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(at) => (
            unsigned.get(..at).unwrap_or_default(),
            unsigned.get(at.saturating_add(1)..),
        ),
        None => (unsigned, None),
    };
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    let mantissa_ok = match mantissa.split_once('.') {
        None => digits(mantissa),
        Some((whole, fraction)) => {
            (digits(whole) && (fraction.is_empty() || digits(fraction)))
                || (whole.is_empty() && digits(fraction))
        }
    };
    let exponent_ok = exponent.is_none_or(|e| digits(e.strip_prefix(['+', '-']).unwrap_or(e)));
    mantissa_ok && exponent_ok
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;

    use super::*;
    use crate::arazzo::expr::{RequestView, ResponseView};

    const COMPOSITION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<composition xmlns="http://schemas.openehr.org/v1"
             xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
             archetype_node_id="openEHR-EHR-COMPOSITION.encounter.v1">
  <name><value>Encounter</value></name>
  <uid xsi:type="OBJECT_VERSION_ID"><value>8849182c-82ad-4088-a07f-48ead4180515::local::1</value></uid>
  <content xsi:type="OBSERVATION" archetype_node_id="at1"><name><value>BP</value></name></content>
  <content xsi:type="OBSERVATION" archetype_node_id="at2"><name><value>HR</value></name></content>
</composition>"#;

    fn judge(query: &str, body: &str, inputs: &[(&str, Value)]) -> Result<Judgement, EvalError> {
        let request = RequestView::default();
        let response = ResponseView {
            status: 200,
            headers: BTreeMap::new(),
            body: Err("not JSON".to_owned()),
            text: body.to_owned(),
        };
        let inputs: BTreeMap<String, Value> = inputs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect();
        let outputs = BTreeMap::new();
        let scope = Scope {
            inputs: &inputs,
            outputs: &outputs,
            workflows: &outputs,
            exchange: Some((&request, &response)),
        };
        XPathCriterion::parse("$response.body", query)?.judge(&scope)
    }

    #[test]
    fn a_holding_criterion_holds() {
        for query in [
            "/oe:composition",
            "count(/oe:composition/oe:content) eq 2",
            "/oe:composition/oe:uid/@xsi:type = 'OBJECT_VERSION_ID'",
            "/oe:composition/oe:name/oe:value",
            "string-join(/oe:composition/oe:content/@archetype_node_id, ',') eq 'at1,at2'",
        ] {
            let judged = judge(query, COMPOSITION, &[]).unwrap();
            assert!(judged.holds, "{query}: {}", judged.received);
        }
    }

    #[test]
    fn a_failing_criterion_fails() {
        for query in [
            "/oe2:composition",
            "/composition",
            "count(/oe:composition/oe:content) eq 3",
            "/oe:composition/oe:name/oe:value = 'Other'",
            "''",
        ] {
            let judged = judge(query, COMPOSITION, &[]).unwrap();
            assert!(!judged.holds, "{query}");
        }
    }

    #[test]
    fn an_unparsable_criterion_is_refused_and_an_undecidable_one_errors() {
        assert!(matches!(
            XPathCriterion::parse("$response.body", "/oe:composition["),
            Err(ParseError::XPath { .. })
        ));
        assert!(matches!(
            XPathCriterion::parse("$response.body", "/zz:composition"),
            Err(ParseError::XPath { .. })
        ));
        assert!(matches!(
            XPathCriterion::parse("$response.body#/a", "/oe:composition"),
            Err(ParseError::XPathContext(_))
        ));
        assert!(matches!(
            judge("(1, 2)", COMPOSITION, &[]),
            Err(EvalError::XPath(_, reason)) if reason.contains("FORG0006")
        ));
        assert!(matches!(
            judge("/oe:composition", "<composition><name></composition>", &[]),
            Err(EvalError::UnreadableXml(_))
        ));
    }

    #[test]
    fn an_empty_body_does_not_hold() {
        let judged = judge("/oe:composition", "", &[]).unwrap();
        assert!(!judged.holds);
        assert!(judged.received.contains("absent"));
    }

    #[test]
    fn embedded_values_are_placed_as_literals() {
        let judged = judge(
            "/oe:composition/oe:name/oe:value = '{$inputs.name}'",
            COMPOSITION,
            &[("name", Value::from("Encounter"))],
        )
        .unwrap();
        assert!(judged.holds);
        let quoted = judge(
            "/oe:composition/oe:name/oe:value = \"{$inputs.name}\"",
            COMPOSITION,
            &[("name", Value::from("En\"counter"))],
        )
        .unwrap();
        assert!(!quoted.holds);
        let counted = judge(
            "count(/oe:composition/oe:content) eq {$inputs.n}",
            COMPOSITION,
            &[("n", Value::from(2))],
        )
        .unwrap();
        assert!(counted.holds);
        assert!(matches!(
            judge(
                "count(/oe:composition/oe:content) eq {$inputs.n}",
                COMPOSITION,
                &[("n", Value::from("2) or (1"))],
            ),
            Err(EvalError::BadXPath(..))
        ));
    }

    #[test]
    fn an_apostrophe_in_a_value_stays_inside_its_literal() {
        let body = r#"<a xmlns="http://schemas.openehr.org/v1"><v>O'Brien</v></a>"#;
        let judged = judge(
            "/oe:a/oe:v = '{$inputs.v}'",
            body,
            &[("v", Value::from("O'Brien"))],
        )
        .unwrap();
        assert!(judged.holds, "{}", judged.received);
    }

    #[test]
    fn numeric_literals_are_recognised() {
        for yes in ["0", "12", "-3", "1.5", ".5", "5.", "1e3", "2.5E-2"] {
            assert!(is_numeric_literal(yes), "{yes}");
        }
        for no in ["", "-", "x", "1e", "1.2.3", "NaN", "inf", "1 or 1", "+1"] {
            assert!(!is_numeric_literal(no), "{no}");
        }
    }
}
