#![cfg(loom)]

use loom::sync::Arc;
use loom::thread;
use nexus_queue::spsc;

#[test]
fn fifo_order() {
    loom::model(|| {
        let (tx, rx) = spsc::ring_buffer::<u32>(2);

        let t = thread::spawn(move || {
            while tx.push(1).is_err() {
                thread::yield_now();
            }
            while tx.push(2).is_err() {
                thread::yield_now();
            }
        });

        let mut received = Vec::new();
        while received.len() < 2 {
            if let Some(v) = rx.pop() {
                received.push(v);
            } else {
                thread::yield_now();
            }
        }

        t.join().unwrap();
        assert_eq!(received, vec![1, 2]);
    });
}

#[test]
fn no_data_race() {
    loom::model(|| {
        let (tx, rx) = spsc::ring_buffer::<u32>(2);

        let t = thread::spawn(move || {
            let _ = tx.push(42);
        });

        let val = loop {
            if let Some(v) = rx.pop() {
                break Some(v);
            }
            if rx.is_disconnected() {
                break rx.pop();
            }
            thread::yield_now();
        };

        t.join().unwrap();
        if let Some(v) = val {
            assert_eq!(v, 42);
        }
    });
}

#[test]
fn full_then_pop_then_push() {
    loom::model(|| {
        let (tx, rx) = spsc::ring_buffer::<u32>(2);

        tx.push(1).unwrap();
        tx.push(2).unwrap();
        assert!(tx.push(3).is_err());

        let t = thread::spawn(move || {
            let a = rx.pop();
            let b = rx.pop();
            (a, b)
        });

        let (a, b) = t.join().unwrap();
        assert_eq!(a, Some(1));
        assert_eq!(b, Some(2));
    });
}

#[test]
fn push_pop_wrap_around() {
    loom::model(|| {
        let (tx, rx) = spsc::ring_buffer::<u32>(2);

        let t = thread::spawn(move || {
            let mut received = Vec::new();
            while received.len() < 4 {
                if let Some(v) = rx.pop() {
                    received.push(v);
                } else {
                    thread::yield_now();
                }
            }
            received
        });

        for i in 1..=4u32 {
            while tx.push(i).is_err() {
                thread::yield_now();
            }
        }

        let received = t.join().unwrap();
        assert_eq!(received, vec![1, 2, 3, 4]);
    });
}

#[test]
fn drop_with_pending_items() {
    loom::model(|| {
        let (tx, rx) = spsc::ring_buffer::<Arc<u32>>(2);
        let val = Arc::new(42);

        tx.push(Arc::clone(&val)).unwrap();

        drop(tx);
        drop(rx);

        assert_eq!(Arc::strong_count(&val), 1);
    });
}
