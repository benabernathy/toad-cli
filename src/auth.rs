use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

/// Builds an `Authorization` header value from an `auth` shorthand string, e.g.
/// `"bearer abc123"` -> `"Bearer abc123"` or `"basic user:pass"` -> `"Basic dXNlcjpwYXNz"`.
///
/// `auth` is expected to already have `{{var}}` interpolation applied.
pub fn build_authorization_value(auth: &str) -> Result<String> {
    let (scheme, rest) = auth.trim_start().split_once(char::is_whitespace).ok_or_else(|| {
        anyhow!(
            "auth value '{}' must be in the form '<scheme> <credential>' (e.g. 'bearer {{{{token}}}}')",
            auth
        )
    })?;

    let credential = rest.trim();

    match scheme.to_lowercase().as_str() {
        "bearer" => {
            if credential.is_empty() {
                return Err(anyhow!("bearer auth is missing a token"));
            }
            Ok(format!("Bearer {credential}"))
        }
        "basic" => {
            if credential.is_empty() {
                return Err(anyhow!(
                    "basic auth is missing credentials (expected 'user:pass')"
                ));
            }
            Ok(format!("Basic {}", BASE64.encode(credential)))
        }
        other => Err(anyhow!(
            "unsupported auth scheme '{}' - supported schemes: bearer, basic",
            other
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_happy_path() {
        let value = build_authorization_value("bearer abc123").unwrap();
        assert_eq!(value, "Bearer abc123");
    }

    #[test]
    fn bearer_is_case_insensitive() {
        let value = build_authorization_value("BEARER abc123").unwrap();
        assert_eq!(value, "Bearer abc123");
    }

    #[test]
    fn basic_happy_path() {
        let value = build_authorization_value("basic user:pass").unwrap();
        assert_eq!(value, "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn unknown_scheme_errors() {
        let err = build_authorization_value("digest abc123").unwrap_err();
        assert!(err.to_string().contains("unsupported auth scheme"));
    }

    #[test]
    fn missing_bearer_token_errors() {
        let err = build_authorization_value("bearer ").unwrap_err();
        assert!(err.to_string().contains("missing a token"));
    }

    #[test]
    fn missing_basic_credential_errors() {
        let err = build_authorization_value("basic ").unwrap_err();
        assert!(err.to_string().contains("missing credentials"));
    }

    #[test]
    fn missing_separator_errors() {
        let err = build_authorization_value("bearer").unwrap_err();
        assert!(err.to_string().contains("must be in the form"));
    }
}
