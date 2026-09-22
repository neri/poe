//! Where USB block devices are published, and the read-only [`BlockDevice`]
//! handle over them.
//!
//! The sessions that talk to a device live inside a USB service (the Pi 3
//! `UsbManager` or the xHCI `XhciUsb`), which is boxed into the system's
//! service list and cannot be borrowed from outside.  This table is what the
//! two sides share instead: a service publishes its devices here and takes
//! read requests from here; a caller finds a device here, queues a request
//! and waits for the result while giving the services foreground time.
//!
//! Nothing here is borrowed across a service poll.  A request owns its data
//! buffer until the caller collects it, so a caller that gives up waiting
//! leaves nothing behind that a late completion could write into.
//!
//! Handles carry the device's attach generation and the media generation
//! they were opened on.  A device that has been unplugged answers
//! [`BlockIoError::NoMedia`]; a different device in the same place, or new
//! media in the same device, answers [`BlockIoError::MediaChanged`].

use alloc::vec::Vec;

use crate::io::fs::media::{BlockDevice, BlockIoError, LBA, MediaId, MediaInfo};

/// Mass storage interfaces that can be in use at once, across all
/// controllers.  Beyond this an interface is refused and counted.
pub const MAX_DEVICES: usize = 4;

/// How long a caller waits for one command's worth of a request: the command
/// itself, its recovery and its retry.
pub const REQUEST_BUDGET_US: u64 = 15_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backend {
    Dwc2,
    Xhci,
}

/// Where a device is and what it said it was.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    pub backend: Backend,
    /// Root port (xHCI) or USB address (DWC2).
    pub port: u8,
    /// Downstream hub port, when behind one.
    pub hub_port: Option<u8>,
    pub interface: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub max_lun: u8,
    pub vendor: [u8; 8],
    pub product: [u8; 16],
}

