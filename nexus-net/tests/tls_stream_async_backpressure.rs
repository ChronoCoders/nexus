//! End-to-end positive guard for `TlsStream::poll_read` over tokio's
//! AsyncRead — drives a 256 KiB app-data burst through a loopback TLS
//! connection and asserts every byte arrives.
//!
//! At the current 8 KiB `tmp` chunk size in `poll_read`, this test
//! passes both pre-fix and post-fix (8 KiB ciphertext stays under
//! rustls's plaintext queue cap). The codec-level pin for the bug
//! lives at
//! `nexus-net::tls::codec::tests::adapter_pattern_with_read_and_process_tls_overflows_on_oversize_chunks`
//! — that is the strong demonstrator that proves `read_tls_step` is
//! the correct primitive, not `read_and_process_tls`. This integration
//! test guards against future tmp-size tuning re-introducing the bug
//! at the integration layer.

#![cfg(all(feature = "tls", feature = "tokio"))]

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use nexus_net::tls::{TlsCodec, TlsConfig, TlsStream};

const PAYLOAD_LEN: usize = 256 * 1024;

fn make_server_config() -> Arc<rustls::ServerConfig> {
    let cert_kp =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).expect("cert generation");
    let chain = vec![rustls::pki_types::CertificateDer::from(
        cert_kp.cert.der().to_vec(),
    )];
    let key = rustls::pki_types::PrivateKeyDer::try_from(cert_kp.key_pair.serialize_der())
        .expect("server key");
    Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .expect("server config"),
    )
}

#[allow(clippy::needless_pass_by_value)]
fn run_burst_server(listener: TcpListener, server_config: Arc<rustls::ServerConfig>) {
    let (tcp, _addr) = listener.accept().expect("accept");
    tcp.set_nodelay(true).ok();
    tcp.set_read_timeout(Some(Duration::from_secs(10))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(10))).ok();

    let server = rustls::ServerConnection::new(server_config).expect("server conn");
    // StreamOwned interleaves rustls plaintext writes with TCP I/O —
    // exactly what's needed so a 256 KiB write_all flushes records as
    // they're produced rather than overflowing rustls's outbound queue.
    let mut tls = rustls::StreamOwned::new(server, tcp);

    let payload = vec![b'x'; PAYLOAD_LEN];
    tls.write_all(&payload).expect("server send burst");
    tls.flush().expect("server flush burst");

    // Drain anything the client says back so the server-side socket
    // stays alive until the client finishes reading.
    let mut sink = [0u8; 1024];
    loop {
        match tls.read(&mut sink) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
}

#[test]
fn tls_stream_handles_oversize_app_data_burst() {
    let server_config = make_server_config();

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("local_addr").port();
    let server_handle = thread::spawn(move || run_burst_server(listener, server_config));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async move {
        let tcp = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("client connect");
        tcp.set_nodelay(true).ok();

        let tls_config = TlsConfig::builder()
            .danger_no_verify()
            .build()
            .expect("client tls config");
        let codec = TlsCodec::new(&tls_config, "localhost").expect("client codec");
        let mut stream = TlsStream::new(tcp, codec);

        // Drive the async handshake to completion before reading
        // application data.
        stream
            .handshake_async()
            .await
            .expect("client handshake_async");

        let mut received = Vec::with_capacity(PAYLOAD_LEN);
        let mut buf = vec![0u8; 16 * 1024];
        while received.len() < PAYLOAD_LEN {
            let n = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buf))
                .await
                .expect("read timeout (likely the pre-fix backpressure bug)")
                .expect("client read");
            assert!(
                n > 0,
                "unexpected EOF mid-burst at {} bytes",
                received.len()
            );
            received.extend_from_slice(&buf[..n]);
        }

        assert_eq!(received.len(), PAYLOAD_LEN, "must receive entire burst");
        assert!(received.iter().all(|&b| b == b'x'), "payload bytes intact");
    });

    server_handle.join().expect("server join");
}

