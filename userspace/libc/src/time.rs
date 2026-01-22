//! Time functions

use crate::syscall;

/// Time value structure
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timespec {
    /// Seconds
    pub tv_sec: i64,
    /// Nanoseconds
    pub tv_nsec: i64,
}

impl Timespec {
    /// Create new timespec
    pub const fn new(sec: i64, nsec: i64) -> Self {
        Timespec {
            tv_sec: sec,
            tv_nsec: nsec,
        }
    }

    /// Create from milliseconds
    pub const fn from_millis(ms: i64) -> Self {
        Timespec {
            tv_sec: ms / 1000,
            tv_nsec: (ms % 1000) * 1_000_000,
        }
    }

    /// Convert to milliseconds
    pub const fn to_millis(&self) -> i64 {
        self.tv_sec * 1000 + self.tv_nsec / 1_000_000
    }

    /// Convert to nanoseconds
    pub const fn to_nanos(&self) -> i64 {
        self.tv_sec * 1_000_000_000 + self.tv_nsec
    }
}

/// Timeval structure (for select, etc.)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timeval {
    /// Seconds
    pub tv_sec: i64,
    /// Microseconds
    pub tv_usec: i64,
}

impl Timeval {
    /// Create new timeval
    pub const fn new(sec: i64, usec: i64) -> Self {
        Timeval {
            tv_sec: sec,
            tv_usec: usec,
        }
    }
}

/// Timezone structure
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timezone {
    /// Minutes west of Greenwich
    pub tz_minuteswest: i32,
    /// Type of DST correction
    pub tz_dsttime: i32,
}

/// Broken-down time
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct Tm {
    /// Seconds (0-59)
    pub tm_sec: i32,
    /// Minutes (0-59)
    pub tm_min: i32,
    /// Hours (0-23)
    pub tm_hour: i32,
    /// Day of month (1-31)
    pub tm_mday: i32,
    /// Month (0-11)
    pub tm_mon: i32,
    /// Year since 1900
    pub tm_year: i32,
    /// Day of week (0-6, Sunday = 0)
    pub tm_wday: i32,
    /// Day of year (0-365)
    pub tm_yday: i32,
    /// Daylight saving time flag
    pub tm_isdst: i32,
    /// Seconds east of UTC
    pub tm_gmtoff: i64,
    /// Timezone name
    pub tm_zone: *const u8,
}

/// Clock IDs
pub mod clocks {
    pub const CLOCK_REALTIME: i32 = 0;
    pub const CLOCK_MONOTONIC: i32 = 1;
    pub const CLOCK_PROCESS_CPUTIME_ID: i32 = 2;
    pub const CLOCK_THREAD_CPUTIME_ID: i32 = 3;
    pub const CLOCK_MONOTONIC_RAW: i32 = 4;
    pub const CLOCK_REALTIME_COARSE: i32 = 5;
    pub const CLOCK_MONOTONIC_COARSE: i32 = 6;
    pub const CLOCK_BOOTTIME: i32 = 7;
}

/// Get time
pub fn time(t: Option<&mut i64>) -> i64 {
    let mut ts = Timespec::default();
    let ret = clock_gettime(clocks::CLOCK_REALTIME, &mut ts);
    if ret < 0 {
        return -1;
    }
    if let Some(tp) = t {
        *tp = ts.tv_sec;
    }
    ts.tv_sec
}

/// Get time of day
pub fn gettimeofday(tv: &mut Timeval, tz: Option<&mut Timezone>) -> i32 {
    let mut ts = Timespec::default();
    let ret = clock_gettime(clocks::CLOCK_REALTIME, &mut ts);
    if ret < 0 {
        return -1;
    }
    tv.tv_sec = ts.tv_sec;
    tv.tv_usec = ts.tv_nsec / 1000;

    if let Some(tzp) = tz {
        *tzp = Timezone::default();
    }

    0
}

