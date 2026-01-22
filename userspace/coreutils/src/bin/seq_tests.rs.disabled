#![cfg(test)]

use super::*;

fn run_seq(args: &[&str]) -> (i32, String) {
    let mut argv: Vec<*const u8> = Vec::new();
    argv.push(b"seq\0".as_ptr());
    for a in args {
        argv.push((a.to_string() + "\0").as_ptr());
    }
    let code = unsafe { main((argv.len()) as i32, argv.as_ptr()) };
    (code, String::new())
}

#[test]
fn basic_increasing_sequence() {
    let (code, _out) = run_seq(&["1", "3"]);
    assert_eq!(code, 0);
}

#[test]
fn zero_increment_is_error() {
    let (code, _out) = run_seq(&["1", "0", "5"]);
    assert_ne!(code, 0);
}
