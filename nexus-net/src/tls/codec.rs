use std::io::{self, Read, Write};

use rustls::ClientConnection;
use rustls::pki_types::ServerName;

use super::{TlsConfig, TlsError};
use crate::ws::FrameReader;

/// Sans-IO TLS codec. Decrypts inbound bytes, encrypts outbound bytes.
///
/// Wraps a rustls `ClientConnection` with an API shaped for nexus-net:
/// feed raw TLS bytes in, get plaintext into a [`FrameReader`]; encrypt
/// plaintext from a [`FrameWriter`](crate::ws::FrameWriter) and flush to a socket.
///
/// # Usage
///
/// ```ignore
/// let config = TlsConfig::new()?;
/// let mut tls = TlsCodec::new(&config, "exchange.com")?;
///
/// // Handshake
/// while tls.is_handshaking() {
///     tls.write_tls_to(&mut socket)?;
///     tls.read_tls_from(&mut socket)?;
///     tls.process_new_packets()?;
/// }
///
/// // Steady state
/// tls.read_tls_from(&mut socket)?;
/// tls.process_into(&mut reader)?;
/// // ... reader.next() ...
/// ```
pub struct TlsCodec {
    inner: ClientConnection,
}

impl TlsCodec {
    /// Create a new TLS codec for the given hostname.
    ///
    /// The hostname is used for SNI (Server Name Indication) and
    /// certificate verification.
    pub fn new(config: &TlsConfig, hostname: &str) -> Result<Self, TlsError> {
        let server_name = ServerName::try_from(hostname.to_owned())
            .map_err(|_| TlsError::InvalidHostname(hostname.to_owned()))?;

        let conn = ClientConnection::new(config.inner.clone(), server_name)?;

        Ok(Self { inner: conn })
    }

    // =========================================================================
    // Inbound (socket → TLS → FrameReader)
    // =========================================================================

