use colored::{ColoredString, Colorize};
use indexmap::IndexMap;
use reqwest::{StatusCode, header::HeaderMap};
use serde::Serialize;
use std::{collections::HashMap, time::Duration};

use crate::{collection::RequestDef, interpolate::interpolate};

/// A response as it was received, before any checks.
pub struct Received {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: String,
    pub elapsed: Duration,
}

/// Totals for a run, reported once at the end.
pub struct Summary {
    pub passed: usize,
    pub failed: usize,
    pub not_run: usize,
    pub elapsed: Duration,
}

pub trait OutputMode {
    /// Called once, before anything is checked or sent, with the requests that will run.
    fn run_start(&self, _requests: &[&str]) {}
    /// `url` is the interpolated `url` setting and `sent_url` is the URL as sent, with the query.
    fn request_start(
        &self,
        _name: &str,
        _req: &RequestDef,
        _vars: &HashMap<String, String>,
        _header_map: &HeaderMap,
        _url: &str,
        _sent_url: &str,
    ) {
    }
    fn request_complete(&self, _name: &str, _received: &Received) {}
    fn request_captured(&self, _name: &str, _captured: &[(String, String)]) {}
    /// Called instead of `request_complete` when an attempt failed and will be retried.
    /// `received` is `None` when the request could not be sent.
    fn attempt_failed(
        &self,
        _name: &str,
        _received: Option<&Received>,
        _err: &str,
        _next_attempt: u32,
        _attempts: u32,
        _delay_ms: u64,
    ) {
    }
    fn request_error(&self, _name: &str, _err: &anyhow::Error) {}
    /// Called once at the end of the run, whether it passed or not.
    fn run_finished(&self, _summary: &Summary) {}
}

pub struct NormalOutput {}

impl OutputMode for NormalOutput {
    fn request_start(
        &self,
        _name: &str,
        _req: &RequestDef,
        _vars: &HashMap<String, String>,
        _header_map: &HeaderMap,
        _url: &str,
        _sent_url: &str,
    ) {
    }

    fn request_complete(&self, name: &str, received: &Received) {
        let status_colored = colorize_response_code(received.status);

        println!("[{}] {} ({:.0?})", name, status_colored, received.elapsed);
        println!("{}", try_pretty_json(&received.body));
    }

    fn attempt_failed(
        &self,
        name: &str,
        received: Option<&Received>,
        err: &str,
        next_attempt: u32,
        attempts: u32,
        delay_ms: u64,
    ) {
        if let Some(received) = received {
            self.request_complete(name, received);
        }
        print_retrying(name, err, next_attempt, attempts, delay_ms);
    }

    fn request_error(&self, name: &str, err: &anyhow::Error) {
        println!("{} -> {:?}", name, err)
    }
}

pub struct QuietOutput {}

impl OutputMode for QuietOutput {
    fn request_start(
        &self,
        _name: &str,
        _req: &RequestDef,
        _vars: &HashMap<String, String>,
        _header_map: &HeaderMap,
        _url: &str,
        _sent_url: &str,
    ) {
    }

    fn request_complete(&self, name: &str, received: &Received) {
        let status_colored = colorize_response_code(received.status);

        println!("[{}] {} ({:.0?})", name, status_colored, received.elapsed);
    }

    fn attempt_failed(
        &self,
        name: &str,
        received: Option<&Received>,
        err: &str,
        next_attempt: u32,
        attempts: u32,
        delay_ms: u64,
    ) {
        if let Some(received) = received {
            self.request_complete(name, received);
        }
        print_retrying(name, err, next_attempt, attempts, delay_ms);
    }

    fn request_error(&self, name: &str, err: &anyhow::Error) {
        println!("{} -> {:?}", name, err)
    }
}

pub struct SilentOutput {}

impl OutputMode for SilentOutput {
    fn request_start(
        &self,
        _name: &str,
        _req: &RequestDef,
        _vars: &HashMap<String, String>,
        _header_map: &HeaderMap,
        _url: &str,
        _sent_url: &str,
    ) {
    }
}

pub struct VerboseOutput {}

