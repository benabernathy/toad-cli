//! The collection as a run uses it. `Collection::load` reads the file, checks it, and resolves
//! everything that doesn't change while toad runs: `body_file` is read, captures are parsed,
//! paths are resolved, and each request's `[config]` defaults and `ignore_config` are applied.
//! Code that runs requests never sees `[config]` or `ignore_config`.

use anyhow::{Context, Result, anyhow};
use indexmap::IndexMap;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use crate::capture::Capture;
use crate::collection_file::{CONFIG_KEYS, CollectionFile, ConfigTable, RequestTable, closest};
use crate::interpolate::interpolate;
use crate::time_limit::TimeLimits;

/// A checked collection, ready to run.
#[derive(Debug)]
pub struct Collection {
    pub vars: HashMap<String, String>,
    pub profiles: HashMap<String, HashMap<String, String>>,
    /// Every request, in file order
    pub requests: IndexMap<String, Request>,
    /// `[config] order`. Every name is a request.
    pub order: Option<Vec<String>>,
}

/// One request, with its `[config]` defaults applied.
#[derive(Debug, Clone)]
pub struct Request {
    pub name: String,
    /// Uppercase
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub query: HashMap<String, String>,
    /// From `body` or the contents of `body_file`
    pub body: Option<Body>,
    /// The request's `auth`, or the one from `[config]`
    pub auth: Option<String>,
    pub expect_status: Option<Vec<u16>>,
    /// The request's `expect_max_ms`, or the one from `[config]`
    pub max_ms: Option<u64>,
    pub retry: RetrySettings,
    pub timeout_secs: u64,
    pub captures: Vec<Capture>,
    pub tls: Tls,
}

#[derive(Debug, Clone)]
pub struct Body {
    pub text: String,
    /// False when `interpolate_body = false`
    pub interpolate: bool,
}

/// Retry settings from the file. `--retry` and `TOAD_RETRY` are applied by `RetryPolicy` when the
/// request runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetrySettings {
    /// The request's own `retry`
    pub retry: Option<u32>,
    pub default: RetryDefault,
    /// The request's `retry_delay_ms`, or the one from `[config]`
    pub delay_ms: Option<u64>,
}

/// Where a request's default retry count comes from when it has no `retry` of its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RetryDefault {
    /// `[config] retry`, which `--retry N` replaces. `None` when it isn't set.
    Config(Option<u32>),
    /// The request has `ignore_config = ["retry"]`, so neither `[config] retry` nor
    /// `--retry N` applies.
    Ignored,
}

#[derive(Debug, Clone, Default)]
pub struct Tls {
    pub ignore_ssl: bool,
    /// `use_custom_ca`, resolved against the collection file's directory, or the path given
    /// with `--use-custom-ca`
    pub custom_ca: Option<String>,
}

impl Collection {
    /// Reads, parses, and checks a collection file.
    pub fn load(path: &Path) -> Result<Collection> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("could not read {}", path.display()))?;
        let file = CollectionFile::parse(&content)
            .with_context(|| format!("could not parse {}", path.display()))?;
        Collection::from_file(file, path.parent().unwrap_or(Path::new(".")))
    }

    /// Checks a parsed file and builds the collection. `base_dir` is where `body_file` and
    /// `use_custom_ca` paths are relative to.
    pub fn from_file(file: CollectionFile, base_dir: &Path) -> Result<Collection> {
        let bodies = read_bodies(&file, base_dir)?;
        let captures = parse_captures(&file)?;
        validate_ignore_config(&file)?;
        validate_names(&file)?;
        validate_expect_max_ms(&file)?;
        validate_order(&file)?;

        let custom_ca = file.config.use_custom_ca.as_deref().map(|path| {
            resolve_relative(path, base_dir)
                .to_string_lossy()
                .to_string()
        });

        let requests = file
            .requests
            .into_iter()
            .zip(bodies)
            .zip(captures)
            .map(|(((name, table), body), captures)| {
                let request = Request::build(
                    name.clone(),
                    table,
                    &file.config,
                    &custom_ca,
                    body,
                    captures,
                );
                (name, request)
            })
            .collect();

        Ok(Collection {
            vars: file.vars,
            profiles: file.profiles,
            requests,
            order: file.config.order,
        })
    }

    /// Parses and checks collection text, with relative paths resolved against the current
    /// directory.
    #[cfg(test)]
    pub fn parse(content: &str) -> Result<Collection> {
        Collection::from_file(CollectionFile::parse(content)?, Path::new("."))
    }

    /// The requests to run, in run order. A request named on the command line runs on its own.
    /// Otherwise `[config] order` is used if it is set, which can list a request more than once,
    /// and every request in file order if it isn't.
    pub fn requests_to_run(&self, name: Option<&str>) -> Result<Vec<&Request>> {
        match (name, &self.order) {
            (Some(n), _) => {
                let req = self.requests.get(n).ok_or_else(|| {
                    let available: Vec<&String> = self.requests.keys().collect();
                    anyhow!("no request named '{}'. Available: {:?}", n, available)
                })?;
                Ok(vec![req])
            }
            (None, Some(order)) => Ok(order.iter().map(|n| &self.requests[n]).collect()),
            (None, None) => Ok(self.requests.values().collect()),
        }
    }

    /// `--use-custom-ca` replaces every request's CA, including requests that ignore
    /// `[config] use_custom_ca`.
    pub fn set_custom_ca(&mut self, path: &Path) {
        for req in self.requests.values_mut() {
            req.tls.custom_ca = Some(path.to_string_lossy().to_string());
        }
    }

    /// The starting variables: `[vars]`, with the profile's values replacing them. `None` if
    /// there is no profile with that name.
    pub fn vars_with_profile(&self, profile: Option<&str>) -> Option<HashMap<String, String>> {
        let mut vars = self.vars.clone();
        if let Some(profile) = profile {
            vars.extend(self.profiles.get(profile)?.clone());
        }
        Some(vars)
    }
}