/// AsyncWriteExt::write_all with a payload larger than rustls's
/// default outbound plaintext queue (64 KiB), followed by the natural
/// `flush → shutdown` shape. Pre-fix (R3): errors with `WriteZero`
/// from rustls's writer mid-call (all-or-nothing `encrypt`).
/// Post-R3: `try_encrypt` chunks the write across multiple poll_write
/// cycles. Post-R4: `poll_shutdown` queues a TLS `close_notify` so the
/// server reads to a clean EOF rather than seeing a truncation alert.
#[test]
fn tls_stream_handles_large_write_via_chunking() {
    let server_config = make_server_config();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("local_addr").port();

    let payload = vec![b'z'; PAYLOAD_LEN];
    let server_handle =
        thread::spawn(move || run_burst_sink_to_eof_server(listener, server_config, PAYLOAD_LEN));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async move {
        let tcp = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("client connect");
        tcp.set_nodelay(true).ok();

        let tls_config = TlsConfig::builder()
            .danger_no_verify()
            .build()
            .expect("client tls config");
        let codec = TlsCodec::new(&tls_config, "localhost").expect("client codec");

        // Default capacities. The chunking happens in poll_write
        // because the 256 KiB payload is 4× rustls's default
        // plaintext queue (64 KiB).
        let mut stream = TlsStream::new(tcp, codec);

        stream
            .handshake_async()
            .await
            .expect("client handshake_async");

        tokio::time::timeout(Duration::from_secs(10), stream.write_all(&payload))
            .await
            .expect("write timeout — chunking loop may be stalled")
            .expect("client write_all");
        stream.flush().await.expect("client flush");

        // poll_shutdown queues close_notify, drains, then closes the
        // transport — server's rustls reader sees a clean EOF (Ok(0)),
        // not the truncation alert it would get from a bare TCP FIN.
        stream.shutdown().await.expect("client shutdown");

        // Drain any inbound bytes the server pushed asynchronously
        // (e.g. TLS 1.3 NewSessionTicket records). Without this, the
        // OS sees unread data in the receive buffer when TcpStream
        // drops and sends RST instead of FIN — surfaces on the server
        // as ConnectionReset mid-read.
        let mut sink = [0u8; 1024];
        loop {
            match tokio::time::timeout(Duration::from_secs(1), stream.read(&mut sink)).await {
                Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                Ok(Ok(_)) => {}
            }
        }
    });

    server_handle.join().expect("server join");
}

/// Drop a `TlsStream` between a `poll_read` returning `Pending` and
/// the runtime re-entering. Guards against future `Drop`-impl
/// regressions: no UB, no panic, no leak. The duplex pipe never
/// writes from the other end so `poll_read` returns `Pending`
/// immediately; the `tokio::time::timeout` then cancels the inner
/// read future, dropping the borrow on the stream — and then the
/// explicit `drop(stream)` runs the destructor.
#[test]
fn tls_stream_drop_mid_poll_does_not_panic() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async {
        // 1 KiB duplex; the other end is held alive but never written to.
        let (other_end, near_end) = tokio::io::duplex(1024);
        let _keepalive = other_end;

        let tls_config = TlsConfig::builder()
            .danger_no_verify()
            .build()
            .expect("client tls config");
        let codec = TlsCodec::new(&tls_config, "localhost").expect("codec");
        let mut stream = TlsStream::new(near_end, codec);

        // poll_read returns Pending (no inbound bytes). Cancel the
        // inner future via timeout to simulate "dropped mid-poll".
        let mut buf = [0u8; 16];
        let res = tokio::time::timeout(Duration::from_millis(50), stream.read(&mut buf)).await;
        assert!(res.is_err(), "expected timeout (no data should arrive)");

        // Explicit drop with buffers possibly populated by the
        // handshake-attempt write side.
        drop(stream);
    });
}
/// (rustls returns Ok(0) once close_notify arrives), then asserts the
/// full payload arrived.
#[allow(clippy::needless_pass_by_value)]
fn run_burst_sink_to_eof_server(
    listener: TcpListener,
    server_config: Arc<rustls::ServerConfig>,
    expected_len: usize,
) {
    let (tcp, _addr) = listener.accept().expect("accept");
    tcp.set_nodelay(true).ok();
    tcp.set_read_timeout(Some(Duration::from_secs(10))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(10))).ok();

    let server = rustls::ServerConnection::new(server_config).expect("server conn");
    let mut tls = rustls::StreamOwned::new(server, tcp);

    let mut received = Vec::with_capacity(expected_len);
    let mut buf = vec![0u8; 8192];
    loop {
        let n = tls.read(&mut buf).expect("server read");
        if n == 0 {
            break; // clean close_notify
        }
        received.extend_from_slice(&buf[..n]);
    }
    assert_eq!(received.len(), expected_len, "server-side payload length");
    assert!(
        received.iter().all(|&b| b == b'z'),
        "server-side payload bytes intact"
    );
}

