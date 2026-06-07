use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::region::MapOptions;

use super::frame::footprint;
use super::{SegmentedLog, slot_path};

fn base(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("nexus-seglog-{}-{}", std::process::id(), name))
}

fn cleanup(base: &Path) {
    for i in 0..3u8 {
        let _ = std::fs::remove_file(slot_path(base, i));
    }
}

fn open(base: &Path, size: usize) -> SegmentedLog {
    SegmentedLog::open(base, size, MapOptions::default()).unwrap()
}

fn wait_conductor(log: &SegmentedLog) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !log.conductor.ready.load(Ordering::Acquire) {
        assert!(std::time::Instant::now() < deadline, "conductor timed out");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn roundtrip() {
    let b = base("rt");
    cleanup(&b);
    let mut log = open(&b, 1 << 16);
    let off = log.append(0, b"hello").unwrap();
    let frame = log.read(off).unwrap();
    assert_eq!(frame.payload(), b"hello");
    assert_eq!(frame.session_id(), 0);
    cleanup(&b);
}

#[test]
fn multiple_records_in_one_segment() {
    let b = base("multi");
    cleanup(&b);
    let mut log = open(&b, 1 << 16);
    let o1 = log.append(0, b"aaa").unwrap();
    let o2 = log.append(0, b"bb").unwrap();
    let o3 = log.append(0, b"cccc").unwrap();
    assert_eq!(log.read(o1).unwrap().payload(), b"aaa");
    assert_eq!(log.read(o2).unwrap().payload(), b"bb");
    assert_eq!(log.read(o3).unwrap().payload(), b"cccc");
    cleanup(&b);
}

#[test]
fn rotation_makes_prev_slot_readable() {
    let b = base("rot");
    cleanup(&b);
    // footprint(8) = 16; 4 records = 64 bytes; segment_size = 64
    let mut log = open(&b, 64);
    let o0 = log.append(0, &[0u8; 8]).unwrap();
    let _o1 = log.append(0, &[1u8; 8]).unwrap();
    let _o2 = log.append(0, &[2u8; 8]).unwrap();
    let o3 = log.append(0, &[3u8; 8]).unwrap();
    // cursor now == 64 == segment_size -> next append triggers rotation
    let o4 = log.append(0, &[4u8; 8]).unwrap();
    // slot 0 (prev) still readable
    assert_eq!(log.read(o0).unwrap().payload(), &[0u8; 8]);
    assert_eq!(log.read(o3).unwrap().payload(), &[3u8; 8]);
    // slot 1 (current) readable
    assert_eq!(log.read(o4).unwrap().payload(), &[4u8; 8]);
    cleanup(&b);
}

#[test]
fn evicted_slot_returns_none() {
    let b = base("evict");
    cleanup(&b);
    // footprint(8) = 16; 4 records per segment; segment_size = 64
    let mut log = open(&b, 64);
    let o0 = log.append(0, &[0u8; 8]).unwrap();
    // fill slot 0
    for _ in 0..3 {
        log.append(0, &[0u8; 8]).unwrap();
    }
    // rotation 1 triggered by the first of the next 4 appends:
    //   slot 0 -> prev, slot 1 -> current, slot 2 -> standby (conductor cleaning slot 2)
    for _ in 0..4 {
        log.append(0, &[0u8; 8]).unwrap();
    }
    // wait for conductor to finish cleaning slot 2 before triggering rotation 2
    wait_conductor(&log);
    // rotation 2: slot 1 -> prev, slot 2 -> current, slot 0 -> standby
    log.append(0, &[0u8; 1]).unwrap();
    // slot 0 is standby -> no longer readable
    assert!(log.read(o0).is_none());
    cleanup(&b);
}

#[test]
fn stale_offset_after_full_cycle_returns_none() {
    let b = base("gen");
    cleanup(&b);
    // footprint(8) = 16; 4 records per segment; segment_size = 64
    let mut log = open(&b, 64);
    // Write to slot 0 (gen 0)
    let stale = log.append(0, &[0u8; 8]).unwrap();
    for _ in 0..3 {
        log.append(0, &[0u8; 8]).unwrap();
    }
    // Rotation 1: slot 1 -> current (gen 1), slot 0 -> prev
    for _ in 0..4 {
        log.append(0, &[0u8; 8]).unwrap();
    }
    wait_conductor(&log);
    // Rotation 2: slot 2 -> current (gen 2), slot 1 -> prev, slot 0 -> standby
    for _ in 0..4 {
        log.append(0, &[0u8; 8]).unwrap();
    }
    wait_conductor(&log);
    // Rotation 3: slot 0 -> current (gen 3), slot 2 -> prev, slot 1 -> standby.
    // Slot 0 is current again -- same slot index as `stale` -- but gen 3 != gen 0.
    log.append(0, &[0u8; 8]).unwrap();
    assert!(log.read(stale).is_none());
    cleanup(&b);
}

#[test]
fn record_too_large_rejected() {
    let b = base("large");
    cleanup(&b);
    let mut log = open(&b, 64);
    assert!(log.append(0, &[0u8; 1024]).is_err());
    cleanup(&b);
}

#[test]
fn empty_payload_roundtrip() {
    // footprint(0) = 8, which is FRAME_HDR -- valid
    let b = base("empty");
    cleanup(&b);
    let mut log = open(&b, 1 << 16);
    let off = log.append(0, &[]).unwrap();
    assert_eq!(log.read(off).unwrap().payload(), b"");
    cleanup(&b);
}

// -- session_id tests --

#[test]
fn session_id_roundtrip() {
    let b = base("sessrt");
    cleanup(&b);
    let mut log = open(&b, 1 << 16);
    let o1 = log.append(42, b"hello").unwrap();
    let o2 = log.append(99, b"world").unwrap();
    let f1 = log.read(o1).unwrap();
    let f2 = log.read(o2).unwrap();
    assert_eq!(f1.session_id(), 42);
    assert_eq!(f1.payload(), b"hello");
    assert_eq!(f2.session_id(), 99);
    assert_eq!(f2.payload(), b"world");
    cleanup(&b);
}

#[test]
fn session_id_survives_rotation() {
    let b = base("sessrot");
    cleanup(&b);
    // footprint(8) = 16; 4 records per segment; segment_size = 64
    let mut log = open(&b, 64);
    let o0 = log.append(10, &[0u8; 8]).unwrap();
    for _ in 0..3 {
        log.append(0, &[0u8; 8]).unwrap();
    }
    // triggers rotation
    log.append(20, &[1u8; 8]).unwrap();
    // o0 is in prev segment, still readable
    let f = log.read(o0).unwrap();
    assert_eq!(f.session_id(), 10);
    cleanup(&b);
}

#[test]
fn scan_returns_session_id() {
    let b = base("scansess");
    cleanup(&b);
    let mut log = open(&b, 1 << 16);
    log.append(1, b"aaa").unwrap();
    log.append(2, b"bbb").unwrap();
    log.append(1, b"ccc").unwrap();

    let mut pos = log.read_start();
    let f1 = log.read_next(&mut pos).unwrap();
    assert_eq!(f1.session_id(), 1);
    assert_eq!(f1.payload(), b"aaa");
    let f2 = log.read_next(&mut pos).unwrap();
    assert_eq!(f2.session_id(), 2);
    assert_eq!(f2.payload(), b"bbb");
    let f3 = log.read_next(&mut pos).unwrap();
    assert_eq!(f3.session_id(), 1);
    assert_eq!(f3.payload(), b"ccc");
    assert!(log.read_next(&mut pos).is_none());
    cleanup(&b);
}

// -- sequential scan tests --

#[test]
fn scan_single_segment() {
    let b = base("scan1");
    cleanup(&b);
    let mut log = open(&b, 1 << 16);
    log.append(0, b"aaa").unwrap();
    log.append(0, b"bb").unwrap();
    log.append(0, b"cccc").unwrap();

    let mut pos = log.read_start();
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), b"aaa");
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), b"bb");
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), b"cccc");
    assert!(log.read_next(&mut pos).is_none());
    cleanup(&b);
}

