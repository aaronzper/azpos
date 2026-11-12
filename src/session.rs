//! A test session: one or more QEMU boots that together run each selected
//! case one time.
//!
//! The guest prints protocol records on the serial port (see
//! `libtestkit::protocol`). The harness reads them and decides the outcome of
//! each case. A guest cannot recover from a kernel panic, so a case that
//! panics or hangs stops its boot. The harness then boots QEMU again and
//! tells the guest runner to start after that case.

use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use libtestkit::{
    protocol::{Event, Flags, SUITE_KERNEL, SUITE_USER},
    stopgap,
};

use crate::{
    artifacts,
    cli::Options,
    fixtures,
    image::{self, RootFile},
    outcome::{classify, ticket_text, Outcome, Raw},
    qemu,
};

/// The first line of a panic of the production kernel. See
/// `kernel/src/panic.rs`.
const KERNEL_PANIC_BANNER: &str = "!!! KERNEL PANIC !!!";
/// The start of the line that `libsystem` logs when a process panics. See
/// `libraries/libsystem/src/runtime.rs`.
const USER_PANIC_BANNER: &str = "!!! USER PANIC !!!";

/// The production kernel prints its panic message on the lines after the
/// banner. The harness waits this long for them.
const PANIC_TEXT_WAIT: Duration = Duration::from_millis(500);

/// The number of serial lines that the harness keeps for a failed case
const MAX_OUTPUT_LINES: usize = 100;

/// A limit on the boots of one session. It stops a session that makes no
/// progress.
const MAX_BOOTS: usize = 500;

/// The system that a session tests
pub enum Target {
    /// The test build of the kernel. It runs the kernel suite, then starts
    /// the user test program from `/programs/systest.exe`.
    TestKernel { kernel_elf: PathBuf },
    /// The production kernel, with the user test program in the place of the
    /// init process.
    Production,
}

/// The result of one case
#[derive(Debug, Clone)]
pub struct CaseResult {
    pub suite: String,
    pub name: String,
    pub outcome: Outcome,
    /// The serial output during the case, with no protocol records
    pub output: Vec<String>,
}

/// The results of one session
#[derive(Debug, Default)]
pub struct Report {
    pub cases: Vec<CaseResult>,
    /// Problems outside of a case, for example a boot that does not reach
    /// the first suite
    pub errors: Vec<String>,
    /// The suites that started in a boot of this session
    pub suites_seen: Vec<String>,
    pub boots: usize,
    pub elapsed: Duration,
}

impl Report {
    pub fn success(&self) -> bool {
        self.errors.is_empty() && !self.cases.iter().any(|c| c.outcome.is_failure())
    }

    fn count(&self, matches: impl Fn(&Outcome) -> bool) -> usize {
        self.cases.iter().filter(|c| matches(&c.outcome)).count()
    }

    /// Prints the details of each failure, then the totals
    pub fn print_summary(&self) {
        let failures: Vec<&CaseResult> =
            self.cases.iter().filter(|c| c.outcome.is_failure()).collect();

        if !failures.is_empty() {
            println!("\nfailures:");
        }
        for case in &failures {
            println!("\n---- {} ----", case.name);
            match &case.outcome {
                Outcome::Fail(message) => println!("{message}"),
                Outcome::Crash(message) => println!("crash: {message}"),
                Outcome::Timeout => println!("The case did not end before its time limit."),
                Outcome::XPass { ticket } => println!(
                    "The case passes, but it has the attribute `known_bug` ({}). \
                     If the bug is fixed, remove the attribute.",
                    ticket_text(*ticket)
                ),
                _ => (),
            }
            if !case.output.is_empty() {
                println!("serial output:");
                for line in &case.output {
                    println!("  | {line}");
                }
            }
        }

        for error in &self.errors {
            println!("\nharness error: {error}");
        }

        let skipped = self.count(|o| matches!(o, Outcome::Skipped));
        if skipped > 0 {
            println!(
                "\n{skipped} skipped cases stop the boot. Add `--full` to run them."
            );
        }

        println!(
            "\ntest result: {}. {} passed; {} failed; {} known bugs (xfail); {} skipped; \
             {} boots; finished in {:.1}s",
            if self.success() { "ok" } else { "FAILED" },
            self.count(|o| matches!(o, Outcome::Pass)),
            failures.len() + self.errors.len(),
            self.count(|o| matches!(o, Outcome::XFail { .. })),
            skipped,
            self.boots,
            self.elapsed.as_secs_f64(),
        );
    }
}

