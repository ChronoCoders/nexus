use std::sync::atomic::AtomicI32;

pub(crate) const LEN_SIZE: usize = size_of::<i32>();
pub(crate) const ALIGN: usize = 8;

pub(crate) const fn align_up(n: usize) -> usize {
    (n + ALIGN - 1) & !(ALIGN - 1)
}

pub(crate) const fn footprint(body: usize) -> usize {
    LEN_SIZE + align_up(body)
}

pub(crate) unsafe fn marker<'a>(ptr: *mut u8) -> &'a AtomicI32 {
    // SAFETY: caller guarantees alignment, initialization, and liveness for 'a;
    // AtomicI32 shares i32's layout.
    unsafe { AtomicI32::from_ptr(ptr.cast()) }
}