impl DeviceInfo {
    pub const fn new(backend: Backend, port: u8, hub_port: Option<u8>, interface: u8) -> Self {
        Self {
            backend,
            port,
            hub_port,
            interface,
            vendor_id: 0,
            product_id: 0,
            max_lun: 0,
            vendor: [b' '; 8],
            product: [b' '; 16],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaState {
    /// Being interrogated, or waiting for the medium to become ready.
    Probing,
    Ready {
        block_size: u32,
        block_count: u64,
        media_id: MediaId,
    },
    NoMedia,
    /// Not ready past the readiness window; looked at again periodically.
    NotReady,
    /// Given up on until the device is enumerated again.
    Failed(&'static str),
    /// Works, but is not something this driver reads.
    Unsupported(&'static str),
    /// Unplugged.  Kept so that old handles can tell.
    Detached,
}

/// Diagnostic counters and the last things that went wrong.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeviceStats {
    pub commands: u64,
    /// Bytes the controller reported as received in data stages, counted
    /// below everything that copies or checks them.
    pub bus_bytes_in: u64,
    pub reads: u64,
    pub bytes_read: u64,
    pub read_errors: u64,
    pub read_retries: u64,
    pub transport_errors: u64,
    pub timeouts: u64,
    pub stalls: u64,
    pub recoveries: u64,
    pub recovery_failures: u64,
    pub invalid_csws: u64,
    pub phase_errors: u64,
    pub media_changes: u64,
    /// Sense key, ASC and ASCQ of the last failed command.
    pub last_sense: (u8, u8, u8),
}

/// A published device, as seen from outside.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceSummary {
    pub index: usize,
    pub generation: u32,
    pub info: DeviceInfo,
    pub state: MediaState,
    pub stats: DeviceStats,
}

/// Identifies one attachment of one interface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceHandle {
    pub index: usize,
    pub generation: u32,
}

#[derive(Debug, Eq, PartialEq)]
pub enum RequestState {
    Queued,
    Active,
    Done(Result<(), BlockIoError>),
}

/// A read queued by a caller and carried out by a session.
#[derive(Debug)]
pub struct Request {
    pub lba: u64,
    pub blocks: u64,
    pub media_id: MediaId,
    pub data: Vec<u8>,
    pub state: RequestState,
    /// The caller stopped waiting.  The session finishes or fails it, and the
    /// result is then thrown away.
    pub abandoned: bool,
    /// A non-destructive re-initialisation rather than a read.
    pub reinit: bool,
}

struct Entry {
    generation: u32,
    attached: bool,
    info: DeviceInfo,
    state: MediaState,
    stats: DeviceStats,
    request: Option<Request>,
    /// The last medium seen ready, so a reset that finds it again can keep
    /// its generation.
    last_media: Option<(MediaId, u32, u64)>,
}

impl Entry {
    const EMPTY: Self = Self {
        generation: 0,
        attached: false,
        info: DeviceInfo::new(Backend::Xhci, 0, None, 0),
        state: MediaState::Detached,
        stats: DeviceStats {
            commands: 0,
            bus_bytes_in: 0,
            reads: 0,
            bytes_read: 0,
            read_errors: 0,
            read_retries: 0,
            transport_errors: 0,
            timeouts: 0,
            stalls: 0,
            recoveries: 0,
            recovery_failures: 0,
            invalid_csws: 0,
            phase_errors: 0,
            media_changes: 0,
            last_sense: (0, 0, 0),
        },
        request: None,
        last_media: None,
    };
}

pub struct Registry {
    entries: [Entry; MAX_DEVICES],
    next_generation: u32,
    next_media_id: u32,
    rejected: u64,
    now_us: Option<fn() -> u64>,
}

impl Registry {
    pub const fn new() -> Self {
        Self {
            entries: [Entry::EMPTY, Entry::EMPTY, Entry::EMPTY, Entry::EMPTY],
            next_generation: 1,
            next_media_id: 1,
            rejected: 0,
            now_us: None,
        }
    }

    /// The clock the backends run on.  Set by whichever registers first; the
    /// block device uses it for its own deadline.
    pub fn set_clock(&mut self, now_us: fn() -> u64) {
        if self.now_us.is_none() {
            self.now_us = Some(now_us);
        }
    }

    pub fn now_us(&self) -> u64 {
        self.now_us.map_or(0, |f| f())
    }

    /// Interfaces turned away because the table was full.
    pub const fn rejected(&self) -> u64 {
        self.rejected
    }

    pub fn has_room(&self) -> bool {
        self.entries.iter().any(Self::reusable)
    }

    /// Places an [`Self::attach`] would succeed on.
    pub fn free_places(&self) -> usize {
        self.entries.iter().filter(|e| Self::reusable(e)).count()
    }

    /// Counts interfaces a backend turned away before attaching, for want of
    /// a place.
    pub fn note_rejected(&mut self, count: usize) {
        self.rejected += count as u64;
    }

    /// A place is free once nothing is attached there and no request is
    /// still waiting to be collected.  A detached entry stays as it is until
    /// it is reused, so a handle to it reads [`MediaState::Detached`] until
    /// then.
    fn reusable(entry: &Entry) -> bool {
        !entry.attached && entry.request.is_none()
    }

    /// Publishes a new interface.  `None` when all places are taken.
    pub fn attach(&mut self, info: DeviceInfo) -> Option<DeviceHandle> {
        // The lowest free place, so that a device plugged back in comes back
        // under the number it had.  A handle to what was there before tells
        // the difference by the generation and answers `MediaChanged`.
        let index = self.entries.iter().position(Self::reusable);
        let Some(index) = index else {
            self.rejected += 1;
            return None;
        };
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        let entry = &mut self.entries[index];
        *entry = Entry::EMPTY;
        entry.generation = generation;
        entry.attached = true;
        entry.info = info;
        entry.state = MediaState::Probing;
        Some(DeviceHandle { index, generation })
    }

    fn entry(&self, handle: DeviceHandle) -> Option<&Entry> {
        self.entries
            .get(handle.index)
            .filter(|e| e.generation == handle.generation && handle.generation != 0)
    }

    fn entry_mut(&mut self, handle: DeviceHandle) -> Option<&mut Entry> {
        self.entries
            .get_mut(handle.index)
            .filter(|e| e.generation == handle.generation && handle.generation != 0)
    }

    /// The device went away.  Its request, if any, fails with `NoMedia`.
    pub fn detach(&mut self, handle: DeviceHandle) {
        if let Some(entry) = self.entry_mut(handle) {
            entry.attached = false;
            entry.state = MediaState::Detached;
            if let Some(request) = entry.request.as_mut()
                && !matches!(request.state, RequestState::Done(_))
            {
                request.state = RequestState::Done(Err(BlockIoError::NoMedia));
            }
            if entry.request.as_ref().is_some_and(|r| r.abandoned) {
                entry.request = None;
            }
        }
    }

    pub fn update_info(&mut self, handle: DeviceHandle, update: impl FnOnce(&mut DeviceInfo)) {
        if let Some(entry) = self.entry_mut(handle) {
            update(&mut entry.info);
        }
    }

    pub fn stats_mut(&mut self, handle: DeviceHandle) -> Option<&mut DeviceStats> {
        self.entry_mut(handle).map(|e| &mut e.stats)
    }

    pub fn state(&self, handle: DeviceHandle) -> Option<MediaState> {
        self.entry(handle).map(|e| e.state)
    }

    /// Records a state other than `Ready`.
    pub fn set_state(&mut self, handle: DeviceHandle, state: MediaState) {
        debug_assert!(!matches!(state, MediaState::Ready { .. }));
        if let Some(entry) = self.entry_mut(handle) {
            entry.state = state;
        }
    }

    /// Records that readable media is present.  Unless the session knows it
    /// is `same_medium` as the one last ready, with the same geometry, it
    /// gets a new media generation: a handle opened before — on other media,
    /// or before a Unit Attention — is not silently carried over.
    pub fn media_ready(
        &mut self,
        handle: DeviceHandle,
        block_size: u32,
        block_count: u64,
        same_medium: bool,
    ) {
        let fresh = MediaId(self.next_media_id);
        let Some(entry) = self.entry_mut(handle) else {
            return;
        };
        let media_id = match entry.last_media {
            Some((id, size, count))
                if same_medium && size == block_size && count == block_count =>
            {
                id
            }
            _ => {
                self.next_media_id = self.next_media_id.wrapping_add(1).max(1);
                fresh
            }
        };
        let entry = self.entry_mut(handle).unwrap();
        entry.last_media = Some((media_id, block_size, block_count));
        entry.state = MediaState::Ready {
            block_size,
            block_count,
            media_id,
        };
    }

    /// The next request for a session to carry out, marked active.
    pub fn take_queued(&mut self, handle: DeviceHandle) -> Option<&mut Request> {
        let request = self.entry_mut(handle)?.request.as_mut()?;
        if request.state != RequestState::Queued {
            return None;
        }
        request.state = RequestState::Active;
        Some(request)
    }

    pub fn active_request(&mut self, handle: DeviceHandle) -> Option<&mut Request> {
        self.entry_mut(handle)?
            .request
            .as_mut()
            .filter(|r| r.state == RequestState::Active)
    }

    pub fn has_queued(&self, handle: DeviceHandle) -> bool {
        self.entry(handle)
            .and_then(|e| e.request.as_ref())
            .is_some_and(|r| r.state == RequestState::Queued)
    }

    /// Finishes the request the session is working on.
    pub fn finish_request(&mut self, handle: DeviceHandle, result: Result<(), BlockIoError>) {
        let Some(entry) = self.entry_mut(handle) else {
            return;
        };
        let Some(request) = entry.request.as_mut() else {
            return;
        };
        if !matches!(request.state, RequestState::Done(_)) {
            if !request.reinit {
                match result {
                    Ok(()) => {
                        entry.stats.reads += 1;
                        entry.stats.bytes_read += request.data.len() as u64;
                    }
                    Err(_) => entry.stats.read_errors += 1,
                }
            }
            request.state = RequestState::Done(result);
        }
        if request.abandoned {
            entry.request = None;
        }
    }

    /// The devices attached now.
    pub fn summaries(&self) -> Vec<DeviceSummary> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.generation != 0 && e.attached)
            .map(|(index, e)| DeviceSummary {
                index,
                generation: e.generation,
                info: e.info,
                state: e.state,
                stats: e.stats,
            })
            .collect()
    }

