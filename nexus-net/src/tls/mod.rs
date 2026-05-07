//! TLS codec — sans-IO encrypt/decrypt via rustls.
//!
//! Sits between the socket and protocol parsers:
//!
//! ```text
//! socket → TlsCodec (decrypt) → FrameReader / ResponseReader → Message
//! Request → TlsCodec (encrypt) → socket
//! ```
//!
//! # Quick Start
//!
//! ```ignore
//! use nexus_net::tls::TlsConfig;
//! use nexus_net::ws::Client;
//!
//! let tls = TlsConfig::new()?;
//! let mut ws = Client::builder()
//!     .tls(&tls)
//!     .connect("wss://exchange.com/ws/v1")?;
//!
//! while let Some(msg) = ws.recv()? {
//!     process(msg);
//! }
//! ```
//!
//! # Choosing an input primitive
//!
//! [`TlsCodec`] provides three inbound primitives. Pick based on how
//! the caller produces ciphertext:
//!
//! | Primitive | Use when |
//! |---|---|
//! | [`TlsCodec::read_tls_step`] | **Streaming app-data adapters** (the common case for async TLS). Caller alternates ciphertext input with plaintext output to avoid overflowing rustls's internal plaintext queue. |
//! | [`TlsCodec::read_and_process_tls`] | **Bounded handshake input.** Caller tolerates plaintext queuing internally until the helper returns. Do **not** use for streaming app-data — large inputs overflow rustls's plaintext queue mid-call. |
//! | [`TlsCodec::read_tls`] | **Advanced use only.** Direct rustls wrapper; caller drives [`TlsCodec::process_new_packets`] and tracks partial consumption manually. Use only when neither of the helpers above fits the adapter shape. |

mod codec;
mod config;
mod error;
mod stream;

pub use codec::TlsCodec;
pub use config::{TlsConfig, TlsConfigBuilder};
pub use error::TlsError;
pub use stream::TlsStream;