/// Get clock time
pub fn clock_gettime(clk_id: i32, tp: &mut Timespec) -> i32 {
    unsafe {
        syscall::syscall2(
            syscall::SYS_CLOCK_GETTIME,
            clk_id as usize,
            tp as *mut Timespec as usize,
        ) as i32
    }
}

/// Get clock resolution
pub fn clock_getres(clk_id: i32, res: &mut Timespec) -> i32 {
    unsafe {
        syscall::syscall2(
            syscall::SYS_CLOCK_GETRES,
            clk_id as usize,
            res as *mut Timespec as usize,
        ) as i32
    }
}

/// Sleep for specified time
pub fn nanosleep(req: &Timespec, rem: Option<&mut Timespec>) -> i32 {
    let rem_ptr = rem
        .map(|r| r as *mut Timespec)
        .unwrap_or(core::ptr::null_mut());
    unsafe {
        syscall::syscall2(
            syscall::SYS_NANOSLEEP,
            req as *const Timespec as usize,
            rem_ptr as usize,
        ) as i32
    }
}

/// Sleep for specified seconds
pub fn sleep(seconds: u32) -> u32 {
    let req = Timespec::new(seconds as i64, 0);
    let mut rem = Timespec::default();

    if nanosleep(&req, Some(&mut rem)) < 0 {
        rem.tv_sec as u32
    } else {
        0
    }
}

/// Sleep for specified microseconds
pub fn usleep(usec: u32) -> i32 {
    let req = Timespec::new(
        (usec / 1_000_000) as i64,
        ((usec % 1_000_000) * 1000) as i64,
    );
    nanosleep(&req, None)
}

/// Convert time_t to broken-down time (UTC)
pub fn gmtime_r<'a>(timer: &i64, result: &'a mut Tm) -> Option<&'a mut Tm> {
    let mut t = *timer;

    // Days since epoch
    let days = t / 86400;
    let rem = t % 86400;

    // Time of day
    result.tm_hour = (rem / 3600) as i32;
    result.tm_min = ((rem % 3600) / 60) as i32;
    result.tm_sec = (rem % 60) as i32;

    // Day of week (1970-01-01 was Thursday = 4)
    result.tm_wday = ((days + 4) % 7) as i32;

    // Year, month, day calculation
    let mut year = 1970i32;
    let mut remaining_days = days as i32;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    result.tm_year = year - 1900;
    result.tm_yday = remaining_days;

    // Month and day
    let leap = is_leap_year(year);
    let days_in_months = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0;
    for (i, &days) in days_in_months.iter().enumerate() {
        if remaining_days < days {
            month = i;
            break;
        }
        remaining_days -= days;
    }

    result.tm_mon = month as i32;
    result.tm_mday = remaining_days + 1;
    result.tm_isdst = 0;
    result.tm_gmtoff = 0;
    result.tm_zone = core::ptr::null();

    Some(result)
}

/// Check if year is leap year
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Convert broken-down time to time_t
pub fn mktime(tm: &mut Tm) -> i64 {
    let year = tm.tm_year + 1900;

    // Days from epoch to start of year
    let mut days = 0i64;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    // Days in current year
    let leap = is_leap_year(year);
    let days_in_months = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    for m in 0..tm.tm_mon as usize {
        if m < 12 {
            days += days_in_months[m] as i64;
        }
    }

    days += (tm.tm_mday - 1) as i64;

    // Convert to seconds
    days * 86400 + tm.tm_hour as i64 * 3600 + tm.tm_min as i64 * 60 + tm.tm_sec as i64
}

/// Get processor time
pub fn clock() -> i64 {
    let mut ts = Timespec::default();
    if clock_gettime(clocks::CLOCK_PROCESS_CPUTIME_ID, &mut ts) < 0 {
        return -1;
    }
    // Return in CLOCKS_PER_SEC units (1,000,000)
    ts.tv_sec * 1_000_000 + ts.tv_nsec / 1000
}

/// Clocks per second
pub const CLOCKS_PER_SEC: i64 = 1_000_000;
