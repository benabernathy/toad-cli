use std::str::FromStr;

use crate::collection::{Request, RetryDefault};

/// Delay between attempts when `retry_delay_ms` is not set.
pub const DEFAULT_RETRY_DELAY_MS: u64 = 1000;

/// A retry setting from `--retry` or `TOAD_RETRY`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RetrySetting {
    /// No retries at all, including a request's own `retry`
    Off,
    /// Replaces the `[config]` default. A request's own `retry` still wins.
    Default(u32),
}

impl FromStr for RetrySetting {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("off") {
            return Ok(RetrySetting::Off);
        }
        s.parse::<u32>().map(RetrySetting::Default).map_err(|_| {
            format!(
                "'{}' is not a valid retry setting (use a whole number of retries, or 'off')",
                s
            )
        })
    }
}

impl RetrySetting {
    /// Resolves the setting from the command line flag, then the environment variable. An
    /// invalid environment value prints a warning and is ignored.
    pub fn resolve(flag: Option<RetrySetting>, env: Option<&str>) -> Option<RetrySetting> {
        if flag.is_some() {
            return flag;
        }
        let env = env?;
        match env.parse() {
            Ok(setting) => Some(setting),
            Err(_) => {
                eprintln!(
                    "unknown TOAD_RETRY value: '{}', using the collection's retry settings",
                    env
                );
                None
            }
        }
    }
}

/// How many times to retry one request, and how long to wait between attempts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetryPolicy {
    pub retries: u32,
    pub delay_ms: u64,
}

impl RetryPolicy {
    /// Works out the policy for a request from its settings and `--retry` / `TOAD_RETRY`.
    pub fn for_request(req: &Request, setting: Option<RetrySetting>) -> RetryPolicy {
        let own = req.retry.retry;
        let retries = match (setting, req.retry.default) {
            (Some(RetrySetting::Off), _) => 0,
            (Some(RetrySetting::Default(n)), RetryDefault::Config(_)) => own.unwrap_or(n),
            (_, RetryDefault::Config(default)) => own.or(default).unwrap_or(0),
            (_, RetryDefault::Ignored) => own.unwrap_or(0),
        };

        RetryPolicy {
            retries,
            delay_ms: req.retry.delay_ms.unwrap_or(DEFAULT_RETRY_DELAY_MS),
        }
    }

    pub fn attempts(&self) -> u32 {
        self.retries.saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::Collection;

    fn policy(toml_str: &str, request: &str, setting: Option<RetrySetting>) -> RetryPolicy {
        let collection = Collection::parse(toml_str).unwrap();
        RetryPolicy::for_request(&collection.requests[request], setting)
    }

    const COLLECTION: &str = r#"
        [config]
        retry = 3
        retry_delay_ms = 250

        [inherits]
        url = "http://x"

        [own]
        url = "http://x"
        retry = 5
        retry_delay_ms = 10

        [zero]
        url = "http://x"
        retry = 0

        [ignores]
        url = "http://x"
        ignore_config = ["retry", "retry_delay_ms"]
    "#;

    #[test]
    fn parses_count_and_off() {
        assert_eq!("3".parse::<RetrySetting>(), Ok(RetrySetting::Default(3)));
        assert_eq!("0".parse::<RetrySetting>(), Ok(RetrySetting::Default(0)));
        assert_eq!("OFF".parse::<RetrySetting>(), Ok(RetrySetting::Off));
        assert!("-1".parse::<RetrySetting>().is_err());
        assert!("1.5".parse::<RetrySetting>().is_err());
        assert!("abc".parse::<RetrySetting>().is_err());
    }

    #[test]
    fn flag_wins_over_env() {
        let setting = RetrySetting::resolve(Some(RetrySetting::Default(2)), Some("off"));
        assert_eq!(setting, Some(RetrySetting::Default(2)));
        assert_eq!(
            RetrySetting::resolve(None, Some("off")),
            Some(RetrySetting::Off)
        );
        assert_eq!(RetrySetting::resolve(None, Some("abc")), None);
        assert_eq!(RetrySetting::resolve(None, None), None);
    }

    #[test]
    fn config_default_and_request_override() {
        assert_eq!(
            policy(COLLECTION, "inherits", None),
            RetryPolicy {
                retries: 3,
                delay_ms: 250
            }
        );
        assert_eq!(
            policy(COLLECTION, "own", None),
            RetryPolicy {
                retries: 5,
                delay_ms: 10
            }
        );
        assert_eq!(policy(COLLECTION, "zero", None).retries, 0);
        assert_eq!(
            policy(COLLECTION, "ignores", None),
            RetryPolicy {
                retries: 0,
                delay_ms: DEFAULT_RETRY_DELAY_MS
            }
        );
    }

    #[test]
    fn setting_replaces_only_the_config_default() {
        let setting = Some(RetrySetting::Default(1));
        assert_eq!(policy(COLLECTION, "inherits", setting).retries, 1);
        assert_eq!(policy(COLLECTION, "own", setting).retries, 5);
        assert_eq!(policy(COLLECTION, "zero", setting).retries, 0);
        assert_eq!(policy(COLLECTION, "ignores", setting).retries, 0);
    }

    #[test]
    fn off_disables_everything() {
        let setting = Some(RetrySetting::Off);
        for name in ["inherits", "own", "zero", "ignores"] {
            assert_eq!(policy(COLLECTION, name, setting).retries, 0, "{name}");
        }
    }
}
