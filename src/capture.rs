use anyhow::{Result, anyhow};
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, HeaderName};
use serde_json::Value;
use serde_json_path::{ExactlyOneError, JsonPath};

/// Where in a response a value is read from, for a capture or an assertion.
#[derive(Debug, Clone)]
pub enum ResponseSource {
    JsonPath(JsonPath),
    Header(String),
    Status,
    Body,
}

impl ResponseSource {
    /// Parses a JSONPath query starting with `$`, `header:<Name>`, `status`, or `body`.
    pub fn parse(expr: &str) -> Result<Self> {
        if expr.starts_with('$') {
            let path = JsonPath::parse(expr)
                .map_err(|e| anyhow!("'{}' is not a valid JSONPath query: {}", expr, e))?;
            Ok(ResponseSource::JsonPath(path))
        } else if let Some(header) = expr.strip_prefix("header:") {
            let header = header.trim();
            HeaderName::from_bytes(header.as_bytes())
                .map_err(|_| anyhow!("'{}' is not a valid header name", header))?;
            Ok(ResponseSource::Header(header.to_string()))
        } else if expr == "status" {
            Ok(ResponseSource::Status)
        } else if expr == "body" {
            Ok(ResponseSource::Body)
        } else {
            Err(anyhow!(
                "'{}' must be a JSONPath query starting with '$', 'header:<Name>', 'status', or 'body'",
                expr
            ))
        }
    }
}

/// A parsed `[<request>.capture]` entry.
#[derive(Debug, Clone)]
pub struct Capture {
    pub name: String,
    pub expr: String,
    pub source: ResponseSource,
}

impl Capture {
    pub fn parse(name: &str, expr: &str) -> Result<Self> {
        if name.is_empty() || name.contains(|c: char| c.is_whitespace() || c == '{' || c == '}') {
            return Err(anyhow!(
                "'{}' is not a valid variable name (it can't be empty or contain whitespace or braces)",
                name
            ));
        }
        if name.starts_with(crate::interpolate::ENV_PREFIX) {
            return Err(crate::variables::reserved_name_error(&format!("'{name}'")));
        }

        let source = ResponseSource::parse(expr)?;

        Ok(Capture {
            name: name.to_string(),
            expr: expr.to_string(),
            source,
        })
    }

    /// Reads the captured value from a response. `json` is the parsed response body, or `None` if
    /// the body is not JSON.
    pub fn extract(
        &self,
        status: StatusCode,
        headers: &HeaderMap,
        body: &str,
        json: Option<&Value>,
    ) -> Result<String> {
        self.extract_inner(status, headers, body, json)
            .map_err(|e| anyhow!("capture '{}': {}", self.name, e))
    }

    fn extract_inner(
        &self,
        status: StatusCode,
        headers: &HeaderMap,
        body: &str,
        json: Option<&Value>,
    ) -> Result<String> {
        match &self.source {
            ResponseSource::JsonPath(path) => {
                let json = json.ok_or_else(|| anyhow!("response body is not JSON"))?;
                let value = path.query(json).exactly_one().map_err(|e| match e {
                    ExactlyOneError::Empty => anyhow!("no value matched '{}'", self.expr),
                    ExactlyOneError::MoreThanOne(n) => {
                        anyhow!("'{}' matched {} values, expected 1", self.expr, n)
                    }
                })?;
                value_to_string(value)
            }
            ResponseSource::Header(name) => {
                let value = headers
                    .get(name.as_str())
                    .ok_or_else(|| anyhow!("response has no '{}' header", name))?;
                value
                    .to_str()
                    .map(str::to_string)
                    .map_err(|_| anyhow!("response header '{}' is not valid text", name))
            }
            ResponseSource::Status => Ok(status.as_u16().to_string()),
            ResponseSource::Body => Ok(body.to_string()),
        }
    }
}

