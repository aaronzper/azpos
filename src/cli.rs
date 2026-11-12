use std::time::Duration;

/// The options of one test run. `tests/ktest.rs` and `tests/e2e.rs` read them
/// from the arguments after `--` on the `cargo test` command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Run only the cases with a name that contains this text
    pub filter: Option<String>,
    /// Also run the cases that stop the boot: known bugs that crash, and
    /// `should_panic` cases
    pub full: bool,
    /// Print the cases and run none
    pub list: bool,
    /// Run the kernel suite (test kernel only)
    pub kernel: bool,
    /// Run the user suite
    pub user: bool,
    /// Print each line of serial output
    pub verbose: bool,
    /// Boot QEMU one time for each case
    pub isolate: bool,
    /// The time limit of one case
    pub case_timeout: Duration,
    /// The time limit from the QEMU start to the first suite, and between
    /// two cases
    pub boot_timeout: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            filter: None,
            full: false,
            list: false,
            kernel: true,
            user: true,
            verbose: false,
            isolate: false,
            case_timeout: Duration::from_secs(20),
            boot_timeout: Duration::from_secs(60),
        }
    }
}

pub const USAGE: &str = "\
Options (after `--` on the cargo command line):
  [FILTER]          Run only the cases with a name that contains FILTER.
                    The name starts with the module path, so `blob`
                    selects the cases of `blob.rs`.
  --full            Also run the cases that stop the boot (known bugs that
                    crash, and should_panic cases). Each one costs a boot.
  --list            Print the cases and run none
  --isolate         Boot QEMU one time for each case. This finds a case
                    that depends on another case.
  --kernel-only     Run only the kernel suite
  --user-only       Run only the user suite
  --timeout SECS    The time limit of one case (default 20)
  -v, --verbose     Print each line of serial output
  -h, --help        Print this text";

impl Options {
    /// Parses the arguments. Returns `Err` with a message for the user if
    /// the arguments are invalid or ask for the help text.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--full" => options.full = true,
                "--list" => options.list = true,
                "--isolate" => options.isolate = true,
                "--kernel-only" => options.user = false,
                "--user-only" => options.kernel = false,
                "-v" | "--verbose" | "--nocapture" => options.verbose = true,
                "--timeout" => {
                    let secs = args.next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("--timeout needs a number of seconds")?;
                    options.case_timeout = Duration::from_secs(secs);
                }
                "-h" | "--help" => return Err(USAGE.to_string()),
                flag if flag.starts_with('-') => {
                    return Err(format!("Unknown option: {flag}\n{USAGE}"));
                }
                filter => {
                    if options.filter.is_some() {
                        return Err(format!("More than one filter\n{USAGE}"));
                    }
                    // A space or a comma would break the argument text
                    if filter.contains([' ', ',', '=']) {
                        return Err("A filter cannot contain a space, `,` or `=`".to_string());
                    }
                    options.filter = Some(filter.to_string());
                }
            }
        }

        if !options.kernel && !options.user {
            return Err("--kernel-only and --user-only exclude each other".to_string());
        }
        if options.isolate && options.list {
            return Err("--isolate and --list exclude each other".to_string());
        }

        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn no_arguments_gives_the_defaults() {
        assert_eq!(parse(&[]), Ok(Options::default()));
    }

    #[test]
    fn parses_filter_and_flags() {
        let options = parse(&[
            "fat::", "--full", "--kernel-only", "--timeout", "5", "-v", "--isolate",
        ]).unwrap();
        assert!(options.isolate);
        assert_eq!(options.filter.as_deref(), Some("fat::"));
        assert!(options.full);
        assert!(options.kernel);
        assert!(!options.user);
        assert!(options.verbose);
        assert_eq!(options.case_timeout, Duration::from_secs(5));
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(parse(&["--bogus"]).is_err());
        assert!(parse(&["a", "b"]).is_err());
        assert!(parse(&["--timeout"]).is_err());
        assert!(parse(&["--kernel-only", "--user-only"]).is_err());
        assert!(parse(&["a=b"]).is_err());
        assert!(parse(&["--isolate", "--list"]).is_err());
    }
}