impl Request {
    fn build(
        name: String,
        table: RequestTable,
        config: &ConfigTable,
        custom_ca: &Option<String>,
        body: Option<String>,
        captures: Vec<Capture>,
    ) -> Request {
        let inherits = |key: &str| !table.ignore_config.iter().any(|k| k == key);
        let from_config = |key: &str, value: Option<u64>| value.filter(|_| inherits(key));

        Request {
            method: table.method.to_uppercase(),
            url: table.url,
            headers: table.headers,
            query: table.query,
            body: body.map(|text| Body {
                text,
                interpolate: table.interpolate_body,
            }),
            auth: table
                .auth
                .or_else(|| config.auth.clone().filter(|_| inherits("auth"))),
            expect_status: table.expect_status,
            max_ms: table
                .expect_max_ms
                .or(from_config("expect_max_ms", config.expect_max_ms)),
            retry: RetrySettings {
                retry: table.retry,
                default: if inherits("retry") {
                    RetryDefault::Config(config.retry)
                } else {
                    RetryDefault::Ignored
                },
                delay_ms: table
                    .retry_delay_ms
                    .or(from_config("retry_delay_ms", config.retry_delay_ms)),
            },
            timeout_secs: table.timeout_secs,
            captures,
            tls: Tls {
                ignore_ssl: config.ignore_ssl && inherits("ignore_ssl"),
                custom_ca: custom_ca.clone().filter(|_| inherits("use_custom_ca")),
            },
            name,
        }
    }

    /// The body as it will be sent: interpolated, unless `interpolate_body` is false.
    pub fn resolved_body(&self, vars: &HashMap<String, String>) -> Option<Result<String>> {
        self.body.as_ref().map(|body| {
            if body.interpolate {
                interpolate(&body.text, vars)
            } else {
                Ok(body.text.clone())
            }
        })
    }
}

fn resolve_relative(path: &str, base_dir: &Path) -> PathBuf {
    if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        base_dir.join(path)
    }
}

/// Each request's body, in file order, with `body_file` read.
fn read_bodies(file: &CollectionFile, base_dir: &Path) -> Result<Vec<Option<String>>> {
    file.requests
        .iter()
        .map(|(request_name, request)| match (&request.body, &request.body_file) {
            (Some(_), Some(_)) => Err(anyhow!(
                "request '{}' specifies both 'body' and 'body_file' - only one may be specified",
                request_name
            )),
            (None, Some(path)) => {
                let body_path = resolve_relative(path, base_dir);
                fs::read_to_string(body_path).map(Some).with_context(|| {
                    format!(
                        "could not read body_file '{}' in request '{}'",
                        path, request_name
                    )
                })
            }
            (body, None) => Ok(body.clone()),
        })
        .collect()
}

