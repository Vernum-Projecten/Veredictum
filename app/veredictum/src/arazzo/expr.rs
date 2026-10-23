// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Arazzo runtime expressions and the criteria judged under the condition
//! profile.
//!
//! The expression forms are the subset of the Arazzo 1.1.0 runtime
//! expression grammar the catalogue uses (§Runtime Expressions,
//! <https://spec.openapis.org/arazzo/v1.1.0.html#runtime-expressions>). A form
//! outside it is a parse error, never a silent literal.
//!
//! The profile pins what Arazzo leaves open. A `simple` condition compares an
//! expression with an integer, a boolean or `null`, and never with a string,
//! because Arazzo makes `simple` string comparison case-insensitive. A
//! `regex` condition is matched against its context in the dialect
//! [`crate::arazzo::regex_profile`] pins, with every embedded `{$expression}`
//! substituted as an escaped literal. A `jsonpath` condition is an RFC 9535
//! query that holds when its nodelist is not empty (Arazzo 1.1.0 §`JSONPath`
//! Conditions), with every embedded value escaped for an RFC 9535 string
//! literal. An `xpath` condition is refused: the profile names `XPath` 3.1, and
//! no engine for it is vetted here.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

use serde_json::Value;

use crate::arazzo::regex_profile::{self, RegexConstruct};

/// One parsed runtime expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// `$statusCode`.
    StatusCode,
    /// `$url`.
    Url,
    /// `$method`.
    Method,
    /// `$request.header.<name>`.
    RequestHeader(String),
    /// `$request.query.<name>`.
    RequestQuery(String),
    /// `$request.path.<name>`.
    RequestPath(String),
    /// `$response.header.<name>`.
    ResponseHeader(String),
    /// `$response.body`, with an optional JSON Pointer (`#/a/b`).
    ResponseBody(String),
    /// `$inputs.<name>`.
    Input(String),
    /// `$steps.<stepId>.outputs.<name>`.
    StepOutput {
        /// The step whose output is read.
        step: String,
        /// The output name.
        output: String,
    },
    /// `$workflows.<workflowId>.outputs.<name>`.
    WorkflowOutput {
        /// The workflow whose output is read.
        workflow: String,
        /// The output name.
        output: String,
    },
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StatusCode => f.write_str("$statusCode"),
            Self::Url => f.write_str("$url"),
            Self::Method => f.write_str("$method"),
            Self::RequestHeader(n) => write!(f, "$request.header.{n}"),
            Self::RequestQuery(n) => write!(f, "$request.query.{n}"),
            Self::RequestPath(n) => write!(f, "$request.path.{n}"),
            Self::ResponseHeader(n) => write!(f, "$response.header.{n}"),
            Self::ResponseBody(p) if p.is_empty() => f.write_str("$response.body"),
            Self::ResponseBody(p) => write!(f, "$response.body#{p}"),
            Self::Input(n) => write!(f, "$inputs.{n}"),
            Self::StepOutput { step, output } => write!(f, "$steps.{step}.outputs.{output}"),
            Self::WorkflowOutput { workflow, output } => {
                write!(f, "$workflows.{workflow}.outputs.{output}")
            }
        }
    }
}

/// Why an expression or a condition could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// The text is not a runtime expression this evaluator understands.
    #[error("`{0}` is not a supported runtime expression")]
    Expression(String),
    /// The condition is not one comparison the profile allows.
    #[error("`{0}` is not a `simple` condition the profile allows")]
    Condition(String),
    /// A `simple` condition compares a string, which the profile refuses.
    #[error(
        "`{0}` compares a string under `simple`, which Arazzo makes case-insensitive; use `regex` or `jsonpath`"
    )]
    StringComparison(String),
    /// An embedded `{$…}` expression is not closed.
    #[error("`{0}` has an unterminated embedded expression")]
    Unterminated(String),
    /// A `regex` condition uses a construct outside the profile's dialect.
    #[error("the regex `{pattern}` uses {construct}, which the profile refuses")]
    Regex {
        /// The pattern as authored.
        pattern: String,
        /// The refused construct.
        construct: RegexConstruct,
    },
    /// A `jsonpath` condition is not an RFC 9535 query.
    #[error("`{query}` is not an RFC 9535 JSONPath query: {reason}")]
    JsonPath {
        /// The query as authored.
        query: String,
        /// The parser's verbatim reason.
        reason: String,
    },
    /// An `xpath` condition, which the evaluator does not judge.
    #[error("`{0}` is an xpath condition, and no vetted XPath 3.1 engine judges it")]
    XPath(String),
    /// An Expression Type Object names a language or version the profile
    /// does not use.
    #[error("expression type `{kind}` version `{version}` is outside the profile")]
    ExpressionType {
        /// The expression language.
        kind: String,
        /// The version named.
        version: String,
    },
    /// A criterion type that needs a `context` has none.
    #[error("a `{0}` criterion needs a `context`")]
    MissingContext(String),
    /// An `x-veredictum-check` value that is not one of the three judges.
    #[error("`x-veredictum-check` is malformed: {0}")]
    Check(String),
}

