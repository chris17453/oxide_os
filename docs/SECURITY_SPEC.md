# OXIDE Security and Trust Specification

**Version:** 1.0  
**Status:** Draft  
**License:** MIT  

---

## 0) Overview

The OXIDE security model provides:

- File integrity verification (hashing)
- File signing (certificates)
- File encryption (per-file and full-disk)
- Trust hierarchy (PKI)
- Quarantine system (untrusted files)
- Trust sharing (peer-to-peer)

Security is built into the filesystem and kernel, not bolted on.

---

## 1) Design Principles

1. **Secure by default.** Unsigned executables are restricted. Encryption is easy.
2. **Trust is explicit.** Users must accept certificates before trusting signed files.
3. **Defense in depth.** Multiple layers: signing, hashing, encryption, quarantine.
4. **Usable security.** Security features must be easy to use or they won't be used.
5. **Offline capable.** All verification works without network access.
6. **Auditable.** All trust decisions are logged.

---

## 2) Cryptographic Algorithms

| Purpose | Algorithm | Notes |
|---------|-----------|-------|
| Hashing | BLAKE3 | Fast, secure, tree-hashable |
| Signing | Ed25519 | Fast, small keys/signatures |
| Key exchange | X25519 | For encrypted sharing |
| Symmetric encryption | AES-256-GCM | With AES-NI hardware |
| Symmetric encryption | ChaCha20-Poly1305 | Without AES hardware |
| Key derivation | Argon2id | Memory-hard, resists GPU |
| Certificates | X.509 v3 | Interoperability |

### 2.1 Default Parameters

```rust
pub const ARGON2_TIME_COST: u32 = 3;
pub const ARGON2_MEMORY_COST: u32 = 65536;  // 64 MiB
pub const ARGON2_PARALLELISM: u32 = 4;
pub const ARGON2_OUTPUT_LEN: usize = 32;
```

---

## 3) Trust Hierarchy

```
┌─────────────────────────────────────────────────────────┐
│                  OXIDE Root CA                         │
│         (Built into OS, immutable)                      │
└─────────────────────┬───────────────────────────────────┘
                      │
        ┌─────────────┴─────────────┐
        │                           │
┌───────▼───────┐           ┌───────▼───────┐
│   Vendor CAs   │           │   User CAs    │
│  (Third-party) │           │  (Personal)   │
└───────┬───────┘           └───────┬───────┘
        │                           │
┌───────▼───────┐           ┌───────▼───────┐
│  App Certs     │           │  User Certs   │
│  Driver Certs  │           │  Device Certs │
└───────────────┘           └───────────────┘
```

### 3.1 Trust Levels

```rust
pub enum TrustLevel {
    System,     // Signed by OXIDE root CA or vendor CA
    Vendor,     // Signed by approved vendor
    User,       // Signed by user-accepted certificate
    Untrusted,  // No signature or unknown signer
}
```

### 3.2 Trust Level Capabilities

| Level | Execute | System files | Drivers | Unrestricted |
|-------|---------|--------------|---------|--------------|
| System | Yes | Yes | Yes | Yes |
| Vendor | Yes | No | Yes* | No |
| User | Yes | No | No | No |
| Untrusted | Prompt | No | No | No |

*Vendor drivers require explicit user approval.

---

## 4) Certificate Management

### 4.1 Trust Store Layout

```
/etc/oxide/trust/
├── system/                     # Read-only, OS updates only
│   ├── root-ca.pem            # OXIDE root CA
│   └── vendor/                # Approved vendor CAs
├── user/                      # User-accepted certificates
├── revoked/                   # Revoked certificates
│   └── crl.pem               # Certificate Revocation List
├── trust.db                   # Trust policy database
└── audit.log                  # Trust decision log
```

### 4.2 Trust Entry Structure

```rust
pub struct TrustEntry {
    pub fingerprint: [u8; 32],
    pub cert: Vec<u8>,
    pub alias: Option<String>,
    pub trust_level: TrustLevel,
    pub trust_flags: TrustFlags,
    pub added_at: u64,
    pub expires_at: Option<u64>,
    pub allowed_paths: Vec<PathBuf>,
    pub notes: Option<String>,
}

bitflags! {
    pub struct TrustFlags: u32 {
        const ALLOW_EXEC       = 1 << 0;
        const ALLOW_SYSTEM     = 1 << 1;
        const ALLOW_DRIVERS    = 1 << 2;
        const ALLOW_ENCRYPTION = 1 << 3;
        const PERSONAL_ONLY    = 1 << 4;
    }
}
```

---

## 5) File Signing

### 5.1 Signature Payload

