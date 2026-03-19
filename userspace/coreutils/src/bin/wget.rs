//! wget — Download files from the web
//!
//! Thin CLI wrapper around oxide-http. All the real work (URL parsing, DNS,
//! TCP, TLS, HTTP, redirects) lives in the library where other tools can use it.
//!
//! — ShadePacket: "700 lines of wget became 120 lines of wget + a library.
//!   The first refactor that actually made things better."

#![no_std]
#![no_main]
#![allow(unused)]

use libc::*;
use oxide_http;
use oxide_http::url;

const MAX_FILENAME: usize = 128;

struct WgetConfig {
    quiet: bool,
    verbose: bool,
    output_file: Option<[u8; MAX_FILENAME]>,
}

impl WgetConfig {
    fn new() -> Self {
        WgetConfig {
            quiet: false,
            verbose: false,
            output_file: None,
        }
    }
}

fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() { return ""; }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 { len += 1; }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}

/// — ShadePacket: "All the HTTP complexity is in oxide_http now.
///   wget is just: parse args → call library → save to file."
fn do_wget(config: &WgetConfig, url_str: &str) -> i32 {
    // Parse URL for filename extraction
    let parsed = match url::parse_url(url_str) {
        Some(p) => p,
        None => {
            printlns("wget: invalid URL format");
            return 1;
        }
    };

    // Determine output filename
    let output_filename = if let Some(ref name_buf) = config.output_file {
        let len = name_buf.iter().position(|&b| b == 0).unwrap_or(MAX_FILENAME);
        core::str::from_utf8(&name_buf[..len]).unwrap_or("download.html")
    } else {
        url::extract_filename(parsed.path)
    };

    if !config.quiet {
        prints("Resolving ");
        prints(parsed.host);
        printlns("...");
    }

    // — ShadePacket: "One function call. DNS, TCP, TLS, HTTP, redirects — all handled."
    let response = match oxide_http::get(url_str) {
        Ok(resp) => resp,
        Err(e) => {
            prints("wget: ");
            match e {
                oxide_http::HttpError::InvalidUrl => printlns("invalid URL"),
                oxide_http::HttpError::DnsResolutionFailed => {
                    prints("unable to resolve '");
                    prints(parsed.host);
                    printlns("'");
                }
                oxide_http::HttpError::SocketError(code) => {
                    prints("socket error "); print_i64(code as i64); printlns("");
                }
                oxide_http::HttpError::ConnectionFailed(code) => {
                    prints("connection failed "); print_i64(code as i64); printlns("");
                }
                oxide_http::HttpError::TlsError(e) => {
                    prints("TLS failed: ");
                    match e {
                        oxide_tls::TlsError::HandshakeFailed(msg) => printlns(msg),
                        oxide_tls::TlsError::CertificateInvalid(msg) => {
                            prints("cert invalid: "); printlns(msg);
                        }
                        oxide_tls::TlsError::IoError(code) => {
                            prints("I/O error "); print_i64(code as i64); printlns("");
                        }
                        _ => printlns("unknown TLS error"),
                    }
                }
                oxide_http::HttpError::TooManyRedirects => printlns("too many redirects"),
                oxide_http::HttpError::SendFailed => printlns("failed to send request"),
                oxide_http::HttpError::ReceiveFailed => printlns("failed to receive response"),
                oxide_http::HttpError::MalformedResponse => printlns("malformed HTTP response"),
            }
            return 1;
        }
    };

    if !config.quiet {
        prints("HTTP/1.1 ");
        print_u64(response.status as u64);
        printlns(match response.status {
            200 => " OK",
            301 => " Moved Permanently",
            302 => " Found",
            304 => " Not Modified",
            400 => " Bad Request",
            403 => " Forbidden",
            404 => " Not Found",
            500 => " Internal Server Error",
            _ => "",
        });
    }

    // Save body to file
    let out_fd = open(output_filename, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if out_fd < 0 {
        prints("wget: cannot create file: ");
        printlns(output_filename);
        return 1;
    }
    let _ = write(out_fd, &response.body);
    close(out_fd);

    if !config.quiet {
        prints("Downloaded: ");
        print_u64(response.body.len() as u64);
        prints(" bytes saved to ");
        printlns(output_filename);
    }

    0
}

fn show_help() {
    printlns("Usage: wget [OPTIONS] URL");
    printlns("");
    printlns("Download files from the web.");
    printlns("");
    printlns("Options:");
    printlns("  -O FILE     Save to FILE (default: extract from URL)");
    printlns("  -q          Quiet mode");
    printlns("  -v          Verbose mode");
    printlns("  -h          Show this help");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = WgetConfig::new();
    let mut url: Option<&str> = None;
    let mut i = 1;

    while i < argc as usize {
        let arg = cstr_to_str(unsafe { *argv.add(i) });
        match arg {
            "-h" | "--help" => { show_help(); return 0; }
            "-q" => config.quiet = true,
            "-v" => config.verbose = true,
            "-O" => {
                i += 1;
                if i < argc as usize {
                    let filename = cstr_to_str(unsafe { *argv.add(i) });
                    let mut buf = [0u8; MAX_FILENAME];
                    let len = filename.len().min(MAX_FILENAME - 1);
                    buf[..len].copy_from_slice(&filename.as_bytes()[..len]);
                    config.output_file = Some(buf);
                } else {
                    printlns("wget: -O requires a filename");
                    return 1;
                }
            }
            _ => {
                if arg.starts_with('-') {
                    prints("wget: unknown option: ");
                    printlns(arg);
                    return 1;
                }
                url = Some(arg);
            }
        }
        i += 1;
    }

    match url {
        Some(u) => do_wget(&config, u),
        None => { printlns("wget: missing URL"); show_help(); 1 }
    }
}
