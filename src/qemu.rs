use std::{path::Path, process::Command};

/// The `fw_cfg` file that holds the arguments of the kernel test runner. See
/// `kernel/src/ktest/mod.rs`.
pub const KTEST_ARGS_FILE: &str = "opt/azpos/ktest";

/// The exit status of QEMU after the test kernel writes `ExitCode::Success`
/// to the `isa-debug-exit` device. See `kernel/src/ktest/exit.rs`.
pub const EXIT_STATUS_SUCCESS: i32 = 33;
/// The exit status of QEMU after the test kernel panics
pub const EXIT_STATUS_PANIC: i32 = 35;

/// Returns the QEMU command with the machine options that all boots share.
/// The serial port of the guest is the standard output of QEMU.
pub fn base_command() -> Command {
    let mut cmd = Command::new("qemu-system-x86_64");
    cmd.arg("-m").arg("2G");
    cmd.arg("-M").arg("q35");
    cmd.arg("-serial").arg("stdio");
    cmd
}

/// Returns the QEMU command for one test boot of the BIOS image at `image`.
///
/// - No window.
/// - QEMU exits on a guest reset, so a triple fault ends the boot.
/// - The guest can stop QEMU through the `isa-debug-exit` device.
/// - QEMU discards all writes to the disk, so each boot sees the same disk.
/// - `kernel_args` reaches the kernel test runner through `fw_cfg`.
pub fn test_command(image: &Path, kernel_args: &str) -> Command {
    let mut cmd = base_command();
    cmd.arg("-display").arg("none");
    cmd.arg("-no-reboot");
    cmd.arg("-device").arg("isa-debug-exit,iobase=0xf4,iosize=0x04");
    cmd.arg("-drive").arg(format!("file={},format=raw,snapshot=on", escape(&image.display().to_string())));
    cmd.arg("-fw_cfg").arg(format!("name={KTEST_ARGS_FILE},string={}", escape(kernel_args)));
    cmd
}

/// QEMU splits an option value at each comma. A double comma is a literal
/// comma.
fn escape(value: &str) -> String {
    value.replace(',', ",,")
}