/// Each request's captures, in file order.
fn parse_captures(file: &CollectionFile) -> Result<Vec<Vec<Capture>>> {
    file.requests
        .iter()
        .map(|(request_name, request)| {
            request
                .capture
                .iter()
                .map(|(name, expr)| {
                    Capture::parse(name, expr).with_context(|| {
                        format!("invalid capture '{}' in request '{}'", name, request_name)
                    })
                })
                .collect()
        })
        .collect()
}

fn validate_ignore_config(file: &CollectionFile) -> Result<()> {
    for (request_name, request) in &file.requests {
        for key in &request.ignore_config {
            if !CONFIG_KEYS.contains(&key.as_str()) {
                return Err(anyhow!(
                    "unknown key '{}' in ignore_config for request '{}' (valid keys: {})",
                    key,
                    request_name,
                    CONFIG_KEYS.join(", ")
                ));
            }
        }
    }
    Ok(())
}

/// `{{env:NAME}}` always reads the environment, so a variable named `env:...` could never be
/// used.
fn validate_names(file: &CollectionFile) -> Result<()> {
    use crate::interpolate::ENV_PREFIX;
    use crate::variables::reserved_name_error;

    let reserved = |name: &String| name.starts_with(ENV_PREFIX);
    if let Some(name) = file.vars.keys().find(|n| reserved(n)) {
        return Err(reserved_name_error(&format!("[vars] '{name}'")));
    }
    for (profile, vars) in &file.profiles {
        if let Some(name) = vars.keys().find(|n| reserved(n)) {
            return Err(reserved_name_error(&format!(
                "[profiles.{profile}] '{name}'"
            )));
        }
    }
    Ok(())
}

fn validate_expect_max_ms(file: &CollectionFile) -> Result<()> {
    if file.config.expect_max_ms == Some(0) {
        return Err(anyhow!("[config] expect_max_ms must be greater than 0"));
    }
    for (request_name, request) in &file.requests {
        if request.expect_max_ms == Some(0) {
            return Err(anyhow!(
                "request '{}' has expect_max_ms = 0 - it must be greater than 0",
                request_name
            ));
        }
    }
    Ok(())
}

/// Checks that `[config] order` lists at least one request, and only requests in the collection.
fn validate_order(file: &CollectionFile) -> Result<()> {
    let Some(order) = &file.config.order else {
        return Ok(());
    };
    if order.is_empty() {
        return Err(anyhow!(
            "[config] order is empty. List the requests to run, or remove it to run every request in file order"
        ));
    }
    for name in order {
        if file.requests.contains_key(name) {
            continue;
        }
        let hint = closest(name, file.requests.keys().map(String::as_str))
            .map(|n| format!(" (did you mean '{n}'?)"))
            .unwrap_or_default();
        return Err(anyhow!("[config] order: no request named '{name}'{hint}"));
    }
    Ok(())
}

