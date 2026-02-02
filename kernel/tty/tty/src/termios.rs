//! Termios structure and flags
//!
//! Terminal I/O settings compatible with POSIX termios.

use bitflags::bitflags;

/// Number of control characters
pub const NCCS: usize = 32;

/// Terminal I/O settings
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Termios {
    /// Input mode flags
    pub c_iflag: InputFlags,
    /// Output mode flags
    pub c_oflag: OutputFlags,
    /// Control mode flags
    pub c_cflag: ControlFlags,
    /// Local mode flags
    pub c_lflag: LocalFlags,
    /// Line discipline
    pub c_line: u8,
    /// Control characters
    pub c_cc: [u8; NCCS],
    /// Input baud rate
    pub c_ispeed: u32,
    /// Output baud rate
    pub c_ospeed: u32,
}

impl Default for Termios {
    fn default() -> Self {
        let mut termios = Termios {
            c_iflag: InputFlags::ICRNL | InputFlags::IXON,
            c_oflag: OutputFlags::OPOST | OutputFlags::ONLCR,
            c_cflag: ControlFlags::CS8 | ControlFlags::CREAD | ControlFlags::HUPCL,
            c_lflag: LocalFlags::ISIG
                | LocalFlags::ICANON
                | LocalFlags::ECHO
                | LocalFlags::ECHOE
                | LocalFlags::ECHOK
                | LocalFlags::ECHOCTL
                | LocalFlags::ECHOKE
                | LocalFlags::IEXTEN,
            c_line: 0,
            c_cc: [0; NCCS],
            c_ispeed: B38400,
            c_ospeed: B38400,
        };

        // Set default control characters
        termios.c_cc[VINTR] = 0x03; // ^C
        termios.c_cc[VQUIT] = 0x1C; // ^\
        termios.c_cc[VERASE] = 0x7F; // DEL
        termios.c_cc[VKILL] = 0x15; // ^U
        termios.c_cc[VEOF] = 0x04; // ^D
        termios.c_cc[VTIME] = 0;
        termios.c_cc[VMIN] = 1;
        termios.c_cc[VSTART] = 0x11; // ^Q
        termios.c_cc[VSTOP] = 0x13; // ^S
        termios.c_cc[VSUSP] = 0x1A; // ^Z
        termios.c_cc[VEOL] = 0;
        termios.c_cc[VREPRINT] = 0x12; // ^R
        termios.c_cc[VDISCARD] = 0x0F; // ^O
        termios.c_cc[VWERASE] = 0x17; // ^W
        termios.c_cc[VLNEXT] = 0x16; // ^V
        termios.c_cc[VEOL2] = 0;

        termios
    }
}

impl Termios {
    /// Check if canonical mode is enabled
    pub fn is_canonical(&self) -> bool {
        self.c_lflag.contains(LocalFlags::ICANON)
    }

    /// Check if echo is enabled
    pub fn is_echo(&self) -> bool {
        self.c_lflag.contains(LocalFlags::ECHO)
    }

    /// Check if signal generation is enabled
    pub fn is_isig(&self) -> bool {
        self.c_lflag.contains(LocalFlags::ISIG)
    }

    /// Set raw mode (disable canonical, echo, signals)
    pub fn set_raw(&mut self) {
        self.c_iflag = InputFlags::empty();
        self.c_oflag = OutputFlags::empty();
        self.c_lflag = LocalFlags::empty();
        self.c_cc[VMIN] = 1;
        self.c_cc[VTIME] = 0;
    }

    /// Set cooked mode (canonical with echo and signals)
    pub fn set_cooked(&mut self) {
        *self = Self::default();
    }
}

// Control character indices
pub const VINTR: usize = 0; // Interrupt (^C)
pub const VQUIT: usize = 1; // Quit (^\)
pub const VERASE: usize = 2; // Erase character (DEL/^H)
pub const VKILL: usize = 3; // Kill line (^U)
pub const VEOF: usize = 4; // End of file (^D)
pub const VTIME: usize = 5; // Timeout for non-canonical read
pub const VMIN: usize = 6; // Minimum characters for non-canonical read
pub const VSWTC: usize = 7; // Switch character
pub const VSTART: usize = 8; // Start output (^Q)
pub const VSTOP: usize = 9; // Stop output (^S)
pub const VSUSP: usize = 10; // Suspend (^Z)
pub const VEOL: usize = 11; // End of line
pub const VREPRINT: usize = 12; // Reprint line (^R)
pub const VDISCARD: usize = 13; // Discard pending output (^O)
pub const VWERASE: usize = 14; // Word erase (^W)
pub const VLNEXT: usize = 15; // Literal next (^V)
pub const VEOL2: usize = 16; // Second end of line

