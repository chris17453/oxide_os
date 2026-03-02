//! — WireSaint: Time primitives for std::time — Instant and SystemTime.
//! Built on CLOCK_MONOTONIC and CLOCK_REALTIME respectively.

use crate::time::Duration;
use crate::fmt;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    secs: u64,
    nanos: u32,
}

impl Instant {
    pub fn now() -> Self {
        let mut ts = oxide_rt::types::Timespec::zero();
        oxide_rt::time::clock_gettime(oxide_rt::time::CLOCK_MONOTONIC, &mut ts);
        Self {
            secs: ts.tv_sec as u64,
            nanos: ts.tv_nsec as u32,
        }
    }

    pub fn checked_sub_instant(&self, other: &Instant) -> Option<Duration> {
        let (secs, nanos) = if self.nanos >= other.nanos {
            (self.secs.checked_sub(other.secs)?, self.nanos - other.nanos)
        } else {
            (self.secs.checked_sub(other.secs)?.checked_sub(1)?, self.nanos + 1_000_000_000 - other.nanos)
        };
        Some(Duration::new(secs, nanos))
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<Instant> {
        let mut secs = self.secs.checked_add(other.as_secs())?;
        let mut nanos = self.nanos + other.subsec_nanos();
        if nanos >= 1_000_000_000 {
            nanos -= 1_000_000_000;
            secs = secs.checked_add(1)?;
        }
        Some(Instant { secs, nanos })
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<Instant> {
        let mut secs = self.secs.checked_sub(other.as_secs())?;
        let nanos = if self.nanos >= other.subsec_nanos() {
            self.nanos - other.subsec_nanos()
        } else {
            secs = secs.checked_sub(1)?;
            self.nanos + 1_000_000_000 - other.subsec_nanos()
        };
        Some(Instant { secs, nanos })
    }
}

impl fmt::Debug for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Instant({}.{:09})", self.secs, self.nanos)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemTime {
    secs: u64,
    nanos: u32,
}

pub const UNIX_EPOCH: SystemTime = SystemTime { secs: 0, nanos: 0 };

impl SystemTime {
    pub const MAX: SystemTime = SystemTime { secs: u64::MAX, nanos: 999_999_999 };
    pub const MIN: SystemTime = SystemTime { secs: 0, nanos: 0 };

    /// — WireSaint: Construct from raw seconds (for stat timestamps).
    pub fn from_secs(secs: u64) -> Self {
        Self { secs, nanos: 0 }
    }

    pub fn now() -> Self {
        let mut ts = oxide_rt::types::Timespec::zero();
        oxide_rt::time::clock_gettime(oxide_rt::time::CLOCK_REALTIME, &mut ts);
        Self {
            secs: ts.tv_sec as u64,
            nanos: ts.tv_nsec as u32,
        }
    }

    pub fn sub_time(&self, other: &SystemTime) -> Result<Duration, Duration> {
        if self >= other {
            let (secs, nanos) = if self.nanos >= other.nanos {
                (self.secs - other.secs, self.nanos - other.nanos)
            } else {
                (self.secs - other.secs - 1, self.nanos + 1_000_000_000 - other.nanos)
            };
            Ok(Duration::new(secs, nanos))
        } else {
            Err(other.sub_time(self).unwrap())
        }
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<SystemTime> {
        let mut secs = self.secs.checked_add(other.as_secs())?;
        let mut nanos = self.nanos + other.subsec_nanos();
        if nanos >= 1_000_000_000 {
            nanos -= 1_000_000_000;
            secs = secs.checked_add(1)?;
        }
        Some(SystemTime { secs, nanos })
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<SystemTime> {
        let mut secs = self.secs.checked_sub(other.as_secs())?;
        let nanos = if self.nanos >= other.subsec_nanos() {
            self.nanos - other.subsec_nanos()
        } else {
            secs = secs.checked_sub(1)?;
            self.nanos + 1_000_000_000 - other.subsec_nanos()
        };
        Some(SystemTime { secs, nanos })
    }
}

impl fmt::Debug for SystemTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SystemTime({}.{:09})", self.secs, self.nanos)
    }
}
