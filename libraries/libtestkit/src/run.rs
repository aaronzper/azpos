use alloc::string::ToString;

use crate::{
    protocol::{Event, Flags},
    Args, Case,
};

/// Runs the cases of one suite in order and reports each step through `emit`.
///
/// `emit` receives one protocol record for each call, with no line end. It
/// must write the record on a line of its own on the serial port.
///
/// The index of a case is its position in `cases`. The index stays the same
/// with any filter, so the host can use it to continue after a crash.
pub fn run_suite(suite: &str, cases: &[&Case], args: &Args, emit: &mut dyn FnMut(&str)) {
    let mut send = |event: Event| emit(&event.to_line());

    send(Event::Suite { suite: suite.to_string(), total: cases.len() });

    let mut selected = 0;
    for (index, case) in cases.iter().enumerate() {
        if index < args.start {
            continue;
        }
        if let Some(filter) = args.filter {
            if !case.name.contains(filter) {
                continue;
            }
        }

        // The limit counts each selected case, also one that is skipped.
        if args.limit.is_some_and(|limit| selected >= limit) {
            break;
        }
        selected += 1;

        let name = case.name.to_string();
        let flags = Flags::of(case);

        if args.list {
            send(Event::Case { index, name, flags });
            continue;
        }
        if case.ends_boot() && !args.full {
            send(Event::Skip { index, name, flags });
            continue;
        }

        send(Event::Start { index, name, flags });
        match (case.func)() {
            Ok(()) => send(Event::Ok { index }),
            Err(failure) => send(Event::Fail { index, message: failure.message }),
        }
    }

    send(Event::Done { suite: suite.to_string() });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Failure, TestResult};
    use alloc::{string::String, vec::Vec};

    fn pass() -> TestResult {
        Ok(())
    }

    fn fail() -> TestResult {
        Err(Failure::new("bad".to_string()))
    }

    fn panics() -> TestResult {
        panic!("the runner must not call this case");
    }

    static PASS: Case = Case::new("m::pass", pass);
    static FAIL: Case = Case::new("m::fail", fail);
    static CRASH: Case = Case::new("m::crash", panics).known_bug(44).crash();

    fn run(args: &str) -> Vec<String> {
        let mut lines = Vec::new();
        run_suite("kernel", &[&PASS, &FAIL, &CRASH], &Args::parse(args), &mut |line| {
            lines.push(line.to_string())
        });
        lines
    }

    #[test]
    fn default_run_skips_the_case_that_stops_the_boot() {
        assert_eq!(
            run(""),
            [
                "@@T suite kernel 3",
                "@@T start 0 m::pass",
                "@@T ok 0",
                "@@T start 1 m::fail",
                "@@T fail 1 bad",
                "@@T skip 2 m::crash known_bug=44 crash",
                "@@T done kernel",
            ]
        );
    }

    #[test]
    fn start_and_limit_select_a_range_and_keep_the_index() {
        assert_eq!(
            run("start=1 limit=1"),
            ["@@T suite kernel 3", "@@T start 1 m::fail", "@@T fail 1 bad", "@@T done kernel"]
        );
    }

    #[test]
    fn filter_selects_by_name() {
        assert_eq!(
            run("filter=pass"),
            ["@@T suite kernel 3", "@@T start 0 m::pass", "@@T ok 0", "@@T done kernel"]
        );
    }

    #[test]
    fn list_mode_runs_nothing() {
        assert_eq!(
            run("list=1 full=1"),
            [
                "@@T suite kernel 3",
                "@@T case 0 m::pass",
                "@@T case 1 m::fail",
                "@@T case 2 m::crash known_bug=44 crash",
                "@@T done kernel",
            ]
        );
    }
}