#[test]
fn scan_across_rotation() {
    let b = base("scanrot");
    cleanup(&b);
    // footprint(8) = 16; 4 records per segment; segment_size = 64
    let mut log = open(&b, 64);
    log.append(0, &[1u8; 8]).unwrap();
    log.append(0, &[2u8; 8]).unwrap();
    log.append(0, &[3u8; 8]).unwrap();
    log.append(0, &[4u8; 8]).unwrap();
    // segment full, next append triggers rotation
    log.append(0, &[5u8; 8]).unwrap();
    log.append(0, &[6u8; 8]).unwrap();

    let mut pos = log.read_start();
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), &[1u8; 8]);
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), &[2u8; 8]);
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), &[3u8; 8]);
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), &[4u8; 8]);
    // crosses into current segment
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), &[5u8; 8]);
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), &[6u8; 8]);
    assert!(log.read_next(&mut pos).is_none());
    cleanup(&b);
}

#[test]
fn scan_resumes_after_append() {
    let b = base("scanresume");
    cleanup(&b);
    let mut log = open(&b, 1 << 16);
    log.append(0, b"first").unwrap();

    let mut pos = log.read_start();
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), b"first");
    assert!(log.read_next(&mut pos).is_none());

    log.append(0, b"second").unwrap();
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), b"second");
    assert!(log.read_next(&mut pos).is_none());
    cleanup(&b);
}

