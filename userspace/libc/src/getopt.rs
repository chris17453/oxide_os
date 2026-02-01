// POSIX getopt() implementation for OXIDE OS
//
// This implements standard POSIX getopt() for command-line parsing.
// Global state is maintained for C compatibility.

use core::ptr;

// Global state for getopt (required by POSIX specification)
static mut OPTIND: i32 = 1;      // Index of next argv element to process
static mut OPTARG: *mut u8 = ptr::null_mut();  // Pointer to option argument
static mut OPTOPT: i32 = 0;      // Last option character
static mut OPTERR: i32 = 1;      // Print error messages if nonzero
static mut OPTP: *const u8 = ptr::null();  // Internal: current position in option

/// POSIX getopt() implementation
///
/// Parses command-line arguments according to the option string.
/// Returns the next option character, or -1 when done.
///
/// optstring format:
///   - Single character: option takes no argument
///   - Character followed by ':': option requires an argument
///   - Character followed by '::': option has optional argument (GNU extension)
///
/// # Safety
/// This function is unsafe because it:
/// - Accesses mutable global state
/// - Dereferences raw pointers (argv, optstring)
/// - Assumes argv is a valid null-terminated array of null-terminated strings
pub unsafe fn getopt_impl(argc: i32, argv: *const *const u8, optstring: *const u8) -> i32 {
    // Validate inputs
    if argv.is_null() || optstring.is_null() {
        return -1;
    }

    if argc <= 0 {
        return -1;
    }

    // Reset internal state if OPTIND was reset to 0 or 1
    if OPTIND == 0 {
        OPTIND = 1;
        OPTP = ptr::null();
    }

    // Clear OPTARG
    OPTARG = ptr::null_mut();

    // Check if we're done or processing non-options
    if OPTIND >= argc {
        return -1;
    }

    let current_arg = *argv.add(OPTIND as usize);
    if current_arg.is_null() {
        return -1;
    }

    // Start of new argument or continuing multi-option argument
    if OPTP.is_null() || *OPTP == 0 {
        OPTP = current_arg;

        // Check for end of options or non-option argument
        if *OPTP != b'-' {
            return -1;
        }

        OPTP = OPTP.add(1);

        // Check for "--" (end of options marker)
        if *OPTP == b'-' {
            OPTP = OPTP.add(1);
            if *OPTP == 0 {
                OPTIND += 1;
                return -1;
            }
            // Single "--" by itself ends option processing
            OPTP = ptr::null();
            return -1;
        }

        // Check for bare "-"
        if *OPTP == 0 {
            return -1;
        }
    }

    // Get current option character
    let opt = *OPTP as i32;
    OPTOPT = opt;
    OPTP = OPTP.add(1);

    // Find option in optstring
    let mut optstr_ptr = optstring;

    // Skip leading ':' or '+' or '-' in optstring (special markers)
    if *optstr_ptr == b':' || *optstr_ptr == b'+' || *optstr_ptr == b'-' {
        optstr_ptr = optstr_ptr.add(1);
    }

    let mut found = false;
    let mut requires_arg = false;
    let mut optional_arg = false;

    while *optstr_ptr != 0 {
        if *optstr_ptr as i32 == opt {
            found = true;
            // Check if option requires an argument
            let next = *optstr_ptr.add(1);
            if next == b':' {
                requires_arg = true;
                // Check for optional argument (::)
                if *optstr_ptr.add(2) == b':' {
                    optional_arg = true;
                    requires_arg = false;
                }
            }
            break;
        }
        optstr_ptr = optstr_ptr.add(1);
    }

    if !found {
        // Unknown option
        if OPTERR != 0 && *optstring != b':' {
            write_error(b"illegal option -- ", opt as u8);
        }

        // Move to next arg if end of current
        if *OPTP == 0 {
            OPTIND += 1;
            OPTP = ptr::null();
        }
        return b'?' as i32;
    }

    // Handle option argument
    if requires_arg {
        // Argument is rest of current argv element
        if *OPTP != 0 {
            OPTARG = OPTP as *mut u8;
            OPTIND += 1;
            OPTP = ptr::null();
        } else {
            // Argument is next argv element
            OPTIND += 1;
            if OPTIND < argc {
                OPTARG = *argv.add(OPTIND as usize) as *mut u8;
                OPTIND += 1;
                OPTP = ptr::null();
            } else {
                // Missing required argument
                if OPTERR != 0 && *optstring != b':' {
                    write_error(b"option requires an argument -- ", opt as u8);
                }
                OPTP = ptr::null();
                return if *optstring == b':' { b':' as i32 } else { b'?' as i32 };
            }
        }
    } else if optional_arg {
        // Optional argument must be in same argv element
        if *OPTP != 0 {
            OPTARG = OPTP as *mut u8;
        }
        OPTIND += 1;
        OPTP = ptr::null();
    } else {
        // No argument - move to next if end of current
        if *OPTP == 0 {
            OPTIND += 1;
            OPTP = ptr::null();
        }
    }

    opt
}

// Helper to write error messages (minimal implementation)
unsafe fn write_error(msg: &[u8], opt_char: u8) {
    use crate::syscall;

    // Write to stderr (fd 2)
    syscall::syscall3(syscall::nr::WRITE, 2, msg.as_ptr() as usize, msg.len());

    // Write the option character
    syscall::syscall3(syscall::nr::WRITE, 2, &opt_char as *const u8 as usize, 1);

    // Write newline
    let newline = b"\n";
    syscall::syscall3(syscall::nr::WRITE, 2, newline.as_ptr() as usize, 1);
}
