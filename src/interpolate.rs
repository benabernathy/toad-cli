use anyhow::{Result, anyhow};
use std::collections::HashMap;

/// `{{env:NAME}}` reads the environment variable `NAME` instead of a collection variable.
pub const ENV_PREFIX: &str = "env:";

/// Looks up an environment variable. Tests pass their own instead of changing the process
/// environment.
pub type Env = dyn Fn(&str) -> Option<String>;

pub fn system_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Replaces each `{{name}}` in `s` with its value from `vars`, and each `{{env:NAME}}` with the
/// environment variable `NAME`. Values are inserted as-is and never interpolated again. Fails if a
/// name is not defined or an environment variable is not set.
///
/// `\{{` is an escape and produces a literal `{{`. `\\{{name}}` produces a literal backslash
/// followed by the value of `name`.
pub fn interpolate(s: &str, vars: &HashMap<String, String>) -> Result<String> {
    interpolate_with_env(s, vars, &system_env)
}

pub fn interpolate_with_env(s: &str, vars: &HashMap<String, String>, env: &Env) -> Result<String> {
    substitute(s, |name| match name.strip_prefix(ENV_PREFIX) {
        Some(env_name) => env(env_name).ok_or_else(|| env_not_set(env_name)),
        None => vars
            .get(name)
            .cloned()
            .ok_or_else(|| undefined_variable(name)),
    })
}

pub fn undefined_variable(name: &str) -> anyhow::Error {
    anyhow!(
        "undefined variable '{name}' (if this should be sent as literal text, write \\{{{{{name}}}}})"
    )
}

pub fn env_not_set(env_name: &str) -> anyhow::Error {
    anyhow!("environment variable '{env_name}' is not set")
}

/// Replaces only the `{{env:NAME}}` references in `s`, leaving other `{{name}}` references as
/// written. On failure, returns the name of the first environment variable that is not set.
pub fn resolve_env(s: &str, env: &Env) -> std::result::Result<String, String> {
    substitute(s, |name| match name.strip_prefix(ENV_PREFIX) {
        Some(env_name) => env(env_name).ok_or_else(|| env_name.to_string()),
        None => Ok(format!("{{{{{name}}}}}")),
    })
}

/// The names of the `{{name}}` references in `s`, in order, including `env:NAME` ones. Escaped
/// braces are not references.
pub fn references(s: &str) -> Vec<String> {
    let mut names = Vec::new();
    let _ = substitute(s, |name| -> std::result::Result<String, ()> {
        names.push(name.to_string());
        Ok(String::new())
    });
    names
}

