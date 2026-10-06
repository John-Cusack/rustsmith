//! Pure-Rust core: ELF identification + header parsing for `_elffile`.
//!
//! Stage-1 mirror of `packaging/_elffile.py` (`ELFFile`). Byte layout only;
//! the PyO3 binding drives the reads off the caller's file object (mirroring
//! the original's incremental `read`/`seek` pattern) and maps short reads to
//! the original `struct.error` branches.
//!
//! License: Apache-2.0 OR BSD-2-Clause (preserved from the original).

/// `(capacity, encoding)` selection: ELF format strings and program-header
/// field indexes, mirroring the `{(1,1): ..., (1,2): ..., (2,1): ..., (2,2): ...}`
/// table in the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElfKind {
    /// `true` for 64-bit headers (8-byte `p_offset`/`p_filesz`).
    pub is64: bool,
    /// `true` for little-endian.
    pub little: bool,
}

impl ElfKind {
    pub fn for_ident(capacity: u8, encoding: u8) -> Option<ElfKind> {
        match (capacity, encoding) {
            (1, 1) => Some(ElfKind { is64: false, little: true }),
            (1, 2) => Some(ElfKind { is64: false, little: false }),
            (2, 1) => Some(ElfKind { is64: true, little: true }),
            (2, 2) => Some(ElfKind { is64: true, little: false }),
            _ => None,
        }
    }

    /// Size of the ELF header past the 16-byte ident (`struct.calcsize(e_fmt)`:
    /// 30 bytes for `<HHIIIIIHHH`, 42 for `<HHIQQQIHHH`).
    pub fn ehdr_len(&self) -> usize {
        if self.is64 { 42 } else { 30 }
    }
    /// Size of one program header (`struct.calcsize(p_fmt)`).
    pub fn phdr_len(&self) -> usize {
        if self.is64 { 56 } else { 32 }
    }
}

fn u16(b: &[u8], little: bool) -> u16 {
    if little {
        u16::from_le_bytes([b[0], b[1]])
    } else {
        u16::from_be_bytes([b[0], b[1]])
    }
}

fn u32(b: &[u8], little: bool) -> u32 {
    if little {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    } else {
        u32::from_be_bytes([b[0], b[1], b[2], b[3]])
    }
}

fn u64(b: &[u8], little: bool) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[..8]);
    if little { u64::from_le_bytes(a) } else { u64::from_be_bytes(a) }
}

/// Parsed ELF header fields used by `ELFFile`
/// (`machine`, `flags`, `e_phoff`, `e_phentsize`, `e_phnum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ehdr {
    pub machine: u16,
    pub flags: u32,
    pub phoff: u64,
    pub phentsize: u64,
    pub phnum: u64,
}

/// Unpack the ELF header (32 bytes past ident for 32-bit, 48 for 64-bit —
/// both `struct.calcsize(e_fmt) == 36`/`48`... see `ehdr_len`).
/// The caller guarantees `data.len() >= kind.ehdr_len()`.
pub fn parse_ehdr(kind: ElfKind, data: &[u8]) -> Ehdr {
    // Offsets are relative to the end of the 16-byte ident:
    // 32-bit `<HHIIIIIHHH`: type[0..2] machine[2..4] version[4..8]
    //   entry[8..12] phoff[12..16] shoff[16..20] flags[20..24] ehsize[24..26]
    //   phentsize[26..28] phnum[28..30].
    // 64-bit `<HHIQQQIHHH`: type[0..2] machine[2..4] version[4..8]
    //   entry[8..16] phoff[16..24] shoff[24..32] flags[32..36] ehsize[36..38]
    //   phentsize[38..40] phnum[40..42].
    let le = kind.little;
    if kind.is64 {
        Ehdr {
            machine: u16(&data[2..4], le),
            flags: u32(&data[32..36], le),
            phoff: u64(&data[16..24], le),
            phentsize: u64::from(u16(&data[38..40], le)),
            phnum: u64::from(u16(&data[40..42], le)),
        }
    } else {
        Ehdr {
            machine: u16(&data[2..4], le),
            flags: u32(&data[20..24], le),
            phoff: u64::from(u32(&data[12..16], le)),
            phentsize: u64::from(u16(&data[26..28], le)),
            phnum: u64::from(u16(&data[28..30], le)),
        }
    }
}

/// A parsed program header: `(p_type, p_offset, p_filesz)`.
/// The caller guarantees `data.len() >= kind.phdr_len()`.
pub fn parse_phdr(kind: ElfKind, data: &[u8]) -> (u32, u64, u64) {
    // 32-bit `<IIIIIIII`: type[0..4] offset[4..8] filesz[16..20].
    // 64-bit `<IIQQQQQQ`: type[0..4] flags[4..8] offset[8..16] filesz[32..40].
    let le = kind.little;
    if kind.is64 {
        (
            u32(&data[0..4], le),
            u64(&data[8..16], le),
            u64(&data[32..40], le),
        )
    } else {
        (
            u32(&data[0..4], le),
            u64::from(u32(&data[4..8], le)),
            u64::from(u32(&data[16..20], le)),
        )
    }
}

/// `PT_INTERP` program-header type.
pub const PT_INTERP: u32 = 3;
