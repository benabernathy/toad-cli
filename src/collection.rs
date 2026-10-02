use crate::capture::Capture;
use crate::interpolate::interpolate;
use crate::time_limit::TimeLimits;
use anyhow::{Context, Result, anyhow};
use indexmap::IndexMap;
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
pub struct RequestFile {
    #[serde(default)]
    pub config: Config,

    #[serde(default)]
    pub vars: HashMap<String, String>,

    #[serde(default)]
    pub profiles: HashMap<String, HashMap<String, String>>,

    #[serde(flatten)]
    pub requests: IndexMap<String, RequestDef>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Config {
    #[serde(default)]
    pub ignore_ssl: bool,

    #[serde(default)]
    pub use_custom_ca: Option<String>,

    #[serde(default)]
    pub auth: Option<String>,

    #[serde(default)]
    pub expect_max_ms: Option<u64>,

    #[serde(default)]
    pub retry: Option<u32>,

    #[serde(default)]
    pub retry_delay_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RequestDef {
    #[serde(default = "default_method")]
    pub method: String,

    pub url: String,

    #[serde(default)]
    pub headers: HashMap<String, String>,

    #[serde(default)]
    pub query: HashMap<String, String>,

    pub body: Option<String>,

    pub body_file: Option<String>,

    /// When false, `body`/`body_file` is sent exactly as written, without `{{var}}` interpolation
    #[serde(default = "default_true")]
    pub interpolate_body: bool,

    #[serde(default)]
    pub auth: Option<String>,

    pub expect_status: Option<Vec<u16>>,

    pub expect_max_ms: Option<u64>,

    pub retry: Option<u32>,

    pub retry_delay_ms: Option<u64>,

    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    #[serde(default)]
    pub capture: IndexMap<String, String>,

    #[serde(default)]
    pub ignore_config: Vec<String>,

    /// Parsed from `capture` by `parse_captures`.
    #[serde(skip)]
    pub captures: Vec<Capture>,
}

impl RequestDef {
    /// The body as it will be sent: interpolated, unless `interpolate_body` is false.
    pub fn resolved_body(&self, vars: &HashMap<String, String>) -> Option<Result<String>> {
        self.body.as_ref().map(|body| {
            if self.interpolate_body {
                interpolate(body, vars)
            } else {
                Ok(body.clone())
            }
        })
    }

    /// The `expect_max_ms` for this request: its own value, or the default from `config`.
    pub fn max_ms(&self, config: &Config) -> Option<u64> {
        self.expect_max_ms.or(config.expect_max_ms)
    }
}

const CONFIG_KEYS: [&str; 6] = [
    "auth",
    "use_custom_ca",
    "ignore_ssl",
    "expect_max_ms",
    "retry",
    "retry_delay_ms",
];

impl Config {
    /// Returns a copy of this config with the listed keys reset to their defaults.
    pub fn without(&self, ignored: &[String]) -> Config {
        let mut config = self.clone();
        for key in ignored {
            match key.as_str() {
                "auth" => config.auth = None,
                "use_custom_ca" => config.use_custom_ca = None,
                "ignore_ssl" => config.ignore_ssl = false,
                "expect_max_ms" => config.expect_max_ms = None,
                "retry" => config.retry = None,
                "retry_delay_ms" => config.retry_delay_ms = None,
                _ => {}
            }
        }
        config
    }
}

fn default_method() -> String {
    "GET".to_string()
}
fn default_timeout() -> u64 {
    30
}
fn default_true() -> bool {
    true
}

fn resolve_relative(path: &str, base_dir: &Path) -> PathBuf {
    if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        base_dir.join(path)
    }
}

pub fn load_ext_body(rf: &mut RequestFile, request_file_path: &Path) -> Result<()> {
    let base_dir = request_file_path.parent().unwrap_or(Path::new("."));

    for (request_name, request) in &mut rf.requests {
        match (&request.body, &request.body_file) {
            (Some(_), Some(_)) => {
                return Err(anyhow!(
                    "request '{}' specifies both 'body' and 'body_file' - only one may be specified",
                    request_name
                ));
            }
            (None, Some(path)) => {
                let body_path = resolve_relative(path, base_dir);

                let content = fs::read_to_string(body_path).with_context(|| {
                    format!(
                        "could not read body_file '{}' in request '{}'",
                        path, request_name
                    )
                })?;
                request.body = Some(content);
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn parse_captures(rf: &mut RequestFile) -> Result<()> {
    for (request_name, request) in &mut rf.requests {
        request.captures = request
            .capture
            .iter()
            .map(|(name, expr)| {
                Capture::parse(name, expr).with_context(|| {
                    format!("invalid capture '{}' in request '{}'", name, request_name)
                })
            })
            .collect::<Result<_>>()?;
    }
    Ok(())
}

pub fn validate_ignore_config(rf: &RequestFile) -> Result<()> {
    for (request_name, request) in &rf.requests {
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

pub fn validate_expect_max_ms(rf: &RequestFile) -> Result<()> {
    if rf.config.expect_max_ms == Some(0) {
        return Err(anyhow!("[config] expect_max_ms must be greater than 0"));
    }
    for (request_name, request) in &rf.requests {
        if request.expect_max_ms == Some(0) {
            return Err(anyhow!(
                "request '{}' has expect_max_ms = 0 - it must be greater than 0",
                request_name
            ));
        }
    }
    Ok(())
}

/// Warnings for time limits that can never trigger because the request's timeout is shorter.
pub fn time_limit_warnings(
    requests: &[(String, RequestDef)],
    config: &Config,
    limits: &TimeLimits,
) -> Vec<String> {
    requests
        .iter()
        .filter_map(|(name, req)| {
            let max_ms = req.max_ms(&config.without(&req.ignore_config))?;
            let effective = limits.effective(max_ms)?;
            (effective >= req.timeout_secs * 1000).then(|| {
                format!(
                    "warning: request '{}' expects at most {}, but timeout_secs = {} will stop it first",
                    name,
                    limits.describe(max_ms),
                    req.timeout_secs
                )
            })
        })
        .collect()
}

pub fn resolve_custom_ca(config: &mut Config, request_file_path: &Path) {
    let base_dir = request_file_path.parent().unwrap_or(Path::new("."));

    if let Some(path) = &config.use_custom_ca {
        config.use_custom_ca = Some(
            resolve_relative(path, base_dir)
                .to_string_lossy()
                .to_string(),
        );
    }
}

pub fn load_requests(rf: &RequestFile, name: Option<&str>) -> Result<Vec<(String, RequestDef)>> {
    match name {
        Some(n) => {
            let req = rf.requests.get(n).cloned().ok_or_else(|| {
                let available: Vec<&String> = rf.requests.keys().collect();
                anyhow!("no request named '{}'. Available: {:?}", n, available)
            })?;
            Ok(vec![(n.to_string(), req)])
        }
        None => Ok(rf
            .requests
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml_str: &str) -> RequestFile {
        toml::from_str(toml_str).unwrap()
    }

    #[test]
    fn without_resets_ignored_keys() {
        let config = Config {
            ignore_ssl: true,
            use_custom_ca: Some("ca.pem".to_string()),
            auth: Some("bearer {{token}}".to_string()),
            expect_max_ms: Some(500),
            retry: Some(3),
            retry_delay_ms: Some(250),
        };
        let config = config.without(&[
            "auth".to_string(),
            "ignore_ssl".to_string(),
            "expect_max_ms".to_string(),
            "retry".to_string(),
            "retry_delay_ms".to_string(),
        ]);
        assert_eq!(config.auth, None);
        assert!(!config.ignore_ssl);
        assert_eq!(config.expect_max_ms, None);
        assert_eq!(config.retry, None);
        assert_eq!(config.retry_delay_ms, None);
        assert_eq!(config.use_custom_ca.as_deref(), Some("ca.pem"));
    }

    #[test]
    fn request_max_ms_overrides_config() {
        let rf = parse(
            r#"
            [config]
            expect_max_ms = 1000

            [default]
            url = "http://x"

            [tight]
            url = "http://x"
            expect_max_ms = 300
            "#,
        );
        assert_eq!(rf.requests["default"].max_ms(&rf.config), Some(1000));
        assert_eq!(rf.requests["tight"].max_ms(&rf.config), Some(300));
    }

    #[test]
    fn zero_expect_max_ms_is_an_error() {
        let rf = parse(
            r#"
            [a]
            url = "http://x"
            expect_max_ms = 0
            "#,
        );
        let err = validate_expect_max_ms(&rf).unwrap_err();
        assert!(err.to_string().contains("must be greater than 0"));

        let rf = parse(
            r#"
            [config]
            expect_max_ms = 0
            "#,
        );
        assert!(validate_expect_max_ms(&rf).is_err());
    }

    #[test]
    fn warns_when_limit_cannot_trigger() {
        use crate::time_limit::TimeScale;

        let rf = parse(
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
        let requests = load_requests(&rf, None).unwrap();

        let warnings = time_limit_warnings(&requests, &rf.config, &TimeLimits::default());
        assert_eq!(warnings.len(), 2);
        assert_eq!(
            warnings[0],
            "warning: request 'too-long' expects at most 40000ms, but timeout_secs = 30 will stop it first"
        );

        let scaled = TimeLimits::resolve(None, Some("3"));
        let warnings = time_limit_warnings(&requests, &rf.config, &scaled);
        assert_eq!(warnings.len(), 3);
        assert!(warnings[0].contains("45000ms (15000ms x 3 from TOAD_TIME_SCALE)"));

        let off = TimeLimits::resolve(Some(TimeScale::Off), None);
        assert!(time_limit_warnings(&requests, &rf.config, &off).is_empty());
    }

    #[test]
    fn unknown_ignore_config_key_is_an_error() {
        let rf = parse(
            r#"
            [login]
            url = "http://x"
            ignore_config = ["auht"]
            "#,
        );
        let err = validate_ignore_config(&rf).unwrap_err();
        assert!(
            err.to_string()
                .contains("unknown key 'auht' in ignore_config")
        );
    }

    #[test]
    fn captures_are_parsed_in_order() {
        let mut rf = parse(
            r#"
            [create-user]
            url = "http://x"

            [create-user.capture]
            user_id = "$.id"
            location = "header:Location"
            "#,
        );
        parse_captures(&mut rf).unwrap();
        let names: Vec<&str> = rf.requests["create-user"]
            .captures
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, ["user_id", "location"]);
    }

    #[test]
    fn invalid_capture_is_an_error() {
        let mut rf = parse(
            r#"
            [create-user]
            url = "http://x"

            [create-user.capture]
            user_id = "id"
            "#,
        );
        let err = parse_captures(&mut rf).unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid capture 'user_id' in request 'create-user'"
        );
    }
}
