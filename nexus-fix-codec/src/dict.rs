/// Dictionary-level knowledge for a specific FIX version.
///
/// Generated per dictionary (FIX 4.2, FIX 4.4, etc.) by `nexus-fix-codegen`.
/// The implementing type is a zero-sized struct — all information is
/// compile-time. [`HeaderDecoder`](crate::HeaderDecoder) and the future
/// `Session` are generic over this trait, so FIX-version dispatch
/// monomorphizes away with no vtable or runtime branching.
pub trait FixDictionary {
    /// The dictionary's message-type enum (generated, closed set).
    type MsgType: Copy + Eq + core::fmt::Debug;

    /// The `BeginString` value for this FIX version (e.g. `b"FIX.4.4"`).
    const BEGIN_STRING: &'static [u8];

    /// Parse a raw MsgType value into the dictionary's enum.
    fn msg_type_from_bytes(bytes: &[u8]) -> Option<Self::MsgType>;

    /// Whether the given message type is an admin (session-level) message.
    fn is_admin(msg_type: Self::MsgType) -> bool;
}
