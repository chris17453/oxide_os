//! OXIDE TLS 1.3 Client Library
//!
//! — ColdCipher: Pure userspace TLS implementation, following the Linux model
//! where TLS is a library (like OpenSSL/rustls), not kernel code. The kernel
//! provides raw TCP sockets; we provide the encryption layer on top.
//!
//! No std. No mercy. Every certificate is guilty until proven innocent.
//! — VeilAudit: "If you trust a cert you haven't verified, you deserve the MITM."
//!
//! Usage:
//! ```
//! let sock = tcp_socket();
//! connect(sock, &addr, SOCKADDR_IN_SIZE);
//! let mut tls = tls_connect(sock, "example.com")?;
//! tls.send(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")?;
//! let mut buf = [0u8; 4096];
//! let n = tls.recv(&mut buf)?;
//! tls.shutdown()?;
//! ```

#![no_std]

extern crate alloc;

pub mod alert;
pub mod extensions;
pub mod handshake;
pub mod key_schedule;
pub mod record;
pub mod transcript;
pub mod x509;

use alloc::vec;
use alloc::vec::Vec;
use handshake::{Handshake, HandshakeState};
use key_schedule::TrafficKeys;
use record::{ContentType, TlsRecord, TLS_12};

/// TLS errors
#[derive(Debug)]
pub enum TlsError {
    HandshakeFailed(&'static str),
    IoError(i32),
    DecryptionFailed,
    ConnectionClosed,
    CertificateInvalid(&'static str),
    Unsupported(&'static str),
}

/// — ColdCipher: A TLS-encrypted stream wrapping a raw TCP socket.
/// After tls_connect(), all send/recv goes through AEAD encryption.
pub struct TlsStream {
    fd: i32,
    client_keys: TrafficKeys,
    server_keys: TrafficKeys,
    client_seq: u64,
    server_seq: u64,
    key_len: usize,
    recv_buf: Vec<u8>,
}

impl TlsStream {
    pub fn send(&mut self, data: &[u8]) -> Result<usize, TlsError> {
        let ciphertext = record::encrypt_record(
            &self.client_keys.key,
            &self.client_keys.iv,
            self.client_seq,
            ContentType::ApplicationData,
            data,
        );
        if ciphertext.is_empty() {
            return Err(TlsError::DecryptionFailed);
        }
        let rec = TlsRecord::application_data(ciphertext);
        let wire = rec.encode();
        let sent = libc::socket::send(self.fd, &wire, 0);
        if sent < 0 {
            return Err(TlsError::IoError(sent as i32));
        }
        self.client_seq += 1;
        Ok(data.len())
    }

    pub fn recv(&mut self, buf: &mut [u8]) -> Result<usize, TlsError> {
        if !self.recv_buf.is_empty() {
            let n = buf.len().min(self.recv_buf.len());
            buf[..n].copy_from_slice(&self.recv_buf[..n]);
            self.recv_buf.drain(..n);
            return Ok(n);
        }
        let mut header = [0u8; 5];
        let n = read_exact(self.fd, &mut header)?;
        if n < 5 { return Err(TlsError::ConnectionClosed); }
        let record_len = ((header[3] as usize) << 8) | header[4] as usize;
        if record_len > record::MAX_CIPHERTEXT_LENGTH {
            return Err(TlsError::DecryptionFailed);
        }
        let mut fragment = vec![0u8; record_len];
        let n = read_exact(self.fd, &mut fragment)?;
        if n < record_len { return Err(TlsError::ConnectionClosed); }
        let (content_type, plaintext) = record::decrypt_record(
            &self.server_keys.key, &self.server_keys.iv, self.server_seq, &fragment,
        ).ok_or(TlsError::DecryptionFailed)?;
        self.server_seq += 1;
        match content_type {
            ContentType::ApplicationData => {
                let n = buf.len().min(plaintext.len());
                buf[..n].copy_from_slice(&plaintext[..n]);
                if plaintext.len() > n { self.recv_buf.extend_from_slice(&plaintext[n..]); }
                Ok(n)
            }
            ContentType::Alert => {
                if plaintext.len() >= 2 && plaintext[1] == 0 {
                    return Err(TlsError::ConnectionClosed);
                }
                Err(TlsError::HandshakeFailed("received alert"))
            }
            _ => Err(TlsError::HandshakeFailed("unexpected content type")),
        }
    }

    pub fn shutdown(&mut self) -> Result<(), TlsError> {
        let alert_data = alert::Alert::warning(alert::AlertDescription::CloseNotify).encode();
        let ciphertext = record::encrypt_record(
            &self.client_keys.key, &self.client_keys.iv, self.client_seq,
            ContentType::Alert, &alert_data,
        );
        if !ciphertext.is_empty() {
            let rec = TlsRecord::application_data(ciphertext);
            let _ = libc::socket::send(self.fd, &rec.encode(), 0);
        }
        Ok(())
    }
}

/// Perform TLS 1.3 handshake on a connected TCP socket
pub fn tls_connect(fd: i32, hostname: &str) -> Result<TlsStream, TlsError> {
    let mut hs = Handshake::new(hostname);

    // Send ClientHello
    let ch = hs.build_client_hello();
    let ch_record = TlsRecord::handshake(ch);
    let sent = libc::socket::send(fd, &ch_record.encode(), 0);
    if sent < 0 { return Err(TlsError::IoError(sent as i32)); }

    // Read ServerHello
    let sh_data = read_handshake_record(fd)?;
    hs.process_server_hello(&sh_data).map_err(|e| TlsError::HandshakeFailed(e))?;

    // Derive handshake keys
    let server_hs = hs.server_hs_secret.unwrap();
    let server_hs_keys = key_schedule::derive_traffic_keys(&server_hs, hs.key_length());
    let mut server_hs_seq: u64 = 0;

    // Read encrypted handshake messages until Finished
    while hs.state != HandshakeState::Connected {
        let mut header = [0u8; 5];
        read_exact(fd, &mut header)?;
        let record_len = ((header[3] as usize) << 8) | header[4] as usize;
        let mut fragment = vec![0u8; record_len];
        read_exact(fd, &mut fragment)?;

        if header[0] == ContentType::ChangeCipherSpec as u8 { continue; }

        if let Some((ct, plaintext)) = record::decrypt_record(
            &server_hs_keys.key, &server_hs_keys.iv, server_hs_seq, &fragment,
        ) {
            server_hs_seq += 1;
            if ct == ContentType::Handshake {
                let mut pos = 0;
                while pos < plaintext.len() {
                    if pos + 4 > plaintext.len() { break; }
                    let msg_len = ((plaintext[pos + 1] as usize) << 16)
                        | ((plaintext[pos + 2] as usize) << 8) | plaintext[pos + 3] as usize;
                    let end = pos + 4 + msg_len;
                    if end > plaintext.len() { break; }
                    hs.process_encrypted_handshake(&plaintext[pos..end])
                        .map_err(|e| TlsError::HandshakeFailed(e))?;
                    pos = end;
                }
            } else if ct == ContentType::Alert {
                return Err(TlsError::HandshakeFailed("server sent alert during handshake"));
            }
        } else {
            return Err(TlsError::DecryptionFailed);
        }
    }

    // Send client Finished
    let client_hs = hs.client_hs_secret.unwrap();
    let client_hs_keys = key_schedule::derive_traffic_keys(&client_hs, hs.key_length());
    let finished_msg = hs.build_client_finished();
    let finished_ct = record::encrypt_record(
        &client_hs_keys.key, &client_hs_keys.iv, 0, ContentType::Handshake, &finished_msg,
    );
    let finished_rec = TlsRecord::application_data(finished_ct);
    let sent = libc::socket::send(fd, &finished_rec.encode(), 0);
    if sent < 0 { return Err(TlsError::IoError(sent as i32)); }

    // Derive application keys
    hs.derive_application_keys().map_err(|e| TlsError::HandshakeFailed(e))?;
    let client_app = hs.client_app_secret.unwrap();
    let server_app = hs.server_app_secret.unwrap();
    let key_len = hs.key_length();

    Ok(TlsStream {
        fd,
        client_keys: key_schedule::derive_traffic_keys(&client_app, key_len),
        server_keys: key_schedule::derive_traffic_keys(&server_app, key_len),
        client_seq: 0,
        server_seq: 0,
        key_len,
        recv_buf: Vec::new(),
    })
}

fn read_exact(fd: i32, buf: &mut [u8]) -> Result<usize, TlsError> {
    let mut total = 0;
    while total < buf.len() {
        let n = libc::socket::recv(fd, &mut buf[total..], 0);
        if n < 0 { return Err(TlsError::IoError(n as i32)); }
        if n == 0 { return Ok(total); }
        total += n as usize;
    }
    Ok(total)
}

fn read_handshake_record(fd: i32) -> Result<Vec<u8>, TlsError> {
    let mut header = [0u8; 5];
    read_exact(fd, &mut header)?;
    if header[0] != ContentType::Handshake as u8 {
        return Err(TlsError::HandshakeFailed("expected handshake record"));
    }
    let record_len = ((header[3] as usize) << 8) | header[4] as usize;
    let mut fragment = vec![0u8; record_len];
    read_exact(fd, &mut fragment)?;
    Ok(fragment)
}