```rust
pub struct SignaturePayload {
    pub version: u8,
    pub hash_algo: u8,              // 0=BLAKE3
    pub content_hash: [u8; 32],
    pub file_size: u64,
    pub signed_at: u64,
    pub signer_fingerprint: [u8; 32],
    pub flags: u32,
}
```

### 5.2 Stored Signature Format

```rust
pub struct StoredSignature {
    pub magic: u32,                 // 0x45465347 "EFSG"
    pub version: u8,
    pub sign_algo: u8,              // 0=Ed25519
    pub payload_len: u16,
    pub signature_len: u16,
    pub cert_len: u16,
    pub payload: Vec<u8>,
    pub signature: [u8; 64],        // Ed25519 signature
    pub certificate: Vec<u8>,       // X.509 DER
}
```

### 5.3 Verification Results

```rust
pub enum VerifyResult {
    Valid { trust_level: TrustLevel, signer: String, signed_at: u64 },
    InvalidSignature,
    ContentModified,
    CertificateExpired,
    CertificateNotYetValid,
    Revoked,
    UnknownSigner,
    ChainError(String),
}
```

---

## 6) File Encryption

### 6.1 Key Hierarchy

```
Master Key (derived from passphrase via Argon2id)
    │
    ├── Metadata Encryption Key (MEK)
    │
    └── File Encryption Keys (FEK)
            └── Per-file unique key, encrypted with MEK
```

### 6.2 Key Slots

```rust
pub struct KeySlot {
    pub slot_id: u8,
    pub flags: u8,
    pub kdf_algo: u8,
    pub kdf_salt: [u8; 32],
    pub kdf_time_cost: u32,
    pub kdf_memory_cost: u32,
    pub kdf_parallelism: u8,
    pub encrypted_mek: [u8; 48],
    pub key_fingerprint: [u8; 8],
}
```

### 6.3 Hardware Key Support (Future)

```rust
pub enum KeySource {
    Passphrase(String),
    KeyFile(PathBuf),
    Tpm { pcr_mask: u32 },
    Yubikey { slot: u8 },
    Fido2 { credential_id: Vec<u8> },
}
```

---

## 7) Quarantine System

### 7.1 Quarantine Triggers

- Received from external source (USB, network, download)
- Signature invalid or untrusted signer
- Executable without signature
- Content hash mismatch
- Manual quarantine by user

### 7.2 Quarantine Storage

```
/var/oxide/quarantine/
└── <uuid>/
    ├── manifest.json       # Metadata
    ├── content.enc         # Encrypted content
    ├── original_meta.json  # Original metadata
    ├── signature.bin       # Original signature
    └── signer_cert.pem     # Signer certificate
```

### 7.3 Quarantine Operations

```rust
pub trait QuarantineManager {
    fn quarantine(&self, path: &Path, reason: QuarantineReason) -> Result<Uuid>;
    fn list(&self) -> Result<Vec<QuarantineEntry>>;
    fn inspect(&self, uuid: &Uuid) -> Result<QuarantineDetails>;
    fn accept(&self, uuid: &Uuid, dest: &Path, trust_signer: bool) -> Result<()>;
    fn reject(&self, uuid: &Uuid) -> Result<()>;
}
```

---

## 8) Trust Sharing

### 8.1 Sharing Methods

| Method | Use case |
|--------|----------|
| QR code | In-person, phone scan |
| NFC tap | Device-to-device |
| USB file | Offline transfer |
| mDNS | Local network discovery |
| Manual | Email fingerprint, verify |

### 8.2 Trust Exchange Protocol

```
Client                          Server
   │                               │
   │──── HELLO ───────────────────▶│
   │     (client fingerprint)      │
   │                               │
   │◀─── HELLO_ACK ────────────────│
   │     (server fingerprint,      │
   │      verification code)       │
   │                               │
   │     [User verifies code]      │
   │                               │
   │──── REQUEST_CERT ────────────▶│
   │                               │
   │◀─── CERT_RESPONSE ────────────│
   │                               │
   │──── ACK ─────────────────────▶│
```

### 8.3 Verification Code

6-digit code for out-of-band verification:

```rust
pub fn generate_verification_code(
    client_fingerprint: &[u8; 32],
    server_fingerprint: &[u8; 32],
    session_nonce: &[u8; 16],
) -> String {
    let input = [client_fingerprint, server_fingerprint, session_nonce].concat();
    let hash = blake3::hash(&input);
    let num = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
    format!("{:06}", num % 1000000)
}
```

---

## 9) Revocation

### 9.1 Revocation Methods

- Local CRL bundled with OS updates (system/vendor certs)
- Manual revocation for user certs
- Optional push to trusted peers

### 9.2 Revocation Propagation

```rust
pub struct RevocationNotification {
    pub fingerprint: [u8; 32],
    pub reason: RevocationReason,
    pub revoked_at: u64,
    pub revoker_fingerprint: [u8; 32],
    pub signature: Vec<u8>,
}
```