/// How one boot ended
#[derive(Debug, PartialEq, Eq)]
enum BootEnd {
    /// Each suite of this boot is complete
    Complete,
    /// A case stopped the boot. The next boot starts after it.
    CaseStoppedBoot { suite: String, index: usize },
    /// The boot ended outside of a case. The session cannot continue.
    Lost(String),
}

/// The case that runs now
struct Running {
    index: usize,
    name: String,
    flags: Flags,
    deadline: Instant,
}

/// Reads the serial output of one boot and records the result of each case.
///
/// This type does no I/O, so the unit tests can drive it with text.
struct Boot<'a> {
    options: &'a Options,
    /// The suites that this boot runs, in order
    expect: Vec<&'static str>,
    /// The number of suites in `expect` that are complete
    suites_done: usize,
    /// The suite that runs now
    suite: Option<String>,
    running: Option<Running>,
    /// The time limit for the next record when no case runs
    idle_deadline: Instant,
    /// The lines of a production kernel panic, and the time to stop the
    /// collection
    panic_text: Option<(Vec<String>, Instant)>,
    /// The serial lines since the current case started
    output: Vec<String>,
    /// The index of the last `start` or `skip` record of this boot
    last_index: Option<usize>,
}

impl<'a> Boot<'a> {
    fn new(options: &'a Options, expect: Vec<&'static str>, now: Instant) -> Self {
        Self {
            options,
            expect,
            suites_done: 0,
            suite: None,
            running: None,
            idle_deadline: now + options.boot_timeout,
            panic_text: None,
            output: Vec::new(),
            last_index: None,
        }
    }

    fn suite_name(&self) -> String {
        self.suite.clone().unwrap_or_else(|| "?".to_string())
    }

    fn record(&mut self, report: &mut Report, name: String, outcome: Outcome) {
        print_case_line(&name, &outcome);

        let output = std::mem::take(&mut self.output);
        report.cases.push(CaseResult { suite: self.suite_name(), name, outcome, output });
    }

    /// Ends the case that runs now with a result that stops the boot
    fn end_boot_in_case(&mut self, report: &mut Report, running: Running, raw: Raw) -> BootEnd {
        let outcome = classify(&running.flags, raw);
        self.record(report, running.name, outcome);
        BootEnd::CaseStoppedBoot { suite: self.suite_name(), index: running.index }
    }

    /// The kernel or the test process panicked
    fn on_crash(&mut self, report: &mut Report, message: String) -> BootEnd {
        match self.running.take() {
            Some(running) => self.end_boot_in_case(report, running, Raw::Crash(message)),
            None => BootEnd::Lost(format!("Panic outside of a test case: {message}")),
        }
    }

