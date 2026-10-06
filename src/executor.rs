use std::collections::HashMap;
use std::str::FromStr;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::ca::load_custom_ca_certificates;
use crate::collection::{Config, RequestDef};
use crate::interpolate::interpolate;
use crate::retry::RetryPolicy;
use crate::time_limit::TimeLimits;

use crate::output::{OutputMode, Received};

/// Settings that are the same for every request in a run.
pub struct RunContext<'a> {
    pub ca_password: Option<&'a str>,
    pub time_limits: TimeLimits,
    pub output: &'a dyn OutputMode,
    /// `--var` values. They replace captured values with the same name.
    pub cli_vars: &'a [(String, String)],
}

pub fn execute_request(
    name: &str,
    req: &RequestDef,
    vars: &HashMap<String, String>,
    config: Config,
    retry: &RetryPolicy,
    ctx: &RunContext,
) -> Result<Vec<(String, String)>> {
    let output = ctx.output;
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
        let certs = load_custom_ca_certificates(ca_path, ctx.ca_password)
            .with_context(|| format!("could not load custom CA for request '{name}'"))?;
        for cert in certs {
            client_builder = client_builder.add_root_certificate(cert);
        }
    }

    let client = client_builder.build()?;

    let mut builder = client
        .request(reqwest::Method::from_bytes(method.as_bytes())?, &url)
        .headers(header_map.clone());
    let mut sent_url = url.clone();

    if !req.query.is_empty() {
        let query: Vec<(String, String)> = req
            .query
            .iter()
            .map(|(k, v)| Ok((k.clone(), interpolate(v, vars)?)))
            .collect::<Result<_>>()?;

        let url = reqwest::Url::parse_with_params(&url, &query)
            .with_context(|| format!("could not build query params for '{name}'"))?;
        sent_url = url.to_string();

        builder = client
            .request(reqwest::Method::from_bytes(method.as_bytes())?, url)
            .headers(header_map.clone());
    }

    if let Some(b) = body_bytes {
        builder = builder.body(b);
    }

    output.request_start(name, req, vars, &header_map, &url, &sent_url);

    let attempts = retry.attempts();
    let mut attempt = 1;
    loop {
        let request = builder
            .try_clone()
            .ok_or_else(|| anyhow!("request '{name}' could not be prepared for sending"))?;

        let Attempt { received, outcome } =
            send_and_check(name, req, &config, &ctx.time_limits, request);

        match outcome {
            Ok(captured) => {
                if let Some(r) = &received {
                    output.request_complete(name, r);
                }
                if !captured.is_empty() {
                    output.request_captured(name, &captured, &replaced_by_cli(&captured, ctx));
                }
                return Ok(captured);
            }
            Err(err) if attempt < attempts => {
                attempt += 1;
                output.attempt_failed(
                    name,
                    received.as_ref(),
                    &format!("{err:#}"),
                    attempt,
                    attempts,
                    retry.delay_ms,
                );
                thread::sleep(Duration::from_millis(retry.delay_ms));
            }
            Err(err) => {
                if let Some(r) = &received {
                    output.request_complete(name, r);
                }
                if attempts > 1 {
                    return Err(anyhow!("{err:#} (after {attempts} attempts)"));
                }
                return Err(err);
            }
        }
    }
}

/// The result of one attempt. `received` is `None` when the request could not be sent.
struct Attempt {
    received: Option<Received>,
    outcome: Result<Vec<(String, String)>>,
}

/// Sends one attempt and checks the response.
fn send_and_check(
    name: &str,
    req: &RequestDef,
    config: &Config,
    time_limits: &TimeLimits,
    request: RequestBuilder,
) -> Attempt {
    let start = Instant::now();
    let response = match request
        .send()
        .with_context(|| format!("request '{name}' failed to send"))
    {
        Ok(response) => response,
        Err(e) => {
            return Attempt {
                received: None,
                outcome: Err(e),
            };
        }
    };

    let status = response.status();
    let headers = response.headers().clone();
    let body = response.text().unwrap_or_default();
    // Timed through the end of the body download, so slow or large bodies count
    let elapsed = start.elapsed();

    let received = Received {
        status,
        headers,
        body,
        elapsed,
    };

    let outcome = check_response(name, req, config, time_limits, &received);
    Attempt {
        received: Some(received),
        outcome,
    }
}

/// The `--var` value for each captured name it replaces. The last `--var` for a name wins.
fn replaced_by_cli(captured: &[(String, String)], ctx: &RunContext) -> Vec<(String, String)> {
    captured
        .iter()
        .filter_map(|(name, _)| ctx.cli_vars.iter().rev().find(|(n, _)| n == name).cloned())
        .collect()
}

/// Checks status, then time, then runs captures.
fn check_response(
    name: &str,
    req: &RequestDef,
    config: &Config,
    time_limits: &TimeLimits,
    received: &Received,
) -> Result<Vec<(String, String)>> {
    if let Some(expected) = &req.expect_status {
        let code = received.status.as_u16();
        if !expected.contains(&code) {
            return Err(anyhow!(
                "request '{}' expected status {:?} but got {}",
                name,
                expected,
                code
            ));
        }
    }

    if let Some(max_ms) = req.max_ms(config)
        && let Some(limit) = time_limits.effective(max_ms)
    {
        let took = received.elapsed.as_millis();
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
        serde_json::from_str::<serde_json::Value>(&received.body).ok()
    };

    req.captures
        .iter()
        .map(|c| {
            let value = c.extract(
                received.status,
                &received.headers,
                &received.body,
                json.as_ref(),
            )?;
            Ok((c.name.clone(), value))
        })
        .collect()
}
