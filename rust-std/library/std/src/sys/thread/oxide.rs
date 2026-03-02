//! — ThreadRogue: Threading for std::thread — spawn, join, sleep, yield.
use crate::ffi::CStr;
use crate::io;
use crate::num::NonZeroUsize;
use crate::thread::ThreadInit;
use crate::time::Duration;

pub const DEFAULT_MIN_STACK_SIZE: usize = 256 * 1024;

pub struct Thread {
    // — ThreadRogue: OXIDE doesn't support thread joining yet,
    // so we just track the thread ID for now
    tid: i64,
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

impl Thread {
    pub unsafe fn new(_stack: usize, _init: Box<ThreadInit>) -> io::Result<Thread> {
        // — ThreadRogue: Thread creation via clone() not yet wired up in std path
        // For now, return unsupported
        Err(io::Error::UNSUPPORTED_PLATFORM)
    }

    pub fn join(self) {
        // — ThreadRogue: No join support yet
    }
}

pub fn set_name(_name: &CStr) {}

pub fn current_os_id() -> Option<u64> {
    Some(oxide_rt::os::gettid() as u64)
}

pub fn available_parallelism() -> io::Result<NonZeroUsize> {
    // — ThreadRogue: Report 1 CPU for now (safe default)
    Ok(unsafe { NonZeroUsize::new_unchecked(1) })
}

pub fn yield_now() {
    oxide_rt::thread::sched_yield();
}

pub fn sleep(dur: Duration) {
    let ts = oxide_rt::types::Timespec {
        tv_sec: dur.as_secs() as i64,
        tv_nsec: dur.subsec_nanos() as i64,
    };
    oxide_rt::thread::nanosleep(&ts, None);
}