impl Expr {
    /// Parses one runtime expression.
    ///
    /// # Errors
    /// [`ParseError::Expression`] for any text outside the supported forms.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let bad = || ParseError::Expression(text.to_owned());
        let name = |rest: &str| {
            if rest.is_empty() {
                Err(bad())
            } else {
                Ok(rest.to_owned())
            }
        };
        match text {
            "$statusCode" => return Ok(Self::StatusCode),
            "$url" => return Ok(Self::Url),
            "$method" => return Ok(Self::Method),
            "$response.body" => return Ok(Self::ResponseBody(String::new())),
            _ => {}
        }
        if let Some(pointer) = text.strip_prefix("$response.body#") {
            if !pointer.is_empty() && !pointer.starts_with('/') {
                return Err(bad());
            }
            return Ok(Self::ResponseBody(pointer.to_owned()));
        }
        if let Some(rest) = text.strip_prefix("$response.header.") {
            return name(rest).map(Self::ResponseHeader);
        }
        if let Some(rest) = text.strip_prefix("$request.header.") {
            return name(rest).map(Self::RequestHeader);
        }
        if let Some(rest) = text.strip_prefix("$request.query.") {
            return name(rest).map(Self::RequestQuery);
        }
        if let Some(rest) = text.strip_prefix("$request.path.") {
            return name(rest).map(Self::RequestPath);
        }
        if let Some(rest) = text.strip_prefix("$inputs.") {
            return name(rest).map(Self::Input);
        }
        if let Some(rest) = text.strip_prefix("$steps.")
            && let Some((step, output)) = rest.split_once(".outputs.")
            && !step.is_empty()
            && !output.is_empty()
        {
            return Ok(Self::StepOutput {
                step: step.to_owned(),
                output: output.to_owned(),
            });
        }
        if let Some(rest) = text.strip_prefix("$workflows.")
            && let Some((workflow, output)) = rest.split_once(".outputs.")
            && !workflow.is_empty()
            && !output.is_empty()
        {
            return Ok(Self::WorkflowOutput {
                workflow: workflow.to_owned(),
                output: output.to_owned(),
            });
        }
        Err(bad())
    }
}

/// Why an expression could not be evaluated: the instrument cannot judge.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvalError {
    /// The expression reads a request or response and none exists yet.
    #[error("`{0}` needs an exchange and none has happened in this step")]
    NoExchange(String),
    /// The expression names an input the run did not supply or generate.
    #[error("input `{0}` has no value")]
    MissingInput(String),
    /// The expression names a step output that was not captured.
    #[error("step output `{0}` was not captured")]
    MissingOutput(String),
    /// The response body is not JSON, so a pointer into it cannot be read.
    #[error("the response body is not JSON ({0}), so `$response.body` cannot be read")]
    UnreadableBody(String),
    /// A `simple` comparison met operand types the profile does not compare.
    #[error("`{0}` compares values the profile does not compare: {1}")]
    Incomparable(String, String),
    /// A `regex` context is not a string.
    #[error("the context of `{0}` is not a string: {1}")]
    NonStringContext(String, String),
    /// The `regex` condition does not compile.
    #[error("the regex `{0}` does not compile: {1}")]
    BadRegex(String, String),
    /// The substituted `jsonpath` condition does not parse.
    #[error("the JSONPath query `{0}` does not parse once substituted: {1}")]
    BadJsonPath(String, String),
    /// A judge cannot decide on this exchange.
    #[error("{0}")]
    Unjudgeable(String),
    /// The text could not be parsed.
    #[error(transparent)]
    Parse(#[from] ParseError),
}

/// The request half of one recorded exchange, as expressions read it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestView {
    /// The method, upper case.
    pub method: String,
    /// The full URL sent.
    pub url: String,
    /// The headers sent, names lower case.
    pub headers: BTreeMap<String, String>,
    /// The query parameters sent.
    pub query: BTreeMap<String, String>,
    /// The path parameters substituted.
    pub path: BTreeMap<String, String>,
}

/// The response half of one recorded exchange, as expressions read it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseView {
    /// The status code.
    pub status: u16,
    /// The headers received, names lower case.
    pub headers: BTreeMap<String, String>,
    /// The body parsed as JSON: `Ok(None)` when empty, `Err` when unparsable.
    pub body: Result<Option<Value>, String>,
}

