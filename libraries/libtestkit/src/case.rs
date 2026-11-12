use alloc::{format, string::String};
use core::fmt::Debug;

/// The result of one test case
pub type TestResult = Result<(), Failure>;

/// The reason a test case fails
#[derive(Debug)]
pub struct Failure {
    pub message: String,
}

impl Failure {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

/// Converts an `Option` or a `Result` into a `TestResult` value. Use it with
/// `?` in a test case.
pub trait OrFail<T> {
    /// Returns the inner value, or a `Failure` that contains `context`
    fn or_fail(self, context: &str) -> Result<T, Failure>;
}

impl<T> OrFail<T> for Option<T> {
    fn or_fail(self, context: &str) -> Result<T, Failure> {
        self.ok_or_else(|| Failure::new(format!("{context}: got None")))
    }
}

impl<T, E: Debug> OrFail<T> for Result<T, E> {
    fn or_fail(self, context: &str) -> Result<T, Failure> {
        self.map_err(|e| Failure::new(format!("{context}: got Err({e:?})")))
    }
}

/// One test case and its properties.
///
/// Declare cases with `ktest!` (kernel) or `cases!` (user program).
pub struct Case {
    /// The full name: module path and function name
    pub name: &'static str,
    /// The test function
    pub func: fn() -> TestResult,
    /// The ticket number of a bug that makes this case fail. `Some(0)` is a
    /// bug with no ticket. The case asserts the correct behavior.
    pub known_bug: Option<u32>,
    /// The known bug panics or hangs the kernel or the process
    pub crash: bool,
    /// The case passes only if the kernel panics with a message that
    /// contains this text
    pub should_panic: Option<&'static str>,
    /// A time limit in seconds that replaces the default of the host
    pub timeout_secs: Option<u32>,
}

impl Case {
    pub const fn new(name: &'static str, func: fn() -> TestResult) -> Self {
        Self {
            name,
            func,
            known_bug: None,
            crash: false,
            should_panic: None,
            timeout_secs: None,
        }
    }

    pub const fn known_bug(mut self, ticket: u32) -> Self {
        self.known_bug = Some(ticket);
        self
    }

    pub const fn crash(mut self) -> Self {
        self.crash = true;
        self
    }

    pub const fn should_panic(mut self, text: &'static str) -> Self {
        self.should_panic = Some(text);
        self
    }

    pub const fn timeout(mut self, secs: u32) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    /// Returns `true` if the expected result of this case stops the boot.
    /// The host must start QEMU again after such a case.
    pub const fn ends_boot(&self) -> bool {
        self.crash || self.should_panic.is_some()
    }
}
