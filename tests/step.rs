mod common;

use common::{start_server, stderr, toad, write_collection};

/// Four requests to the test server's `/echo` route.
fn collection(test_name: &str) -> String {
    let base_url = start_server();
    let requests: String = ["login", "create", "get", "delete"]
        .iter()
        .map(|name| {
            format!("[{name}]\nmethod = \"POST\"\nurl = \"{base_url}/echo\"\nbody = '{{}}'\n\n")
        })
        .collect();
    let file = write_collection(test_name, &requests);
    file.to_str().unwrap().to_string()
}

// The test harness runs toad with stdin closed, so it is never a terminal here.

#[test]
fn step_without_a_terminal_is_an_error() {
    let file = collection("step-no-tty");
    let output = toad(&[&file, "--step"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("--step waits for a key, but stdin is not a terminal"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn break_without_a_terminal_is_an_error() {
    let file = collection("break-no-tty");
    let output = toad(&[&file, "--break", "get"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("--break waits for a key, but stdin is not a terminal"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn unknown_breakpoint_is_an_error() {
    let file = collection("break-unknown");
    let output = toad(&[&file, "-b", "create,gte"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("--break 'gte': no request named 'gte' (did you mean 'get'?)"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn list_requests_does_not_need_a_terminal() {
    let file = collection("step-list");
    let output = toad(&[&file, "-l", "--step"]);
    assert!(output.status.success(), "{}", stderr(&output));
}

/// Runs toad in a pseudo-terminal, so it can read single keys the way it does for a person.
/// Windows CI has no Unix PTY, and the key handling there is covered by the unit tests.
#[cfg(unix)]
mod terminal {
    use super::collection;
    use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
    use std::io::{Read, Write};
    use std::sync::mpsc::{Receiver, channel};
    use std::time::{Duration, Instant};

    struct Session {
        child: Box<dyn Child + Send + Sync>,
        writer: Box<dyn Write + Send>,
        output: Receiver<Vec<u8>>,
        seen: String,
        // Closing the master ends the session, so keep it until the test is done
        _master: Box<dyn MasterPty + Send>,
    }

    impl Session {
        fn start(args: &[&str]) -> Session {
            let pair = native_pty_system()
                .openpty(PtySize {
                    rows: 50,
                    cols: 200,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .unwrap();
            let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_toad"));
            command.args(args);
            command.env("NO_COLOR", "1");
            for name in [
                "TOAD_OUTPUT",
                "TOAD_TIME_SCALE",
                "TOAD_CA_PASSWORD",
                "TOAD_RETRY",
            ] {
                command.env_remove(name);
            }
            let child = pair.slave.spawn_command(command).unwrap();
            drop(pair.slave);

            let mut reader = pair.master.try_clone_reader().unwrap();
            let (tx, output) = channel();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            });

            Session {
                child,
                writer: pair.master.take_writer().unwrap(),
                output,
                seen: String::new(),
                _master: pair.master,
            }
        }

        /// Waits until the output contains `text`.
        fn expect(&mut self, text: &str) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !self.seen.contains(text) {
                let left = deadline.saturating_duration_since(Instant::now());
                match self.output.recv_timeout(left) {
                    Ok(bytes) => self.seen.push_str(&String::from_utf8_lossy(&bytes)),
                    Err(_) => panic!(
                        "timed out waiting for {text:?}. Output so far:\n{}",
                        self.seen
                    ),
                }
            }
        }

        /// Waits for the next prompt, then presses `key`.
        fn press(&mut self, key: &str) {
            self.expect("[q]uit: ");
            // Only the part after this prompt counts for the next expect
            self.seen = self
                .seen
                .split_off(self.seen.rfind("[q]uit: ").unwrap() + 8);
            self.writer.write_all(key.as_bytes()).unwrap();
            self.writer.flush().unwrap();
        }

        /// Waits for toad to exit and returns its exit code and the output since the last key.
        fn finish(mut self) -> (u32, String) {
            let code = self.child.wait().unwrap().exit_code();
            while let Ok(bytes) = self.output.recv_timeout(Duration::from_millis(200)) {
                self.seen.push_str(&String::from_utf8_lossy(&bytes));
            }
            (code, self.seen.replace('\r', ""))
        }
    }

    #[test]
    fn step_then_quit() {
        let file = collection("step-quit");
        let mut session = Session::start(&[&file, "--step", "-o", "quiet"]);
        session.expect("next request is 'login'");
        session.press("s");
        session.expect("[login] 200");
        session.expect("next request is 'create'");
        session.press("q");
        let (code, out) = session.finish();
        assert_eq!(code, 130, "{out}");
        assert!(
            out.contains("stopped before 'create' (1 of 4 requests run)"),
            "{out}"
        );
        assert!(!out.contains("[create]"), "{out}");
    }

    #[test]
    fn step_then_continue_without_breakpoints_runs_to_the_end() {
        let file = collection("step-continue");
        let mut session = Session::start(&[&file, "-s", "-o", "quiet"]);
        session.press("c");
        let (code, out) = session.finish();
        assert_eq!(code, 0, "{out}");
        assert!(out.contains("[delete] 200"), "{out}");
        assert!(!out.contains("next request"), "{out}");
    }

    #[test]
    fn break_stops_at_each_breakpoint() {
        let file = collection("break-continue");
        let mut session =
            Session::start(&[&file, "--break", "create", "-b", "delete", "-o", "quiet"]);
        session.expect("[login] 200");
        session.expect("breakpoint: next request is 'create'");
        session.press("c");
        session.expect("[get] 200");
        session.expect("breakpoint: next request is 'delete'");
        session.press("c");
        let (code, out) = session.finish();
        assert_eq!(code, 0, "{out}");
        assert!(out.contains("[delete] 200"), "{out}");
    }

    #[test]
    fn step_from_a_breakpoint_then_run_to_the_end() {
        let file = collection("break-step-run");
        let mut session = Session::start(&[&file, "-b", "create,delete", "-o", "quiet"]);
        session.expect("breakpoint: next request is 'create'");
        session.press("s");
        session.expect("next request is 'get'");
        session.press("r");
        let (code, out) = session.finish();
        assert_eq!(code, 0, "{out}");
        // r ignores the breakpoint on delete
        assert!(!out.contains("breakpoint"), "{out}");
        assert!(out.contains("[delete] 200"), "{out}");
    }

    #[test]
    fn other_keys_are_ignored() {
        let file = collection("step-other-keys");
        let mut session = Session::start(&[&file, "--step", "-o", "quiet"]);
        session.press("x\rQ");
        let (code, out) = session.finish();
        assert_eq!(code, 130, "{out}");
        assert!(
            out.contains("stopped before 'login' (0 of 4 requests run)"),
            "{out}"
        );
    }

    #[test]
    fn ctrl_c_quits() {
        let file = collection("step-ctrl-c");
        let mut session = Session::start(&[&file, "--step", "-o", "quiet"]);
        session.press("\x03");
        let (code, out) = session.finish();
        assert_eq!(code, 130, "{out}");
        assert!(out.contains("^C"), "{out}");
    }
}