    /// The device currently at `index`, if one is attached.
    pub fn current(&self, index: usize) -> Option<DeviceHandle> {
        let entry = self.entries.get(index)?;
        (entry.attached && entry.generation != 0).then_some(DeviceHandle {
            index,
            generation: entry.generation,
        })
    }

    /// Checks a handle against the device and media it was opened on.
    fn check(&self, handle: DeviceHandle, media_id: MediaId) -> Result<&Entry, BlockIoError> {
        let entry = self
            .entries
            .get(handle.index)
            .ok_or(BlockIoError::InvalidParameter)?;
        if entry.generation != handle.generation {
            return Err(BlockIoError::MediaChanged);
        }
        match entry.state {
            MediaState::Ready { media_id: m, .. } if m == media_id => Ok(entry),
            MediaState::Ready { .. } => Err(BlockIoError::MediaChanged),
            MediaState::Detached | MediaState::NoMedia => Err(BlockIoError::NoMedia),
            // A Unit Attention put it back to probing: whatever it finds,
            // it will be under a new media generation.
            MediaState::Probing => Err(BlockIoError::MediaChanged),
            _ => Err(BlockIoError::DeviceError),
        }
    }

    /// Queues a read of `blocks` blocks from `lba` for the device and media
    /// `handle` and `media_id` name.  One request per device at a time.
    pub fn submit_read(
        &mut self,
        handle: DeviceHandle,
        media_id: MediaId,
        lba: u64,
        blocks: u64,
        block_size: u32,
    ) -> Result<(), BlockIoError> {
        let entry = self.check(handle, media_id)?;
        let MediaState::Ready { block_count, .. } = entry.state else {
            return Err(BlockIoError::DeviceError);
        };
        let end = lba
            .checked_add(blocks)
            .ok_or(BlockIoError::InvalidParameter)?;
        if end > block_count {
            return Err(BlockIoError::InvalidParameter);
        }
        let bytes = blocks
            .checked_mul(block_size as u64)
            .and_then(|b| usize::try_from(b).ok())
            .ok_or(BlockIoError::InvalidParameter)?;
        if entry.request.is_some() {
            // The queue is one deep per interface; a request still in flight
            // — including one a caller gave up on — keeps the place.
            return Err(BlockIoError::DeviceError);
        }
        let mut data = Vec::new();
        data.try_reserve_exact(bytes)
            .map_err(|_| BlockIoError::DeviceError)?;
        data.resize(bytes, 0);
        let entry = self.entry_mut(handle).ok_or(BlockIoError::NoMedia)?;
        entry.request = Some(Request {
            lba,
            blocks,
            media_id,
            data,
            state: RequestState::Queued,
            abandoned: false,
            reinit: false,
        });
        Ok(())
    }

