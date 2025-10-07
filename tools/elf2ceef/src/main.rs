//! ELF to CEEF
//! Copyright (c) 2021 MEG-OS project

pub mod ceef;
pub mod elf;

use ceef::*;
use compress::stk1::{Configuration, S7s, Stk1};
use core::mem::transmute;
use elf::*;
use std::{
    env,
    fs::File,
    io::{Read, Write},
    iter,
    path::Path,
    process,
};

fn usage() {
    let mut args = env::args_os();
    let arg = args.next().unwrap();
    let path = Path::new(&arg);
    let lpc = path.file_name().unwrap();
    eprintln!("{} INFILE OUTFILE", lpc.to_str().unwrap());
    process::exit(1);
}

fn main() {
    let mut args = env::args();
    let _ = args.next().unwrap();

    let mut version = CeefVersion::CURRENT;
    let mut in_file = None;
    let mut out_file = None;
    while let Some(arg) = args.next() {
        if arg.starts_with("-") {
            match arg.as_str() {
                "-o" => {
                    out_file = args.next();
                }
                "-v0" => {
                    version = CeefVersion::V0;
                }
                "-v1" => {
                    version = CeefVersion::V1;
                }
                "--" => {
                    in_file = args.next();
                    break;
                }
                _ => return usage(),
            }
        } else {
            in_file = Some(arg);
            break;
        }
    }
    let in_file = match in_file {
        Some(v) => v,
        None => return usage(),
    };
    let out_file = out_file.unwrap_or(match args.next() {
        Some(v) => v,
        None => return usage(),
    });

    let mut is = File::open(in_file).expect("Failed to open input file");
    let mut src_blob = Vec::new();
    let _ = is
        .read_to_end(&mut src_blob)
        .expect("Failed to read input file");

    let mut data: Vec<u8> = Vec::with_capacity(src_blob.len());

    let header = Elf32Hdr::from_slice(&src_blob).expect("Bad executable");

    const BASE_ADDR_MASK: u32 = 0xFFFFF000;
    let mut base_addr = u32::MAX;
    let mut minalloc = 0;
    let n_segments = header.e_phnum as usize;

    match version {
        CeefVersion::V0 => {
            // CEEF version 0
            let mut ceef_sec_hdr: Vec<CeefSecHeader> = Vec::with_capacity(n_segments);

            println!("number of program headers: {}", n_segments);

            for i in 0..n_segments {
                let phdr: &Elf32Phdr = unsafe {
                    transmute(
                        &src_blob[header.e_phoff as usize + (header.e_phentsize as usize) * i],
                    )
                };

                let ceef_hdr = CeefSecHeader::new(
                    phdr.p_flags as u8,
                    phdr.p_vaddr,
                    phdr.p_filesz,
                    phdr.p_memsz,
                    if phdr.p_align != 0 {
                        phdr.p_align.trailing_zeros() as u8
                    } else {
                        0
                    },
                );

                println!(
                    "Phdr #{} {} {} {:08x} {:08x} {:x}({:?}) {} {}",
                    i,
                    ceef_hdr.attr(),
                    ceef_hdr.align(),
                    ceef_hdr.vaddr,
                    ceef_hdr.memsz,
                    phdr.p_type as usize,
                    phdr.p_type,
                    phdr.p_offset,
                    ceef_hdr.filesz,
                );

                if phdr.p_type == ElfSegmentType::LOAD {
                    let max_addr = phdr.p_vaddr + phdr.p_memsz;
                    base_addr = base_addr.min(phdr.p_vaddr & BASE_ADDR_MASK);
                    minalloc = minalloc.max(max_addr);

                    if phdr.p_filesz > 0 {
                        let f_offset = phdr.p_offset as usize;
                        let f_size = phdr.p_filesz as usize;
                        data.extend(src_blob[f_offset..f_offset + f_size].iter());

                        ceef_sec_hdr.push(ceef_hdr);
                    }
                }
            }

            let mut new_header = CeefHeader::default();
            new_header.n_secs = ceef_sec_hdr.len() as u8;
            new_header.base = base_addr;
            new_header.minalloc = minalloc - base_addr;
            new_header.entry = header.e_entry;

            println!(
                "CEEF: ver {} base {:08x} minalloc {:08x} entry {:08x}",
                new_header.version, new_header.base, new_header.minalloc, new_header.entry,
            );

            let mut os = File::create(out_file).expect("Failed to create output file");
            os.write_all(&new_header.as_bytes()).unwrap();
            for section in ceef_sec_hdr {
                os.write_all(&section.as_bytes()).unwrap();
            }
            os.write_all(data.as_slice()).unwrap();
        }

        CeefVersion::V1 => {
            // CEEF version 1
            println!("number of program headers: {}", n_segments);

            for i in 0..n_segments {
                let phdr: &Elf32Phdr = unsafe {
                    transmute(
                        &src_blob[header.e_phoff as usize + (header.e_phentsize as usize) * i],
                    )
                };

                println!(
                    "Phdr #{} {} {:08x} {:08x} {:x}({:?}) {:08x}",
                    i,
                    phdr.p_flags as u8,
                    phdr.p_vaddr,
                    phdr.p_filesz,
                    phdr.p_type as usize,
                    phdr.p_type,
                    phdr.p_offset,
                );

                if phdr.p_type == ElfSegmentType::LOAD {
                    let max_addr = phdr.p_vaddr + phdr.p_memsz;
                    base_addr = base_addr.min(phdr.p_vaddr & BASE_ADDR_MASK);
                    minalloc = minalloc.max(max_addr);

                    if phdr.p_filesz > 0 {
                        let f_offset = phdr.p_offset as usize;
                        let f_size = phdr.p_filesz as usize;
                        let rva = (phdr.p_vaddr - base_addr) as usize;
                        let delta = rva as isize - data.len() as isize;
                        assert!(delta >= 0);
                        if delta > 0 {
                            data.extend(iter::repeat(0).take(delta as usize));
                        }
                        data.extend(src_blob[f_offset..f_offset + f_size].iter());
                    }
                }
            }

            let mut new_header = CeefHeader::default();
            new_header.version = version;
            new_header.base = base_addr;
            new_header.minalloc = minalloc - base_addr;
            new_header.entry = header.e_entry;

            println!(
                "CEEF: ver {} base {:08x} minalloc {:08x} entry {:08x}",
                new_header.version, new_header.base, new_header.minalloc, new_header.entry,
            );

            let compressed = Stk1::encode_with_test(&data, Configuration::default()).unwrap();
            println!(
                "* compressed: {} <= {} ({:.2}%)",
                compressed.len(),
                data.len(),
                compressed.len() as f64 / data.len() as f64 * 100.0
            );

            let mut os = File::create(out_file).expect("Failed to create output file");
            os.write_all(&new_header.as_bytes()).unwrap();

            let mut mini_header = Vec::new();
            S7s::write(&mut mini_header, data.len());
            S7s::write(&mut mini_header, compressed.len());

            os.write_all(mini_header.as_slice()).unwrap();
            os.write_all(compressed.as_slice()).unwrap();
        }
    }
}
