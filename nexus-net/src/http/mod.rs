//! Sans-IO HTTP/1.x protocol primitives.
//!
//! Built on [`httparse`] for SIMD-accelerated header parsing.
//! Uses [`ReadBuf`](crate::buf::ReadBuf) for incremental byte buffering.
//!
//! - [`ResponseReader`] — parse inbound HTTP responses (used by REST client)
//! - [`ChunkedDecoder`] — chunked transfer encoding decoder
//! - [`write_request`] / [`write_response`] — construct outbound HTTP messages
//!
//! The HTTP client API is in [`rest`](crate::rest).
//! `RequestReader` is internal (used for WebSocket upgrade handshake).

mod chunked;
mod error;
mod request;
mod response;

/// Default capacity for HTTP request/response handshake buffers.
///
/// Sized to comfortably fit a typical HTTP/1.1 head section
/// (request line + headers up to ~3-4 KiB) on a single allocation.
/// Used as the initial capacity of [`RequestReader`] / [`ResponseReader`]
/// and as the per-recv read cap during the WebSocket upgrade handshake
/// and REST request/response headers.
///
/// Tuning beyond this default is a follow-up — currently this is a
/// hardcoded internal default. Callers that need larger handshake
/// budgets (very long cookies, many headers, OCSP-stapled
/// certificates over HTTP) would currently work around by sending
/// fewer headers; a builder knob is a separate concern.
pub const HTTP_HANDSHAKE_BUFFER: usize = 4096;

pub use chunked::ChunkedDecoder;
pub use error::HttpError;
// RequestReader parses inbound HTTP requests (used for WS upgrade handshake).
// The public HTTP client API is in `rest::`.
pub use request::RequestReader;
pub use response::{
    Response, ResponseReader, request_size, response_size, write_request, write_response,
};
