use std::collections::HashSet;
use std::fmt::Write;

use super::{
    HEADER, RGroup, RMember, RMessage, group_type, pascal, screaming, snake, subtree_tags,
};

pub fn emit(messages: &[RMessage]) -> String {
    let mut s = String::new();
    s.push_str(HEADER);
    for m in messages {
        let prefix = pascal(&m.name);
        for mem in &m.members {
            if let RMember::Group(g) = mem {
                emit_group(&mut s, &prefix, g);
            }
        }
    }
    s
}

fn emit_group(s: &mut String, prefix: &str, g: &RGroup) {
    let base = group_type(prefix, &g.name);
    let entry = format!("{base}Entry");
    let iter = format!("{base}Iter");

    let _ = write!(
        s,
        "pub struct {iter}<'buf> {{\n    buf: &'buf [u8],\n    pos: usize,\n    remaining: u16,\n}}\n\n"
    );
    let _ = writeln!(s, "impl<'buf> {iter}<'buf> {{");
    let _ = write!(
        s,
        "    pub fn new(buf: &'buf [u8], span: nexus_fix_codec::GroupSpan) -> Self {{\n        Self {{ buf, pos: span.offset as usize, remaining: span.count }}\n    }}\n}}\n\n"
    );
    let _ = writeln!(
        s,
        "impl<'buf> Iterator for {iter}<'buf> {{\n    type Item = {entry}<'buf>;"
    );
    let _ = write!(
        s,
        "    fn next(&mut self) -> Option<Self::Item> {{\n        if self.remaining == 0 {{\n            return None;\n        }}\n        self.remaining -= 1;\n        let (e, next) = {entry}::decode(self.buf, self.pos);\n        self.pos = next;\n        Some(e)\n    }}\n}}\n\n"
    );

    emit_entry(s, &base, &entry, g);

    for mem in &g.members {
        if let RMember::Group(inner) = mem {
            emit_group(s, &base, inner);
        }
    }
}

fn emit_entry(s: &mut String, base: &str, entry: &str, g: &RGroup) {
    let _ = writeln!(s, "pub struct {entry}<'buf> {{\n    buf: &'buf [u8],");
    let mut seen = HashSet::new();
    for mem in &g.members {
        match mem {
            RMember::Field(f) if seen.insert(f.number) => {
                let _ = writeln!(s, "    {}: nexus_fix_codec::FieldSpan,", snake(&f.name));
            }
            RMember::Group(inner) if seen.insert(inner.number) => {
                let _ = writeln!(s, "    {}: nexus_fix_codec::GroupSpan,", snake(&inner.name));
            }
            _ => {}
        }
    }
    s.push_str("}\n\n");

    let mut tags = Vec::new();
    subtree_tags(&g.members, &mut tags);
    let tag_list = tag_array(&tags);

    let _ = writeln!(s, "impl<'buf> {entry}<'buf> {{");
    s.push_str(
        "    fn decode(buf: &'buf [u8], start: usize) -> (Self, usize) {\n        let mut e = Self {\n            buf,\n",
    );
    let mut seen = HashSet::new();
    for mem in &g.members {
        match mem {
            RMember::Field(f) if seen.insert(f.number) => {
                let _ = writeln!(
                    s,
                    "            {}: nexus_fix_codec::FieldSpan::EMPTY,",
                    snake(&f.name)
                );
            }
            RMember::Group(inner) if seen.insert(inner.number) => {
                let _ = writeln!(
                    s,
                    "            {}: nexus_fix_codec::GroupSpan::EMPTY,",
                    snake(&inner.name)
                );
            }
            _ => {}
        }
    }
    s.push_str("        };\n");
    s.push_str("        let mut r = nexus_fix_codec::FieldReader::new(buf, start);\n");
    s.push_str("        let mut first = true;\n");
    s.push_str("        loop {\n            let mark = r.pos();\n");
    s.push_str("            let Some(f) = r.next_field() else { break };\n");
    let _ = writeln!(
        s,
        "            if (f.tag == {} && !first) || ![{tag_list}].contains(&f.tag) {{\n                return (e, mark);\n            }}",
        g.delimiter
    );
    s.push_str("            first = false;\n");

    let mut arms: Vec<String> = Vec::new();
    let mut seen_arm = HashSet::new();
    for mem in &g.members {
        match mem {
            RMember::Field(f) if seen_arm.insert(f.number) => {
                arms.push(format!(
                    "f.tag == super::fields::TAG_{} {{\n                e.{} = f.value;\n            }}",
                    screaming(&f.name),
                    snake(&f.name)
                ));
            }
            RMember::Group(inner) if seen_arm.insert(inner.number) => {
                arms.push(nested_arm(inner));
            }
            _ => {}
        }
    }
    let _ = writeln!(s, "            if {}", arms.join(" else if "));
    s.push_str("        }\n        (e, r.pos())\n    }\n\n");

    emit_entry_accessors(s, base, g);
    s.push_str("}\n\n");
}

fn nested_arm(inner: &RGroup) -> String {
    let mut tags = Vec::new();
    subtree_tags(&inner.members, &mut tags);
    let tag_list = tag_array(&tags);
    let mut b = String::new();
    let _ = writeln!(
        b,
        "f.tag == super::fields::TAG_{} {{",
        screaming(&inner.name)
    );
    b.push_str(
        "                let (count, _) = nexus_fix_codec::parse_tag(f.value.slice(buf));\n",
    );
    let _ = writeln!(
        b,
        "                e.{} = nexus_fix_codec::GroupSpan::new(r.pos() as u32, count as u16);",
        snake(&inner.name)
    );
    b.push_str("                loop {\n                    let nmark = r.pos();\n");
    b.push_str("                    match r.next_field() {\n");
    let _ = writeln!(
        b,
        "                        Some(nf) if [{tag_list}].contains(&nf.tag) => {{}}"
    );
    b.push_str("                        _ => {\n                            r = nexus_fix_codec::FieldReader::new(buf, nmark);\n                            break;\n                        }\n");
    b.push_str("                    }\n                }\n");
    b.push_str("            }");
    b
}

fn emit_entry_accessors(s: &mut String, base: &str, g: &RGroup) {
    let mut seen = HashSet::new();
    for mem in &g.members {
        match mem {
            RMember::Field(f) if seen.insert(f.number) => {
                let name = snake(&f.name);
                let _ = write!(
                    s,
                    "    pub fn {name}(&self) -> Option<&'buf [u8]> {{\n        if self.{name}.is_present() {{ Some(self.{name}.slice(self.buf)) }} else {{ None }}\n    }}\n\n"
                );
                if f.is_enum {
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
            }
            RMember::Group(inner) if seen.insert(inner.number) => {
                let name = snake(&inner.name);
                let iter = format!("{}Iter", group_type(base, &inner.name));
                let _ = write!(
                    s,
                    "    pub fn {name}(&self) -> super::groups::{iter}<'buf> {{\n        super::groups::{iter}::new(self.buf, self.{name})\n    }}\n\n"
                );
            }
            _ => {}
        }
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
