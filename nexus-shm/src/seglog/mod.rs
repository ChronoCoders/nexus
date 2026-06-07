mod conductor;
mod error;
mod frame;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::region::MapOptions;
use crate::segment::Segment;

use conductor::{CleanRequest, ConductorHandle};
use frame::{ALIGN, FRAME_HDR, align_up, commit_len_ptr, footprint, session_id_ptr};

pub use error::SegmentedLogError;
pub use frame::{Frame, LogOffset};

struct Slot {
    _segment: Segment,
    data: *mut u8,
}

// SAFETY: a `Slot` is a mmap'd segment handle plus a cached data pointer into
// that mapping. The mapping lives in shared memory, not thread-local state.
// Concurrent access is governed by the frame-level atomics.
unsafe impl Send for Slot {}

/// Three-segment bounded append log with background segment rotation.
///
/// Maintains three fixed mmap'd segments: one active for appends, one
/// read-only for lookups, one being cleaned by a conductor thread. When the
/// active segment fills, roles rotate: old readable becomes conductor input,
/// active becomes readable, clean standby becomes active. The hot-path append
/// never blocks on cleaning; the conductor must finish before the *next*
/// rotation (size segments with enough headroom for the expected message rate).
///
/// # Global offset addressing
///
/// Sequential reads use a monotonically increasing `u64` position that spans
/// all segments. The physical slot is derived via modular arithmetic:
///
/// ```text
/// segment_number = pos / segment_size
/// local_offset   = pos % segment_size
/// slot_index     = segment_number % 3
/// ```
///
/// This works because the init order (`current=0, prev=2, standby=1`) is
/// chosen so that the rotation cycle (`current->prev, standby->current,
/// old_prev->standby`) visits slots in order 0 -> 1 -> 2 -> 0 -> ...:
///
/// ```text
/// epoch 0: current=0  prev=2  standby=1
/// epoch 1: current=1  prev=0  standby=2
/// epoch 2: current=2  prev=1  standby=0
/// epoch 3: current=0  prev=2  standby=1   (cycle repeats)
/// ```
///
/// At any point, only the current (`epoch`) and previous (`epoch - 1`)
/// segments are readable. Older segments have been handed to the conductor
/// for cleaning.
pub struct SegmentedLog {
    // conductor is dropped first (joins thread), then slots (unmaps memory).
    conductor: ConductorHandle,
    slots: [Slot; 3],
    segment_size: usize,
    current: usize,
    prev: usize,
    standby: usize,
    cursor: usize,
    epoch: u64,
    slot_gen: [u32; 3],
}

fn slot_path(base: &Path, i: u8) -> PathBuf {
    let mut s = base.as_os_str().to_owned();
    s.push(format!(".seg{i}"));
    PathBuf::from(s)
}

impl SegmentedLog {
    /// Open (or recreate) a three-segment log rooted at `base`.
    ///
    /// Segment files are `{base}.seg0`, `{base}.seg1`, `{base}.seg2`.
    /// `segment_size` is rounded up to an 8-byte boundary.
    pub fn open(
        base: &Path,
        segment_size: usize,
        map: MapOptions,
    ) -> Result<Self, SegmentedLogError> {
        let size = align_up(segment_size.max(FRAME_HDR * 8));

        let mk = |i: u8| -> Result<Slot, SegmentedLogError> {
            let seg = Segment::create(&slot_path(base, i), size, map)?;
            let data = seg.data();
            Ok(Slot {
                _segment: seg,
                data,
            })
        };

        let s0 = mk(0)?;
        let s1 = mk(1)?;
        let s2 = mk(2)?;

        // SAFETY: each `sN.data` points to the start of a freshly mapped segment
        // with at least `FRAME_HDR` bytes and 4-byte alignment guaranteed by mmap.
        // Zeroing commit_len prevents stale data from a prior run being interpreted
        // as a committed record by `read()`.
        unsafe {
            (*commit_len_ptr(s0.data)).store(0, Ordering::Relaxed);
            (*commit_len_ptr(s1.data)).store(0, Ordering::Relaxed);
            (*commit_len_ptr(s2.data)).store(0, Ordering::Relaxed);
        }

        Ok(Self {
            conductor: ConductorHandle::spawn(),
            slots: [s0, s1, s2],
            segment_size: size,
            current: 0,
            prev: 2,
            standby: 1,
            cursor: 0,
            epoch: 0,
            slot_gen: [0, u32::MAX, u32::MAX],
        })
    }

    pub fn segment_size(&self) -> usize {
        self.segment_size
    }

    /// Append `payload` to the active segment, tagged with `session_id`.
    ///
    /// Returns a [`LogOffset`] valid for reads until the slot is rotated out
    /// (two rotations after this write).
    pub fn append(
        &mut self,
        session_id: u32,
        payload: &[u8],
    ) -> Result<LogOffset, SegmentedLogError> {
        let body = payload.len();
        let foot = footprint(body);
        if foot > self.segment_size {
            return Err(SegmentedLogError::RecordTooLarge {
                max: self.segment_size.saturating_sub(FRAME_HDR),
            });
        }
        if self.cursor + foot > self.segment_size {
            self.rotate()?;
        }
        let off = self.cursor;
        let data = self.slots[self.current].data;
        // SAFETY: `off + foot <= self.segment_size` (checked above or after rotate).
        // `data` points into a live mmap'd segment that is at least `segment_size`
        // bytes. Frame header fields are at 4-byte-aligned offsets within the
        // segment. The sentinel store at `data.add(next)` is bounds-checked before
        // it is written.
        unsafe {
            let ptr = data.add(off);
            std::ptr::copy_nonoverlapping(payload.as_ptr(), ptr.add(FRAME_HDR), body);
            *session_id_ptr(ptr) = session_id;
            let next = off + foot;
            if next + FRAME_HDR <= self.segment_size {
                (*commit_len_ptr(data.add(next))).store(0, Ordering::Relaxed);
            }
            // Store body+1 so that 0 remains the unambiguous "not committed" sentinel,
            // allowing empty payloads (body=0) to be stored and read back correctly.
            (*commit_len_ptr(ptr)).store((body as u32).wrapping_add(1), Ordering::Release);
        }
        self.cursor += foot;
        Ok(LogOffset::new(
            self.current as u8,
            off,
            self.slot_gen[self.current],
        ))
    }