    /// Queues a non-destructive re-initialisation: the device's readiness
    /// and capacity are established again.
    pub fn submit_reinit(&mut self, handle: DeviceHandle) -> Result<(), BlockIoError> {
        let entry = self.entry_mut(handle).ok_or(BlockIoError::NoMedia)?;
        if !entry.attached {
            return Err(BlockIoError::NoMedia);
        }
        if entry.request.is_some() {
            return Err(BlockIoError::DeviceError);
        }
        entry.request = Some(Request {
            lba: 0,
            blocks: 0,
            media_id: MediaId::ZERO,
            data: Vec::new(),
            state: RequestState::Queued,
            abandoned: false,
            reinit: true,
        });
        Ok(())
    }

    /// Collects a finished request.
    pub fn take_result(&mut self, handle: DeviceHandle) -> Option<Result<Vec<u8>, BlockIoError>> {
        let entry = self.entries.get_mut(handle.index)?;
        if entry.generation != handle.generation {
            return None;
        }
        if !matches!(entry.request.as_ref()?.state, RequestState::Done(_)) {
            return None;
        }
        let request = entry.request.take()?;
        match request.state {
            RequestState::Done(Ok(())) => Some(Ok(request.data)),
            RequestState::Done(Err(error)) => Some(Err(error)),
            _ => None,
        }
    }

