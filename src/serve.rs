use std::{fs::OpenOptions, io::Write, path::PathBuf};

use anyhow::{Context, Result};
use chrono::Local;

pub fn run(port: u16, output_file: Option<PathBuf>) -> Result<()> {
    let server = tiny_http::Server::http(("0.0.0.0", port))
        .map_err(|e| anyhow::anyhow!("could not bind to port {}: {}", port, e))?;

    println!("listening on port {}", port);

    let mut file = match &output_file {
        Some(path) => Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("could not open output file {}", path.display()))?,
        ),
        None => None,
    };

    for mut request in server.incoming_requests() {
        let timestamp = Local::now().to_rfc3339();
        let method = request.method().to_string();
        let url = request.url().to_string();

        let mut body = String::new();
        request.as_reader().read_to_string(&mut body).ok();

        let headers: Vec<(String, String)> = request
            .headers()
            .iter()
            .map(|h| (h.field.to_string(), h.value.to_string()))
            .collect();

        let block = format_capture(&timestamp, &method, &url, &headers, &body);

        match &mut file {
            Some(f) => {
                f.write_all(block.as_bytes())
                    .with_context(|| "could not write to output file")?;
                println!("[{}] {} {}", timestamp, method, url);
            }
            None => print!("{}", block),
        }

        let response = tiny_http::Response::from_string("OK").with_status_code(200);
        let _ = request.respond(response);
    }

    Ok(())
}

fn format_capture(
    timestamp: &str,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &str,
) -> String {
    let mut out = format!("[{}] {} {}\n", timestamp, method, url);

    out.push_str("Headers:\n");
    for (k, v) in headers {
        out.push_str(&format!("  {}: {}\n", k, v));
    }

    out.push_str("Body:\n");
    out.push_str(&try_pretty_json(body));
    out.push('\n');
    out.push_str(&"-".repeat(40));
    out.push('\n');

    out
}

fn try_pretty_json(s: &str) -> String {
    serde_json::from_str::<serde_json::Value>(s)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| s.to_string())
}
