//! Certificate Handling
//!
//! Provides basic self-signed certificate generation for RDP TLS.

use alloc::vec::Vec;
use crypto::{ed25519, random, sha256};

/// Self-signed certificate
#[derive(Clone)]
pub struct SelfSignedCert {
    /// Certificate data (DER encoded)
    pub certificate: Vec<u8>,
    /// Private key
    pub private_key: Vec<u8>,
}

impl SelfSignedCert {
    /// Generate a new self-signed certificate
    pub fn generate(common_name: &str) -> Self {
        // Generate Ed25519 key pair
        let keypair = ed25519::Keypair::generate(&random::random_bytes());

        // Build a minimal X.509 certificate structure
        // This is a simplified version for RDP compatibility
        let cert = build_certificate(common_name, &keypair);

        Self {
            certificate: cert,
            private_key: keypair.secret.as_bytes().to_vec(),
        }
    }

    /// Get certificate fingerprint (SHA-256)
    pub fn fingerprint(&self) -> [u8; 32] {
        sha256(&self.certificate)
    }
}

/// Certificate wrapper
#[derive(Clone)]
pub struct Certificate {
    /// DER encoded certificate
    pub der: Vec<u8>,
}

impl Certificate {
    /// Create from DER bytes
    pub fn from_der(der: Vec<u8>) -> Self {
        Self { der }
    }

    /// Get fingerprint
    pub fn fingerprint(&self) -> [u8; 32] {
        sha256(&self.der)
    }
}

/// Build a minimal X.509 certificate (DER encoded)
///
/// This creates a basic self-signed certificate suitable for RDP TLS.
fn build_certificate(common_name: &str, keypair: &ed25519::Keypair) -> Vec<u8> {
    let mut cert = Vec::with_capacity(512);

    // This is a simplified certificate structure
    // In production, you'd want proper X.509 encoding

    // SEQUENCE (Certificate)
    let cert_content = build_tbs_certificate(common_name, keypair);
    let signature = sign_certificate(&cert_content, keypair);

    // TBSCertificate
    cert.extend_from_slice(&cert_content);

    // signatureAlgorithm
    cert.extend_from_slice(&[
        0x30, 0x05, // SEQUENCE
        0x06, 0x03, 0x2B, 0x65, 0x70, // OID 1.3.101.112 (Ed25519)
    ]);

    // signatureValue (BIT STRING)
    cert.push(0x03); // BIT STRING tag
    cert.push((signature.len() + 1) as u8);
    cert.push(0x00); // unused bits
    cert.extend_from_slice(&signature);

    // Wrap in SEQUENCE
    wrap_sequence(&cert)
}

/// Build TBSCertificate structure
fn build_tbs_certificate(common_name: &str, keypair: &ed25519::Keypair) -> Vec<u8> {
    let mut tbs = Vec::with_capacity(256);

    // version [0] EXPLICIT INTEGER (v3 = 2)
    tbs.extend_from_slice(&[0xA0, 0x03, 0x02, 0x01, 0x02]);

    // serialNumber INTEGER
    let serial = random::random_bytes::<8>();
    tbs.push(0x02); // INTEGER tag
    tbs.push(0x08);
    tbs.extend_from_slice(&serial);

    // signature AlgorithmIdentifier (Ed25519)
    tbs.extend_from_slice(&[0x30, 0x05, 0x06, 0x03, 0x2B, 0x65, 0x70]);

    // issuer Name (RDN: CN=common_name)
    let issuer = build_name(common_name);
    tbs.extend_from_slice(&issuer);

    // validity
    tbs.extend_from_slice(&build_validity());

    // subject Name (same as issuer for self-signed)
    tbs.extend_from_slice(&issuer);

    // subjectPublicKeyInfo
    tbs.extend_from_slice(&build_public_key_info(keypair));

    wrap_sequence(&tbs)
}

/// Build Name (distinguished name)
fn build_name(common_name: &str) -> Vec<u8> {
    // RDNSequence containing CommonName
    let cn_bytes = common_name.as_bytes();

    let mut rdn = Vec::new();
    // OID for CommonName: 2.5.4.3
    rdn.extend_from_slice(&[0x06, 0x03, 0x55, 0x04, 0x03]);
    // UTF8String
    rdn.push(0x0C);
    rdn.push(cn_bytes.len() as u8);
    rdn.extend_from_slice(cn_bytes);

    let rdn_seq = wrap_sequence(&rdn);
    let rdn_set = wrap_set(&rdn_seq);

    wrap_sequence(&rdn_set)
}

/// Build validity period
fn build_validity() -> Vec<u8> {
    // Not Before: 20200101000000Z
    // Not After: 20300101000000Z
    let mut validity = Vec::new();

    // notBefore (UTCTime)
    validity.extend_from_slice(&[
        0x17, 0x0D, // UTCTime
        0x32, 0x30, 0x30, 0x31, 0x30, 0x31, // 200101
        0x30, 0x30, 0x30, 0x30, 0x30, 0x30, // 000000
        0x5A, // Z
    ]);

    // notAfter (UTCTime)
    validity.extend_from_slice(&[
        0x17, 0x0D, 0x33, 0x30, 0x30, 0x31, 0x30, 0x31, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x5A,
    ]);

    wrap_sequence(&validity)
}

/// Build SubjectPublicKeyInfo
fn build_public_key_info(keypair: &ed25519::Keypair) -> Vec<u8> {
    let mut spki = Vec::new();

    // algorithm AlgorithmIdentifier (Ed25519)
    spki.extend_from_slice(&[0x30, 0x05, 0x06, 0x03, 0x2B, 0x65, 0x70]);

    // subjectPublicKey BIT STRING
    let pubkey = keypair.public.as_bytes();
    spki.push(0x03); // BIT STRING
    spki.push((pubkey.len() + 1) as u8);
    spki.push(0x00); // unused bits
    spki.extend_from_slice(pubkey);

    wrap_sequence(&spki)
}

/// Sign certificate content
fn sign_certificate(tbs: &[u8], keypair: &ed25519::Keypair) -> Vec<u8> {
    let sig = ed25519::sign(tbs, keypair);
    sig.as_bytes().to_vec()
}

/// Wrap data in ASN.1 SEQUENCE
fn wrap_sequence(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len() + 4);
    result.push(0x30); // SEQUENCE tag

    if data.len() < 128 {
        result.push(data.len() as u8);
    } else if data.len() < 256 {
        result.push(0x81);
        result.push(data.len() as u8);
    } else {
        result.push(0x82);
        result.push((data.len() >> 8) as u8);
        result.push(data.len() as u8);
    }

    result.extend_from_slice(data);
    result
}

/// Wrap data in ASN.1 SET
fn wrap_set(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len() + 4);
    result.push(0x31); // SET tag

    if data.len() < 128 {
        result.push(data.len() as u8);
    } else if data.len() < 256 {
        result.push(0x81);
        result.push(data.len() as u8);
    } else {
        result.push(0x82);
        result.push((data.len() >> 8) as u8);
        result.push(data.len() as u8);
    }

    result.extend_from_slice(data);
    result
}
