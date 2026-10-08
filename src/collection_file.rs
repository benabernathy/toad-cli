//! The collection file as written in TOML. These types only describe the file format: serde
//! reads them and the JSON Schema is generated from them. `collection::Collection` turns them into
//! what a run uses.

use anyhow::{Result, anyhow};
use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::HashMap;

/// A parsed collection file. Nothing outside the file has been read or checked yet.
#[derive(Debug)]
pub struct CollectionFile {
    pub config: ConfigTable,
    pub vars: HashMap<String, String>,
    pub profiles: HashMap<String, HashMap<String, String>>,
    pub requests: IndexMap<String, RequestTable>,
}

/// The file as serde reads it. Requests are kept as raw tables so each one can be checked on its
/// own, and an error can name the request it came from.
#[derive(Deserialize)]
struct RawCollectionFile {
    #[serde(default)]
    config: ConfigTable,

    #[serde(default)]
    vars: HashMap<String, String>,

    #[serde(default)]
    profiles: HashMap<String, HashMap<String, String>>,

    #[serde(flatten)]
    requests: IndexMap<String, toml::Value>,
}

/// If `name` looks like a misspelling of one of the special tables, returns that table's name.
fn misspelled_section(name: &str) -> Option<&'static str> {
    ["config", "vars", "profiles"]
        .into_iter()
        .find(|section| edit_distance(&name.to_lowercase(), section) <= 2)
}

/// The candidate closest to `name`, ignoring case, if it is within two edits. Used for "did you
/// mean" hints.
pub fn closest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    candidates
        .into_iter()
        .map(|c| (edit_distance(&name.to_lowercase(), &c.to_lowercase()), c))
        .filter(|(distance, _)| *distance <= 2)
        .min()
        .map(|(_, c)| c)
}

/// Levenshtein distance between two strings.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

impl CollectionFile {
    /// Parses a collection file. Unknown settings are an error, so a misspelled check can't be
    /// skipped without anyone noticing.
    pub fn parse(content: &str) -> Result<CollectionFile> {
        let raw: RawCollectionFile = toml::from_str(content)?;

        let mut requests = IndexMap::with_capacity(raw.requests.len());
        for (name, value) in raw.requests {
            if !value.is_table() {
                return Err(anyhow!(
                    "'{}' is not a request - every top-level table is a request, and settings for \
                     every request go in [config]",
                    name
                ));
            }
            let request: RequestTable =
                value
                    .try_into()
                    .map_err(|e: toml::de::Error| match misspelled_section(&name) {
                        Some(section) => anyhow!(
                            "request '{}': {} (did you mean [{}]?)",
                            name,
                            e.message(),
                            section
                        ),
                        None => anyhow!("request '{}': {}", name, e.message()),
                    })?;
            requests.insert(name, request);
        }

        Ok(CollectionFile {
            config: raw.config,
            vars: raw.vars,
            profiles: raw.profiles,
            requests,
        })
    }
}

/// Settings that apply to every request in the collection. A request's own setting wins over
/// the one here, and `ignore_config` turns one off for a single request.
// The doc comments here and on `RequestTable` are the descriptions in `schema/toad.schema.json`,
// and the struct names are their names in its definitions.
#[derive(Debug, Deserialize, Default, Clone)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[cfg_attr(test, schemars(rename = "Config"))]
#[serde(deny_unknown_fields)]
pub struct ConfigTable {
    /// Skip TLS certificate verification. Only for test servers with self-signed certificates.
    #[serde(default)]
    pub ignore_ssl: bool,

    /// Path to a CA bundle (PEM, JKS, or PKCS12) to trust, relative to the collection file.
    /// The keystore password comes from `--use-custom-ca-password` or `TOAD_CA_PASSWORD`.
    #[serde(default)]
    pub use_custom_ca: Option<String>,

    /// Authorization shorthand for every request: "bearer <token>" or "basic <user>:<pass>",
    /// e.g. "bearer {{token}}".
    #[serde(default)]
    pub auth: Option<String>,

    /// Fail a request that takes longer than this many milliseconds.
    #[serde(default)]
    pub expect_max_ms: Option<u64>,

    /// Retry a failed request this many more times. `retry = 3` means up to 4 attempts.
    #[serde(default)]
    pub retry: Option<u32>,

    /// Milliseconds to wait between retries. Defaults to 1000.
    #[serde(default)]
    pub retry_delay_ms: Option<u64>,

    /// The requests to run, in this order. A request can be listed more than once, and requests
    /// that aren't listed don't run. Without it, every request runs in file order. Naming a
    /// request on the command line runs only that request.
    #[serde(default)]
    #[cfg_attr(test, schemars(length(min = 1)))]
    pub order: Option<Vec<String>>,
}

