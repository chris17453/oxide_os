//! — NeonVale: Terminal detection via ioctl TIOCGWINSZ.
use crate::os::fd::{AsFd, AsRawFd};

pub fn is_terminal(fd: &impl AsFd) -> bool {
    let fd = fd.as_fd();
    let mut ws = oxide_rt::types::Winsize { ws_row: 0, ws_col: 0, ws_xpixel: 0, ws_ypixel: 0 };
    oxide_rt::io::ioctl(fd.as_raw_fd(), oxide_rt::types::ioctl_nr::TIOCGWINSZ, &mut ws as *mut _ as usize) == 0
}