    /// Handles one line of serial output
    fn on_line(&mut self, line: &str, now: Instant, report: &mut Report) -> Option<BootEnd> {
        // After the panic banner of the production kernel, each line is a
        // part of the panic message.
        if let Some((lines, _)) = &mut self.panic_text {
            lines.push(line.trim().to_string());
            return None;
        }

        let Some(event) = Event::parse(line) else {
            if line.contains(KERNEL_PANIC_BANNER) {
                self.panic_text = Some((Vec::new(), now + PANIC_TEXT_WAIT));
            } else if let Some(at) = line.find(USER_PANIC_BANNER) {
                let message = line[at + USER_PANIC_BANNER.len()..].trim();
                return Some(self.on_crash(report, format!("user panic: {message}")));
            } else if !line.trim().is_empty() && self.output.len() < MAX_OUTPUT_LINES {
                self.output.push(line.to_string());
            }
            return None;
        };

        match event {
            Event::Suite { suite, total } => {
                // A later boot of the same session continues the suite
                if !self.options.list && !report.suites_seen.contains(&suite) {
                    println!("\nsuite {suite}: {total} cases");
                    report.suites_seen.push(suite.clone());
                }
                self.suite = Some(suite);
                self.idle_deadline = now + self.options.boot_timeout;
            }

            Event::Start { index, name, flags } => {
                let limit = flags.timeout_secs
                    .map(|secs| Duration::from_secs(secs as u64))
                    .unwrap_or(self.options.case_timeout);
                self.output.clear();
                self.last_index = Some(index);
                self.running = Some(Running { index, name, flags, deadline: now + limit });
            }

            Event::Ok { index } | Event::Fail { index, .. } => {
                let raw = match event {
                    Event::Fail { message, .. } => Raw::Fail(message),
                    _ => Raw::Ok,
                };

                match self.running.take() {
                    // A case with the `crash` flag did not crash, but it
                    // failed. The bug can leave the kernel in a bad state,
                    // for example after a write through a bad pointer. Do
                    // not run more cases in this boot.
                    Some(running) if running.index == index
                        && running.flags.crash
                        && raw != Raw::Ok =>
                    {
                        return Some(self.end_boot_in_case(report, running, raw));
                    }
                    Some(running) if running.index == index => {
                        let outcome = classify(&running.flags, raw);
                        self.record(report, running.name, outcome);
                    }
                    _ => {
                        return Some(BootEnd::Lost(format!(
                            "The result of case {index} has no matching start record"
                        )));
                    }
                }
                self.idle_deadline = now + self.options.boot_timeout;
            }

            Event::Skip { index, name, .. } => {
                self.output.clear();
                self.last_index = Some(index);
                self.record(report, name, Outcome::Skipped);
            }

            Event::Case { name, flags, .. } => {
                println!("{}: {name}{}", self.suite_name(), describe_flags(&flags));
            }

            Event::Done { suite } => {
                if self.expect.get(self.suites_done).copied() != Some(suite.as_str()) {
                    return Some(BootEnd::Lost(format!("Unexpected end of suite {suite}")));
                }
                self.suites_done += 1;
                self.suite = None;
                self.idle_deadline = now + self.options.boot_timeout;

                if self.suites_done == self.expect.len() {
                    return Some(BootEnd::Complete);
                }
            }

            Event::Panic { message } => return Some(self.on_crash(report, message)),
        }

        None
    }

    /// Checks the time limits. Call it at short intervals.
    fn on_tick(&mut self, now: Instant, report: &mut Report) -> Option<BootEnd> {
        if let Some((lines, until)) = &self.panic_text {
            if now >= *until {
                let message = lines.join(" ");
                self.panic_text = None;
                return Some(self.on_crash(report, message));
            }
            return None;
        }

        match &self.running {
            Some(running) if now >= running.deadline => {
                let running = self.running.take().unwrap();
                Some(self.end_boot_in_case(report, running, Raw::Timeout))
            }
            None if now >= self.idle_deadline => Some(BootEnd::Lost(format!(
                "No test record for {} s. The guest did not reach the next suite or case.",
                self.options.boot_timeout.as_secs()
            ))),
            _ => None,
        }
    }

    /// QEMU exited before the boot was complete
    fn on_exit(&mut self, status: Option<i32>, report: &mut Report) -> BootEnd {
        let status = match status {
            Some(code) => format!("status {code}"),
            None => "a signal".to_string(),
        };

        if let Some((lines, _)) = self.panic_text.take() {
            return self.on_crash(report, lines.join(" "));
        }

        match self.running.take() {
            Some(running) => {
                let message = format!(
                    "QEMU exited with {status} and no panic message (possible triple fault)"
                );
                self.end_boot_in_case(report, running, Raw::Crash(message))
            }
            None => BootEnd::Lost(format!("QEMU exited with {status} outside of a test case")),
        }
    }
}

fn describe_flags(flags: &Flags) -> String {
    let mut text = String::new();
    if let Some(ticket) = flags.known_bug {
        text.push_str(&format!(" [known bug {}]", ticket_text(ticket)));
    }
    if flags.crash {
        text.push_str(" [crash]");
    }
    if flags.should_panic.is_some() {
        text.push_str(" [should_panic]");
    }
    text
}

