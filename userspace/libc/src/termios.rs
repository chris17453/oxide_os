//! Terminal I/O

use crate::syscall;

/// Terminal speed type
pub type Speed = u32;

/// Terminal control character index type
pub type TcFlag = u32;

/// Control characters
pub const NCCS: usize = 32;

/// Termios structure
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Termios {
    /// Input modes
    pub c_iflag: TcFlag,
    /// Output modes
    pub c_oflag: TcFlag,
    /// Control modes
    pub c_cflag: TcFlag,
    /// Local modes
    pub c_lflag: TcFlag,
    /// Line discipline
    pub c_line: u8,
    /// Control characters
    pub c_cc: [u8; NCCS],
    /// Input speed
    pub c_ispeed: Speed,
    /// Output speed
    pub c_ospeed: Speed,
}

impl Default for Termios {
    fn default() -> Self {
        Termios {
            c_iflag: 0,
            c_oflag: 0,
            c_cflag: 0,
            c_lflag: 0,
            c_line: 0,
            c_cc: [0; NCCS],
            c_ispeed: 0,
            c_ospeed: 0,
        }
    }
}

/// Input flags
pub mod iflag {
    use super::TcFlag;
    pub const IGNBRK: TcFlag = 0o000001;
    pub const BRKINT: TcFlag = 0o000002;
    pub const IGNPAR: TcFlag = 0o000004;
    pub const PARMRK: TcFlag = 0o000010;
    pub const INPCK: TcFlag = 0o000020;
    pub const ISTRIP: TcFlag = 0o000040;
    pub const INLCR: TcFlag = 0o000100;
    pub const IGNCR: TcFlag = 0o000200;
    pub const ICRNL: TcFlag = 0o000400;
    pub const IUCLC: TcFlag = 0o001000;
    pub const IXON: TcFlag = 0o002000;
    pub const IXANY: TcFlag = 0o004000;
    pub const IXOFF: TcFlag = 0o010000;
    pub const IMAXBEL: TcFlag = 0o020000;
    pub const IUTF8: TcFlag = 0o040000;
}

/// Output flags
pub mod oflag {
    use super::TcFlag;
    pub const OPOST: TcFlag = 0o000001;
    pub const OLCUC: TcFlag = 0o000002;
    pub const ONLCR: TcFlag = 0o000004;
    pub const OCRNL: TcFlag = 0o000010;
    pub const ONOCR: TcFlag = 0o000020;
    pub const ONLRET: TcFlag = 0o000040;
    pub const OFILL: TcFlag = 0o000100;
    pub const OFDEL: TcFlag = 0o000200;
}

/// Control flags
pub mod cflag {
    use super::TcFlag;
    pub const CSIZE: TcFlag = 0o000060;
    pub const CS5: TcFlag = 0o000000;
    pub const CS6: TcFlag = 0o000020;
    pub const CS7: TcFlag = 0o000040;
    pub const CS8: TcFlag = 0o000060;
    pub const CSTOPB: TcFlag = 0o000100;
    pub const CREAD: TcFlag = 0o000200;
    pub const PARENB: TcFlag = 0o000400;
    pub const PARODD: TcFlag = 0o001000;
    pub const HUPCL: TcFlag = 0o002000;
    pub const CLOCAL: TcFlag = 0o004000;
}

/// Local flags
pub mod lflag {
    use super::TcFlag;
    pub const ISIG: TcFlag = 0o000001;
    pub const ICANON: TcFlag = 0o000002;
    pub const XCASE: TcFlag = 0o000004;
    pub const ECHO: TcFlag = 0o000010;
    pub const ECHOE: TcFlag = 0o000020;
    pub const ECHOK: TcFlag = 0o000040;
    pub const ECHONL: TcFlag = 0o000100;
    pub const NOFLSH: TcFlag = 0o000200;
    pub const TOSTOP: TcFlag = 0o000400;
    pub const ECHOCTL: TcFlag = 0o001000;
    pub const ECHOPRT: TcFlag = 0o002000;
    pub const ECHOKE: TcFlag = 0o004000;
    pub const FLUSHO: TcFlag = 0o010000;
    pub const PENDIN: TcFlag = 0o040000;
    pub const IEXTEN: TcFlag = 0o100000;
    pub const EXTPROC: TcFlag = 0o200000;
}

