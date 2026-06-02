use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::dict::FieldType;

use super::{
    HEADER, RField, RGroup, RMember, RMessage, group_type, pascal, screaming, snake, subtree_tags,
};

enum Top<'a> {
    Field(&'a RField),
    Group(&'a RGroup),
}

pub fn emit(messages: &[RMessage]) -> String {
    let mut s = String::new();
    s.push_str(HEADER);
    for m in messages {
        emit_message(&mut s, m);
    }
    s
}

fn emit_message(s: &mut String, m: &RMessage) {
    let ty = pascal(&m.name);
    let tops: Vec<Top> = m
        .members
        .iter()
        .map(|mem| match mem {
            RMember::Field(f) => Top::Field(f),
            RMember::Group(g) => Top::Group(g),
        })
        .collect();

    let mut data_handled: HashSet<u32> = HashSet::new();
    let mut data_after: HashMap<u32, &RField> = HashMap::new();
    for w in tops.windows(2) {
        if let [Top::Field(l), Top::Field(d)] = w
            && l.ftype == FieldType::Length
            && d.ftype == FieldType::Data
        {
            data_handled.insert(d.number);
            data_after.insert(l.number, *d);
        }
    }

    emit_struct(s, &ty, &tops);
    let _ = writeln!(s, "impl<'buf> {ty}<'buf> {{");
    emit_required(s, &tops);
    emit_decode(s, &tops, &data_handled, &data_after);
    emit_accessors(s, &tops, &m.name);
    s.push_str("}\n\n");
}

fn emit_required(s: &mut String, tops: &[Top]) {
    let mut req = Vec::new();
    let mut seen = HashSet::new();
    for t in tops {
        match t {
            Top::Field(f) if f.required && seen.insert(f.number) => req.push(f.number),
            Top::Group(g) if g.required && seen.insert(g.number) => req.push(g.number),
            _ => {}
        }
    }
    let list = req
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(s, "    pub const REQUIRED: &'static [u32] = &[{list}];\n");
}

fn emit_struct(s: &mut String, ty: &str, tops: &[Top]) {
    let _ = writeln!(s, "pub struct {ty}<'buf> {{\n    buf: &'buf [u8],");
    let mut seen = HashSet::new();
    for t in tops {
        match t {
            Top::Field(f) if seen.insert(f.number) => {
                let _ = writeln!(s, "    {}: nexus_fix_codec::FieldSpan,", snake(&f.name));
            }
            Top::Group(g) if seen.insert(g.number) => {
                let _ = writeln!(s, "    {}: nexus_fix_codec::GroupSpan,", snake(&g.name));
            }
            _ => {}
        }
    }
    s.push_str("}\n\n");
}

fn emit_decode(
    s: &mut String,
    tops: &[Top],
    data_handled: &HashSet<u32>,
    data_after: &HashMap<u32, &RField>,
) {
    s.push_str("    pub fn decode(buf: &'buf [u8]) -> Self {\n");
    s.push_str("        let mut m = Self {\n            buf,\n");
    let mut seen = HashSet::new();
    for t in tops {
        match t {
            Top::Field(f) if seen.insert(f.number) => {
                let _ = writeln!(
                    s,
                    "            {}: nexus_fix_codec::FieldSpan::EMPTY,",
                    snake(&f.name)
                );
            }
            Top::Group(g) if seen.insert(g.number) => {
                let _ = writeln!(
                    s,
                    "            {}: nexus_fix_codec::GroupSpan::EMPTY,",
                    snake(&g.name)
                );
            }
            _ => {}
        }
    }
    s.push_str("        };\n");

    let mut arms: Vec<String> = Vec::new();
    let mut seen_arm = HashSet::new();
    for t in tops {
        match t {
            Top::Field(f) => {
                if data_handled.contains(&f.number) || !seen_arm.insert(f.number) {
                    continue;
                }
                if let Some(d) = data_after.get(&f.number) {
                    arms.push(data_arm(f, d));
                } else {
                    arms.push(format!(
                        "f.tag == super::fields::TAG_{} {{\n                m.{} = f.value;\n            }}",
                        screaming(&f.name),
                        snake(&f.name)
                    ));
                }
            }
            Top::Group(g) => {
                if !seen_arm.insert(g.number) {
                    continue;
                }
                arms.push(group_arm(g));
            }
        }
    }

    if !arms.is_empty() {
        s.push_str("        let mut r = nexus_fix_codec::FieldReader::new(buf, 0);\n");
        s.push_str("        while let Some(f) = r.next_field() {\n");
        let _ = writeln!(s, "            if {}", arms.join(" else if "));
        s.push_str("        }\n");
    }
    s.push_str("        m\n    }\n\n");
}

fn data_arm(len: &RField, data: &RField) -> String {
    let mut b = String::new();
    let _ = writeln!(b, "f.tag == super::fields::TAG_{} {{", screaming(&len.name));
    let _ = writeln!(b, "                m.{} = f.value;", snake(&len.name));
    b.push_str("                let (n, _) = nexus_fix_codec::parse_tag(f.value.slice(buf));\n");
    b.push_str("                let dstart = r.pos();\n");
    b.push_str("                let (_, dtl) = nexus_fix_codec::parse_tag(&buf[dstart..]);\n");
    b.push_str("                let vstart = dstart + dtl + 1;\n");
    let _ = writeln!(
        b,
        "                m.{} = nexus_fix_codec::FieldSpan::new(vstart as u32, n);",
        snake(&data.name)
    );
    b.push_str(
        "                r = nexus_fix_codec::FieldReader::new(buf, vstart + n as usize + 1);\n",
    );
    b.push_str("            }");
    b
}

fn group_arm(g: &RGroup) -> String {
    let mut tags = Vec::new();
    subtree_tags(&g.members, &mut tags);
    let tag_list = tag_array(&tags);
    let mut b = String::new();
    let _ = writeln!(b, "f.tag == super::fields::TAG_{} {{", screaming(&g.name));
    b.push_str(
        "                let (count, _) = nexus_fix_codec::parse_tag(f.value.slice(buf));\n",
    );
    let _ = writeln!(
        b,
        "                m.{} = nexus_fix_codec::GroupSpan::new(r.pos() as u32, count as u16);",
        snake(&g.name)
    );
    b.push_str("                loop {\n");
    b.push_str("                    let mark = r.pos();\n");
    b.push_str("                    match r.next_field() {\n");
    let _ = writeln!(
        b,
        "                        Some(gf) if [{tag_list}].contains(&gf.tag) => {{}}"
    );
    b.push_str("                        _ => {\n");
    b.push_str("                            r = nexus_fix_codec::FieldReader::new(buf, mark);\n");
    b.push_str("                            break;\n");
    b.push_str("                        }\n");
    b.push_str("                    }\n                }\n");
    b.push_str("            }");
    b
}

fn emit_accessors(s: &mut String, tops: &[Top], msg_name: &str) {
    let prefix = pascal(msg_name);
    let mut seen = HashSet::new();
    for t in tops {
        match t {
            Top::Field(f) if seen.insert(f.number) => {
                let name = snake(&f.name);
                let _ = write!(
                    s,
                    "    pub fn {name}(&self) -> Option<&'buf [u8]> {{\n        if self.{name}.is_present() {{ Some(self.{name}.slice(self.buf)) }} else {{ None }}\n    }}\n\n"
                );
                if f.is_enum {
                    emit_enum_accessor(s, f, &name);
                }
            }
            Top::Group(g) if seen.insert(g.number) => {
                let name = snake(&g.name);
                let iter = format!("{}Iter", group_type(&prefix, &g.name));
                let _ = write!(
                    s,
                    "    pub fn {name}(&self) -> super::groups::{iter}<'buf> {{\n        super::groups::{iter}::new(self.buf, self.{name})\n    }}\n\n"
                );
            }
            _ => {}
        }
    }
}

fn emit_enum_accessor(s: &mut String, f: &RField, name: &str) {
    let ty = pascal(&f.name);
    if f.single_char {
        let _ = write!(
            s,
            "    pub fn {name}_enum(&self) -> Option<super::fields::{ty}> {{\n        super::fields::{ty}::from_byte(*self.{name}()?.first()?)\n    }}\n\n"
        );
    } else {
        let _ = write!(
            s,
            "    pub fn {name}_enum(&self) -> Option<super::fields::{ty}> {{\n        super::fields::{ty}::from_bytes(self.{name}()?)\n    }}\n\n"
        );
    }
}

fn tag_array(tags: &[u32]) -> String {
    tags.iter()
        .enumerate()
        .map(|(i, t)| {
            if i == 0 {
                format!("{t}u32")
            } else {
                t.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}
