use core::marker::PhantomData;

use crate::dict::FixDictionary;
use crate::field::FieldView;
use crate::reader::{FieldReader, RawField};
use crate::span::FieldSpan;
use crate::types::FixTimestamp;
use nexus_ascii::AsciiTextStr;

/// Decoded FIX message header, generic over the dictionary.
///
/// Scans the 8 standard header fields (tags 8, 9, 35, 34, 49, 56, 43, 52)
/// from a FIX message buffer. The first non-header tag is stored as
/// [`overflow`](Self::overflow) so the per-message `wrap` can continue
/// decoding without re-scanning.
///
/// Structurally identical across all FIX versions — only
/// [`msg_type_enum`](Self::msg_type_enum) dispatches through the
/// [`FixDictionary`] trait. This type lives in the codec rather than
/// being generated per dictionary.
pub struct HeaderDecoder<'buf, D: FixDictionary> {
    /// The underlying field reader. `pub` for generated `wrap` code that
    /// continues the scan into the message body.
    pub reader: FieldReader<'buf>,
    /// First non-header field encountered during header decode. Consumed
    /// by the generated `wrap` to avoid re-scanning.
    pub overflow: Option<RawField>,
    begin_string: FieldSpan,
    body_length: FieldSpan,
    msg_type: FieldSpan,
    msg_seq_num: FieldSpan,
    sender_comp_id: FieldSpan,
    target_comp_id: FieldSpan,
    poss_dup_flag: FieldSpan,
    sending_time: FieldSpan,
    _dict: PhantomData<D>,
}

impl<'buf, D: FixDictionary> HeaderDecoder<'buf, D> {
    /// Decode the standard header from `buf`.
    ///
    /// Scans fields until a non-header tag is encountered (stored as
    /// overflow) or the buffer is exhausted.
    pub fn decode(buf: &'buf [u8]) -> Self {
        let mut h = Self {
            reader: FieldReader::new(buf, 0),
            overflow: None,
            begin_string: FieldSpan::EMPTY,
            body_length: FieldSpan::EMPTY,
            msg_type: FieldSpan::EMPTY,
            msg_seq_num: FieldSpan::EMPTY,
            sender_comp_id: FieldSpan::EMPTY,
            target_comp_id: FieldSpan::EMPTY,
            poss_dup_flag: FieldSpan::EMPTY,
            sending_time: FieldSpan::EMPTY,
            _dict: PhantomData,
        };
        while let Some(f) = h.reader.next_field() {
            match f.tag {
                8 => h.begin_string = f.value,
                9 => h.body_length = f.value,
                35 => h.msg_type = f.value,
                34 => h.msg_seq_num = f.value,
                49 => h.sender_comp_id = f.value,
                56 => h.target_comp_id = f.value,
                43 => h.poss_dup_flag = f.value,
                52 => h.sending_time = f.value,
                _ => {
                    h.overflow = Some(f);
                    break;
                }
            }
        }
        h
    }

