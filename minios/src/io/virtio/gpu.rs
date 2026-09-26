//! Single scanout VirtIO GPU 2D output.
use alloc::boxed::Box;

use super::Device;
use super::dma::Dma;
use super::queue::Buffer;
use crate::io::graphics::{
    CurrentMode, GraphicsOutputDevice, ModeIndex, ModeInfo, PixelFormat, PreferredGraphicsMode,
};
use crate::mem::MemoryManager;
use crate::{PhysicalAddress, System};

const GET_DISPLAY_INFO: u32 = 0x0100;
const CREATE_2D: u32 = 0x0101;
const UNREF: u32 = 0x0102;
const SET_SCANOUT: u32 = 0x0103;
const RESOURCE_FLUSH: u32 = 0x0104;
const TRANSFER: u32 = 0x0105;
const ATTACH: u32 = 0x0106;
const DETACH: u32 = 0x0107;
const OK_NODATA: u32 = 0x1100;
const OK_DISPLAY_INFO: u32 = 0x1101;
/// Largest scanout accepted from the host: a 1920x1200 BGRX framebuffer is 8.8 MiB.
const MAX_WIDTH: u32 = 1920;
const MAX_HEIGHT: u32 = 1200;
const RESOURCE: u32 = 1;

static mut ACTIVE: *mut VirtioGpu = core::ptr::null_mut();

pub struct VirtioGpu {
    device: Device,
    framebuffer: Dma,
    mode: [ModeInfo; 1],
    current: CurrentMode,
    active: bool,
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn get32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

impl VirtioGpu {
    fn command(
        &mut self,
        request: &[u8],
        response: &mut [u8],
        expected: u32,
    ) -> Result<(), &'static str> {
        if request.len() < 24 || response.len() < 24 {
            return Err("short GPU command");
        }
        let mut tx = Dma::new(request.len(), 16)?.with_owner(self.device.owner.clone());
        let rx = Dma::new(response.len(), 16)?.with_owner(self.device.owner.clone());
        tx.bytes_mut().copy_from_slice(request);
        tx.release();
        rx.release();
        let used = self.device.run(
            &[
                Buffer {
                    address: tx.addr(),
                    length: request.len() as u32,
                    writable: false,
                },
                Buffer {
                    address: rx.addr(),
                    length: response.len() as u32,
                    writable: true,
                },
            ],
            1_000_000,
        )? as usize;
        rx.acquire();
        if used != response.len()
            || get32(rx.bytes(), 0) != expected
            || get32(rx.bytes(), 4) != 0
            || get32(rx.bytes(), 8) != 0
            || get32(rx.bytes(), 12) != 0
            || get32(rx.bytes(), 16) != 0
        {
            return Err("bad GPU response");
        }
        response[..used].copy_from_slice(&rx.bytes()[..used]);
        Ok(())
    }
    fn no_data(&mut self, request: &[u8]) -> Result<(), &'static str> {
        self.command(request, &mut [0; 24], OK_NODATA)
    }
    fn rect(&self, type_: u32) -> [u8; 48] {
        let mut req = [0; 48];
        put32(&mut req, 0, type_);
        put32(&mut req, 32, self.mode[0].width as u32);
        put32(&mut req, 36, self.mode[0].height as u32);
        put32(&mut req, 40, RESOURCE);
        req
    }
    fn present(&mut self, address: usize, length: usize) -> Result<(), &'static str> {
        if !self.active || length == 0 {
            return Ok(());
        }
        let start = address.saturating_sub(self.framebuffer.as_ptr() as usize);
        let end = start.saturating_add(length).min(self.framebuffer.len());
        let stride = self.mode[0].bytes_per_scanline as usize;
        if start >= end || stride == 0 {
            return Ok(());
        }
        let y = (start / stride) as u32;
        let bottom = ((end + stride - 1) / stride).min(self.mode[0].height as usize) as u32;
        if y >= bottom {
            return Ok(());
        }
        self.framebuffer
            .release_range(y as usize * stride, (bottom - y) as usize * stride);
        let mut req = [0; 56];
        put32(&mut req, 0, TRANSFER);
        put32(&mut req, 28, y);
        put32(&mut req, 32, self.mode[0].width as u32);
        put32(&mut req, 36, bottom - y);
        put64(&mut req, 40, (y as usize * stride) as u64);
        put32(&mut req, 48, RESOURCE);
        self.no_data(&req).map_err(|_| "TRANSFER failed")?;
        let mut req = self.rect(RESOURCE_FLUSH);
        put32(&mut req, 28, y);
        put32(&mut req, 36, bottom - y);
        self.no_data(&req)
    }
    fn start(&mut self) -> Result<(), &'static str> {
        if self.active {
            return Ok(());
        }
        let mut req = [0; 40];
        put32(&mut req, 0, CREATE_2D);
        put32(&mut req, 24, RESOURCE);
        put32(&mut req, 28, 2); // B8G8R8X8_UNORM
        put32(&mut req, 32, self.mode[0].width as u32);
        put32(&mut req, 36, self.mode[0].height as u32);
        if self.no_data(&req).is_err() {
            // A malformed response does not prove the device rejected creation.
            self.cleanup();
            return Err("CREATE_2D failed");
        }
        let mut req = [0; 48];
        put32(&mut req, 0, ATTACH);
        put32(&mut req, 24, RESOURCE);
        put32(&mut req, 28, 1);
        put64(&mut req, 32, self.framebuffer.addr());
        put32(&mut req, 40, self.framebuffer.len() as u32);
        if self.no_data(&req).is_err() {
            self.cleanup();
            return Err("ATTACH_BACKING failed");
        }
        let mut req = self.rect(SET_SCANOUT);
        put32(&mut req, 40, 0); // scanout 0
        put32(&mut req, 44, RESOURCE);
        if self.no_data(&req).is_err() {
            self.cleanup();
            return Err("SET_SCANOUT failed");
        }
        self.active = true;
        if self
            .present(self.framebuffer.as_ptr() as usize, self.framebuffer.len())
            .is_err()
        {
            self.cleanup();
            return Err("initial present failed");
        }
        Ok(())
    }
    fn cleanup(&mut self) {
        let mut failed = false;
        let mut req = [0; 48];
        put32(&mut req, 0, SET_SCANOUT);
        put32(&mut req, 32, self.mode[0].width as u32);
        put32(&mut req, 36, self.mode[0].height as u32);
        failed |= self.no_data(&req).is_err();
        let mut req = [0; 32];
        put32(&mut req, 0, DETACH);
        put32(&mut req, 24, RESOURCE);
        failed |= self.no_data(&req).is_err();
        put32(&mut req, 0, UNREF);
        failed |= self.no_data(&req).is_err();
        if failed {
            let _ = self.device.restart(0);
        }
        self.active = false;
    }
}

