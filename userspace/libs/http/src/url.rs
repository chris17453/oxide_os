//! URL parsing for HTTP client.
//!
//! Handles scheme://host[:port]/path decomposition.
//! — ShadePacket: "RFC 3986 says URLs are complicated. We handle the
//!   80% case that covers 99% of the internet."

/// Parsed URL components.
pub struct ParsedUrl<'a> {
    /// Scheme (http, https, ftp)
    pub scheme: &'a str,
    /// Hostname
    pub host: &'a str,
    /// Port number
    pub port: u16,
    /// Path (including leading /)
    pub path: &'a str,
}

impl<'a> ParsedUrl<'a> {
    /// Is this an HTTPS URL?
    pub fn is_https(&self) -> bool {
        self.port == 443 || self.scheme == "https"
    }
}

/// Parse a URL: [scheme://]host[:port][/path]
///
/// — ShadePacket: "Like Linux wget — scheme determines default port via
///   the equivalent of getservbyname(). Accepts bare hostnames (defaults
///   to http). Explicit :port overrides the scheme default."
pub fn parse_url(url: &str) -> Option<ParsedUrl<'_>> {
    let (rest, scheme, default_port) = if let Some(r) = url.strip_prefix("https://") {
        (r, "https", 443u16)
    } else if let Some(r) = url.strip_prefix("http://") {
        (r, "http", 80u16)
    } else if let Some(r) = url.strip_prefix("ftp://") {
        (r, "ftp", 21u16)
    } else {
        (url, "http", 80u16)
    };

    if rest.is_empty() {
        return None;
    }

    // Split host from path
    let (host_port, path) = if let Some(idx) = rest.find('/') {
        (&rest[..idx], &rest[idx..])
    } else {
        (rest, "/")
    };

    if host_port.is_empty() {
        return None;
    }

    // Explicit :port overrides scheme default
    let (host, port) = if let Some(idx) = host_port.find(':') {
        let port = parse_port(&host_port[idx + 1..])?;
        (&host_port[..idx], port)
    } else {
        (host_port, default_port)
    };

    Some(ParsedUrl { scheme, host, port, path })
}

/// Extract filename from URL path.
/// — ShadePacket: "Last path component. Default to index.html because
///   every web server since 1993 agrees on that convention."
pub fn extract_filename(path: &str) -> &str {
    if let Some(idx) = path.rfind('/') {
        let filename = &path[idx + 1..];
        if !filename.is_empty() {
            return filename;
        }
    }
    "index.html"
}

/// Parse port number from string.
fn parse_port(s: &str) -> Option<u16> {
    if s.is_empty() { return None; }
    let mut port: u32 = 0;
    for c in s.bytes() {
        if c < b'0' || c > b'9' { return None; }
        port = port * 10 + (c - b'0') as u32;
        if port > 65535 { return None; }
    }
    if port == 0 { None } else { Some(port as u16) }
}

/// Parse an IPv4 address from string (e.g., "192.168.1.1").
pub fn parse_ip(s: &str) -> Option<(u8, u8, u8, u8)> {
    let mut octets = [0u8; 4];
    let mut idx = 0;
    let mut current: u16 = 0;
    let mut has_digit = false;

    for c in s.bytes() {
        if c == b'.' {
            if !has_digit || idx >= 3 || current > 255 { return None; }
            octets[idx] = current as u8;
            idx += 1;
            current = 0;
            has_digit = false;
        } else if c >= b'0' && c <= b'9' {
            current = current * 10 + (c - b'0') as u16;
            has_digit = true;
            if current > 255 { return None; }
        } else {
            return None;
        }
    }

    if !has_digit || idx != 3 || current > 255 { return None; }
    octets[idx] = current as u8;
    Some((octets[0], octets[1], octets[2], octets[3]))
}
