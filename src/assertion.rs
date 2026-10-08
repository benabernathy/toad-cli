//! `[<request>.expect]`: checks on the response body, headers, and status. Each key is a source,
//! the same as a capture's (`$...`, `header:<Name>`, `status`, `body`), and each value is either a
//! plain value, which means `equals`, or an inline table of checks.

use std::collections::HashMap;
use std::fmt;

use anyhow::{Context, Result, anyhow};
use regex::Regex;
use serde_json::{Number, Value};
use serde_json_path::JsonPath;

use crate::capture::ResponseSource;
use crate::collection_file::closest;
use crate::interpolate::{interpolate, references};
use crate::output::Received;

pub const CHECKS: [&str; 7] = [
    "equals",
    "matches",
    "contains",
    "starts_with",
    "exists",
    "type",
    "length",
];

pub const TYPES: [&str; 6] = ["string", "number", "boolean", "array", "object", "null"];

/// Longest rendering of an actual value in a failure message, in characters.
const MAX_SHOWN: usize = 100;

/// One key of `[<request>.expect]` and its checks.
#[derive(Debug, Clone)]
pub struct Assertion {
    pub expr: String,
    source: ResponseSource,
    checks: Vec<Check>,
}

#[derive(Debug, Clone)]
enum Check {
    Equals(Expected),
    Matches(Regex),
    Contains(Expected),
    StartsWith(String),
    Exists(bool),
    Type(String),
    Length(u64),
}

/// The value given to `equals` or `contains`.
#[derive(Debug, Clone)]
struct Expected {
    value: Value,
    /// The value is a string that uses `{{name}}`. It is compared with the actual value's text, the
    /// way a capture reads it, so `"{{user_id}}"` equals `42` when `user_id` was captured from it.
    as_text: bool,
}

/// One check that failed, with the expected and actual values.
#[derive(Debug, Clone)]
pub struct AssertionFailure {
    /// The key in `[<request>.expect]`, such as `$.id`
    pub expr: String,
    /// The check's name, such as `equals`
    pub check: &'static str,
    pub expected: Value,
    /// `None` when the source has no single value: nothing matched, more than one value matched,
    /// or the body is not JSON
    pub actual: Option<Value>,
    /// What went wrong, without the key: `expected 42, got 43`
    pub message: String,
}

/// The error for a response that failed one or more assertions. The executor reads the failures
/// from it to report each one.
#[derive(Debug)]
pub struct AssertionsFailed {
    pub request: String,
    pub failures: Vec<AssertionFailure>,
    /// Set when the request was retried
    pub attempts: Option<u32>,
}

