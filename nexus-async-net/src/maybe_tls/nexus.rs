//! MaybeTls — plain TCP or TLS, unified async I/O (nexus-async-rt backend).
//!
//! Unlike the tokio variant which delegates TLS to `tokio-rustls`, this
//! drives nexus-net's sans-IO [`TlsCodec`] at the poll level. The codec
//! handles encrypt/decrypt; we shuttle bytes between it and the TCP stream.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use nexus_async_rt::{AsyncRead, AsyncWrite, TcpStream};
#[cfg(feature = "tls")]
use nexus_net::buf::{ReadBuf, WriteBuf};

/// Per-poll TLS read chunk size used by the TLS adapter's `poll_read`.
/// Module-level const so it can be used in struct field types;
/// re-exposed publicly as [`TlsInner::TMP_SIZE`].
#[cfg(feature = "tls")]
const TMP_SIZE: usize = 8192;

// Latent bug guard: read_and_process_tls is used during handshake to
// consume the full slice. If the burst carrying ServerFinished also
// piggybacks app-data records (TLS 1.3 allows this), the helper
// continues consuming past the handshake transition and queues the
// app-data plaintext in rustls's internal buffer — capped at ~16 KiB.
// With TMP_SIZE = 8 KiB we cannot overflow it on a single read. **If
// you bump TMP_SIZE past 16 KiB, fix the handshake-piggyback path
// first** — see the 0.7.0 follow-up issue. The proper fix is
// hoisting handshake into TlsInner so `pending_read` is reachable
// for direct stash without an intermediate allocation.
#[cfg(feature = "tls")]
const _: () = assert!(
    TMP_SIZE <= 16 * 1024,
    "TMP_SIZE > 16 KiB requires handshake-piggyback fix (0.7.0)"
);

/// Async stream that may or may not be TLS-wrapped.
///
/// Created by connection builders based on the URL scheme.
///
/// # Shutdown (TLS variant)
///
/// `poll_shutdown` queues a TLS `close_notify` alert, flushes the
/// resulting ciphertext to the transport, then closes the underlying
/// transport. Callers do not need to flush manually — `poll_shutdown`
/// drives any pending plaintext through to the wire as part of its
/// shutdown sequence.
///
/// If the caller drops the stream without calling `poll_shutdown`,
/// any pending plaintext (in rustls's outbound queue) and ciphertext
/// (in `pending_write`) is discarded, and the peer sees TCP FIN
/// without close_notify — which rustls treats as a truncation alert.
/// Callers needing graceful termination must call `shutdown().await`
/// (or drive `poll_shutdown` to `Ready`) before drop.
///
/// # Memory (TLS variant)
///
/// Each TLS-wrapped connection allocates approximately 81 KiB of
/// heap-resident buffers:
///
/// | Buffer | Size | Purpose |
/// |---|---|---|
/// | `pending_read` | 8 KiB | Spillover for partially-consumed inbound TLS records |
/// | `pending_write` | 64 KiB | Outbound ciphertext FIFO (drains to socket) |
/// | `tmp` | 8 KiB | Per-connection scratch buffer for transport reads |
/// | rustls state | ~1 KiB | Crypto state + small fixed buffers |
///
/// Trading workloads with small frequent messages can reduce
/// `pending_write` via the connection builder's
/// `tls_buffer_capacities(read_cap, write_cap)` setter — 8–16 KiB
/// is sufficient for most order-entry and market-data clients. For
/// 1000 connections with default sizing, expect ~81 MiB of buffer
/// footprint.
pub enum MaybeTls {
    /// Plain TCP (ws://, http://).
    Plain(TcpStream),
    /// TLS over TCP (wss://, https://).
    #[cfg(feature = "tls")]
    Tls(Box<TlsInner>),
}