fn value_to_string(value: &Value) -> Result<String> {
    match value {
        Value::Null => Err(anyhow!("matched value is null")),
        Value::String(s) => Ok(s.clone()),
        other => Ok(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    fn run(expr: &str, body: &str) -> Result<String> {
        let capture = Capture::parse("v", expr)?;
        let json = serde_json::from_str::<Value>(body).ok();
        capture.extract(StatusCode::OK, &HeaderMap::new(), body, json.as_ref())
    }

    const USERS: &str = r#"{
        "id": 42,
        "name": "Toad",
        "active": true,
        "deleted_at": null,
        "address": {"city": "Austin", "zip": "78701"},
        "roles": ["reader", "editor"],
        "users": [
            {"id": 1, "role": "admin"},
            {"id": 2, "role": "user"},
            {"id": 3, "role": "user"}
        ]
    }"#;

    #[test]
    fn number_is_captured_as_text() {
        assert_eq!(run("$.id", USERS).unwrap(), "42");
    }

    #[test]
    fn string_is_captured_without_quotes() {
        assert_eq!(run("$.name", USERS).unwrap(), "Toad");
    }

    #[test]
    fn boolean_is_captured_as_text() {
        assert_eq!(run("$.active", USERS).unwrap(), "true");
    }

    #[test]
    fn object_and_array_are_captured_as_compact_json() {
        assert_eq!(
            run("$.address", USERS).unwrap(),
            r#"{"city":"Austin","zip":"78701"}"#
        );
        assert_eq!(run("$.roles", USERS).unwrap(), r#"["reader","editor"]"#);
    }

    #[test]
    fn filter_selects_one_item() {
        assert_eq!(run("$.users[?@.role == 'admin'].id", USERS).unwrap(), "1");
    }

    #[test]
    fn negative_index_selects_last_item() {
        assert_eq!(run("$.users[-1].id", USERS).unwrap(), "3");
    }

    #[test]
    fn null_is_an_error() {
        let err = run("$.deleted_at", USERS).unwrap_err();
        assert_eq!(err.to_string(), "capture 'v': matched value is null");
    }

    #[test]
    fn no_match_is_an_error() {
        let err = run("$.missing", USERS).unwrap_err();
        assert_eq!(err.to_string(), "capture 'v': no value matched '$.missing'");
    }

    #[test]
    fn multiple_matches_is_an_error() {
        let err = run("$.users[?@.role == 'user'].id", USERS).unwrap_err();
        assert!(err.to_string().contains("matched 2 values, expected 1"));
    }

    #[test]
    fn json_path_on_non_json_body_is_an_error() {
        let err = run("$.id", "not json").unwrap_err();
        assert!(err.to_string().contains("response body is not JSON"));
    }

    #[test]
    fn body_captures_raw_text() {
        assert_eq!(run("body", "plain text").unwrap(), "plain text");
    }

    #[test]
    fn status_is_captured() {
        let capture = Capture::parse("code", "status").unwrap();
        let value = capture
            .extract(StatusCode::CREATED, &HeaderMap::new(), "", None)
            .unwrap();
        assert_eq!(value, "201");
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let mut headers = HeaderMap::new();
        headers.insert("location", HeaderValue::from_static("/users/42"));
        let capture = Capture::parse("loc", "header:Location").unwrap();
        let value = capture
            .extract(StatusCode::CREATED, &headers, "", None)
            .unwrap();
        assert_eq!(value, "/users/42");
    }

    #[test]
    fn missing_header_is_an_error() {
        let capture = Capture::parse("loc", "header:Location").unwrap();
        let err = capture
            .extract(StatusCode::OK, &HeaderMap::new(), "", None)
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "capture 'loc': response has no 'Location' header"
        );
    }

    #[test]
    fn invalid_json_path_is_rejected() {
        assert!(Capture::parse("v", "$.users[?").is_err());
    }

    #[test]
    fn unknown_source_is_rejected() {
        let err = Capture::parse("v", "id").unwrap_err();
        assert!(err.to_string().contains("must be a JSONPath query"));
    }

    #[test]
    fn invalid_variable_name_is_rejected() {
        assert!(Capture::parse("user id", "$.id").is_err());
        assert!(Capture::parse("", "$.id").is_err());
    }
}
