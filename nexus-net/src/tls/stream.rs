//! TLS stream wrapper — implements `Read + Write` over the sans-IO codec.
//!
//! Wraps a transport stream `S` and a [`TlsCodec`] into a single type
//! that transparently encrypts/decrypts. Protocol clients (`ws::Client`,
//! `rest::Client`) are generic over `S` — when `S = TlsStream<TcpStream>`,
//! TLS is handled transparently with zero branching in the protocol layer.
//!
//! ```text
//! Client<TlsStream<TcpStream>>   — encrypted
//! Client<TcpStream>              — plaintext
//! ```

use std::io::{self, Read, Write};

#[cfg(feature = "tokio")]
use crate::buf::{ReadBuf, WriteBuf};

use super::codec::TlsCodec;

/// Per-poll TLS read chunk size used by the tokio adapter's
/// `poll_read`. Module-level const so it can be used in struct field
/// types; re-exposed publicly as [`TlsStream::TMP_SIZE`].
#[cfg(feature = "tokio")]
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
#[cfg(feature = "tokio")]
const _: () = assert!(
    TMP_SIZE <= 16 * 1024,
    "TMP_SIZE > 16 KiB requires handshake-piggyback fix (0.7.0)"
);

/// A stream that transparently encrypts and decrypts via [`TlsCodec`].
///
/// Implements `Read` and `Write` by routing through the TLS codec.
/// The inner stream `S` carries raw ciphertext; callers see plaintext.
///
/// # Shutdown
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
/// # Memory
///
/// Each TLS-wrapped connection on the tokio adapter allocates
/// approximately 81 KiB of heap-resident buffers:
///
/// | Buffer | Size | Purpose |
/// |---|---|---|
/// | `pending_read` | 8 KiB | Spillover for partially-consumed inbound TLS records |
/// | `pending_write` | 64 KiB | Outbound ciphertext FIFO (drains to socket) |
/// | `tmp` | 8 KiB | Per-connection scratch buffer for transport reads |
/// | rustls state | ~1 KiB | Crypto state + small fixed buffers |
///
/// Trading workloads with small frequent messages can reduce
/// `pending_write` via [`with_capacities`](Self::with_capacities) —
/// 8–16 KiB is sufficient for most order-entry and market-data
/// clients. For 1000 connections with default sizing, expect
/// ~81 MiB of buffer footprint.
pub struct TlsStream<S> {
    stream: S,
    codec: TlsCodec,
    /// Ciphertext read from the transport but not yet accepted by rustls.
    /// Used by the tokio adapter to interleave plaintext draining with
    /// ciphertext stepping (avoids `received plaintext buffer full` on
    /// large steady-state app data bursts).
    #[cfg(feature = "tokio")]
    pending_read: ReadBuf,
    /// Ciphertext waiting to be flushed to the transport. Cursor-based
    /// FIFO — no per-write memmove (auto-resets on full drain).
    #[cfg(feature = "tokio")]
    pending_write: WriteBuf,
    /// Per-poll scratch buffer for the tokio adapter's `poll_read`.
    /// Boxed so the 8 KiB stays off the per-poll stack frame —
    /// eliminates a per-poll memset + stack-probe pair on the hot
    /// read path.
    #[cfg(feature = "tokio")]
    tmp: Box<[u8; TMP_SIZE]>,
}

impl<S> TlsStream<S> {
    /// Per-poll TLS read chunk size used by the tokio adapter's
    /// `poll_read`. `pending_read` capacity must be at least this
    /// large so the spillover-copy after a partial codec read fits.
    /// Only present under `feature = "tokio"` — non-tokio builds
    /// don't have the buffers this sizes.
    #[cfg(feature = "tokio")]
    pub const TMP_SIZE: usize = TMP_SIZE;

