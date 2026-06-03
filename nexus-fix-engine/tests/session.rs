use std::time::{Duration, Instant};

use nexus_fix_codec::{find_tag, parse_fix_uint, validate_checksum};
use nexus_fix_engine::{DisconnectReason, Event, Session, SessionConfig, State};

const HB: Duration = Duration::from_secs(30);

fn config() -> SessionConfig<'static> {
    SessionConfig {
        begin_string: b"FIX.4.4",
        sender_comp_id: b"US",
        target_comp_id: b"THEM",
        heart_bt_int: HB,
    }
}

fn inbound(msg_type: &[u8], seq: u32, extra: &[(u32, &[u8])]) -> Vec<u8> {
    let mut m = format!(
        "35={}\x0149=THEM\x0156=US\x0134={seq}\x01",
        String::from_utf8_lossy(msg_type)
    )
    .into_bytes();
    for (tag, value) in extra {
        m.extend_from_slice(format!("{tag}=").as_bytes());
        m.extend_from_slice(value);
        m.push(1);
    }
    m.extend_from_slice(b"10=000\x01");
    m
}

fn drain(s: &mut Session) -> Vec<Event> {
    std::iter::from_fn(|| s.poll_event()).collect()
}

fn flush(s: &mut Session, now: Instant) -> Vec<Vec<u8>> {
    let mut buf = [0u8; 256];
    let mut out = Vec::new();
    while let Some(n) = s.encode_pending(&mut buf, now, 1_780_505_733_000_000_000) {
        out.push(buf[..n].to_vec());
    }
    out
}

fn field(msg: &[u8], tag: u32) -> &[u8] {
    find_tag(msg, 0, tag).map_or(b"", |span| span.slice(msg))
}

fn establish(s: &mut Session, now: Instant) {
    s.connect(now);
    flush(s, now);
    s.handle_message(&inbound(b"A", 1, &[(98, b"0"), (108, b"30")]), now);
    assert_eq!(s.state(), State::Active);
    drain(s);
}

#[test]
fn initiator_handshake() {
    let mut s = Session::new(config());
    let now = Instant::now();
    s.connect(now);
    assert_eq!(s.state(), State::LogonSent);

    let sent = flush(&mut s, now);
    assert_eq!(sent.len(), 1);
    let logon = &sent[0];
    validate_checksum(logon).unwrap();
    assert_eq!(field(logon, 8), b"FIX.4.4");
    assert_eq!(field(logon, 35), b"A");
    assert_eq!(field(logon, 49), b"US");
    assert_eq!(field(logon, 56), b"THEM");
    assert_eq!(field(logon, 34), b"1");
    assert_eq!(field(logon, 108), b"30");
    assert_eq!(field(logon, 52).len(), 21);

    s.handle_message(&inbound(b"A", 1, &[(98, b"0"), (108, b"30")]), now);
    assert_eq!(s.state(), State::Active);
    assert_eq!(
        drain(&mut s),
        vec![Event::Established { heart_bt_int_s: 30 }]
    );
    assert_eq!(s.next_inbound_seq(), 2);
    assert_eq!(s.next_outbound_seq(), 2);
}

#[test]
fn acceptor_handshake() {
    let mut s = Session::new(config());
    let now = Instant::now();
    s.handle_message(&inbound(b"A", 1, &[(98, b"0"), (108, b"15")]), now);
    assert_eq!(s.state(), State::Active);
    assert_eq!(
        drain(&mut s),
        vec![Event::Established { heart_bt_int_s: 15 }]
    );

    let sent = flush(&mut s, now);
    assert_eq!(sent.len(), 1);
    assert_eq!(field(&sent[0], 35), b"A");
    assert_eq!(field(&sent[0], 108), b"15");
}

#[test]
fn logon_reset_seq_num_flag() {
    let mut s = Session::new(config());
    let now = Instant::now();
    s.handle_message(&inbound(b"A", 1, &[(108, b"30"), (141, b"Y")]), now);
    assert_eq!(s.state(), State::Active);
    let sent = flush(&mut s, now);
    assert_eq!(field(&sent[0], 34), b"1");
}

#[test]
fn body_length_is_exact() {
    let mut s = Session::new(config());
    let now = Instant::now();
    s.connect(now);
    let sent = flush(&mut s, now);
    let msg = &sent[0];
    let blen = parse_fix_uint(field(msg, 9)).unwrap() as usize;
    let body_start = find_tag(msg, 0, 35).unwrap().offset as usize - 3;
    let trailer = msg.len() - 7;
    assert_eq!(blen, trailer - body_start);
}