/// Everything an expression may read at one point of a workflow run.
#[derive(Debug, Clone, Copy)]
pub struct Scope<'a> {
    /// The workflow inputs, supplied or generated.
    pub inputs: &'a BTreeMap<String, Value>,
    /// The outputs captured by earlier steps.
    pub outputs: &'a BTreeMap<String, BTreeMap<String, Value>>,
    /// The outputs of the workflows this one depends on.
    pub workflows: &'a BTreeMap<String, BTreeMap<String, Value>>,
    /// The current step's exchange, once it has happened.
    pub exchange: Option<(&'a RequestView, &'a ResponseView)>,
}

impl Scope<'_> {
    /// Evaluates `expr`; `Ok(None)` means the value is absent from a readable
    /// request or response.
    ///
    /// # Errors
    /// The [`EvalError`] when the instrument cannot read the value at all.
    pub fn eval(&self, expr: &Expr) -> Result<Option<Value>, EvalError> {
        let exchange = || {
            self.exchange
                .ok_or_else(|| EvalError::NoExchange(expr.to_string()))
        };
        Ok(match expr {
            Expr::Input(name) => Some(
                self.inputs
                    .get(name)
                    .cloned()
                    .ok_or_else(|| EvalError::MissingInput(name.clone()))?,
            ),
            Expr::StepOutput { step, output } => Some(
                self.outputs
                    .get(step)
                    .and_then(|o| o.get(output))
                    .cloned()
                    .ok_or_else(|| EvalError::MissingOutput(expr.to_string()))?,
            ),
            Expr::WorkflowOutput { workflow, output } => Some(
                self.workflows
                    .get(workflow)
                    .and_then(|o| o.get(output))
                    .cloned()
                    .ok_or_else(|| EvalError::MissingOutput(expr.to_string()))?,
            ),
            Expr::StatusCode => Some(Value::from(exchange()?.1.status)),
            Expr::Url => Some(Value::String(exchange()?.0.url.clone())),
            Expr::Method => Some(Value::String(exchange()?.0.method.clone())),
            Expr::RequestHeader(name) => exchange()?
                .0
                .headers
                .get(&name.to_ascii_lowercase())
                .cloned()
                .map(Value::String),
            Expr::RequestQuery(name) => exchange()?.0.query.get(name).cloned().map(Value::String),
            Expr::RequestPath(name) => exchange()?.0.path.get(name).cloned().map(Value::String),
            Expr::ResponseHeader(name) => exchange()?
                .1
                .headers
                .get(&name.to_ascii_lowercase())
                .cloned()
                .map(Value::String),
            Expr::ResponseBody(pointer) => match &exchange()?.1.body {
                Err(reason) => return Err(EvalError::UnreadableBody(reason.clone())),
                Ok(None) => None,
                Ok(Some(body)) => body.pointer(pointer).cloned(),
            },
        })
    }

    /// Substitutes every embedded `{$expression}` in `template`, passing each
    /// value's text through `escape`.
    ///
    /// # Errors
    /// [`EvalError`] when an embedded expression does not parse or has no value.
    pub fn substitute(
        &self,
        template: &str,
        escape: impl Fn(&str) -> String,
    ) -> Result<String, EvalError> {
        let mut out = String::new();
        let mut rest = template;
        while let Some(start) = rest.find("{$") {
            let (before, tail) = rest.split_at(start);
            out.push_str(before);
            let inner_and_after = tail.get(1..).unwrap_or_default();
            let Some(end) = inner_and_after.find('}') else {
                return Err(ParseError::Unterminated(template.to_owned()).into());
            };
            let (inner, after) = inner_and_after.split_at(end);
            let expr = Expr::parse(inner)?;
            let value = self
                .eval(&expr)?
                .ok_or_else(|| EvalError::MissingOutput(expr.to_string()))?;
            out.push_str(&escape(&value_text(&value)));
            rest = after.get(1..).unwrap_or_default();
        }
        out.push_str(rest);
        Ok(out)
    }
}

/// The text of a value as it is placed into a request or a pattern: a string
/// is its content, anything else its JSON text.
#[must_use]
pub fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// The comparison operators the profile allows in a `simple` condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    /// `==`.
    Eq,
    /// `!=`.
    Ne,
    /// `<`.
    Lt,
    /// `<=`.
    Le,
    /// `>`.
    Gt,
    /// `>=`.
    Ge,
}

impl CmpOp {
    fn parse(token: &str) -> Option<Self> {
        Some(match token {
            "==" => Self::Eq,
            "!=" => Self::Ne,
            "<" => Self::Lt,
            "<=" => Self::Le,
            ">" => Self::Gt,
            ">=" => Self::Ge,
            _ => return None,
        })
    }

