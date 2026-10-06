//! Resolves `{{env:NAME}}` in `[vars]`, and checks that every variable a run uses is defined
//! before any request is sent.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};

use crate::collection::{Config, RequestDef, RequestFile, edit_distance};
use crate::interpolate::{
    ENV_PREFIX, Env, env_not_set, references, resolve_env, undefined_variable,
};

/// The run's starting variables, with `{{env:NAME}}` in their values resolved.
pub struct ResolvedVars {
    pub vars: HashMap<String, String>,
    /// Variables whose value uses an environment variable that is not set, mapped to that
    /// environment variable. They are only an error if a request that runs uses them.
    pub missing_env: HashMap<String, String>,
}

/// Resolves `{{env:NAME}}` in each variable's value. A value with no `{{env:NAME}}` is kept
/// exactly as written, the same as before environment variables were supported.
pub fn resolve_vars(vars: HashMap<String, String>, env: &Env) -> ResolvedVars {
    let mut resolved = ResolvedVars {
        vars: HashMap::with_capacity(vars.len()),
        missing_env: HashMap::new(),
    };
    for (name, value) in vars {
        let uses_env = references(&value).iter().any(|r| r.starts_with(ENV_PREFIX));
        if !uses_env {
            resolved.vars.insert(name, value);
            continue;
        }
        match resolve_env(&value, env) {
            Ok(value) => {
                resolved.vars.insert(name, value);
            }
            Err(env_name) => {
                resolved.missing_env.insert(name, env_name);
            }
        }
    }
    resolved
}

/// Checks every variable used by `requests`, in run order. A variable captured by an earlier
/// request counts as defined. Returns (request name, problem) pairs, empty if the run can start.
pub fn check_requests(
    requests: &[(String, RequestDef)],
    config: &Config,
    resolved: &ResolvedVars,
    env: &Env,
) -> Vec<(String, String)> {
    let mut captured: HashSet<&str> = HashSet::new();
    let mut problems = Vec::new();

    for (request_name, req) in requests {
        let config = config.without(&req.ignore_config);
        let mut checked = HashSet::new();
        for name in used_names(req, &config) {
            if !checked.insert(name.clone()) {
                continue;
            }
            if let Some(problem) = check_name(&name, &captured, resolved, env) {
                problems.push((request_name.clone(), problem));
            }
        }
        captured.extend(req.capture.keys().map(String::as_str));
    }
    problems
}

fn check_name(
    name: &str,
    captured: &HashSet<&str>,
    resolved: &ResolvedVars,
    env: &Env,
) -> Option<String> {
    if let Some(env_name) = name.strip_prefix(ENV_PREFIX) {
        return env(env_name)
            .is_none()
            .then(|| env_not_set(env_name).to_string());
    }
    if resolved.vars.contains_key(name) || captured.contains(name) {
        return None;
    }
    if let Some(env_name) = resolved.missing_env.get(name) {
        return Some(format!(
            "{} (used by {{{{{name}}}}})",
            env_not_set(env_name)
        ));
    }
    Some(undefined_variable(name).to_string())
}

/// The `{{name}}` references in every part of the request that is interpolated when it is sent.
fn used_names(req: &RequestDef, config: &Config) -> Vec<String> {
    let mut texts: Vec<&str> = vec![&req.url];
    texts.extend(req.headers.values().map(String::as_str));
    texts.extend(req.query.values().map(String::as_str));
    if let Some(auth) = req.auth.as_deref().or(config.auth.as_deref()) {
        texts.push(auth);
    }
    if req.interpolate_body
        && let Some(body) = &req.body
    {
        texts.push(body);
    }
    texts.into_iter().flat_map(references).collect()
}

/// `{{env:NAME}}` always reads the environment, so a variable named `env:...` could never be
/// used.
pub fn validate_names(rf: &RequestFile) -> Result<()> {
    let reserved = |name: &String| name.starts_with(ENV_PREFIX);
    if let Some(name) = rf.vars.keys().find(|n| reserved(n)) {
        return Err(reserved_name_error(&format!("[vars] '{name}'")));
    }
    for (profile, vars) in &rf.profiles {
        if let Some(name) = vars.keys().find(|n| reserved(n)) {
            return Err(reserved_name_error(&format!(
                "[profiles.{profile}] '{name}'"
            )));
        }
    }
    Ok(())
}

/// Parses a `--var NAME=VALUE` argument. The value is everything after the first `=`, so it can
/// contain `=` and commas.
pub fn parse_cli_var(arg: &str) -> Result<(String, String), String> {
    match arg.split_once('=') {
        Some((name, value)) if !name.is_empty() => Ok((name.to_string(), value.to_string())),
        _ => Err(format!("'{arg}' must be NAME=VALUE")),
    }
}

/// `--var` can only set a variable declared in `[vars]`, a profile, or a capture, so a misspelled
/// name is an error instead of a value that is never used.
pub fn check_cli_vars(rf: &RequestFile, cli_vars: &[(String, String)]) -> Result<()> {
    let declared: HashSet<&str> = rf
        .vars
        .keys()
        .chain(rf.profiles.values().flat_map(HashMap::keys))
        .chain(rf.requests.values().flat_map(|r| r.capture.keys()))
        .map(String::as_str)
        .collect();

    for (name, _) in cli_vars {
        if name.starts_with(ENV_PREFIX) {
            return Err(reserved_name_error(&format!("--var '{name}'")));
        }
        if declared.contains(name.as_str()) {
            continue;
        }
        let hint = closest(name, &declared)
            .map(|n| format!(" (did you mean '{n}'?)"))
            .unwrap_or_default();
        return Err(anyhow!(
            "--var '{name}': no variable named '{name}' in [vars], a profile, or a capture{hint}"
        ));
    }
    Ok(())
}

