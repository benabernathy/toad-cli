mod common;

use common::{stderr, stdout, toad, write_collection};

#[test]
fn misspelled_setting_stops_before_any_request() {
    let file = write_collection(
        "misspelled",
        r#"
[first]
url = "http://127.0.0.1:1/first"

[second]
url = "http://127.0.0.1:1/second"
expect_stauts = [200]
"#,
    );
    let output = toad(&[file.to_str().unwrap()]);
    let err = stderr(&output);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        err.contains("request 'second': unknown field `expect_stauts`"),
        "{err}"
    );
    // Nothing was sent, not even the first request
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
}