impl OutputMode for VerboseOutput {
    fn request_start(
        &self,
        _name: &str,
        req: &RequestDef,
        vars: &HashMap<String, String>,
        header_map: &HeaderMap,
        url: &str,
        _sent_url: &str,
    ) {
        println!("{}", "-- request -------------------------------".dimmed());
        println!("{} {}", req.method.to_uppercase().cyan().bold(), url);
        if !req.query.is_empty() {
            println!("{}", "query:".dimmed());
            for (k, v) in &req.query {
                println!(
                    "  {} = {}",
                    k.dimmed(),
                    interpolate(v, vars).unwrap_or_else(|_| v.clone())
                );
            }
        }

        if !header_map.is_empty() {
            println!("{}", "headers".dimmed());
            for (k, v) in header_map {
                println!(
                    "  {}: {}",
                    k.as_str().dimmed(),
                    v.to_str().unwrap_or("<binary>")
                );
            }
        }

        if let Some(Ok(body)) = req.resolved_body(vars) {
            println!("{}", "body".dimmed());
            println!("{}", try_pretty_json(&body));
        }
    }

    fn request_complete(&self, name: &str, received: &Received) {
        let status_colored = colorize_response_code(received.status);

        println!("[{}] {} ({:.0?})", name, status_colored, received.elapsed);
        println!("{}", try_pretty_json(&received.body));
    }

    fn request_captured(&self, _name: &str, captured: &[(String, String)]) {
        println!("{}", "captured:".dimmed());
        for (k, v) in captured {
            println!("  {} = {}", k.dimmed(), v);
        }
    }

    fn attempt_failed(
        &self,
        name: &str,
        received: Option<&Received>,
        err: &str,
        next_attempt: u32,
        attempts: u32,
        delay_ms: u64,
    ) {
        if let Some(received) = received {
            self.request_complete(name, received);
        }
        print_retrying(name, err, next_attempt, attempts, delay_ms);
    }

    fn request_error(&self, name: &str, err: &anyhow::Error) {
        println!("{} -> {:?}", name, err)
    }
}

pub struct ResponseOnlyOutput {}

impl OutputMode for ResponseOnlyOutput {
    fn request_start(
        &self,
        _name: &str,
        _req: &RequestDef,
        _vars: &HashMap<String, String>,
        _header_map: &HeaderMap,
        _url: &str,
        _sent_url: &str,
    ) {
    }

    fn request_complete(&self, _name: &str, received: &Received) {
        println!("{}", try_pretty_json(&received.body));
    }
}

pub struct RequestOnlyOutput {}

impl OutputMode for RequestOnlyOutput {
    fn request_start(
        &self,
        _name: &str,
        req: &RequestDef,
        vars: &HashMap<String, String>,
        _header_map: &HeaderMap,
        _url: &str,
        _sent_url: &str,
    ) {
        if let Some(Ok(body)) = req.resolved_body(vars) {
            println!("{}", "body".dimmed());
            println!("{}", try_pretty_json(&body));
        }
    }
}

/// One JSON object per line for each event. Field order follows the declaration order here.
#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Event<'a> {
    Start {
        version: u32,
        requests: &'a [&'a str],
    },
    RequestStart {
        name: &'a str,
        method: String,
        url: &'a str,
        headers: IndexMap<String, String>,
        body: Option<String>,
    },
    Response {
        name: &'a str,
        status: u16,
        elapsed_ms: u128,
        headers: IndexMap<String, String>,
        body: &'a str,
    },
    AttemptFailed {
        name: &'a str,
        status: Option<u16>,
        elapsed_ms: Option<u128>,
        headers: Option<IndexMap<String, String>>,
        body: Option<&'a str>,
        error: &'a str,
        next_attempt: u32,
        attempts: u32,
        delay_ms: u64,
    },
    Captured {
        name: &'a str,
        values: IndexMap<&'a str, &'a str>,
    },
    Error {
        name: &'a str,
        error: String,
    },
    Summary {
        passed: usize,
        failed: usize,
        not_run: usize,
        elapsed_ms: u128,
    },
}

/// Bumped when a change to the events could break a program that reads them
const JSON_VERSION: u32 = 1;

pub struct JsonOutput {}

impl JsonOutput {
    fn emit(&self, event: Event) {
        // Serializing these types can't fail: every map key is a string
        println!(
            "{}",
            serde_json::to_string(&event).expect("event serializes")
        );
    }
}

impl OutputMode for JsonOutput {
    fn run_start(&self, requests: &[&str]) {
        self.emit(Event::Start {
            version: JSON_VERSION,
            requests,
        });
    }

