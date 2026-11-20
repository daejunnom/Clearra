//! Transport-independent contract for a future bounded Windows cleanup runner.
//!
//! The live WSL manager does not invoke this module. Giving the current
//! blocking `Command` calls a deadline argument would not bound their lifetime.

use std::borrow::Cow;
use std::time::{Duration, Instant};

const DEDICATED_DISTRIBUTION: &str = "Clearra-Build";
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const MAX_LIST_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Command {
    ListRunning,
    TerminateDedicated,
}

impl Command {
    fn arguments(self) -> &'static [&'static str] {
        match self {
            Self::ListRunning => &["--list", "--running", "--quiet"],
            Self::TerminateDedicated => &["--terminate", DEDICATED_DISTRIBUTION],
        }
    }
}

#[derive(Clone, Copy)]
enum ListEncoding {
    Utf8,
    Utf16Le,
}

struct CompleteList {
    bytes: Vec<u8>,
    encoding: ListEncoding,
}

struct Reply {
    exit_code: i32,
    stdout: Option<CompleteList>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlError {
    TimedOut,
    ContainmentUnproven,
    IncompleteOutput,
    OutputLimit,
}

trait WslControl {
    fn now(&self) -> Instant;

    /// A successful reply requires a settled, fully owned host command, not
    /// that its WSL service operation finished. A list's stdout must have
    /// reached EOF within MAX_LIST_BYTES; partial data is never a reply.
    /// The transport must identify its actual encoding, not guess from a
    /// lossy decode. It must return within the supplied absolute deadline,
    /// including process reclamation and capture; use no detached readers.
    fn run_until(&mut self, command: Command, deadline: Instant) -> Result<Reply, ControlError>;