fn closest<'a>(name: &str, candidates: &HashSet<&'a str>) -> Option<&'a str> {
    candidates
        .iter()
        .map(|c| (edit_distance(&name.to_lowercase(), &c.to_lowercase()), *c))
        .filter(|(distance, _)| *distance <= 2)
        .min()
        .map(|(_, c)| c)
}

pub fn reserved_name_error(what: &str) -> anyhow::Error {
    anyhow!(
        "{what}: names starting with '{ENV_PREFIX}' are reserved, because {{{{{ENV_PREFIX}NAME}}}} reads the environment variable NAME"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::load_requests;

    fn env(name: &str) -> Option<String> {
        match name {
            "API_TOKEN" => Some("tok-123".to_string()),
            "EMPTY" => Some(String::new()),
            _ => None,
        }
    }

    fn problems(collection: &str) -> Vec<(String, String)> {
        let rf = RequestFile::parse(collection).unwrap();
        let requests = load_requests(&rf, None).unwrap();
        let resolved = resolve_vars(rf.vars.clone(), &env);
        check_requests(&requests, &rf.config, &resolved, &env)
    }

    #[test]
    fn vars_values_are_resolved() {
        let vars = HashMap::from([
            ("token".to_string(), "Bearer {{env:API_TOKEN}}".to_string()),
            ("empty".to_string(), "{{env:EMPTY}}".to_string()),
            ("missing".to_string(), "{{env:MISSING}}".to_string()),
        ]);
        let resolved = resolve_vars(vars, &env);
        assert_eq!(resolved.vars["token"], "Bearer tok-123");
        assert_eq!(resolved.vars["empty"], "");
        assert_eq!(resolved.missing_env["missing"], "MISSING");
    }

    #[test]
    fn values_without_env_are_kept_as_written() {
        let vars = HashMap::from([("raw".to_string(), r"\{{x}} {{y}}".to_string())]);
        assert_eq!(resolve_vars(vars, &env).vars["raw"], r"\{{x}} {{y}}");
    }

    #[test]
    fn body_is_not_checked_when_interpolation_is_off() {
        let found = problems(
            r#"
            [r]
            url = "http://x"
            body = '{"a": "{{not_a_var}}"}'
            interpolate_body = false
            "#,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn ignored_config_auth_is_not_checked() {
        let found = problems(
            r#"
            [config]
            auth = "bearer {{env:MISSING}}"

            [login]
            url = "http://x"
            ignore_config = ["auth"]
            "#,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn config_auth_is_checked() {
        let found = problems(
            r#"
            [config]
            auth = "bearer {{env:MISSING}}"

            [r]
            url = "http://x"
            "#,
        );
        assert_eq!(
            found,
            [(
                "r".to_string(),
                "environment variable 'MISSING' is not set".to_string()
            )]
        );
    }

    #[test]
    fn each_problem_is_reported_once_per_request() {
        let found = problems(
            r#"
            [r]
            url = "http://x/{{id}}/{{id}}"
            query = { id = "{{id}}" }
            "#,
        );
        assert_eq!(found.len(), 1, "{found:?}");
    }

    fn cli_var_error(collection: &str, name: &str) -> Option<String> {
        let rf = RequestFile::parse(collection).unwrap();
        check_cli_vars(&rf, &[(name.to_string(), "x".to_string())])
            .err()
            .map(|e| e.to_string())
    }

    const DECLARED: &str = r#"
        [vars]
        base_url = "http://x"

        [profiles.ci]
        token = "abc"

        [start-task]
        url = "{{base_url}}/tasks"
        capture = { taskId = "$.id" }
        "#;

    #[test]
    fn cli_var_parses_name_and_value() {
        assert_eq!(parse_cli_var("id=42"), Ok(("id".into(), "42".into())));
        assert_eq!(parse_cli_var("id="), Ok(("id".into(), "".into())));
        assert_eq!(parse_cli_var("q=a=b,c"), Ok(("q".into(), "a=b,c".into())));
        assert!(parse_cli_var("id").is_err());
        assert!(parse_cli_var("=42").is_err());
    }

    #[test]
    fn cli_var_can_set_vars_profile_values_and_captures() {
        for name in ["base_url", "token", "taskId"] {
            assert_eq!(cli_var_error(DECLARED, name), None, "{name}");
        }
    }

    #[test]
    fn cli_var_must_be_declared() {
        assert_eq!(
            cli_var_error(DECLARED, "taskid").unwrap(),
            "--var 'taskid': no variable named 'taskid' in [vars], a profile, or a capture (did you mean 'taskId'?)"
        );
        assert_eq!(
            cli_var_error(DECLARED, "user").unwrap(),
            "--var 'user': no variable named 'user' in [vars], a profile, or a capture"
        );
    }

    #[test]
    fn cli_var_env_prefix_is_reserved() {
        let err = cli_var_error(DECLARED, "env:HOME").unwrap();
        assert!(
            err.starts_with("--var 'env:HOME': names starting with 'env:' are reserved"),
            "{err}"
        );
    }

    #[test]
    fn env_prefix_is_reserved_in_profiles() {
        let rf = RequestFile::parse(
            r#"
            [profiles.ci]
            "env:TOKEN" = "x"
            "#,
        )
        .unwrap();
        let err = validate_names(&rf).unwrap_err().to_string();
        assert!(
            err.starts_with("[profiles.ci] 'env:TOKEN': names starting with 'env:' are reserved"),
            "{err}"
        );
    }
}
