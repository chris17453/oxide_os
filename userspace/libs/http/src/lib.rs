//! OXIDE HTTP Client Library
//!
//! Reusable HTTP/1.1 client for HTTPS and HTTP connections. Handles:
//! - URL parsing (scheme, host, port, path)
//! - DNS resolution
//! - TCP connection + optional TLS 1.3 handshake
//! - HTTP request building and response parsing
//! - Redirect following (up to 20 hops)
//! - Streaming body reads
//!
//! — ShadePacket: "Every HTTP library starts as 'just parse a URL' and ends as
//!   'oh god why are there 47 redirect flavors and chunked encoding exists'.
//!   This one is no different. But at least it's ours."
//!
//! Usage:
//! ```
//! let response = oxide_http::get("https://example.com")?;
//! // response.status == 200
//! // response.body contains the HTML
//! ```

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

pub mod url;

use libc::socket::{
    SOCKADDR_IN_SIZE, connect, recv, send, shut, shutdown, sockaddr_in_octets, tcp_socket,
};

// ============================================================================
// Public API types
// ============================================================================

/// HTTP response from a completed request.
/// — ShadePacket: "Status code, headers as raw text, body as bytes. Simple."
pub struct Response {
    /// HTTP status code (200, 301, 404, etc.)
    pub status: u16,
    /// Raw response headers as a string
    pub headers: String,
    /// Response body bytes
    pub body: Vec<u8>,
    /// Final URL after redirects
    pub url: String,
}

/// HTTP client errors.
/// — ShadePacket: "Everything that can go wrong between 'I want a webpage'
///   and 'here are some bytes'. Which is... a lot."
#[derive(Debug)]
pub enum HttpError {
    /// URL couldn't be parsed
    InvalidUrl,
    /// DNS resolution failed
    DnsResolutionFailed,
    /// TCP socket creation failed
    SocketError(i32),
    /// TCP connection failed
    ConnectionFailed(i32),
    /// TLS handshake failed
    TlsError(oxide_tls::TlsError),
    /// Too many redirects
    TooManyRedirects,
    /// Failed to send request
    SendFailed,
    /// Failed to receive response
    ReceiveFailed,
    /// Response had no headers / malformed
    MalformedResponse,
}

/// Maximum number of redirects to follow.
const MAX_REDIRECTS: u32 = 20;

// ============================================================================
// Public API
// ============================================================================

/// Perform an HTTP GET request, following redirects.
///
/// Handles HTTP and HTTPS (auto-detected from URL scheme or port 443).
/// Returns the complete response including status, headers, and body.
///
/// — ShadePacket: "One function. One URL. Get bytes. That's the contract."
pub fn get(url: &str) -> Result<Response, HttpError> {
    get_with_redirects(url, 0)
}

/// Perform an HTTP GET, returning just the body bytes or an error.
///
/// — ShadePacket: "For when you don't care about headers. Most callers don't."
pub fn get_bytes(url: &str) -> Result<Vec<u8>, HttpError> {
    let resp = get(url)?;
    if resp.status >= 200 && resp.status < 300 {
        Ok(resp.body)
    } else {
        Err(HttpError::MalformedResponse)
    }
}

// ============================================================================
// Internal implementation
// ============================================================================

fn get_with_redirects(url: &str, redirect_count: u32) -> Result<Response, HttpError> {
    if redirect_count >= MAX_REDIRECTS {
        return Err(HttpError::TooManyRedirects);
    }

    let parsed = url::parse_url(url).ok_or(HttpError::InvalidUrl)?;

    // Resolve hostname
    let ip = resolve_host(parsed.host)?;

    // TCP connect
    let sock = tcp_socket();
    if sock < 0 {
        return Err(HttpError::SocketError(sock));
    }

    let addr = sockaddr_in_octets(parsed.port, ip.0, ip.1, ip.2, ip.3);
    let ret = connect(sock, &addr, SOCKADDR_IN_SIZE);
    if ret < 0 {
        libc::close(sock);
        return Err(HttpError::ConnectionFailed(ret));
    }

    // TLS handshake for HTTPS
    let mut tls_stream: Option<oxide_tls::TlsStream> = None;
    if parsed.is_https() {
        match oxide_tls::tls_connect(sock, parsed.host) {
            Ok(stream) => tls_stream = Some(stream),
            Err(e) => {
                libc::close(sock);
                return Err(HttpError::TlsError(e));
            }
        }
    }

    // Build and send HTTP request
    let request = build_request("GET", parsed.path, parsed.host);
    let sent = send_data(&mut tls_stream, sock, request.as_bytes());
    if sent < 0 {
        cleanup(tls_stream, sock);
        return Err(HttpError::SendFailed);
    }

    // Receive response
    let raw = receive_response(&mut tls_stream, sock)?;
    cleanup(tls_stream, sock);

    // Parse headers and body
    let (status, headers, body) = parse_response(&raw)?;

    // Handle redirects
    if is_redirect(status) {
        if let Some(location) = extract_header(&headers, "location") {
            // — ShadePacket: "Handle relative and absolute redirect URLs.
            //   Servers are creative about what they put in Location."
            let redirect_url = if location.starts_with("http://") || location.starts_with("https://") {
                String::from(location)
            } else {
                // Relative URL — reconstruct from original
                let scheme = if parsed.is_https() { "https://" } else { "http://" };
                let mut full = String::new();
                full.push_str(scheme);
                full.push_str(parsed.host);
                if !location.starts_with('/') {
                    full.push('/');
                }
                full.push_str(location);
                full
            };
            return get_with_redirects(&redirect_url, redirect_count + 1);
        }
    }

    Ok(Response {
        status,
        headers,
        body,
        url: String::from(url),
    })
}

