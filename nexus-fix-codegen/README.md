# nexus-fix-codegen

Dictionary-driven code generator for [`nexus-fix-codec`](../nexus-fix-codec).
Reads a QuickFIX XML dictionary and emits zero-copy, zero-alloc Rust decoders
and encoders that sit directly on the codec primitives.

## Output

Per dictionary, five files:

- `fields.rs` — `TAG_*` constants and typed enums (`from_byte`/`as_byte` or
  `from_bytes`/`as_bytes`).
- `messages.rs` — per-`MsgType` flyweight decoders. A single forward pass over
  `FieldReader` dispatches each tag into a `FieldSpan` slot; accessors are pure
  reads. DATA fields are read length-delimited so an embedded `0x01` never
  mis-splits.
- `groups.rs` — repeating-group iterators and per-entry decoders, recursive.
- `encoders.rs` — consume-self builders over `FieldWriter`.
- `mod.rs` — re-exports, `BEGIN_STRING`, and `MsgType` dispatch.

## CLI

```bash
cargo run -p nexus-fix-codegen -- --dict dict/FIX44.xml --out src/generated/
```

## build.rs

```rust
nexus_fix_codegen::generate()
    .dictionary("dict/FIX44.xml")
    .out_dir(std::env::var("OUT_DIR").unwrap())
    .run()
    .unwrap();
```

```rust
pub mod fix {
    include!(concat!(env!("OUT_DIR"), "/mod.rs"));
}
```

DATA fields inside repeating groups are rejected at generation time
(`EmitError::DataInGroup`).
