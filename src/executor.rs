use std::collections::HashMap;
use std::str::FromStr;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::ca::load_custom_ca_certificates;
use crate::collection::{Config, RequestDef};
use crate::interpolate::interpolate;
use crate::time_limit::TimeLimits;

use crate::output::OutputMode;

pub fn execute_request(
    name: &str,
    req: &RequestDef,
    vars: &HashMap<String, String>,
    config: Config,
    ca_password: Option<&str>,
    time_limits: &TimeLimits,
    output: &dyn OutputMode,
) -> Result<Vec<(String, String)>> {
    let url = interpolate(&req.url, vars)?;
    let method = req.method.to_uppercase();

    // build headers
    let mut header_map = HeaderMap::new();
    for (k, v) in &req.headers {
        let v = interpolate(v, vars)?;
        header_map.insert(HeaderName::from_str(k)?, HeaderValue::from_str(&v)?);
    }

    if let Some(raw_auth) = req.auth.as_deref().or(config.auth.as_deref()) {
        if header_map.contains_key(reqwest::header::AUTHORIZATION) {
            return Err(anyhow!(
                "request '{}' sets both 'auth' and a manual 'Authorization' header - use only one",
                name
            ));
        }
        let interpolated_auth = interpolate(raw_auth, vars)?;
        let auth_value = crate::auth::build_authorization_value(&interpolated_auth)
            .with_context(|| format!("invalid auth value in request '{name}'"))?;
        header_map.insert(
            reqwest::header::AUTHORIZATION,
            HeaderValue::from_str(&auth_value)?,
        );
    }

    // build body
    let body_bytes = if let Some(body) = req.resolved_body(vars) {
        let body = body?;

        // validate it's real JSON
        serde_json::from_str::<serde_json::Value>(&body)
            .with_context(|| format!("body in '{name} is not valid JSON"))?;
        header_map.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Some(body.into_bytes())
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
            .map(|(k, v)| Ok((k.clone(), interpolate(v, vars)?)))
            .collect::<Result<_>>()?;

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

    let status = response.status();
    let response_headers = response.headers().clone();
    let body_text = response.text().unwrap_or_default();
    // Timed through the end of the body download, so slow or large bodies count
    let elapsed = start.elapsed();
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

    if let Some(max_ms) = req.max_ms(&config)
        && let Some(limit) = time_limits.effective(max_ms)
    {
        let took = elapsed.as_millis();
        if took > u128::from(limit) {
            return Err(anyhow!(
                "request '{}' took {}ms, expected at most {}",
                name,
                took,
                time_limits.describe(max_ms)
            ));
        }
    }

    let json = if req.captures.is_empty() {
        None
    } else {
        serde_json::from_str::<serde_json::Value>(&body_text).ok()
    };

    let captured = req
        .captures
        .iter()
        .map(|c| {
            let value = c.extract(status, &response_headers, &body_text, json.as_ref())?;
            Ok((c.name.clone(), value))
        })
        .collect::<Result<Vec<_>>>()?;

    if !captured.is_empty() {
        output.request_captured(name, &captured);
    }

    Ok(captured)
}
