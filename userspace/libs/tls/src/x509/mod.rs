//! X.509 certificate parsing and verification.
//!
//! The trust infrastructure for TLS. Parses DER-encoded certificates,
//! extracts public keys and SANs, and verifies certificate chains from
//! leaf through intermediates to a trusted root store.
//!
//! — VeilAudit: "X.509: a standard so overengineered it took 30 years
//!   to get widespread right. And people still mess it up."

pub mod der;
pub mod parser;
pub mod verify;

// Re-export the things callers actually need
pub use der::{DerElement, DerParser, Oid, Tag};
pub use parser::{
    Certificate, Extensions, Name, PublicKeyInfo, SignatureAlgorithm, Validity,
};
pub use verify::{verify_chain, VerifyError};