    fn holds(self, ordering: std::cmp::Ordering) -> bool {
        use std::cmp::Ordering::{Equal, Greater, Less};
        match self {
            Self::Eq => ordering == Equal,
            Self::Ne => ordering != Equal,
            Self::Lt => ordering == Less,
            Self::Le => ordering != Greater,
            Self::Gt => ordering == Greater,
            Self::Ge => ordering != Less,
        }
    }
}

/// One operand of a `simple` condition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    /// A runtime expression.
    Expr(Expr),
    /// An integer literal.
    Integer(i64),
    /// `true` or `false`.
    Bool(bool),
    /// `null`, which tests presence.
    Null,
}

impl Operand {
    fn parse(token: &str, condition: &str) -> Result<Self, ParseError> {
        if token.starts_with('$') {
            return Expr::parse(token).map(Self::Expr);
        }
        if token.starts_with('\'') || token.starts_with('"') {
            return Err(ParseError::StringComparison(condition.to_owned()));
        }
        match token {
            "null" => Ok(Self::Null),
            "true" => Ok(Self::Bool(true)),
            "false" => Ok(Self::Bool(false)),
            number => number
                .parse::<i64>()
                .map(Self::Integer)
                .map_err(|_not_an_integer| ParseError::Condition(condition.to_owned())),
        }
    }
}

/// A `simple` condition: one comparison of two operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleCondition {
    /// The left operand.
    pub left: Operand,
    /// The operator.
    pub op: CmpOp,
    /// The right operand.
    pub right: Operand,
}

/// What a judged criterion found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judgement {
    /// Whether the criterion holds.
    pub holds: bool,
    /// What the criterion expects, in words.
    pub expected: String,
    /// What the exchange delivered, in words.
    pub received: String,
}

impl SimpleCondition {
    /// Parses `<operand> <op> <operand>`, the only `simple` shape the profile
    /// allows.
    ///
    /// # Errors
    /// [`ParseError`] for any other shape, and for a string literal.
    pub fn parse(condition: &str) -> Result<Self, ParseError> {
        let tokens: Vec<&str> = condition.split_whitespace().collect();
        let [left, op, right] = tokens.as_slice() else {
            if condition.contains('\'') || condition.contains('"') {
                return Err(ParseError::StringComparison(condition.to_owned()));
            }
            return Err(ParseError::Condition(condition.to_owned()));
        };
        let op = CmpOp::parse(op).ok_or_else(|| ParseError::Condition(condition.to_owned()))?;
        Ok(Self {
            left: Operand::parse(left, condition)?,
            op,
            right: Operand::parse(right, condition)?,
        })
    }

    /// Judges the condition in `scope`.
    ///
    /// # Errors
    /// [`EvalError`] when an operand cannot be read or the operand types are
    /// not comparable under the profile.
    pub fn judge(&self, scope: &Scope<'_>, text: &str) -> Result<Judgement, EvalError> {
        let left = resolve(&self.left, scope)?;
        let right = resolve(&self.right, scope)?;
        let holds = match (&self.right, &self.left) {
            (Operand::Null, _) | (_, Operand::Null) => {
                let present = |v: &Option<Value>| v.as_ref().is_some_and(|v| !v.is_null());
                let same = present(&left) == present(&right);
                match self.op {
                    CmpOp::Eq => same,
                    CmpOp::Ne => !same,
                    _ => {
                        return Err(EvalError::Incomparable(
                            text.to_owned(),
                            "null only supports `==` and `!=`".to_owned(),
                        ));
                    }
                }
            }
            _ => match (&left, &right) {
                (Some(Value::Number(a)), Some(Value::Number(b))) => {
                    let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) else {
                        return Err(EvalError::Incomparable(
                            text.to_owned(),
                            "only integers are compared".to_owned(),
                        ));
                    };
                    self.op.holds(a.cmp(&b))
                }
                (Some(Value::Bool(a)), Some(Value::Bool(b)))
                    if matches!(self.op, CmpOp::Eq | CmpOp::Ne) =>
                {
                    self.op.holds(a.cmp(b))
                }
                (None, _) | (_, None) => false,
                (a, b) => {
                    return Err(EvalError::Incomparable(
                        text.to_owned(),
                        format!("{} against {}", describe(a.as_ref()), describe(b.as_ref())),
                    ));
                }
            },
        };
        let (expected, received) = match (&self.left, self.op, &self.right) {
            (Operand::Expr(Expr::StatusCode), CmpOp::Eq, Operand::Integer(code)) => (
                format!("status {code}"),
                format!("status {}", describe(left.as_ref())),
            ),
            (Operand::Expr(expr), _, _) => (
                text.to_owned(),
                format!("{expr} is {}", describe(left.as_ref())),
            ),
            _ => (
                text.to_owned(),
                format!(
                    "{} against {}",
                    describe(left.as_ref()),
                    describe(right.as_ref())
                ),
            ),
        };
        Ok(Judgement {
            holds,
            expected,
            received,
        })
    }
}