/// TLS state: a TCP stream plus the sans-IO codec and cursor-based
/// staging buffers for ciphertext in both directions.
///
/// Opaque to users — fields are `pub(crate)`. Exposed only because
/// [`MaybeTls::Tls`] holds a `Box<TlsInner>`.
#[cfg(feature = "tls")]
pub struct TlsInner {
    pub(crate) stream: TcpStream,
    pub(crate) codec: nexus_net::tls::TlsCodec,
    /// Ciphertext read from the transport but not yet accepted by
    /// rustls. Cursor-based — `advance(n)` is O(1) and auto-resets
    /// when fully drained.
    pending_read: ReadBuf,
    /// Ciphertext waiting to be flushed to the transport. Same
    /// cursor semantics as `pending_read`.
    pending_write: WriteBuf,
    /// Per-poll scratch buffer for `poll_read`. Boxed so the 8 KiB
    /// stays off the per-poll stack frame — eliminates a per-poll
    /// memset + stack-probe pair.
    tmp: Box<[u8; TMP_SIZE]>,
}

#[cfg(feature = "tls")]
impl TlsInner {
    /// Per-poll TLS read chunk size used by `poll_read`.
    /// `pending_read` capacity must be at least this large so the
    /// spillover-copy after a partial codec read fits.
    pub(crate) const TMP_SIZE: usize = TMP_SIZE;

    /// Default capacity for the outbound ciphertext buffer.
    ///
    /// 64 KiB matches rustls's `DEFAULT_BUFFER_LIMIT` — the outbound
    /// plaintext queue cap. A 64 KiB plaintext encrypt produces
    /// ~64 KiB + ~120 bytes of ciphertext (TLS record headers + auth
    /// tags), so a single max-size encrypt triggers exactly one
    /// drain/refill iteration in `poll_write`. Bumping this to
    /// 80 KiB would absorb the overhead in one shot but breaks the
    /// symmetric default; the drain/refill is cheap (non-blocking
    /// write) and the symmetry is the more discoverable choice.
    ///
    /// Larger writes are chunked across multiple `poll_write` calls
    /// via `TlsCodec::try_encrypt` regardless of this cap.
    pub(crate) const DEFAULT_PENDING_WRITE_CAPACITY: usize = 65_536;

    /// Construct with default buffer capacities. Convenience wrapper
    /// around [`with_capacities`](Self::with_capacities) — kept for
    /// callers that don't need overrides (currently only tests; the
    /// builder plumbing always goes through `with_capacities`).
    #[allow(dead_code)]
    pub(crate) fn new(stream: TcpStream, codec: nexus_net::tls::TlsCodec) -> Self {
        Self::with_capacities(
            stream,
            codec,
            Self::TMP_SIZE,
            Self::DEFAULT_PENDING_WRITE_CAPACITY,
        )
    }

    /// Construct with explicit buffer capacities.
    ///
    /// `pending_read_cap` **must be at least [`TMP_SIZE`](Self::TMP_SIZE)** —
    /// the per-poll read chunk size in `poll_read`. The spillover-copy
    /// after a partial codec read assumes spare capacity for the full
    /// remainder of one tmp read.
    ///
    /// # Panics
    /// Panics if `pending_read_cap < TMP_SIZE`.
    pub(crate) fn with_capacities(
        stream: TcpStream,
        codec: nexus_net::tls::TlsCodec,
        pending_read_cap: usize,
        pending_write_cap: usize,
    ) -> Self {
        assert!(
            pending_read_cap >= Self::TMP_SIZE,
            "pending_read_cap ({pending_read_cap}) must be >= TMP_SIZE ({})",
            Self::TMP_SIZE,
        );
        Self {
            stream,
            codec,
            pending_read: ReadBuf::with_capacity(pending_read_cap),
            pending_write: WriteBuf::new(pending_write_cap, 0),
            // Heap-allocated, lives for the connection's lifetime. Earlier
            // versions stack-allocated this per `poll_read`; the per-poll
            // memset + stack probe was a measurable cost on the steady-state
            // hot path. For long-lived TLS connections the alloc amortises
            // over millions of polls.
            tmp: Box::new([0u8; TMP_SIZE]),
        }
    }

}

impl MaybeTls {
    /// Whether this connection is TLS-wrapped.
    pub fn is_tls(&self) -> bool {
        match self {
            Self::Plain(_) => false,
            #[cfg(feature = "tls")]
            Self::Tls(_) => true,
        }
    }
}

// =============================================================================
// AsyncRead
// =============================================================================