    /// Feed raw TLS bytes from a byte slice (sans-IO path).
    ///
    /// Returns the number of bytes consumed. **May be less than
    /// `src.len()`** — rustls's deframer can require a
    /// [`process_new_packets`](Self::process_new_packets) call before
    /// accepting more bytes.
    ///
    /// For an overview of when to use each input primitive, see the
    /// [module-level docs](crate::tls). Most callers want
    /// [`read_tls_step`](Self::read_tls_step) (streaming app-data) or
    /// [`read_and_process_tls`](Self::read_and_process_tls) (bounded
    /// handshake input). Reach for `read_tls` directly only when
    /// implementing a new adapter shape that none of those fit.
    #[deprecated(
        since = "0.6.2",
        note = "use `read_tls_step` (streaming) or `read_and_process_tls` \
                (bounded handshake) — these encode rustls's partial-consumption \
                failure mode that bare `read_tls` does not"
    )]
    pub fn read_tls(&mut self, src: &[u8]) -> Result<usize, TlsError> {
        let mut cursor = io::Cursor::new(src);
        Ok(self.inner.read_tls(&mut cursor)?)
    }

    /// Advance the codec by a single TLS packet step: one
    /// [`read_tls`](Self::read_tls) + [`process_new_packets`](Self::process_new_packets)
    /// pair.
    ///
    /// Returns the number of ciphertext bytes consumed from `src`. The
    /// caller must drain any plaintext (via
    /// [`read_plaintext`](Self::read_plaintext) or
    /// [`process_into`](Self::process_into)) before calling again —
    /// feeding more ciphertext while plaintext is queued can overflow
    /// rustls's internal plaintext buffer.
    ///
    /// Use this for streaming application data where the caller
    /// interleaves ciphertext input with plaintext output (the standard
    /// async TLS adapter shape: poll socket → step codec → drain
    /// plaintext → repeat). For bounded input where the caller can
    /// tolerate plaintext staying inside rustls until the full slice is
    /// consumed (TLS handshakes, in-memory tests), use
    /// [`read_and_process_tls`](Self::read_and_process_tls) instead.
    ///
    /// # Returns
    ///
    /// `Ok(0)` if `src` is empty. Otherwise `Ok(n)` where `n > 0` is the
    /// number of bytes consumed (always `<= src.len()`; rustls's
    /// deframer caps each call at its internal `READ_SIZE`).
    ///
    /// # Errors
    ///
    /// Two distinct error sources:
    ///
    /// - **Propagated from rustls** via `?`: any error from
    ///   [`read_tls`](Self::read_tls) (including
    ///   `received plaintext buffer full` when the caller hasn't
    ///   drained plaintext between steps) or
    ///   [`process_new_packets`](Self::process_new_packets) (alerts,
    ///   decryption failures, protocol violations).
    /// - **`TlsError::Io(InvalidData)` from this method**: rustls's
    ///   deframer accepted 0 bytes from a non-empty input slice
    ///   without erroring — meaning it cannot complete a record from
    ///   the bytes provided alone (partial record, or the caller
    ///   misused the API by passing zero-byte progress repeatedly).
    #[inline]
    pub fn read_tls_step(&mut self, src: &[u8]) -> Result<usize, TlsError> {
        if src.is_empty() {
            return Ok(0);
        }
        // Internal use of the deprecated primitive. The deprecation
        // warns external callers; we know the failure mode and are
        // wrapping it correctly here.
        #[allow(deprecated)]
        let consumed = self.read_tls(src)?;
        if consumed == 0 {
            return Err(TlsError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "TLS codec made no progress on non-empty input \
                 (deframer cannot complete a record from the provided bytes)",
            )));
        }
        self.process_new_packets()?;
        Ok(consumed)
    }

    /// Feed buffered TLS bytes through rustls, looping until the entire
    /// slice is consumed.
    ///
    /// **Use only for bounded input** where the caller can tolerate
    /// plaintext staying queued inside rustls until the helper returns
    /// — TLS handshakes (the original motivation, see issue #200) and
    /// in-memory tests. Do **not** use for streaming application data:
    /// rustls's internal plaintext buffer has a cap, and feeding a
    /// large ciphertext slice without giving the caller a chance to
    /// drain plaintext between steps will overflow it
    /// (`received plaintext buffer full`). For streaming adapters, use
    /// [`read_tls_step`](Self::read_tls_step) instead — feed at most
    /// one accepted prefix, return plaintext to the caller, then feed
    /// more.
    ///
    /// Sync paths reading directly from a [`Read`](std::io::Read)
    /// source should use [`read_tls_from`](Self::read_tls_from)
    /// instead. Each call reads up to rustls's internal `READ_SIZE`
    /// (4 KiB) from the source and buffers it; the caller pairs it
    /// with [`process_new_packets`](Self::process_new_packets) and
    /// re-drives in their own loop, pulling more bytes from the
    /// transport as available.
    ///
    /// # Why a loop is required
    ///
    /// `rustls::Connection::read_tls` is not guaranteed to consume the
    /// full provided slice on a single call. It may consume part, return
    /// that count, and require [`process_new_packets`](Self::process_new_packets)
    /// before accepting more. Calling `read_tls(&buf)` once and ignoring
    /// the returned consumed count silently drops the unconsumed tail
    /// (issue #200 — a TLS handshake against a server that splits its
    /// response into multiple records inside a single TCP segment fails
    /// because the unconsumed bytes vanish).
    ///
    /// # Returns
    ///
    /// `Ok(src.len())` when the entire slice has been consumed and
    /// processed.
    ///
    /// # Errors
    ///
    /// - `TlsError::Io(InvalidData)` if rustls's deframer can't make
    ///   progress (returns 0 bytes consumed) despite the prior
    ///   `process_new_packets` call. Indicates a malformed or hostile
    ///   TLS stream.
    /// - Any error returned by [`read_tls`](Self::read_tls) or
    ///   [`process_new_packets`](Self::process_new_packets).
    pub fn read_and_process_tls(&mut self, src: &[u8]) -> Result<usize, TlsError> {
        let mut consumed = 0;
        while consumed < src.len() {
            // Internal use of the deprecated primitive — the loop here
            // is what makes it safe to call.
            #[allow(deprecated)]
            let n = self.read_tls(&src[consumed..])?;
            if n == 0 {
                return Err(TlsError::Io(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "TLS codec stopped before consuming buffered input \
                     (rustls deframer cannot make progress)",
                )));
            }
            consumed += n;
            self.process_new_packets()?;
        }
        Ok(consumed)
    }

    /// Read raw TLS bytes from a socket.
    ///
    /// Returns the number of bytes read, or 0 on EOF.
    pub fn read_tls_from<R: Read>(&mut self, src: &mut R) -> io::Result<usize> {
        self.inner.read_tls(src)
    }

    /// Process buffered TLS records (decrypt).
    ///
    /// Call after [`read_tls`](Self::read_tls) or
    /// [`read_tls_from`](Self::read_tls_from) to decrypt any
    /// complete TLS records. This does not produce plaintext
    /// directly — call [`process_into`](Self::process_into) or
    /// [`read_plaintext`](Self::read_plaintext) afterwards.
    pub fn process_new_packets(&mut self) -> Result<(), TlsError> {
        self.inner.process_new_packets()?;
        Ok(())
    }

    /// Decrypt buffered TLS records and feed plaintext into a FrameReader.
    ///
    /// Combines [`process_new_packets`](Self::process_new_packets) and
    /// a read into the FrameReader in one call. Returns the number of
    /// plaintext bytes fed.
    pub fn process_into(&mut self, reader: &mut FrameReader) -> Result<usize, TlsError> {
        self.inner.process_new_packets()?;

        // Use BufRead::fill_buf to avoid ChunkVecBuffer::read overhead.
        // fill_buf returns a reference to buffered plaintext — one fewer
        // copy than Read::read which copies into an intermediate buffer.
        let mut rd = self.inner.reader();
        let chunk = match std::io::BufRead::fill_buf(&mut rd) {
            Ok(chunk) => chunk,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(0),
            Err(e) => return Err(TlsError::Io(e)),
        };
        if chunk.is_empty() {
            return Ok(0);
        }
        let n = chunk.len();
        if let Err(e) = reader.read(chunk) {
            return Err(TlsError::Io(io::Error::other(format!(
                "FrameReader buffer full: {e}"
            ))));
        }
        std::io::BufRead::consume(&mut rd, n);
        Ok(n)
    }

    /// Read decrypted plaintext into a buffer (sans-IO path).
    ///
    /// For users who want to feed bytes into FrameReader manually
    /// or use a different parser.
    #[inline]
    pub fn read_plaintext(&mut self, dst: &mut [u8]) -> Result<usize, TlsError> {
        match self.inner.reader().read(dst) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(TlsError::Io(e)),
        }
    }

    // =========================================================================
    // Outbound (FrameWriter → TLS → socket)
    // =========================================================================

    /// Encrypt plaintext for sending (all-or-nothing).
    ///
    /// The encrypted bytes are buffered internally. Call
    /// [`write_tls_to`](Self::write_tls_to) to flush them to a socket.
    ///
    /// **Errors with `WriteZero` if `plaintext.len()` exceeds rustls's
    /// outbound plaintext queue limit** (default 64 KiB; see
    /// [`set_buffer_limit`](Self::set_buffer_limit)). For streaming
    /// adapters where the caller may pass arbitrarily-large buffers
    /// (the standard `AsyncWrite::poll_write` shape), use the chunked
    /// [`try_encrypt`](Self::try_encrypt) instead and let the caller
    /// chunk via `write_all`.
    #[deprecated(
        since = "0.6.2",
        note = "use `try_encrypt` — it returns the accepted byte count instead \
                of erroring when rustls's plaintext queue is full, and is the \
                correct primitive for `AsyncWrite::poll_write`"
    )]
    #[inline]
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<(), TlsError> {
        self.inner.writer().write_all(plaintext)?;
        Ok(())
    }

    /// Encrypt up to `plaintext.len()` bytes, returning the number of
    /// bytes actually accepted by rustls's outbound plaintext queue.
    ///
    /// This is the chunked variant of [`encrypt`](Self::encrypt). Use
    /// it when the caller may pass more bytes than rustls's queue
    /// limit (default `DEFAULT_BUFFER_LIMIT = 64 KiB`; tune with
    /// [`set_buffer_limit`](Self::set_buffer_limit)). Returning the
    /// accepted count lets the caller chunk subsequent writes — the
    /// standard `AsyncWrite::poll_write` contract. Paired with
    /// [`encrypt`](Self::encrypt) for all-or-nothing semantics on
    /// known-small plaintexts.
    ///
    /// # Returns
    ///
    /// `Ok(0)` if rustls's queue is full and cannot accept any bytes
    /// (caller should drain ciphertext to the socket and retry).
    /// Otherwise `Ok(n)` where `n > 0` is the number of plaintext
    /// bytes queued for encryption. `n` may be less than
    /// `plaintext.len()`.
    ///
    /// # Errors
    ///
    /// Any error from rustls's writer other than `WriteZero` (which
    /// is translated to `Ok(0)` so the caller treats it as
    /// backpressure rather than a hard failure).
    #[inline]
    pub fn try_encrypt(&mut self, plaintext: &[u8]) -> Result<usize, TlsError> {
        match self.inner.writer().write(plaintext) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == io::ErrorKind::WriteZero => Ok(0),
            Err(e) => Err(TlsError::Io(e)),
        }
    }

    /// Set rustls's outbound plaintext queue limit. `None` for
    /// unlimited (rustls accepts as much plaintext as memory allows;
    /// pair with a caller-side bound).
    ///
    /// Default is rustls's `DEFAULT_BUFFER_LIMIT = 64 KiB`. Trading
    /// workloads with small messages typically don't need to change
    /// this. Bulk-transfer workloads (large snapshots, file uploads
    /// over TLS) may benefit from raising it to reduce drain/refill
    /// cycles in [`try_encrypt`](Self::try_encrypt).
    pub fn set_buffer_limit(&mut self, limit: Option<usize>) {
        self.inner.set_buffer_limit(limit);
    }

    /// Queue a TLS `close_notify` alert.
    ///
    /// Subsequent calls to [`wants_write`](Self::wants_write) will
    /// return true until the alert ciphertext has been written via
    /// [`write_tls_to`](Self::write_tls_to).
    ///
    /// Idempotent: rustls tracks whether close_notify has been sent
    /// and no-ops on duplicate calls.
    ///
    /// Use in `AsyncWrite::poll_shutdown` (or equivalent) before
    /// closing the underlying transport. Without close_notify, the
    /// peer sees TCP FIN as a potential truncation and may error its
    /// read loop mid-stream.
    #[inline]
    pub fn send_close_notify(&mut self) {
        self.inner.send_close_notify();
    }

    /// Flush encrypted bytes to a socket.
    ///
    /// Returns the number of bytes written. Call in a loop or when
    /// [`wants_write`](Self::wants_write) returns true.
    pub fn write_tls_to<W: Write>(&mut self, dst: &mut W) -> io::Result<usize> {
        self.inner.write_tls(dst)
    }

    // =========================================================================
    // State
    // =========================================================================

    /// Whether the TLS handshake is still in progress.
    #[inline]
    pub fn is_handshaking(&self) -> bool {
        self.inner.is_handshaking()
    }

    /// Whether the codec has buffered TLS data to read.
    #[inline]
    pub fn wants_read(&self) -> bool {
        self.inner.wants_read()
    }

    /// Whether the codec has encrypted data to write.
    #[inline]
    pub fn wants_write(&self) -> bool {
        self.inner.wants_write()
    }
}