    /// The caller has stopped waiting for its request.
    pub fn abandon(&mut self, handle: DeviceHandle) {
        let Some(entry) = self.entries.get_mut(handle.index) else {
            return;
        };
        if entry.generation != handle.generation {
            return;
        }
        match entry.request.as_mut() {
            Some(request) if matches!(request.state, RequestState::Done(_)) => entry.request = None,
            // Never started: nothing refers to its buffer, so it can go now.
            Some(request) if request.state == RequestState::Queued => entry.request = None,
            Some(request) => request.abandoned = true,
            None => {}
        }
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

static mut GLOBAL: Registry = Registry::new();

/// Runs `f` on the system-wide registry.
///
/// The system runs services and callers on one foreground core, and neither
/// side calls back into the other from inside `f`, so the access is never
/// nested.
pub fn with_global<R>(f: impl FnOnce(&mut Registry) -> R) -> R {
    unsafe { f(&mut *(&raw mut GLOBAL)) }
}

/// What a synchronous read needs from its surroundings.  The system provides
/// the real one; tests provide one that drives a fake device.
pub trait BlockHost {
    /// True while the services are being polled: a read started from inside
    /// one could never complete, so it is refused at once.
    fn in_service(&self) -> bool;
    /// Gives the services one round of foreground time.
    fn poll(&mut self);
    fn with_registry<R>(&mut self, f: impl FnOnce(&mut Registry) -> R) -> R;
}

/// The running system.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemHost;

impl BlockHost for SystemHost {
    fn in_service(&self) -> bool {
        #[cfg(not(test))]
        {
            crate::System::is_polling_services()
        }
        #[cfg(test)]
        {
            false
        }
    }

    fn poll(&mut self) {
        #[cfg(not(test))]
        crate::System::poll_services();
    }

    fn with_registry<R>(&mut self, f: impl FnOnce(&mut Registry) -> R) -> R {
        with_global(f)
    }
}

/// Microseconds from the clock the USB services use, which reads the
/// hardware counter directly.  Unlike the system tick it does not depend on
/// the timer interrupt being serviced on time.  Zero before a USB service
/// has set it.
pub fn now_us() -> u64 {
    with_global(|r| r.now_us())
}

/// Lists the USB block devices the system knows about.
pub fn devices() -> Vec<DeviceSummary> {
    with_global(|r| r.summaries())
}

/// Opens the device at `index` on the media it has now.
pub fn open(index: usize) -> Result<UsbBlockDevice, BlockIoError> {
    UsbBlockDevice::open_on(SystemHost, index)
}

/// A read-only block device over one USB mass storage interface, LUN 0.
///
/// Opened on one medium.  Once the device reports a medium change, or is
/// replaced, the handle stops working and a new one has to be opened: the
/// new medium is never handed to an old handle.
pub struct UsbBlockDevice<H: BlockHost = SystemHost> {
    pub(super) host: H,
    handle: DeviceHandle,
    info: MediaInfo,
}

impl<H: BlockHost> UsbBlockDevice<H> {
    pub fn open_on(mut host: H, index: usize) -> Result<Self, BlockIoError> {
        let (handle, state) = host.with_registry(|r| {
            let handle = r.current(index).ok_or(BlockIoError::NoMedia)?;
            Ok((handle, r.state(handle)))
        })?;
        match state {
            Some(MediaState::Ready {
                block_size,
                block_count,
                media_id,
            }) => Ok(Self {
                host,
                handle,
                info: MediaInfo {
                    media_id,
                    flags: 0,
                    block_size,
                    io_align: 1,
                    block_count: LBA(block_count),
                },
            }),
            Some(MediaState::NoMedia) | Some(MediaState::Detached) | None => {
                Err(BlockIoError::NoMedia)
            }
            _ => Err(BlockIoError::DeviceError),
        }
    }

    pub const fn handle(&self) -> DeviceHandle {
        self.handle
    }

    /// Waits for the request just queued, giving the services foreground time
    /// in between.  Returns its data, or gives up and abandons it once the
    /// budget has passed.
    fn wait(&mut self, budget_us: u64) -> Result<Vec<u8>, BlockIoError> {
        let handle = self.handle;
        let start = self.host.with_registry(|r| r.now_us());
        loop {
            // Nothing from the registry is held across this: the services
            // borrow it themselves while they run.
            self.host.poll();
            let (result, now) = self
                .host
                .with_registry(|r| (r.take_result(handle), r.now_us()));
            if let Some(result) = result {
                return result;
            }
            if now.wrapping_sub(start) > budget_us {
                self.host.with_registry(|r| r.abandon(handle));
                return Err(BlockIoError::DeviceError);
            }
            core::hint::spin_loop();
        }
    }
}

/// Commands a request of `bytes` bytes is split into, for its time budget.
fn commands_for(bytes: usize) -> u64 {
    (bytes.div_ceil(super::bot::MAX_COMMAND_BYTES) as u64).max(1)
}

impl<H: BlockHost> BlockDevice for UsbBlockDevice<H> {
    /// Asks the device to establish readiness and capacity again.  Nothing is
    /// reset on the bus unless the device fails to answer.  The handle stays
    /// on the medium it was opened on; if the device now reports a different
    /// one, reads through this handle fail with `MediaChanged`.
    fn reset(&mut self) -> Result<(), BlockIoError> {
        if self.host.in_service() {
            return Err(BlockIoError::DeviceError);
        }
        let handle = self.handle;
        self.host.with_registry(|r| r.submit_reinit(handle))?;
        self.wait(REQUEST_BUDGET_US + super::session::READY_WINDOW_US)?;
        self.host.with_registry(|r| match r.state(handle) {
            Some(MediaState::Ready { .. }) => Ok(()),
            Some(MediaState::NoMedia) | Some(MediaState::Detached) | None => {
                Err(BlockIoError::NoMedia)
            }
            _ => Err(BlockIoError::DeviceError),
        })
    }

    fn read(&mut self, lba: LBA, buffer: &mut [u8]) -> Result<(), BlockIoError> {
        let block_size = self.info.block_size as usize;
        if block_size == 0 || buffer.len() % block_size != 0 {
            return Err(BlockIoError::BadBufferSize);
        }
        if self.host.in_service() {
            return Err(BlockIoError::DeviceError);
        }
        let handle = self.handle;
        let media_id = self.info.media_id;
        let blocks = (buffer.len() / block_size) as u64;
        let end = lba
            .0
            .checked_add(blocks)
            .ok_or(BlockIoError::InvalidParameter)?;
        if end > self.info.block_count.0 {
            return Err(BlockIoError::InvalidParameter);
        }
        // Give the services a turn before judging the handle.  A device that
        // has failed refuses at once, and a caller that retries in a loop —
        // a benchmark, a scan — would otherwise never let the USB stack run,
        // so an unplug would go unnoticed for as long as the loop lasts.
        self.host.poll();
        if blocks == 0 {
            // No I/O, but only for a handle that is still good.
            return self
                .host
                .with_registry(|r| r.check(handle, media_id).map(|_| ()));
        }
        self.host
            .with_registry(|r| r.submit_read(handle, media_id, lba.0, blocks, block_size as u32))?;
        let data = self.wait(REQUEST_BUDGET_US * commands_for(buffer.len()))?;
        if data.len() != buffer.len() {
            return Err(BlockIoError::DeviceError);
        }
        buffer.copy_from_slice(&data);
        Ok(())
    }

    /// Never reaches the device.
    fn write(&mut self, _lba: LBA, _buffer: &[u8]) -> Result<(), BlockIoError> {
        Err(BlockIoError::WriteProtected)
    }

    /// Nothing is ever written, so nothing is ever cached for writing.
    fn flush(&mut self) -> Result<(), BlockIoError> {
        Ok(())
    }

    fn media_info(&mut self) -> &MediaInfo {
        &self.info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> DeviceInfo {
        DeviceInfo::new(Backend::Xhci, 1, None, 0)
    }

    #[test]
    fn places_are_limited_and_refusals_counted() {
        let mut r = Registry::new();
        let handles: Vec<_> = (0..MAX_DEVICES)
            .map(|_| r.attach(info()).unwrap())
            .collect();
        assert!(!r.has_room());
        assert_eq!(r.attach(info()), None);
        assert_eq!(r.rejected(), 1);
        r.detach(handles[1]);
        let again = r.attach(info()).unwrap();
        assert_eq!(again.index, 1);
        assert_ne!(again.generation, handles[1].generation);
    }

    #[test]
    fn a_device_plugged_back_in_gets_its_number_back_and_the_old_one_is_not_listed() {
        let mut r = Registry::new();
        let first = r.attach(info()).unwrap();
        for _ in 0..20 {
            let current = r.current(0).unwrap();
            r.detach(current);
            assert!(r.summaries().is_empty(), "a detached device is not listed");
            let again = r.attach(info()).unwrap();
            assert_eq!(again.index, 0);
            assert_eq!(r.summaries().len(), 1);
        }
        assert_ne!(r.current(0).unwrap().generation, first.generation);
    }

    #[test]
    fn requests_are_checked_before_they_are_queued() {
        let mut r = Registry::new();
        let h = r.attach(info()).unwrap();
        r.media_ready(h, 512, 100, false);
        let Some(MediaState::Ready { media_id, .. }) = r.state(h) else {
            panic!()
        };
        assert_eq!(
            r.submit_read(h, media_id, 99, 2, 512),
            Err(BlockIoError::InvalidParameter)
        );
        assert_eq!(
            r.submit_read(h, media_id, u64::MAX, 2, 512),
            Err(BlockIoError::InvalidParameter)
        );
        assert_eq!(r.submit_read(h, media_id, 98, 2, 512), Ok(()));
        assert_eq!(
            r.submit_read(h, media_id, 0, 1, 512),
            Err(BlockIoError::DeviceError),
            "one request at a time"
        );
        assert_eq!(
            r.submit_read(h, MediaId(media_id.0 + 1), 0, 1, 512),
            Err(BlockIoError::MediaChanged)
        );
    }

    #[test]
    fn a_detach_fails_the_request_and_old_handles_see_no_media() {
        let mut r = Registry::new();
        let h = r.attach(info()).unwrap();
        r.media_ready(h, 512, 100, false);
        let Some(MediaState::Ready { media_id, .. }) = r.state(h) else {
            panic!()
        };
        r.submit_read(h, media_id, 0, 1, 512).unwrap();
        assert!(r.take_queued(h).is_some());
        r.detach(h);
        assert_eq!(r.take_result(h), Some(Err(BlockIoError::NoMedia)));
        assert_eq!(r.check(h, media_id).err(), Some(BlockIoError::NoMedia));
        // Another device in the same place is a different device.
        let other = r.attach(info()).unwrap();
        assert_eq!(other.index, h.index);
        assert_eq!(r.check(h, media_id).err(), Some(BlockIoError::MediaChanged));
    }

    #[test]
    fn new_media_is_a_new_generation() {
        let mut r = Registry::new();
        let h = r.attach(info()).unwrap();
        r.media_ready(h, 512, 100, false);
        let Some(MediaState::Ready { media_id, .. }) = r.state(h) else {
            panic!()
        };
        r.set_state(h, MediaState::Probing);
        assert_eq!(r.check(h, media_id).err(), Some(BlockIoError::MediaChanged));
        // The same medium found again by a reset keeps its generation...
        r.media_ready(h, 512, 100, true);
        assert!(r.check(h, media_id).is_ok());
        // ...but not with a different geometry, or after a Unit Attention.
        r.media_ready(h, 512, 200, true);
        assert_eq!(r.check(h, media_id).err(), Some(BlockIoError::MediaChanged));
        let Some(MediaState::Ready { media_id, .. }) = r.state(h) else {
            panic!()
        };
        r.media_ready(h, 512, 200, false);
        assert_eq!(r.check(h, media_id).err(), Some(BlockIoError::MediaChanged));
    }

    #[test]
    fn an_abandoned_request_keeps_its_buffer_until_the_session_lets_go() {
        let mut r = Registry::new();
        let h = r.attach(info()).unwrap();
        r.media_ready(h, 512, 100, false);
        let Some(MediaState::Ready { media_id, .. }) = r.state(h) else {
            panic!()
        };
        r.submit_read(h, media_id, 0, 1, 512).unwrap();
        r.take_queued(h).unwrap();
        r.abandon(h);
        // Still there: the session may be writing into it.
        assert!(r.active_request(h).is_some());
        assert_eq!(
            r.submit_read(h, media_id, 0, 1, 512),
            Err(BlockIoError::DeviceError)
        );
        r.finish_request(h, Ok(()));
        assert!(r.active_request(h).is_none());
        assert_eq!(r.take_result(h), None, "the result went with the caller");
        assert_eq!(r.submit_read(h, media_id, 0, 1, 512), Ok(()));
        // A queued request nobody has started can simply go.
        r.abandon(h);
        assert!(!r.has_queued(h));
    }
}
