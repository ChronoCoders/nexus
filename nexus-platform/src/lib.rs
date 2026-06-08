//! Platform-specific OS primitives behind a portable Rust API.
//!
//! # Lock primitives
//!
//! - [`FileLock`] — RAII exclusive file lock for mutual exclusion
//! - [`LeaseLock`] — kernel-mediated process liveness detection
//! - [`Liveness`] — result of probing a lease lock

pub mod lock;

pub use lock::{FileLock, LeaseLock, Liveness};
