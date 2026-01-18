# Phase 21: Security

**Stage:** 4 - Advanced
**Status:** Complete (x86_64)
**Dependencies:** Phase 11 (Storage)

---

## Goal

Implement cryptographic security features including signing, encryption, and trust management.

---

## Deliverables

| Item | Status |
|------|--------|
| X.509 certificate management | [x] |
| Ed25519 file signing | [x] |
| AES-256-GCM encryption | [x] |
| ChaCha20-Poly1305 encryption | [x] |
| Trust store with revocation | [x] |
| Quarantine system | [x] |
| Trust sharing (QR, NFC, file) | [x] |

---

## Architecture Status

| Arch | Certs | Signing | Encrypt | Trust | Done |
|------|-------|---------|---------|-------|------|
| x86_64 | [x] | [x] | [x] | [x] | [x] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Security Architecture

```
┌─────────────────────────────────────────────────────┐
│                  Trust Store                         │
│  ┌────────────────────────────────────────────┐     │
│  │ Trusted Keys (Ed25519 public keys)         │     │
│  │ - System keys (OS vendor)                  │     │
│  │ - User keys (imported)                     │     │
│  │ - Revocation list                          │     │
│  └────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────┘
                       │
          ┌────────────┼────────────┐
          ▼            ▼            ▼
┌──────────────┐ ┌──────────────┐ ┌──────────────┐
│ File Signing │ │  Encryption  │ │  Quarantine  │
│  (Ed25519)   │ │ (AES/ChaCha) │ │   System     │
└──────────────┘ └──────────────┘ └──────────────┘
```

---

## Ed25519 Signing

```rust
// Key generation
pub fn generate_keypair() -> (SecretKey, PublicKey);

// Signing
pub fn sign(message: &[u8], secret_key: &SecretKey) -> Signature;

// Verification
pub fn verify(message: &[u8], signature: &Signature, public_key: &PublicKey) -> bool;

// File signature format
#[repr(C)]
pub struct FileSignature {
    magic: [u8; 4],           // "ESIG"
    version: u32,             // 1
    algorithm: u32,           // 1 = Ed25519
    key_id: [u8; 32],         // SHA-256 of public key
    timestamp: u64,           // Unix timestamp
    signature: [u8; 64],      // Ed25519 signature
    // Signature covers: file content || metadata
}
```

---

## Encryption

```rust
// AES-256-GCM
pub fn aes_encrypt(key: &[u8; 32], nonce: &[u8; 12], plaintext: &[u8], aad: &[u8]) -> Vec<u8>;
pub fn aes_decrypt(key: &[u8; 32], nonce: &[u8; 12], ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>>;

// ChaCha20-Poly1305
pub fn chacha_encrypt(key: &[u8; 32], nonce: &[u8; 12], plaintext: &[u8], aad: &[u8]) -> Vec<u8>;
pub fn chacha_decrypt(key: &[u8; 32], nonce: &[u8; 12], ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>>;

// Encrypted file format
#[repr(C)]
pub struct EncryptedFile {
    magic: [u8; 4],           // "EENC"
    version: u32,             // 1
    algorithm: u32,           // 1=AES-256-GCM, 2=ChaCha20-Poly1305
    nonce: [u8; 12],          // Random nonce
    key_derivation: u32,      // 1=password, 2=key_id
    // If password: salt, iterations for Argon2id
    // If key_id: recipient public key for X25519
    ciphertext_len: u64,
    // ciphertext follows (includes auth tag)
}
```

---

## Trust Store

```rust
pub struct TrustStore {
    /// Trusted public keys
    keys: HashMap<KeyId, TrustedKey>,

    /// Revoked keys
    revoked: HashSet<KeyId>,

    /// Trust levels
    levels: HashMap<KeyId, TrustLevel>,
}

pub struct TrustedKey {
    pub public_key: PublicKey,
    pub name: String,
    pub email: Option<String>,
    pub fingerprint: [u8; 32],
    pub created: Timestamp,
    pub expires: Option<Timestamp>,
}

pub enum TrustLevel {
    System,      // OS vendor, full trust
    User,        // User imported, prompted on first use
    Once,        // One-time trust
    Untrusted,   // Explicitly untrusted
}

// Storage: ~/.efflux/trust/
// - keys/       - Public key files
// - revoked/    - Revocation entries
// - config      - Trust configuration
```

---

## Quarantine System