fn print_case_line(name: &str, outcome: &Outcome) {
    let text = match outcome {
        Outcome::Pass => "ok".to_string(),
        Outcome::Fail(_) => "FAILED".to_string(),
        Outcome::Crash(_) => "FAILED (crash)".to_string(),
        Outcome::Timeout => "FAILED (timeout)".to_string(),
        Outcome::XFail { ticket, .. } => format!("xfail (known bug {})", ticket_text(*ticket)),
        Outcome::XPass { ticket } => format!("FAILED (xpass, known bug {})", ticket_text(*ticket)),
        Outcome::Skipped => "skipped (stops the boot)".to_string(),
    };
    println!("test {name} ... {text}");
}

/// A message from a thread that reads QEMU output
enum Message {
    /// One line of serial output
    Serial(String),
    /// One line of the QEMU error output
    QemuError(String),
    /// The serial output is closed: QEMU exited
    Closed,
}

fn read_lines(stream: impl Read + Send + 'static, wrap: fn(String) -> Message, end: Option<Message>)
    -> impl FnOnce(mpsc::Sender<Message>) + Send + 'static
{
    move |sender| {
        let mut reader = BufReader::new(stream);
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            match reader.read_until(b'\n', &mut bytes) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&bytes);
                    let line = line.trim_end_matches(['\r', '\n']).to_string();
                    if sender.send(wrap(line)).is_err() {
                        return;
                    }
                }
            }
        }
        if let Some(end) = end {
            let _ = sender.send(end);
        }
    }
}

fn spawn_readers(child: &mut Child) -> Receiver<Message> {
    let (sender, receiver) = mpsc::channel();

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let serial = read_lines(stdout, Message::Serial, Some(Message::Closed));
    let errors = read_lines(stderr, Message::QemuError, None);

    let sender_2 = sender.clone();
    thread::spawn(move || serial(sender));
    thread::spawn(move || errors(sender_2));

    receiver
}

/// A QEMU process that stops when this value drops.
///
/// A panic in the harness then does not leave QEMU alive.
struct QemuChild(Child);

impl Drop for QemuChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Boots QEMU one time and reads its output until the boot ends.
///
/// Also returns the index of the last case that started or was skipped.
fn boot_once(
    image: &Path,
    kernel_args: &str,
    expect: Vec<&'static str>,
    options: &Options,
    report: &mut Report,
) -> (BootEnd, Option<usize>) {
    let mut command = qemu::test_command(image, kernel_args);
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => QemuChild(child),
        Err(e) => {
            return (BootEnd::Lost(format!("Cannot start qemu-system-x86_64: {e}")), None);
        }
    };
    let receiver = spawn_readers(&mut child.0);

    let mut boot = Boot::new(options, expect, Instant::now());
    let mut qemu_errors = Vec::new();

    let end = loop {
        let end = match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(Message::Serial(line)) => {
                if options.verbose {
                    println!("  | {line}");
                }
                boot.on_line(&line, Instant::now(), report)
            }
            Ok(Message::QemuError(line)) => {
                qemu_errors.push(line);
                None
            }
            Ok(Message::Closed) | Err(RecvTimeoutError::Disconnected) => {
                let status = child.0.wait().ok().and_then(|s| s.code());
                Some(boot.on_exit(status, report))
            }
            Err(RecvTimeoutError::Timeout) => boot.on_tick(Instant::now(), report),
        };

        if let Some(end) = end {
            break end;
        }
    };

    // STOPGAP: the user test program cannot exit (GitHub #4) and the
    // production kernel does not stop QEMU. The drop of `child` stops QEMU.
    drop(child);
    let _ = std::io::stdout().flush();

    let end = match end {
        BootEnd::Lost(reason) if !qemu_errors.is_empty() => {
            BootEnd::Lost(format!("{reason}\nQEMU error output:\n{}", qemu_errors.join("\n")))
        }
        end => end,
    };
    (end, boot.last_index)
}

/// Returns the argument text that both guest runners understand
fn guest_args(options: &Options, start: usize, limit: Option<usize>) -> String {
    let mut args = format!(
        "start={start} full={} list={}",
        options.full as u8,
        options.list as u8,
    );
    if let Some(limit) = limit {
        args.push_str(&format!(" limit={limit}"));
    }
    if let Some(filter) = &options.filter {
        args.push_str(&format!(" filter={filter}"));
    }
    args
}

