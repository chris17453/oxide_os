//! seq - print a sequence of numbers

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintln("usage: seq [first [incr]] last");
        return 1;
    }

    let (first, incr, last) = match argc {
        2 => {
            let last = parse_arg(unsafe { *argv.add(1) });
            (1, 1, last)
        }
        3 => {
            let first = parse_arg(unsafe { *argv.add(1) });
            let last = parse_arg(unsafe { *argv.add(2) });
            (first, 1, last)
        }
        _ => {
            let first = parse_arg(unsafe { *argv.add(1) });
            let incr = parse_arg(unsafe { *argv.add(2) });
            let last = parse_arg(unsafe { *argv.add(3) });
            (first, incr, last)
        }
    };

    if incr == 0 {
        eprintln("seq: zero increment");
        return 1;
    }

    let mut i = first;
    if incr > 0 {
        while i <= last {
            print_i64(i);
            println("");
            i += incr;
        }
    } else {
        while i >= last {
            print_i64(i);
            println("");
            i += incr;
        }
    }

    0
}

fn parse_arg(ptr: *const u8) -> i64 {
    if ptr.is_null() {
        return 0;
    }
    let mut i = 0;
    let negative = unsafe { *ptr == b'-' };
    if negative {
        i = 1;
    }
    let mut result: i64 = 0;
    loop {
        let c = unsafe { *ptr.add(i) };
        if c == 0 {
            break;
        }
        if c >= b'0' && c <= b'9' {
            result = result * 10 + (c - b'0') as i64;
        }
        i += 1;
    }
    if negative { -result } else { result }
}