```rust
// Files from untrusted sources are quarantined
pub struct QuarantineEntry {
    pub path: PathBuf,
    pub original_path: PathBuf,
    pub source: QuarantineSource,
    pub timestamp: Timestamp,
    pub hash: [u8; 32],
    pub status: QuarantineStatus,
}

pub enum QuarantineSource {
    ExternalMedia(String),    // USB drive ID
    Network(IpAddr),          // Download source
    Email(String),            // Sender
    Unknown,
}

pub enum QuarantineStatus {
    Pending,                  // Awaiting user decision
    Approved,                 // User approved
    Rejected,                 // User rejected
    AutoApproved(String),     // Auto-approved (signed by trusted key)
}

// Quarantine directory: /var/quarantine/
// Files are renamed to hash, original path stored in metadata
```

---

## X.509 Certificates

```rust
pub struct Certificate {
    pub version: u8,
    pub serial: Vec<u8>,
    pub issuer: Name,
    pub subject: Name,
    pub not_before: Timestamp,
    pub not_after: Timestamp,
    pub public_key: PublicKey,
    pub extensions: Vec<Extension>,
    pub signature: Vec<u8>,
}

// Operations
pub fn parse_pem(pem: &str) -> Result<Certificate>;
pub fn parse_der(der: &[u8]) -> Result<Certificate>;
pub fn verify_chain(cert: &Certificate, trust_anchors: &[Certificate]) -> Result<()>;
pub fn check_revocation(cert: &Certificate, crl: &Crl) -> Result<()>;
```

---

## Trust Sharing

```rust
// Export trust for sharing
pub enum TrustExport {
    // QR code (up to ~2KB)
    QrCode {
        public_key: PublicKey,
        name: String,
        fingerprint: [u8; 32],
    },

    // NFC (NDEF record)
    Nfc {
        public_key: PublicKey,
        name: String,
    },

    // File export
    File {
        keys: Vec<TrustedKey>,
        format: ExportFormat,  // PEM, DER, JSON
    },
}

// Import process
pub fn import_trust(export: TrustExport) -> Result<()> {
    // 1. Parse and validate
    // 2. Show fingerprint to user
    // 3. User confirms
    // 4. Add to trust store
}
```

---

## Key Files

```
crates/security/efflux-crypto/src/
├── lib.rs
├── ed25519.rs         # Ed25519 signing
├── aes.rs             # AES-256-GCM
├── chacha.rs          # ChaCha20-Poly1305
├── x25519.rs          # Key exchange
└── argon2.rs          # Password hashing

crates/security/efflux-trust/src/
├── lib.rs
├── store.rs           # Trust store
├── key.rs             # Key management
├── revoke.rs          # Revocation
└── share.rs           # Trust sharing

crates/security/efflux-quarantine/src/
├── lib.rs
├── entry.rs           # Quarantine entries
├── policy.rs          # Auto-approval rules
└── ui.rs              # User prompts

crates/security/efflux-x509/src/
├── lib.rs
├── cert.rs            # Certificate parsing
├── chain.rs           # Chain validation
└── crl.rs             # Revocation lists

userspace/security/
├── efflux-sign        # Sign files
├── efflux-verify      # Verify signatures
├── efflux-encrypt     # Encrypt files
├── efflux-decrypt     # Decrypt files
└── efflux-trust       # Trust management
```

---

## Syscalls/APIs

| Name | Description |
|------|-------------|
| verify_signature | Verify file signature |
| get_trust_level | Get trust level for key |
| quarantine_check | Check if file is quarantined |
| quarantine_approve | Approve quarantined file |

---

## Exit Criteria

- [x] Ed25519 signing and verification works
- [x] AES-256-GCM encryption/decryption works
- [x] ChaCha20-Poly1305 encryption/decryption works
- [x] Trust store manages keys
- [x] Revocation prevents use of compromised keys
- [x] Quarantine blocks untrusted files
- [x] Trust can be shared via QR/file
- [ ] Works on all 8 architectures

---

## Test: Sign and Verify

```bash
# Generate key pair
$ efflux-sign --generate-key
Generated key pair:
  Public: ~/.efflux/keys/mykey.pub
  Secret: ~/.efflux/keys/mykey.sec
  Fingerprint: a1b2c3d4...

# Sign a file
$ efflux-sign --sign mykey.sec important.txt
Signed: important.txt.sig

# Verify signature
$ efflux-verify important.txt important.txt.sig
Signature valid
  Signed by: mykey
  Timestamp: 2025-01-18 12:00:00
  Fingerprint: a1b2c3d4...

# Import someone else's key
$ efflux-trust --import friend.pub
Importing key:
  Name: Friend
  Fingerprint: x1y2z3w4...
Trust this key? [y/N] y
Key added to trust store.

# Verify their signed file
$ efflux-verify package.tar package.tar.sig
Signature valid
  Signed by: Friend
  Trust level: User
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 21 of EFFLUX Implementation*