/// Returns the path of the disk image of this session.
///
/// The name contains the process ID, so two sessions can run at the same time.
fn image_path(target: &Target) -> PathBuf {
    let name = match target {
        Target::TestKernel { .. } => "ktest",
        Target::Production => "e2e",
    };
    artifacts::work_dir().join(format!("{name}-{}.img", std::process::id()))
}

/// Builds the disk image for one boot at `path`
fn build_image(target: &Target, user_args: &str, path: &Path) {
    // STOPGAP: the user test program has no arguments (GitHub #5, #41).
    // The harness writes them into the ELF file.
    let mut systest = std::fs::read(artifacts::systest_elf()).expect("Cannot read the systest ELF");
    stopgap::patch_args(&mut systest, user_args)
        .expect("Cannot write the arguments into the systest ELF");

    let mut files = fixtures::files();

    let kernel = match target {
        Target::TestKernel { kernel_elf } => {
            files.push(RootFile::from_host("Programs/adam.exe", &artifacts::adam_elf()));
            files.push(RootFile::new("Programs/systest.exe", systest));
            kernel_elf.clone()
        }
        Target::Production => {
            // STOPGAP: no process can start another process (GitHub #5), so
            // the test program takes the place of the init process.
            files.push(RootFile::new("Programs/adam.exe", systest));
            artifacts::kernel_elf()
        }
    };

    image::create_bios_image(&kernel, path, &files);
}

/// The arguments and suites of one boot
#[derive(Debug, PartialEq, Eq)]
struct BootPlan {
    /// The suites that the boot runs, in order
    suites: Vec<&'static str>,
    kernel_start: usize,
    user_start: usize,
    /// The maximum number of cases that each suite selects
    limit: Option<usize>,
}

/// The state of the boot loop of one session.
///
/// This type does no I/O, so the unit tests can drive it with fake boot ends.
#[derive(Debug)]
struct Schedule {
    /// The suites that are not complete, in order
    suites: Vec<&'static str>,
    kernel_start: usize,
    user_start: usize,
    /// One boot for each case
    isolate: bool,
}

impl Schedule {
    fn new(suites: Vec<&'static str>, isolate: bool) -> Self {
        Self { suites, kernel_start: 0, user_start: 0, isolate }
    }

    /// Returns the plan of the next boot, or `None` if the session is complete
    fn next_boot(&self) -> Option<BootPlan> {
        let first = *self.suites.first()?;
        let (suites, limit) = if self.isolate {
            (vec![first], Some(1))
        } else {
            (self.suites.clone(), None)
        };
        Some(BootPlan {
            suites,
            kernel_start: self.kernel_start,
            user_start: self.user_start,
            limit,
        })
    }

    fn set_start(&mut self, suite: &str, start: usize) {
        if suite == SUITE_USER {
            self.user_start = start;
        } else {
            self.kernel_start = start;
        }
    }

    /// Records how a boot ended. Returns `Err` if the session cannot continue.
    ///
    /// `last_index` is the index of the last case that started or was skipped.
    fn on_boot_end(&mut self, end: BootEnd, last_index: Option<usize>) -> Result<(), String> {
        match end {
            BootEnd::Complete if self.isolate => match last_index {
                // The boot ran one case. The next boot runs the next case.
                Some(index) => {
                    let suite = self.suites[0];
                    self.set_start(suite, index + 1);
                }
                // The boot selected no case, so the suite is complete
                None => {
                    self.suites.remove(0);
                }
            },
            BootEnd::Complete => self.suites.clear(),
            BootEnd::CaseStoppedBoot { suite, index } => {
                // The suites before this suite are complete
                if let Some(at) = self.suites.iter().position(|s| *s == suite) {
                    self.suites.drain(..at);
                }
                self.set_start(&suite, index + 1);
            }
            BootEnd::Lost(reason) => return Err(reason),
        }
        Ok(())
    }
}

