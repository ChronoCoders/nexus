use std::sync::atomic::AtomicU32;

pub(crate) const FRAME_HDR: usize = 8;
pub(crate) const ALIGN: usize = 8;

pub(crate) const fn align_up(n: usize) -> usize {
    (n + ALIGN - 1) & !(ALIGN - 1)
}

pub(crate) const fn footprint(body: usize) -> usize {
    FRAME_HDR + align_up(body)
}

pub(crate) fn commit_len_ptr(ptr: *mut u8) -> *mut AtomicU32 {
    ptr.cast()
}

pub(crate) fn session_id_ptr(ptr: *mut u8) -> *mut u32 {
    // SAFETY: ptr is frame-aligned (>= 4-byte), so ptr+4 is also 4-byte aligned.
    unsafe { ptr.add(4).cast() }
}

/// Zero-copy view of a committed record in the log.
///
/// Provides access to the session tag, global offset, and payload bytes
/// without copying from the underlying mmap'd segment.
#[repr(C)]
pub struct Frame<'buf> {
    payload: &'buf [u8],
    offset: u64,
    session_id: u32,
}

impl<'buf> Frame<'buf> {
    pub(crate) fn new(payload: &'buf [u8], offset: u64, session_id: u32) -> Self {
        Self {
            payload,
            offset,
            session_id,
        }
    }

    pub fn session_id(&self) -> u32 {
        self.session_id
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn payload(&self) -> &'buf [u8] {
        self.payload
    }
}

/// Opaque position handle returned by [`SegmentedLog::append`], passed to
/// [`SegmentedLog::read`]. Valid until the slot it references is rotated out.
///
/// Encoding: `[63:34]` = generation, `[33:32]` = slot index, `[31:0]` = local offset.
/// The default value (`u64::MAX`) encodes slot index 3 (no valid slot), so
/// [`SegmentedLog::read`] always returns `None` for a default offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogOffset(u64);

impl Default for LogOffset {
    fn default() -> Self {
        Self(u64::MAX)
    }
}

impl LogOffset {
    pub(crate) fn new(slot: u8, local_off: usize, epoch: u32) -> Self {
        Self((epoch as u64) << 34 | (slot as u64) << 32 | local_off as u64)
    }

    pub(crate) fn global_offset(self, segment_size: usize) -> u64 {
        self.epoch() as u64 * segment_size as u64 + self.local_off() as u64
    }

    pub(crate) fn slot(self) -> usize {
        ((self.0 >> 32) & 0x3) as usize
    }

    pub(crate) fn local_off(self) -> usize {
        (self.0 & 0xFFFF_FFFF) as usize
    }

    pub(crate) fn epoch(self) -> u32 {
        (self.0 >> 34) as u32
    }
}
