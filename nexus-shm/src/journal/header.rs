use crate::pod::Pod;

pub trait RecordHeader: Pod {}

impl<T: Pod> RecordHeader for T {}

pub trait SeqHeader: RecordHeader {
    fn seq(&self) -> u64;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct FixHeader {
    pub seq: u64,
    pub timestamp: u64,
}

// SAFETY: repr(C) over two u64 — fixed layout, no pointers, no Drop, valid for
// every bit pattern.
unsafe impl Pod for FixHeader {}

impl SeqHeader for FixHeader {
    fn seq(&self) -> u64 {
        self.seq
    }
}
