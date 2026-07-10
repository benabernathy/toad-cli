use std::collections::HashMap;
use std::str::FromStr;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::ca::load_custom_ca_certificates;
use crate::collection::{Config, RequestDef};
use crate::interpolate::interpolate;

use crate::output::OutputMode;

pub fn execute_request(
    name: &str,
    req: &RequestDef,
    vars: &HashMap<String, String>,
    config: Config,
    ca_password: Option<&str>,
    output: &dyn OutputMode,
) -> Result<()> {
    let url = interpolate(&req.url, vars);
    let method = req.method.to_uppercase();

    // build headers
    let mut header_map = HeaderMap::new();
    for (k, v) in &req.headers {
        let v = interpolate(v, vars);
        header_map.insert(HeaderName::from_str(k)?, HeaderValue::from_str(&v)?);
    }

    // build body
    let body_bytes = if let Some(body) = &req.body {
        let interpolated = interpolate(body, vars);

        // validate it's real JSON
        serde_json::from_str::<serde_json::Value>(&interpolated)
            .with_context(|| format!("body in '{name} is not valid JSON"))?;
        header_map.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Some(interpolated.into_bytes())
    } else {
        None
    };

    let mut client_builder = Client::builder()
        .timeout(std::time::Duration::from_secs(req.timeout_secs))
        .danger_accept_invalid_certs(config.ignore_ssl);

    if let Some(ca_path) = &config.use_custom_ca {
        let certs = load_custom_ca_certificates(ca_path, ca_password)
            .with_context(|| format!("could not load custom CA for request '{name}'"))?;
        for cert in certs {
            client_builder = client_builder.add_root_certificate(cert);
        }
    }

    let client = client_builder.build()?;

    let mut builder = client
        .request(reqwest::Method::from_bytes(method.as_bytes())?, &url)
        .headers(header_map.clone());

    if !req.query.is_empty() {
        let query: Vec<(String, String)> = req
            .query
            .iter()
            .map(|(k, v)| (k.clone(), interpolate(v, vars)))
            .collect();

        let url = reqwest::Url::parse_with_params(&url, &query)
            .with_context(|| format!("could not build query params for '{name}'"))?;

        builder = client
            .request(reqwest::Method::from_bytes(method.as_bytes())?, url)
            .headers(header_map.clone());
    }

    if let Some(b) = body_bytes {
        builder = builder.body(b);
    }

    output.request_start(name, req, vars, &header_map, &url);

    let start = Instant::now();
    let response = builder
        .send()
        .with_context(|| format!("request '{name}' failed to send"))?;
    let elapsed = start.elapsed();

    let status = response.status();
    let body_text = response.text().unwrap_or_default();
    let _response_body_length = format!("{}B", body_text.len());

    output.request_complete(name, status, elapsed, &body_text);

    if let Some(expected) = &req.expect_status {
        let code = status.as_u16();
        if !expected.contains(&code) {
            return Err(anyhow!(
                "request '{}' expected status {:?} but got {}",
                name,
                expected,
                code
            ));
        }
    }

    Ok(())
}