bitflags! {
    /// Input mode flags (c_iflag)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct InputFlags: u32 {
        /// Ignore BREAK condition
        const IGNBRK = 0o0000001;
        /// Signal interrupt on BREAK
        const BRKINT = 0o0000002;
        /// Ignore characters with parity errors
        const IGNPAR = 0o0000004;
        /// Mark parity and framing errors
        const PARMRK = 0o0000010;
        /// Enable input parity check
        const INPCK = 0o0000020;
        /// Strip 8th bit
        const ISTRIP = 0o0000040;
        /// Map NL to CR on input
        const INLCR = 0o0000100;
        /// Ignore CR
        const IGNCR = 0o0000200;
        /// Map CR to NL on input
        const ICRNL = 0o0000400;
        /// Map uppercase to lowercase
        const IUCLC = 0o0001000;
        /// Enable start/stop output control
        const IXON = 0o0002000;
        /// Any character restarts output
        const IXANY = 0o0004000;
        /// Enable start/stop input control
        const IXOFF = 0o0010000;
        /// Ring bell when queue full
        const IMAXBEL = 0o0020000;
        /// Input is UTF-8
        const IUTF8 = 0o0040000;
    }
}

bitflags! {
    /// Output mode flags (c_oflag)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct OutputFlags: u32 {
        /// Perform output processing
        const OPOST = 0o0000001;
        /// Map lowercase to uppercase
        const OLCUC = 0o0000002;
        /// Map NL to CR-NL on output
        const ONLCR = 0o0000004;
        /// Map CR to NL on output
        const OCRNL = 0o0000010;
        /// Don't output CR at column 0
        const ONOCR = 0o0000020;
        /// Don't output CR
        const ONLRET = 0o0000040;
        /// Send fill characters for delay
        const OFILL = 0o0000100;
        /// Fill character is DEL
        const OFDEL = 0o0000200;
    }
}

bitflags! {
    /// Control mode flags (c_cflag)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ControlFlags: u32 {
        /// 5 bits per character
        const CS5 = 0o0000000;
        /// 6 bits per character
        const CS6 = 0o0000020;
        /// 7 bits per character
        const CS7 = 0o0000040;
        /// 8 bits per character
        const CS8 = 0o0000060;
        /// Character size mask
        const CSIZE = 0o0000060;
        /// 2 stop bits
        const CSTOPB = 0o0000100;
        /// Enable receiver
        const CREAD = 0o0000200;
        /// Enable parity
        const PARENB = 0o0000400;
        /// Odd parity
        const PARODD = 0o0001000;
        /// Hang up on last close
        const HUPCL = 0o0002000;
        /// Ignore modem control lines
        const CLOCAL = 0o0004000;
    }
}

bitflags! {
    /// Local mode flags (c_lflag)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LocalFlags: u32 {
        /// Enable signals (INTR, QUIT, SUSP)
        const ISIG = 0o0000001;
        /// Canonical mode (line editing)
        const ICANON = 0o0000002;
        /// Map uppercase to lowercase on input (obsolete)
        const XCASE = 0o0000004;
        /// Echo input characters
        const ECHO = 0o0000010;
        /// Echo erase as backspace-space-backspace
        const ECHOE = 0o0000020;
        /// Echo NL after KILL
        const ECHOK = 0o0000040;
        /// Echo NL even if ECHO is off
        const ECHONL = 0o0000100;
        /// Disable flush after interrupt
        const NOFLSH = 0o0000200;
        /// Send SIGTTOU for background output
        const TOSTOP = 0o0000400;
        /// Echo control characters as ^X
        const ECHOCTL = 0o0001000;
        /// Visual erase for line kill
        const ECHOPRT = 0o0002000;
        /// Echo kill by erasing line
        const ECHOKE = 0o0004000;
        /// Output being flushed
        const FLUSHO = 0o0010000;
        /// Retype pending input (not implemented)
        const PENDIN = 0o0040000;
        /// Enable implementation-defined input processing
        const IEXTEN = 0o0100000;
        /// Extended processing (same as IEXTEN on many systems)
        const EXTPROC = 0o0200000;
    }
}

