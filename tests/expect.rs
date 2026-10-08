mod common;

use serde_json::{Value, json};

use common::{start_server, stderr, stdout, toad, write_collection};

fn events(output: &std::process::Output) -> Vec<Value> {
    stdout(output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn kinds(events: &[Value]) -> Vec<&str> {
    events
        .iter()
        .map(|e| e["event"].as_str().unwrap())
        .collect()
}

/// Logs in, creates a user, and reads it back, with `get_user_expect` as the `[get-user.expect]`
/// table.
fn user_flow(base_url: &str, get_user_expect: &str) -> String {
    format!(
        r#"
[config]
auth = "bearer {{{{token}}}}"

[login]
method = "POST"
url = "{base_url}/login"
ignore_config = ["auth"]

[login.capture]
token = "$.access_token"

[create-user]
method = "POST"
url = "{base_url}/users"

[create-user.expect]
"status" = 201
"$.roles" = {{ contains = "editor", length = 2 }}
"header:Location" = {{ starts_with = "/users/" }}

[create-user.capture]
user_id = "$.id"

[get-user]
url = "{base_url}/users/{{{{user_id}}}}"

[get-user.expect]
{get_user_expect}

[get-user.capture]
name = "$.name"
"#
    )
}

#[test]
fn passing_assertions() {
    let base_url = start_server();
    let file = write_collection(
        "expect-pass",
        &user_flow(
            &base_url,
            r#""$.id" = "{{user_id}}"
"$.name" = { equals = "Toad", type = "string" }
"$.deleted_at" = { exists = false }"#,
        ),
    );
    let output = toad(&[file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn failed_assertions_are_listed_and_stop_the_run() {
    let base_url = start_server();
    let file = write_collection(
        "expect-fail",
        &user_flow(
            &base_url,
            r#""$.id" = 43
"$.name" = { type = "string", matches = "^Frog" }"#,
        ),
    );
    let output = toad(&[file.to_str().unwrap(), "-o", "quiet"]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{out}");
    assert!(
        out.contains(
            "get-user -> request 'get-user' failed 2 assertions\n  $.id: expected 43, got 42\n  $.name: expected a match for \"^Frog\", got \"Toad\"\n"
        ),
        "{out}"
    );
}

#[test]
fn json_output_has_an_event_for_each_failed_assertion() {
    let base_url = start_server();
    let file = write_collection(
        "expect-json",
        &user_flow(
            &base_url,
            r#""$.id" = 43
"$.missing" = { exists = true }"#,
        ),
    );
    let output = toad(&[
        file.to_str().unwrap(),
        "-o",
        "json",
        "get-user",
        "--var",
        "user_id=42",
        "--var",
        "token=tok-123",
    ]);
    let events = events(&output);
    assert_eq!(output.status.code(), Some(1), "{events:#?}");
    assert_eq!(
        kinds(&events),
        [
            "start",
            "request_start",
            "response",
            "assertion_failed",
            "assertion_failed",
            "error",
            "summary"
        ]
    );
    assert_eq!(
        events[3],
        json!({
            "event": "assertion_failed",
            "name": "get-user",
            "source": "$.id",
            "check": "equals",
            "expected": 43,
            "actual": 42,
            "message": "expected 43, got 42"
        })
    );
    // No value matched, so there is no `actual`
    assert_eq!(
        events[4],
        json!({
            "event": "assertion_failed",
            "name": "get-user",
            "source": "$.missing",
            "check": "exists",
            "expected": true,
            "message": "expected a value, but nothing matched"
        })
    );
    assert_eq!(
        events[5]["error"],
        "request 'get-user' failed 2 assertions; $.id: expected 43, got 42; $.missing: expected a value, but nothing matched"
    );
}

#[test]
fn assertions_run_before_captures() {
    let base_url = start_server();
    // `$.name` would be captured, but the failed assertion stops the request first
    let file = write_collection(
        "expect-before-capture",
        &user_flow(&base_url, r#""$.id" = 1"#),
    );
    let output = toad(&[file.to_str().unwrap(), "-o", "json"]);
    let events = events(&output);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        !events
            .iter()
            .any(|e| e["event"] == "captured" && e["name"] == "get-user"),
        "{events:#?}"
    );
}

#[test]
fn failed_assertion_is_retried() {
    let base_url = start_server();
    // The first response is `{}`, the second `{"id": 7}`
    let file = write_collection(
        "expect-retry",
        &format!(
            r#"
[flaky]
url = "{base_url}/flaky/empty/expect-retry/1"
retry = 2
retry_delay_ms = 10

[flaky.expect]
"$.id" = 7
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
            "assertion_failed",
            "response",
            "summary"
        ]
    );
    assert_eq!(
        events[2]["error"],
        "request 'flaky' failed 1 assertion; $.id: expected 7, but nothing matched"
    );
}

#[test]
fn last_attempt_says_how_many_attempts_were_made() {
    let base_url = start_server();
    let file = write_collection(
        "expect-retry-fail",
        &format!(
            r#"
[flaky]
url = "{base_url}/flaky/empty/expect-retry-fail/5"
retry = 1
retry_delay_ms = 10

[flaky.expect]
"$.id" = 7
"#
        ),
    );
    let output = toad(&[file.to_str().unwrap(), "-o", "quiet"]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{out}");
    assert!(
        out.contains(
            "flaky -> request 'flaky' failed 1 assertion (after 2 attempts)\n  $.id: expected 7, but nothing matched"
        ),
        "{out}"
    );
}

#[test]
fn undefined_variable_in_expect_is_reported_before_sending() {
    let base_url = start_server();
    let file = write_collection(
        "expect-undefined",
        &format!(
            r#"
[get-user]
url = "{base_url}/users/42"

[get-user.expect]
"$.name" = "{{{{user_name}}}}"
"#
        ),
    );
    let output = toad(&[file.to_str().unwrap(), "-o", "json"]);
    let events = events(&output);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(kinds(&events), ["start", "error", "summary"]);
    assert!(
        events[1]["error"]
            .as_str()
            .unwrap()
            .starts_with("undefined variable 'user_name'"),
        "{events:#?}"
    );
}

#[test]
fn invalid_expect_is_reported_when_the_file_loads() {
    let file = write_collection(
        "expect-invalid",
        r#"
[get-user]
url = "http://127.0.0.1:1/users/42"

[get-user.expect]
"$.name" = { equal = "Toad" }
"#,
    );
    let output = toad(&[file.to_str().unwrap()]);
    let err = stderr(&output);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        err.contains("invalid expect '$.name' in request 'get-user'"),
        "{err}"
    );
    assert!(
        err.contains("unknown check 'equal' (did you mean 'equals'?)"),
        "{err}"
    );
}
