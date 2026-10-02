use anyhow::{Result, anyhow};
use std::collections::HashMap;

/// Replaces each `{{name}}` in `s` with its value from `vars`. Values are inserted as-is and never
/// interpolated again. Fails if a name is not defined.
///
/// `\{{` is an escape and produces a literal `{{`. `\\{{name}}` produces a literal backslash
/// followed by the value of `name`.
pub fn interpolate(s: &str, vars: &HashMap<String, String>) -> Result<String> {
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
        let value = vars.get(name).ok_or_else(|| {
            anyhow!(
                "undefined variable '{name}' (if this should be sent as literal text, write \\{{{{{name}}}}})"
            )
        })?;

        result.push_str(value);
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
}
