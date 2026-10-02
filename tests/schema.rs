mod common;

use common::{stderr, stdout, toad};

#[test]
fn schema_flag_prints_the_schema_file() {
    let output = toad(&["--schema"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));

    let expected = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/toad.schema.json"),
    )
    .unwrap();
    assert_eq!(stdout(&output), expected);
}

#[test]
fn schema_flag_cannot_be_used_with_a_collection() {
    let output = toad(&["--schema", "api.toml"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("cannot be used with"),
        "{}",
        stderr(&output)
    );
}
