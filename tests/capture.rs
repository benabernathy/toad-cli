use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread;

use tiny_http::{Header, Response, Server};

/// Starts a fake API on a random port and returns its base URL.
///
/// - `POST /echo` returns the request body unchanged
/// - `POST /login` returns a token, and rejects requests that send an Authorization header
/// - `POST /users` returns `{"id": 42}` with `Location: /users/42`
/// - `GET /users/42` and `DELETE /users/42` require `Authorization: Bearer tok-123`
fn start_server() -> String {
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

fn write_file(test_name: &str, file_name: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("toad-test-{}-{}", test_name, std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(file_name);
    fs::write(&path, contents).unwrap();
    path
}

fn write_collection(test_name: &str, contents: &str) -> PathBuf {
    write_file(test_name, "collection.toml", contents)
}

fn toad(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_toad"))
        .args(args)
        .env_remove("TOAD_OUTPUT")
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn login_create_read_delete_chain() {
    let base_url = start_server();
    let file = write_collection(
        "chain",
        &format!(
            r#"
[config]
auth = "bearer {{{{token}}}}"

[vars]
base_url = "{base_url}"

[login]
method = "POST"
url = "{{{{base_url}}}}/login"
ignore_config = ["auth"]
expect_status = [200]

[login.capture]
token = "$.access_token"

[create-user]
method = "POST"
url = "{{{{base_url}}}}/users"
body = '{{"name": "Toad"}}'
expect_status = [201]

[create-user.capture]
user_id = "$.id"
location = "header:Location"
roles = "$.roles"
code = "status"

[get-user]
method = "GET"
url = "{{{{base_url}}}}/users/{{{{user_id}}}}"
expect_status = [200]

[get-user-by-location]
method = "GET"
url = "{{{{base_url}}}}{{{{location}}}}"
expect_status = [200]

[delete-user]
method = "DELETE"
url = "{{{{base_url}}}}/users/{{{{user_id}}}}"
expect_status = [204]
"#
        ),
    );

    let output = toad(&[file.to_str().unwrap(), "-o", "verbose"]);
    let out = stdout(&output);
    assert!(output.status.success(), "toad failed:\n{out}");
    assert!(out.contains("token = tok-123"), "{out}");
    assert!(out.contains("user_id = 42"), "{out}");
    assert!(out.contains("location = /users/42"), "{out}");
    assert!(out.contains(r#"roles = ["reader","editor"]"#), "{out}");
    assert!(out.contains("code = 201"), "{out}");
}

#[test]
fn login_without_ignore_config_fails_on_undefined_token() {
    let base_url = start_server();
    let file = write_collection(
        "no-ignore",
        &format!(
            r#"
[config]
auth = "bearer {{{{token}}}}"

[login]
method = "POST"
url = "{base_url}/login"

[login.capture]
token = "$.access_token"
"#
        ),
    );

    let output = toad(&[file.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(stdout(&output).contains("undefined variable 'token'"));
}

#[test]
fn single_request_uses_vars_fallback() {
    let base_url = start_server();
    let file = write_collection(
        "fallback",
        &format!(
            r#"
[vars]
user_id = "42"
token = "tok-123"

[create-user]
method = "POST"
url = "{base_url}/users"
auth = "bearer {{{{token}}}}"

[create-user.capture]
user_id = "$.id"

[get-user]
url = "{base_url}/users/{{{{user_id}}}}"
auth = "bearer {{{{token}}}}"
expect_status = [200]
"#
        ),
    );

    let output = toad(&[file.to_str().unwrap(), "get-user"]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn single_request_without_fallback_fails() {
    let base_url = start_server();
    let file = write_collection(
        "no-fallback",
        &format!(
            r#"
[create-user]
method = "POST"
url = "{base_url}/users"

[create-user.capture]
user_id = "$.id"

[get-user]
url = "{base_url}/users/{{{{user_id}}}}"
"#
        ),
    );

    let output = toad(&[file.to_str().unwrap(), "get-user"]);
    assert!(!output.status.success());
    assert!(stdout(&output).contains("undefined variable 'user_id'"));
}

#[test]
fn failed_capture_stops_the_run() {
    let base_url = start_server();
    let file = write_collection(
        "failed-capture",
        &format!(
            r#"
[login]
method = "POST"
url = "{base_url}/login"

[login.capture]
token = "$.missing"

[never-runs]
url = "{base_url}/users/42"
"#
        ),
    );

    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert!(!output.status.success());
    assert!(
        out.contains("capture 'token': no value matched '$.missing'"),
        "{out}"
    );
    assert!(!out.contains("[never-runs]"), "{out}");
}

#[test]
fn invalid_capture_is_reported_before_any_request() {
    let file = write_collection(
        "invalid-capture",
        r#"
[create-user]
method = "POST"
url = "http://127.0.0.1:1/users"

[create-user.capture]
user_id = "id"
"#,
    );

    let output = toad(&[file.to_str().unwrap()]);
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        err.contains("invalid capture 'user_id' in request 'create-user'"),
        "{err}"
    );
}

#[test]
fn escaped_braces_are_sent_literally() {
    let base_url = start_server();
    let file = write_collection(
        "escape",
        &format!(
            r#"
[vars]
email = "ben@example.com"

[send-welcome-email]
method = "POST"
url = "{base_url}/echo"
body = '''
{{
  "to": "{{{{email}}}}",
  "subject": "Welcome, \{{{{first_name}}}}!"
}}
'''

[send-welcome-email.capture]
to = "$.to"
subject = "$.subject"
"#
        ),
    );

    let output = toad(&[file.to_str().unwrap(), "-o", "verbose"]);
    let out = stdout(&output);
    assert!(output.status.success(), "{out}");
    assert!(out.contains("to = ben@example.com"), "{out}");
    assert!(out.contains("subject = Welcome, {{first_name}}!"), "{out}");
}

#[test]
fn interpolate_body_false_sends_body_file_unchanged() {
    let base_url = start_server();
    let template =
        r#"{"template": "Hi {{first_name}}, {{#if vip}}welcome back{{/if}}. Path: C:\\{{dir}}"}"#;
    write_file("raw-body", "welcome.hbs.json", template);
    let file = write_collection(
        "raw-body",
        &format!(
            r#"
[vars]
base_url = "{base_url}"

[upload-template]
method = "POST"
url = "{{{{base_url}}}}/echo"
body_file = "./welcome.hbs.json"
interpolate_body = false

[upload-template.capture]
sent = "body"
"#
        ),
    );

    let output = toad(&[file.to_str().unwrap(), "-o", "verbose"]);
    let out = stdout(&output);
    assert!(output.status.success(), "{out}");
    assert!(out.contains(&format!("sent = {template}")), "{out}");
}
