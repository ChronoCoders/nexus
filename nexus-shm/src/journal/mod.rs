mod error;
mod frame;
mod header;
mod reader;
#[cfg(test)]
mod tests;
mod writer;

use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::region::MapOptions;
use crate::segment::Segment;

pub use error::JournalError;
pub use header::{FixHeader, RecordHeader, SeqHeader};
pub use reader::{ReadRange, ReadRecord, Reader};
pub use writer::{WriteClaim, Writer};

use frame::{LEN_SIZE, align_up, footprint, marker};

const MIN_SEGMENT: usize = 64;

#[derive(Clone, Copy)]
pub struct JournalConfig {
    pub segment_size: usize,
    pub map: MapOptions,
}

impl Default for JournalConfig {
    fn default() -> Self {
        Self {
            segment_size: 64 * 1024 * 1024,
            map: MapOptions::default(),
        }
    }
}

pub struct Journal<H>(PhantomData<H>);

impl<H: RecordHeader> Journal<H> {
    pub fn open(
        base: impl AsRef<Path>,
        cfg: JournalConfig,
    ) -> Result<(Writer<H>, Reader<H>), JournalError> {
        let base = base.as_ref().to_path_buf();
        let segment_size = align_up(cfg.segment_size.max(MIN_SEGMENT));

        let mut last = None;
        let mut i = 0u64;
        while segment_path(&base, i).exists() {
            last = Some(i);
            i += 1;
        }

        let index = last.unwrap_or(0);
        let active = Segment::create(&segment_path(&base, index), segment_size, cfg.map)?;
        let tail = recover_tail::<H>(&active, segment_size);

        let writer = Writer {
            base: base.clone(),
            segment_size,
            map: cfg.map,
            active,
            index,
            tail,
            _marker: PhantomData,
        };

        let seg0 = Segment::attach(&segment_path(&base, 0), cfg.map)?;
        let reader = Reader {
            base,
            segment_size,
            map: cfg.map,
            segments: vec![seg0],
            seg_idx: 0,
            cursor: 0,
            _marker: PhantomData,
        };

        Ok((writer, reader))
    }
}

fn recover_tail<H: RecordHeader>(seg: &Segment, segment_size: usize) -> usize {
    let data = seg.data();
    let hsize = size_of::<H>();
    let mut cur = 0;
    while cur + LEN_SIZE <= segment_size {
        // SAFETY: `cur` is a 4-aligned offset within the mapped data region.
        let v = unsafe { marker(data.add(cur)) }.load(Ordering::Acquire);
        if v == 0 {
            break;
        }
        if v < 0 {
            cur += LEN_SIZE + (-v) as usize;
            continue;
        }
        let body = v as usize;
        if body < hsize || cur + footprint(body) > segment_size {
            break;
        }
        cur += footprint(body);
    }
    cur
}

fn segment_path(base: &Path, index: u64) -> PathBuf {
    let mut p = base.as_os_str().to_owned();
    p.push(format!(".{index}"));
    PathBuf::from(p)
}