    fn sleep_until(&mut self, wake_at: Instant) -> Result<(), ControlError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    InitialList,
    Terminate,
    Readback,
    WaitForReadback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CleanupError {
    Deadline(Phase),
    Control(Phase, ControlError),
    ListExit(Phase, i32),
    InvalidList(Phase),
    EarlyWake,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CleanupResult {
    AlreadyStopped,
    VerifiedStopped { terminate_exit: i32 },
}

fn before_deadline<C: WslControl, T>(
    control: &mut C,
    deadline: Instant,
    phase: Phase,
    run: impl FnOnce(&mut C) -> Result<T, ControlError>,
) -> Result<T, CleanupError> {
    if control.now() >= deadline {
        return Err(CleanupError::Deadline(phase));
    }
    let result = run(control);
    // Reject a late reply even if the underlying command reports success.
    if control.now() >= deadline {
        return Err(CleanupError::Deadline(phase));
    }
    result.map_err(|error| match error {
        ControlError::TimedOut => CleanupError::Deadline(phase),
        error => CleanupError::Control(phase, error),
    })
}

fn dedicated_is_running(
    control: &mut impl WslControl,
    deadline: Instant,
    phase: Phase,
) -> Result<bool, CleanupError> {
    let reply = before_deadline(control, deadline, phase, |control| {
        control.run_until(Command::ListRunning, deadline)
    })?;
    if reply.exit_code != 0 {
        return Err(CleanupError::ListExit(phase, reply.exit_code));
    }
    let listing = reply.stdout.ok_or(CleanupError::InvalidList(phase))?;
    if listing.bytes.len() > MAX_LIST_BYTES {
        return Err(CleanupError::Control(phase, ControlError::OutputLimit));
    }
    let running = parse_list(&listing).ok_or(CleanupError::InvalidList(phase))?;
    if control.now() >= deadline {
        return Err(CleanupError::Deadline(phase));
    }
    Ok(running)
}

fn cleanup_dedicated(
    control: &mut impl WslControl,
    deadline: Instant,
) -> Result<CleanupResult, CleanupError> {
    if !dedicated_is_running(control, deadline, Phase::InitialList)? {
        return Ok(CleanupResult::AlreadyStopped);
    }
    let terminate = before_deadline(control, deadline, Phase::Terminate, |control| {
        control.run_until(Command::TerminateDedicated, deadline)
    })?;
    loop {
        // Termination can race an independent stop. Its exit code is kept as
        // evidence, but only a successful full readback proves stopped.
        if !dedicated_is_running(control, deadline, Phase::Readback)? {
            return Ok(CleanupResult::VerifiedStopped {
                terminate_exit: terminate.exit_code,
            });
        }
        let now = control.now();
        let wake_at = now
            .checked_add(POLL_INTERVAL)
            .map_or(deadline, |poll_at| poll_at.min(deadline));
        before_deadline(control, deadline, Phase::WaitForReadback, |control| {
            control.sleep_until(wake_at)
        })?;
        if control.now() < wake_at {
            return Err(CleanupError::EarlyWake);
        }
    }
}

fn parse_list(listing: &CompleteList) -> Option<bool> {
    let decoded: Cow<'_, str> = match listing.encoding {
        ListEncoding::Utf8 => {
            let bytes = listing
                .bytes
                .strip_prefix(&[0xef, 0xbb, 0xbf])
                .unwrap_or(&listing.bytes);
            Cow::Borrowed(std::str::from_utf8(bytes).ok()?)
        }
        ListEncoding::Utf16Le => {
            let bytes = listing
                .bytes
                .strip_prefix(&[0xff, 0xfe])
                .unwrap_or(&listing.bytes);
            let (units, tail) = bytes.as_chunks::<2>();
            if !tail.is_empty() {
                return None;
            }
            let values = units.iter().copied().map(u16::from_le_bytes);
            Cow::Owned(
                char::decode_utf16(values)
                    .collect::<Result<String, _>>()
                    .ok()?,
            )
        }
    };
    if !decoded.is_empty() && !decoded.ends_with('\n') {
        return None;
    }
    let mut found = false;
    // Unlike lines().any(), an early matching line must not hide malformed
    // trailing records. Only whole validated listings prove a state.
    for raw in decoded.split_terminator('\n') {
        let line = raw.trim_end_matches('\r');
        if line.chars().any(char::is_control) {
            return None;
        }
        if line.eq_ignore_ascii_case(DEDICATED_DISTRIBUTION) {
            found = true;
        } else if line.trim().eq_ignore_ascii_case(DEDICATED_DISTRIBUTION) {
            // A padded name is ambiguous, not permission to terminate or
            // evidence of absence. Quiet output must identify the exact name.
            return None;
        }
    }
    Some(found)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    struct ScriptedReply {
        command: Command,
        elapsed: Duration,
        reply: Result<Reply, ControlError>,
    }

    struct Mock {
        now: Instant,
        replies: VecDeque<ScriptedReply>,
        commands: Vec<(Command, Instant)>,
        wake_at: Vec<Instant>,
        wake_early: bool,
    }

    impl Mock {
        fn new(replies: impl IntoIterator<Item = ScriptedReply>) -> Self {
            Self {
                now: Instant::now(),
                replies: replies.into_iter().collect(),
                commands: Vec::new(),
                wake_at: Vec::new(),
                wake_early: false,
            }
        }
    }

    impl WslControl for Mock {
        fn now(&self) -> Instant {
            self.now
        }

        fn run_until(
            &mut self,
            command: Command,
            deadline: Instant,
        ) -> Result<Reply, ControlError> {
            assert!(self.now < deadline, "command admitted after its deadline");
            self.commands.push((command, deadline));
            let next = self.replies.pop_front().expect("unexpected command");
            assert_eq!(command, next.command);
            self.now += next.elapsed;
            next.reply
        }

        fn sleep_until(&mut self, wake_at: Instant) -> Result<(), ControlError> {
            self.wake_at.push(wake_at);
            if !self.wake_early {
                self.now = wake_at;
            }
            Ok(())
        }
    }

    fn list(bytes: Vec<u8>, encoding: ListEncoding, exit_code: i32) -> ScriptedReply {
        ScriptedReply {
            command: Command::ListRunning,
            elapsed: Duration::ZERO,
            reply: Ok(Reply {
                exit_code,
                stdout: Some(CompleteList { bytes, encoding }),
            }),
        }
    }

    fn utf8(value: &str) -> ScriptedReply {
        list(value.as_bytes().to_vec(), ListEncoding::Utf8, 0)
    }

    fn terminate(exit_code: i32) -> ScriptedReply {
        ScriptedReply {
            command: Command::TerminateDedicated,
            elapsed: Duration::ZERO,
            reply: Ok(Reply {
                exit_code,
                stdout: None,
            }),
        }
    }

    #[test]
    fn wsl_deadline_protocol_fixed_arguments_and_exact_dedicated_name() {
        assert_eq!(
            Command::TerminateDedicated.arguments(),
            &["--terminate", "Clearra-Build"]
        );
        assert_eq!(
            Command::ListRunning.arguments(),
            &["--list", "--running", "--quiet"]
        );
        let bytes = "\u{feff}Ubuntu\r\nClearra-Build-old\r\n머신\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let mut mock = Mock::new([list(bytes, ListEncoding::Utf16Le, 0)]);
        let deadline = mock.now + Duration::from_secs(1);
        assert_eq!(
            cleanup_dedicated(&mut mock, deadline),
            Ok(CleanupResult::AlreadyStopped)
        );
        assert_eq!(mock.commands, [(Command::ListRunning, deadline)]);
    }

    #[test]
    fn wsl_deadline_protocol_rechecks_even_when_terminate_exit_is_zero_or_nonzero() {
        for exit_code in [0, 9] {
            let mut mock = Mock::new([
                utf8("Ubuntu\nCLEARRA-build\n"),
                terminate(exit_code),
                utf8("Clearra-Build\n"),
                utf8("Ubuntu\n"),
            ]);
            let start = mock.now;
            let deadline = start + Duration::from_secs(1);
            assert_eq!(
                cleanup_dedicated(&mut mock, deadline),
                Ok(CleanupResult::VerifiedStopped {
                    terminate_exit: exit_code
                })
            );
            assert_eq!(mock.commands.len(), 4);
            assert!(mock.commands.iter().all(|&(_, limit)| limit == deadline));
            assert_eq!(mock.wake_at, [start + POLL_INTERVAL]);
        }
    }

    #[test]
    fn wsl_deadline_protocol_rejects_late_success_in_every_process_phase() {
        for (phase, command_count) in [
            (Phase::InitialList, 1),
            (Phase::Terminate, 2),
            (Phase::Readback, 3),
        ] {
            let mut steps = [utf8("Clearra-Build\n"), terminate(0), utf8("Ubuntu\n")];
            steps[command_count - 1].elapsed = Duration::from_secs(2);
            let mut mock = Mock::new(steps);
            let deadline = mock.now + Duration::from_secs(1);
            assert_eq!(
                cleanup_dedicated(&mut mock, deadline),
                Err(CleanupError::Deadline(phase))
            );
            assert_eq!(mock.commands.len(), command_count);
            assert!(mock.commands.iter().all(|&(_, limit)| limit == deadline));
        }
    }

    #[test]
    fn wsl_deadline_protocol_caps_sleep_and_refuses_a_broken_clock() {
        for wake_early in [false, true] {
            let mut mock = Mock::new([
                utf8("Clearra-Build\n"),
                terminate(0),
                utf8("Clearra-Build\n"),
            ]);
            mock.wake_early = wake_early;
            let deadline = mock.now + Duration::from_millis(40);
            assert_eq!(
                cleanup_dedicated(&mut mock, deadline),
                Err(if wake_early {
                    CleanupError::EarlyWake
                } else {
                    CleanupError::Deadline(Phase::WaitForReadback)
                })
            );
            assert_eq!(mock.wake_at, [deadline]);
        }
    }

    #[test]
    fn wsl_deadline_protocol_does_not_admit_unsettled_or_truncated_host_commands() {
        for error in [
            ControlError::TimedOut,
            ControlError::ContainmentUnproven,
            ControlError::IncompleteOutput,
            ControlError::OutputLimit,
        ] {
            let mut mock = Mock::new([ScriptedReply {
                command: Command::ListRunning,
                elapsed: Duration::ZERO,
                reply: Err(error),
            }]);
            let deadline = mock.now + Duration::from_secs(1);
            let expected = if error == ControlError::TimedOut {
                CleanupError::Deadline(Phase::InitialList)
            } else {
                CleanupError::Control(Phase::InitialList, error)
            };
            assert_eq!(cleanup_dedicated(&mut mock, deadline), Err(expected));
            assert_eq!(mock.commands.len(), 1);
        }
    }

    #[test]
    fn wsl_deadline_protocol_does_not_treat_invalid_listing_as_absence() {
        let cases = [
            (
                list(b"Ubuntu\n".to_vec(), ListEncoding::Utf8, 2),
                CleanupError::ListExit(Phase::Readback, 2),
            ),
            (
                list(vec![b'U'; MAX_LIST_BYTES + 1], ListEncoding::Utf8, 0),
                CleanupError::Control(Phase::Readback, ControlError::OutputLimit),
            ),
            (
                utf8("Clearra-Build\nUbuntu"),
                CleanupError::InvalidList(Phase::Readback),
            ),
            (
                utf8("Clearra-Build\nU\0buntu\n"),
                CleanupError::InvalidList(Phase::Readback),
            ),
            (
                utf8(" Clearra-Build\n"),
                CleanupError::InvalidList(Phase::Readback),
            ),
            (
                list(vec![0xff, b'\n'], ListEncoding::Utf8, 0),
                CleanupError::InvalidList(Phase::Readback),
            ),
            (
                list(vec![0xff, 0xfe, b'U'], ListEncoding::Utf16Le, 0),
                CleanupError::InvalidList(Phase::Readback),
            ),
        ];
        for (invalid, expected) in cases {
            let mut mock = Mock::new([utf8("Clearra-Build\n"), terminate(0), invalid]);
            let deadline = mock.now + Duration::from_secs(1);
            assert_eq!(cleanup_dedicated(&mut mock, deadline), Err(expected));
        }
    }
}