/// Control characters indices
pub mod cc {
    pub const VINTR: usize = 0;
    pub const VQUIT: usize = 1;
    pub const VERASE: usize = 2;
    pub const VKILL: usize = 3;
    pub const VEOF: usize = 4;
    pub const VTIME: usize = 5;
    pub const VMIN: usize = 6;
    pub const VSWTC: usize = 7;
    pub const VSTART: usize = 8;
    pub const VSTOP: usize = 9;
    pub const VSUSP: usize = 10;
    pub const VEOL: usize = 11;
    pub const VREPRINT: usize = 12;
    pub const VDISCARD: usize = 13;
    pub const VWERASE: usize = 14;
    pub const VLNEXT: usize = 15;
    pub const VEOL2: usize = 16;
}

/// Baud rates
pub mod baud {
    use super::Speed;
    pub const B0: Speed = 0o000000;
    pub const B50: Speed = 0o000001;
    pub const B75: Speed = 0o000002;
    pub const B110: Speed = 0o000003;
    pub const B134: Speed = 0o000004;
    pub const B150: Speed = 0o000005;
    pub const B200: Speed = 0o000006;
    pub const B300: Speed = 0o000007;
    pub const B600: Speed = 0o000010;
    pub const B1200: Speed = 0o000011;
    pub const B1800: Speed = 0o000012;
    pub const B2400: Speed = 0o000013;
    pub const B4800: Speed = 0o000014;
    pub const B9600: Speed = 0o000015;
    pub const B19200: Speed = 0o000016;
    pub const B38400: Speed = 0o000017;
    pub const B57600: Speed = 0o010001;
    pub const B115200: Speed = 0o010002;
    pub const B230400: Speed = 0o010003;
    pub const B460800: Speed = 0o010004;
    pub const B500000: Speed = 0o010005;
    pub const B576000: Speed = 0o010006;
    pub const B921600: Speed = 0o010007;
    pub const B1000000: Speed = 0o010010;
    pub const B1152000: Speed = 0o010011;
    pub const B1500000: Speed = 0o010012;
    pub const B2000000: Speed = 0o010013;
    pub const B2500000: Speed = 0o010014;
    pub const B3000000: Speed = 0o010015;
    pub const B3500000: Speed = 0o010016;
    pub const B4000000: Speed = 0o010017;
}

/// tcsetattr optional actions
pub mod action {
    pub const TCSANOW: i32 = 0;
    pub const TCSADRAIN: i32 = 1;
    pub const TCSAFLUSH: i32 = 2;
}

/// tcflush queue selectors
pub mod queue {
    pub const TCIFLUSH: i32 = 0;
    pub const TCOFLUSH: i32 = 1;
    pub const TCIOFLUSH: i32 = 2;
}

/// tcflow actions
pub mod flow {
    pub const TCOOFF: i32 = 0;
    pub const TCOON: i32 = 1;
    pub const TCIOFF: i32 = 2;
    pub const TCION: i32 = 3;
}

/// IOCTL numbers
const TCGETS: u64 = 0x5401;
const TCSETS: u64 = 0x5402;
const TCSETSW: u64 = 0x5403;
const TCSETSF: u64 = 0x5404;
const TCFLSH: u64 = 0x540B;
const TCXONC: u64 = 0x540A;
const TCSBRK: u64 = 0x5409;
const TCSBRKP: u64 = 0x5425;
const TIOCGWINSZ: u64 = 0x5413;
const TIOCSWINSZ: u64 = 0x5414;

/// Get terminal attributes
pub fn tcgetattr(fd: i32, termios: &mut Termios) -> i32 {
    unsafe {
        syscall::syscall3(
            syscall::SYS_IOCTL,
            fd as usize,
            TCGETS as usize,
            termios as *mut Termios as usize,
        ) as i32
    }
}