fn resolve(operand: &Operand, scope: &Scope<'_>) -> Result<Option<Value>, EvalError> {
    Ok(match operand {
        Operand::Expr(expr) => scope.eval(expr)?,
        Operand::Integer(n) => Some(Value::from(*n)),
        Operand::Bool(b) => Some(Value::Bool(*b)),
        Operand::Null => None,
    })
}

/// A short rendering of a resolved value for an explanation.
fn describe(value: Option<&Value>) -> String {
    match value {
        None => "absent".to_owned(),
        Some(Value::String(text)) => format!("\"{text}\""),
        Some(other) => other.to_string(),
    }
}

/// A `regex` criterion: an ECMA-262-style pattern matched against a context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexCriterion {
    /// The context expression.
    pub context: Expr,
    /// The pattern, with embedded `{$…}` expressions still in place.
    pub pattern: String,
}

/// The text every embedded `{$…}` expression stands for while a template is
/// checked statically: inert in a regex, a `JSONPath` string and a `JSONPath`
/// number alike.
const PLACEHOLDER: &str = "0";

/// Checks every embedded `{$…}` expression of `template` parses, and returns
/// the template with each replaced by [`PLACEHOLDER`].
fn with_placeholders(template: &str) -> Result<String, ParseError> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{$") {
        let (before, tail) = rest.split_at(start);
        out.push_str(before);
        let inner_and_after = tail.get(1..).unwrap_or_default();
        let Some(end) = inner_and_after.find('}') else {
            return Err(ParseError::Unterminated(template.to_owned()));
        };
        let (inner, after) = inner_and_after.split_at(end);
        Expr::parse(inner)?;
        out.push_str(PLACEHOLDER);
        rest = after.get(1..).unwrap_or_default();
    }
    out.push_str(rest);
    Ok(out)
}

/// The text a context value is matched as: a string is its content, a number
/// or a boolean its JSON text (Arazzo 1.1.0 §Regex Conditions applies a
/// pattern to `$statusCode`); `None` for an object, an array or `null`.
fn context_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(_) | Value::Bool(_) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

impl RegexCriterion {
    /// Parses the context expression, checks the embedded expressions of the
    /// pattern parse, and checks the pattern stays in the profile's dialect.
    ///
    /// # Errors
    /// [`ParseError`] when the context or an embedded expression does not
    /// parse, or the pattern uses a construct the profile refuses.
    pub fn parse(context: &str, pattern: &str) -> Result<Self, ParseError> {
        let checked = with_placeholders(pattern)?;
        regex_profile::check(&checked).map_err(|construct| ParseError::Regex {
            pattern: pattern.to_owned(),
            construct,
        })?;
        Ok(Self {
            context: Expr::parse(context)?,
            pattern: pattern.to_owned(),
        })
    }

    /// Judges the criterion in `scope`.
    ///
    /// An absent or `null` context does not hold: the response was readable
    /// and did not carry the value (Arazzo 1.1.0 §Regex Conditions).
    ///
    /// # Errors
    /// [`EvalError`] when a value cannot be read, the context is an object or
    /// an array, or the substituted pattern does not compile.
    pub fn judge(&self, scope: &Scope<'_>) -> Result<Judgement, EvalError> {
        let pattern = scope.substitute(&self.pattern, regex::escape)?;
        let regex = regex_profile::compile(&pattern)
            .map_err(|error| EvalError::BadRegex(error.pattern, error.reason))?;
        let expected = format!("{} matches /{pattern}/", self.context);
        let context = scope.eval(&self.context)?;
        let (holds, received) = match &context {
            None | Some(Value::Null) => (false, format!("{} is absent", self.context)),
            Some(value) => {
                let Some(text) = context_text(value) else {
                    return Err(EvalError::NonStringContext(
                        self.context.to_string(),
                        value.to_string(),
                    ));
                };
                (
                    regex.is_match(&text),
                    format!("{} is {}", self.context, describe(Some(value))),
                )
            }
        };
        Ok(Judgement {
            holds,
            expected,
            received,
        })
    }
}

/// A `jsonpath` criterion: an RFC 9535 query applied to a context value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonPathCriterion {
    /// The context expression.
    pub context: Expr,
    /// The query, with embedded `{$…}` expressions still in place.
    pub query: String,
}

