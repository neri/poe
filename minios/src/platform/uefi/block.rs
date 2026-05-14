//! Block device driver for UEFI

use super::*;
use alloc::format;
use alloc::vec::Vec;
use uefi::Identify;
use uefi::proto::device_path::DevicePath;
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::block::BlockIO;

pub unsafe fn init() {
    unsafe {
        println!("List of Volumes:");

        {
            let image = get_protocol::<LoadedImage>(uefi::boot::image_handle()).unwrap();
            let device = image.device().unwrap();
            let path = get_protocol::<DevicePath>(device).unwrap();
            println!("BootDrive: {}", parse_device_path(&path).join("/"));
        }

        let buffer =
            uefi::boot::locate_handle_buffer(uefi::boot::SearchType::ByProtocol(&BlockIO::GUID))
                .unwrap();
        for handle in buffer.iter() {
            let path = get_protocol::<DevicePath>(*handle).unwrap();
            println!("BlockIo: {}", parse_device_path(&path).join("/"));

            // let io = get_protocol::<BlockIO>(*handle).unwrap();
            // let media = io.media();
            // println!("  MediaInfo: {:?}", media);

            // if let Ok(fs) = get_protocol::<SimpleFileSystem>(*handle) {
            //     println!("  SimpleFileSystem: {:?}", fs);
            // }

            // if media.is_media_present() {
            //     let media_id = media.media_id();
            //     let block_size = media.block_size() as usize;
            //     let mut buffer = Vec::with_capacity(block_size);
            //     buffer.resize(block_size, 0);
            //     let lba = 0;
            //     io.read_blocks(media_id, lba, &mut buffer).unwrap();

            //     for (i, line) in buffer.chunks(16).enumerate() {
            //         print!("{:04x}: ", i * 16);
            //         for byte in line {
            //             print!("{:02x} ", byte);
            //         }
            //         for byte in line {
            //             let ch = match byte {
            //                 0x20..=0x7e => *byte as char,
            //                 _ => '.',
            //             };
            //             print!("{}", ch);
            //         }
            //         println!("");
            //     }
            // }
        }
    }

    // todo!()
}

fn parse_device_path(path: &DevicePath) -> Vec<String> {
    let mut result = Vec::new();
    let mut bytes = path.as_bytes();
    loop {
        if bytes.len() < 4 {
            result.push(format!(
                "Bad({})",
                bytes
                    .iter()
                    .map(|v| format!("{:02x}", v))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
            break;
        }
        let type_ = bytes[0];
        let sub_type = bytes[1];
        if (type_, sub_type) == (0x7f, 0xff) {
            // end of device path
            break;
        }
        let length = u16::from_le_bytes([bytes[2], bytes[3]]);
        let node = format!("({:02x},{:02x},{:04x})", type_, sub_type, length);
        result.push(node);
        bytes = &bytes[length as usize..];
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionSignature {
    Unknown,
    Mbr([u8; 4]),
    Gpt([u8; 16]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionType {
    Unknown,
    Mbr,
    Gpt,
}
