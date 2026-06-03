//! Sans-IO FIX session layer.
//!
//! [`Session`] is a pure state machine: the caller owns the transport,
//! the clock, and the encode buffer. Inbound messages go in through
//! [`Session::handle_message`], time goes in through
//! [`Session::handle_timeout`], and the session communicates back through
//! drained [`Event`]s and pending admin messages encoded on demand with
//! [`Session::encode_pending`].
//!
//! The session never allocates after construction. Admin messages
//! (Logon, Logout, Heartbeat, TestRequest, ResendRequest, SequenceReset,
//! Reject) are handled internally; application messages surface as
//! [`Event::App`] and the caller decodes them from its own buffer.

mod session;
mod timestamp;

pub use session::{DisconnectReason, Event, Session, SessionConfig, State};