    /// Default capacity for the outbound ciphertext buffer used by
    /// the tokio adapter.
    ///
    /// 64 KiB matches rustls's `DEFAULT_BUFFER_LIMIT` — the outbound
    /// plaintext queue cap. A 64 KiB plaintext encrypt produces about
    /// 64 KiB plus ~120 bytes of ciphertext (TLS record headers and
    /// auth tags), so a single max-size encrypt triggers exactly one
    /// drain/refill iteration in `poll_write`. Bumping this to 80 KiB
    /// would absorb the overhead in one shot but breaks the symmetric
    /// default; the drain/refill is cheap (non-blocking write) and the
    /// symmetry is the more discoverable choice.
    ///
    /// Larger writes are chunked across multiple `poll_write` calls
    /// via [`TlsCodec::try_encrypt`] regardless of this cap.
    #[cfg(feature = "tokio")]
    pub const DEFAULT_PENDING_WRITE_CAPACITY: usize = 65_536;

    /// Wrap a transport stream with a TLS codec, using default
    /// buffer capacities for the tokio adapter (`TMP_SIZE` for
    /// `pending_read`, `DEFAULT_PENDING_WRITE_CAPACITY` for
    /// `pending_write`).
    ///
    /// The codec should already be constructed with the correct hostname.
    /// Call [`handshake`](Self::handshake) (sync) or
    /// [`handshake_async`](Self::handshake_async) (tokio) before
    /// reading or writing plaintext.
    ///
    /// For custom buffer sizing on tokio, use
    /// [`with_capacities`](Self::with_capacities).
    pub fn new(stream: S, codec: TlsCodec) -> Self {
        Self {
            stream,
            codec,
            #[cfg(feature = "tokio")]
            pending_read: ReadBuf::with_capacity(Self::TMP_SIZE),
            #[cfg(feature = "tokio")]
            pending_write: WriteBuf::new(Self::DEFAULT_PENDING_WRITE_CAPACITY, 0),
            // Heap-allocated, lives for the connection's lifetime. Earlier
            // versions stack-allocated this per `poll_read`; the per-poll
            // memset + stack probe was a measurable cost on the steady-state
            // hot path. For long-lived TLS connections the alloc amortises
            // over millions of polls.
            #[cfg(feature = "tokio")]
            tmp: Box::new([0u8; TMP_SIZE]),
        }
    }

    /// Wrap a transport stream with explicit buffer capacities for
    /// the tokio adapter.
    ///
    /// `pending_read_cap` holds ciphertext read from the transport
    /// but not yet accepted by rustls. **Must be at least
    /// [`TMP_SIZE`](Self::TMP_SIZE)** — the per-poll read chunk size.
    /// 8 KiB suffices for any well-formed TLS stream; oversize is
    /// harmless waste.
    ///
    /// `pending_write_cap` holds ciphertext rustls has produced but
    /// not yet flushed to the transport. The drain loop in
    /// `poll_write` handles arbitrary plaintext sizes regardless of
    /// this capacity, but smaller capacities mean more drain/refill
    /// cycles for big writes. 64 KiB amortises a single 64 KiB
    /// plaintext encrypt; trading workloads with small messages can
    /// use 8–16 KiB to reduce per-connection footprint.
    ///
    /// # Panics
    /// Panics if `pending_read_cap < TMP_SIZE`.
    #[cfg(feature = "tokio")]
    pub fn with_capacities(
        stream: S,
        codec: TlsCodec,
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
            // Heap-allocated; see comment in `new()`.
            tmp: Box::new([0u8; TMP_SIZE]),
        }
    }

    /// Access the underlying transport stream.
    pub fn stream(&self) -> &S {
        &self.stream
    }

    /// Mutable access to the underlying transport stream.
    pub fn stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }

    /// Access the TLS codec.
    pub fn codec(&self) -> &TlsCodec {
        &self.codec
    }

    /// Mutable access to the TLS codec.
    pub fn codec_mut(&mut self) -> &mut TlsCodec {
        &mut self.codec
    }

    /// Decompose into the inner stream and codec.
    pub fn into_parts(self) -> (S, TlsCodec) {
        (self.stream, self.codec)
    }

    /// Set rustls's outbound plaintext queue limit. Convenience
    /// pass-through to [`TlsCodec::set_buffer_limit`].
    ///
    /// Default is rustls's `DEFAULT_BUFFER_LIMIT = 64 KiB`. Bulk-
    /// transfer workloads (large snapshots, file uploads over TLS)
    /// may benefit from raising it to reduce drain/refill cycles in
    /// `poll_write`. `None` for unlimited (caller is responsible for
    /// not encrypting more than memory allows).
    pub fn set_buffer_limit(&mut self, limit: Option<usize>) {
        self.codec.set_buffer_limit(limit);
    }
}

