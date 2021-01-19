// Executable and Linking Format

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf32Hdr {
    pub n_ident: [u8; 16],
    pub e_type: ElfType,
    pub e_machine: Machine,
    pub e_version: u32,
    pub e_entry: u32,
    pub e_phoff: u32,
    pub e_shoff: u32,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

impl Elf32Hdr {
    pub const MAGIC: [u8; 4] = *b"\x7FELF";

    pub fn is_valid(&self) -> bool {
        (self.n_ident[..4] == Self::MAGIC)
            && (self.n_ident[4] == 1)
            && (self.n_ident[5] == 1)
            && (self.n_ident[6] == 1)
    }
}

#[repr(u16)]
#[non_exhaustive]
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ElfType {
    NONE = 0,
    REL = 1,
    EXEC = 2,
    DYN = 3,
    CORE = 4,
}

#[repr(u16)]
#[non_exhaustive]
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Machine {
    None = 0,
    M32 = 1,
    SPARC = 2,
    _386 = 3,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf32Phdr {
    pub p_type: ElfSegmentType,
    pub p_offset: u32,
    pub p_vaddr: u32,
    pub p_paddr: u32,
    pub p_filesz: u32,
    pub p_memsz: u32,
    pub p_flags: u32,
    pub p_align: u32,
}

#[repr(u32)]
#[non_exhaustive]
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ElfSegmentType {
    NULL = 0,
    LOAD = 1,
    DYBAMIC = 2,
    INTERP = 3,
    NOTE = 4,
    SHLIB = 5,
    PHDR = 6,
    TLS = 7,
    PT_LOOS = 0x6000_0000,
    PT_HIOS = 0x6FFF_FFFF,
    PT_LOPROC = 0x7000_0000,
    PT_HIPROC = 0x7FFF_FFFF,
}