/// Warnings for time limits that can never trigger because the request's timeout is shorter.
pub fn time_limit_warnings(requests: &[&Request], limits: &TimeLimits) -> Vec<String> {
    // A request listed more than once in `order` is only warned about once
    let mut seen = HashSet::new();
    requests
        .iter()
        .filter(|req| seen.insert(&req.name))
        .filter_map(|req| {
            let max_ms = req.max_ms?;
            let effective = limits.effective(max_ms)?;
            (effective >= req.timeout_secs * 1000).then(|| {
                format!(
                    "warning: request '{}' expects at most {}, but timeout_secs = {} will stop it first",
                    req.name,
                    limits.describe(max_ms),
                    req.timeout_secs
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml_str: &str) -> Collection {
        Collection::parse(toml_str).unwrap()
    }

    fn parse_err(toml_str: &str) -> String {
        Collection::parse(toml_str).unwrap_err().to_string()
    }

    fn names(requests: &[&Request]) -> Vec<String> {
        requests.iter().map(|r| r.name.clone()).collect()
    }

    const FLOW: &str = r#"
        [login]
        url = "http://x/login"

        [create-user]
        url = "http://x/users"

        [get-user]
        url = "http://x/users/1"

        [delete-user]
        url = "http://x/users/1"
        "#;

    fn with_order(order: &str) -> String {
        format!("[config]\norder = {order}\n{FLOW}")
    }

    #[test]
    fn requests_run_in_file_order_by_default() {
        let collection = parse(FLOW);
        let requests = collection.requests_to_run(None).unwrap();
        assert_eq!(
            names(&requests),
            ["login", "create-user", "get-user", "delete-user"]
        );
    }

    #[test]
    fn order_sets_which_requests_run_and_when() {
        let collection = parse(&with_order(r#"["login", "get-user", "create-user"]"#));
        let requests = collection.requests_to_run(None).unwrap();
        assert_eq!(names(&requests), ["login", "get-user", "create-user"]);
    }

    #[test]
    fn order_can_repeat_a_request() {
        let collection = parse(&with_order(
            r#"["login", "create-user", "get-user", "delete-user", "get-user"]"#,
        ));
        let requests = collection.requests_to_run(None).unwrap();
        assert_eq!(
            names(&requests),
            [
                "login",
                "create-user",
                "get-user",
                "delete-user",
                "get-user"
            ]
        );
    }

    #[test]
    fn named_request_ignores_order() {
        let collection = parse(&with_order(r#"["login"]"#));
        let requests = collection.requests_to_run(Some("get-user")).unwrap();
        assert_eq!(names(&requests), ["get-user"]);
    }

    #[test]
    fn order_with_unknown_request_suggests_one() {
        assert_eq!(
            parse_err(&with_order(r#"["login", "get-usr"]"#)),
            "[config] order: no request named 'get-usr' (did you mean 'get-user'?)"
        );
    }

    #[test]
    fn empty_order_is_an_error() {
        assert!(parse_err(&with_order("[]")).starts_with("[config] order is empty"));
    }

    #[test]
    fn order_cannot_be_ignored_by_a_request() {
        let err = parse_err(
            r#"
            [r]
            url = "http://x"
            ignore_config = ["order"]
            "#,
        );
        assert!(
            err.starts_with("unknown key 'order' in ignore_config"),
            "{err}"
        );
    }

    const CONFIG: &str = r#"
        [config]
        ignore_ssl = true
        use_custom_ca = "ca.pem"
        auth = "bearer {{token}}"
        expect_max_ms = 500
        retry = 3
        retry_delay_ms = 250
        "#;

    #[test]
    fn requests_inherit_config() {
        let collection = parse(&format!("{CONFIG}\n[r]\nurl = \"http://x\""));
        let req = &collection.requests["r"];
        assert_eq!(req.auth.as_deref(), Some("bearer {{token}}"));
        assert!(req.tls.ignore_ssl);
        assert_eq!(req.tls.custom_ca.as_deref(), Some("./ca.pem"));
        assert_eq!(req.max_ms, Some(500));
        assert_eq!(
            req.retry,
            RetrySettings {
                retry: None,
                default: RetryDefault::Config(Some(3)),
                delay_ms: Some(250),
            }
        );
    }

    #[test]
    fn ignore_config_removes_config_settings() {
        let collection = parse(&format!(
            r#"{CONFIG}
            [r]
            url = "http://x"
            ignore_config = ["auth", "ignore_ssl", "use_custom_ca", "expect_max_ms", "retry", "retry_delay_ms"]
            "#
        ));
        let req = &collection.requests["r"];
        assert_eq!(req.auth, None);
        assert!(!req.tls.ignore_ssl);
        assert_eq!(req.tls.custom_ca, None);
        assert_eq!(req.max_ms, None);
        assert_eq!(
            req.retry,
            RetrySettings {
                retry: None,
                default: RetryDefault::Ignored,
                delay_ms: None,
            }
        );
    }

    #[test]
    fn request_settings_override_config() {
        let collection = parse(&format!(
            r#"{CONFIG}
            [r]
            url = "http://x"
            auth = "basic a:b"
            expect_max_ms = 300
            retry = 1
            retry_delay_ms = 10
            ignore_config = ["auth", "expect_max_ms", "retry_delay_ms"]
            "#
        ));
        let req = &collection.requests["r"];
        // ignore_config only removes the [config] value, so the request's own settings stay
        assert_eq!(req.auth.as_deref(), Some("basic a:b"));
        assert_eq!(req.max_ms, Some(300));
        assert_eq!(req.retry.retry, Some(1));
        assert_eq!(req.retry.delay_ms, Some(10));
    }

    #[test]
    fn custom_ca_override_applies_to_every_request() {
        let mut collection = parse(&format!(
            r#"{CONFIG}
            [ignores]
            url = "http://x"
            ignore_config = ["use_custom_ca"]
            "#
        ));
        collection.set_custom_ca(Path::new("/other/ca.pem"));
        assert_eq!(
            collection.requests["ignores"].tls.custom_ca.as_deref(),
            Some("/other/ca.pem")
        );
    }

    #[test]
    fn profile_values_replace_vars() {
        let collection = parse(
            r#"
            [vars]
            a = "1"
            b = "2"

            [profiles.ci]
            b = "3"
            "#,
        );
        let vars = collection.vars_with_profile(Some("ci")).unwrap();
        assert_eq!((vars["a"].as_str(), vars["b"].as_str()), ("1", "3"));
        assert!(collection.vars_with_profile(Some("nope")).is_none());
        assert_eq!(collection.vars_with_profile(None).unwrap()["b"], "2");
    }

    #[test]
    fn body_and_body_file_together_is_an_error() {
        let err = parse_err(
            r#"
            [r]
            url = "http://x"
            body = "{}"
            body_file = "body.json"
            "#,
        );
        assert!(
            err.contains("specifies both 'body' and 'body_file'"),
            "{err}"
        );
    }

    #[test]
    fn env_prefix_is_reserved_in_profiles() {
        let err = parse_err(
            r#"
            [profiles.ci]
            "env:TOKEN" = "x"
            "#,
        );
        assert!(
            err.starts_with("[profiles.ci] 'env:TOKEN': names starting with 'env:' are reserved"),
            "{err}"
        );
    }

    #[test]
    fn repeated_request_is_warned_about_once() {
        let collection = parse(
            r#"
            [config]
            order = ["slow", "slow"]

            [slow]
            url = "http://x"
            timeout_secs = 1
            expect_max_ms = 2000
            "#,
        );
        let requests = collection.requests_to_run(None).unwrap();
        let limits = TimeLimits::resolve(None, None);
        assert_eq!(time_limit_warnings(&requests, &limits).len(), 1);
    }

    #[test]
    fn zero_expect_max_ms_is_an_error() {
        let err = parse_err(
            r#"
            [a]
            url = "http://x"
            expect_max_ms = 0
            "#,
        );
        assert!(err.contains("must be greater than 0"));

        let err = parse_err(
            r#"
            [config]
            expect_max_ms = 0
            "#,
        );
        assert!(err.contains("must be greater than 0"));
    }

    #[test]
    fn warns_when_limit_cannot_trigger() {
        use crate::time_limit::TimeScale;

        let collection = parse(
            r#"
            [config]
            expect_max_ms = 15000

            [inherits]
            url = "http://x"

            [too-long]
            url = "http://x"
            expect_max_ms = 40000

            [ignored]
            url = "http://x"
            expect_max_ms = 40000
            ignore_config = ["expect_max_ms"]
            "#,
        );
        let requests = collection.requests_to_run(None).unwrap();

        let warnings = time_limit_warnings(&requests, &TimeLimits::default());
        assert_eq!(warnings.len(), 2);
        assert_eq!(
            warnings[0],
            "warning: request 'too-long' expects at most 40000ms, but timeout_secs = 30 will stop it first"
        );

        let scaled = TimeLimits::resolve(None, Some("3"));
        let warnings = time_limit_warnings(&requests, &scaled);
        assert_eq!(warnings.len(), 3);
        assert!(warnings[0].contains("45000ms (15000ms x 3 from TOAD_TIME_SCALE)"));

        let off = TimeLimits::resolve(Some(TimeScale::Off), None);
        assert!(time_limit_warnings(&requests, &off).is_empty());
    }

    #[test]
    fn unknown_ignore_config_key_is_an_error() {
        let err = parse_err(
            r#"
            [login]
            url = "http://x"
            ignore_config = ["auht"]
            "#,
        );
        assert!(err.contains("unknown key 'auht' in ignore_config"));
    }

    #[test]
    fn captures_are_parsed_in_order() {
        let collection = parse(
            r#"
            [create-user]
            url = "http://x"

            [create-user.capture]
            user_id = "$.id"
            location = "header:Location"
            "#,
        );
        let names: Vec<&str> = collection.requests["create-user"]
            .captures
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, ["user_id", "location"]);
    }

    #[test]
    fn invalid_capture_is_an_error() {
        let err = parse_err(
            r#"
            [create-user]
            url = "http://x"

            [create-user.capture]
            user_id = "id"
            "#,
        );
        assert_eq!(err, "invalid capture 'user_id' in request 'create-user'");
    }
}