---

## 10) Execution Policy

### 10.1 Policy Levels

```rust
pub enum ExecPolicy {
    Permissive,     // Allow all
    Advisory,       // Warn on unsigned
    Standard,       // Require signature, prompt for unknown
    Strict,         // Require trusted signer
    Lockdown,       // System-signed only
}
```

### 10.2 Execution Decision

```rust
pub enum ExecDecision {
    Allow,
    AllowWithWarning { warning: String },
    Prompt { signer: String },
    Quarantine,
    Deny { reason: String },
}
```

---

## 11) Syscalls

### 11.1 Signing

```rust
pub fn sys_sign(path: *const u8, path_len: usize, flags: u32) -> Result<()>;
pub fn sys_verify(path: *const u8, path_len: usize, result: *mut VerifyResult) -> Result<()>;
pub fn sys_seal(path: *const u8, path_len: usize) -> Result<()>;
```

### 11.2 Encryption

```rust
pub fn sys_encrypt(path: *const u8, path_len: usize, key_slot: u32) -> Result<()>;
pub fn sys_decrypt(path: *const u8, path_len: usize) -> Result<()>;
pub fn sys_keyctl(cmd: KeyctlCmd, arg: *mut u8) -> Result<()>;
```

### 11.3 Trust Management

```rust
pub fn sys_trust_add(cert: *const u8, cert_len: usize, flags: TrustFlags) -> Result<()>;
pub fn sys_trust_remove(fingerprint: *const [u8; 32]) -> Result<()>;
pub fn sys_trust_revoke(fingerprint: *const [u8; 32], reason: RevocationReason) -> Result<()>;
pub fn sys_trust_query(fingerprint: *const [u8; 32], info: *mut TrustEntry) -> Result<()>;
```

### 11.4 Quarantine

```rust
pub fn sys_quarantine(path: *const u8, path_len: usize, reason: QuarantineReason) -> Result<Uuid>;
pub fn sys_quarantine_accept(uuid: *const Uuid, dest: *const u8, dest_len: usize, trust: bool) -> Result<()>;
pub fn sys_quarantine_reject(uuid: *const Uuid) -> Result<()>;
```

---

## 12) CLI Tools

### 12.1 oxide trust

```bash
oxide trust list [--level <level>]
oxide trust add <cert.pem> [--alias <name>]
oxide trust remove <fingerprint>
oxide trust revoke <fingerprint> --reason <reason>
oxide trust export [--qr]
oxide trust share --advertise
oxide trust discover
```

### 12.2 oxide sign

```bash
oxide sign <file>
oxide verify <file>
oxide seal <file>
oxide inspect <file>
```

### 12.3 oxide crypt

```bash
oxide encrypt <file>
oxide decrypt <file>
oxide crypt add-key
oxide crypt remove-key --slot <n>
oxide crypt change-key --slot <n>
```

### 12.4 oxide quarantine

```bash
oxide quarantine list
oxide quarantine inspect <uuid>
oxide quarantine accept <uuid> [--trust-signer]
oxide quarantine reject <uuid>
```

---

## 13) Implementation Phases

### Phase 1: Core Cryptography
- [ ] BLAKE3 hashing
- [ ] Ed25519 signing/verification
- [ ] AES-256-GCM encryption
- [ ] Argon2id key derivation
- [ ] X.509 parsing

### Phase 2: File Signing
- [ ] Signature creation
- [ ] Signature verification
- [ ] Certificate chain validation
- [ ] oxide.fs integration

### Phase 3: Trust Store
- [ ] Trust database
- [ ] Certificate management
- [ ] Revocation handling
- [ ] CLI tools

### Phase 4: Encryption
- [ ] Per-file encryption
- [ ] Key slot management
- [ ] CLI tools

### Phase 5: Quarantine
- [ ] Quarantine storage
- [ ] Quarantine triggers
- [ ] Accept/reject flow

### Phase 6: Trust Sharing
- [ ] Export/import
- [ ] QR code generation
- [ ] mDNS discovery
- [ ] Exchange protocol

### Phase 7: Execution Policy
- [ ] Policy configuration
- [ ] Kernel exec hook
- [ ] User prompts

### Phase 8: Hardware Keys
- [ ] TPM integration
- [ ] YubiKey support
- [ ] FIDO2 support

---

## 14) Testing

### Unit Tests
- Cryptographic operations
- Certificate parsing
- Signature creation/verification

### Integration Tests
- Full signing workflow
- Full encryption workflow
- Quarantine workflow
- Trust sharing workflow

### Security Tests
- Invalid signature detection
- Tampered content detection
- Expired/revoked certificate handling

---

*End of OXIDE Security and Trust Specification*
