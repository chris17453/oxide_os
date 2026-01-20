//! date - print or set the system date and time

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let mut tv_sec: i64 = 0;
    let mut tv_usec: i64 = 0;

    if sys_gettimeofday(&mut tv_sec, &mut tv_usec) < 0 {
        eprintlns("date: cannot get time");
        return 1;
    }

    // Convert Unix timestamp to date/time
    // This is a simplified implementation
    let days_since_epoch = tv_sec / 86400;
    let time_of_day = tv_sec % 86400;

    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year/month/day (simplified - doesn't handle all edge cases)
    let (year, month, day) = days_to_ymd(days_since_epoch);

    // Day of week
    let dow = ((days_since_epoch + 4) % 7) as usize; // Jan 1, 1970 was Thursday
    let dow_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let month_names = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                       "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

    // Print: "Wed Jan 15 14:30:00 UTC 2025"
    prints(dow_names[dow]);
    prints(" ");
    prints(month_names[(month - 1) as usize]);
    prints(" ");
    print_num(day as u64, 2);
    prints(" ");
    print_num(hours as u64, 2);
    prints(":");
    print_num(minutes as u64, 2);
    prints(":");
    print_num(seconds as u64, 2);
    prints(" UTC ");
    print_u64(year as u64);
    printlns("");

    0
}

fn days_to_ymd(days: i64) -> (i32, i32, i32) {
    let mut remaining = days;
    let mut year = 1970i32;

    // Find year
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    // Find month
    let days_in_months = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1i32;
    for &days_in_month in &days_in_months {
        if remaining < days_in_month {
            break;
        }
        remaining -= days_in_month;
        month += 1;
    }

    let day = remaining as i32 + 1;

    (year, month, day)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn print_num(n: u64, width: usize) {
    let mut buf = [b'0'; 10];
    let mut val = n;
    let mut i = buf.len();

    if val == 0 {
        i -= 1;
    } else {
        while val > 0 {
            i -= 1;
            buf[i] = b'0' + (val % 10) as u8;
            val /= 10;
        }
    }

    // Pad with zeros
    let digits = buf.len() - i;
    for _ in digits..width {
        putchar(b'0');
    }

    for j in i..buf.len() {
        putchar(buf[j]);
    }
}
