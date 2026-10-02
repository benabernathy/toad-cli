mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{start_server, stderr, stdout, toad, toad_with_env, write_collection};

/// A collection with one request, `flaky`, that fails `n` times in the given mode before
/// succeeding. `config` and `request` are extra lines for `[config]` and `[flaky]`.
fn flaky_collection(test_name: &str, mode: &str, n: u32, config: &str, request: &str) -> PathBuf {
    let base_url = start_server();
    write_collection(
        test_name,
        &format!(
            r#"
[config]
retry_delay_ms = 10
{config}

[flaky]
url = "{base_url}/flaky/{mode}/{test_name}/{n}"
expect_status = [200]
{request}
"#
        ),
    )
}

#[test]
fn succeeds_after_retries() {
    let file = flaky_collection("succeeds", "status", 2, "retry = 2", "");
    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert!(output.status.success(), "{out}");
    assert!(out.contains("retrying 'flaky' in 10ms (attempt 2 of 3): request 'flaky' expected status [200] but got 503"), "{out}");
    assert!(
        out.contains("retrying 'flaky' in 10ms (attempt 3 of 3)"),
        "{out}"
    );
    assert_eq!(out.matches("[flaky] 503").count(), 2, "{out}");
    assert!(out.contains("[flaky] 200"), "{out}");
}

#[test]
fn fails_after_last_attempt() {
    let file = flaky_collection("exhausted", "status", 5, "retry = 1", "");
    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(
        out.contains(
            "flaky -> request 'flaky' expected status [200] but got 503 (after 2 attempts)"
        ),
        "{out}"
    );
}

#[test]
fn single_attempt_error_is_unchanged() {
    let file = flaky_collection("no-retry", "status", 5, "", "");
    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(!out.contains("retrying"), "{out}");
    assert!(!out.contains("attempts"), "{out}");
}

