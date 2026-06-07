#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub(crate) use linux::{lock_exclusive_blocking, try_lock_exclusive, unlock};

#[cfg(not(target_os = "linux"))]
compile_error!(
    "seglog file locking requires OFD locks (Linux). \
     macOS/Windows support is not yet implemented."
);
