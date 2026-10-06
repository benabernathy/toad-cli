//! The JSON Schema for collection files, at `schema/toad.schema.json`.
//!
//! The schema is generated from `Config` and `RequestDef` by a test, so it can't drift from what
//! toad accepts. After changing either struct, regenerate it with:
//!
//! ```text
//! UPDATE_SCHEMA=1 cargo test schema
//! ```
//!
//! The schema must never reject a file toad accepts. Many settings can contain `{{var}}`, so
//! string settings only get a pattern when toad itself checks the value without interpolating it.

/// The schema printed by `toad --schema`, so it always matches the installed version.
pub const SCHEMA: &str = include_str!("../schema/toad.schema.json");

#[cfg(test)]
use crate::collection::{CONFIG_KEYS, Config, RequestDef};
#[cfg(test)]
use schemars::{Schema, SchemaGenerator, generate::SchemaSettings, json_schema};
#[cfg(test)]
use serde_json::{Value, json};

/// Common methods are offered as completions, but toad sends any method.
#[cfg(test)]
pub fn method(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "string",
        "anyOf": [
            { "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] },
            { "type": "string" }
        ]
    })
}

#[cfg(test)]
pub fn status_codes(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "array",
        "items": { "type": "integer", "minimum": 100, "maximum": 599 }
    })
}

/// Matches the checks in `Capture::parse`.
#[cfg(test)]
pub fn captures(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "object",
        "propertyNames": { "pattern": "^[^\\s{}]+$", "not": { "pattern": "^env:" } },
        "additionalProperties": {
            "type": "string",
            "pattern": "^(\\$.*|header:.+|status|body)$"
        }
    })
}

#[cfg(test)]
pub fn config_keys(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "array",
        "items": { "enum": CONFIG_KEYS }
    })
}

/// Builds the schema for a whole collection file. The top level is written out here because
/// `RawRequestFile` keeps requests as raw TOML values: `config`, `vars`, and `profiles` are fixed,
/// and every other table is a request.
#[cfg(test)]
fn generate() -> Value {
    let mut generator = SchemaSettings::draft07().into_generator();
    let config = generator.subschema_for::<Config>();
    let request = generator.subschema_for::<RequestDef>();
    let definitions = generator.take_definitions(true);
    // Matches `variables::validate_names`
    let not_env = json!({ "not": { "pattern": "^env:" } });

    let mut schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "toad collection",
        "description": "A toad collection file. See https://github.com/benabernathy/toad-cli",
        "type": "object",
        "properties": {
            "config": config,
            "vars": {
                "description": "Variables used as {{name}} in requests.",
                "type": "object",
                "propertyNames": not_env,
                "additionalProperties": { "type": "string" }
            },
            "profiles": {
                "description": "Named sets of variables, chosen with --profile. A profile's values replace the ones in [vars].",
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "propertyNames": not_env,
                    "additionalProperties": { "type": "string" }
                }
            }
        },
        "additionalProperties": request,
        "definitions": definitions
    });
    remove_null_types(&mut schema);
    schema
}