    fn request_start(
        &self,
        name: &str,
        req: &RequestDef,
        vars: &HashMap<String, String>,
        header_map: &HeaderMap,
        _url: &str,
        sent_url: &str,
    ) {
        let mut headers = header_object(header_map);
        if let Some(auth) = headers.get_mut(reqwest::header::AUTHORIZATION.as_str()) {
            *auth = redact_authorization(auth);
        }
        self.emit(Event::RequestStart {
            name,
            method: req.method.to_uppercase(),
            url: sent_url,
            headers,
            body: req.resolved_body(vars).and_then(Result::ok),
        });
    }

    fn request_complete(&self, name: &str, received: &Received) {
        self.emit(Event::Response {
            name,
            status: received.status.as_u16(),
            elapsed_ms: received.elapsed.as_millis(),
            headers: header_object(&received.headers),
            body: &received.body,
        });
    }

    fn request_captured(&self, name: &str, captured: &[(String, String)]) {
        self.emit(Event::Captured {
            name,
            values: captured
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect(),
        });
    }

    fn attempt_failed(
        &self,
        name: &str,
        received: Option<&Received>,
        err: &str,
        next_attempt: u32,
        attempts: u32,
        delay_ms: u64,
    ) {
        self.emit(Event::AttemptFailed {
            name,
            status: received.map(|r| r.status.as_u16()),
            elapsed_ms: received.map(|r| r.elapsed.as_millis()),
            headers: received.map(|r| header_object(&r.headers)),
            body: received.map(|r| r.body.as_str()),
            error: err,
            next_attempt,
            attempts,
            delay_ms,
        });
    }

    fn request_error(&self, name: &str, err: &anyhow::Error) {
        self.emit(Event::Error {
            name,
            error: format!("{err:#}"),
        });
    }

    fn run_finished(&self, summary: &Summary) {
        self.emit(Event::Summary {
            passed: summary.passed,
            failed: summary.failed,
            not_run: summary.not_run,
            elapsed_ms: summary.elapsed.as_millis(),
        });
    }
}

/// Header names are lowercase. A header sent more than once has its values joined with ", ".
fn header_object(header_map: &HeaderMap) -> IndexMap<String, String> {
    let mut headers: IndexMap<String, String> = IndexMap::new();
    for (k, v) in header_map {
        let v = String::from_utf8_lossy(v.as_bytes());
        headers
            .entry(k.as_str().to_string())
            .and_modify(|existing| {
                existing.push_str(", ");
                existing.push_str(&v);
            })
            .or_insert_with(|| v.to_string());
    }
    headers
}

/// Keeps the scheme so the output still shows which kind of auth was sent: `Bearer ***`.
fn redact_authorization(value: &str) -> String {
    match value.split_once(' ') {
        Some((scheme, _)) => format!("{scheme} ***"),
        None => "***".to_string(),
    }
}

fn print_retrying(name: &str, err: &str, next_attempt: u32, attempts: u32, delay_ms: u64) {
    println!(
        "{} {}",
        format!(
            "retrying '{}' in {}ms (attempt {} of {}):",
            name, delay_ms, next_attempt, attempts
        )
        .yellow(),
        err
    );
}

fn colorize_response_code(status: StatusCode) -> ColoredString {
    let code = status.as_u16();
    if status.is_success() {
        code.to_string().green()
    } else if status.is_client_error() {
        code.to_string().yellow()
    } else if status.is_server_error() {
        code.to_string().red()
    } else {
        code.to_string().white()
    }
}

fn try_pretty_json(s: &str) -> String {
    serde_json::from_str::<serde_json::Value>(s)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_authorization_but_keeps_the_scheme() {
        assert_eq!(redact_authorization("Bearer tok-123"), "Bearer ***");
        assert_eq!(redact_authorization("Basic dXNlcjpwYXNz"), "Basic ***");
        assert_eq!(redact_authorization("tok-123"), "***");
    }

    #[test]
    fn joins_repeated_headers() {
        let mut headers = HeaderMap::new();
        headers.append("set-cookie", "a=1".parse().unwrap());
        headers.append("set-cookie", "b=2".parse().unwrap());
        headers.append("content-type", "application/json".parse().unwrap());
        let object = header_object(&headers);
        assert_eq!(object["set-cookie"], "a=1, b=2");
        assert_eq!(object["content-type"], "application/json");
    }
}
