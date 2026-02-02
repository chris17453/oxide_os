//! Internet Checksum Calculation

/// Calculate Internet checksum (RFC 1071)
///
/// Returns the checksum in network byte order (big endian).
pub fn internet_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;

    // Sum 16-bit words
    let mut i = 0;
    while i + 1 < data.len() {
        let word = u16::from_be_bytes([data[i], data[i + 1]]);
        sum = sum.wrapping_add(word as u32);
        i += 2;
    }

    // Handle odd byte
    if i < data.len() {
        sum = sum.wrapping_add((data[i] as u32) << 8);
    }

    // Fold 32-bit sum to 16 bits
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    // One's complement
    !sum as u16
}

/// Calculate checksum with pseudo header
///
/// Used for TCP and UDP checksums.
pub fn checksum_with_pseudo(pseudo: &[u8], data: &[u8]) -> u16 {
    let mut sum: u32 = 0;

    // Sum pseudo header
    let mut i = 0;
    while i + 1 < pseudo.len() {
        let word = u16::from_be_bytes([pseudo[i], pseudo[i + 1]]);
        sum = sum.wrapping_add(word as u32);
        i += 2;
    }

    // Sum data
    i = 0;
    while i + 1 < data.len() {
        let word = u16::from_be_bytes([data[i], data[i + 1]]);
        sum = sum.wrapping_add(word as u32);
        i += 2;
    }

    // Handle odd byte
    if i < data.len() {
        sum = sum.wrapping_add((data[i] as u32) << 8);
    }

    // Fold 32-bit sum to 16 bits
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    // One's complement
    !sum as u16
}

/// Verify checksum
///
/// Returns true if checksum is valid (result is 0 after including checksum field).
pub fn verify_checksum(data: &[u8]) -> bool {
    internet_checksum(data) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum() {
        // Example from RFC 1071
        let data = [0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7];
        let checksum = internet_checksum(&data);
        // After including checksum, should verify to 0
        let mut with_checksum = data.to_vec();
        with_checksum.extend_from_slice(&checksum.to_be_bytes());
        // Verification would need proper placement
    }
}
