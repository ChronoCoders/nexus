/// Types byte-copyable through shared memory.
///
/// # Safety
///
/// The type must have a stable layout (`repr(C)`, `repr(transparent)`, or a
/// primitive), contain no pointers or `Drop` glue, and be valid for every bit
/// pattern of its size — a reader may observe bytes mid-write or from a crashed
/// writer. `bool` and `char` are excluded for this reason despite being `Copy`.
pub unsafe trait Pod: Copy + 'static {}

macro_rules! impl_pod {
    ($($t:ty),*) => { $( unsafe impl Pod for $t {} )* };
}

impl_pod!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64
);

unsafe impl<T: Pod, const N: usize> Pod for [T; N] {}