// Baud rates
pub const B0: u32 = 0;
pub const B50: u32 = 1;
pub const B75: u32 = 2;
pub const B110: u32 = 3;
pub const B134: u32 = 4;
pub const B150: u32 = 5;
pub const B200: u32 = 6;
pub const B300: u32 = 7;
pub const B600: u32 = 8;
pub const B1200: u32 = 9;
pub const B1800: u32 = 10;
pub const B2400: u32 = 11;
pub const B4800: u32 = 12;
pub const B9600: u32 = 13;
pub const B19200: u32 = 14;
pub const B38400: u32 = 15;
pub const B57600: u32 = 0o010001;
pub const B115200: u32 = 0o010002;
pub const B230400: u32 = 0o010003;

// ioctl request codes for termios
pub const TCGETS: u64 = 0x5401; // Get termios
pub const TCSETS: u64 = 0x5402; // Set termios immediately
pub const TCSETSW: u64 = 0x5403; // Set termios after drain
pub const TCSETSF: u64 = 0x5404; // Set termios after flush
pub const TCGETA: u64 = 0x5405; // Get termio (old)
pub const TCSETA: u64 = 0x5406; // Set termio (old)
pub const TCSETAW: u64 = 0x5407; // Set termio after drain (old)
pub const TCSETAF: u64 = 0x5408; // Set termio after flush (old)
pub const TCSBRK: u64 = 0x5409; // Send break
pub const TCXONC: u64 = 0x540A; // Flow control
pub const TCFLSH: u64 = 0x540B; // Flush queues
pub const TIOCEXCL: u64 = 0x540C; // Set exclusive mode
pub const TIOCNXCL: u64 = 0x540D; // Clear exclusive mode
pub const TIOCSCTTY: u64 = 0x540E; // Set controlling terminal
pub const TIOCGPGRP: u64 = 0x540F; // Get foreground process group
pub const TIOCSPGRP: u64 = 0x5410; // Set foreground process group
pub const TIOCOUTQ: u64 = 0x5411; // Output queue size
pub const TIOCSTI: u64 = 0x5412; // Simulate terminal input
pub const TIOCGWINSZ: u64 = 0x5413; // Get window size
pub const TIOCSWINSZ: u64 = 0x5414; // Set window size
pub const TIOCMGET: u64 = 0x5415; // Get modem status
pub const TIOCMBIS: u64 = 0x5416; // Set modem bits
pub const TIOCMBIC: u64 = 0x5417; // Clear modem bits
pub const TIOCMSET: u64 = 0x5418; // Set modem status
pub const TIOCGSOFTCAR: u64 = 0x5419; // Get software carrier
pub const TIOCSSOFTCAR: u64 = 0x541A; // Set software carrier
pub const FIONREAD: u64 = 0x541B; // Bytes available for read
pub const TIOCLINUX: u64 = 0x541C; // Linux-specific
pub const TIOCCONS: u64 = 0x541D; // Console redirect
pub const TIOCGSERIAL: u64 = 0x541E; // Get serial info
pub const TIOCSSERIAL: u64 = 0x541F; // Set serial info
pub const TIOCPKT: u64 = 0x5420; // Packet mode
pub const FIONBIO: u64 = 0x5421; // Non-blocking I/O
pub const TIOCNOTTY: u64 = 0x5422; // Release controlling terminal
pub const TIOCSETD: u64 = 0x5423; // Set line discipline
pub const TIOCGETD: u64 = 0x5424; // Get line discipline
pub const TCSBRKP: u64 = 0x5425; // Send break (timed)
pub const TIOCSBRK: u64 = 0x5427; // Set break
pub const TIOCCBRK: u64 = 0x5428; // Clear break
pub const TIOCGSID: u64 = 0x5429; // Get session ID
pub const TIOCGPTN: u64 = 0x80045430; // Get PTY number (ioctl arg is *mut u32)
pub const TIOCSPTLCK: u64 = 0x40045431; // Lock/unlock PTY slave