#[test]
fn scan_evicted_returns_none() {
    let b = base("scanevict");
    cleanup(&b);
    // footprint(8) = 16; 4 records per segment; segment_size = 64
    let mut log = open(&b, 64);
    let start = log.read_start();
    // fill segment 0
    for _ in 0..4 {
        log.append(0, &[0u8; 8]).unwrap();
    }
    // rotation 1
    for _ in 0..4 {
        log.append(0, &[0u8; 8]).unwrap();
    }
    wait_conductor(&log);
    // rotation 2 -- segment 0 is now standby, evicted
    log.append(0, &[0u8; 8]).unwrap();

    let mut pos = start;
    assert!(log.read_next(&mut pos).is_none());
    cleanup(&b);
}

#[test]
fn write_pos_increases_monotonically() {
    let b = base("wpos");
    cleanup(&b);
    // Large segment so no rotation needed for this test.
    let mut log = open(&b, 1 << 16);
    let mut prev_pos = log.write_pos();
    assert_eq!(prev_pos, 0);
    for _ in 0..12 {
        log.append(0, &[0u8; 8]).unwrap();
        let wp = log.write_pos();
        assert!(wp > prev_pos, "write_pos must increase: {wp} <= {prev_pos}");
        prev_pos = wp;
    }
    cleanup(&b);
}

#[test]
fn write_pos_increases_across_rotation() {
    let b = base("wposrot");
    cleanup(&b);
    // footprint(8) = 16; 4 records per segment
    let mut log = open(&b, 64);
    let mut prev_pos = 0u64;
    for i in 0..4 {
        log.append(0, &[i as u8; 8]).unwrap();
        let wp = log.write_pos();
        assert!(wp > prev_pos, "write_pos must increase: {wp} <= {prev_pos}");
        prev_pos = wp;
    }
    // triggers rotation
    log.append(0, &[4u8; 8]).unwrap();
    let wp = log.write_pos();
    assert!(
        wp > prev_pos,
        "write_pos must increase after rotation: {wp} <= {prev_pos}"
    );
    cleanup(&b);
}

#[test]
fn slot_order_is_sequential() {
    let b = base("slotord");
    cleanup(&b);
    // footprint(8) = 16; 4 records per segment; segment_size = 64
    let mut log = open(&b, 64);
    assert_eq!(log.current, 0);
    // fill and rotate
    for _ in 0..4 {
        log.append(0, &[0u8; 8]).unwrap();
    }
    log.append(0, &[0u8; 8]).unwrap();
    assert_eq!(log.current, 1);
    // fill and rotate again
    for _ in 0..3 {
        log.append(0, &[0u8; 8]).unwrap();
    }
    wait_conductor(&log);
    log.append(0, &[0u8; 8]).unwrap();
    assert_eq!(log.current, 2);
    cleanup(&b);
}