/// Set terminal attributes
pub fn tcsetattr(fd: i32, optional_actions: i32, termios: &Termios) -> i32 {
    let cmd = match optional_actions {
        action::TCSANOW => TCSETS,
        action::TCSADRAIN => TCSETSW,
        action::TCSAFLUSH => TCSETSF,
        _ => return -1,
    };

    unsafe {
        syscall::syscall3(
            syscall::SYS_IOCTL,
            fd as usize,
            cmd as usize,
            termios as *const Termios as usize,
        ) as i32
    }
}

/// Flush terminal queues
pub fn tcflush(fd: i32, queue_selector: i32) -> i32 {
    unsafe {
        syscall::syscall3(
            syscall::SYS_IOCTL,
            fd as usize,
            TCFLSH as usize,
            queue_selector as usize,
        ) as i32
    }
}

/// Flow control
pub fn tcflow(fd: i32, action: i32) -> i32 {
    unsafe {
        syscall::syscall3(
            syscall::SYS_IOCTL,
            fd as usize,
            TCXONC as usize,
            action as usize,
        ) as i32
    }
}

/// Send break
pub fn tcsendbreak(fd: i32, duration: i32) -> i32 {
    unsafe {
        syscall::syscall3(
            syscall::SYS_IOCTL,
            fd as usize,
            if duration == 0 { TCSBRK } else { TCSBRKP } as usize,
            duration as usize,
        ) as i32
    }
}

/// Drain output
pub fn tcdrain(fd: i32) -> i32 {
    unsafe {
        syscall::syscall3(
            syscall::SYS_IOCTL,
            fd as usize,
            TCSBRK as usize,
            1,
        ) as i32
    }
}

/// Get input speed
pub fn cfgetispeed(termios: &Termios) -> Speed {
    termios.c_ispeed
}

/// Get output speed
pub fn cfgetospeed(termios: &Termios) -> Speed {
    termios.c_ospeed
}

/// Set input speed
pub fn cfsetispeed(termios: &mut Termios, speed: Speed) -> i32 {
    termios.c_ispeed = speed;
    0
}

/// Set output speed
pub fn cfsetospeed(termios: &mut Termios, speed: Speed) -> i32 {
    termios.c_ospeed = speed;
    0
}

/// Set both input and output speed
pub fn cfsetspeed(termios: &mut Termios, speed: Speed) -> i32 {
    termios.c_ispeed = speed;
    termios.c_ospeed = speed;
    0
}

/// Make raw mode
pub fn cfmakeraw(termios: &mut Termios) {
    termios.c_iflag &= !(iflag::IGNBRK | iflag::BRKINT | iflag::PARMRK | iflag::ISTRIP
        | iflag::INLCR | iflag::IGNCR | iflag::ICRNL | iflag::IXON);
    termios.c_oflag &= !oflag::OPOST;
    termios.c_lflag &= !(lflag::ECHO | lflag::ECHONL | lflag::ICANON | lflag::ISIG | lflag::IEXTEN);
    termios.c_cflag &= !(cflag::CSIZE | cflag::PARENB);
    termios.c_cflag |= cflag::CS8;
    termios.c_cc[cc::VMIN] = 1;
    termios.c_cc[cc::VTIME] = 0;
}

/// Window size structure
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct Winsize {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

/// Get window size
pub fn tcgetwinsize(fd: i32, ws: &mut Winsize) -> i32 {
    unsafe {
        syscall::syscall3(
            syscall::SYS_IOCTL,
            fd as usize,
            TIOCGWINSZ as usize,
            ws as *mut Winsize as usize,
        ) as i32
    }
}

/// Set window size
pub fn tcsetwinsize(fd: i32, ws: &Winsize) -> i32 {
    unsafe {
        syscall::syscall3(
            syscall::SYS_IOCTL,
            fd as usize,
            TIOCSWINSZ as usize,
            ws as *const Winsize as usize,
        ) as i32
    }
}

/// Check if fd is a terminal
pub fn isatty(fd: i32) -> bool {
    let mut termios = Termios::default();
    tcgetattr(fd, &mut termios) == 0
}

/// Get terminal name
pub fn ttyname(_fd: i32) -> *const u8 {
    // In a real implementation, this would look up the device name
    core::ptr::null()
}