/// Runs a test session and returns its results. Prints each case as it ends.
pub fn run(target: &Target, options: &Options) -> Report {
    let started = Instant::now();
    let mut report = Report::default();

    let mut suites = Vec::new();
    if matches!(target, Target::TestKernel { .. }) && options.kernel {
        suites.push(SUITE_KERNEL);
    }
    if matches!(target, Target::Production) || options.user {
        suites.push(SUITE_USER);
    }
    let mut schedule = Schedule::new(suites, options.isolate);
    let image = image_path(target);

    while let Some(plan) = schedule.next_boot() {
        report.boots += 1;
        if report.boots > MAX_BOOTS {
            report.errors.push(format!("The session needs more than {MAX_BOOTS} boots"));
            break;
        }

        let kernel_args = format!(
            "{} kernel={} user={}",
            guest_args(options, plan.kernel_start, plan.limit),
            plan.suites.contains(&SUITE_KERNEL) as u8,
            plan.suites.contains(&SUITE_USER) as u8,
        );
        let user_args = guest_args(options, plan.user_start, plan.limit);

        let boot_started = Instant::now();
        build_image(target, &user_args, &image);
        let image_time = boot_started.elapsed();

        let (end, last_index) = boot_once(&image, &kernel_args, plan.suites, options, &mut report);
        if options.verbose {
            println!(
                "  boot {}: image {:.1}s, QEMU {:.1}s, end: {end:?}",
                report.boots,
                image_time.as_secs_f64(),
                (boot_started.elapsed() - image_time).as_secs_f64(),
            );
        }

        if let Err(reason) = schedule.on_boot_end(end, last_index) {
            report.errors.push(reason);
            break;
        }
    }

    let _ = std::fs::remove_file(&image);
    report.elapsed = started.elapsed();
    report
}