    /// Return the frame stored at `offset`, or `None` if the slot has been
    /// rotated out and is no longer readable.
    pub fn read(&self, offset: LogOffset) -> Option<Frame<'_>> {
        let slot = offset.slot();
        if slot != self.current && slot != self.prev {
            return None;
        }
        if offset.epoch() != self.slot_gen[slot] {
            return None;
        }
        let off = offset.local_off();
        let data = self.slots[slot].data;
        // SAFETY: `slot` is either `current` or `prev`, both of which hold live
        // mmap'd segments. `off` is a value previously returned by `append()` for
        // this slot, so `off < segment_size`. The bounds check on `off + FRAME_HDR
        // + body` prevents reading past the end of the segment.
        unsafe {
            let ptr = data.add(off);
            let stored = (*commit_len_ptr(ptr)).load(Ordering::Acquire);
            if stored == 0 {
                return None;
            }
            let body = (stored - 1) as usize;
            if off + FRAME_HDR + body > self.segment_size {
                return None;
            }
            Some(Frame::new(
                std::slice::from_raw_parts(ptr.add(FRAME_HDR), body),
                offset.global_offset(self.segment_size),
                *session_id_ptr(ptr),
            ))
        }
    }

    /// Monotonically increasing global offset at the current write position.
    pub fn write_pos(&self) -> u64 {
        self.epoch * self.segment_size as u64 + self.cursor as u64
    }

    /// Global offset at the start of the oldest readable segment.
    pub fn read_start(&self) -> u64 {
        if self.epoch == 0 {
            0
        } else {
            (self.epoch - 1) * self.segment_size as u64
        }
    }

    /// Read the next committed frame at `pos`, advancing past it.
    ///
    /// Returns `None` when caught up to the writer or when `pos` references
    /// an evicted segment. The slot is determined by `pos / segment_size % 3`;
    /// the init order guarantees this maps directly to the physical slot index.
    ///
    /// `pos` must be frame-aligned (a multiple of 8). Values obtained from
    /// [`read_start`] and advanced by this method satisfy this invariant.
    pub fn read_next(&self, pos: &mut u64) -> Option<Frame<'_>> {
        debug_assert!(
            (*pos).is_multiple_of(ALIGN as u64),
            "pos must be frame-aligned (got {pos})",
            pos = *pos,
        );
        let seg_size = self.segment_size as u64;
        let seg = *pos / seg_size;
        let local = (*pos % seg_size) as usize;
        let epoch = self.epoch;

        if seg > epoch || (epoch > 0 && seg + 1 < epoch) {
            return None;
        }

        let slot = (seg % 3) as usize;

        if local + FRAME_HDR > self.segment_size {
            if seg < epoch {
                *pos = (seg + 1) * seg_size;
                return self.read_next(pos);
            }
            return None;
        }

        let data = self.slots[slot].data;
        // SAFETY: `slot` is `seg % 3` where `seg` is either `epoch` (current) or
        // `epoch - 1` (prev), both live mmap'd segments. `local` is bounded by
        // `segment_size` via the modulo. The `local + FRAME_HDR` check above ensures
        // we don't read past the segment for the header. The `local + FRAME_HDR + body`
        // check below prevents reading past the segment for the payload.
        unsafe {
            let ptr = data.add(local);
            let stored = (*commit_len_ptr(ptr)).load(Ordering::Acquire);
            if stored == 0 {
                if seg < epoch {
                    *pos = (seg + 1) * seg_size;
                    return self.read_next(pos);
                }
                return None;
            }
            let body = (stored - 1) as usize;
            if local + FRAME_HDR + body > self.segment_size {
                return None;
            }
            let frame_offset = *pos;
            *pos += footprint(body) as u64;
            Some(Frame::new(
                std::slice::from_raw_parts(ptr.add(FRAME_HDR), body),
                frame_offset,
                *session_id_ptr(ptr),
            ))
        }
    }

    fn rotate(&mut self) -> Result<(), SegmentedLogError> {
        if !self.conductor.ready.load(Ordering::Acquire) {
            return Err(SegmentedLogError::StandbyNotReady);
        }
        let old_prev = self.prev;
        self.prev = self.current;
        self.current = self.standby;
        self.standby = old_prev;
        self.cursor = 0;
        self.epoch += 1;
        self.slot_gen[self.current] = self.epoch as u32;
        self.conductor.ready.store(false, Ordering::Release);
        let _ = self.conductor.tx.as_ref().map(|tx| {
            tx.try_send(CleanRequest {
                data: self.slots[old_prev].data,
                segment_size: self.segment_size,
            })
        });
        Ok(())
    }
}