impl<S: Read + Write> TlsStream<S> {
    /// Drive the TLS handshake to completion (blocking).
    ///
    /// Call once after construction, before any read/write.
    pub fn handshake(&mut self) -> Result<(), super::TlsError> {
        while self.codec.is_handshaking() {
            while self.codec.wants_write() {
                self.codec.write_tls_to(&mut self.stream)?;
            }
            if self.codec.wants_read() {
                // Sync path: hand rustls a `Read` directly so it drives
                // the per-call read internally. The async equivalent
                // (`handshake_async`) buffers bytes from the async
                // transport into a tmp slice first, then feeds them
                // via `read_and_process_tls`.
                self.codec.read_tls_from(&mut self.stream)?;
                self.codec.process_new_packets()?;
            }
        }
        // Flush any remaining handshake data.
        while self.codec.wants_write() {
            self.codec.write_tls_to(&mut self.stream)?;
        }
        Ok(())
    }
}

// =============================================================================
// Read + Write — blocking path
// =============================================================================

impl<S: Read + Write> Read for TlsStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Try reading plaintext that's already buffered.
        let n = self.codec.read_plaintext(buf).map_err(tls_to_io)?;
        if n > 0 {
            return Ok(n);
        }

        // Need more ciphertext from the transport.
        // TLS may consume records without producing plaintext (session
        // tickets, key updates). Loop until we get plaintext or EOF.
        loop {
            let tls_n = self.codec.read_tls_from(&mut self.stream)?;
            if tls_n == 0 {
                return Ok(0); // EOF
            }
            self.codec.process_new_packets().map_err(tls_to_io)?;
            let n = self.codec.read_plaintext(buf).map_err(tls_to_io)?;
            if n > 0 {
                return Ok(n);
            }
        }
    }
}

impl<S: Read + Write> Write for TlsStream<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Sync `Write` is all-or-nothing by trait contract, so the
        // deprecated `encrypt` is the right primitive here — async
        // adapters use `try_encrypt` to surface partial acceptance to
        // the runtime, which sync Write doesn't model.
        #[allow(deprecated)]
        self.codec.encrypt(buf).map_err(tls_to_io)?;
        while self.codec.wants_write() {
            self.codec.write_tls_to(&mut self.stream)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        while self.codec.wants_write() {
            self.codec.write_tls_to(&mut self.stream)?;
        }
        self.stream.flush()
    }
}

// =============================================================================
// AsyncRead + AsyncWrite — tokio path
// =============================================================================

#[cfg(feature = "tokio")]
impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> TlsStream<S> {
    /// Drive the TLS handshake to completion asynchronously (tokio).
    ///
    /// Call once after construction, before any read/write.
    pub async fn handshake_async(&mut self) -> Result<(), super::TlsError> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut tmp = [0u8; 8192];
        while self.codec.is_handshaking() {
            while self.codec.wants_write() {
                let n = self.codec.write_tls_to(&mut tmp.as_mut_slice())?;
                self.stream.write_all(&tmp[..n]).await?;
            }
            self.stream.flush().await?;
            if self.codec.wants_read() {
                let n = self.stream.read(&mut tmp).await?;
                if n == 0 {
                    return Err(super::TlsError::Io(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "connection closed during TLS handshake",
                    )));
                }
                self.codec.read_and_process_tls(&tmp[..n])?;
            }
        }
        while self.codec.wants_write() {
            let n = self.codec.write_tls_to(&mut tmp.as_mut_slice())?;
            self.stream.write_all(&tmp[..n]).await?;
        }
        self.stream.flush().await?;
        Ok(())
    }
}