impl std::fmt::Debug for TlsCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsCodec")
            .field("handshaking", &self.inner.is_handshaking())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use crate::buf::ReadBuf;

    use super::*;

    // -------------------------------------------------------------------------
    // In-memory handshake scaffolding (lifted from examples/perf_tls.rs).
    // -------------------------------------------------------------------------

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

    /// Generate an N-cert ECDSA-P256 chain whose serialized DER pushes
    /// the TLS 1.3 server's first handshake burst past rustls's
    /// `READ_SIZE = 4096` per-call deframer cap. ECDSA keygen is
    /// microseconds (vs RSA-4096's ~1.5s per key) so this stays cheap
    /// even at chain depth 10.
    ///
    /// Why a deep chain instead of one big RSA cert: chain depth scales
    /// the Certificate message linearly without paying for slow RSA
    /// keygen. 10 P-256 certs ≈ 5KB of cert bytes, comfortably over
    /// 4096. Each link is signed by its parent — a real CA-style chain.
    ///
    /// Returns `(chain_in_send_order, leaf_key_der)`. The chain is
    /// `[leaf, intermediate_n, ..., intermediate_1, root]` — the order
    /// rustls sends in the Certificate message.
    fn generate_oversize_ecdsa_chain() -> (Vec<rustls::pki_types::CertificateDer<'static>>, Vec<u8>)
    {
        use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair};

        const CHAIN_DEPTH: usize = 10;

        // Generate the root + intermediates + leaf. Each non-leaf is a
        // CA-flagged cert that signs the next link.
        let mut keys: Vec<KeyPair> = Vec::with_capacity(CHAIN_DEPTH);
        let mut certs: Vec<rcgen::Certificate> = Vec::with_capacity(CHAIN_DEPTH);

        // Root.
        let root_key = KeyPair::generate().expect("root key");
        let mut root_params = CertificateParams::new(Vec::<String>::new()).expect("root params");
        root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let root_cert = root_params.self_signed(&root_key).expect("root self-sign");
        keys.push(root_key);
        certs.push(root_cert);

        // Intermediates (CHAIN_DEPTH - 2 of them, all CA-flagged).
        for _ in 0..(CHAIN_DEPTH - 2) {
            let key = KeyPair::generate().expect("int key");
            let mut params = CertificateParams::new(Vec::<String>::new()).expect("int params");
            params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
            let parent_cert = certs.last().expect("parent");
            let parent_key = keys.last().expect("parent key");
            let cert = params
                .signed_by(&key, parent_cert, parent_key)
                .expect("int signed");
            keys.push(key);
            certs.push(cert);
        }

        // Leaf (signed by the deepest intermediate, SAN=localhost).
        let leaf_key = KeyPair::generate().expect("leaf key");
        let leaf_params =
            CertificateParams::new(vec!["localhost".to_string()]).expect("leaf params");
        let parent_cert = certs.last().expect("parent");
        let parent_key = keys.last().expect("parent key");
        let leaf_cert = leaf_params
            .signed_by(&leaf_key, parent_cert, parent_key)
            .expect("leaf signed");

        // Server sends [leaf, intermediates_descending, root] in the
        // Certificate message. We built `certs` as [root, int_1, ...,
        // int_n], so reverse + prepend leaf.
        let mut chain: Vec<rustls::pki_types::CertificateDer<'static>> =
            Vec::with_capacity(CHAIN_DEPTH);
        chain.push(rustls::pki_types::CertificateDer::from(
            leaf_cert.der().to_vec(),
        ));
        for cert in certs.iter().rev() {
            chain.push(rustls::pki_types::CertificateDer::from(cert.der().to_vec()));
        }

        (chain, leaf_key.serialize_der())
    }

    /// In-memory pipe for handshake bytes.
    struct MemPipe {
        buf: Vec<u8>,
    }

    impl MemPipe {
        fn new() -> Self {
            Self { buf: Vec::new() }
        }

        fn write_to(&mut self, data: &[u8]) {
            self.buf.extend_from_slice(data);
        }

        fn read_from(&mut self, dst: &mut [u8]) -> usize {
            let n = dst.len().min(self.buf.len());
            dst[..n].copy_from_slice(&self.buf[..n]);
            self.buf.drain(..n);
            n
        }

        fn len(&self) -> usize {
            self.buf.len()
        }
    }

    /// Build the server side and capture its first multi-record handshake
    /// burst (ServerHello + EncryptedExtensions + Certificate + CertVerify +
    /// Finished under TLS 1.3 — several records pushed back-to-back). The
    /// returned `server_out` is the slice we feed to the client `TlsCodec`
    /// to exercise the partial-consumption surface.
    fn setup_and_capture_server_burst(
        cert_chain: Vec<rustls::pki_types::CertificateDer<'static>>,
        key_der: Vec<u8>,
    ) -> (TlsCodec, rustls::ServerConnection, Vec<u8>) {
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

        let mut c2s = MemPipe::new();
        let mut s2c = MemPipe::new();

        // Client writes ClientHello.
        // Loop `while wants_write()` (mirroring the server side below)
        // for defense-in-depth — if a future rustls or cert config splits
        // the ClientHello across multiple write batches, a single
        // write_tls_to call would leave bytes pending in the codec.
        while client.wants_write() {
            let mut cursor = Cursor::new(Vec::new());
            client.write_tls_to(&mut cursor).unwrap();
            c2s.write_to(cursor.get_ref());
        }

        // Server consumes ClientHello.
        let mut tmp = vec![0u8; 16384];
        let n = c2s.read_from(&mut tmp);
        server
            .read_tls(&mut Cursor::new(&tmp[..n]))
            .expect("server reads ClientHello");
        server.process_new_packets().unwrap();

        // Server writes its multi-record burst.
        while server.wants_write() {
            let mut cursor = Cursor::new(Vec::new());
            server.write_tls(&mut cursor).unwrap();
            s2c.write_to(cursor.get_ref());
        }

        let mut server_out = vec![0u8; s2c.len()];
        let n = s2c.read_from(&mut server_out);
        assert!(n > 0, "server should have produced handshake bytes");
        server_out.truncate(n);

        (client, server, server_out)
    }

    // -------------------------------------------------------------------------
    // Tests
    // -------------------------------------------------------------------------

    /// Regression test for issue #200.
    ///
    /// Pre-fix: `read_tls(&buf)` may consume only part of `buf`. Calling
    /// code in nexus-async-net + nexus-net's tls/stream.rs ignored the
    /// returned consumed count, dropping the unconsumed tail and stalling
    /// the TLS handshake. Post-fix: `read_and_process_tls` loops until the
    /// entire slice is consumed.
    #[test]
    fn read_and_process_tls_consumes_full_slice() {
        let (chain, key) = generate_self_signed();
        let (mut client, _server, server_out) = setup_and_capture_server_burst(chain, key);

        let consumed = client
            .read_and_process_tls(&server_out)
            .expect("helper must consume the full slice");

        assert_eq!(
            consumed,
            server_out.len(),
            "helper must consume every byte (issue #200)"
        );
        assert!(
            client.wants_write(),
            "client should have produced its handshake response"
        );
    }

    /// Stricter exercise: feed the captured server bytes one byte per
    /// `read_and_process_tls` call. Catches a class of bugs where the
    /// helper itself drops bytes between calls or skips the
    /// `process_new_packets` step in some iterations.
    #[test]
    fn read_and_process_tls_byte_at_a_time() {
        let (chain, key) = generate_self_signed();
        let (mut client, _server, server_out) = setup_and_capture_server_burst(chain, key);

        for byte in &server_out {
            client
                .read_and_process_tls(std::slice::from_ref(byte))
                .expect("byte-at-a-time must succeed");
        }

        assert!(
            client.wants_write(),
            "client should have produced its handshake response \
             after byte-at-a-time consumption"
        );
    }

    /// **The actual end-to-end regression test for issue #200.**
    ///
    /// The other tests in this module either don't exercise the helper's
    /// multi-iteration loop (`read_and_process_tls_consumes_full_slice`
    /// uses a small burst that consumes in one inner iteration;
    /// `read_and_process_tls_byte_at_a_time` invokes the helper many times
    /// with 1-byte slices but each invocation has a 1-iteration loop),
    /// or test only rustls's contract without exercising our helper
    /// (`bare_read_tls_partially_consumes_large_slice`).
    ///
    /// This test uses a 10-cert ECDSA-P256 chain to push the server's
    /// first handshake burst past rustls's `READ_SIZE = 4096` per-call
    /// cap. Chain depth (not key size) provides the bytes — keeps
    /// keygen fast. The helper is fed the whole burst in ONE call; its
    /// internal loop must iterate multiple times to consume everything.
    /// This is exactly the shape birch hit against polymarket.
    #[test]
    fn read_and_process_tls_handles_oversize_burst() {
        let (chain, key) = generate_oversize_ecdsa_chain();
        let (mut client, _server, server_out) = setup_and_capture_server_burst(chain, key);

        // Confirm the test is actually exercising the partial-consumption
        // path. If this assertion fails, future contributors investigating
        // know the burst-size assumption broke (e.g., rustls raised
        // READ_SIZE, or the cert chain shrank). Bump the chain size or
        // the key size in `generate_oversize_ecdsa_chain` to restore.
        assert!(
            server_out.len() > 4096,
            "burst must exceed READ_SIZE to exercise multi-iteration loop, \
             got {} bytes — bump cert chain in generate_oversize_ecdsa_chain",
            server_out.len()
        );

        let consumed = client
            .read_and_process_tls(&server_out)
            .expect("helper must consume the full slice across multiple iterations");

        assert_eq!(
            consumed,
            server_out.len(),
            "helper must consume every byte across the multi-iteration loop \
             (issue #200 — the actual partial-consumption surface)"
        );
        assert!(
            client.wants_write(),
            "client should have produced its handshake response after \
             consuming the oversize burst"
        );
    }

    /// Drive an in-memory TLS 1.3 handshake to completion.
    /// Returns the connected client codec + server connection ready for
    /// app-data exchange. Used by `read_tls_step` tests that need a
    /// post-handshake codec.
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

    /// Encrypt `payload` from the server side and capture the resulting
    /// ciphertext.
    fn encrypt_server_payload(server: &mut rustls::ServerConnection, payload: &[u8]) -> Vec<u8> {
        use std::io::Write as _;
        server.writer().write_all(payload).unwrap();
        let mut ciphertext = Vec::new();
        while server.wants_write() {
            server.write_tls(&mut ciphertext).unwrap();
        }
        ciphertext
    }

    /// Empty input should be a cheap no-op, not an error.
    #[test]
    fn read_tls_step_empty_input_returns_zero() {
        let client_config = TlsConfig::builder().danger_no_verify().build().unwrap();
        let mut client = TlsCodec::new(&client_config, "localhost").unwrap();

        let n = client
            .read_tls_step(&[])
            .expect("empty input must not error");
        assert_eq!(n, 0);
    }

    /// Happy path: feed a small ciphertext prefix, get a non-zero
    /// consumed count back, drain the resulting plaintext.
    #[test]
    fn read_tls_step_normal_step() {
        let (mut client, mut server) = connected_pair();
        let payload = b"hello, world";
        let ciphertext = encrypt_server_payload(&mut server, payload);
        assert!(!ciphertext.is_empty());

        let consumed = client
            .read_tls_step(&ciphertext)
            .expect("step must succeed on fresh ciphertext");
        assert!(consumed > 0, "must consume at least one byte");
        assert!(consumed <= ciphertext.len());

        let mut dst = vec![0u8; payload.len()];
        let n = client.read_plaintext(&mut dst).unwrap();
        assert_eq!(n, payload.len());
        assert_eq!(&dst[..n], payload);
    }

    /// `read_tls_step` itself never overflows the plaintext buffer
    /// because it consumes one accepted prefix per call. The error
    /// surfaces on `read_and_process_tls` (the bounded helper) when
    /// fed a large slice without an interleaved drain. This test is
    /// the moved-from-nexus-async-net pin documenting the constraint
    /// that motivates `read_tls_step`'s existence.
    ///
    /// Named for what the body actually exercises (the failing
    /// helper), not for the helper it advocates against.
    #[test]
    fn read_and_process_tls_rejects_when_plaintext_buffer_full() {
        let (mut client, mut server) = connected_pair();
        let payload = vec![b'x'; 64 * 1024];
        let ciphertext = encrypt_server_payload(&mut server, &payload);

        let error = client
            .read_and_process_tls(&ciphertext)
            .expect_err("full-slice processing should overfill rustls plaintext");

        assert!(
            error.to_string().contains("received plaintext buffer full"),
            "unexpected error: {error}"
        );
    }

    /// `try_encrypt` returns the partial accepted count when rustls's
    /// outbound plaintext queue can't hold the full input. Lower the
    /// queue limit explicitly so the test doesn't depend on rustls's
    /// internal default.
    #[test]
    fn try_encrypt_returns_partial_when_queue_fills() {
        let (mut client, _server) = connected_pair();
        client.set_buffer_limit(Some(4096));

        // First 4 KiB fits.
        let n1 = client.try_encrypt(&[b'a'; 4096]).unwrap();
        assert_eq!(n1, 4096);

        // Next chunk: queue is full. try_encrypt accepts 0.
        let n2 = client.try_encrypt(&[b'b'; 4096]).unwrap();
        assert_eq!(n2, 0, "queue full → try_encrypt must report 0 accepted");
    }

    /// `set_buffer_limit(None)` lifts the cap entirely — `try_encrypt`
    /// accepts everything in one shot.
    #[test]
    fn set_buffer_limit_none_unlimits_queue() {
        let (mut client, _server) = connected_pair();
        client.set_buffer_limit(None);

        let n = client.try_encrypt(&[b'x'; 256 * 1024]).unwrap();
        assert_eq!(
            n,
            256 * 1024,
            "unlimited queue must accept the entire payload"
        );
    }

    /// The original `encrypt` is all-or-nothing: it errors with
    /// `WriteZero` if the input doesn't fit. This test pins that
    /// shape so the migration to `try_encrypt` in adapters stays
    /// motivated.
    #[test]
    #[allow(deprecated)] // exercising the deprecated primitive on purpose
    fn encrypt_errors_when_payload_exceeds_queue_limit() {
        let (mut client, _server) = connected_pair();
        client.set_buffer_limit(Some(4096));

        let err = client
            .encrypt(&[b'x'; 8192])
            .expect_err("encrypt must error when input > queue limit");
        let TlsError::Io(io_err) = err else {
            panic!("expected TlsError::Io, got {err:?}");
        };
        assert_eq!(io_err.kind(), io::ErrorKind::WriteZero);
    }

    // -------------------------------------------------------------------------
    // Side-by-side adapter-pattern demonstrators
    //
    // These two tests are the strong pin for why `read_tls_step` exists.
    // They mimic the EXACT shape of an async adapter loop: pull a chunk of
    // ciphertext, drain plaintext between chunks, repeat.
    //
    // The negative test fixes the chunk size at 32 KiB — the size a future
    // adapter optimisation might use to amortise syscall cost (the
    // current 8 KiB tmp accidentally hides the bug). At that size,
    // `read_and_process_tls` overflows rustls's 16 KiB plaintext queue
    // mid-chunk: the helper's internal loop iterates until the slice is
    // consumed, so it processes ~5 records back-to-back before yielding,
    // and the inter-chunk plaintext drain doesn't help.
    //
    // The positive test runs the same shape with the same chunk size
    // through `read_tls_step` + a `pending_read` spillover. It recovers
    // the full payload because each `read_tls_step` advances by exactly
    // one accepted prefix, which lets the caller drain plaintext between
    // packet steps — never letting rustls's queue fill.
    //
    // Together they pin: the bug is real, the fix solves it, and the
    // chunk-size headroom that would otherwise be a latent foot-gun for
    // future adapter authors is closed.
    // -------------------------------------------------------------------------

    /// **Negative side**: feeding 32 KiB ciphertext chunks through
    /// `read_and_process_tls` overflows rustls's plaintext queue
    /// mid-chunk, even with proper inter-chunk plaintext draining.
    #[test]
    fn adapter_pattern_with_read_and_process_tls_overflows_on_oversize_chunks() {
        let (mut client, mut server) = connected_pair();
        let payload = vec![b'x'; 64 * 1024];
        let ciphertext = encrypt_server_payload(&mut server, &payload);

        // 32 KiB > 16 KiB plaintext queue cap. The helper's internal
        // loop processes ~5 records back-to-back per chunk; queue
        // overflows on the ~5th record.
        let chunk_size = 32 * 1024;
        let mut cursor = 0;
        let mut dst = [0u8; 4096];

        let result = loop {
            // Drain plaintext between chunks (proper adapter etiquette).
            let n = client
                .read_plaintext(&mut dst)
                .expect("read_plaintext must not error here");
            if n > 0 {
                continue;
            }

            if cursor >= ciphertext.len() {
                break Ok(());
            }

            let end = (cursor + chunk_size).min(ciphertext.len());
            match client.read_and_process_tls(&ciphertext[cursor..end]) {
                Ok(consumed) => cursor += consumed,
                Err(e) => break Err(e),
            }
        };

        let err = result.expect_err(
            "32 KiB ciphertext chunks via read_and_process_tls must overflow \
             rustls's plaintext queue mid-chunk",
        );
        assert!(
            err.to_string().contains("received plaintext buffer full"),
            "unexpected error: {err}"
        );
    }

    /// **Positive side**: the same 64 KiB payload, same 32 KiB chunk
    /// size, but consumed via `read_tls_step` + a `pending_read`
    /// spillover. The step-and-drain pattern recovers the entire
    /// payload without overflowing.
    #[test]
    fn adapter_pattern_with_read_tls_step_handles_oversize_chunks() {
        let (mut client, mut server) = connected_pair();
        let payload = vec![b'x'; 64 * 1024];
        let ciphertext = encrypt_server_payload(&mut server, &payload);

        let chunk_size = 32 * 1024;
        let mut pending = ReadBuf::with_capacity(chunk_size);
        let mut plaintext = Vec::with_capacity(payload.len());
        let mut cursor = 0;
        let mut dst = [0u8; 4096];

        // Cap iterations so a regression doesn't loop forever.
        for _ in 0..1_000_000 {
            // 1. Drain rustls's plaintext queue.
            let n = client.read_plaintext(&mut dst).unwrap();
            if n > 0 {
                plaintext.extend_from_slice(&dst[..n]);
                if plaintext.len() == payload.len() {
                    break;
                }
                continue;
            }

            // 2. Step buffered ciphertext one packet at a time.
            if !pending.is_empty() {
                let consumed = client
                    .read_tls_step(pending.data())
                    .expect("step must succeed against buffered ciphertext");
                pending.advance(consumed);
                continue;
            }

            // 3. Pull the next 32 KiB chunk.
            if cursor < ciphertext.len() {
                let end = (cursor + chunk_size).min(ciphertext.len());
                let chunk = &ciphertext[cursor..end];
                let consumed = client
                    .read_tls_step(chunk)
                    .expect("step must succeed on fresh chunk");
                if consumed < chunk.len() {
                    let rem = &chunk[consumed..];
                    let spare = pending.spare();
                    spare[..rem.len()].copy_from_slice(rem);
                    pending.filled(rem.len());
                }
                cursor = end;
                continue;
            }

            break;
        }

        assert_eq!(
            plaintext, payload,
            "step + drain must recover the entire payload"
        );
        assert_eq!(cursor, ciphertext.len(), "must consume entire ciphertext");
        assert!(pending.is_empty(), "no leftover ciphertext");
    }

    /// Demonstrates the contract difference between `read_tls` and
    /// `read_and_process_tls` (issue #200).
    ///
    /// rustls 0.23 clamps each `read_tls` call to a 4096-byte chunk per
    /// the deframer's internal `READ_SIZE` (see
    /// `rustls::msgs::deframer::buffers::DeframerVecBuffer::prepare_read`).
    /// Any slice larger than that is partially consumed in one call —
    /// the buggy pattern `codec.read_tls(&buf)?; process_new_packets()?;`
    /// silently drops everything past byte 4096 because the call site
    /// ignores the returned count.
    ///
    /// In the real-world failure (Polymarket's WSS endpoint) the server
    /// emits a multi-record TLS 1.3 handshake burst (ServerHello +
    /// EncryptedExtensions + Certificate + CertVerify + Finished) that
    /// can easily exceed 4096 bytes when the cert chain is non-trivial,
    /// or arrive concatenated inside a single TCP segment. The server
    /// times out after ~15s waiting for the client's Finished record
    /// that never comes, because the client never decrypted past the
    /// 4096th byte.
    ///
    /// The 4096-byte cap is rustls-internal and may change in future
    /// versions. If it does, this assertion needs adjusting (raise the
    /// input size above the new cap), but the helper's loop remains
    /// correct — partial consumption is the documented contract of
    /// `Connection::read_tls`, not an implementation accident.
    #[test]
    #[allow(deprecated)] // exercising the deprecated primitive on purpose
    fn bare_read_tls_partially_consumes_large_slice() {
        let client_config = TlsConfig::builder().danger_no_verify().build().unwrap();
        let mut client = TlsCodec::new(&client_config, "localhost").unwrap();

        // Larger than rustls's READ_SIZE (4096) per-call cap. Contents
        // don't need to be valid TLS — `read_tls` only buffers; it does
        // not validate. (Validation happens in `process_new_packets`,
        // which we do not call.)
        let oversize = vec![0u8; 8192];

        let consumed = client
            .read_tls(&oversize)
            .expect("read_tls buffers without validating");

        assert!(
            consumed < oversize.len(),
            "expected partial consumption (issue #200 surface): \
             rustls should clamp to its per-call READ_SIZE cap, but \
             consumed {consumed} of {} bytes in one call",
            oversize.len(),
        );
    }
}
