//! — ThreadRogue: Command-line argument retrieval for std::env::args().
pub use super::common::Args;
use crate::ffi::OsString;

pub fn args() -> Args {
    let argc = oxide_rt::args::argc() as usize;
    let mut rust_args = Vec::new();
    for i in 0..argc {
        if let Some(bytes) = oxide_rt::args::arg(i) {
            let s = String::from_utf8_lossy(bytes).into_owned();
            rust_args.push(OsString::from(s));
        }
    }
    Args::new(rust_args)
}