/// Resolve hostname to IPv4 address.
/// — ShadePacket: "IP literal first, DNS second. Like every resolver since 1983."
fn resolve_host(host: &str) -> Result<(u8, u8, u8, u8), HttpError> {
    // Try IP literal first
    if let Some(ip) = url::parse_ip(host) {
        return Ok(ip);
    }
    // DNS resolution
    libc::dns::resolve(host, None).ok_or(HttpError::DnsResolutionFailed)
}

/// Build an HTTP/1.1 request.
/// — ShadePacket: "Four headers. That's all you need. Everything else is vanity."
fn build_request(method: &str, path: &str, host: &str) -> String {
    let mut req = String::with_capacity(256);
    req.push_str(method);
    req.push(' ');
    req.push_str(path);
    req.push_str(" HTTP/1.1\r\nHost: ");
    req.push_str(host);
    req.push_str("\r\nUser-Agent: oxide-http/0.1\r\nAccept: */*\r\nConnection: close\r\n\r\n");
    req
}

/// Send data over TLS or raw TCP.
fn send_data(tls: &mut Option<oxide_tls::TlsStream>, sock: i32, data: &[u8]) -> isize {
    if let Some(stream) = tls {
        match stream.send(data) {
            Ok(n) => n as isize,
            Err(_) => -1,
        }
    } else {
        send(sock, data, 0)
    }
}

/// Receive the full HTTP response (headers + body).
/// — ShadePacket: "Read until the server hangs up. Connection: close makes this simple."
fn receive_response(tls: &mut Option<oxide_tls::TlsStream>, sock: i32) -> Result<Vec<u8>, HttpError> {
    let mut data = Vec::with_capacity(8192);
    let mut buf = [0u8; 4096];
    let mut eagain_count = 0u32;

    loop {
        let n = if let Some(stream) = tls {
            match stream.recv(&mut buf) {
                Ok(n) => n as isize,
                Err(oxide_tls::TlsError::ConnectionClosed) => 0,
                Err(oxide_tls::TlsError::IoError(code)) => code as isize,
                Err(_) => -1,
            }
        } else {
            recv(sock, &mut buf, 0)
        };

        if n < 0 {
            if n == -11 {
                eagain_count += 1;
                if eagain_count < 10 { continue; }
            }
            break;
        }
        if n == 0 { break; }
        eagain_count = 0;
        data.extend_from_slice(&buf[..n as usize]);
    }

    if data.is_empty() {
        Err(HttpError::ReceiveFailed)
    } else {
        Ok(data)
    }
}

/// Parse raw HTTP response into status, headers, body.
fn parse_response(raw: &[u8]) -> Result<(u16, String, Vec<u8>), HttpError> {
    // Find header/body separator
    let sep = find_bytes(raw, b"\r\n\r\n").ok_or(HttpError::MalformedResponse)?;
    let header_bytes = &raw[..sep];
    let body = Vec::from(&raw[sep + 4..]);

    let headers = core::str::from_utf8(header_bytes)
        .map_err(|_| HttpError::MalformedResponse)?;

    // Parse status code from first line: "HTTP/1.1 200 OK"
    let status = parse_status_code(headers).ok_or(HttpError::MalformedResponse)?;

    Ok((status, String::from(headers), body))
}

/// Extract status code from the first line of headers.
fn parse_status_code(headers: &str) -> Option<u16> {
    // Find first space (after HTTP/1.x)
    let first_space = headers.find(' ')?;
    let rest = &headers[first_space + 1..];
    // Status code is the next 3 characters
    if rest.len() < 3 { return None; }
    let code_str = &rest[..3];
    let mut code: u16 = 0;
    for b in code_str.bytes() {
        if b < b'0' || b > b'9' { return None; }
        code = code * 10 + (b - b'0') as u16;
    }
    Some(code)
}

/// Check if status code is a redirect.
fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Extract a header value by name (case-insensitive).
/// — ShadePacket: "HTTP headers are case-insensitive per RFC 7230.
///   Anyone who says otherwise hasn't read the spec."
fn extract_header<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    for line in headers.split("\r\n") {
        if line.len() > name.len() + 1 {
            let prefix = &line[..name.len()];
            if prefix.eq_ignore_ascii_case(name) && line.as_bytes()[name.len()] == b':' {
                return Some(line[name.len() + 1..].trim());
            }
        }
    }
    None
}

/// Find a byte pattern in a buffer.
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > haystack.len() { return None; }
    for i in 0..=(haystack.len() - needle.len()) {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

/// Clean up connection resources.
fn cleanup(tls: Option<oxide_tls::TlsStream>, sock: i32) {
    if let Some(mut stream) = tls {
        let _ = stream.shutdown();
    }
    shutdown(sock, shut::RDWR);
    libc::close(sock);
}
