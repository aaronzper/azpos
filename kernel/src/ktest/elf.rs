//! A builder for small 64-bit little-endian ELF files.
//!
//! The ELF loader cases use it to make files that the compiler cannot make.

use alloc::{boxed::Box, vec::Vec};

/// The length of the ELF file header
pub const EHDR_LEN: usize = 64;
/// The length of one program header
pub const PHDR_LEN: usize = 56;

/// One program header and its data
pub struct Segment<'a> {
    /// The value of `p_type`, for example `PT_LOAD` (1)
    pub p_type: u32,
    /// The value of `p_flags`
    pub flags: u32,
    /// The virtual address of the segment
    pub vaddr: u64,
    /// The size of the segment in memory. It is not less than `data.len()`.
    pub memsz: u64,
    /// The bytes of the segment in the file
    pub data: &'a [u8],
}

/// Returns an ELF file with the given type, entry address and segments.
///
/// The data of the segments follows the program headers.
pub fn minimal_elf(e_type: u16, entry: u64, segments: &[Segment]) -> Box<[u8]> {
    let mut out = Vec::new();

    // e_ident: magic, 64-bit class, little-endian, version 1
    out.extend_from_slice(&[0x7F, b'E', b'L', b'F', 2, 1, 1, 0]);
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&e_type.to_le_bytes());
    out.extend_from_slice(&0x3Eu16.to_le_bytes()); // e_machine: x86-64
    out.extend_from_slice(&1u32.to_le_bytes()); // e_version
    out.extend_from_slice(&entry.to_le_bytes());
    out.extend_from_slice(&(EHDR_LEN as u64).to_le_bytes()); // e_phoff
    out.extend_from_slice(&0u64.to_le_bytes()); // e_shoff
    out.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    out.extend_from_slice(&(EHDR_LEN as u16).to_le_bytes()); // e_ehsize
    out.extend_from_slice(&(PHDR_LEN as u16).to_le_bytes()); // e_phentsize
    out.extend_from_slice(&(segments.len() as u16).to_le_bytes()); // e_phnum
    out.extend_from_slice(&0u16.to_le_bytes()); // e_shentsize
    out.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
    out.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx

    let mut data_offset = EHDR_LEN + PHDR_LEN * segments.len();
    for s in segments {
        out.extend_from_slice(&s.p_type.to_le_bytes());
        out.extend_from_slice(&s.flags.to_le_bytes());
        out.extend_from_slice(&(data_offset as u64).to_le_bytes()); // p_offset
        out.extend_from_slice(&s.vaddr.to_le_bytes()); // p_vaddr
        out.extend_from_slice(&s.vaddr.to_le_bytes()); // p_paddr
        out.extend_from_slice(&(s.data.len() as u64).to_le_bytes()); // p_filesz
        out.extend_from_slice(&s.memsz.to_le_bytes());
        out.extend_from_slice(&1u64.to_le_bytes()); // p_align
        data_offset += s.data.len();
    }

    for s in segments {
        out.extend_from_slice(s.data);
    }

    out.into_boxed_slice()
}
