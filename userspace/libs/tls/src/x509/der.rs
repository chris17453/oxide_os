//! ASN.1 DER (Distinguished Encoding Rules) parser.
//!
//! Streaming Tag-Length-Value decoder for the encoding format that X.509
//! certificates are serialized in. Every byte is suspect, every length field
//! a potential buffer overrun waiting to happen.
//! — VeilAudit: "DER: where one wrong length byte owns your entire trust chain."

use alloc::string::String;
use alloc::vec::Vec;

// — VeilAudit: "Known OIDs. Memorized like mugshots in a police lineup."
pub const OID_SHA256_RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b];
pub const OID_SHA384_RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0c];
pub const OID_SHA512_RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0d];
pub const OID_ECDSA_SHA256: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02];
pub const OID_ECDSA_SHA384: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x03];
pub const OID_ED25519: &[u8] = &[0x2b, 0x65, 0x70];
pub const OID_RSA_ENCRYPTION: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
pub const OID_EC_PUBLIC_KEY: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
pub const OID_PRIME256V1: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
pub const OID_COMMON_NAME: &[u8] = &[0x55, 0x04, 0x03];
pub const OID_ORGANIZATION: &[u8] = &[0x55, 0x04, 0x0a];
pub const OID_COUNTRY: &[u8] = &[0x55, 0x04, 0x06];
pub const OID_SUBJECT_ALT_NAME: &[u8] = &[0x55, 0x1d, 0x11];
pub const OID_BASIC_CONSTRAINTS: &[u8] = &[0x55, 0x1d, 0x13];
pub const OID_KEY_USAGE: &[u8] = &[0x55, 0x1d, 0x0f];

/// ASN.1 tag identifiers.
/// — VeilAudit: "Tags are just type IDs wearing trench coats."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    /// BOOLEAN (0x01)
    Boolean,
    /// INTEGER (0x02) — the backbone of every RSA key
    Integer,
    /// BIT STRING (0x03) — because sometimes you need to count unused bits
    BitString,
    /// OCTET STRING (0x04)
    OctetString,
    /// NULL (0x05) — the nihilism of ASN.1
    Null,
    /// OBJECT IDENTIFIER (0x06) — dotted-decimal identity crisis
    Oid,
    /// UTF8String (0x0C)
    Utf8String,
    /// PrintableString (0x13)
    PrintableString,
    /// IA5String (0x16)
    Ia5String,
    /// UTCTime (0x17) — Y2K called, it wants its time format back
    UtcTime,
    /// GeneralizedTime (0x18)
    GeneralizedTime,
    /// SEQUENCE (0x30) — constructed, ordered container
    Sequence,
    /// SET (0x31) — constructed, unordered container
    Set,
    /// Context-specific constructed tag [0]-[15] (0xA0-0xAF)
    /// — VeilAudit: "The 'it depends' of ASN.1 tagging."
    ContextSpecific(u8),
    /// Primitive context-specific tag [0]-[15] (0x80-0x8F)
    ContextPrimitive(u8),
}

/// A parsed DER element: tag + raw value bytes.
/// — VeilAudit: "Every element is a potential payload. Treat accordingly."
#[derive(Debug, Clone)]
pub struct DerElement<'a> {
    pub tag: Tag,
    pub data: &'a [u8],
}

