use std::marker::PhantomData;
use std::sync::atomic::Ordering;

use crate::segment::Segment;

use super::error::JournalError;
use super::frame::{LEN_SIZE, footprint, marker};
use super::header::RecordHeader;

pub struct Writer<H: RecordHeader> {
    pub(super) base: std::path::PathBuf,
    pub(super) segment_size: usize,
    pub(super) map: crate::region::MapOptions,
    pub(super) active: Segment,
    pub(super) index: u64,
    pub(super) tail: usize,
    pub(super) _marker: PhantomData<H>,
}

impl<H: RecordHeader> Writer<H> {
    pub fn try_claim(
        &mut self,
        header: H,
        payload_len: usize,
    ) -> Result<WriteClaim<'_, H>, JournalError> {
        let body = size_of::<H>() + payload_len;
        if body == 0 {
            return Err(JournalError::EmptyRecord);
        }
        let foot = footprint(body);
        if body > i32::MAX as usize || foot > self.segment_size {
            return Err(JournalError::RecordTooLarge {
                frame: foot,
                capacity: self.segment_size,
            });
        }
        if self.tail + foot > self.segment_size {
            self.roll()?;
        }
        Ok(WriteClaim {
            off: self.tail,
            body,
            foot,
            header,
            payload_len,
            writer: self,
        })
    }

    fn roll(&mut self) -> Result<(), JournalError> {
        let remaining = self.segment_size - self.tail;
        if remaining >= LEN_SIZE {
            let data = self.active.data();
            // SAFETY: tail is a 4-aligned offset within the mapped data region.
            let m = unsafe { marker(data.add(self.tail)) };
            m.store(-((remaining - LEN_SIZE) as i32), Ordering::Release);
        }
        self.index += 1;
        let path = super::segment_path(&self.base, self.index);
        self.active = Segment::create(&path, self.segment_size, self.map)?;
        self.tail = 0;
        Ok(())
    }
}

pub struct WriteClaim<'a, H: RecordHeader> {
    writer: &'a mut Writer<H>,
    off: usize,
    body: usize,
    foot: usize,
    header: H,
    payload_len: usize,
}

impl<H: RecordHeader> WriteClaim<'_, H> {
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        let start = self.off + LEN_SIZE + size_of::<H>();
        let data = self.writer.active.data();
        // SAFETY: the region is reserved for this claim, lies within the mapped
        // data, and is exclusively borrowed through `&mut self`.
        unsafe { std::slice::from_raw_parts_mut(data.add(start), self.payload_len) }
    }

    pub fn commit(self) {
        let data = self.writer.active.data();
        // SAFETY: the header slot is reserved for this claim and within the
        // mapped data; `H: Pod`, so an unaligned byte write is valid.
        unsafe { std::ptr::write_unaligned(data.add(self.off + LEN_SIZE).cast::<H>(), self.header) }

        let next = self.off + self.foot;
        if next + LEN_SIZE <= self.writer.segment_size {
            // SAFETY: `next` is a 4-aligned offset within the mapped data.
            let m = unsafe { marker(data.add(next)) };
            m.store(0, Ordering::Relaxed);
        }

        // SAFETY: the marker slot is 4-aligned and within the mapped data.
        let m = unsafe { marker(data.add(self.off)) };
        m.store(self.body as i32, Ordering::Release);
        self.writer.tail = next;
    }
}
