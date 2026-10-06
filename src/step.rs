//! Step mode (`--step`) and breakpoints (`--break`): stopping before a request and waiting for a
//! key.

use std::collections::HashSet;
use std::io::{IsTerminal, Write};

use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal;

use crate::collection::{RequestFile, closest};

/// What to do at a stop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Key {
    /// Run the next request, then stop again.
    Step,
    /// Run until the next breakpoint.
    Continue,
    /// Run to the end, ignoring breakpoints.
    Run,
    /// Stop without running any more requests.
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Step,
    Continue,
    Run,
}

/// Decides where the run stops. It knows nothing about the terminal, so the run loop and tests
/// can drive it with any keys.
pub struct Stepper {
    mode: Mode,
    breakpoints: HashSet<String>,
}

impl Stepper {
    /// `None` when neither `--step` nor `--break` is given, so the run never stops.
    pub fn new(step: bool, breakpoints: &[String]) -> Option<Stepper> {
        if !step && breakpoints.is_empty() {
            return None;
        }
        Some(Stepper {
            mode: if step { Mode::Step } else { Mode::Continue },
            breakpoints: breakpoints.iter().cloned().collect(),
        })
    }

    pub fn should_stop(&self, name: &str) -> bool {
        match self.mode {
            Mode::Step => true,
            Mode::Continue => self.breakpoints.contains(name),
            Mode::Run => false,
        }
    }

    /// Applies a key pressed at a stop. Returns false for `Key::Quit`.
    pub fn press(&mut self, key: Key) -> bool {
        self.mode = match key {
            Key::Step => Mode::Step,
            Key::Continue => Mode::Continue,
            Key::Run => Mode::Run,
            Key::Quit => return false,
        };
        true
    }

    fn stop_message(&self, name: &str) -> String {
        if self.breakpoints.contains(name) {
            format!("breakpoint: next request is '{name}'")
        } else {
            format!("next request is '{name}'")
        }
    }
}

/// Checks that every `--break` name is a request in the collection.
pub fn check_breakpoints(rf: &RequestFile, breakpoints: &[String]) -> Result<()> {
    for name in breakpoints {
        if rf.requests.contains_key(name) {
            continue;
        }
        let hint = closest(name, rf.requests.keys().map(String::as_str))
            .map(|n| format!(" (did you mean '{n}'?)"))
            .unwrap_or_default();
        return Err(anyhow!("--break '{name}': no request named '{name}'{hint}"));
    }
    Ok(())
}

/// `--step` and `--break` wait for a key, so they must never run where nobody can press one,
/// such as a CI job.
pub fn check_terminal(step: bool) -> Result<()> {
    if std::io::stdin().is_terminal() {
        return Ok(());
    }
    let flag = if step { "--step" } else { "--break" };
    Err(anyhow!(
        "{flag} waits for a key, but stdin is not a terminal"
    ))
}

const PROMPT: &str = "[s]tep, [c]ontinue, [r]un to end, [q]uit: ";

/// Prints where the run stopped and waits for a key. Everything goes to stderr, so stdout only
/// has the output mode's output.
pub fn ask(stepper: &Stepper, name: &str) -> Result<Key> {
    // Raw mode is on before the prompt appears, so a key pressed right away is read as one key
    // instead of waiting in the terminal's line buffer
    let raw = RawMode::enable()?;
    let mut stderr = std::io::stderr();
    // Raw mode doesn't turn \n into \r\n
    write!(stderr, "{}\r\n{PROMPT}", stepper.stop_message(name))?;
    stderr.flush()?;
    let (key, shown) = read_key()?;
    drop(raw);
    writeln!(stderr, "{shown}")?;
    Ok(key)
}

