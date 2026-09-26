//! VirtIO entropy. Quality depends on the host backend.
use super::Device;
use super::dma::Dma;
use super::queue::Buffer;
use crate::System;
use crate::sync::spin::SpinMutex;

static DEVICE: SpinMutex<Option<Device>> = SpinMutex::new(None);

pub(super) fn attach(base: usize, size: usize) -> Result<(), &'static str> {
    let mut slot = DEVICE.lock();
    if slot.is_some() {
        return Ok(());
    }
    let mut device = unsafe { Device::new(base, size, 0) }?;
    let mut sample = Dma::new(16, 16)?.with_owner(device.owner.clone());
    let got = request(&mut device, &mut sample)?;
    if got == 0 {
        return Err("empty entropy response");
    }
    *slot = Some(device);
    crate::println!("virtio-rng: ready");
    Ok(())
}

fn request(device: &mut Device, dma: &mut Dma) -> Result<usize, &'static str> {
    dma.release();
    let used = device.run(
        &[Buffer {
            address: dma.addr(),
            length: dma.len() as u32,
            writable: true,
        }],
        1_000_000,
    )? as usize;
    dma.acquire();
    if used == 0 || used > dma.len() {
        return Err("invalid entropy length");
    }
    Ok(used)
}

pub fn fill(buffer: &mut [u8]) -> Result<(), &'static str> {
    let mut device = DEVICE.lock().take().ok_or("no rng or busy")?;
    let result = (|| {
        let mut dma = Dma::new(buffer.len().min(4096).max(1), 16)?.with_owner(device.owner.clone());
        let mut offset = 0;
        while offset < buffer.len() {
            let count = (buffer.len() - offset).min(dma.len());
            let got = request(&mut device, &mut dma)?;
            let got = got.min(count);
            buffer[offset..offset + got].copy_from_slice(&dma.bytes()[..got]);
            offset += got;
        }
        Ok(())
    })();
    *DEVICE.lock() = Some(device);
    result
}
