pub(crate) const FRAME_HEADER: usize = 8;
pub(crate) const ALIGN: usize = 8;

pub(crate) const TYPE_DATA: u16 = 0;
pub(crate) const TYPE_PAD: u16 = 1;

pub(crate) const fn align_up(n: usize) -> usize {
    (n + ALIGN - 1) & !(ALIGN - 1)
}

pub(crate) const fn footprint(body: usize) -> usize {
    FRAME_HEADER + align_up(body)
}