/// Waits for s, c, r, or q (either case) and ignores other keys. Ctrl-C quits, because raw mode
/// stops the terminal from sending it as a signal.
fn read_key() -> Result<(Key, &'static str)> {
    loop {
        let Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event::read()?
        else {
            continue;
        };
        if modifiers.contains(KeyModifiers::CONTROL) {
            if c == 'c' {
                return Ok((Key::Quit, "^C"));
            }
            continue;
        }
        match c.to_ascii_lowercase() {
            's' => return Ok((Key::Step, "s")),
            'c' => return Ok((Key::Continue, "c")),
            'r' => return Ok((Key::Run, "r")),
            'q' => return Ok((Key::Quit, "q")),
            _ => {}
        }
    }
}

/// Turns raw mode off when dropped, including when reading a key fails.
struct RawMode;

impl RawMode {
    fn enable() -> Result<RawMode> {
        terminal::enable_raw_mode()?;
        Ok(RawMode)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The requests a run stops before, pressing `keys` in order at each stop.
    fn stops(step: bool, breakpoints: &[&str], requests: &[&str], keys: &[Key]) -> Vec<String> {
        let breakpoints: Vec<String> = breakpoints.iter().map(|s| s.to_string()).collect();
        let mut stepper = Stepper::new(step, &breakpoints).unwrap();
        let mut keys = keys.iter();
        let mut stopped = Vec::new();
        for name in requests {
            if stepper.should_stop(name) {
                stopped.push(name.to_string());
                if !stepper.press(*keys.next().expect("ran out of keys")) {
                    break;
                }
            }
        }
        stopped
    }

    const REQUESTS: &[&str] = &["login", "create", "get", "delete"];

    #[test]
    fn neither_flag_never_stops() {
        assert!(Stepper::new(false, &[]).is_none());
    }

    #[test]
    fn step_stops_before_every_request() {
        let found = stops(true, &[], REQUESTS, &[Key::Step; 4]);
        assert_eq!(found, REQUESTS);
    }

    #[test]
    fn continue_without_breakpoints_runs_to_the_end() {
        let found = stops(true, &[], REQUESTS, &[Key::Step, Key::Continue]);
        assert_eq!(found, ["login", "create"]);
    }

    #[test]
    fn break_stops_only_at_breakpoints() {
        let found = stops(false, &["create", "delete"], REQUESTS, &[Key::Continue; 2]);
        assert_eq!(found, ["create", "delete"]);
    }

    #[test]
    fn step_from_a_breakpoint_stops_before_the_next_request() {
        let found = stops(false, &["create"], REQUESTS, &[Key::Step, Key::Continue]);
        assert_eq!(found, ["create", "get"]);
    }

    #[test]
    fn run_ignores_remaining_breakpoints() {
        let found = stops(false, &["login", "get"], REQUESTS, &[Key::Run]);
        assert_eq!(found, ["login"]);
    }

    #[test]
    fn continue_from_step_mode_stops_at_the_next_breakpoint() {
        let found = stops(true, &["delete"], REQUESTS, &[Key::Continue, Key::Continue]);
        assert_eq!(found, ["login", "delete"]);
    }

    #[test]
    fn quit_stops_the_run() {
        let found = stops(true, &[], REQUESTS, &[Key::Step, Key::Quit]);
        assert_eq!(found, ["login", "create"]);
    }

    #[test]
    fn stop_message_names_a_breakpoint() {
        let stepper = Stepper::new(true, &["get".to_string()]).unwrap();
        assert_eq!(stepper.stop_message("login"), "next request is 'login'");
        assert_eq!(
            stepper.stop_message("get"),
            "breakpoint: next request is 'get'"
        );
    }

    #[test]
    fn unknown_breakpoint_suggests_a_request() {
        let rf = RequestFile::parse(
            r#"
            [get-user]
            url = "http://x"
            "#,
        )
        .unwrap();
        let err = check_breakpoints(&rf, &["get-usr".to_string()]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "--break 'get-usr': no request named 'get-usr' (did you mean 'get-user'?)"
        );
    }
}