/// Escapes a value for the inside of an RFC 9535 string literal, single- or
/// double-quoted (RFC 9535 §2.3.1.1): quotes, the backslash and every control
/// character become `\uXXXX` or their short escapes.
#[must_use]
pub fn jsonpath_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\u0027"),
            '"' => out.push_str("\\u0022"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
}

impl JsonPathCriterion {
    /// Parses the context and checks the query is RFC 9535 once every embedded
    /// expression is replaced by an inert value.
    ///
    /// # Errors
    /// [`ParseError`] when the context or an embedded expression does not
    /// parse, or the query is not RFC 9535.
    pub fn parse(context: &str, query: &str) -> Result<Self, ParseError> {
        let checked = with_placeholders(query)?;
        serde_json_path::JsonPath::parse(&checked).map_err(|error| ParseError::JsonPath {
            query: query.to_owned(),
            reason: error.to_string(),
        })?;
        Ok(Self {
            context: Expr::parse(context)?,
            query: query.to_owned(),
        })
    }

    /// Judges the criterion in `scope`: it holds when the query's nodelist
    /// is not empty (Arazzo 1.1.0 §`JSONPath` Conditions).
    ///
    /// An absent or `null` context does not hold.
    ///
    /// # Errors
    /// [`EvalError`] when a value cannot be read or the substituted query
    /// does not parse.
    pub fn judge(&self, scope: &Scope<'_>) -> Result<Judgement, EvalError> {
        let query = scope.substitute(&self.query, jsonpath_escape)?;
        let path = serde_json_path::JsonPath::parse(&query)
            .map_err(|error| EvalError::BadJsonPath(query.clone(), error.to_string()))?;
        let expected = format!("{} selects at least one node by {query}", self.context);
        let (holds, received) = match scope.eval(&self.context)? {
            None | Some(Value::Null) => (false, format!("{} is absent", self.context)),
            Some(value) => {
                let nodes = path.query(&value).all();
                let received = match nodes.as_slice() {
                    [] => format!("{query} selects no node of {}", self.context),
                    [one] => format!("{query} selects {}", describe(Some(one))),
                    many => format!("{query} selects {} nodes", many.len()),
                };
                (!nodes.is_empty(), received)
            }
        };
        Ok(Judgement {
            holds,
            expected,
            received,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exchange(status: u16, body: &str) -> (RequestView, ResponseView) {
        let parsed = if body.is_empty() {
            Ok(None)
        } else {
            serde_json::from_str(body)
                .map(Some)
                .map_err(|e| e.to_string())
        };
        (
            RequestView::default(),
            ResponseView {
                status,
                headers: BTreeMap::from([("location".to_owned(), "https://x/ehr/1".to_owned())]),
                body: parsed,
            },
        )
    }

    fn judge_simple(condition: &str, status: u16, body: &str) -> Result<Judgement, EvalError> {
        let (request, response) = exchange(status, body);
        let inputs = BTreeMap::new();
        let outputs = BTreeMap::new();
        let scope = Scope {
            inputs: &inputs,
            outputs: &outputs,
            workflows: &outputs,
            exchange: Some((&request, &response)),
        };
        SimpleCondition::parse(condition)?.judge(&scope, condition)
    }

    #[test]
    fn expressions_parse_and_print_back() {
        for text in [
            "$statusCode",
            "$response.body#/ehr_id/value",
            "$response.body",
            "$response.header.Location",
            "$inputs.ehr_id",
            "$steps.create-it.outputs.ehr_id",
            "$request.path.ehr_id",
        ] {
            assert_eq!(Expr::parse(text).unwrap().to_string(), text);
        }
        for bad in [
            "$response.body#ehr_id",
            "$inputs.",
            "$steps.a.b",
            "$foo",
            "statusCode",
        ] {
            assert!(Expr::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_status_condition_holds_or_not_with_its_explanation() {
        let pass = judge_simple("$statusCode == 409", 409, "").unwrap();
        assert!(pass.holds);
        let fail = judge_simple("$statusCode == 409", 201, "").unwrap();
        assert!(!fail.holds);
        assert_eq!(fail.expected, "status 409");
        assert_eq!(fail.received, "status 201");
        assert!(judge_simple("$statusCode >= 400", 404, "").unwrap().holds);
        assert!(!judge_simple("$statusCode < 300", 404, "").unwrap().holds);
    }

    #[test]
    fn presence_is_tested_against_null() {
        let body = r#"{"ehr_id":{"value":"abc"}}"#;
        assert!(
            judge_simple("$response.body#/ehr_id/value != null", 201, body)
                .unwrap()
                .holds
        );
        assert!(
            !judge_simple("$response.body#/system_id != null", 201, body)
                .unwrap()
                .holds
        );
        assert!(
            judge_simple("$response.header.location != null", 201, body)
                .unwrap()
                .holds
        );
        assert!(
            judge_simple("$response.header.ETag == null", 201, body)
                .unwrap()
                .holds
        );
    }

    #[test]
    fn the_profile_refuses_simple_string_comparison() {
        assert!(matches!(
            SimpleCondition::parse("$response.body#/a == 'x'"),
            Err(ParseError::StringComparison(_))
        ));
        assert!(matches!(
            SimpleCondition::parse("$response.body#/a == 'two words'"),
            Err(ParseError::StringComparison(_))
        ));
        assert!(matches!(
            SimpleCondition::parse("$statusCode == 201 && $statusCode != 1"),
            Err(ParseError::Condition(_))
        ));
        assert!(matches!(
            SimpleCondition::parse("$statusCode == 2.5"),
            Err(ParseError::Condition(_))
        ));
        let body = r#"{"a":"x"}"#;
        assert!(matches!(
            judge_simple("$response.body#/a == 3", 200, body),
            Err(EvalError::Incomparable(..))
        ));
    }

    #[test]
    fn an_unreadable_body_is_an_evaluation_error_never_a_failure() {
        assert!(matches!(
            judge_simple("$response.body#/a != null", 200, "<xml/>"),
            Err(EvalError::UnreadableBody(_))
        ));
        assert!(matches!(
            SimpleCondition::parse("$statusCode == 200").unwrap().judge(
                &Scope {
                    inputs: &BTreeMap::new(),
                    outputs: &BTreeMap::new(),
                    workflows: &BTreeMap::new(),
                    exchange: None
                },
                "$statusCode == 200"
            ),
            Err(EvalError::NoExchange(_))
        ));
    }

    #[test]
    fn a_regex_criterion_substitutes_embedded_values_as_literals() {
        let (request, response) = exchange(200, r#"{"ehr_id":{"value":"a.b"}}"#);
        let inputs = BTreeMap::from([("id".to_owned(), Value::from("a.b"))]);
        let outputs = BTreeMap::new();
        let scope = Scope {
            inputs: &inputs,
            outputs: &outputs,
            workflows: &outputs,
            exchange: Some((&request, &response)),
        };
        let exact =
            RegexCriterion::parse("$response.body#/ehr_id/value", "^{$inputs.id}$").unwrap();
        assert!(exact.judge(&scope).unwrap().holds);

        let inputs = BTreeMap::from([("id".to_owned(), Value::from("aXb"))]);
        let scope = Scope {
            inputs: &inputs,
            ..scope
        };
        let judged = exact.judge(&scope).unwrap();
        assert!(!judged.holds, "the `.` of a substituted value is literal");
        assert_eq!(judged.received, "$response.body#/ehr_id/value is \"a.b\"");

        let absent = RegexCriterion::parse("$response.body#/nope", "^x$").unwrap();
        assert!(!absent.judge(&scope).unwrap().holds);
        let not_string = RegexCriterion::parse("$response.body#/ehr_id", "^x$").unwrap();
        assert!(matches!(
            not_string.judge(&scope),
            Err(EvalError::NonStringContext(..))
        ));
        assert!(RegexCriterion::parse("$response.body", "^{$inputs.id$").is_err());
        assert!(RegexCriterion::parse("$response.body", "^{$nope}$").is_err());
    }

    #[test]
    fn a_missing_input_is_an_evaluation_error() {
        let scope = Scope {
            inputs: &BTreeMap::new(),
            outputs: &BTreeMap::new(),
            workflows: &BTreeMap::new(),
            exchange: None,
        };
        assert_eq!(
            scope.eval(&Expr::Input("ehr_id".to_owned())),
            Err(EvalError::MissingInput("ehr_id".to_owned()))
        );
        let workflow = Expr::parse("$workflows.setup.outputs.ehr_id").unwrap();
        assert!(matches!(
            scope.eval(&workflow),
            Err(EvalError::MissingOutput(_))
        ));
    }

    fn judge_with<T>(
        body: &str,
        inputs: &[(&str, Value)],
        judge: impl FnOnce(&Scope<'_>) -> T,
    ) -> T {
        let (request, response) = exchange(200, body);
        let inputs: BTreeMap<String, Value> = inputs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect();
        let none = BTreeMap::new();
        let workflows = BTreeMap::from([(
            "setup".to_owned(),
            BTreeMap::from([("id".to_owned(), Value::from("w-1"))]),
        )]);
        judge(&Scope {
            inputs: &inputs,
            outputs: &none,
            workflows: &workflows,
            exchange: Some((&request, &response)),
        })
    }

    #[test]
    fn simple_compares_integers_booleans_and_null_only() {
        let body = r#"{"q":true,"n":3,"s":"3"}"#;
        assert!(
            judge_simple("$response.body#/q == true", 200, body)
                .unwrap()
                .holds
        );
        assert!(
            !judge_simple("$response.body#/q != true", 200, body)
                .unwrap()
                .holds
        );
        assert!(
            judge_simple("$response.body#/n >= 3", 200, body)
                .unwrap()
                .holds
        );
        assert!(
            !judge_simple("$response.body#/missing == 3", 200, body)
                .unwrap()
                .holds,
            "an absent value in a readable response does not hold"
        );
        assert!(
            matches!(
                judge_simple("$response.body#/s == 3", 200, body),
                Err(EvalError::Incomparable(..))
            ),
            "a numeric string is not coerced"
        );
        assert!(matches!(
            judge_simple("$response.body#/q < true", 200, body),
            Err(EvalError::Incomparable(..))
        ));
    }

    #[test]
    fn a_regex_outside_the_profile_is_refused_statically() {
        for pattern in ["^a(?=b)$", r"(a)\1", "(?i)abc", r"\Aabc"] {
            assert!(
                matches!(
                    RegexCriterion::parse("$response.body#/a", pattern),
                    Err(ParseError::Regex { .. })
                ),
                "{pattern}"
            );
        }
        assert!(RegexCriterion::parse("$response.body#/a", "^{$inputs.id}[0-9]{2}$").is_ok());
    }

    #[test]
    fn a_regex_reads_a_number_context_as_its_text() {
        let status = RegexCriterion::parse("$statusCode", r"^2\d{2}$").unwrap();
        assert!(judge_with("", &[], |s| status.judge(s)).unwrap().holds);
        let null = RegexCriterion::parse("$response.body#/a", "^x$").unwrap();
        assert!(
            !judge_with(r#"{"a":null}"#, &[], |s| null.judge(s))
                .unwrap()
                .holds,
            "a null context does not hold"
        );
    }

    #[test]
    fn a_jsonpath_criterion_holds_on_a_non_empty_nodelist() {
        let body = r#"{"ehr_id":{"value":"x'y"},"items":[1,2]}"#;
        let exact =
            JsonPathCriterion::parse("$response.body", "$.ehr_id[?@ == '{$inputs.id}']").unwrap();
        let holds = judge_with(body, &[("id", Value::from("x'y"))], |s| exact.judge(s)).unwrap();
        assert!(holds.holds, "{holds:?}");
        let other = judge_with(body, &[("id", Value::from("X'Y"))], |s| exact.judge(s)).unwrap();
        assert!(!other.holds, "RFC 9535 comparison is exact");
        let many = JsonPathCriterion::parse("$response.body", "$.items[*]").unwrap();
        let judged = judge_with(body, &[], |s| many.judge(s)).unwrap();
        assert!(judged.holds);
        assert_eq!(judged.received, "$.items[*] selects 2 nodes");
        let none = JsonPathCriterion::parse("$response.body", "$.missing").unwrap();
        assert!(!judge_with(body, &[], |s| none.judge(s)).unwrap().holds);
        let from_setup = JsonPathCriterion::parse(
            "$response.body",
            "$.ehr_id[?@ == '{$workflows.setup.outputs.id}']",
        )
        .unwrap();
        assert!(
            !judge_with(body, &[], |s| from_setup.judge(s))
                .unwrap()
                .holds
        );
    }

    #[test]
    fn a_jsonpath_context_that_is_absent_fails_and_an_unreadable_one_errors() {
        let query = JsonPathCriterion::parse("$response.body#/nope", "$.a").unwrap();
        assert!(
            !judge_with(r#"{"a":1}"#, &[], |s| query.judge(s))
                .unwrap()
                .holds
        );
        assert!(matches!(
            judge_with("not json", &[], |s| query.judge(s)),
            Err(EvalError::UnreadableBody(_))
        ));
    }

    #[test]
    fn a_query_that_is_not_rfc_9535_is_refused() {
        assert!(matches!(
            JsonPathCriterion::parse("$response.body", "$.a[?(@.b"),
            Err(ParseError::JsonPath { .. })
        ));
        assert!(matches!(
            JsonPathCriterion::parse("$response.body", "$..[?@.v == '{$nope}']"),
            Err(ParseError::Expression(_))
        ));
    }

    #[test]
    fn a_substituted_value_is_escaped_for_a_jsonpath_string() {
        assert_eq!(jsonpath_escape("a'b\"c\\d"), "a\\u0027b\\u0022c\\\\d");
        assert_eq!(jsonpath_escape("\x01"), "\\u0001");
    }
}