/// schemars describes `Option<T>` as "T or null". TOML has no null, so a missing setting is the
/// only way to leave one out, and editors shouldn't offer null as a value.
#[cfg(test)]
fn remove_null_types(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(types)) = map.get_mut("type") {
                types.retain(|t| t != "null");
                if types.len() == 1 {
                    let only = types.remove(0);
                    map.insert("type".into(), only);
                }
            }
            if map.get("default") == Some(&Value::Null) {
                map.remove("default");
            }
            map.values_mut().for_each(remove_null_types);
        }
        Value::Array(items) => items.iter_mut().for_each(remove_null_types),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{RequestFile, parse_captures, validate_ignore_config, validate_order};
    use crate::variables::validate_names;
    use std::{fs, path::PathBuf};

    fn schema_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schema/toad.schema.json")
    }

    /// Schema errors for a collection, as "path: message" lines. Empty when it is valid.
    fn schema_errors(collection: &str) -> Vec<String> {
        let validator = jsonschema::validator_for(&generate()).unwrap();
        let instance: Value = toml::from_str(collection).unwrap();
        validator
            .iter_errors(&instance)
            .map(|e| format!("{}: {}", e.instance_path(), e))
            .collect()
    }

    /// Whether toad accepts a collection, checking everything it checks before sending a request
    /// that doesn't need other files.
    fn toad_accepts(collection: &str) -> bool {
        let Ok(mut rf) = RequestFile::parse(collection) else {
            return false;
        };
        parse_captures(&mut rf).is_ok()
            && validate_ignore_config(&rf).is_ok()
            && validate_names(&rf).is_ok()
            && validate_order(&rf).is_ok()
    }
    #[test]
    fn schema_file_is_up_to_date() {
        let generated = serde_json::to_string_pretty(&generate()).unwrap() + "\n";
        let path = schema_path();

        if std::env::var_os("UPDATE_SCHEMA").is_some() {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &generated).unwrap();
            return;
        }

        let on_disk = fs::read_to_string(&path).unwrap_or_default();
        assert!(
            on_disk == generated,
            "{} is out of date. Regenerate it with: UPDATE_SCHEMA=1 cargo test schema",
            path.display()
        );
    }

    #[test]
    fn example_collections_are_valid() {
        let doc = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("doc");
        for entry in fs::read_dir(doc).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "toml") {
                let errors = schema_errors(&fs::read_to_string(&path).unwrap());
                assert!(errors.is_empty(), "{}: {errors:#?}", path.display());
            }
        }
    }

    /// The schema must flag what toad rejects, and must never flag what toad accepts.
    #[test]
    fn schema_agrees_with_toad() {
        let cases = [
            // accepted
            r#"[r]
url = "http://x""#,
            r#"[r]
url = "{{base_url}}/users"
method = "post"
auth = "{{auth}}"
expect_status = [200, 404]
ignore_config = ["auth", "retry"]
[r.capture]
id = "$.id"
token = "header: X-Token"
code = "status"
raw = "body"
[r.headers]
Accept = "application/json""#,
            r#"[config]
auth = "bearer {{token}}"
retry = 2
[vars]
token = "abc"
[profiles.ci]
token = "def"
[r]
url = "http://x"
method = "PROPFIND""#,
            r#"[config]
order = ["login", "r", "r"]
[login]
url = "http://x/login"
[r]
url = "http://x""#,
            r#"[vars]
token = "{{env:API_TOKEN}}"
[profiles.ci]
token = "{{env:CI_TOKEN}}"
[r]
url = "http://x"
auth = "bearer {{env:API_TOKEN}}""#,
            // rejected
            r#"[r]
url = "http://x"
expect_stauts = [200]"#,
            r#"[config]
retyr = 3"#,
            r#"[r]
method = "GET""#,
            r#"[r]
url = "http://x"
ignore_config = ["auht"]"#,
            r#"[r]
url = "http://x"
[r.capture]
id = "id""#,
            r#"[r]
url = "http://x"
[r.capture]
"my id" = "$.id""#,
            r#"[vars]
user_id = 1"#,
            r#"[config]
order = []
[r]
url = "http://x""#,
            r#"[config]
order = "r"
[r]
url = "http://x""#,
            r#"[vars]
"env:HOME" = "x""#,
            r#"[profiles.ci]
"env:HOME" = "x""#,
            r#"[r]
url = "http://x"
[r.capture]
"env:id" = "$.id""#,
            r#"[r]
url = "http://x"
timeout_secs = "30""#,
        ];
        for case in cases {
            let errors = schema_errors(case);
            assert_eq!(
                errors.is_empty(),
                toad_accepts(case),
                "schema and toad disagree on:\n{case}\nschema errors: {errors:#?}"
            );
        }
    }
}