#[cfg(feature = "tokio")]
impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> tokio::io::AsyncRead
    for TlsStream<S>
{
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        let this = self.get_mut();

        loop {
            // 1. Drain any plaintext already decrypted by rustls.
            let slice = buf.initialize_unfilled();
            let n = this.codec.read_plaintext(slice).map_err(tls_to_io)?;
            if n > 0 {
                buf.advance(n);
                return std::task::Poll::Ready(Ok(()));
            }

            // 2. If we have ciphertext from a previous step that didn't
            //    fully consume, advance one packet step on it.
            if !this.pending_read.is_empty() {
                let consumed = this
                    .codec
                    .read_tls_step(this.pending_read.data())
                    .map_err(tls_to_io)?;
                // State invariant: every error leg above this line MUST
                // return before reaching here. If you add new error
                // returns, place them BEFORE this side-effect — pending_read
                // can be left inconsistent if advance() is half-applied.
                this.pending_read.advance(consumed);
                continue;
            }

            // 3. No buffered ciphertext — pull more from the transport.
            //    Use the heap-resident scratch buffer so the 8 KiB
            //    doesn't sit on every poll_read stack frame.
            let filled = {
                let mut tmp_buf = tokio::io::ReadBuf::new(&mut this.tmp[..]);
                match std::pin::Pin::new(&mut this.stream).poll_read(cx, &mut tmp_buf) {
                    std::task::Poll::Ready(Ok(())) => tmp_buf.filled().len(),
                    std::task::Poll::Ready(Err(e)) => {
                        return std::task::Poll::Ready(Err(e));
                    }
                    std::task::Poll::Pending => return std::task::Poll::Pending,
                }
            };
            if filled == 0 {
                return std::task::Poll::Ready(Ok(())); // EOF
            }
            let consumed = this
                .codec
                .read_tls_step(&this.tmp[..filled])
                .map_err(tls_to_io)?;
            if consumed < filled {
                let rem_len = filled - consumed;
                let spare = this.pending_read.spare();
                spare[..rem_len].copy_from_slice(&this.tmp[consumed..filled]);
                // State invariant: every error leg above this line MUST
                // return before reaching here. If you add new error
                // returns, place them BEFORE this side-effect — pending_read
                // can be left inconsistent if filled() is half-applied.
                this.pending_read.filled(rem_len);
            }
            // Loop back: drain plaintext or step pending_read.
        }
    }
}

#[cfg(feature = "tokio")]
impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite
    for TlsStream<S>
{
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        let this = self.get_mut();

        // 1. Drain pending ciphertext to free pending_write space.
        if let Err(e) = drain_pending(this, cx) {
            return std::task::Poll::Ready(Err(e));
        }
        if !this.pending_write.is_empty() {
            // Socket can't take more — backpressure surfaces here.
            return std::task::Poll::Pending;
        }

        // 2. Pull queued ciphertext from rustls into pending_write and
        //    on to the socket. Frees rustls's plaintext queue so
        //    try_encrypt has room for new bytes.
        if let Err(e) = drain_codec_to_pending(this, cx) {
            return std::task::Poll::Ready(Err(e));
        }
        if let Err(e) = drain_pending(this, cx) {
            return std::task::Poll::Ready(Err(e));
        }
        if !this.pending_write.is_empty() {
            return std::task::Poll::Pending;
        }

        // 3. Encrypt as much of buf as rustls's queue can accept.
        //    Chunked: returns Ok(0) if the queue is full and the caller
        //    must come back later (after socket drains more).
        let consumed = this.codec.try_encrypt(buf).map_err(tls_to_io)?;
        if consumed == 0 {
            // Defensive: rustls should not return 0 here after we've
            // drained both its outbound queue and the socket. If it
            // does (rustls bug or edge case), wake_by_ref ensures the
            // runtime re-polls us instead of stalling indefinitely.
            cx.waker().wake_by_ref();
            return std::task::Poll::Pending;
        }

        // 4. Best-effort flush of what we just produced.
        if let Err(e) = drain_codec_to_pending(this, cx) {
            return std::task::Poll::Ready(Err(e));
        }
        if let Err(e) = drain_pending(this, cx) {
            return std::task::Poll::Ready(Err(e));
        }

        std::task::Poll::Ready(Ok(consumed))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        let this = self.get_mut();

        // Drain any remaining rustls ciphertext into pending_write.
        if let Err(e) = drain_codec_to_pending(this, cx) {
            return std::task::Poll::Ready(Err(e));
        }

        // Flush pending ciphertext to the stream.
        if let Err(e) = drain_pending(this, cx) {
            return std::task::Poll::Ready(Err(e));
        }
        if !this.pending_write.is_empty() {
            return std::task::Poll::Pending;
        }

        std::pin::Pin::new(&mut this.stream).poll_flush(cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        let this = self.get_mut();

        // 1. Queue close_notify (idempotent — rustls no-ops on dupes,
        //    so this loop re-entering after Pending is safe).
        this.codec.send_close_notify();

        // 2. Drain rustls's queue (now including close_notify
        //    ciphertext) into pending_write.
        if let Err(e) = drain_codec_to_pending(this, cx) {
            return std::task::Poll::Ready(Err(e));
        }

        // 3. Flush pending_write to the transport. If we can't fully
        //    drain yet, wait for the next poll.
        if let Err(e) = drain_pending(this, cx) {
            return std::task::Poll::Ready(Err(e));
        }
        if !this.pending_write.is_empty() {
            return std::task::Poll::Pending;
        }

        // 4. Now safe to shutdown the transport.
        std::pin::Pin::new(&mut this.stream).poll_shutdown(cx)
    }
}

