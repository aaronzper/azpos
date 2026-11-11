/// Declares kernel test cases. Use it only in code that has `#[cfg(test)]`.
///
/// Each function becomes a `Case` that the kernel test runner collects. The
/// body must end with a statement. A check macro or `?` ends the case early
/// with a `Failure`.
///
/// ```ignore
/// ktest! {
///     /// A new table gives the IDs 0, 1, 2 in order.
///     fn ids_count_up_from_zero() {
///         let mut table: IDTable<u32, u8> = IDTable::new();
///         check_eq!(table.add_entry(7), 0);
///     }
///
///     #[known_bug(44, crash)]
///     fn read_after_seek_past_end_returns_zero() {
///         // ...
///     }
/// }
/// ```
///
/// Attributes:
/// - `#[known_bug(N)]`: ticket N makes this case fail. The case asserts the
///   correct behavior. Use `unfiled` for a bug with no ticket.
/// - `#[known_bug(N, crash)]`: the bug panics or hangs the kernel.
/// - `#[should_panic("text")]`: a kernel panic with this text is the correct
///   behavior.
/// - `#[timeout(secs)]`: a time limit for this case.
#[macro_export]
macro_rules! ktest {
    ($( $(#[$($attr:tt)*])* fn $name:ident() $body:block )*) => {
        $(
            #[test_case]
            #[allow(non_upper_case_globals)]
            static $name: $crate::Case = $crate::__case!($name, $body, $( [$($attr)*] )*);
        )*
    };
}

/// Declares the test cases of one module of a user test program.
///
/// The macro defines `pub static CASES: &[Case]` with the cases in source
/// order. The syntax and the attributes are the same as in `ktest!`. The
/// program must add `CASES` of each module to the list that it runs.
#[macro_export]
macro_rules! cases {
    ($( $(#[$($attr:tt)*])* fn $name:ident() $body:block )*) => {
        /// The test cases of this module, in source order
        pub static CASES: &[$crate::Case] = &[
            $( $crate::__case!($name, $body, $( [$($attr)*] )*) ),*
        ];
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __case {
    ($name:ident, $body:block, $( [$($attr:tt)*] )*) => {{
        #[allow(unreachable_code)]
        fn run() -> $crate::TestResult {
            $body
            Ok(())
        }

        let case = $crate::Case::new(
            concat!(module_path!(), "::", stringify!($name)),
            run,
        );
        $( let case = $crate::__case_attr!(case, $($attr)*); )*
        case
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __case_attr {
    ($case:ident, doc = $text:literal) => { $case };
    ($case:ident, known_bug(unfiled)) => { $case.known_bug(0) };
    ($case:ident, known_bug(unfiled, crash)) => { $case.known_bug(0).crash() };
    ($case:ident, known_bug($ticket:literal)) => { $case.known_bug($ticket) };
    ($case:ident, known_bug($ticket:literal, crash)) => { $case.known_bug($ticket).crash() };
    ($case:ident, should_panic($text:literal)) => { $case.should_panic($text) };
    ($case:ident, timeout($secs:literal)) => { $case.timeout($secs) };
}

/// Ends the test case with a `Failure` that contains the given message
#[macro_export]
macro_rules! fail {
    ($($arg:tt)+) => {
        return Err($crate::Failure::new($crate::__format!(
            "{}:{}: {}",
            file!(),
            line!(),
            $crate::__format!($($arg)+),
        )))
    };
}

/// Ends the test case with a `Failure` if the condition is false
#[macro_export]
macro_rules! check {
    ($cond:expr $(,)?) => {
        if !($cond) {
            $crate::fail!("check failed: {}", stringify!($cond));
        }
    };
    ($cond:expr, $($arg:tt)+) => {
        if !($cond) {
            $crate::fail!(
                "check failed: {}: {}",
                stringify!($cond),
                $crate::__format!($($arg)+),
            );
        }
    };
}

/// Ends the test case with a `Failure` if the two values are not equal
#[macro_export]
macro_rules! check_eq {
    ($left:expr, $right:expr $(,)?) => {
        match (&$left, &$right) {
            (left, right) => {
                if !(*left == *right) {
                    $crate::fail!(
                        "check failed: {} == {}: left = {:?}, right = {:?}",
                        stringify!($left),
                        stringify!($right),
                        left,
                        right,
                    );
                }
            }
        }
    };
}

/// Ends the test case with a `Failure` if the two values are equal
#[macro_export]
macro_rules! check_ne {
    ($left:expr, $right:expr $(,)?) => {
        match (&$left, &$right) {
            (left, right) => {
                if *left == *right {
                    $crate::fail!(
                        "check failed: {} != {}: both = {:?}",
                        stringify!($left),
                        stringify!($right),
                        left,
                    );
                }
            }
        }
    };
}

/// Ends the test case with a `Failure` if the value does not match the pattern
#[macro_export]
macro_rules! check_matches {
    ($value:expr, $pattern:pat $(,)?) => {
        match &$value {
            $pattern => (),
            other => {
                $crate::fail!(
                    "check failed: {} matches {}: got {:?}",
                    stringify!($value),
                    stringify!($pattern),
                    other,
                );
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::{Case, OrFail, TestResult};

    mod sample {
        crate::cases! {
            /// A case with no attribute.
            fn plain() {
                crate::check!(1 + 1 == 2);
            }

            #[known_bug(44, crash)]
            #[timeout(3)]
            fn bug() {
                crate::check_eq!(1, 2);
            }

            #[should_panic("out of bounds")]
            fn panics() {}

            #[known_bug(unfiled)]
            fn no_ticket() {}
        }
    }

    #[test]
    fn cases_macro_keeps_order_names_and_attributes() {
        let cases: &[Case] = sample::CASES;
        assert_eq!(cases.len(), 4);

        assert!(cases[0].name.ends_with("sample::plain"));
        assert!(!cases[0].ends_boot());
        assert!((cases[0].func)().is_ok());

        assert!(cases[1].name.ends_with("sample::bug"));
        assert_eq!(cases[1].known_bug, Some(44));
        assert!(cases[1].crash);
        assert_eq!(cases[1].timeout_secs, Some(3));

        assert_eq!(cases[2].should_panic, Some("out of bounds"));
        assert!(cases[2].ends_boot());

        assert_eq!(cases[3].known_bug, Some(0));
        assert!(!cases[3].crash);
    }

    #[test]
    fn check_eq_reports_file_line_and_both_values() {
        let failure = (sample::CASES[1].func)().unwrap_err();
        assert!(failure.message.contains("macros.rs:"));
        assert!(failure.message.contains("left = 1, right = 2"));
    }

    fn uses_or_fail() -> TestResult {
        let value: Option<u8> = None;
        value.or_fail("lookup")?;
        Ok(())
    }

    #[test]
    fn or_fail_converts_none_into_a_failure() {
        assert_eq!(uses_or_fail().unwrap_err().message, "lookup: got None");
    }
}