    /// The underlying message buffer.
    #[inline]
    pub fn buf(&self) -> &'buf [u8] {
        self.reader.buf()
    }

    pub fn begin_string(&self) -> Option<FieldView<'buf, &'buf [u8]>> {
        FieldView::new(self.begin_string, self.reader.buf())
    }

    pub fn body_length(&self) -> Option<FieldView<'buf, u32>> {
        FieldView::new(self.body_length, self.reader.buf())
    }

    pub fn msg_type(&self) -> Option<FieldView<'buf, &'buf [u8]>> {
        FieldView::new(self.msg_type, self.reader.buf())
    }

    pub fn msg_seq_num(&self) -> Option<FieldView<'buf, u64>> {
        FieldView::new(self.msg_seq_num, self.reader.buf())
    }

    pub fn sender_comp_id(&self) -> Option<FieldView<'buf, &'buf AsciiTextStr>> {
        FieldView::new(self.sender_comp_id, self.reader.buf())
    }

    pub fn target_comp_id(&self) -> Option<FieldView<'buf, &'buf AsciiTextStr>> {
        FieldView::new(self.target_comp_id, self.reader.buf())
    }

    pub fn poss_dup_flag(&self) -> Option<FieldView<'buf, bool>> {
        FieldView::new(self.poss_dup_flag, self.reader.buf())
    }

    pub fn sending_time(&self) -> Option<FieldView<'buf, FixTimestamp>> {
        FieldView::new(self.sending_time, self.reader.buf())
    }

    /// Parse the MsgType field through the dictionary's enum.
    pub fn msg_type_enum(&self) -> Option<D::MsgType> {
        if self.msg_type.is_present() {
            D::msg_type_from_bytes(self.msg_type.slice(self.reader.buf()))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestMsgType {
        NewOrderSingle,
        Heartbeat,
    }

    struct TestDict;

    impl FixDictionary for TestDict {
        type MsgType = TestMsgType;
        const BEGIN_STRING: &'static [u8] = b"FIX.4.4";

        fn msg_type_from_bytes(bytes: &[u8]) -> Option<TestMsgType> {
            match bytes {
                b"D" => Some(TestMsgType::NewOrderSingle),
                b"0" => Some(TestMsgType::Heartbeat),
                _ => None,
            }
        }

        fn is_admin(msg_type: TestMsgType) -> bool {
            matches!(msg_type, TestMsgType::Heartbeat)
        }
    }

    type Header<'a> = HeaderDecoder<'a, TestDict>;

    #[test]
    fn decodes_all_header_fields() {
        let msg = b"8=FIX.4.4\x019=99\x0135=D\x0134=42\x0149=SENDER\x0156=TARGET\x0143=Y\x0152=20260603-14:30:00\x0111=X\x01";
        let h = Header::decode(msg);
        assert_eq!(h.begin_string().unwrap().as_bytes(), b"FIX.4.4");
        assert_eq!(h.body_length().unwrap().get(), 99);
        assert_eq!(h.msg_type().unwrap().as_bytes(), b"D");
        assert_eq!(h.msg_seq_num().unwrap().get(), 42);
        assert_eq!(h.sender_comp_id().unwrap().get().as_bytes(), b"SENDER");
        assert_eq!(h.target_comp_id().unwrap().get().as_bytes(), b"TARGET");
        assert!(h.poss_dup_flag().unwrap().get());
        assert!(h.sending_time().unwrap().is_valid());
    }

    #[test]
    fn msg_type_enum_dispatches() {
        let msg = b"35=D\x0111=X\x01";
        let h = Header::decode(msg);
        assert_eq!(h.msg_type_enum(), Some(TestMsgType::NewOrderSingle));

        let msg2 = b"35=0\x01";
        let h2 = Header::decode(msg2);
        assert_eq!(h2.msg_type_enum(), Some(TestMsgType::Heartbeat));
        assert!(TestDict::is_admin(h2.msg_type_enum().unwrap()));
    }

    #[test]
    fn unknown_msg_type_is_none() {
        let msg = b"35=ZZ\x01";
        let h = Header::decode(msg);
        assert!(h.msg_type_enum().is_none());
    }

    #[test]
    fn absent_fields_are_none() {
        let h = Header::decode(b"11=X\x01");
        assert!(h.begin_string().is_none());
        assert!(h.msg_type().is_none());
        assert!(h.msg_type_enum().is_none());
        assert!(h.msg_seq_num().is_none());
        assert!(h.poss_dup_flag().is_none());
    }

    #[test]
    fn overflow_captures_first_body_field() {
        let msg = b"8=FIX.4.4\x0135=D\x0111=ORD1\x0155=BTC\x01";
        let h = Header::decode(msg);
        let of = h.overflow.as_ref().unwrap();
        assert_eq!(of.tag, 11);
        assert_eq!(of.value.slice(msg), b"ORD1");
    }

    #[test]
    fn empty_buffer() {
        let h = Header::decode(b"");
        assert!(h.begin_string().is_none());
        assert!(h.overflow.is_none());
    }
}
