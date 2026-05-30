//! nexus-async-net — async adapters for nexus-net.
//!
//! Thin async wrappers over nexus-net's synchronous protocol primitives.
//! Same zero-copy parsing, same performance — just `.await` on I/O.
//!
//! # Runtime Features
//!
//! Exactly one async runtime must be enabled (mutually exclusive):
//!
//! - **`tokio-rt`** (default) — tokio-based adapters for WebSocket and REST.
//! - **`nexus`** — nexus-async-rt-based adapters (single-threaded, pre-allocated).
//!   *(Renamed from `nexus-rt` in v0.4.2.)*
//!
//! # Modules
//!
//! - [`ws`] — Async WebSocket (wraps FrameReader/FrameWriter).
//!   Both backends provide `recv()`/`send_*()`. The tokio backend also
//!   implements `Stream`/`Sink` for ecosystem integration.
//! - [`rest`] — Async HTTP REST client (wraps RequestWriter/ResponseReader)
//!
//! # Custom transports
//!
//! `WsStream<S>` / `HttpConnection<S>` consume a
//! [`WireStream`](nexus_net::WireStream) — the canonical `MaybeTls`
//! transport implements it directly. To plug a custom
//! `AsyncRead+AsyncWrite` transport into the same API, wrap it at
//! the call site:
//!
//! - tokio (`feature = "tokio-rt"`): [`AsyncReadAdapter`]
//! - nexus-async-rt (`feature = "nexus"`): [`NexusAsyncReadAdapter`]
//!
//! ```ignore
//! let tcp = tokio::net::TcpStream::connect(addr).await?;
//! let ws = WsStreamBuilder::new()
//!     .connect_with(AsyncReadAdapter::new(tcp), url)
//!     .await?;
//! ```

#![warn(missing_docs)]

#[cfg(all(feature = "tokio-rt", feature = "nexus"))]
compile_error!("features `tokio-rt` and `nexus` are mutually exclusive — pick one async runtime");

// `maybe_tls` is implementation surface for the connection builders;
// exposed publicly only so integration tests in this crate's
// `tests/` directory can construct `TlsInner` directly (the test
// binary is a separate compilation unit).
#[doc(hidden)]
pub mod maybe_tls;
pub mod rest;
mod wire;
pub mod ws;

#[cfg(feature = "tokio-rt")]
pub use wire::AsyncReadAdapter;
#[cfg(feature = "nexus")]
pub use wire::NexusAsyncReadAdapter;

// Re-export nexus-net types that appear in our public API.
// Users who need deeper access can depend on nexus-net directly.
pub use nexus_net::ws::{
    CloseCode, CloseFrame, Error as WsError, FrameReader, FrameReaderBuilder, FrameWriter,
    HandshakeError, Message, OwnedCloseFrame, OwnedMessage, Role,
};
pub use nexus_net::{WireStream, buf::WriteBuf};

/// REST types used in [`rest::HttpConnection`] and [`rest::ClientSlot`].
pub mod rest_types {
    pub use nexus_net::http::ResponseReader;
    pub use nexus_net::rest::{Request, RequestWriter, RestError, RestResponse};
}

#[cfg(feature = "tls")]
pub use nexus_net::tls::{TlsConfig, TlsError};
