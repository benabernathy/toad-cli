mod common;

use common::{start_server, stderr, stdout, toad, toad_with_env, write_collection};

// No test sets TOAD_TEST_UNSET, so it stands for a variable missing from the environment.

#[test]
fn vars_value_reads_the_environment() {
    let base_url = start_server();
    let file = write_collection(
        "env-in-vars",
        &format!(
            r#"
[vars]
token = "{{{{env:TOAD_TEST_TOKEN}}}}"

[get-user]
url = "{base_url}/users/42"
auth = "bearer {{{{token}}}}"
expect_status = [200]
"#
        ),
    );

    let output = toad_with_env(&[file.to_str().unwrap()], &[("TOAD_TEST_TOKEN", "tok-123")]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn request_reads_the_environment_directly() {
    let base_url = start_server();
    let file = write_collection(
        "env-in-header",
        &format!(
            r#"
[get-user]
url = "{base_url}/users/42"
expect_status = [200]

[get-user.headers]
Authorization = "Bearer {{{{env:TOAD_TEST_TOKEN}}}}"
"#
        ),
    );

    let output = toad_with_env(&[file.to_str().unwrap()], &[("TOAD_TEST_TOKEN", "tok-123")]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn profile_value_reads_the_environment() {
    let base_url = start_server();
    let file = write_collection(
        "env-in-profile",
        &format!(
            r#"
[vars]
token = "wrong"

[profiles.ci]
token = "{{{{env:TOAD_TEST_TOKEN}}}}"

[get-user]
url = "{base_url}/users/42"
auth = "bearer {{{{token}}}}"
expect_status = [200]
"#
        ),
    );

    let output = toad_with_env(
        &[file.to_str().unwrap(), "--profile", "ci"],
        &[("TOAD_TEST_TOKEN", "tok-123")],
    );
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn unset_variable_stops_the_run_before_any_request() {
    let base_url = start_server();
    let file = write_collection(
        "env-unset",
        &format!(
            r#"
[vars]
token = "{{{{env:TOAD_TEST_UNSET}}}}"

[create-user]
method = "POST"
url = "{base_url}/users"

[get-user]
url = "{base_url}/users/42"
auth = "bearer {{{{token}}}}"
"#
        ),
    );

    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{out}");
    assert_eq!(
        out,
        "get-user -> environment variable 'TOAD_TEST_UNSET' is not set (used by {{token}})\n"
    );
}

#[test]
fn unset_variable_used_directly_names_the_variable() {
    let file = write_collection(
        "env-unset-direct",
        r#"
[get-user]
url = "http://127.0.0.1:1/users?key={{env:TOAD_TEST_UNSET}}"
"#,
    );

    let output = toad(&[file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        "get-user -> environment variable 'TOAD_TEST_UNSET' is not set\n"
    );
}

#[test]
fn unset_variable_only_matters_to_requests_that_run() {
    let base_url = start_server();
    let file = write_collection(
        "env-not-run",
        &format!(
            r#"
[vars]
token = "{{{{env:TOAD_TEST_UNSET}}}}"

[login]
method = "POST"
url = "{base_url}/login"
expect_status = [200]

[get-user]
url = "{base_url}/users/42"
auth = "bearer {{{{token}}}}"
"#
        ),
    );

    let output = toad(&[file.to_str().unwrap(), "login"]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn captured_value_replaces_an_unset_variable() {
    let base_url = start_server();
    let file = write_collection(
        "env-captured",
        &format!(
            r#"
[vars]
token = "{{{{env:TOAD_TEST_UNSET}}}}"

[login]
method = "POST"
url = "{base_url}/login"

[login.capture]
token = "$.access_token"

[get-user]
url = "{base_url}/users/42"
auth = "bearer {{{{token}}}}"
expect_status = [200]
"#
        ),
    );

    let output = toad(&[file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn undefined_variable_stops_the_run_before_any_request() {
    let base_url = start_server();
    let file = write_collection(
        "undefined-before-run",
        &format!(
            r#"
[create-user]
method = "POST"
url = "{base_url}/users"

[get-user]
url = "{base_url}/users/{{{{user_id}}}}"
"#
        ),
    );

    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{out}");
    // create-user was never sent
    assert_eq!(
        out,
        "get-user -> undefined variable 'user_id' (if this should be sent as literal text, write \\{{user_id}})\n"
    );
}

#[test]
fn every_problem_is_reported() {
    let file = write_collection(
        "every-problem",
        r#"
[first]
url = "http://127.0.0.1:1/{{env:TOAD_TEST_UNSET}}/{{a}}"

[second]
url = "http://127.0.0.1:1/{{b}}"
"#,
    );

    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert_eq!(out.lines().count(), 3, "{out}");
    assert!(
        out.contains("first -> environment variable 'TOAD_TEST_UNSET'"),
        "{out}"
    );
    assert!(out.contains("first -> undefined variable 'a'"), "{out}");
    assert!(out.contains("second -> undefined variable 'b'"), "{out}");
}

#[test]
fn env_prefix_is_reserved_in_vars() {
    let file = write_collection(
        "reserved-vars",
        r#"
[vars]
"env:HOME" = "x"

[r]
url = "http://127.0.0.1:1/"
"#,
    );

    let output = toad(&[file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("[vars] 'env:HOME': names starting with 'env:' are reserved"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn env_prefix_is_reserved_in_captures() {
    let file = write_collection(
        "reserved-capture",
        r#"
[r]
url = "http://127.0.0.1:1/"

[r.capture]
"env:ID" = "$.id"
"#,
    );

    let output = toad(&[file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("names starting with 'env:' are reserved"),
        "{}",
        stderr(&output)
    );
}