#[test]
fn app_message_emits_event() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    s.handle_message(&inbound(b"D", 2, &[(11, b"ORD1")]), now);
    assert_eq!(
        drain(&mut s),
        vec![Event::App {
            seq_num: 2,
            poss_dup: false
        }]
    );
    assert_eq!(s.next_inbound_seq(), 3);
}

#[test]
fn test_request_is_echoed() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    s.handle_message(&inbound(b"1", 2, &[(112, b"PROBE7")]), now);
    let sent = flush(&mut s, now);
    assert_eq!(sent.len(), 1);
    assert_eq!(field(&sent[0], 35), b"0");
    assert_eq!(field(&sent[0], 112), b"PROBE7");
}

#[test]
fn heartbeat_fires_on_outbound_idle() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    flush(&mut s, now);

    s.handle_timeout(now + Duration::from_secs(29));
    assert!(!s.has_pending());
    s.handle_timeout(now + Duration::from_secs(30));
    let sent = flush(&mut s, now + Duration::from_secs(30));
    assert_eq!(sent.len(), 1);
    assert_eq!(field(&sent[0], 35), b"0");
    assert!(field(&sent[0], 112).is_empty());
}

#[test]
fn heartbeat_not_queued_twice() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    s.handle_timeout(now + Duration::from_secs(31));
    s.handle_timeout(now + Duration::from_secs(32));
    let sent = flush(&mut s, now + Duration::from_secs(32));
    assert_eq!(sent.len(), 1);
}

#[test]
fn inbound_silence_probes_then_disconnects() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);

    let probe_at = now + Duration::from_secs(36);
    s.handle_timeout(probe_at);
    let sent = flush(&mut s, probe_at);
    assert!(sent.iter().any(|m| field(m, 35) == b"1"));

    s.handle_timeout(probe_at + HB);
    assert_eq!(s.state(), State::Disconnected);
    assert!(drain(&mut s).contains(&Event::Disconnected {
        reason: DisconnectReason::TestRequestTimeout
    }));
    let sent = flush(&mut s, probe_at + HB);
    assert!(sent.iter().any(|m| field(m, 35) == b"5"));
}

#[test]
fn probe_answered_keeps_session_alive() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);

    let probe_at = now + Duration::from_secs(36);
    s.handle_timeout(probe_at);
    flush(&mut s, probe_at);
    s.handle_message(
        &inbound(b"0", 2, &[(112, b"1")]),
        probe_at + Duration::from_secs(1),
    );
    s.handle_timeout(probe_at + HB);
    assert_eq!(s.state(), State::Active);
}

#[test]
fn gap_triggers_resend_request() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);

    s.handle_message(&inbound(b"D", 5, &[(11, b"ORD5")]), now);
    assert_eq!(s.state(), State::Resending);
    assert!(drain(&mut s).is_empty());

    let sent = flush(&mut s, now);
    assert_eq!(sent.len(), 1);
    assert_eq!(field(&sent[0], 35), b"2");
    assert_eq!(field(&sent[0], 7), b"2");
    assert_eq!(field(&sent[0], 16), b"0");

    for seq in 2..=5 {
        s.handle_message(&inbound(b"D", seq, &[(43, b"Y"), (11, b"X")]), now);
    }
    assert_eq!(s.state(), State::Active);
    assert_eq!(s.next_inbound_seq(), 6);
    let events = drain(&mut s);
    assert_eq!(events.len(), 4);
    assert!(
        events
            .iter()
            .all(|e| matches!(e, Event::App { poss_dup: true, .. }))
    );
}

#[test]
fn gap_fill_advances_past_admin_messages() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);

    s.handle_message(&inbound(b"D", 6, &[(11, b"ORD6")]), now);
    assert_eq!(s.state(), State::Resending);
    flush(&mut s, now);

    s.handle_message(
        &inbound(b"4", 2, &[(43, b"Y"), (123, b"Y"), (36, b"7")]),
        now,
    );
    assert_eq!(s.next_inbound_seq(), 7);
    assert_eq!(s.state(), State::Active);
    assert!(drain(&mut s).contains(&Event::SequenceReset { new_seq: 7 }));
}

#[test]
fn sequence_reset_reset_mode_ignores_seq() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    s.handle_message(&inbound(b"4", 999, &[(36, b"50")]), now);
    assert_eq!(s.next_inbound_seq(), 50);
    assert!(drain(&mut s).contains(&Event::SequenceReset { new_seq: 50 }));
}

