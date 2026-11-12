//! A guest test runner prints one record on each line of the serial port.
//! The host reads the records and decides the result of each case.
//!
//! Records:
//!
//! ```text
//! @@T suite <suite> <total>
//! @@T start <index> <name> [flags]
//! @@T ok <index>
//! @@T fail <index> <message>
//! @@T skip <index> <name> [flags]
//! @@T case <index> <name> [flags]
//! @@T done <suite>
//! @@T panic <message>
//! ```
//!
//! Flags: `known_bug=<ticket>`, `crash`, `timeout=<seconds>` and
//! `should_panic=<text>`. The `should_panic` flag is always last, because its
//! text can contain spaces.
//!
//! A record can follow other text on the same line. The kernel logger puts
//! `[PID n] ` before each line that a user process writes.

use alloc::{format, string::{String, ToString}};
use core::fmt::Write;

use crate::Case;

/// The text that starts each record
pub const PREFIX: &str = "@@T ";

/// The name of the suite that runs inside the kernel
pub const SUITE_KERNEL: &str = "kernel";
/// The name of the suite that runs in a user process
pub const SUITE_USER: &str = "user";

/// The properties of a case that the host needs
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Flags {
    pub known_bug: Option<u32>,
    pub crash: bool,
    pub timeout_secs: Option<u32>,
    pub should_panic: Option<String>,
}

impl Flags {
    pub fn of(case: &Case) -> Self {
        Self {
            known_bug: case.known_bug,
            crash: case.crash,
            timeout_secs: case.timeout_secs,
            should_panic: case.should_panic.map(|s| s.to_string()),
        }
    }

    fn write_to(&self, out: &mut String) {
        if let Some(ticket) = self.known_bug {
            write!(out, " known_bug={ticket}").unwrap();
        }
        if self.crash {
            out.push_str(" crash");
        }
        if let Some(secs) = self.timeout_secs {
            write!(out, " timeout={secs}").unwrap();
        }
        if let Some(text) = &self.should_panic {
            write!(out, " should_panic={}", one_line(text)).unwrap();
        }
    }

    fn parse(mut text: &str) -> Self {
        let mut flags = Self::default();

        loop {
            text = text.trim_start();
            if text.is_empty() {
                break;
            }

            if let Some(rest) = text.strip_prefix("should_panic=") {
                flags.should_panic = Some(rest.to_string());
                break;
            }

            let (token, rest) = text.split_once(' ').unwrap_or((text, ""));
            text = rest;

            match token.split_once('=') {
                Some(("known_bug", v)) => flags.known_bug = v.parse().ok(),
                Some(("timeout", v)) => flags.timeout_secs = v.parse().ok(),
                None if token == "crash" => flags.crash = true,
                _ => (),
            }
        }

        flags
    }
}

/// One record of the protocol
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A suite starts. `total` counts all cases, before any filter.
    Suite { suite: String, total: usize },
    /// A case starts
    Start { index: usize, name: String, flags: Flags },
    /// The case that runs now returned `Ok`
    Ok { index: usize },
    /// The case that runs now returned a `Failure`
    Fail { index: usize, message: String },
    /// The runner did not run this case, because it stops the boot
    Skip { index: usize, name: String, flags: Flags },
    /// List mode: one case
    Case { index: usize, name: String, flags: Flags },
    /// A suite is complete
    Done { suite: String },
    /// The test kernel panicked
    Panic { message: String },
}

impl Event {
    /// Returns the record as one line with no line end
    pub fn to_line(&self) -> String {
        let mut out = String::from(PREFIX);

        match self {
            Event::Suite { suite, total } => {
                write!(out, "suite {suite} {total}").unwrap();
            }
            Event::Start { index, name, flags } => {
                write!(out, "start {index} {name}").unwrap();
                flags.write_to(&mut out);
            }
            Event::Ok { index } => {
                write!(out, "ok {index}").unwrap();
            }
            Event::Fail { index, message } => {
                write!(out, "fail {index} {}", one_line(message)).unwrap();
            }
            Event::Skip { index, name, flags } => {
                write!(out, "skip {index} {name}").unwrap();
                flags.write_to(&mut out);
            }
            Event::Case { index, name, flags } => {
                write!(out, "case {index} {name}").unwrap();
                flags.write_to(&mut out);
            }
            Event::Done { suite } => {
                write!(out, "done {suite}").unwrap();
            }
            Event::Panic { message } => {
                write!(out, "panic {}", one_line(message)).unwrap();
            }
        }

        out
    }

