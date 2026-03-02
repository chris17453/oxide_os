//! OXIDE OS-specific extensions to primitives in the [`std::ffi`] module.
//!
//! OXIDE uses byte-oriented paths (like Unix), so OsStr is just bytes.
#![stable(feature = "rust1", since = "1.0.0")]

use crate::ffi::{OsStr, OsString};
use crate::sealed::Sealed;

/// OXIDE OS-specific extensions to [`OsString`].
#[stable(feature = "rust1", since = "1.0.0")]
pub trait OsStringExt: Sealed {
    /// Creates an `OsString` from a byte vector.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn from_vec(vec: Vec<u8>) -> Self;

    /// Yields the underlying byte vector of this `OsString`.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn into_vec(self) -> Vec<u8>;
}

#[stable(feature = "rust1", since = "1.0.0")]
impl OsStringExt for OsString {
    #[inline]
    fn from_vec(vec: Vec<u8>) -> OsString {
        unsafe { OsString::from_encoded_bytes_unchecked(vec) }
    }

    #[inline]
    fn into_vec(self) -> Vec<u8> {
        self.into_encoded_bytes()
    }
}

/// OXIDE OS-specific extensions to [`OsStr`].
#[stable(feature = "rust1", since = "1.0.0")]
pub trait OsStrExt: Sealed {
    /// Creates an `OsStr` from a byte slice.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn from_bytes(slice: &[u8]) -> &Self;

    /// Gets the underlying byte view of the `OsStr` slice.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn as_bytes(&self) -> &[u8];
}

#[stable(feature = "rust1", since = "1.0.0")]
impl OsStrExt for OsStr {
    #[inline]
    fn from_bytes(slice: &[u8]) -> &OsStr {
        unsafe { OsStr::from_encoded_bytes_unchecked(slice) }
    }

    #[inline]
    fn as_bytes(&self) -> &[u8] {
        self.as_encoded_bytes()
    }
}
