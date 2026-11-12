use libtestkit::protocol::Flags;

/// What the harness saw for one case, before the flags of the case apply
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Raw {
    /// The case returned `Ok`
    Ok,
    /// The case returned a `Failure` with this message
    Fail(String),
    /// The kernel or the test process panicked, or QEMU exited, during the
    /// case. The text is the panic message.
    Crash(String),
    /// The case did not end before its time limit
    Timeout,
}

/// The final result of one case
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail(String),
    Crash(String),
    Timeout,
    /// A known bug made the case fail, as expected. `how` describes the
    /// failure.
    XFail { ticket: u32, how: String },
    /// The case has a known bug but it passed. The bug is fixed, or the case
    /// does not test it.
    XPass { ticket: u32 },
    /// The runner did not run the case, because it stops the boot
    Skipped,
}

impl Outcome {
    /// Returns `true` if this outcome makes the test run fail
    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            Outcome::Fail(_) | Outcome::Crash(_) | Outcome::Timeout | Outcome::XPass { .. }
        )
    }
}

/// Applies the flags of a case to its raw result.
///
/// 1. `should_panic`: a crash with the expected text is a pass. A normal
///    return is a fail.
/// 2. `known_bug`: a pass becomes `XPass`. Each other result becomes `XFail`.
pub fn classify(flags: &Flags, raw: Raw) -> Outcome {
    let base = match (&flags.should_panic, raw) {
        (Some(text), Raw::Crash(message)) => {
            if message.contains(text.as_str()) {
                Outcome::Pass
            } else {
                Outcome::Fail(format!(
                    "expected a panic that contains {text:?}, got: {message}"
                ))
            }
        }
        (Some(text), Raw::Ok) => {
            Outcome::Fail(format!("expected a panic that contains {text:?}, got none"))
        }
        (_, Raw::Ok) => Outcome::Pass,
        (_, Raw::Fail(message)) => Outcome::Fail(message),
        (_, Raw::Crash(message)) => Outcome::Crash(message),
        (_, Raw::Timeout) => Outcome::Timeout,
    };

    let Some(ticket) = flags.known_bug else {
        return base;
    };

    match base {
        Outcome::Pass => Outcome::XPass { ticket },
        Outcome::Fail(message) => Outcome::XFail { ticket, how: message },
        Outcome::Crash(message) => Outcome::XFail { ticket, how: format!("crash: {message}") },
        Outcome::Timeout => Outcome::XFail { ticket, how: "timeout".to_string() },
        other => other,
    }
}

/// Returns the text for a known bug: `#44`, or `no ticket` for ticket 0
pub fn ticket_text(ticket: u32) -> String {
    match ticket {
        0 => "no ticket".to_string(),
        n => format!("#{n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Flags {
        Flags::default()
    }

    fn known_bug() -> Flags {
        Flags { known_bug: Some(44), crash: true, ..Flags::default() }
    }

    fn should_panic() -> Flags {
        Flags { should_panic: Some("out of bounds".to_string()), ..Flags::default() }
    }

    #[test]
    fn a_plain_case_keeps_its_raw_result() {
        assert_eq!(classify(&plain(), Raw::Ok), Outcome::Pass);
        assert_eq!(classify(&plain(), Raw::Fail("m".into())), Outcome::Fail("m".into()));
        assert_eq!(classify(&plain(), Raw::Crash("p".into())), Outcome::Crash("p".into()));
        assert_eq!(classify(&plain(), Raw::Timeout), Outcome::Timeout);
    }

    #[test]
    fn a_known_bug_turns_each_failure_into_xfail() {
        for raw in [Raw::Fail("m".into()), Raw::Crash("p".into()), Raw::Timeout] {
            let outcome = classify(&known_bug(), raw);
            assert!(matches!(outcome, Outcome::XFail { ticket: 44, .. }), "{outcome:?}");
            assert!(!outcome.is_failure());
        }
    }

    #[test]
    fn a_known_bug_that_passes_is_xpass_and_fails_the_run() {
        let outcome = classify(&known_bug(), Raw::Ok);
        assert_eq!(outcome, Outcome::XPass { ticket: 44 });
        assert!(outcome.is_failure());
    }

    #[test]
    fn should_panic_passes_only_on_the_expected_panic() {
        let right = Raw::Crash("src/x.rs:1:1: index 3 is out of bounds".into());
        assert_eq!(classify(&should_panic(), right), Outcome::Pass);

        let wrong = Raw::Crash("src/x.rs:1:1: other".into());
        assert!(matches!(classify(&should_panic(), wrong), Outcome::Fail(_)));
        assert!(matches!(classify(&should_panic(), Raw::Ok), Outcome::Fail(_)));
        assert_eq!(classify(&should_panic(), Raw::Timeout), Outcome::Timeout);
    }

    #[test]
    fn should_panic_and_known_bug_combine() {
        let flags = Flags { known_bug: Some(7), ..should_panic() };
        // The panic does not occur yet: expected failure.
        assert!(matches!(classify(&flags, Raw::Ok), Outcome::XFail { ticket: 7, .. }));
        // The panic occurs: the bug is fixed.
        let right = Raw::Crash("out of bounds".into());
        assert_eq!(classify(&flags, right), Outcome::XPass { ticket: 7 });
    }
}