impl fmt::Display for AssertionsFailed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let n = self.failures.len();
        write!(
            f,
            "request '{}' failed {} assertion{}",
            self.request,
            n,
            if n == 1 { "" } else { "s" }
        )?;
        if let Some(attempts) = self.attempts {
            write!(f, " (after {attempts} attempts)")?;
        }
        for failure in &self.failures {
            write!(f, "\n  {}: {}", failure.expr, failure.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for AssertionsFailed {}

/// What a source holds in a response.
enum Found {
    One(Value),
    Nothing,
    Many(usize),
}

impl Assertion {
    pub fn parse(expr: &str, value: &toml::Value) -> Result<Assertion> {
        let source = ResponseSource::parse(expr)?;
        let checks = match value {
            toml::Value::Table(table) if table.is_empty() => {
                return Err(anyhow!(
                    "no checks given (valid checks: {})",
                    CHECKS.join(", ")
                ));
            }
            toml::Value::Table(table) => table
                .iter()
                .map(|(name, value)| Check::parse(name, value))
                .collect::<Result<Vec<_>>>()?,
            plain => vec![Check::Equals(Expected::new(to_json(plain)))],
        };

        let exists = checks.iter().find_map(|c| match c {
            Check::Exists(e) => Some(*e),
            _ => None,
        });
        if exists.is_some() && matches!(source, ResponseSource::Status | ResponseSource::Body) {
            return Err(anyhow!(
                "'exists' only works with a JSONPath query or a header, because every response has a status and a body"
            ));
        }
        if exists == Some(false) && checks.len() > 1 {
            return Err(anyhow!(
                "'exists = false' can't be combined with other checks"
            ));
        }

        Ok(Assertion {
            expr: expr.to_string(),
            source,
            checks,
        })
    }

    /// The text in this assertion that is interpolated before the request is sent.
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        self.checks.iter().filter_map(|check| match check {
            Check::Equals(e) | Check::Contains(e) => e.value.as_str(),
            Check::StartsWith(s) => Some(s),
            _ => None,
        })
    }

    /// A copy with `{{name}}` filled in.
    pub fn with_vars(&self, vars: &HashMap<String, String>) -> Result<Assertion> {
        let fill =
            |s: &str| interpolate(s, vars).with_context(|| format!("in expect '{}'", self.expr));
        let checks = self
            .checks
            .iter()
            .map(|check| {
                Ok(match check {
                    Check::Equals(e) => Check::Equals(e.with_vars(fill)?),
                    Check::Contains(e) => Check::Contains(e.with_vars(fill)?),
                    Check::StartsWith(s) => Check::StartsWith(fill(s)?),
                    other => other.clone(),
                })
            })
            .collect::<Result<_>>()?;
        Ok(Assertion {
            expr: self.expr.clone(),
            source: self.source.clone(),
            checks,
        })
    }

    /// Runs every check against a response. `json` is the parsed body, or `None` if the body is
    /// not JSON. Returns the checks that failed.
    pub fn check(&self, received: &Received, json: Option<&Value>) -> Vec<AssertionFailure> {
        let found = self.find(received, json);
        self.checks
            .iter()
            .filter_map(|check| {
                let problem = match &found {
                    Ok(Found::One(actual)) => check.problem(actual),
                    Ok(Found::Nothing) => match check {
                        Check::Exists(false) => None,
                        _ => Some(self.nothing_found()),
                    },
                    Ok(Found::Many(n)) => match check {
                        Check::Exists(true) => None,
                        _ => Some(format!("but '{}' matched {n} values, not 1", self.expr)),
                    },
                    Err(problem) => Some(problem.clone()),
                }?;
                Some(AssertionFailure {
                    expr: self.expr.clone(),
                    check: check.name(),
                    expected: check.expected_value(),
                    actual: match &found {
                        Ok(Found::One(actual)) => Some(actual.clone()),
                        _ => None,
                    },
                    message: format!("expected {}, {}", check.describe(), problem),
                })
            })
            .collect()
    }

    fn find(&self, received: &Received, json: Option<&Value>) -> Result<Found, String> {
        match &self.source {
            ResponseSource::JsonPath(path) => {
                let json = json.ok_or("but the response body is not JSON")?;
                Ok(query(path, json))
            }
            ResponseSource::Header(name) => match received.headers.get(name.as_str()) {
                None => Ok(Found::Nothing),
                Some(value) => value
                    .to_str()
                    .map(|v| Found::One(Value::String(v.to_string())))
                    .map_err(|_| format!("but the '{name}' header is not valid text")),
            },
            ResponseSource::Status => Ok(Found::One(received.status.as_u16().into())),
            ResponseSource::Body => Ok(Found::One(Value::String(received.body.clone()))),
        }
    }

    fn nothing_found(&self) -> String {
        match &self.source {
            ResponseSource::Header(name) => format!("but the response has no '{name}' header"),
            _ => "but nothing matched".to_string(),
        }
    }
}

fn query(path: &JsonPath, json: &Value) -> Found {
    let nodes = path.query(json).all();
    match nodes.as_slice() {
        [] => Found::Nothing,
        [one] => Found::One((*one).clone()),
        many => Found::Many(many.len()),
    }
}

impl Check {
    fn parse(name: &str, value: &toml::Value) -> Result<Check> {
        let wrong = |what: &str| anyhow!("'{name}' must be {what}");
        match name {
            "equals" => Ok(Check::Equals(Expected::new(to_json(value)))),
            "contains" => Ok(Check::Contains(Expected::new(to_json(value)))),
            "matches" => {
                let pattern = value.as_str().ok_or_else(|| wrong("a string"))?;
                Regex::new(pattern)
                    .map(Check::Matches)
                    .map_err(|e| anyhow!("'{pattern}' is not a valid regular expression: {e}"))
            }
            "starts_with" => value
                .as_str()
                .map(|s| Check::StartsWith(s.to_string()))
                .ok_or_else(|| wrong("a string")),
            "exists" => value
                .as_bool()
                .map(Check::Exists)
                .ok_or_else(|| wrong("true or false")),
            "type" => match value.as_str() {
                Some(t) if TYPES.contains(&t) => Ok(Check::Type(t.to_string())),
                _ => Err(wrong(&format!("one of: {}", TYPES.join(", ")))),
            },
            "length" => value
                .as_integer()
                .and_then(|n| u64::try_from(n).ok())
                .map(Check::Length)
                .ok_or_else(|| wrong("a whole number, 0 or more")),
            _ => {
                let hint = closest(name, CHECKS)
                    .map(|c| format!(" (did you mean '{c}'?)"))
                    .unwrap_or_default();
                Err(anyhow!(
                    "unknown check '{name}'{hint}. Valid checks: {}. To compare with a table, write {{ equals = {{ ... }} }}",
                    CHECKS.join(", ")
                ))
            }
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Check::Equals(_) => "equals",
            Check::Matches(_) => "matches",
            Check::Contains(_) => "contains",
            Check::StartsWith(_) => "starts_with",
            Check::Exists(_) => "exists",
            Check::Type(_) => "type",
            Check::Length(_) => "length",
        }
    }

    /// The value the check was given, for JSON output.
    fn expected_value(&self) -> Value {
        match self {
            Check::Equals(e) | Check::Contains(e) => e.value.clone(),
            Check::Matches(re) => Value::String(re.as_str().to_string()),
            Check::StartsWith(s) | Check::Type(s) => Value::String(s.clone()),
            Check::Exists(e) => Value::Bool(*e),
            Check::Length(n) => (*n).into(),
        }
    }

    /// What the check expects, to follow "expected" in a failure message.
    fn describe(&self) -> String {
        match self {
            Check::Equals(e) => show(&e.value),
            // As written, so it can be copied back into the file. JSON would double each backslash.
            Check::Matches(re) => format!("a match for \"{}\"", re.as_str()),
            Check::Contains(e) => format!("a value containing {}", show(&e.value)),
            Check::StartsWith(s) => format!("text starting with {}", show(&s.as_str().into())),
            Check::Exists(true) => "a value".to_string(),
            Check::Exists(false) => "no value".to_string(),
            Check::Type(t) => format!("type {t}"),
            Check::Length(n) => format!("length {n}"),
        }
    }

    /// Why `actual` fails the check, to follow "expected ..." in a failure message. `None` if it
    /// passes.
    fn problem(&self, actual: &Value) -> Option<String> {
        let got = || format!("got {}", show(actual));
        match self {
            Check::Equals(e) => (!e.matches(actual)).then(got),
            Check::Matches(re) => match actual.as_str() {
                Some(s) => (!re.is_match(s)).then(got),
                None => Some(format!("{}, which is not a string", got())),
            },
            Check::Contains(e) => match actual {
                Value::String(s) => (!s.contains(&text(&e.value))).then(got),
                Value::Array(items) => (!items.iter().any(|item| e.matches(item))).then(got),
                _ => Some(format!("{}, which is not a string or an array", got())),
            },
            Check::StartsWith(prefix) => match actual.as_str() {
                Some(s) => (!s.starts_with(prefix.as_str())).then(got),
                None => Some(format!("{}, which is not a string", got())),
            },
            Check::Exists(true) => None,
            Check::Exists(false) => Some(got()),
            Check::Type(t) => {
                let actual_type = type_name(actual);
                (actual_type != t).then(|| format!("got {actual_type} {}", show(actual)))
            }
            Check::Length(n) => {
                let length = match actual {
                    Value::String(s) => s.chars().count(),
                    Value::Array(items) => items.len(),
                    Value::Object(fields) => fields.len(),
                    _ => return Some(format!("{}, which has no length", got())),
                };
                (length as u64 != *n).then(|| format!("got length {length}"))
            }
        }
    }
}

impl Expected {
    fn new(value: Value) -> Expected {
        let as_text = value.as_str().is_some_and(|s| !references(s).is_empty());
        Expected { value, as_text }
    }

    fn with_vars(&self, fill: impl Fn(&str) -> Result<String>) -> Result<Expected> {
        let value = match &self.value {
            Value::String(s) => Value::String(fill(s)?),
            other => other.clone(),
        };
        Ok(Expected {
            value,
            as_text: self.as_text,
        })
    }

    fn matches(&self, actual: &Value) -> bool {
        if self.as_text {
            text(actual) == text(&self.value)
        } else {
            json_eq(&self.value, actual)
        }
    }
}

/// Equality where numbers are compared by value, so `42` equals `42.0`.
fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => numbers_eq(x, y),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| json_eq(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, a)| y.get(k).is_some_and(|b| json_eq(a, b)))
        }
        _ => a == b,
    }
}

