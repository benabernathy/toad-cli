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

    #[serde(default)]
    pub auth: Option<String>,

    pub expect_status: Option<Vec<u16>>,

    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_method() -> String {
    "GET".to_string()
}
fn default_timeout() -> u64 {
    30
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
