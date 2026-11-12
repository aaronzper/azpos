//! QEMU firmware configuration (`fw_cfg`) device, read through I/O ports.
//!
//! The host passes data to the guest with the QEMU option
//! `-fw_cfg name=<file>,string=<text>`. See the QEMU document
//! `docs/specs/fw_cfg.rst`.

use alloc::vec::Vec;
use x86_64::instructions::port::Port;

const PORT_SELECTOR: u16 = 0x510;
const PORT_DATA: u16 = 0x511;

/// Selector of the signature. The data is the text `QEMU`.
const KEY_SIGNATURE: u16 = 0x0000;
/// Selector of the file directory
const KEY_FILE_DIR: u16 = 0x0019;

const FILE_NAME_LEN: usize = 56;

fn select(key: u16) {
    unsafe { Port::<u16>::new(PORT_SELECTOR).write(key) };
}

fn read_u8() -> u8 {
    unsafe { Port::<u8>::new(PORT_DATA).read() }
}

/// Reads a big-endian `u16` from the selected item
fn read_u16() -> u16 {
    u16::from_be_bytes([read_u8(), read_u8()])
}

/// Reads a big-endian `u32` from the selected item
fn read_u32() -> u32 {
    u32::from_be_bytes([read_u8(), read_u8(), read_u8(), read_u8()])
}

/// Returns `true` if the machine has a `fw_cfg` device
pub fn present() -> bool {
    select(KEY_SIGNATURE);
    [read_u8(), read_u8(), read_u8(), read_u8()] == *b"QEMU"
}

/// Returns the contents of the file `name`. Returns `None` if the machine
/// has no `fw_cfg` device or the device has no such file.
pub fn read_file(name: &str) -> Option<Vec<u8>> {
    if !present() {
        return None;
    }

    // The directory is a count, then one entry for each file. An entry is
    // the size (u32), the selector (u16), 2 reserved bytes and the name.
    select(KEY_FILE_DIR);
    let count = read_u32();

    let mut found = None;
    for _ in 0..count {
        let size = read_u32();
        let key = read_u16();
        let _reserved = read_u16();

        let mut file_name = [0u8; FILE_NAME_LEN];
        for byte in file_name.iter_mut() {
            *byte = read_u8();
        }
        let len = file_name.iter().position(|b| *b == 0).unwrap_or(FILE_NAME_LEN);

        if &file_name[..len] == name.as_bytes() {
            found = Some((size, key));
            break;
        }
    }

    let (size, key) = found?;
    select(key);
    Some((0..size).map(|_| read_u8()).collect())
}
