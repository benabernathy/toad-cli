use std::str::FromStr;

/// How `expect_max_ms` limits are applied, set with `--time-scale` or `TOAD_TIME_SCALE`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeScale {
    /// Limits are not checked
    Off,
    /// Every limit is multiplied by this factor
    Factor(f64),
}

impl FromStr for TimeScale {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("off") {
            return Ok(TimeScale::Off);
        }
        match s.parse::<f64>() {
            Ok(f) if f.is_finite() && f > 0.0 => Ok(TimeScale::Factor(f)),
            _ => Err(format!(
                "'{}' is not a valid time scale (use a number greater than 0, or 'off')",
                s
            )),
        }
    }
}

/// The time scale in effect for a run, and where it came from.
#[derive(Debug, Clone, Copy)]
pub struct TimeLimits {
    pub scale: TimeScale,
    /// `"--time-scale"` or `"TOAD_TIME_SCALE"`, or `None` for the default
    pub source: Option<&'static str>,
}

impl Default for TimeLimits {
    fn default() -> Self {
        TimeLimits {
            scale: TimeScale::Factor(1.0),
            source: None,
        }
    }
}

impl TimeLimits {
    /// Resolves the scale from the command line flag, then the environment variable, then the
    /// default of 1. An invalid environment value prints a warning and is ignored.
    pub fn resolve(flag: Option<TimeScale>, env: Option<&str>) -> TimeLimits {
        if let Some(scale) = flag {
            return TimeLimits {
                scale,
                source: Some("--time-scale"),
            };
        }
        match env.map(TimeScale::from_str) {
            Some(Ok(scale)) => TimeLimits {
                scale,
                source: Some("TOAD_TIME_SCALE"),
            },
            Some(Err(_)) => {
                eprintln!(
                    "unknown TOAD_TIME_SCALE value: '{}', using 1",
                    env.unwrap_or_default()
                );
                TimeLimits::default()
            }
            None => TimeLimits::default(),
        }
    }

    /// The limit to enforce for `expect_max_ms`, or `None` if limits are off.
    pub fn effective(&self, expect_max_ms: u64) -> Option<u64> {
        match self.scale {
            TimeScale::Off => None,
            TimeScale::Factor(f) => Some((expect_max_ms as f64 * f).round() as u64),
        }
    }

    /// Describes the effective limit for messages, e.g. `600ms (300ms x 2 from TOAD_TIME_SCALE)`.
    pub fn describe(&self, expect_max_ms: u64) -> String {
        match (self.scale, self.source) {
            (TimeScale::Factor(f), Some(source)) if f != 1.0 => format!(
                "{}ms ({}ms x {} from {})",
                self.effective(expect_max_ms).unwrap_or(expect_max_ms),
                expect_max_ms,
                f,
                source
            ),
            _ => format!("{}ms", expect_max_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_factor_and_off() {
        assert_eq!("2".parse::<TimeScale>(), Ok(TimeScale::Factor(2.0)));
        assert_eq!("1.5".parse::<TimeScale>(), Ok(TimeScale::Factor(1.5)));
        assert_eq!("off".parse::<TimeScale>(), Ok(TimeScale::Off));
        assert_eq!("OFF".parse::<TimeScale>(), Ok(TimeScale::Off));
    }

    #[test]
    fn rejects_zero_negative_and_garbage() {
        assert!("0".parse::<TimeScale>().is_err());
        assert!("-1".parse::<TimeScale>().is_err());
        assert!("abc".parse::<TimeScale>().is_err());
        assert!("inf".parse::<TimeScale>().is_err());
        assert!("NaN".parse::<TimeScale>().is_err());
    }

    #[test]
    fn flag_wins_over_env() {
        let limits = TimeLimits::resolve(Some(TimeScale::Factor(1.0)), Some("3"));
        assert_eq!(limits.scale, TimeScale::Factor(1.0));
        assert_eq!(limits.source, Some("--time-scale"));
    }

    #[test]
    fn env_is_used_without_flag() {
        let limits = TimeLimits::resolve(None, Some("off"));
        assert_eq!(limits.scale, TimeScale::Off);
        assert_eq!(limits.source, Some("TOAD_TIME_SCALE"));
    }

    #[test]
    fn invalid_env_falls_back_to_default() {
        let limits = TimeLimits::resolve(None, Some("abc"));
        assert_eq!(limits.scale, TimeScale::Factor(1.0));
        assert_eq!(limits.source, None);
    }

    #[test]
    fn effective_scales_and_rounds() {
        let limits = TimeLimits::resolve(Some(TimeScale::Factor(1.5)), None);
        assert_eq!(limits.effective(301), Some(452));
        let off = TimeLimits::resolve(Some(TimeScale::Off), None);
        assert_eq!(off.effective(300), None);
    }

    #[test]
    fn describe_names_the_source_only_when_scaled() {
        let scaled = TimeLimits::resolve(None, Some("2"));
        assert_eq!(
            scaled.describe(300),
            "600ms (300ms x 2 from TOAD_TIME_SCALE)"
        );
        let one = TimeLimits::resolve(Some(TimeScale::Factor(1.0)), None);
        assert_eq!(one.describe(300), "300ms");
        assert_eq!(TimeLimits::default().describe(300), "300ms");
    }
}