/// Streaming DER parser. Advances through a byte slice one TLV at a time.
/// — VeilAudit: "Position tracking in untrusted data. What could go wrong."
pub struct DerParser<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> DerParser<'a> {
    /// Create a new parser over the given byte slice.
    pub fn new(data: &'a [u8]) -> Self {
        DerParser { data, pos: 0 }
    }

    /// How many bytes remain unparsed.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Current position in the data.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Whether we've consumed all data.
    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// Peek at the next byte without consuming it.
    #[allow(dead_code)]
    fn peek(&self) -> Option<u8> {
        if self.pos < self.data.len() {
            Some(self.data[self.pos])
        } else {
            None
        }
    }

    /// Consume one byte.
    fn read_byte(&mut self) -> Option<u8> {
        if self.pos < self.data.len() {
            let b = self.data[self.pos];
            self.pos += 1;
            Some(b)
        } else {
            None
        }
    }

    /// Parse a DER length field.
    /// Short form: single byte < 0x80 is the length.
    /// Long form: first byte 0x80 | n means n subsequent bytes encode the length.
    /// — VeilAudit: "Length fields: the #1 cause of buffer overreads since 1988."
    fn parse_length(&mut self) -> Option<usize> {
        let first = self.read_byte()?;
        if first < 0x80 {
            // — VeilAudit: "Short form. Simple. Suspicious."
            Some(first as usize)
        } else if first == 0x80 {
            // Indefinite length — not valid in DER
            // — VeilAudit: "Indefinite length in DER? That's a paddlin'."
            None
        } else {
            let num_bytes = (first & 0x7F) as usize;
            if num_bytes > 4 {
                // — VeilAudit: "A length that needs >4 bytes? In a certificate?
                //   Either you're parsing a DVD or someone's fuzzing us."
                return None;
            }
            let mut length: usize = 0;
            for _ in 0..num_bytes {
                let b = self.read_byte()?;
                length = length.checked_shl(8)?.checked_add(b as usize)?;
            }
            Some(length)
        }
    }

    /// Decode a tag byte into our Tag enum.
    /// — VeilAudit: "Tag decoding: where 0x30 means 'here comes everything'."
    fn decode_tag(tag_byte: u8) -> Tag {
        match tag_byte {
            0x01 => Tag::Boolean,
            0x02 => Tag::Integer,
            0x03 => Tag::BitString,
            0x04 => Tag::OctetString,
            0x05 => Tag::Null,
            0x06 => Tag::Oid,
            0x0C => Tag::Utf8String,
            0x13 => Tag::PrintableString,
            0x16 => Tag::Ia5String,
            0x17 => Tag::UtcTime,
            0x18 => Tag::GeneralizedTime,
            0x30 => Tag::Sequence,
            0x31 => Tag::Set,
            // Constructed context-specific: 0xA0..0xAF
            b if (b & 0xE0) == 0xA0 => Tag::ContextSpecific(b & 0x1F),
            // Primitive context-specific: 0x80..0x8F
            b if (b & 0xE0) == 0x80 => Tag::ContextPrimitive(b & 0x1F),
            // — VeilAudit: "Unknown tag? Log it, distrust it, move on."
            _ => Tag::OctetString, // treat unknown as opaque bytes
        }
    }

    /// Read the next TLV element from the stream.
    /// Returns None on EOF or malformed data.
    /// — VeilAudit: "Every call to next() is a trust decision."
    pub fn next(&mut self) -> Option<DerElement<'a>> {
        if self.pos >= self.data.len() {
            return None;
        }

        let tag_byte = self.read_byte()?;
        let tag = Self::decode_tag(tag_byte);
        let length = self.parse_length()?;

        // Bounds check — don't let a lying length field read past the end
        // — VeilAudit: "Trust but verify. Actually, just verify."
        if self.pos.checked_add(length)? > self.data.len() {
            return None;
        }

        let data = &self.data[self.pos..self.pos + length];
        self.pos += length;

        Some(DerElement { tag, data })
    }

    /// Read the next element and verify it has the expected tag.
    /// — VeilAudit: "Expecting a SEQUENCE but got an INTEGER? Someone's lying."
    pub fn expect(&mut self, expected: Tag) -> Option<DerElement<'a>> {
        let elem = self.next()?;
        if elem.tag == expected {
            Some(elem)
        } else {
            None
        }
    }

    /// Create a sub-parser for the value bytes of a constructed element.
    /// Use this to descend into SEQUENCEs and SETs.
    /// — VeilAudit: "Entering a SEQUENCE is like opening a Russian nesting doll
    ///   of trust issues."
    pub fn enter(element: &DerElement<'a>) -> DerParser<'a> {
        DerParser::new(element.data)
    }

    /// Skip forward by `n` bytes.
    pub fn skip(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.data.len());
    }

    /// Get the underlying data slice from current position.
    pub fn remaining_data(&self) -> &'a [u8] {
        if self.pos < self.data.len() {
            &self.data[self.pos..]
        } else {
            &[]
        }
    }

    /// Read raw bytes from a specific range in the original data.
    /// Used to capture raw TBSCertificate bytes for signature verification.
    /// — VeilAudit: "Raw bytes. No interpretation. No tampering. The only
    ///   honest representation."
    pub fn slice(&self, start: usize, end: usize) -> Option<&'a [u8]> {
        if end <= self.data.len() && start <= end {
            Some(&self.data[start..end])
        } else {
            None
        }
    }
}