/// Drain pending ciphertext to the underlying tokio stream. Handles partial
/// writes by advancing the buffer cursor (auto-resets on full drain).
#[cfg(feature = "tokio")]
fn drain_pending<S: tokio::io::AsyncWrite + Unpin>(
    this: &mut TlsStream<S>,
    cx: &mut std::task::Context<'_>,
) -> io::Result<()> {
    while !this.pending_write.is_empty() {
        match std::pin::Pin::new(&mut this.stream).poll_write(cx, this.pending_write.data()) {
            std::task::Poll::Ready(Ok(0)) => {
                return Err(io::Error::new(io::ErrorKind::WriteZero, "write returned 0"));
            }
            std::task::Poll::Ready(Ok(n)) => {
                this.pending_write.advance(n);
            }
            std::task::Poll::Ready(Err(e)) => return Err(e),
            std::task::Poll::Pending => return Ok(()), // Will be retried next poll.
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
#[cfg(feature = "tokio")]
fn drain_codec_to_pending<S: tokio::io::AsyncWrite + Unpin>(
    this: &mut TlsStream<S>,
    cx: &mut std::task::Context<'_>,
) -> io::Result<()> {
    while this.codec.wants_write() {
        if this.pending_write.spare().is_empty() {
            // Backpressure: try to drain to free space.
            drain_pending(this, cx)?;
            if this.pending_write.spare().is_empty() {
                // Socket can't take more right now. Remaining ciphertext
                // stays queued in rustls until the next poll re-enters.
                return Ok(());
            }
        }
        let n = this.codec.write_tls_to(&mut this.pending_write.spare())?;
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
        this.pending_write.filled(n);
        drain_pending(this, cx)?;
    }
    Ok(())
}

// =============================================================================
// Helpers
// =============================================================================

fn tls_to_io(e: super::TlsError) -> io::Error {
    match e {
        super::TlsError::Io(io) => io,
        other => io::Error::other(other),
    }
}

#[cfg(all(test, feature = "tokio"))]
mod tests {
    use super::*;
    use crate::tls::TlsConfig;

    fn make_codec() -> TlsCodec {
        let cfg = TlsConfig::builder().danger_no_verify().build().unwrap();
        TlsCodec::new(&cfg, "localhost").unwrap()
    }

    #[test]
    fn with_capacities_at_minimum_succeeds() {
        // pending_read_cap == TMP_SIZE is the minimum allowed; must
        // construct without panicking.
        let _ = TlsStream::with_capacities(
            (),
            make_codec(),
            TlsStream::<()>::TMP_SIZE,
            TlsStream::<()>::DEFAULT_PENDING_WRITE_CAPACITY,
        );
    }

    #[test]
    fn new_uses_default_capacities() {
        // Smoke test: default constructor must build successfully.
        let _ = TlsStream::new((), make_codec());
    }

    #[test]
    #[should_panic(expected = "TMP_SIZE")]
    fn with_capacities_panics_on_undersized_pending_read() {
        // Anything below TMP_SIZE breaks the spillover-copy invariant
        // in poll_read.
        let _ = TlsStream::with_capacities(
            (),
            make_codec(),
            TlsStream::<()>::TMP_SIZE - 1,
            TlsStream::<()>::DEFAULT_PENDING_WRITE_CAPACITY,
        );
    }
}
