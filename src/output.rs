use colored::{ColoredString, Colorize};
use reqwest::{StatusCode, header::HeaderMap};
use std::{collections::HashMap, time::Duration};

use crate::{collection::RequestDef, interpolate::interpolate};

pub trait OutputMode {
    fn request_start(
        &self,
        _name: &str,
        _req: &RequestDef,
        _vars: &HashMap<String, String>,
        _header_map: &HeaderMap,
        _url: &str,
    ) {
    }
    fn request_complete(&self, _name: &str, _status: StatusCode, _elapsed: Duration, _body: &str) {}
    fn request_captured(&self, _name: &str, _captured: &[(String, String)]) {}
    /// Called instead of `request_complete` when an attempt failed and will be retried.
    /// `response` is `None` when the request could not be sent.
    fn attempt_failed(
        &self,
        _name: &str,
        _response: Option<(StatusCode, Duration, &str)>,
        _err: &str,
        _next_attempt: u32,
        _attempts: u32,
        _delay_ms: u64,
    ) {
    }
    fn request_error(&self, _name: &str, _err: &str) {}
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
    ) {
    }

    fn request_complete(&self, name: &str, status: StatusCode, elapsed: Duration, body: &str) {
        let status_colored = colorize_response_code(status);

        println!("[{}] {} ({:.0?})", name, status_colored, elapsed);
        println!("{}", try_pretty_json(body));
    }

    fn attempt_failed(
        &self,
        name: &str,
        response: Option<(StatusCode, Duration, &str)>,
        err: &str,
        next_attempt: u32,
        attempts: u32,
        delay_ms: u64,
    ) {
        if let Some((status, elapsed, body)) = response {
            self.request_complete(name, status, elapsed, body);
        }
        print_retrying(name, err, next_attempt, attempts, delay_ms);
    }

    fn request_error(&self, name: &str, err: &str) {
        println!("{} -> {}", name, err)
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
    ) {
    }

    fn request_complete(&self, name: &str, status: StatusCode, elapsed: Duration, _body: &str) {
        let status_colored = colorize_response_code(status);

        println!("[{}] {} ({:.0?})", name, status_colored, elapsed);
    }

    fn attempt_failed(
        &self,
        name: &str,
        response: Option<(StatusCode, Duration, &str)>,
        err: &str,
        next_attempt: u32,
        attempts: u32,
        delay_ms: u64,
    ) {
        if let Some((status, elapsed, body)) = response {
            self.request_complete(name, status, elapsed, body);
        }
        print_retrying(name, err, next_attempt, attempts, delay_ms);
    }

    fn request_error(&self, name: &str, err: &str) {
        println!("{} -> {}", name, err)
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
    ) {
    }

    fn request_complete(&self, _name: &str, _status: StatusCode, _elapsed: Duration, _body: &str) {}

    fn request_error(&self, _name: &str, _err: &str) {}
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

    fn request_complete(&self, name: &str, status: StatusCode, elapsed: Duration, body: &str) {
        let status_colored = colorize_response_code(status);

        println!("[{}] {} ({:.0?})", name, status_colored, elapsed);
        println!("{}", try_pretty_json(body));
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
        response: Option<(StatusCode, Duration, &str)>,
        err: &str,
        next_attempt: u32,
        attempts: u32,
        delay_ms: u64,
    ) {
        if let Some((status, elapsed, body)) = response {
            self.request_complete(name, status, elapsed, body);
        }
        print_retrying(name, err, next_attempt, attempts, delay_ms);
    }

    fn request_error(&self, name: &str, err: &str) {
        println!("{} -> {}", name, err)
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
    ) {
    }

    fn request_complete(&self, _name: &str, _status: StatusCode, _elapsed: Duration, body: &str) {
        println!("{}", try_pretty_json(body));
    }

    fn request_error(&self, _name: &str, _err: &str) {}
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
    ) {
        if let Some(Ok(body)) = req.resolved_body(vars) {
            println!("{}", "body".dimmed());
            println!("{}", try_pretty_json(&body));
        }
    }

    fn request_complete(&self, _name: &str, _status: StatusCode, _elapsed: Duration, _body: &str) {}

    fn request_error(&self, _name: &str, _err: &str) {}
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