    /// Finds a record in a line of serial output. Returns `None` if the line
    /// has no record or the record is malformed.
    pub fn parse(line: &str) -> Option<Self> {
        let at = line.find(PREFIX)?;
        let record = line[at + PREFIX.len()..].trim_end_matches(['\r', '\n']);
        let (kind, rest) = record.split_once(' ').unwrap_or((record, ""));

        // Splits `<index> <name> [flags]`
        let indexed = |rest: &str| -> Option<(usize, String, Flags)> {
            let (index, rest) = rest.split_once(' ')?;
            let (name, flags) = rest.split_once(' ').unwrap_or((rest, ""));
            if name.is_empty() {
                return None;
            }
            Some((index.parse().ok()?, name.to_string(), Flags::parse(flags)))
        };

        Some(match kind {
            "suite" => {
                let (suite, total) = rest.split_once(' ')?;
                Event::Suite { suite: suite.to_string(), total: total.trim().parse().ok()? }
            }
            "start" => {
                let (index, name, flags) = indexed(rest)?;
                Event::Start { index, name, flags }
            }
            "ok" => Event::Ok { index: rest.trim().parse().ok()? },
            "fail" => {
                let (index, message) = rest.split_once(' ').unwrap_or((rest, ""));
                Event::Fail { index: index.parse().ok()?, message: message.to_string() }
            }
            "skip" => {
                let (index, name, flags) = indexed(rest)?;
                Event::Skip { index, name, flags }
            }
            "case" => {
                let (index, name, flags) = indexed(rest)?;
                Event::Case { index, name, flags }
            }
            "done" => Event::Done { suite: rest.trim().to_string() },
            "panic" => Event::Panic { message: rest.to_string() },
            _ => return None,
        })
    }
}

/// Replaces each line break with a space, so that a record stays on one line
pub fn one_line(text: &str) -> String {
    let mut out = format!("{text}");
    if out.contains(['\r', '\n']) {
        out = out.replace(['\r', '\n'], " ");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn flags_all() -> Flags {
        Flags {
            known_bug: Some(44),
            crash: true,
            timeout_secs: Some(5),
            should_panic: Some("index out of bounds".to_string()),
        }
    }

    #[test]
    fn each_record_survives_a_round_trip() {
        let events = vec![
            Event::Suite { suite: "kernel".to_string(), total: 12 },
            Event::Start { index: 0, name: "a::b".to_string(), flags: Flags::default() },
            Event::Start { index: 3, name: "a::c".to_string(), flags: flags_all() },
            Event::Ok { index: 3 },
            Event::Fail { index: 4, message: "src/x.rs:3: check failed: a == b".to_string() },
            Event::Skip { index: 5, name: "a::d".to_string(), flags: flags_all() },
            Event::Case { index: 6, name: "a::e".to_string(), flags: Flags::default() },
            Event::Done { suite: "user".to_string() },
            Event::Panic { message: "src/y.rs:9:5: boom".to_string() },
        ];

        for event in events {
            assert_eq!(Event::parse(&event.to_line()), Some(event));
        }
    }

    #[test]
    fn finds_a_record_after_the_logger_prefix() {
        let event = Event::parse("[PID 0] @@T ok 7\r\n");
        assert_eq!(event, Some(Event::Ok { index: 7 }));
    }

    #[test]
    fn ignores_lines_with_no_record() {
        assert_eq!(Event::parse("Hello world!"), None);
        assert_eq!(Event::parse("@@T"), None);
        assert_eq!(Event::parse("@@T bogus 1"), None);
        assert_eq!(Event::parse("@@T ok x"), None);
        assert_eq!(Event::parse("@@T start 1"), None);
    }

    #[test]
    fn a_fail_message_stays_on_one_line() {
        let event = Event::Fail { index: 1, message: "line one\nline two".to_string() };
        let line = event.to_line();
        assert!(!line.contains('\n'));
        assert_eq!(
            Event::parse(&line),
            Some(Event::Fail { index: 1, message: "line one line two".to_string() })
        );
    }
}
