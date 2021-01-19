// Kernel file converter

// use byteorder::*;
use convert::{ceef::*, elf::*};
use core::mem::transmute;
use std::env;
use std::fs::File;
use std::io::Read;
use std::io::Write;

fn main() {
    let mut args = env::args();
    let _ = args.next().unwrap();

    let in_file = args.next().unwrap();
    let out_file = args.next().unwrap();

    let mut is = File::open(in_file).unwrap();
    let mut blob = Vec::new();
    let _ = is.read_to_end(&mut blob).unwrap();

    let mut data: Vec<u8> = Vec::with_capacity(blob.len());

    let header: &Elf32Hdr = unsafe { transmute(&blob[0]) };

    assert!(
        header.is_valid() && header.e_type == ElfType::EXEC && header.e_machine == Machine::_386,
        "Bad executable"
    );

    let n_segments = header.e_phnum as usize;
    let mut ceef_sec_hdr: Vec<CeefSecHeader> = Vec::with_capacity(n_segments);

    println!("program header {}", n_segments);
    for i in 0..n_segments {
        let phdr: &Elf32Phdr = unsafe {
            transmute(&blob[header.e_phoff as usize + (header.e_phentsize as usize) * i])
        };

        println!(
            "Phdr #{} {:?} {} {} {} {:08x} {:08x}",
            i, phdr.p_type, phdr.p_flags, phdr.p_offset, phdr.p_filesz, phdr.p_vaddr, phdr.p_memsz
        );

        if phdr.p_type == ElfSegmentType::LOAD {
            if phdr.p_filesz > 0 {
                let f_offset = phdr.p_offset as usize;
                let f_size = phdr.p_filesz as usize;
                let old_size = data.len();
                data.extend(blob[f_offset..f_offset + f_size].iter());
                println!(
                    "### COPY {} => {}, fpos {} size {}",
                    old_size,
                    data.len(),
                    f_offset,
                    f_size
                );

                ceef_sec_hdr.push(CeefSecHeader::new(
                    phdr.p_flags as u8,
                    phdr.p_vaddr,
                    phdr.p_filesz,
                ));
            }
        }
    }

    let mut new_header = CeefHeader::default();
    new_header.n_sec = ceef_sec_hdr.len() as u8;
    new_header.entry = header.e_entry;

    let mut os = File::create(out_file).unwrap();
    os.write_all(&new_header.as_bytes()).unwrap();
    for section in ceef_sec_hdr {
        os.write_all(&section.as_bytes()).unwrap();
    }
    os.write_all(data.as_slice()).unwrap();
}