impl AsyncRead for MaybeTls {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_read(cx, buf),
            #[cfg(feature = "tls")]
            MaybeTls::Tls(inner) => {
                if buf.is_empty() {
                    return Poll::Ready(Ok(0));
                }

                loop {
                    // 1. Drain any plaintext rustls has decrypted.
                    let n = inner.codec.read_plaintext(buf).map_err(tls_to_io)?;
                    if n > 0 {
                        return Poll::Ready(Ok(n));
                    }

                    // 2. Step buffered ciphertext one packet at a time
                    //    so rustls can release plaintext between calls.
                    if !inner.pending_read.is_empty() {
                        let consumed = inner
                            .codec
                            .read_tls_step(inner.pending_read.data())
                            .map_err(tls_to_io)?;
                        // State invariant: every error leg above this
                        // line MUST return before reaching here. If you
                        // add new error returns, place them BEFORE this
                        // side-effect — pending_read can be left
                        // inconsistent if advance() is half-applied.
                        inner.pending_read.advance(consumed);
                        continue;
                    }

                    // 3. No buffered ciphertext — pull more from the transport
                    //    into the heap-resident scratch buffer (avoids a
                    //    per-poll 8 KiB stack memset).
                    let n = match Pin::new(&mut inner.stream).poll_read(cx, &mut inner.tmp[..]) {
                        Poll::Ready(Ok(0)) => return Poll::Ready(Ok(0)), // EOF
                        Poll::Ready(Ok(n)) => n,
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Pending => return Poll::Pending,
                    };
                    let consumed = inner
                        .codec
                        .read_tls_step(&inner.tmp[..n])
                        .map_err(tls_to_io)?;
                    if consumed < n {
                        let rem_len = n - consumed;
                        let spare = inner.pending_read.spare();
                        spare[..rem_len].copy_from_slice(&inner.tmp[consumed..n]);
                        // State invariant: every error leg above this
                        // line MUST return before reaching here. If you
                        // add new error returns, place them BEFORE this
                        // side-effect — pending_read can be left
                        // inconsistent if filled() is half-applied.
                        inner.pending_read.filled(rem_len);
                    }
                }
            }
        }
    }
}

// =============================================================================
// AsyncWrite
// =============================================================================

impl AsyncWrite for MaybeTls {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_write(cx, buf),
            #[cfg(feature = "tls")]
            MaybeTls::Tls(inner) => {
                // 1. Drain pending ciphertext to free pending_write space.
                drain_pending(inner, cx)?;
                if !inner.pending_write.is_empty() {
                    return Poll::Pending;
                }

                // 2. Pull queued ciphertext from rustls into pending_write
                //    and on to the socket. Frees rustls's plaintext queue
                //    so try_encrypt has room for new bytes.
                drain_codec_to_pending(inner, cx)?;
                drain_pending(inner, cx)?;
                if !inner.pending_write.is_empty() {
                    return Poll::Pending;
                }

                // 3. Encrypt as much of buf as rustls's queue can accept.
                //    Chunked: returns Ok(0) if the queue is full and the
                //    caller must come back later.
                let consumed = inner.codec.try_encrypt(buf).map_err(tls_to_io)?;
                if consumed == 0 {
                    // Defensive: rustls should not return 0 here after
                    // we've drained both its outbound queue and the
                    // socket. If it does (rustls bug or edge case),
                    // wake_by_ref ensures the runtime re-polls us
                    // instead of stalling indefinitely.
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }

                // 4. Best-effort flush of what we just produced.
                drain_codec_to_pending(inner, cx)?;
                drain_pending(inner, cx)?;

                Poll::Ready(Ok(consumed))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_flush(cx),
            #[cfg(feature = "tls")]
            MaybeTls::Tls(inner) => {
                // Drain any codec ciphertext not yet staged.
                drain_codec_to_pending(inner, cx)?;

                // Drain pending_write to the transport.
                drain_pending(inner, cx)?;
                if !inner.pending_write.is_empty() {
                    return Poll::Pending;
                }

                // Flush the underlying stream.
                Pin::new(&mut inner.stream).poll_flush(cx)
            }
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_shutdown(cx),
            #[cfg(feature = "tls")]
            MaybeTls::Tls(inner) => {
                // 1. Queue close_notify (idempotent — rustls no-ops on
                //    dupes, so re-entering after Pending is safe).
                inner.codec.send_close_notify();

                // 2. Drain rustls's queue (now including close_notify
                //    ciphertext) into pending_write.
                drain_codec_to_pending(inner, cx)?;

                // 3. Flush pending_write to the transport. If we can't
                //    fully drain yet, wait for the next poll.
                drain_pending(inner, cx)?;
                if !inner.pending_write.is_empty() {
                    return Poll::Pending;
                }

                // 4. Now safe to shutdown the transport.
                Pin::new(&mut inner.stream).poll_shutdown(cx)
            }
        }
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Drain the `pending_write` buffer to the transport, writing as much as the
/// socket will accept without blocking. Cursor advances are O(1) and
/// auto-reset to the buffer's start when fully drained.
#[cfg(feature = "tls")]
fn drain_pending(inner: &mut TlsInner, cx: &mut Context<'_>) -> io::Result<()> {
    while !inner.pending_write.is_empty() {
        match Pin::new(&mut inner.stream).poll_write(cx, inner.pending_write.data()) {
            Poll::Ready(Ok(0)) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "transport write returned 0",
                ));
            }
            Poll::Ready(Ok(n)) => {
                inner.pending_write.advance(n);
            }
            Poll::Ready(Err(e)) => return Err(e),
            Poll::Pending => return Ok(()), // will retry on next poll
        }
    }
    Ok(())
}

