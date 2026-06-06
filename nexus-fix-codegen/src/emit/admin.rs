use std::collections::HashSet;
use std::fmt::Write;

use super::{HEADER, RField, RMessage, emit_value_accessor};

/// (tag, snake_field_name, rust_return_type)
type ProtocolField = (u32, &'static str, &'static str);

/// (struct_name, msgtype_byte, protocol_required_fields)
const SPECS: &[(&str, &str, &[ProtocolField])] = &[
    (
        "Logon",
        "A",
        &[
            (108, "heart_bt_int", "u32"),
            (98, "encrypt_method", "u32"),
            (141, "reset_seq_num_flag", "bool"),
        ],
    ),
    (
        "Logout",
        "5",
        &[(58, "text", "&'buf nexus_fix_codec::AsciiTextStr")],
    ),
    (
        "Heartbeat",
        "0",
        &[(112, "test_req_id", "&'buf nexus_fix_codec::AsciiTextStr")],
    ),
    (
        "TestRequest",
        "1",
        &[(112, "test_req_id", "&'buf nexus_fix_codec::AsciiTextStr")],
    ),
    (
        "ResendRequest",
        "2",
        &[(7, "begin_seq_no", "u64"), (16, "end_seq_no", "u64")],
    ),
    (
        "SequenceReset",
        "4",
        &[(36, "new_seq_no", "u64"), (123, "gap_fill_flag", "bool")],
    ),
    (
        "Reject",
        "3",
        &[
            (45, "ref_seq_num", "u64"),
            (58, "text", "&'buf nexus_fix_codec::AsciiTextStr"),
        ],
    ),
];

pub fn emit(messages: &[RMessage]) -> String {
    let mut s = String::new();
    s.push_str(HEADER);
    for &(name, msgtype, proto_fields) in SPECS {
        emit_admin_type(&mut s, name, msgtype, proto_fields, messages);
    }
    s
}

fn emit_admin_type(
    s: &mut String,
    name: &str,
    msgtype: &str,
    proto_fields: &[ProtocolField],
    messages: &[RMessage],
) {
    let _ = writeln!(s, "pub struct {name}<'buf> {{");
    s.push_str("    buf: &'buf [u8],\n}\n\n");

    // FixAdminMsg impl
    let _ = writeln!(
        s,
        "impl<'buf> nexus_fix_codec::FixAdminMsg<'buf> for {name}<'buf> {{"
    );
    s.push_str("    fn decode(buf: &'buf [u8]) -> Self {\n        Self { buf }\n    }\n}\n\n");

    // Protocol-required accessors
    let _ = writeln!(s, "impl<'buf> {name}<'buf> {{");
    let proto_tags: HashSet<u32> = proto_fields.iter().map(|&(tag, _, _)| tag).collect();
    for &(tag, field_name, rust_type) in proto_fields {
        let _ = write!(
            s,
            "    pub fn {field_name}(&self) -> Option<nexus_fix_codec::FieldView<'buf, {rust_type}>> {{\n        \
             nexus_fix_codec::find_tag(self.buf, 0, {tag})\n            \
             .and_then(|s| nexus_fix_codec::FieldView::new(s, self.buf))\n    \
             }}\n\n"
        );
    }

    // Venue-specific fields from the XML (non-group, not already in proto set)
    if let Some(msg) = messages.iter().find(|m| m.is_admin && m.msgtype == msgtype) {
        let extra: Vec<&RField> = msg
            .members
            .iter()
            .filter_map(|mem| match mem {
                super::RMember::Field(f) if !proto_tags.contains(&f.number) => Some(f),
                _ => None,
            })
            .collect();
        for f in extra {
            emit_value_accessor(s, f, "self.buf");
        }
    }

    s.push_str("}\n\n");
}

/// The 7 associated type assignments for the `FixDictionary` impl.
pub fn emit_dict_assoc_types(s: &mut String) {
    for &(name, _, _) in SPECS {
        let _ = writeln!(s, "    type {name}<'buf> = admin::{name}<'buf>;");
    }
}
