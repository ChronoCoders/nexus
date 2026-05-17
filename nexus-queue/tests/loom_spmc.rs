#![cfg(loom)]

use loom::sync::Arc;
use loom::thread;
use nexus_queue::spmc;

#[test]
fn no_lost_items_two_consumers() {
    loom::model(|| {
        let (tx, rx) = spmc::ring_buffer::<u32>(2);
        let rx2 = rx.clone();

        tx.push(1).unwrap();
        tx.push(2).unwrap();
        drop(tx);

        let t1 = thread::spawn(move || {
            let mut received = Vec::new();
            loop {
                if let Some(v) = rx.pop() {
                    received.push(v);
                } else if rx.is_disconnected() {
                    while let Some(v) = rx.pop() {
                        received.push(v);
                    }
                    break;
                } else {
                    thread::yield_now();
                }
            }
            received
        });

        let t2 = thread::spawn(move || {
            let mut received = Vec::new();
            loop {
                if let Some(v) = rx2.pop() {
                    received.push(v);
                } else if rx2.is_disconnected() {
                    while let Some(v) = rx2.pop() {
                        received.push(v);
                    }
                    break;
                } else {
                    thread::yield_now();
                }
            }
            received
        });

        let mut all: Vec<_> = t1.join().unwrap();
        all.extend(t2.join().unwrap());
        all.sort_unstable();
        assert_eq!(all, vec![1, 2]);
    });
}

#[test]
fn no_duplicates() {
    loom::model(|| {
        let (tx, rx) = spmc::ring_buffer::<u32>(2);
        let rx2 = rx.clone();

        tx.push(10).unwrap();
        tx.push(11).unwrap();
        drop(tx);

        let t1 = thread::spawn(move || {
            let mut received = Vec::new();
            loop {
                if let Some(v) = rx.pop() {
                    received.push(v);
                } else if rx.is_disconnected() {
                    while let Some(v) = rx.pop() {
                        received.push(v);
                    }
                    break;
                } else {
                    thread::yield_now();
                }
            }
            received
        });

        let t2 = thread::spawn(move || {
            let mut received = Vec::new();
            loop {
                if let Some(v) = rx2.pop() {
                    received.push(v);
                } else if rx2.is_disconnected() {
                    while let Some(v) = rx2.pop() {
                        received.push(v);
                    }
                    break;
                } else {
                    thread::yield_now();
                }
            }
            received
        });

        let mut all: Vec<_> = t1.join().unwrap();
        all.extend(t2.join().unwrap());
        assert_eq!(all.len(), 2);
        all.sort_unstable();
        assert_eq!(all, vec![10, 11]);
    });
}

#[test]
fn concurrent_pop_cas_contention() {
    loom::model(|| {
        let (tx, rx) = spmc::ring_buffer::<u32>(2);
        let rx2 = rx.clone();

        tx.push(1).unwrap();

        let t1 = thread::spawn(move || rx.pop());
        let t2 = thread::spawn(move || rx2.pop());

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        let mut got = Vec::new();
        if let Some(v) = r1 {
            got.push(v);
        }
        if let Some(v) = r2 {
            got.push(v);
        }
        assert_eq!(got, vec![1]);
    });
}

#[test]
fn drop_with_pending_items() {
    loom::model(|| {
        let (tx, rx) = spmc::ring_buffer::<Arc<u32>>(2);
        let val = Arc::new(42);

        tx.push(Arc::clone(&val)).unwrap();

        drop(tx);
        drop(rx);

        assert_eq!(Arc::strong_count(&val), 1);
    });
}