/// Runs a session, prints the summary and returns the process exit code
pub fn run_and_report(target: &Target, options: &Options) -> i32 {
    let report = run(target, options);

    if options.list {
        return if report.errors.is_empty() { 0 } else { 1 };
    }

    report.print_summary();
    if report.success() { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds serial lines to a `Boot` and returns the report and the end
    fn feed(expect: Vec<&'static str>, lines: &[&str]) -> (Report, Option<BootEnd>) {
        let options = Options::default();
        let now = Instant::now();
        let mut report = Report::default();
        let mut boot = Boot::new(&options, expect, now);

        for line in lines {
            if let Some(end) = boot.on_line(line, now, &mut report) {
                return (report, Some(end));
            }
        }
        (report, None)
    }

    fn outcomes(report: &Report) -> Vec<&Outcome> {
        report.cases.iter().map(|c| &c.outcome).collect()
    }

    #[test]
    fn a_boot_with_two_suites_is_complete_after_the_second() {
        let (report, end) = feed(vec![SUITE_KERNEL, SUITE_USER], &[
            "Hello world!",
            "@@T suite kernel 2",
            "@@T start 0 k::a",
            "@@T ok 0",
            "@@T start 1 k::b",
            "@@T fail 1 src/x.rs:1: check failed: x",
            "@@T done kernel",
            "PID 0 attatched to logger",
            "[PID 0] @@T suite user 1",
            "[PID 0] @@T start 0 u::a",
            "[PID 0] @@T ok 0",
            "[PID 0] @@T done user",
        ]);

        assert_eq!(end, Some(BootEnd::Complete));
        assert_eq!(report.cases.len(), 3);
        assert_eq!(report.cases[2].suite, "user");
        assert!(matches!(outcomes(&report)[..], [Outcome::Pass, Outcome::Fail(_), Outcome::Pass]));
        assert!(!report.success());
    }

    #[test]
    fn a_test_kernel_panic_in_a_known_bug_case_is_xfail_and_stops_the_boot() {
        let (report, end) = feed(vec![SUITE_KERNEL], &[
            "@@T suite kernel 9",
            "@@T start 4 k::seek known_bug=44 crash",
            "@@T panic src/blob.rs:27:25: range start index 4 out of range",
        ]);

        assert_eq!(end, Some(BootEnd::CaseStoppedBoot { suite: "kernel".into(), index: 4 }));
        assert!(matches!(outcomes(&report)[..], [Outcome::XFail { ticket: 44, .. }]));
        assert!(report.success());
    }

    #[test]
    fn a_production_kernel_panic_collects_the_lines_after_the_banner() {
        let options = Options::default();
        let now = Instant::now();
        let mut report = Report::default();
        let mut boot = Boot::new(&options, vec![SUITE_USER], now);

        for line in [
            "[PID 0] @@T suite user 3",
            "[PID 0] @@T start 2 u::seek",
            "!!! KERNEL PANIC !!!",
            "From thread 2 (process 0)",
            "panicked at kernel/src/blob.rs:27:25:",
            "range start index 4 out of range",
        ] {
            assert_eq!(boot.on_line(line, now, &mut report), None);
        }

        // The message is complete only after the wait
        assert_eq!(boot.on_tick(now, &mut report), None);
        let end = boot.on_tick(now + PANIC_TEXT_WAIT, &mut report);

        assert_eq!(end, Some(BootEnd::CaseStoppedBoot { suite: "user".into(), index: 2 }));
        match &report.cases[0].outcome {
            Outcome::Crash(message) => assert!(message.contains("range start index 4")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_user_panic_stops_the_boot() {
        let (report, end) = feed(vec![SUITE_USER], &[
            "[PID 0] @@T suite user 3",
            "[PID 0] @@T start 1 u::utf8 known_bug=34 crash",
            "[PID 0] !!! USER PANIC !!! panicked at libsci/src/resources/mod.rs:72:14: Invalid",
        ]);

        assert_eq!(end, Some(BootEnd::CaseStoppedBoot { suite: "user".into(), index: 1 }));
        assert!(matches!(outcomes(&report)[..], [Outcome::XFail { ticket: 34, .. }]));
    }

    #[test]
    fn a_case_that_does_not_end_is_a_timeout() {
        let options = Options::default();
        let now = Instant::now();
        let mut report = Report::default();
        let mut boot = Boot::new(&options, vec![SUITE_KERNEL], now);

        boot.on_line("@@T suite kernel 1", now, &mut report);
        boot.on_line("@@T start 0 k::hang timeout=3", now, &mut report);

        assert_eq!(boot.on_tick(now + Duration::from_secs(2), &mut report), None);
        let end = boot.on_tick(now + Duration::from_secs(3), &mut report);

        assert_eq!(end, Some(BootEnd::CaseStoppedBoot { suite: "kernel".into(), index: 0 }));
        assert_eq!(outcomes(&report), [&Outcome::Timeout]);
    }

    #[test]
    fn a_boot_that_prints_no_record_is_lost() {
        let options = Options::default();
        let now = Instant::now();
        let mut report = Report::default();
        let mut boot = Boot::new(&options, vec![SUITE_KERNEL], now);

        boot.on_line("Hello world!", now, &mut report);
        let end = boot.on_tick(now + options.boot_timeout, &mut report);
        assert!(matches!(end, Some(BootEnd::Lost(_))));
    }

    #[test]
    fn a_qemu_exit_in_a_case_is_a_crash_and_outside_a_case_is_lost() {
        let options = Options::default();
        let now = Instant::now();
        let mut report = Report::default();

        let mut boot = Boot::new(&options, vec![SUITE_KERNEL], now);
        boot.on_line("@@T suite kernel 1", now, &mut report);
        boot.on_line("@@T start 0 k::fault", now, &mut report);
        let end = boot.on_exit(Some(0), &mut report);
        assert_eq!(end, BootEnd::CaseStoppedBoot { suite: "kernel".into(), index: 0 });
        assert!(matches!(outcomes(&report)[..], [Outcome::Crash(_)]));

        let mut boot = Boot::new(&options, vec![SUITE_KERNEL], now);
        assert!(matches!(boot.on_exit(Some(1), &mut report), BootEnd::Lost(_)));
    }

    #[test]
    fn a_crash_case_that_fails_with_no_crash_stops_the_boot() {
        let (report, end) = feed(vec![SUITE_USER], &[
            "[PID 0] @@T suite user 2",
            "[PID 0] @@T start 0 u::bad_pointer known_bug=26 crash",
            "[PID 0] @@T fail 0 src/x.rs:1: check failed: x",
        ]);

        assert_eq!(end, Some(BootEnd::CaseStoppedBoot { suite: "user".into(), index: 0 }));
        assert!(matches!(outcomes(&report)[..], [Outcome::XFail { ticket: 26, .. }]));
    }

    #[test]
    fn a_skipped_case_does_not_fail_the_run() {
        let (report, end) = feed(vec![SUITE_KERNEL], &[
            "@@T suite kernel 1",
            "@@T skip 0 k::seek known_bug=44 crash",
            "@@T done kernel",
        ]);

        assert_eq!(end, Some(BootEnd::Complete));
        assert_eq!(outcomes(&report), [&Outcome::Skipped]);
        assert!(report.success());
    }

    fn stopped(suite: &str, index: usize) -> BootEnd {
        BootEnd::CaseStoppedBoot { suite: suite.into(), index }
    }

    #[test]
    fn a_normal_session_runs_both_suites_in_one_boot() {
        let mut schedule = Schedule::new(vec![SUITE_KERNEL, SUITE_USER], false);
        let plan = schedule.next_boot().unwrap();
        assert_eq!(plan.suites, [SUITE_KERNEL, SUITE_USER]);
        assert_eq!(plan.limit, None);

        schedule.on_boot_end(BootEnd::Complete, Some(7)).unwrap();
        assert_eq!(schedule.next_boot(), None);
    }

    #[test]
    fn a_stop_in_the_user_suite_continues_with_only_the_user_suite() {
        let mut schedule = Schedule::new(vec![SUITE_KERNEL, SUITE_USER], false);
        schedule.on_boot_end(stopped(SUITE_KERNEL, 3), Some(3)).unwrap();
        let plan = schedule.next_boot().unwrap();
        assert_eq!(plan.suites, [SUITE_KERNEL, SUITE_USER]);
        assert_eq!(plan.kernel_start, 4);

        schedule.on_boot_end(stopped(SUITE_USER, 1), Some(1)).unwrap();
        let plan = schedule.next_boot().unwrap();
        assert_eq!(plan.suites, [SUITE_USER]);
        assert_eq!(plan.user_start, 2);
    }

    #[test]
    fn an_isolated_session_runs_one_case_in_each_boot() {
        let mut schedule = Schedule::new(vec![SUITE_KERNEL, SUITE_USER], true);
        let mut boots = Vec::new();

        // Fake boots: the kernel suite selects cases 0 and 2. Case 2 stops
        // the boot. The user suite selects case 0.
        let ends = [
            (BootEnd::Complete, Some(0)),
            (stopped(SUITE_KERNEL, 2), Some(2)),
            (BootEnd::Complete, None),
            (BootEnd::Complete, Some(0)),
            (BootEnd::Complete, None),
        ];
        for (end, last_index) in ends {
            boots.push(schedule.next_boot().unwrap());
            schedule.on_boot_end(end, last_index).unwrap();
        }
        assert_eq!(schedule.next_boot(), None);

        let starts: Vec<_> = boots.iter()
            .map(|p| (p.suites.clone(), p.kernel_start, p.user_start, p.limit))
            .collect();
        assert_eq!(starts, [
            (vec![SUITE_KERNEL], 0, 0, Some(1)),
            (vec![SUITE_KERNEL], 1, 0, Some(1)),
            (vec![SUITE_KERNEL], 3, 0, Some(1)),
            (vec![SUITE_USER], 3, 0, Some(1)),
            (vec![SUITE_USER], 3, 1, Some(1)),
        ]);
    }

    #[test]
    fn a_lost_boot_ends_the_session() {
        let mut schedule = Schedule::new(vec![SUITE_USER], true);
        assert!(schedule.on_boot_end(BootEnd::Lost("x".into()), None).is_err());
    }

    #[test]
    fn a_boot_records_the_last_started_or_skipped_index() {
        let options = Options::default();
        let now = Instant::now();
        let mut report = Report::default();
        let mut boot = Boot::new(&options, vec![SUITE_KERNEL], now);
        assert_eq!(boot.last_index, None);

        boot.on_line("@@T suite kernel 5", now, &mut report);
        boot.on_line("@@T start 1 k::a", now, &mut report);
        boot.on_line("@@T ok 1", now, &mut report);
        assert_eq!(boot.last_index, Some(1));
        boot.on_line("@@T skip 3 k::b crash", now, &mut report);
        assert_eq!(boot.last_index, Some(3));
    }

    #[test]
    fn a_failed_case_keeps_its_serial_output() {
        let (report, _) = feed(vec![SUITE_KERNEL], &[
            "@@T suite kernel 1",
            "noise before the case",
            "@@T start 0 k::a",
            "debug line from the case",
            "@@T fail 0 bad",
        ]);

        assert_eq!(report.cases[0].output, ["debug line from the case"]);
    }
}
