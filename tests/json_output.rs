mod common;

use serde_json::{Value, json};

use common::{start_server, stdout, toad, toad_with_env, write_collection};

/// Parses every stdout line as JSON, failing the test on a line that isn't.
fn events(output: &std::process::Output) -> Vec<Value> {
    stdout(output)
        .lines()
        .map(|line| {
            serde_json::from_str(line).unwrap_or_else(|e| panic!("not JSON: {line:?}: {e}"))
        })
        .collect()
}

fn kinds(events: &[Value]) -> Vec<&str> {
    events
        .iter()
        .map(|e| e["event"].as_str().unwrap())
        .collect()
}

#[test]
fn passing_run() {
    let base_url = start_server();
    let file = write_collection(
        "json-pass",
        &format!(
            r#"
[login]
method = "POST"
url = "{base_url}/login"

[login.capture]
token = "$.access_token"

[get-user]
url = "{base_url}/users/42"
auth = "bearer {{{{token}}}}"
"#
        ),
    );
    let output = toad(&[file.to_str().unwrap(), "-o", "json"]);
    let events = events(&output);
    assert!(output.status.success(), "{events:#?}");

    assert_eq!(
        kinds(&events),
        [
            "start",
            "request_start",
            "response",
            "captured",
            "request_start",
            "response",
            "summary"
        ]
    );
    assert_eq!(
        events[0],
        json!({"event": "start", "version": 1, "requests": ["login", "get-user"]})
    );
    assert_eq!(events[1]["method"], "POST");
    assert_eq!(
        events[3],
        json!({"event": "captured", "name": "login", "values": {"token": "tok-123"}})
    );

    let get_user = &events[4];
    assert_eq!(get_user["url"], format!("{base_url}/users/42"));
    assert_eq!(get_user["headers"]["authorization"], "Bearer ***");
    assert_eq!(get_user["body"], Value::Null);

    let response = &events[5];
    assert_eq!(response["name"], "get-user");
    assert_eq!(response["status"], 200);
    assert_eq!(response["body"], r#"{"id": 42, "name": "Toad"}"#);
    assert!(response["elapsed_ms"].is_u64(), "{response}");
    assert!(
        response["headers"]["content-length"].is_string(),
        "{response}"
    );

    let summary = &events[6];
    assert_eq!(summary["passed"], 2);
    assert_eq!(summary["failed"], 0);
    assert_eq!(summary["not_run"], 0);
    assert!(summary["elapsed_ms"].is_u64(), "{summary}");
}

#[test]
fn request_body_is_a_string() {
    let base_url = start_server();
    let file = write_collection(
        "json-body",
        &format!(
            r#"
[echo]
method = "POST"
url = "{base_url}/echo"
body = '{{"name": "Toad"}}'
"#
        ),
    );
    let output = toad(&[file.to_str().unwrap(), "-o", "json"]);
    let events = events(&output);
    assert!(output.status.success(), "{events:#?}");
    assert_eq!(events[1]["body"], r#"{"name": "Toad"}"#);
    assert_eq!(events[2]["body"], r#"{"name": "Toad"}"#);
}

#[test]
fn retried_request() {
    let base_url = start_server();
    let file = write_collection(
        "json-retry",
        &format!(
            r#"
[flaky]
url = "{base_url}/flaky/status/json-retry/1"
expect_status = [200]
retry = 1
retry_delay_ms = 10
"#
        ),
    );
    let output = toad(&[file.to_str().unwrap(), "-o", "json"]);
    let events = events(&output);
    assert!(output.status.success(), "{events:#?}");
    assert_eq!(
        kinds(&events),
        [
            "start",
            "request_start",
            "attempt_failed",
            "response",
            "summary"
        ]
    );

    let failed = &events[2];
    assert_eq!(failed["status"], 503);
    assert_eq!(failed["body"], r#"{"error": "unavailable"}"#);
    assert_eq!(
        failed["error"],
        "request 'flaky' expected status [200] but got 503"
    );
    assert_eq!(failed["next_attempt"], 2);
    assert_eq!(failed["attempts"], 2);
    assert_eq!(failed["delay_ms"], 10);
    assert_eq!(events[3]["status"], 200);
}

#[test]
fn attempt_that_could_not_be_sent() {
    let file = write_collection(
        "json-unsent",
        r#"
[down]
url = "http://127.0.0.1:1/search"
retry = 1
retry_delay_ms = 10

[down.query]
q = "toad"
"#,
    );
    let output = toad(&[file.to_str().unwrap(), "-o", "json"]);
    let events = events(&output);
    assert!(!output.status.success(), "{events:#?}");
    assert_eq!(
        kinds(&events),
        [
            "start",
            "request_start",
            "attempt_failed",
            "error",
            "summary"
        ]
    );
    // The URL as sent, with the query
    assert_eq!(events[1]["url"], "http://127.0.0.1:1/search?q=toad");
    let failed = &events[2];
    assert_eq!(failed["status"], Value::Null);
    assert_eq!(failed["elapsed_ms"], Value::Null);
    assert_eq!(failed["headers"], Value::Null);
    assert_eq!(failed["body"], Value::Null);
    // One line, with the cause chain joined by ": "
    let error = events[3]["error"].as_str().unwrap();
    assert!(
        error.starts_with("request 'down' failed to send: "),
        "{error}"
    );
    assert!(error.ends_with("(after 2 attempts)"), "{error}");
}

#[test]
fn failed_run() {
    let base_url = start_server();
    let file = write_collection(
        "json-fail",
        &format!(
            r#"
[first]
url = "{base_url}/login"
method = "POST"

[second]
url = "{base_url}/missing"
expect_status = [200]

[third]
url = "{base_url}/login"
method = "POST"
"#
        ),
    );
    let output = toad(&[file.to_str().unwrap(), "-o", "json"]);
    let events = events(&output);
    assert!(!output.status.success(), "{events:#?}");
    assert_eq!(
        kinds(&events),
        [
            "start",
            "request_start",
            "response",
            "request_start",
            "response",
            "error",
            "summary"
        ]
    );
    assert_eq!(events[4]["status"], 401);
    assert_eq!(
        events[5],
        json!({
            "event": "error",
            "name": "second",
            "error": "request 'second' expected status [200] but got 401"
        })
    );
    assert_eq!(events[6]["passed"], 1, "{}", events[6]);
    assert_eq!(events[6]["failed"], 1);
    assert_eq!(events[6]["not_run"], 1);
}

#[test]
fn undefined_variable_stops_before_sending() {
    let file = write_collection(
        "json-undefined",
        r#"
[one]
url = "http://127.0.0.1:1/{{nope}}"

[two]
url = "http://127.0.0.1:1/"
"#,
    );
    let output = toad(&[file.to_str().unwrap(), "-o", "json"]);
    let events = events(&output);
    assert!(!output.status.success(), "{events:#?}");
    assert_eq!(kinds(&events), ["start", "error", "summary"]);
    assert_eq!(events[1]["name"], "one");
    assert_eq!(events[2]["passed"], 0);
    assert_eq!(events[2]["failed"], 0);
    assert_eq!(events[2]["not_run"], 2);
}

#[test]
fn toad_output_env_var() {
    let file = write_collection(
        "json-env",
        r#"
[one]
url = "http://127.0.0.1:1/{{nope}}"
"#,
    );
    let output = toad_with_env(&[file.to_str().unwrap()], &[("TOAD_OUTPUT", "json")]);
    assert_eq!(kinds(&events(&output)), ["start", "error", "summary"]);
}
