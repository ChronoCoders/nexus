use std::marker::PhantomData;
use std::ops::RangeBounds;
use std::sync::atomic::Ordering;

use crate::segment::Segment;

use super::error::JournalError;
use super::frame::{LEN_SIZE, footprint, marker};
use super::header::{RecordHeader, SeqHeader};

pub struct Reader<H: RecordHeader> {
    pub(super) base: std::path::PathBuf,
    pub(super) segment_size: usize,
    pub(super) map: crate::region::MapOptions,
    pub(super) segments: Vec<Segment>,
    pub(super) seg_idx: usize,
    pub(super) cursor: usize,
    pub(super) _marker: PhantomData<H>,
}

impl<H: RecordHeader> Reader<H> {
    pub fn next_record(&mut self) -> Option<ReadRecord<'_, H>> {
        loop {
            if self.cursor + LEN_SIZE > self.segment_size {
                if self.advance_segment() {
                    continue;
                }
                return None;
            }
            let data = self.segments[self.seg_idx].data();
            // SAFETY: cursor is a 4-aligned offset within the mapped data.
            let v = unsafe { marker(data.add(self.cursor)) }.load(Ordering::Acquire);
            if v == 0 {
                return None;
            }
            if v < 0 {
                self.cursor += LEN_SIZE + (-v) as usize;
                if self.cursor + LEN_SIZE > self.segment_size && !self.advance_segment() {
                    return None;
                }
                continue;
            }
            let body = v as usize;
            let hsize = size_of::<H>();
            if body < hsize {
                return None;
            }
            let off = self.cursor;
            // SAFETY: the committed frame holds `H` at `off + LEN_SIZE`; `H: Pod`
            // so an unaligned read is valid.
            let header = unsafe { std::ptr::read_unaligned(data.add(off + LEN_SIZE).cast::<H>()) };
            // SAFETY: the payload lies within the committed frame and the mapping
            // outlives the borrow held through `&mut self`.
            let payload = unsafe {
                std::slice::from_raw_parts(data.add(off + LEN_SIZE + hsize), body - hsize)
            };
            self.cursor = off + footprint(body);
            return Some(ReadRecord { header, payload });
        }
    }

    fn advance_segment(&mut self) -> bool {
        if self.seg_idx + 1 >= self.segments.len() && !self.load_next() {
            return false;
        }
        self.seg_idx += 1;
        self.cursor = 0;
        true
    }

    fn load_next(&mut self) -> bool {
        let next = self.segments.len() as u64;
        let path = super::segment_path(&self.base, next);
        match Segment::attach(&path, self.map) {
            Ok(seg) => {
                self.segments.push(seg);
                true
            }
            Err(_) => false,
        }
    }

    pub fn read_range<R>(&mut self, range: R) -> Result<ReadRange<'_, H>, JournalError>
    where
        H: SeqHeader,
        R: RangeBounds<u64>,
    {
        while self.load_next() {}
        let lo = match range.start_bound() {
            std::ops::Bound::Included(&n) => n,
            std::ops::Bound::Excluded(&n) => n.saturating_add(1),
            std::ops::Bound::Unbounded => 0,
        };
        let hi = match range.end_bound() {
            std::ops::Bound::Included(&n) => n,
            std::ops::Bound::Excluded(&n) => n.saturating_sub(1),
            std::ops::Bound::Unbounded => u64::MAX,
        };
        Ok(ReadRange {
            segments: &self.segments,
            segment_size: self.segment_size,
            seg_idx: 0,
            cursor: 0,
            lo,
            hi,
            _marker: PhantomData,
        })
    }
}

pub struct ReadRecord<'a, H: RecordHeader> {
    header: H,
    payload: &'a [u8],
}

impl<H: RecordHeader> ReadRecord<'_, H> {
    pub fn header(&self) -> H {
        self.header
    }

    pub fn payload(&self) -> &[u8] {
        self.payload
    }
}

pub struct ReadRange<'a, H: SeqHeader> {
    segments: &'a [Segment],
    segment_size: usize,
    seg_idx: usize,
    cursor: usize,
    lo: u64,
    hi: u64,
    _marker: PhantomData<H>,
}

impl<'a, H: SeqHeader> Iterator for ReadRange<'a, H> {
    type Item = ReadRecord<'a, H>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.seg_idx >= self.segments.len() {
                return None;
            }
            if self.cursor + LEN_SIZE > self.segment_size {
                self.seg_idx += 1;
                self.cursor = 0;
                continue;
            }
            let data = self.segments[self.seg_idx].data();
            // SAFETY: cursor is a 4-aligned offset within the mapped data.
            let v = unsafe { marker(data.add(self.cursor)) }.load(Ordering::Acquire);
            if v == 0 {
                self.seg_idx += 1;
                self.cursor = 0;
                continue;
            }
            if v < 0 {
                self.cursor += LEN_SIZE + (-v) as usize;
                continue;
            }
            let body = v as usize;
            let hsize = size_of::<H>();
            if body < hsize {
                return None;
            }
            let off = self.cursor;
            self.cursor = off + footprint(body);
            // SAFETY: the committed frame holds `H` at `off + LEN_SIZE`; `H: Pod`.
            let header = unsafe { std::ptr::read_unaligned(data.add(off + LEN_SIZE).cast::<H>()) };
            if header.seq() < self.lo || header.seq() > self.hi {
                continue;
            }
            // SAFETY: the payload lies within the committed frame and `segments`
            // outlives `'a`.
            let payload = unsafe {
                std::slice::from_raw_parts(data.add(off + LEN_SIZE + hsize), body - hsize)
            };
            return Some(ReadRecord { header, payload });
        }
    }
}