/// Move all ciphertext rustls wants to write into `pending_write`,
/// draining `pending_write` to the socket between iterations so a
/// single big encrypt can't outrun `pending_write`'s fixed capacity.
///
/// Returns `Ok(())` once rustls is drained or the socket can no longer
/// accept bytes (in which case the leftover ciphertext stays inside
/// rustls and is picked up on the next call).
///
/// Distinguishes two distinct exit conditions:
/// - `pending_write.spare().is_empty()` after a drain attempt —
///   legitimate backpressure, returns `Ok(())`.
/// - `write_tls_to` returns 0 into a non-empty spare slice — a rustls
///   contract violation. Surfaced as `WriteZero` rather than masked
///   as a stalled connection.
#[cfg(feature = "tls")]
fn drain_codec_to_pending(inner: &mut TlsInner, cx: &mut Context<'_>) -> io::Result<()> {
    while inner.codec.wants_write() {
        if inner.pending_write.spare().is_empty() {
            // Backpressure: try to drain to free space.
            drain_pending(inner, cx)?;
            if inner.pending_write.spare().is_empty() {
                // Socket can't take more right now. Remaining
                // ciphertext stays queued inside rustls and is picked
                // up by the next poll_write/poll_flush.
                return Ok(());
            }
        }
        let n = inner.codec.write_tls_to(&mut inner.pending_write.spare())?;
        if n == 0 {
            // wants_write said yes, spare was non-empty, yet rustls
            // produced 0 bytes. Surface explicitly — silent break here
            // would mask a stalled connection as success.
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "rustls reported wants_write but produced 0 bytes \
                 into a non-empty buffer",
            ));
        }
        inner.pending_write.filled(n);
        drain_pending(inner, cx)?;
    }
    Ok(())
}

/// Convert a [`TlsError`](nexus_net::tls::TlsError) into an [`io::Error`].
#[cfg(feature = "tls")]
fn tls_to_io(e: nexus_net::tls::TlsError) -> io::Error {
    match e {
        nexus_net::tls::TlsError::Io(io_err) => io_err,
        other => io::Error::other(other),
    }
}

#[cfg(all(test, feature = "tls"))]
mod tests {
    use std::io::{Cursor, Write};
    use std::sync::Arc;

    use nexus_net::buf::ReadBuf;
    use nexus_net::tls::{TlsCodec, TlsConfig};

    fn generate_self_signed() -> (Vec<rustls::pki_types::CertificateDer<'static>>, Vec<u8>) {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("cert generation");
        (
            vec![rustls::pki_types::CertificateDer::from(
                cert.cert.der().to_vec(),
            )],
            cert.key_pair.serialize_der(),
        )
    }