/// Exercises the write-side backpressure branch of
/// `drain_codec_to_pending`: with `pending_write_cap == TMP_SIZE`
/// (8 KiB) the loop must drain-and-refill multiple times to flush a
/// 32 KiB encrypt through. The write succeeds end-to-end without
/// surfacing the `WriteZero` "rustls produced 0 bytes" branch — that
/// branch only fires on a rustls contract violation, never on
/// legitimate "spare is full, drain to socket" backpressure.
///
/// Why 32 KiB and not larger: rustls bounds its outbound plaintext
/// queue at `DEFAULT_BUFFER_LIMIT = 64 KiB`. A single `encrypt(buf)`
/// larger than that errors with WriteZero from rustls's writer (not
/// from our adapter). 32 KiB stays well under the limit while still
/// requiring ~4 drain/refill iterations through an 8 KiB
/// pending_write.
const WRITE_BACKPRESSURE_PAYLOAD_LEN: usize = 32 * 1024;

#[allow(clippy::needless_pass_by_value)]
fn run_burst_sink_server(
    listener: TcpListener,
    server_config: Arc<rustls::ServerConfig>,
    expected_len: usize,
) {
    let (tcp, _addr) = listener.accept().expect("accept");
    tcp.set_nodelay(true).ok();
    tcp.set_read_timeout(Some(Duration::from_secs(10))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(10))).ok();

    let server = rustls::ServerConnection::new(server_config).expect("server conn");
    let mut tls = rustls::StreamOwned::new(server, tcp);

    let mut received = Vec::with_capacity(expected_len);
    let mut buf = vec![0u8; 8192];
    while received.len() < expected_len {
        match tls.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => received.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    assert_eq!(received.len(), expected_len, "server-side payload length");
    assert!(
        received.iter().all(|&b| b == b'y'),
        "server-side payload bytes intact"
    );
}

#[test]
fn tls_stream_handles_oversize_write_with_tiny_pending_write() {
    let server_config = make_server_config();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("local_addr").port();

    let payload = vec![b'y'; WRITE_BACKPRESSURE_PAYLOAD_LEN];
    let server_handle = thread::spawn(move || {
        run_burst_sink_server(listener, server_config, WRITE_BACKPRESSURE_PAYLOAD_LEN)
    });

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async move {
        let tcp = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("client connect");
        tcp.set_nodelay(true).ok();

        let tls_config = TlsConfig::builder()
            .danger_no_verify()
            .build()
            .expect("client tls config");
        let codec = TlsCodec::new(&tls_config, "localhost").expect("client codec");

        // pending_write_cap = TMP_SIZE forces the drain-and-refill
        // loop to iterate multiple times for a 32 KiB encrypt,
        // exercising the legitimate backpressure exit path of
        // `drain_codec_to_pending`.
        let mut stream = TlsStream::with_capacities(
            tcp,
            codec,
            TlsStream::<tokio::net::TcpStream>::TMP_SIZE,
            TlsStream::<tokio::net::TcpStream>::TMP_SIZE,
        );

        stream
            .handshake_async()
            .await
            .expect("client handshake_async");

        tokio::time::timeout(Duration::from_secs(10), stream.write_all(&payload))
            .await
            .expect("write timeout — backpressure loop may be stalled")
            .expect("client write_all");
        stream.flush().await.expect("client flush");
        stream.shutdown().await.expect("client shutdown");
    });

    server_handle.join().expect("server join");
}
