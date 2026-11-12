//! Arguments for the user test program.
//!
//! STOPGAP: a user process has no arguments today. The host writes the
//! argument text into a marked block inside the ELF file of the test program
//! before it puts the file in the disk image. Remove this module when
//! `spawn` passes arguments to a process (GitHub #5 and #41).

/// The bytes that mark the start of the block in the ELF file
pub const ARGS_MAGIC: [u8; 16] = *b"@@AZPOS-TESTARGS";

/// The maximum length of the argument text in bytes
pub const ARGS_CAPACITY: usize = 240;

/// The block that holds the argument text. The text ends at the first zero
/// byte.
#[repr(C)]
pub struct ArgsBlock {
    pub magic: [u8; 16],
    pub text: [u8; ARGS_CAPACITY],
}

impl ArgsBlock {
    /// A block with empty text
    pub const EMPTY: Self = Self {
        magic: ARGS_MAGIC,
        text: [0; ARGS_CAPACITY],
    };
}

/// Guest side: copies the text of the block into `buf` and returns it.
///
/// The read is volatile, because the compiler sees only the empty block.
///
/// # Safety
/// `block` must point to a valid `ArgsBlock`.
pub unsafe fn read_args(block: *const ArgsBlock, buf: &mut [u8; ARGS_CAPACITY]) -> &str {
    let text = unsafe { &raw const (*block).text };
    *buf = unsafe { core::ptr::read_volatile(text) };

    let len = buf.iter().position(|b| *b == 0).unwrap_or(ARGS_CAPACITY);
    core::str::from_utf8(&buf[..len]).unwrap_or("")
}

/// The reason `patch_args` fails
#[derive(Debug, PartialEq, Eq)]
pub enum PatchError {
    /// The file contains no block
    BlockNotFound,
    /// The file contains more than one block
    BlockNotUnique,
    /// The text does not fit in the block
    TextTooLong,
}

/// Host side: writes `text` into the block of the ELF file in `elf`
pub fn patch_args(elf: &mut [u8], text: &str) -> Result<(), PatchError> {
    if text.len() >= ARGS_CAPACITY {
        return Err(PatchError::TextTooLong);
    }

    let mut blocks = elf
        .windows(ARGS_MAGIC.len())
        .enumerate()
        .filter(|(_, window)| *window == ARGS_MAGIC)
        .map(|(at, _)| at);

    let at = blocks.next().ok_or(PatchError::BlockNotFound)?;
    if blocks.next().is_some() {
        return Err(PatchError::BlockNotUnique);
    }

    let start = at + ARGS_MAGIC.len();
    let block = elf.get_mut(start..start + ARGS_CAPACITY).ok_or(PatchError::BlockNotFound)?;
    block.fill(0);
    block[..text.len()].copy_from_slice(text.as_bytes());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn file_with_blocks(count: usize) -> Vec<u8> {
        let mut file = Vec::from([0xAAu8; 100]);
        for _ in 0..count {
            file.extend_from_slice(&ARGS_MAGIC);
            file.extend_from_slice(&[0; ARGS_CAPACITY]);
            file.extend_from_slice(&[0xBB; 50]);
        }
        file
    }

    #[test]
    fn the_guest_reads_the_text_that_the_host_writes() {
        let mut file = file_with_blocks(1);
        patch_args(&mut file, "filter=seek start=4").unwrap();

        let block = file[100..].as_ptr() as *const ArgsBlock;
        let mut buf = [0; ARGS_CAPACITY];
        let text = unsafe { read_args(block, &mut buf) };
        assert_eq!(text, "filter=seek start=4");

        // The bytes around the block do not change.
        assert_eq!(file[99], 0xAA);
        assert_eq!(file[100 + 16 + ARGS_CAPACITY], 0xBB);
    }

    #[test]
    fn a_second_patch_replaces_the_first() {
        let mut file = file_with_blocks(1);
        patch_args(&mut file, "filter=a_long_filter_text").unwrap();
        patch_args(&mut file, "start=1").unwrap();

        let block = file[100..].as_ptr() as *const ArgsBlock;
        let mut buf = [0; ARGS_CAPACITY];
        assert_eq!(unsafe { read_args(block, &mut buf) }, "start=1");
    }

    #[test]
    fn an_empty_block_gives_empty_text() {
        let block = ArgsBlock::EMPTY;
        let mut buf = [0; ARGS_CAPACITY];
        assert_eq!(unsafe { read_args(&block, &mut buf) }, "");
    }

    #[test]
    fn patch_rejects_bad_input() {
        assert_eq!(patch_args(&mut file_with_blocks(0), "x"), Err(PatchError::BlockNotFound));
        assert_eq!(patch_args(&mut file_with_blocks(2), "x"), Err(PatchError::BlockNotUnique));

        let long = "x".repeat(ARGS_CAPACITY);
        assert_eq!(patch_args(&mut file_with_blocks(1), &long), Err(PatchError::TextTooLong));
    }
}
