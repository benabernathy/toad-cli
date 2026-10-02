mod common;

use std::path::PathBuf;

use common::{start_server, stderr, stdout, toad, toad_with_env, write_collection};

/// A collection with one request, `slow`, against the 300ms `/slow` route.
fn slow_collection(test_name: &str, config: &str, request: &str) -> PathBuf {
    let base_url = start_server();
    write_collection(
        test_name,
        &format!(
            r#"
[config]
{config}

[slow]
url = "{base_url}/slow"
{request}
"#
        ),
    )
}

#[test]
fn over_limit_fails() {
    let file = slow_collection("over", "", "expect_max_ms = 100");
    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(out.contains("request 'slow' took "), "{out}");
    assert!(out.contains("ms, expected at most 100ms"), "{out}");
}

#[test]
fn under_limit_passes() {
    let file = slow_collection("under", "", "expect_max_ms = 5000");
    let output = toad(&[file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn time_includes_body_download() {
    let base_url = start_server();
    let file = write_collection(
        "slow-body",
        &format!(
            r#"
[slow-body]
url = "{base_url}/slow-body"
expect_max_ms = 100
"#
        ),
    );
    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(out.contains("expected at most 100ms"), "{out}");
}

#[test]
fn config_default_applies() {
    let file = slow_collection("config-default", "expect_max_ms = 100", "");
    let output = toad(&[file.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(stdout(&output).contains("expected at most 100ms"));
}

#[test]
fn request_value_overrides_config() {
    let file = slow_collection("override", "expect_max_ms = 100", "expect_max_ms = 5000");
    let output = toad(&[file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn ignore_config_skips_default() {
    let file = slow_collection(
        "ignore",
        "expect_max_ms = 100",
        r#"ignore_config = ["expect_max_ms"]"#,
    );
    let output = toad(&[file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn slow_response_skips_captures() {
    let file = slow_collection(
        "captures",
        "",
        "expect_max_ms = 100\n\n[slow.capture]\nid = \"$.id\"",
    );
    let output = toad(&[file.to_str().unwrap(), "-o", "verbose"]);
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(!out.contains("captured:"), "{out}");
}

#[test]
fn env_scale_relaxes_limits() {
    let file = slow_collection("env-scale", "", "expect_max_ms = 100");
    let output = toad_with_env(&[file.to_str().unwrap()], &[("TOAD_TIME_SCALE", "10")]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn failure_message_names_the_scale_source() {
    let file = slow_collection("scale-message", "", "expect_max_ms = 100");
    let output = toad_with_env(&[file.to_str().unwrap()], &[("TOAD_TIME_SCALE", "2")]);
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(
        out.contains("expected at most 200ms (100ms x 2 from TOAD_TIME_SCALE)"),
        "{out}"
    );
}

#[test]
fn flag_wins_over_env() {
    let file = slow_collection("flag-wins", "", "expect_max_ms = 100");
    let output = toad_with_env(
        &[file.to_str().unwrap(), "--time-scale", "1"],
        &[("TOAD_TIME_SCALE", "10")],
    );
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(out.contains("expected at most 100ms"), "{out}");
    assert!(!out.contains("from"), "{out}");
}

#[test]
fn flag_off_skips_limits() {
    let file = slow_collection("flag-off", "", "expect_max_ms = 100");
    let output = toad(&[file.to_str().unwrap(), "--time-scale", "off"]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn invalid_env_warns_and_uses_1() {
    let file = slow_collection("bad-env", "", "expect_max_ms = 100");
    let output = toad_with_env(&[file.to_str().unwrap()], &[("TOAD_TIME_SCALE", "abc")]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("unknown TOAD_TIME_SCALE value: 'abc', using 1"));
    assert!(stdout(&output).contains("expected at most 100ms"));
}

#[test]
fn invalid_flag_is_rejected() {
    let file = slow_collection("bad-flag", "", "expect_max_ms = 100");
    let output = toad(&[file.to_str().unwrap(), "--time-scale", "0"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("not a valid time scale"));
}

#[test]
fn unreachable_limit_warns_and_still_runs() {
    let file = slow_collection("unreachable", "", "expect_max_ms = 40000");
    let output = toad(&[file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stdout(&output));
    assert!(stderr(&output).contains(
        "warning: request 'slow' expects at most 40000ms, but timeout_secs = 30 will stop it first"
    ));
}

#[test]
fn toad_output_response_only_is_recognized() {
    let file = slow_collection("response-only", "", "");
    let output = toad_with_env(
        &[file.to_str().unwrap()],
        &[("TOAD_OUTPUT", "response-only")],
    );
    assert!(output.status.success());
    assert!(!stderr(&output).contains("unknown TOAD_OUTPUT"));
    assert_eq!(stdout(&output).trim(), "{\n  \"id\": 1\n}");
}
