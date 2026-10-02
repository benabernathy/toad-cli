mod common;

use common::{stdout, toad};

/// The tutorial's finished collection must load (vars, captures, ignore_config, and so on) and
/// list its requests. This doesn't send anything, so it doesn't need network access.
#[test]
fn tutorial_collection_loads() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/doc/tutorial.toml");
    let output = toad(&[path, "-l"]);
    let out = stdout(&output);
    assert!(output.status.success(), "{out}");
    let names: Vec<&str> = out.lines().map(str::trim).collect();
    assert_eq!(
        names,
        [
            "get-post",
            "get-author",
            "list-user-posts",
            "create-post",
            "missing-user"
        ]
    );
}