/// Decoded OID — stores the raw DER-encoded bytes for fast comparison
/// plus a human-readable dotted string for diagnostics.
/// — VeilAudit: "OIDs: the social security numbers of cryptographic algorithms."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Oid {
    pub raw: Vec<u8>,
    pub dotted: String,
}

impl Oid {
    /// Decode OID bytes into dotted notation (e.g., "1.2.840.113549.1.1.11").
    /// The first byte encodes two components: first = byte / 40, second = byte % 40.
    /// Subsequent bytes use base-128 with high-bit continuation.
    /// — VeilAudit: "OID encoding: proof that ASN.1 was designed by committee
    ///   at 2 AM on a Friday."
    pub fn from_der(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        let raw = Vec::from(data);
        let mut dotted = String::new();

        // First octet encodes two components
        let first = data[0];
        let c1 = first / 40;
        let c2 = first % 40;

        // — VeilAudit: "Two components in one byte. Because efficiency."
        push_component(&mut dotted, c1 as u64, true);
        push_component(&mut dotted, c2 as u64, false);

        // Remaining components: variable-length base-128
        let mut i = 1;
        while i < data.len() {
            let mut value: u64 = 0;
            loop {
                if i >= data.len() {
                    // — VeilAudit: "Truncated OID. Somebody's fuzzing us."
                    return None;
                }
                let byte = data[i];
                i += 1;
                value = value.checked_shl(7)?.checked_add((byte & 0x7F) as u64)?;
                if byte & 0x80 == 0 {
                    break;
                }
            }
            push_component(&mut dotted, value, false);
        }

        Some(Oid { raw, dotted })
    }

    /// Check if this OID matches a known raw encoding.
    pub fn matches(&self, known: &[u8]) -> bool {
        self.raw == known
    }
}

