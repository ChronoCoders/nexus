use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use super::frame::commit_len_ptr;

pub(crate) struct CleanRequest {
    pub(crate) data: *mut u8,
    pub(crate) segment_size: usize,
}

// SAFETY: `data` points into a mmap'd segment that remains mapped until the
// owning `Slot` drops, which happens only after `ConductorHandle` drops and
// the conductor thread joins.
unsafe impl Send for CleanRequest {}

fn conductor_main(rx: std::sync::mpsc::Receiver<CleanRequest>, ready: &Arc<AtomicBool>) {
    for req in rx {
        // SAFETY: `req.data` points to the start of a live mmap'd segment sent by
        // `rotate()`. The segment remains mapped until the owning `Slot` drops,
        // which happens only after `ConductorHandle` drops and this thread joins.
        unsafe { (*commit_len_ptr(req.data)).store(0, Ordering::Release) };
        let _ = req.segment_size;
        ready.store(true, Ordering::Release);
    }
}

pub(crate) struct ConductorHandle {
    pub(crate) tx: Option<std::sync::mpsc::SyncSender<CleanRequest>>,
    pub(crate) ready: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ConductorHandle {
    pub(crate) fn spawn() -> Self {
        let ready = Arc::new(AtomicBool::new(true));
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let thread = {
            let ready = Arc::clone(&ready);
            std::thread::spawn(move || conductor_main(rx, &ready))
        };
        Self {
            tx: Some(tx),
            ready,
            thread: Some(thread),
        }
    }
}

impl Drop for ConductorHandle {
    fn drop(&mut self) {
        drop(self.tx.take());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
