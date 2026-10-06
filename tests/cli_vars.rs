mod common;

use common::{start_server, stderr, stdout, toad, write_collection};
use serde_json::{Value, json};

// No test sets TOAD_TEST_UNSET, so it stands for a variable missing from the environment.

/// A collection whose `echo` request sends `{"id": "<id>"}` to `/echo`, which returns it unchanged.
/// `create-user` captures `id` (42) from the response. `echo-token` sends `{"token": "<token>"}`, and
/// `token` reads an environment variable that is not set.
fn collection(test_name: &str) -> String {
    let base_url = start_server();
    let file = write_collection(
        test_name,
        &format!(
            r#"
[vars]
base_url = "{base_url}"
id = "from-vars"
token = "{{{{env:TOAD_TEST_UNSET}}}}"

[profiles.ci]
id = "from-profile"

[create-user]
method = "POST"
url = "{{{{base_url}}}}/users"
auth = "bearer tok-123"
expect_status = [201]

[create-user.capture]
id = "$.id"

[echo]
method = "POST"
url = "{{{{base_url}}}}/echo"
body = '{{"id": "{{{{id}}}}"}}'

[echo-token]
method = "POST"
url = "{{{{base_url}}}}/echo"
body = '{{"token": "{{{{token}}}}"}}'
"#
        ),
    );
    file.to_str().unwrap().to_string()
}

/// The response bodies printed by `-o response-only`.
fn responses(output: &std::process::Output) -> Vec<Value> {
    serde_json::Deserializer::from_str(&stdout(output))
        .into_iter()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn var_replaces_vars_value() {
    let file = collection("cli-var-vars");
    let output = toad(&[&file, "echo", "-o", "response-only", "--var", "id=7"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(responses(&output), [json!({"id": "7"})]);
}

#[test]
fn var_replaces_profile_value() {
    let file = collection("cli-var-profile");
    let output = toad(&[
        &file,
        "echo",
        "-o",
        "response-only",
        "--profile",
        "ci",
        "--var",
        "id=7",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(responses(&output), [json!({"id": "7"})]);
}

#[test]
fn var_replaces_captured_value() {
    let file = collection("cli-var-capture");
    let output = toad(&[
        &file,
        "-o",
        "response-only",
        "--var",
        "id=7",
        "--var",
        "token=tok-123",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        responses(&output),
        [
            json!({"id": 42, "roles": ["reader", "editor"]}),
            json!({"id": "7"}),
            json!({"token": "tok-123"})
        ]
    );
}

#[test]
fn var_replaces_value_from_the_environment() {
    let file = collection("cli-var-env");
    let output = toad(&[
        &file,
        "echo-token",
        "-o",
        "response-only",
        "--var",
        "token=tok-123",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(responses(&output), [json!({"token": "tok-123"})]);
}

#[test]
fn var_value_is_not_interpolated() {
    let file = collection("cli-var-literal");
    let output = toad(&[
        &file,
        "echo",
        "-o",
        "response-only",
        "--var",
        "id={{base_url}}",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(responses(&output), [json!({"id": "{{base_url}}"})]);
}

#[test]
fn var_can_contain_equals_signs_and_commas() {
    let file = collection("cli-var-commas");
    let output = toad(&[&file, "echo", "-o", "response-only", "--var", "id=a=b,c"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(responses(&output), [json!({"id": "a=b,c"})]);
}

#[test]
fn last_var_wins() {
    let file = collection("cli-var-repeat");
    let output = toad(&[
        &file,
        "echo",
        "-o",
        "response-only",
        "--var",
        "id=1",
        "--var",
        "id=2",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(responses(&output), [json!({"id": "2"})]);
}

#[test]
fn undeclared_var_stops_before_any_request() {
    let file = collection("cli-var-undeclared");
    let output = toad(&[&file, "--var", "ids=7"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains(
            "--var 'ids': no variable named 'ids' in [vars], a profile, or a capture (did you mean 'id'?)"
        ),
        "{}",
        stderr(&output)
    );
}

#[test]
fn var_without_equals_is_a_usage_error() {
    let file = collection("cli-var-no-equals");
    let output = toad(&[&file, "--var", "id"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("'id' must be NAME=VALUE"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn verbose_output_notes_a_replaced_capture() {
    let file = collection("cli-var-verbose");
    let output = toad(&[&file, "create-user", "-o", "verbose", "--var", "id=7"]);
    let out = stdout(&output);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(out.contains("id = 42 (replaced by --var id=7)"), "{out}");
}

#[test]
fn json_captured_event_lists_replaced_values() {
    let file = collection("cli-var-json");
    let output = toad(&[&file, "create-user", "-o", "json", "--var", "id=7"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let captured: Vec<Value> = stdout(&output)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|e| e["event"] == "captured")
        .collect();
    assert_eq!(
        captured,
        [json!({
            "event": "captured",
            "name": "create-user",
            "values": {"id": "42"},
            "replaced": {"id": "7"}
        })]
    );
}