/// Push a numeric component onto a dotted OID string.
fn push_component(s: &mut String, value: u64, first: bool) {
    if !first {
        s.push('.');
    }
    // Manual u64 -> string without format! dependency on std
    // — VeilAudit: "No format! in no_std. We do things the hard way."
    if value == 0 {
        s.push('0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut pos = 20;
    let mut v = value;
    while v > 0 {
        pos -= 1;
        buf[pos] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    for &b in &buf[pos..] {
        s.push(b as char);
    }
}

/// Parse a DER-encoded INTEGER, stripping any leading zero padding byte.
/// — VeilAudit: "Leading zeros in integers: ASN.1's way of saying
///   'this number is positive, I promise'."
pub fn parse_integer(data: &[u8]) -> Vec<u8> {
    // Strip the leading 0x00 that DER adds for positive numbers with high bit set
    if data.len() > 1 && data[0] == 0x00 {
        Vec::from(&data[1..])
    } else {
        Vec::from(data)
    }
}

/// Parse a DER-encoded BIT STRING, returning the actual bit content.
/// First byte is the number of unused bits in the last byte.
/// — VeilAudit: "Unused bits. In a BIT STRING. Named by Captain Obvious."
pub fn parse_bitstring(data: &[u8]) -> Option<&[u8]> {
    if data.is_empty() {
        return None;
    }
    // First byte = number of unused bits (0 for byte-aligned data)
    let _unused_bits = data[0];
    if data.len() < 2 {
        return None;
    }
    Some(&data[1..])
}

/// Parse a UTCTime string (YYMMDDHHMMSSZ) into a Unix timestamp.
/// — VeilAudit: "UTCTime: two-digit year. Because Y2K taught us nothing."
pub fn parse_utctime(data: &[u8]) -> Option<u64> {
    // Format: YYMMDDHHMMSSZ (13 bytes)
    if data.len() < 13 {
        return None;
    }

    let year = parse_2digit(data, 0)?;
    let month = parse_2digit(data, 2)?;
    let day = parse_2digit(data, 4)?;
    let hour = parse_2digit(data, 6)?;
    let minute = parse_2digit(data, 8)?;
    let second = parse_2digit(data, 10)?;

    // RFC 5280: YY >= 50 means 19YY, YY < 50 means 20YY
    // — VeilAudit: "The 2050 problem. We'll deal with it in 2049."
    let full_year = if year >= 50 { 1900 + year } else { 2000 + year };

    date_to_unix(full_year, month, day, hour, minute, second)
}

/// Parse a GeneralizedTime string (YYYYMMDDHHMMSSZ) into a Unix timestamp.
/// — VeilAudit: "GeneralizedTime: four-digit year. Progress."
pub fn parse_generalized_time(data: &[u8]) -> Option<u64> {
    if data.len() < 15 {
        return None;
    }

    let year = parse_4digit(data, 0)?;
    let month = parse_2digit(data, 4)?;
    let day = parse_2digit(data, 6)?;
    let hour = parse_2digit(data, 8)?;
    let minute = parse_2digit(data, 10)?;
    let second = parse_2digit(data, 12)?;

    date_to_unix(year, month, day, hour, minute, second)
}

/// Parse two ASCII digits at a given offset.
fn parse_2digit(data: &[u8], offset: usize) -> Option<u64> {
    if offset + 1 >= data.len() {
        return None;
    }
    let d1 = (data[offset] as char).to_digit(10)? as u64;
    let d2 = (data[offset + 1] as char).to_digit(10)? as u64;
    Some(d1 * 10 + d2)
}

/// Parse four ASCII digits at a given offset.
fn parse_4digit(data: &[u8], offset: usize) -> Option<u64> {
    if offset + 3 >= data.len() {
        return None;
    }
    let d1 = (data[offset] as char).to_digit(10)? as u64;
    let d2 = (data[offset + 1] as char).to_digit(10)? as u64;
    let d3 = (data[offset + 2] as char).to_digit(10)? as u64;
    let d4 = (data[offset + 3] as char).to_digit(10)? as u64;
    Some(d1 * 1000 + d2 * 100 + d3 * 10 + d4)
}

/// Convert a date to Unix timestamp.
/// — VeilAudit: "Manual calendar math. Because chrono is 400KB of bloat."
fn date_to_unix(year: u64, month: u64, day: u64, hour: u64, minute: u64, second: u64) -> Option<u64> {
    if month < 1 || month > 12 || day < 1 || day > 31 || hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    // Days per month (non-leap)
    let days_in_month: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    // Count days from epoch (1970-01-01)
    let mut days: u64 = 0;

    // Years
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }

    // Months in current year
    for m in 1..month {
        days += days_in_month[(m - 1) as usize];
        if m == 2 && is_leap(year) {
            days += 1;
        }
    }

    // Days (1-indexed)
    days += day - 1;

    Some(days * 86400 + hour * 3600 + minute * 60 + second)
}

/// Is it a leap year?
fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
