//! A single outstanding request split virtqueue (without indirect descriptors).
use alloc::sync::Arc;
use core::sync::atomic::AtomicBool;

use super::barrier;
use super::dma::Dma;
use super::transport::Transport;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Buffer {
    pub address: u64,
    pub length: u32,
    pub writable: bool,
}

pub struct Queue {
    memory: Dma,
    size: u16,
    next_avail: u16,
    next_used: u16,
    pending: bool,
    used_offset: usize,
}

impl Queue {
    pub fn capacity(&self) -> u16 {
        self.size
    }
    pub fn new(
        mmio: &dyn Transport,
        index: u16,
        owner: Option<Arc<AtomicBool>>,
    ) -> Result<Self, &'static str> {
        mmio.select_queue(index);
        let max = mmio.queue_max();
        if max == 0 || mmio.queue_ready() {
            return Err("queue unavailable");
        }
        let size = (max.min(128) as u16)
            .checked_next_power_of_two()
            .ok_or("queue size")?;
        let size = if size as u32 > max { size / 2 } else { size };
        if size == 0 {
            return Err("queue size");
        }
        let used_offset = ((16 * size as usize + 6 + 2 * size as usize + 4095) / 4096) * 4096;
        let mut memory = Dma::new(used_offset + 6 + 8 * size as usize, 4096)?;
        if let Some(owner) = owner {
            memory = memory.with_owner(owner);
        }
        mmio.setup_queue(
            size,
            memory.addr(),
            memory.addr() + 16 * size as u64,
            memory.addr() + used_offset as u64,
        );
        Ok(Self {
            memory,
            size,
            next_avail: 0,
            next_used: 0,
            pending: false,
            used_offset,
        })
    }

    fn put16(&self, offset: usize, value: u16) {
        unsafe { (self.memory.as_ptr().add(offset) as *mut u16).write_volatile(value.to_le()) }
    }
    fn get16(&self, offset: usize) -> u16 {
        u16::from_le(unsafe { (self.memory.as_ptr().add(offset) as *const u16).read_volatile() })
    }
    fn get32(&self, offset: usize) -> u32 {
        u32::from_le(unsafe { (self.memory.as_ptr().add(offset) as *const u32).read_volatile() })
    }
    pub fn submit(
        &mut self,
        mmio: &dyn Transport,
        index: u16,
        buffers: &[Buffer],
    ) -> Result<(), &'static str> {
        self.memory.acquire();
        if self.get16(self.used_offset + 2) != self.next_used {
            return Err("unexpected used entry before submit");
        }
        if self.pending || buffers.is_empty() || buffers.len() > self.size as usize {
            return Err("queue busy or descriptor exhaustion");
        }
        for (i, buffer) in buffers.iter().enumerate() {
            if buffer.length == 0 || buffer.address.checked_add(buffer.length as u64).is_none() {
                return Err("invalid descriptor");
            }
            let p = unsafe { self.memory.as_ptr().add(i * 16) };
            unsafe {
                (p as *mut u64).write_volatile(buffer.address.to_le());
                (p.add(8) as *mut u32).write_volatile(buffer.length.to_le());
                (p.add(12) as *mut u16).write_volatile(
                    ((if i + 1 < buffers.len() { 1 } else { 0 })
                        | if buffer.writable { 2 } else { 0 }) as u16,
                );
                (p.add(14) as *mut u16).write_volatile((i as u16 + 1).to_le());
            }
        }
        let avail = 16 * self.size as usize;
        self.put16(
            avail + 4 + 2 * (self.next_avail as usize % self.size as usize),
            0,
        );
        self.memory.release();
        barrier::device();
        self.next_avail = self.next_avail.wrapping_add(1);
        self.put16(avail + 2, self.next_avail);
        self.memory.release();
        self.pending = true;
        mmio.notify(index);
        Ok(())
    }
    pub fn poll(&mut self) -> Result<Option<u32>, &'static str> {
        self.memory.acquire();
        if self.get16(self.used_offset + 2) == self.next_used {
            return Ok(None);
        }
        if !self.pending {
            return Err("unexpected used entry");
        }
        let slot = self.next_used as usize % self.size as usize;
        let id = self.get32(self.used_offset + 4 + 8 * slot);
        let len = self.get32(self.used_offset + 8 + 8 * slot);
        if id != 0 {
            return Err("invalid used descriptor ID");
        }
        self.next_used = self.next_used.wrapping_add(1);
        self.pending = false;
        Ok(Some(len))
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;

    use super::*;
    use crate::io::virtio::mmio::Mmio;

    fn rig() -> (Box<[u32; 128]>, Mmio, Queue) {
        let mut regs = Box::new([0u32; 128]);
        regs[0] = 0x7472_6976;
        regs[1] = 2;
        regs[0x034 / 4] = 8;
        let mmio = unsafe { Mmio::new(regs.as_mut_ptr() as usize, 0x200) }.unwrap();
        let queue = Queue::new(&mmio, 0, None).unwrap();
        (regs, mmio, queue)
    }

    #[test]
    fn descriptor_chain_and_wrap() {
        let (_regs, mmio, mut queue) = rig();
        queue.next_avail = u16::MAX;
        queue.next_used = u16::MAX;
        queue.put16(queue.used_offset + 2, u16::MAX);
        let a = Dma::new(16, 16).unwrap();
        let b = Dma::new(16, 16).unwrap();
        queue
            .submit(
                &mmio,
                0,
                &[
                    Buffer {
                        address: a.addr(),
                        length: 16,
                        writable: false,
                    },
                    Buffer {
                        address: b.addr(),
                        length: 16,
                        writable: true,
                    },
                ],
            )
            .unwrap();
        assert_eq!(queue.next_avail, 0);
        assert_eq!(queue.get16(16 * queue.size as usize + 2), 0);
        assert_eq!(queue.get16(12), 1);
        assert_eq!(queue.get16(16 + 12), 2);
        assert!(
            queue
                .submit(
                    &mmio,
                    0,
                    &[Buffer {
                        address: a.addr(),
                        length: 16,
                        writable: false
                    }]
                )
                .is_err()
        );
        let slot = u16::MAX as usize % queue.size as usize;
        unsafe {
            (queue.memory.as_ptr().add(queue.used_offset + 4 + 8 * slot) as *mut u32)
                .write_volatile(0);
            (queue.memory.as_ptr().add(queue.used_offset + 8 + 8 * slot) as *mut u32)
                .write_volatile(16);
        }
        queue.put16(queue.used_offset + 2, 0);
        assert_eq!(queue.poll().unwrap(), Some(16));
        assert_eq!(queue.next_used, 0);
        assert!(queue.poll().unwrap().is_none());
    }

    #[test]
    fn rejects_invalid_used_id() {
        let (_regs, mmio, mut queue) = rig();
        let a = Dma::new(16, 16).unwrap();
        queue
            .submit(
                &mmio,
                0,
                &[Buffer {
                    address: a.addr(),
                    length: 16,
                    writable: true,
                }],
            )
            .unwrap();
        unsafe { (queue.memory.as_ptr().add(queue.used_offset + 4) as *mut u32).write_volatile(9) };
        queue.put16(queue.used_offset + 2, 1);
        assert_eq!(queue.poll(), Err("invalid used descriptor ID"));
    }

    #[test]
    fn layout_and_device_address_are_independent_of_cpu_pointer() {
        let (_regs, mmio, mut queue) = rig();
        assert_eq!(queue.memory.addr() % 4096, 0);
        assert_eq!(queue.used_offset % 4096, 0);
        let cpu = Dma::new(16, 16).unwrap();
        let device_address = 0x1234_5000;
        assert_ne!(cpu.as_ptr() as u64, device_address);
        queue
            .submit(
                &mmio,
                0,
                &[Buffer {
                    address: device_address,
                    length: 16,
                    writable: true,
                }],
            )
            .unwrap();
        let recorded =
            u64::from_le(unsafe { (queue.memory.as_ptr() as *const u64).read_unaligned() });
        assert_eq!(recorded, device_address);
    }

    #[test]
    fn rejects_exhaustion_and_duplicate_completion() {
        let (_regs, mmio, mut queue) = rig();
        let buffer = Buffer {
            address: 0x1000,
            length: 16,
            writable: true,
        };
        assert_eq!(
            queue.submit(&mmio, 0, &[buffer; 9]),
            Err("queue busy or descriptor exhaustion")
        );
        queue.submit(&mmio, 0, &[buffer]).unwrap();
        queue.put16(queue.used_offset + 2, 1);
        assert_eq!(queue.poll(), Ok(Some(0)));
        queue.put16(queue.used_offset + 2, 2);
        assert_eq!(
            queue.submit(&mmio, 0, &[buffer]),
            Err("unexpected used entry before submit")
        );
    }
}