pub(super) fn attach(base: usize, size: usize) -> Result<(), &'static str> {
    if unsafe { !(&raw const ACTIVE).read().is_null() } {
        return Ok(());
    }
    let mut device = unsafe { Device::new(base, size, 0) }?;
    if device.queue.capacity() < 2 {
        return Err("GPU queue has fewer than two descriptors");
    }
    let mut tx = Dma::new(24, 16)?.with_owner(device.owner.clone());
    let rx = Dma::new(24 + 16 * 24, 16)?.with_owner(device.owner.clone());
    put32(tx.bytes_mut(), 0, GET_DISPLAY_INFO);
    tx.release();
    rx.release();
    let used = device.run(
        &[
            Buffer {
                address: tx.addr(),
                length: 24,
                writable: false,
            },
            Buffer {
                address: rx.addr(),
                length: rx.len() as u32,
                writable: true,
            },
        ],
        1_000_000,
    )? as usize;
    rx.acquire();
    let bytes = rx.bytes();
    if used != rx.len()
        || get32(bytes, 0) != OK_DISPLAY_INFO
        || get32(bytes, 4) != 0
        || get32(bytes, 8) != 0
        || get32(bytes, 12) != 0
        || get32(bytes, 16) != 0
        || get32(bytes, 40) == 0
    {
        return Err("no enabled scanout 0");
    }
    // Follow the host's preferred mode (QEMU: 1280x800, or `xres`/`yres`) when
    // its framebuffer leaves at least half of the free heap to the other
    // devices; otherwise fall back to 800x600, as on arm64 virt whose early
    // heap is 4 MiB.
    let preferred = (
        get32(bytes, 32).min(MAX_WIDTH) as u16,
        get32(bytes, 36).min(MAX_HEIGHT) as u16,
    );
    if preferred.0 == 0 || preferred.1 == 0 {
        return Err("zero display dimensions");
    }
    let fb_size = |(width, height): (u16, u16)| width as usize * height as usize * 4;
    let fallback = (preferred.0.min(800), preferred.1.min(600));
    let (width, height) = if fb_size(preferred) <= MemoryManager::free_memory_count() / 2 {
        preferred
    } else {
        fallback
    };
    let framebuffer = Dma::new(fb_size((width, height)), 4096)?.with_owner(device.owner.clone());
    let info = ModeInfo {
        width,
        height,
        bytes_per_scanline: width * 4,
        pixel_format: PixelFormat::BGRX8888,
    };
    let mut gpu = Box::new(VirtioGpu {
        device,
        current: CurrentMode {
            current: ModeIndex(0),
            info,
            fb: PhysicalAddress::from_usize(framebuffer.as_ptr() as usize),
            fb_size: framebuffer.len(),
        },
        framebuffer,
        mode: [info],
        active: false,
    });
    unsafe { (&raw mut ACTIVE).write(&mut *gpu) };
    System::conctl().set_graphics(gpu);
    crate::println!("virtio-gpu: {}x{} scanout", width, height);
    Ok(())
}