#[test]
fn errors_before_sending_are_not_retried() {
    let file = write_collection(
        "pre-send",
        r#"
[config]
retry = 5
retry_delay_ms = 2000

[broken]
url = "http://127.0.0.1:1/{{undefined}}"
"#,
    );
    let start = Instant::now();
    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(out.contains("undefined variable 'undefined'"), "{out}");
    assert!(!out.contains("retrying"), "{out}");
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn connection_failures_are_retried() {
    let file = write_collection(
        "refused",
        r#"
[config]
retry = 1
retry_delay_ms = 10

[refused]
url = "http://127.0.0.1:1/"
"#,
    );
    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(
        out.contains(
            "retrying 'refused' in 10ms (attempt 2 of 2): request 'refused' failed to send"
        ),
        "{out}"
    );
    assert!(out.contains("(after 2 attempts)"), "{out}");
}

#[test]
fn slow_responses_are_retried() {
    let file = flaky_collection("slow", "slow", 1, "retry = 1", "expect_max_ms = 100");
    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert!(output.status.success(), "{out}");
    assert!(out.contains("expected at most 100ms"), "{out}");
}

#[test]
fn capture_failures_are_retried() {
    let file = flaky_collection(
        "capture",
        "empty",
        2,
        "retry = 2",
        "\n[flaky.capture]\nid = \"$.id\"",
    );
    let output = toad(&[file.to_str().unwrap(), "-o", "verbose"]);
    let out = stdout(&output);
    assert!(output.status.success(), "{out}");
    assert!(
        out.contains("capture 'id': no value matched '$.id'"),
        "{out}"
    );
    assert_eq!(out.matches("captured:").count(), 1, "{out}");
    assert!(out.contains("id = 7"), "{out}");
}

#[test]
fn request_value_overrides_config() {
    let file = flaky_collection("override", "status", 2, "retry = 0", "retry = 2");
    let output = toad(&[file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn retry_zero_turns_retries_off() {
    let file = flaky_collection("zero", "status", 1, "retry = 3", "retry = 0");
    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(!out.contains("retrying"), "{out}");
}

#[test]
fn ignore_config_turns_retries_off() {
    let file = flaky_collection(
        "ignore",
        "status",
        1,
        "retry = 3",
        r#"ignore_config = ["retry"]"#,
    );
    let output = toad(&[file.to_str().unwrap()]);
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(!out.contains("retrying"), "{out}");
}

#[test]
fn delay_is_waited_out() {
    let file = flaky_collection("delay", "status", 2, "retry = 2", "retry_delay_ms = 200");
    let start = Instant::now();
    let output = toad(&[file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stdout(&output));
    assert!(start.elapsed() >= Duration::from_millis(400));
    assert!(stdout(&output).contains("retrying 'flaky' in 200ms"));
}

#[test]
fn response_only_prints_only_the_final_body() {
    let file = flaky_collection("response-only", "status", 1, "retry = 1", "");
    let output = toad(&[file.to_str().unwrap(), "-o", "response-only"]);
    assert!(output.status.success());
    assert_eq!(stdout(&output).trim(), "{\n  \"id\": 7\n}");
}

#[test]
fn quiet_shows_retry_lines() {
    let file = flaky_collection("quiet", "status", 1, "retry = 1", "");
    let output = toad(&[file.to_str().unwrap(), "-o", "quiet"]);
    let out = stdout(&output);
    assert!(output.status.success(), "{out}");
    assert!(
        out.contains("retrying 'flaky' in 10ms (attempt 2 of 2)"),
        "{out}"
    );
    assert!(!out.contains("unavailable"), "{out}");
}

#[test]
fn flag_replaces_config_default() {
    let file = flaky_collection("flag-default", "status", 2, "", "");
    let output = toad(&[file.to_str().unwrap(), "--retry", "2"]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn flag_does_not_override_request_value() {
    let file = flaky_collection("flag-request", "status", 1, "", "retry = 0");
    let output = toad(&[file.to_str().unwrap(), "--retry", "3"]);
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(!out.contains("retrying"), "{out}");
}

#[test]
fn flag_does_not_apply_when_request_ignores_config() {
    let file = flaky_collection(
        "flag-ignore",
        "status",
        1,
        "",
        r#"ignore_config = ["retry"]"#,
    );
    let output = toad(&[file.to_str().unwrap(), "--retry", "3"]);
    assert!(!output.status.success());
    assert!(!stdout(&output).contains("retrying"));
}

#[test]
fn flag_off_disables_request_retries() {
    let file = flaky_collection("flag-off", "status", 1, "", "retry = 3");
    let output = toad(&[file.to_str().unwrap(), "--retry", "off"]);
    let out = stdout(&output);
    assert!(!output.status.success(), "{out}");
    assert!(!out.contains("retrying"), "{out}");
}

#[test]
fn env_sets_default() {
    let file = flaky_collection("env", "status", 2, "", "");
    let output = toad_with_env(&[file.to_str().unwrap()], &[("TOAD_RETRY", "2")]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn flag_wins_over_env() {
    let file = flaky_collection("flag-wins", "status", 2, "", "");
    let output = toad_with_env(
        &[file.to_str().unwrap(), "--retry", "2"],
        &[("TOAD_RETRY", "off")],
    );
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn invalid_env_warns_and_is_ignored() {
    let file = flaky_collection("bad-env", "status", 1, "retry = 1", "");
    let output = toad_with_env(&[file.to_str().unwrap()], &[("TOAD_RETRY", "lots")]);
    assert!(output.status.success(), "{}", stdout(&output));
    assert!(
        stderr(&output)
            .contains("unknown TOAD_RETRY value: 'lots', using the collection's retry settings")
    );
}

#[test]
fn invalid_flag_is_rejected() {
    let file = flaky_collection("bad-flag", "status", 1, "", "");
    let output = toad(&[file.to_str().unwrap(), "--retry", "1.5"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("not a valid retry setting"),
        "{}",
        stderr(&output)
    );
}