/// Calls `value_of` for each `{{name}}` in `s` and inserts what it returns, applying the escapes
/// described on `interpolate`.
fn substitute<E>(
    s: &str,
    mut value_of: impl FnMut(&str) -> std::result::Result<String, E>,
) -> std::result::Result<String, E> {
    let mut result = String::with_capacity(s.len());
    let mut rest = s;

    while let Some(start) = rest.find("{{") {
        let before = &rest[..start];
        let after_open = &rest[start + 2..];

        if before.ends_with("\\\\") {
            // `\\{{name}}`: keep one backslash, then interpolate as usual
            result.push_str(&before[..before.len() - 1]);
        } else if let Some(before) = before.strip_suffix('\\') {
            // `\{{`: literal braces
            result.push_str(before);
            result.push_str("{{");
            rest = after_open;
            continue;
        } else {
            result.push_str(before);
        }

        let Some(end) = after_open.find("}}") else {
            result.push_str("{{");
            rest = after_open;
            break;
        };

        let name = &after_open[..end];
        result.push_str(&value_of(name)?);
        rest = &after_open[end + 2..];
    }

    result.push_str(rest);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn replaces_variables() {
        let v = vars(&[("base_url", "http://x"), ("id", "42")]);
        assert_eq!(
            interpolate("{{base_url}}/users/{{id}}", &v).unwrap(),
            "http://x/users/42"
        );
    }

    #[test]
    fn undefined_variable_is_an_error() {
        let err = interpolate("/users/{{user_id}}", &HashMap::new()).unwrap_err();
        assert_eq!(
            err.to_string(),
            r"undefined variable 'user_id' (if this should be sent as literal text, write \{{user_id}})"
        );
    }

    #[test]
    fn values_are_not_interpolated_again() {
        let v = vars(&[("a", "{{b}}"), ("b", "wrong")]);
        assert_eq!(interpolate("x={{a}}", &v).unwrap(), "x={{b}}");
    }

    #[test]
    fn object_value_in_json_body() {
        let v = vars(&[("address", r#"{"city":"Austin"}"#)]);
        assert_eq!(
            interpolate(r#"{"address": {{address}}}"#, &v).unwrap(),
            r#"{"address": {"city":"Austin"}}"#
        );
    }

    #[test]
    fn unclosed_braces_are_left_alone() {
        assert_eq!(interpolate("a {{ b", &HashMap::new()).unwrap(), "a {{ b");
    }

    #[test]
    fn text_without_variables_is_unchanged() {
        let body = r#"{"a": {"b": 1}}"#;
        assert_eq!(interpolate(body, &HashMap::new()).unwrap(), body);
    }

    #[test]
    fn escaped_braces_are_literal() {
        let v = vars(&[("email", "a@b.c")]);
        assert_eq!(
            interpolate(r"{{email}} Hello \{{first_name}}!", &v).unwrap(),
            "a@b.c Hello {{first_name}}!"
        );
    }

    #[test]
    fn escaped_braces_with_triple_braces() {
        assert_eq!(
            interpolate(r"\{{{html}}}", &HashMap::new()).unwrap(),
            "{{{html}}}"
        );
    }

    #[test]
    fn double_backslash_keeps_one_backslash_and_interpolates() {
        let v = vars(&[("dir", "temp")]);
        assert_eq!(interpolate(r"C:\\{{dir}}", &v).unwrap(), r"C:\temp");
    }

    #[test]
    fn backslash_elsewhere_is_unchanged() {
        let v = vars(&[("id", "42")]);
        assert_eq!(interpolate(r"a\b {{id}} c\", &v).unwrap(), r"a\b 42 c\");
    }

    fn env(name: &str) -> Option<String> {
        match name {
            "API_TOKEN" => Some("tok-123".to_string()),
            "EMPTY" => Some(String::new()),
            "BRACES" => Some("{{user_id}}".to_string()),
            _ => None,
        }
    }

    #[test]
    fn reads_environment_variables() {
        let v = vars(&[("base_url", "http://x")]);
        assert_eq!(
            interpolate_with_env("{{base_url}}?t={{env:API_TOKEN}}", &v, &env).unwrap(),
            "http://x?t=tok-123"
        );
    }

    #[test]
    fn unset_environment_variable_is_an_error() {
        let err = interpolate_with_env("{{env:MISSING}}", &HashMap::new(), &env).unwrap_err();
        assert_eq!(err.to_string(), "environment variable 'MISSING' is not set");
    }

    #[test]
    fn empty_environment_variable_counts_as_set() {
        assert_eq!(
            interpolate_with_env("a{{env:EMPTY}}b", &HashMap::new(), &env).unwrap(),
            "ab"
        );
    }

    #[test]
    fn environment_values_are_not_interpolated_again() {
        let v = vars(&[("user_id", "wrong")]);
        assert_eq!(
            interpolate_with_env("{{env:BRACES}}", &v, &env).unwrap(),
            "{{user_id}}"
        );
    }

    #[test]
    fn escaped_environment_reference_is_literal() {
        assert_eq!(
            interpolate_with_env(r"\{{env:MISSING}}", &HashMap::new(), &env).unwrap(),
            "{{env:MISSING}}"
        );
    }

    #[test]
    fn resolve_env_leaves_other_variables_alone() {
        assert_eq!(
            resolve_env("{{base_url}}/{{env:API_TOKEN}}", &env).unwrap(),
            "{{base_url}}/tok-123"
        );
        assert_eq!(resolve_env("{{env:MISSING}}", &env).unwrap_err(), "MISSING");
    }

    #[test]
    fn references_lists_names_and_skips_escapes() {
        assert_eq!(
            references(r"{{base_url}}/{{env:API_TOKEN}} \{{literal}} {{id}}"),
            ["base_url", "env:API_TOKEN", "id"]
        );
    }
}