pub fn available() -> bool {
    unsafe { !(&raw const ACTIVE).read().is_null() }
}

fn framebuffer_sync(address: usize, length: usize) {
    let gpu = unsafe { (&raw mut ACTIVE).read().as_mut() };
    if let Some(gpu) = gpu {
        let _ = gpu.present(address, length);
    }
}

impl GraphicsOutputDevice for VirtioGpu {
    fn modes(&self) -> &[ModeInfo] {
        &self.mode
    }
    fn current_mode(&self) -> &CurrentMode {
        &self.current
    }
    fn preferred_graphics_mode(&self) -> Option<PreferredGraphicsMode> {
        Some(self.mode[0].into())
    }
    fn set_mode(&mut self, mode: ModeIndex) -> Result<(), ()> {
        if mode != ModeIndex(0) {
            return Err(());
        }
        self.start().map_err(|reason| {
            crate::println!("virtio-gpu: start failed: {reason}");
        })
    }
    fn detach(&mut self) {
        if self.active {
            self.cleanup();
        }
    }
    fn framebuffer_sync(&self) -> Option<fn(usize, usize)> {
        Some(framebuffer_sync)
    }
}

#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use alloc::vec::Vec;
    use std::sync::Mutex;

    use super::*;
    use crate::io::virtio::transport::Transport;

    #[derive(Default)]
    struct State {
        status: u32,
        desc: u64,
        used: u64,
        queue_size: u16,
        used_index: u16,
        fail_create: bool,
        fail_scanout: bool,
        requests: Vec<Vec<u8>>,
    }
    struct Fake(Arc<Mutex<State>>);
    impl Transport for Fake {
        fn status(&self) -> u32 {
            self.0.lock().unwrap().status
        }
        fn set_status(&self, value: u32) {
            let mut state = self.0.lock().unwrap();
            state.status = value;
            if value == 0 {
                state.used_index = 0;
            }
        }
        fn features(&self) -> u64 {
            1 << 32
        }
        fn set_features(&self, _: u64) {}
        fn select_queue(&self, _: u16) {}
        fn queue_max(&self) -> u32 {
            8
        }
        fn queue_ready(&self) -> bool {
            false
        }
        fn setup_queue(&self, size: u16, desc: u64, _: u64, used: u64) {
            let mut state = self.0.lock().unwrap();
            state.queue_size = size;
            state.desc = desc;
            state.used = used;
            state.used_index = 0;
        }
        fn notify(&self, _: u16) {
            let mut state = self.0.lock().unwrap();
            let desc = state.desc as *const u8;
            let request_addr = u64::from_le(unsafe { (desc as *const u64).read_unaligned() });
            let request_len =
                u32::from_le(unsafe { (desc.add(8) as *const u32).read_unaligned() }) as usize;
            let response_addr =
                u64::from_le(unsafe { (desc.add(16) as *const u64).read_unaligned() });
            let response_len =
                u32::from_le(unsafe { (desc.add(24) as *const u32).read_unaligned() });
            let request =
                unsafe { core::slice::from_raw_parts(request_addr as *const u8, request_len) }
                    .to_vec();
            let type_ = get32(&request, 0);
            let fail = (type_ == CREATE_2D && state.fail_create)
                || (type_ == SET_SCANOUT && state.fail_scanout);
            if type_ == CREATE_2D && fail {
                state.fail_create = false;
            }
            if fail {
                state.fail_scanout = false;
            }
            state.requests.push(request);
            unsafe {
                (response_addr as *mut u32)
                    .write_unaligned((if fail { 0x1200u32 } else { OK_NODATA }).to_le());
                let slot = state.used_index as usize % state.queue_size as usize;
                ((state.used as usize + 4 + slot * 8) as *mut u32).write_unaligned(0);
                ((state.used as usize + 8 + slot * 8) as *mut u32)
                    .write_unaligned(response_len.to_le());
                state.used_index = state.used_index.wrapping_add(1);
                ((state.used as usize + 2) as *mut u16).write_unaligned(state.used_index.to_le());
            }
        }
        fn ack_interrupt(&self) -> u32 {
            0
        }
        fn config_u32(&self, _: usize) -> u32 {
            0
        }
        fn config_generation(&self) -> u32 {
            0
        }
    }

    fn gpu() -> (VirtioGpu, Arc<Mutex<State>>) {
        let state = Arc::new(Mutex::new(State::default()));
        let device = Device::from_transport(Box::new(Fake(state.clone())), 0)
            .ok()
            .unwrap();
        let framebuffer = Dma::new(64 * 64 * 4, 4096)
            .unwrap()
            .with_owner(device.owner.clone());
        let info = ModeInfo {
            width: 64,
            height: 64,
            bytes_per_scanline: 256,
            pixel_format: PixelFormat::BGRX8888,
        };
        let gpu = VirtioGpu {
            current: CurrentMode {
                current: ModeIndex(0),
                info,
                fb: PhysicalAddress::from_usize(framebuffer.as_ptr() as usize),
                fb_size: framebuffer.len(),
            },
            device,
            framebuffer,
            mode: [info],
            active: false,
        };
        (gpu, state)
    }

    #[test]
    fn detach_and_restart_release_resources_in_order() {
        let (mut gpu, state) = gpu();
        gpu.start().unwrap();
        assert!(gpu.active);
        let base = gpu.framebuffer.as_ptr() as usize;
        gpu.present(base + 2 * 256, 2 * 256).unwrap();
        {
            let requests = &state.lock().unwrap().requests;
            let transfer = &requests[requests.len() - 2];
            assert_eq!(get32(transfer, 0), TRANSFER);
            assert_eq!(get32(transfer, 28), 2);
            assert_eq!(get32(transfer, 36), 2);
        }
        gpu.detach();
        assert!(!gpu.active);
        {
            let requests = &state.lock().unwrap().requests;
            assert_eq!(
                requests[requests.len() - 3..]
                    .iter()
                    .map(|r| get32(r, 0))
                    .collect::<Vec<_>>(),
                [SET_SCANOUT, DETACH, UNREF]
            );
        }
        gpu.start().unwrap();
        assert!(gpu.active);
        gpu.detach();
    }

    #[test]
    fn failed_scanout_cleans_up_and_can_restart() {
        let (mut gpu, state) = gpu();
        state.lock().unwrap().fail_scanout = true;
        assert_eq!(gpu.start(), Err("SET_SCANOUT failed"));
        assert!(!gpu.active);
        {
            let requests = &state.lock().unwrap().requests;
            assert_eq!(
                requests[requests.len() - 3..]
                    .iter()
                    .map(|r| get32(r, 0))
                    .collect::<Vec<_>>(),
                [SET_SCANOUT, DETACH, UNREF]
            );
        }
        gpu.start().unwrap();
        assert!(gpu.active);
        gpu.detach();
    }

    #[test]
    fn failed_create_cleans_up_and_can_restart() {
        let (mut gpu, state) = gpu();
        state.lock().unwrap().fail_create = true;
        assert_eq!(gpu.start(), Err("CREATE_2D failed"));
        assert!(!gpu.active);
        {
            let requests = &state.lock().unwrap().requests;
            assert_eq!(
                requests[requests.len() - 3..]
                    .iter()
                    .map(|r| get32(r, 0))
                    .collect::<Vec<_>>(),
                [SET_SCANOUT, DETACH, UNREF]
            );
        }
        gpu.start().unwrap();
        assert!(gpu.active);
        gpu.detach();
    }

    #[test]
    fn failed_detach_resets_transport_before_restart() {
        let (mut gpu, state) = gpu();
        gpu.start().unwrap();
        state.lock().unwrap().fail_scanout = true;
        gpu.detach();
        assert!(!gpu.active);
        assert_eq!(state.lock().unwrap().status, 15);
        gpu.start().unwrap();
        assert!(gpu.active);
        gpu.detach();
    }
}
