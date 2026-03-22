//! SASL Authentication for D-Bus
//!
//! — ColdCipher: D-Bus authentication happens before any messages flow. The client
//! sends "AUTH EXTERNAL <uid_hex>\r\n" and the server responds "OK <guid>\r\n".
//! EXTERNAL auth uses the kernel's SCM_CREDENTIALS to verify the client's identity.
//!
//! For OXIDE's oxide-dbusd, we always accept EXTERNAL auth since we trust the
//! kernel's credential passing. No passwords, no cookies, no DBUS_COOKIE_SHA1 BS.

use alloc::string::String;
use alloc::vec::Vec;

/// Build the SASL AUTH EXTERNAL handshake bytes.
/// uid is the numeric UID, hex-encoded as ASCII.
pub fn auth_external_request(uid: u32) -> Vec<u8> {
    // D-Bus SASL: "\0AUTH EXTERNAL <hex_uid>\r\n"
    let mut buf = Vec::new();
    buf.push(0); // NUL byte (required as first byte of SASL)
    buf.extend_from_slice(b"AUTH EXTERNAL ");

    // Hex-encode the UID string "0" -> "30", "1000" -> "31303030"
    let uid_str = format_u32(uid);
    for byte in uid_str.as_bytes() {
        let hi = HEX_CHARS[((*byte >> 4) & 0xF) as usize];
        let lo = HEX_CHARS[(*byte & 0xF) as usize];
        buf.push(hi);
        buf.push(lo);
    }

    buf.extend_from_slice(b"\r\n");
    buf
}

/// Check if a response is "OK <guid>"
pub fn parse_auth_ok(response: &[u8]) -> Option<String> {
    // Look for "OK " at the start (after possible leading whitespace)
    let s = core::str::from_utf8(response).ok()?;
    let trimmed = s.trim();
    if trimmed.starts_with("OK ") {
        let guid = trimmed[3..].trim_end_matches("\r\n").trim();
        Some(String::from(guid))
    } else {
        None
    }
}

/// Send NEGOTIATE_UNIX_FD request
pub fn negotiate_unix_fd() -> Vec<u8> {
    b"NEGOTIATE_UNIX_FD\r\n".to_vec()
}

/// Send BEGIN to start message mode
pub fn begin_message_mode() -> Vec<u8> {
    b"BEGIN\r\n".to_vec()
}

const HEX_CHARS: [u8; 16] = *b"0123456789abcdef";

fn format_u32(n: u32) -> String {
    if n == 0 {
        return String::from("0");
    }
    let mut buf = [0u8; 10];
    let mut i = 0;
    let mut val = n;
    while val > 0 {
        buf[i] = b'0' + (val % 10) as u8;
        val /= 10;
        i += 1;
    }
    buf[..i].reverse();
    String::from(core::str::from_utf8(&buf[..i]).unwrap_or("0"))
}
