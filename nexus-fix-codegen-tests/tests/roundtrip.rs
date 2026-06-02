use nexus_fix_codegen_tests::fix;

#[test]
fn decodes_scalar_fields_and_enum() {
    let msg = b"11=ORD123\x0154=1\x0155=BTC-USD\x0138=10\x01";
    let m = fix::messages::NewOrderSingle::decode(msg);
    assert_eq!(m.cl_ord_id(), Some(&b"ORD123"[..]));
    assert_eq!(m.symbol(), Some(&b"BTC-USD"[..]));
    assert_eq!(m.order_qty(), Some(&b"10"[..]));
    assert_eq!(m.side_enum(), Some(fix::fields::Side::BUY));
}

#[test]
fn absent_field_is_none() {
    let msg = b"11=ORD123\x01";
    let m = fix::messages::NewOrderSingle::decode(msg);
    assert_eq!(m.symbol(), None);
    assert_eq!(m.side_enum(), None);
}

#[test]
fn decodes_data_field_with_embedded_soh() {
    let msg = b"11=A\x0195=3\x0196=a\x01b\x0155=X\x01";
    let m = fix::messages::NewOrderSingle::decode(msg);
    assert_eq!(m.raw_data_length(), Some(&b"3"[..]));
    assert_eq!(m.raw_data(), Some(&b"a\x01b"[..]));
    assert_eq!(m.symbol(), Some(&b"X"[..]));
}

#[test]
fn decodes_repeating_group() {
    let msg = b"11=A\x01453=2\x01448=PARTY1\x01452=1\x01448=PARTY2\x01452=2\x0155=X\x01";
    let m = fix::messages::NewOrderSingle::decode(msg);
    let parties: Vec<_> = m.no_party_i_ds().collect();
    assert_eq!(parties.len(), 2);
    assert_eq!(parties[0].party_id(), Some(&b"PARTY1"[..]));
    assert_eq!(parties[1].party_id(), Some(&b"PARTY2"[..]));
    assert_eq!(m.symbol(), Some(&b"X"[..]));
}

#[test]
fn decodes_nested_group() {
    let msg = b"11=A\x01453=1\x01448=P1\x01452=1\x01802=2\x01523=S1\x01803=1\x01523=S2\x01803=2\x0155=X\x01";
    let m = fix::messages::NewOrderSingle::decode(msg);
    let parties: Vec<_> = m.no_party_i_ds().collect();
    assert_eq!(parties.len(), 1);
    assert_eq!(parties[0].party_id(), Some(&b"P1"[..]));
    let subs: Vec<_> = parties[0].no_party_sub_i_ds().collect();
    assert_eq!(subs.len(), 2);
    assert_eq!(subs[0].party_sub_id(), Some(&b"S1"[..]));
    assert_eq!(subs[1].party_sub_id(), Some(&b"S2"[..]));
    assert_eq!(m.symbol(), Some(&b"X"[..]));
}

#[test]
fn msgtype_dispatch() {
    assert_eq!(
        fix::MsgType::from_bytes(b"D"),
        Some(fix::MsgType::NewOrderSingle)
    );
    assert_eq!(fix::MsgType::NewOrderSingle.as_bytes(), b"D");
    assert_eq!(fix::MsgType::from_bytes(b"ZZ"), None);
}

#[test]
fn encodes_round_trip() {
    let mut buf = [0u8; 128];
    let n = fix::encoders::NewOrderSingleEncoder::new(&mut buf)
        .cl_ord_id(b"ORD1")
        .side_value(fix::fields::Side::SELL)
        .symbol(b"ETH-USD")
        .finish();
    let encoded = &buf[..n];
    let m = fix::messages::NewOrderSingle::decode(encoded);
    assert_eq!(m.cl_ord_id(), Some(&b"ORD1"[..]));
    assert_eq!(m.side_enum(), Some(fix::fields::Side::SELL));
    assert_eq!(m.symbol(), Some(&b"ETH-USD"[..]));
}

#[test]
fn encodes_data_field() {
    let mut buf = [0u8; 64];
    let n = fix::encoders::NewOrderSingleEncoder::new(&mut buf)
        .cl_ord_id(b"A")
        .raw_data(b"x\x01y")
        .finish();
    let m = fix::messages::NewOrderSingle::decode(&buf[..n]);
    assert_eq!(m.raw_data_length(), Some(&b"3"[..]));
    assert_eq!(m.raw_data(), Some(&b"x\x01y"[..]));
}