fn numbers_eq(x: &Number, y: &Number) -> bool {
    match (x.as_i64(), y.as_i64()) {
        (Some(a), Some(b)) => a == b,
        _ => match (x.as_u64(), y.as_u64()) {
            (Some(a), Some(b)) => a == b,
            _ => x.as_f64() == y.as_f64(),
        },
    }
}

/// A value as text, the way a capture reads it: strings without quotes, everything else as
/// compact JSON.
fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// A value as compact JSON for a failure message, shortened if it is long.
fn show(value: &Value) -> String {
    let json = value.to_string();
    if json.chars().count() <= MAX_SHOWN {
        return json;
    }
    let short: String = json.chars().take(MAX_SHOWN).collect();
    format!("{short}...")
}

/// TOML has no null, and a date or time is compared as the text it was written as.
fn to_json(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(s) => Value::String(s.clone()),
        toml::Value::Integer(n) => (*n).into(),
        toml::Value::Float(f) => Number::from_f64(*f).map_or(Value::Null, Value::Number),
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(items) => Value::Array(items.iter().map(to_json).collect()),
        toml::Value::Table(table) => {
            Value::Object(table.iter().map(|(k, v)| (k.clone(), to_json(v))).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;
    use reqwest::header::{HeaderMap, HeaderValue};
    use std::time::Duration;

    const USER: &str = r#"{
        "id": 42,
        "price": 9.5,
        "name": "Toad",
        "email": "toad@example.com",
        "active": true,
        "deleted_at": null,
        "roles": ["reader", "editor"],
        "address": {"city": "Austin", "zip": "78701"},
        "users": [{"id": 1}, {"id": 2}]
    }"#;

    fn received(body: &str) -> Received {
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        Received {
            status: StatusCode::OK,
            headers,
            body: body.to_string(),
            elapsed: Duration::from_millis(1),
        }
    }

    /// Parses one `[r.expect]` line, such as `"$.id" = 42`, and checks it against `body`.
    fn check_with(line: &str, body: &str, vars: &[(&str, &str)]) -> Vec<String> {
        let table: toml::Table = toml::from_str(line).unwrap();
        let (expr, value) = table.iter().next().unwrap();
        let vars = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let assertion = Assertion::parse(expr, value)
            .unwrap()
            .with_vars(&vars)
            .unwrap();
        let json = serde_json::from_str::<Value>(body).ok();
        assertion
            .check(&received(body), json.as_ref())
            .into_iter()
            .map(|f| format!("{}: {}", f.expr, f.message))
            .collect()
    }

    fn check(line: &str) -> Vec<String> {
        check_with(line, USER, &[])
    }

    fn passes(line: &str) {
        let failures = check(line);
        assert!(failures.is_empty(), "{line}: {failures:?}");
    }

    fn fails(line: &str, message: &str) {
        assert_eq!(check(line), [message], "{line}");
    }

    fn parse_err(line: &str) -> String {
        let table: toml::Table = toml::from_str(line).unwrap();
        let (expr, value) = table.iter().next().unwrap();
        Assertion::parse(expr, value).unwrap_err().to_string()
    }

    #[test]
    fn plain_value_means_equals() {
        passes(r#""$.id" = 42"#);
        passes(r#""$.name" = "Toad""#);
        passes(r#""$.active" = true"#);
        passes(r#""$.roles" = ["reader", "editor"]"#);
        passes(r#""$.address" = { equals = { zip = "78701", city = "Austin" } }"#);
        fails(r#""$.id" = 43"#, "$.id: expected 43, got 42");
    }

    #[test]
    fn numbers_are_compared_by_value() {
        passes(r#""$.id" = 42.0"#);
        passes(r#""$.price" = 9.5"#);
    }

    #[test]
    fn equals_does_not_convert_types() {
        fails(r#""$.id" = "42""#, r#"$.id: expected "42", got 42"#);
        fails(
            r#""$.active" = "true""#,
            r#"$.active: expected "true", got true"#,
        );
    }

    #[test]
    fn interpolated_strings_are_compared_as_text() {
        let line = r#""$.id" = "{{user_id}}""#;
        assert!(check_with(line, USER, &[("user_id", "42")]).is_empty());
        assert_eq!(
            check_with(line, USER, &[("user_id", "7")]),
            [r#"$.id: expected "7", got 42"#]
        );
        assert!(check_with(r#""$.name" = "{{n}}""#, USER, &[("n", "Toad")]).is_empty());
    }

    #[test]
    fn matches() {
        passes(r#""$.email" = { matches = ".+@.+" }"#);
        fails(
            r#""$.name" = { matches = "^T.d$" }"#,
            r#"$.name: expected a match for "^T.d$", got "Toad""#,
        );
        fails(
            r#""$.id" = { matches = "4" }"#,
            r#"$.id: expected a match for "4", got 42, which is not a string"#,
        );
    }

    #[test]
    fn contains() {
        passes(r#""$.roles" = { contains = "editor" }"#);
        passes(r#""$.email" = { contains = "@example" }"#);
        passes(r#""$.users" = { contains = { id = 2 } }"#);
        fails(
            r#""$.roles" = { contains = "admin" }"#,
            r#"$.roles: expected a value containing "admin", got ["reader","editor"]"#,
        );
        fails(
            r#""$.address" = { contains = "Austin" }"#,
            r#"$.address: expected a value containing "Austin", got {"city":"Austin","zip":"78701"}, which is not a string or an array"#,
        );
    }

    #[test]
    fn starts_with_reads_headers() {
        let failures = check(r#""header:Content-Type" = { starts_with = "application/json" }"#);
        assert!(failures.is_empty(), "{failures:?}");
        fails(
            r#""header:content-type" = { starts_with = "text/" }"#,
            r#"header:content-type: expected text starting with "text/", got "application/json; charset=utf-8""#,
        );
    }

    #[test]
    fn exists() {
        passes(r#""$.deleted_at" = { exists = true }"#);
        passes(r#""$.missing" = { exists = false }"#);
        passes(r#""$.users[*].id" = { exists = true }"#);
        passes(r#""header:X-Missing" = { exists = false }"#);
        fails(
            r#""$.deleted_at" = { exists = false }"#,
            "$.deleted_at: expected no value, got null",
        );
        fails(
            r#""$.missing" = { exists = true }"#,
            "$.missing: expected a value, but nothing matched",
        );
        fails(
            r#""header:X-Missing" = { exists = true }"#,
            "header:X-Missing: expected a value, but the response has no 'X-Missing' header",
        );
    }

    #[test]
    fn type_and_length() {
        passes(r#""$.roles" = { type = "array", length = 2 }"#);
        passes(r#""$.deleted_at" = { type = "null" }"#);
        passes(r#""$.name" = { length = 4 }"#);
        passes(r#""$.address" = { length = 2 }"#);
        fails(
            r#""$.id" = { type = "string" }"#,
            "$.id: expected type string, got number 42",
        );
        fails(
            r#""$.roles" = { length = 3 }"#,
            "$.roles: expected length 3, got length 2",
        );
        fails(
            r#""$.id" = { length = 2 }"#,
            "$.id: expected length 2, got 42, which has no length",
        );
    }

    #[test]
    fn every_failed_check_is_reported() {
        assert_eq!(
            check(r#""$.name" = { type = "number", starts_with = "X" }"#),
            [
                r#"$.name: expected type number, got string "Toad""#,
                r#"$.name: expected text starting with "X", got "Toad""#
            ]
        );
    }

    #[test]
    fn missing_or_ambiguous_values_fail() {
        fails(
            r#""$.missing" = 1"#,
            "$.missing: expected 1, but nothing matched",
        );
        fails(
            r#""$.users[*].id" = 1"#,
            "$.users[*].id: expected 1, but '$.users[*].id' matched 2 values, not 1",
        );
        assert_eq!(
            check_with(r#""$.id" = 1"#, "not json", &[]),
            ["$.id: expected 1, but the response body is not JSON"]
        );
    }

    #[test]
    fn status_and_body() {
        passes(r#""status" = 200"#);
        passes(r#""body" = { contains = "Toad" }"#);
        fails(r#""status" = 201"#, "status: expected 201, got 200");
    }

    #[test]
    fn long_values_are_shortened() {
        let body = format!(r#"{{"text": "{}"}}"#, "x".repeat(500));
        let failures = check_with(r#""$.text" = "y""#, &body, &[]);
        assert!(failures[0].ends_with("xxx..."), "{failures:?}");
        assert!(failures[0].len() < 150, "{failures:?}");
    }

    #[test]
    fn invalid_assertions_are_rejected() {
        assert!(
            parse_err(r#""$.a" = { equal = 1 }"#)
                .starts_with("unknown check 'equal' (did you mean 'equals'?)")
        );
        assert!(parse_err(r#""$.a" = { city = "Austin" }"#).contains("write { equals = { ... } }"));
        assert!(parse_err(r#""$.a" = {}"#).starts_with("no checks given"));
        assert!(
            parse_err(r#""$.a" = { matches = "(" }"#).contains("not a valid regular expression")
        );
        assert_eq!(
            parse_err(r#""$.a" = { matches = 1 }"#),
            "'matches' must be a string"
        );
        assert_eq!(
            parse_err(r#""$.a" = { type = "int" }"#),
            "'type' must be one of: string, number, boolean, array, object, null"
        );
        assert_eq!(
            parse_err(r#""$.a" = { length = -1 }"#),
            "'length' must be a whole number, 0 or more"
        );
        assert!(parse_err(r#""status" = { exists = true }"#).starts_with("'exists' only works"));
        assert_eq!(
            parse_err(r#""$.a" = { exists = false, equals = 1 }"#),
            "'exists = false' can't be combined with other checks"
        );
        assert!(parse_err(r#""id" = 1"#).contains("must be a JSONPath query"));
    }

    #[test]
    fn failure_message_lists_each_failure() {
        let failure = |expr: &str, message: &str| AssertionFailure {
            expr: expr.to_string(),
            check: "equals",
            expected: Value::Null,
            actual: None,
            message: message.to_string(),
        };
        let mut err = AssertionsFailed {
            request: "get-user".to_string(),
            failures: vec![failure("$.id", "expected 1, got 2")],
            attempts: None,
        };
        assert_eq!(
            err.to_string(),
            "request 'get-user' failed 1 assertion\n  $.id: expected 1, got 2"
        );
        err.failures
            .push(failure("$.name", r#"expected "a", got "b""#));
        err.attempts = Some(3);
        assert_eq!(
            err.to_string(),
            "request 'get-user' failed 2 assertions (after 3 attempts)\n  $.id: expected 1, got 2\n  $.name: expected \"a\", got \"b\""
        );
    }
}