#[test]
fn scan_empty_log() {
    let b = base("scanempty");
    cleanup(&b);
    let log = open(&b, 1 << 16);
    let mut pos = log.read_start();
    assert_eq!(pos, 0);
    assert!(log.read_next(&mut pos).is_none());
    assert_eq!(log.write_pos(), 0);
    cleanup(&b);
}

#[test]
fn scan_empty_payloads() {
    let b = base("scanemptypay");
    cleanup(&b);
    let mut log = open(&b, 1 << 16);
    log.append(0, &[]).unwrap();
    log.append(0, &[]).unwrap();
    log.append(0, b"x").unwrap();

    let mut pos = log.read_start();
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), b"");
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), b"");
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), b"x");
    assert!(log.read_next(&mut pos).is_none());
    cleanup(&b);
}

#[test]
fn scan_variable_size_records() {
    let b = base("scanvar");
    cleanup(&b);
    let mut log = open(&b, 1 << 16);
    log.append(0, b"a").unwrap();
    log.append(0, b"bb").unwrap();
    log.append(0, b"ccccccccc").unwrap(); // 9 bytes, aligns up to 16
    log.append(0, b"dd").unwrap();

    let mut pos = log.read_start();
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), b"a");
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), b"bb");
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), b"ccccccccc");
    assert_eq!(log.read_next(&mut pos).unwrap().payload(), b"dd");
    assert!(log.read_next(&mut pos).is_none());
    cleanup(&b);
}

#[test]
fn scan_cursor_matches_write_pos_after_drain() {
    let b = base("scandrain");
    cleanup(&b);
    let mut log = open(&b, 64);
    log.append(0, &[1u8; 8]).unwrap();
    log.append(0, &[2u8; 8]).unwrap();
    log.append(0, &[3u8; 8]).unwrap();

    let mut pos = log.read_start();
    while log.read_next(&mut pos).is_some() {}
    assert_eq!(pos, log.write_pos());

    // also holds after rotation
    log.append(0, &[4u8; 8]).unwrap(); // fills segment
    log.append(0, &[5u8; 8]).unwrap(); // triggers rotation
    while log.read_next(&mut pos).is_some() {}
    assert_eq!(pos, log.write_pos());
    cleanup(&b);
}

#[test]
fn read_start_advances_after_rotation() {
    let b = base("readstart");
    cleanup(&b);
    // footprint(8) = 16; 4 records per segment; segment_size = 64
    let mut log = open(&b, 64);
    assert_eq!(log.read_start(), 0);

    // fill segment 0, trigger rotation
    for _ in 0..4 {
        log.append(0, &[0u8; 8]).unwrap();
    }
    log.append(0, &[0u8; 8]).unwrap();
    // epoch 1: prev = segment 0, current = segment 1
    assert_eq!(log.read_start(), 0);

    // fill segment 1, trigger rotation
    for _ in 0..3 {
        log.append(0, &[0u8; 8]).unwrap();
    }
    wait_conductor(&log);
    log.append(0, &[0u8; 8]).unwrap();
    // epoch 2: prev = segment 1, current = segment 2
    // segment 0 is evicted; read_start should be at segment 1
    assert_eq!(log.read_start(), 64);
    cleanup(&b);
}

#[test]
fn frame_offset_matches_global_position() {
    let b = base("frmoff");
    cleanup(&b);
    let mut log = open(&b, 1 << 16);
    log.append(0, b"aaa").unwrap();
    log.append(0, b"bbb").unwrap();

    let mut pos = log.read_start();
    let f1 = log.read_next(&mut pos).unwrap();
    assert_eq!(f1.offset(), 0);
    let f2 = log.read_next(&mut pos).unwrap();
    assert_eq!(f2.offset(), footprint(3) as u64);
    cleanup(&b);
}
