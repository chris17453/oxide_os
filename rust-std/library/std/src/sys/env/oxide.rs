//! — NeonRoot: Environment variable access for std::env.
pub use super::common::Env;
use crate::ffi::{OsStr, OsString};
use crate::io;

pub fn env() -> Env {
    let mut rust_env = vec![];
    oxide_rt::env::env_iter(|k, v| {
        let key = String::from_utf8_lossy(k).into_owned();
        let val = String::from_utf8_lossy(v).into_owned();
        rust_env.push((OsString::from(key), OsString::from(val)));
    });
    Env::new(rust_env)
}

pub fn getenv(key: &OsStr) -> Option<OsString> {
    let key_bytes = key.as_encoded_bytes();
    oxide_rt::env::getenv_bytes(key_bytes).map(|v| {
        OsString::from(String::from_utf8_lossy(v).into_owned())
    })
}

pub unsafe fn setenv(key: &OsStr, val: &OsStr) -> io::Result<()> {
    let k = key.as_encoded_bytes();
    let v = val.as_encoded_bytes();
    oxide_rt::env::setenv_bytes(k, v);
    Ok(())
}

pub unsafe fn unsetenv(key: &OsStr) -> io::Result<()> {
    let k = key.as_encoded_bytes();
    oxide_rt::env::unsetenv_bytes(k);
    Ok(())
}