/// One request. Every top-level table other than `[config]`, `[vars]`, and `[profiles]` is a
/// request, and the table name is the request name.
#[derive(Debug, Deserialize, Clone)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[cfg_attr(test, schemars(rename = "RequestDef"))]
#[serde(deny_unknown_fields)]
pub struct RequestTable {
    /// HTTP method. Defaults to GET.
    #[serde(default = "default_method")]
    #[cfg_attr(test, schemars(schema_with = "crate::schema::method"))]
    pub method: String,

    /// URL to send the request to, e.g. "{{base_url}}/users/1".
    pub url: String,

    /// Request headers, e.g. { Accept = "application/json" }.
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// Query string parameters, added to the URL.
    #[serde(default)]
    pub query: HashMap<String, String>,

    /// Request body. Use either `body` or `body_file`, not both.
    pub body: Option<String>,

    /// Path to a file to send as the request body, relative to the collection file.
    pub body_file: Option<String>,

    /// When false, `body`/`body_file` is sent exactly as written, without `{{var}}` interpolation
    #[serde(default = "default_true")]
    pub interpolate_body: bool,

    /// Authorization shorthand: "bearer <token>" or "basic <user>:<pass>", e.g.
    /// "bearer {{token}}". Overrides `auth` in `[config]`.
    #[serde(default)]
    pub auth: Option<String>,

    /// Status codes that count as a pass. Without it, any response passes.
    // `default` keeps the schema from marking this required, since `schema_with` hides the Option
    #[serde(default)]
    #[cfg_attr(test, schemars(schema_with = "crate::schema::status_codes"))]
    pub expect_status: Option<Vec<u16>>,

    /// Fail the request if it takes longer than this many milliseconds. Overrides
    /// `expect_max_ms` in `[config]`.
    pub expect_max_ms: Option<u64>,

    /// Retry the request this many more times if it fails. Overrides `retry` in `[config]`.
    pub retry: Option<u32>,

    /// Milliseconds to wait between retries. Overrides `retry_delay_ms` in `[config]`.
    pub retry_delay_ms: Option<u64>,

    /// Seconds to wait for a response before giving up. Defaults to 30.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Values to read from the response into variables for later requests. Each value is a
    /// JSONPath query starting with "$", "header:<Name>", "status", or "body".
    #[serde(default)]
    #[cfg_attr(test, schemars(schema_with = "crate::schema::captures"))]
    pub capture: IndexMap<String, String>,

    /// Checks on the response. Each key is a JSONPath query starting with "$", "header:<Name>",
    /// "status", or "body". Each value is the expected value, or a table of checks: equals,
    /// matches, contains, starts_with, exists, type, length.
    #[serde(default)]
    #[cfg_attr(test, schemars(schema_with = "crate::schema::expect"))]
    pub expect: IndexMap<String, toml::Value>,

    /// `[config]` settings to ignore for this request.
    #[serde(default)]
    #[cfg_attr(test, schemars(schema_with = "crate::schema::config_keys"))]
    pub ignore_config: Vec<String>,
}

pub const CONFIG_KEYS: [&str; 6] = [
    "auth",
    "use_custom_ca",
    "ignore_ssl",
    "expect_max_ms",
    "retry",
    "retry_delay_ms",
];

fn default_method() -> String {
    "GET".to_string()
}
fn default_timeout() -> u64 {
    30
}
fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_request_key_names_the_request() {
        let err = CollectionFile::parse(
            r#"
            [get-user]
            url = "http://x"
            expect_stauts = [200]
            "#,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.starts_with("request 'get-user': unknown field `expect_stauts`"),
            "{msg}"
        );
    }

    #[test]
    fn unknown_request_subtable_is_an_error() {
        let err = CollectionFile::parse(
            r#"
            [get-user]
            url = "http://x"

            [get-user.headres]
            X-Test = "1"
            "#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown field `headres`"));
    }

    #[test]
    fn description_is_not_a_setting() {
        let err = CollectionFile::parse(
            r#"
            [get-user]
            url = "http://x"
            description = "Fetch a user"
            "#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown field `description`"));
    }

    #[test]
    fn unknown_config_key_is_an_error() {
        let err = CollectionFile::parse(
            r#"
            [config]
            retyr = 3
            "#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown field `retyr`"));
    }

    #[test]
    fn top_level_setting_is_an_error() {
        let err = CollectionFile::parse(
            r#"
            retry = 3

            [get-user]
            url = "http://x"
            "#,
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("settings for every request go in [config]")
        );
    }

    #[test]
    fn misspelled_config_table_gets_a_hint() {
        let err = CollectionFile::parse(
            r#"
            [confg]
            retry = 3
            "#,
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "request 'confg': missing field `url` (did you mean [config]?)"
        );
    }

    #[test]
    fn request_names_are_not_mistaken_for_sections() {
        assert_eq!(misspelled_section("get-user"), None);
        assert_eq!(misspelled_section("login"), None);
        assert_eq!(misspelled_section("Vars"), Some("vars"));
        assert_eq!(misspelled_section("profile"), Some("profiles"));
    }
}