    fn connected_pair() -> (TlsCodec, rustls::ServerConnection) {
        let (cert_chain, key_der) = generate_self_signed();
        let key = rustls::pki_types::PrivateKeyDer::try_from(key_der).unwrap();
        let server_config = Arc::new(
            rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(cert_chain, key)
                .unwrap(),
        );
        let mut server = rustls::ServerConnection::new(server_config).unwrap();

        let client_config = TlsConfig::builder().danger_no_verify().build().unwrap();
        let mut client = TlsCodec::new(&client_config, "localhost").unwrap();

        let mut c2s = Vec::new();
        let mut s2c = Vec::new();

        for _ in 0..64 {
            while client.wants_write() {
                client.write_tls_to(&mut c2s).unwrap();
            }

            if !c2s.is_empty() {
                server.read_tls(&mut Cursor::new(&c2s)).unwrap();
                server.process_new_packets().unwrap();
                c2s.clear();
            }

            while server.wants_write() {
                server.write_tls(&mut s2c).unwrap();
            }

            if !s2c.is_empty() {
                client.read_and_process_tls(&s2c).unwrap();
                s2c.clear();
            }

            if !client.is_handshaking() && !server.is_handshaking() {
                return (client, server);
            }
        }

        panic!("TLS handshake did not complete");
    }

    fn encrypt_server_payload(server: &mut rustls::ServerConnection, payload: &[u8]) -> Vec<u8> {
        server.writer().write_all(payload).unwrap();

        let mut ciphertext = Vec::new();
        while server.wants_write() {
            server.write_tls(&mut ciphertext).unwrap();
        }
        ciphertext
    }

    /// Mirror of the adapter's `poll_read` loop, exercised against a
    /// real connected codec pair: drain plaintext → step pending
    /// ciphertext → pull more ciphertext.
    ///
    /// Uses 32 KiB chunks — deliberately oversized vs the 8 KiB tmp
    /// the adapter currently uses. At the adapter's current tmp size
    /// this test passes regardless of which helper drives the loop
    /// (8 KiB stays under rustls's plaintext queue cap). The
    /// codec-level pin for the bug lives at
    /// `nexus_net::tls::codec::tests::adapter_pattern_with_read_and_process_tls_overflows_on_oversize_chunks`
    /// — that proves `read_tls_step` is the correct primitive. This
    /// adapter test guards against future tmp-size tuning
    /// re-introducing the bug here at the integration layer.
    #[test]
    fn pending_read_flow_drains_plaintext_before_more_ciphertext() {
        let (mut client, mut server) = connected_pair();
        let payload = vec![b'x'; 64 * 1024];
        let ciphertext = encrypt_server_payload(&mut server, &payload);

        let chunk_size = 32 * 1024;
        let mut pending_read = ReadBuf::with_capacity(chunk_size);
        let mut plaintext = Vec::with_capacity(payload.len());
        let mut offset = 0;
        let mut dst = [0u8; 1024];

        for _ in 0..1_000_000 {
            let n = client.read_plaintext(&mut dst).unwrap();
            if n > 0 {
                plaintext.extend_from_slice(&dst[..n]);
                if plaintext.len() == payload.len() {
                    break;
                }
                continue;
            }

            if !pending_read.is_empty() {
                let consumed = client.read_tls_step(pending_read.data()).unwrap();
                pending_read.advance(consumed);
                continue;
            }

            if offset < ciphertext.len() {
                let end = (offset + chunk_size).min(ciphertext.len());
                let chunk = &ciphertext[offset..end];
                let consumed = client.read_tls_step(chunk).unwrap();
                if consumed < chunk.len() {
                    let rem = &chunk[consumed..];
                    let spare = pending_read.spare();
                    spare[..rem.len()].copy_from_slice(rem);
                    pending_read.filled(rem.len());
                }
                offset = end;
                continue;
            }

            break;
        }

        assert_eq!(plaintext, payload);
        assert_eq!(offset, ciphertext.len());
        assert!(pending_read.is_empty());
    }
}