#[test]
fn resend_request_is_gap_filled() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    s.allocate_seq(now);
    s.allocate_seq(now);

    s.handle_message(&inbound(b"2", 2, &[(7, b"2"), (16, b"3")]), now);
    assert!(drain(&mut s).contains(&Event::ResendRange { begin: 2, end: 3 }));
    let sent = flush(&mut s, now);
    assert_eq!(sent.len(), 1);
    let gap_fill = &sent[0];
    assert_eq!(field(gap_fill, 35), b"4");
    assert_eq!(field(gap_fill, 34), b"2");
    assert_eq!(field(gap_fill, 43), b"Y");
    assert_eq!(field(gap_fill, 123), b"Y");
    assert_eq!(field(gap_fill, 36), b"4");
}

#[test]
fn seq_too_low_disconnects() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    s.handle_message(&inbound(b"D", 2, &[]), now);
    drain(&mut s);
    s.handle_message(&inbound(b"D", 2, &[]), now);
    assert_eq!(s.state(), State::Disconnected);
    assert!(drain(&mut s).contains(&Event::Disconnected {
        reason: DisconnectReason::SeqNumTooLow
    }));
}

#[test]
fn poss_dup_below_expected_is_ignored() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    s.handle_message(&inbound(b"D", 2, &[]), now);
    drain(&mut s);
    s.handle_message(&inbound(b"D", 2, &[(43, b"Y")]), now);
    assert_eq!(s.state(), State::Active);
    assert!(drain(&mut s).is_empty());
}

#[test]
fn comp_id_mismatch_disconnects() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    let msg = b"35=D\x0149=EVIL\x0156=US\x0134=2\x0110=000\x01";
    s.handle_message(msg, now);
    assert_eq!(s.state(), State::Disconnected);
    assert!(drain(&mut s).contains(&Event::Disconnected {
        reason: DisconnectReason::CompIdMismatch
    }));
}

#[test]
fn initiated_logout_round_trip() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);

    s.logout(now);
    assert_eq!(s.state(), State::LogoutPending);
    let sent = flush(&mut s, now);
    assert_eq!(field(&sent[0], 35), b"5");

    s.handle_message(&inbound(b"5", 2, &[]), now);
    assert_eq!(s.state(), State::Disconnected);
    assert!(drain(&mut s).contains(&Event::Disconnected {
        reason: DisconnectReason::Logout
    }));
}

#[test]
fn counterparty_logout_is_confirmed() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    s.handle_message(&inbound(b"5", 2, &[]), now);
    assert_eq!(s.state(), State::Disconnected);
    let sent = flush(&mut s, now);
    assert_eq!(sent.len(), 1);
    assert_eq!(field(&sent[0], 35), b"5");
}

#[test]
fn logout_timeout_disconnects() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    s.logout(now);
    s.handle_timeout(now + HB);
    assert_eq!(s.state(), State::Disconnected);
    assert!(drain(&mut s).contains(&Event::Disconnected {
        reason: DisconnectReason::LogoutTimeout
    }));
}

#[test]
fn logon_timeout_disconnects() {
    let mut s = Session::new(config());
    let now = Instant::now();
    s.connect(now);
    flush(&mut s, now);
    s.handle_timeout(now + HB);
    assert_eq!(s.state(), State::Disconnected);
    assert!(drain(&mut s).contains(&Event::Disconnected {
        reason: DisconnectReason::LogonTimeout
    }));
}

#[test]
fn reject_received_surfaces_event() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    s.handle_message(&inbound(b"3", 2, &[(45, b"7")]), now);
    assert!(drain(&mut s).contains(&Event::RejectReceived { ref_seq_num: 7 }));
}

#[test]
fn seq_nums_survive_reconnect() {
    let mut s = Session::new(config());
    let now = Instant::now();
    establish(&mut s, now);
    s.allocate_seq(now);
    s.handle_message(&inbound(b"5", 2, &[]), now);
    flush(&mut s, now);
    drain(&mut s);
    assert_eq!(s.state(), State::Disconnected);

    s.connect(now);
    let sent = flush(&mut s, now);
    assert_eq!(field(&sent[0], 34), b"4");
    s.handle_message(&inbound(b"A", 3, &[(108, b"30")]), now);
    assert_eq!(s.state(), State::Active);
}

#[test]
fn next_timeout_tracks_deadlines() {
    let mut s = Session::new(config());
    assert!(s.next_timeout().is_none());
    let now = Instant::now();
    s.connect(now);
    assert_eq!(s.next_timeout(), Some(now + HB));
    flush(&mut s, now);
    s.handle_message(&inbound(b"A", 1, &[(108, b"30")]), now);
    assert_eq!(s.next_timeout(), Some(now + HB));
}

#[test]
fn messages_ignored_while_disconnected() {
    let mut s = Session::new(config());
    let now = Instant::now();
    s.handle_message(&inbound(b"D", 1, &[]), now);
    assert_eq!(s.state(), State::Disconnected);
    assert!(drain(&mut s).is_empty());
    assert!(!s.has_pending());
}
