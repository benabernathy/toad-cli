mod common;

use common::{start_server, stderr, stdout, toad, write_collection};
use serde_json::{Value, json};

/// The response bodies printed by `-o response-only`.
fn responses(output: &std::process::Output) -> Vec<Value> {
    serde_json::Deserializer::from_str(&stdout(output))
        .into_iter()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// `bump` sends `n` with an "x" added to `/echo` and captures the result back into `n`, so each
/// run sees the value the run before it captured. `reset` and `skipped` send fixed values.
fn collection(test_name: &str, order: &str) -> String {
    let base_url = start_server();
    let file = write_collection(
        test_name,
        &format!(
            r#"
[config]
order = {order}

[vars]
n = ""

[skipped]
method = "POST"
url = "{base_url}/echo"
body = '{{"skipped": true}}'

[bump]
method = "POST"
url = "{base_url}/echo"
body = '{{"n": "{{{{n}}}}x"}}'

[bump.capture]
n = "$.n"

[reset]
method = "POST"
url = "{base_url}/echo"
body = '{{"n": ""}}'

[reset.capture]
n = "$.n"
"#
        ),
    );
    file.to_str().unwrap().to_string()
}

#[test]
fn repeated_request_uses_values_captured_by_the_run_before() {
    let file = collection(
        "order-repeat",
        r#"["bump", "bump", "reset", "bump", "bump", "bump"]"#,
    );
    let output = toad(&[&file, "-o", "response-only"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        responses(&output),
        [
            json!({"n": "x"}),
            json!({"n": "xx"}),
            json!({"n": ""}),
            json!({"n": "x"}),
            json!({"n": "xx"}),
            json!({"n": "xxx"}),
        ]
    );
}

#[test]
fn list_requests_shows_the_run_order() {
    let file = collection("order-list", r#"["reset", "bump", "bump"]"#);
    let output = toad(&[&file, "-l"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "\treset\n\tbump\n\tbump\n");
}

#[test]
fn named_request_runs_even_if_not_in_order() {
    let file = collection("order-named", r#"["bump"]"#);
    let output = toad(&[&file, "skipped", "-o", "response-only"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(responses(&output), [json!({"skipped": true})]);
}

#[test]
fn unknown_request_in_order_stops_before_any_request() {
    let file = collection("order-unknown", r#"["bump", "bmup"]"#);
    let output = toad(&[&file]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains("[config] order: no request named 'bmup' (did you mean 'bump'?)"),
        "{}",
        stderr(&output)
    );
}
