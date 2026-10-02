#![allow(dead_code)]

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;

use tiny_http::{Header, Response, Server};

/// Starts a fake API on a random port and returns its base URL.
///
/// - `GET /slow` waits 300ms, then returns `{"id": 1}`
/// - `GET /slow-body` sends headers and part of the body right away, and the rest 300ms later
/// - `POST /echo` returns the request body unchanged
/// - `POST /login` returns a token, and rejects requests that send an Authorization header
/// - `POST /users` returns `{"id": 42}` with `Location: /users/42`
/// - `GET /users/42` and `DELETE /users/42` require `Authorization: Bearer tok-123`
pub fn start_server() -> String {
    let server = Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();

    thread::spawn(move || {
        for mut request in server.incoming_requests() {
            let auth = request
                .headers()
                .iter()
                .find(|h| h.field.equiv("Authorization"))
                .map(|h| h.value.to_string());
            let authorized = auth.as_deref() == Some("Bearer tok-123");
            let method = request.method().to_string();

            if request.url() == "/slow" {
                thread::sleep(Duration::from_millis(300));
                let _ = request.respond(Response::from_string(r#"{"id": 1}"#));
                continue;
            }

            if request.url() == "/slow-body" {
                // Headers go out right away; the end of the body arrives 300ms later
                let response = Response::new(200.into(), vec![], SlowBody::new(), None, None);
                let _ = request.respond(response);
                continue;
            }

            if request.url() == "/echo" {
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                let _ = request.respond(Response::from_string(body));
                continue;
            }

            let (status, body, location) = match (method.as_str(), request.url()) {
                ("POST", "/login") if auth.is_some() => (400, "{}", None),
                ("POST", "/login") => (200, r#"{"access_token": "tok-123"}"#, None),
                (_, _) if !authorized => (401, "{}", None),
                ("POST", "/users") => (
                    201,
                    r#"{"id": 42, "roles": ["reader", "editor"]}"#,
                    Some("/users/42"),
                ),
                ("GET", "/users/42") => (200, r#"{"id": 42, "name": "Toad"}"#, None),
                ("DELETE", "/users/42") => (204, "", None),
                _ => (404, "{}", None),
            };

            let mut response = Response::from_string(body).with_status_code(status);
            if let Some(location) = location {
                response.add_header(Header::from_bytes("Location", location).unwrap());
            }
            let _ = request.respond(response);
        }
    });

    format!("http://127.0.0.1:{port}")
}

pub fn write_file(test_name: &str, file_name: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("toad-test-{}-{}", test_name, std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(file_name);
    fs::write(&path, contents).unwrap();
    path
}

pub fn write_collection(test_name: &str, contents: &str) -> PathBuf {
    write_file(test_name, "collection.toml", contents)
}

pub fn toad(args: &[&str]) -> Output {
    toad_with_env(args, &[])
}

/// Runs toad with only the given TOAD_* environment variables set.
pub fn toad_with_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_toad"));
    command
        .args(args)
        .env_remove("TOAD_OUTPUT")
        .env_remove("TOAD_TIME_SCALE")
        .env_remove("TOAD_CA_PASSWORD");
    for (k, v) in env {
        command.env(k, v);
    }
    command.output().unwrap()
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// A response body that sends 40KB right away, then waits 300ms before sending the rest.
/// tiny_http holds the headers until it has read 32KB of body, so the first part has to be larger
/// than that for the headers to go out before the pause.
struct SlowBody {
    body: Vec<u8>,
    pos: usize,
    paused: bool,
}

impl SlowBody {
    fn new() -> Self {
        let body = format!(r#"{{"padding": "{}", "id": 1}}"#, "x".repeat(50_000));
        SlowBody {
            body: body.into_bytes(),
            pos: 0,
            paused: false,
        }
    }
}

impl Read for SlowBody {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        const FIRST_PART: usize = 40_000;
        if self.pos >= FIRST_PART && !self.paused {
            thread::sleep(Duration::from_millis(300));
            self.paused = true;
        }
        let end = if self.paused {
            self.body.len()
        } else {
            FIRST_PART
        };
        let n = buf.len().min(end - self.pos);
        buf[..n].copy_from_slice(&self.body[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}
