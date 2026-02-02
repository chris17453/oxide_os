# GW-BASIC Implementation Analysis for OXIDE OS

**Analysis Date:** 2025-01-25  
**Version:** v1  
**Status:** Comprehensive analysis of syscall integration and missing features

## Executive Summary

GW-BASIC has a well-structured interpreter with WATOS platform abstraction in `apps/gwbasic/src/platform/oxide_platform.rs`. The core interpreter, lexer, and parser are **fully implemented** and working. However, several features need kernel-level support or better syscall integration to be "100% legit working":

| Category | Status | Blocker |
|----------|--------|---------|
| **Core Interpreter** | ✅ Working | - |
| **Console I/O** | ✅ Working | - |
| **File I/O** | ✅ Working | syscalls connected via watos stubs |
| **Text Graphics** | ✅ Working | ANSI escape sequences |
| **Pixel Graphics** | ⚠️ Partial | Need framebuffer syscall integration |
| **Sound/Audio** | ❌ Not working | No syscall connection |
| **INKEY$/Non-blocking** | ❌ Not working | Need non-blocking stdin |

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         GW-BASIC                                │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────┐  ┌────────┐  ┌────────────┐  ┌──────────┐         │
│  │  Lexer  │→ │ Parser │→ │ Interpreter│→ │  Screen  │         │
│  └─────────┘  └────────┘  └────────────┘  └──────────┘         │
│       ↓                         ↓              ↓                │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                  Platform Abstraction                    │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌──────────────┐    │   │
│  │  │ OxideConsole│  │ OxideFileIO │  │OxideGraphics │    │   │
│  │  │   (libc)    │  │   (libc)    │  │   (ANSI)     │    │   │
│  │  └─────────────┘  └─────────────┘  └──────────────┘    │   │
│  └─────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────┤
│                       oxide_main.rs                             │
│        watos_* stubs redirect to libc syscalls                  │
├─────────────────────────────────────────────────────────────────┤
│                       OXIDE libc                                │
│                    (syscall wrappers)                           │
├─────────────────────────────────────────────────────────────────┤
│                     OXIDE Kernel                                │
│        VFS, TTY, devfs, framebuffer, audio                      │
└─────────────────────────────────────────────────────────────────┘
```

---

## 1. SYSCALL INTEGRATION STATUS

### 1.1 Working Syscalls (via oxide_main.rs watos_* stubs)

| WATOS Stub | libc Call | Kernel Syscall | Status |
|------------|-----------|----------------|--------|
| `watos_console_write` | `libc::write()` | SYS_WRITE (1) | ✅ Working |
| `watos_console_read` | `libc::read()` | SYS_READ (2) | ✅ Working |
| `watos_file_open` | `libc::open()` | SYS_OPEN (20) | ✅ Working |
| `watos_file_close` | `libc::close()` | SYS_CLOSE (21) | ✅ Working |
| `watos_file_read` | `libc::read()` | SYS_READ (2) | ✅ Working |
| `watos_file_write` | `libc::write()` | SYS_WRITE (1) | ✅ Working |
| `watos_file_tell` | `libc::lseek()` | SYS_LSEEK (22) | ✅ Working |
| `watos_file_size` | `libc::lseek()` | SYS_LSEEK (22) | ✅ Working |
| `watos_timer_syscall` | `libc::time::time()` | SYS_GETTIMEOFDAY (60) | ✅ Working |
| `watos_get_date` | `libc::time::gmtime_r()` | SYS_GETTIMEOFDAY (60) | ✅ Working |
| `watos_get_time` | `libc::time::time()` | SYS_GETTIMEOFDAY (60) | ✅ Working |
| `watos_get_free_memory` | hardcoded 512KB | N/A | ⚠️ Placeholder |

### 1.2 Non-Working / Stub Syscalls

| WATOS Stub | Implementation | Issue |
|------------|----------------|-------|
| `watos_get_key_no_wait` | Returns 0 | ❌ No non-blocking stdin |
| `watos_get_cursor_row` | Returns 0 | ❌ No cursor query syscall |
| `watos_get_cursor_col` | Returns 0 | ❌ No cursor query syscall |
| `watos_get_pixel` | Returns 0 | ❌ No framebuffer read syscall |

---

## 2. CRITICAL ISSUES

### Issue 2.1: Non-Blocking Keyboard (INKEY$) 
**Severity: CRITICAL** | **Location:** `oxide_main.rs:121-127`

**Problem:**
```rust
#[no_mangle]
pub extern "C" fn watos_get_key_no_wait() -> u8 {
    // Non-blocking read - return 0 if no key available
    // OXIDE doesn't have non-blocking stdin yet, return 0
    0
}
```

The `INKEY$` function always returns empty string - no keyboard scanning possible.

**Required Kernel Changes:**
1. Add `O_NONBLOCK` flag support to TTY read (already in kernel)
2. Need to call `fcntl(STDIN, F_SETFL, O_NONBLOCK)` before read

**Fix in oxide_main.rs:**
```rust
#[no_mangle]
pub extern "C" fn watos_get_key_no_wait() -> u8 {
    // Set stdin to non-blocking
    let flags = libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL, 0);
    libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, flags | libc::O_NONBLOCK);
    
    let mut buf = [0u8; 1];
    let n = libc::read(libc::STDIN_FILENO, &mut buf);
    
    // Restore blocking mode
    libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, flags);
    
    if n > 0 { buf[0] } else { 0 }
}
```

**Blocker:** libc `fcntl()` is a stub returning 0! (See vim_analysis.md)

---

### Issue 2.2: Graphics Mode - No Framebuffer Access
**Severity: CRITICAL** | **Location:** `graphics_backend/watos_vga.rs`

**Problem:**
The VGA backend defines syscalls 30-41 for graphics operations:
```rust
mod syscall {
    pub const SYS_VGA_SET_MODE: u32 = 30;
    pub const SYS_VGA_SET_PIXEL: u32 = 31;
    pub const SYS_VGA_GET_PIXEL: u32 = 32;
    pub const SYS_VGA_BLIT: u32 = 33;
    pub const SYS_VGA_CLEAR: u32 = 34;
    pub const SYS_VGA_FLIP: u32 = 35;
    // ...
}
```

**BUT these syscalls don't exist in OXIDE kernel!**

OXIDE's syscall table (`userspace/libc/src/syscall.rs`) shows:
- 30-39: File operations (MKDIR, RMDIR, UNLINK, etc.)
- No VGA syscalls defined

**Kernel HAS framebuffer support via:**
- `/dev/fb0` - Framebuffer device
- `FBIOGET_VSCREENINFO` ioctl
- `FBIOGET_FSCREENINFO` ioctl
- Direct memory-mapped writes

**Solution:** Rewrite `WatosVgaBackend` to use `/dev/fb0` instead of fake syscalls:

```rust
// In oxide_platform.rs or new oxide_vga.rs
pub struct OxideVgaBackend {
    fb_fd: i32,
    fb_ptr: *mut u8,
    fb_size: usize,
    width: usize,
    height: usize,
    stride: usize,
    bpp: usize,
}

impl OxideVgaBackend {
    pub fn new(mode: VideoMode) -> Result<Self> {
        // 1. Open framebuffer device
        let fd = libc::open("/dev/fb0", libc::O_RDWR, 0);
        if fd < 0 {
            return Err(Error::RuntimeError("Cannot open /dev/fb0".into()));
        }
        
        // 2. Get framebuffer info via ioctl
        let mut var_info = FbVarScreenInfo::default();
        libc::ioctl(fd, FBIOGET_VSCREENINFO, &mut var_info as *mut _ as u64);
        
        let mut fix_info = FbFixScreenInfo::default();
        libc::ioctl(fd, FBIOGET_FSCREENINFO, &mut fix_info as *mut _ as u64);
        
        // 3. mmap the framebuffer
        let fb_size = fix_info.smem_len as usize;
        let fb_ptr = libc::mmap(
            core::ptr::null_mut(),
            fb_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        );
        
        Ok(OxideVgaBackend {
            fb_fd: fd,
            fb_ptr: fb_ptr as *mut u8,
            fb_size,
            width: var_info.xres as usize,
            height: var_info.yres as usize,
            stride: fix_info.line_length as usize,
            bpp: (var_info.bits_per_pixel / 8) as usize,
        })
    }
    
    fn pset(&mut self, x: i32, y: i32, color: u8) {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            let offset = (y as usize) * self.stride + (x as usize) * self.bpp;
            let rgb = PALETTE_16[color.min(15) as usize];
            unsafe {
                // Write BGRA pixel
                *self.fb_ptr.add(offset) = (rgb & 0xFF) as u8;         // B
                *self.fb_ptr.add(offset + 1) = ((rgb >> 8) & 0xFF) as u8;  // G
                *self.fb_ptr.add(offset + 2) = ((rgb >> 16) & 0xFF) as u8; // R
                if self.bpp == 4 {
                    *self.fb_ptr.add(offset + 3) = 0xFF;               // A
                }
            }
        }
    }
}
```

**Blockers:**
1. Need `mmap()` syscall working for userspace (it exists in kernel)
2. Need libc `mmap()` wrapper (check if exists)

---

### Issue 2.3: Sound/Audio Not Connected
**Severity: HIGH** | **Location:** `interpreter.rs:489-499`

**Current Implementation:**
```rust
AstNode::Beep => {
    console_println!("\x07"); // ASCII bell character
    Ok(())
}
AstNode::Sound(freq, duration) => {
    let _f = self.evaluate_expression(&freq)?;
    let _d = self.evaluate_expression(&duration)?;
    // Simulated - would play sound
    console_println!("\x07");
    Ok(())
}
```

**Problem:** 
- BEEP sends BEL character (works if terminal supports it)
- SOUND does nothing with actual sound hardware

**Kernel Has:**
- `crates/audio/audio/` - Full audio subsystem with mixer
- PC speaker access at port 0x61 (see `crates/arch/arch-x86_64/src/apic.rs:245`)

**Solution Options:**

**Option A: PC Speaker Beep (Simple)**
Add a syscall for PC speaker beep:
```rust
// In kernel: implement SYS_BEEP syscall
pub fn sys_beep(freq_hz: u64, duration_ms: u64) -> i64 {
    use crate::arch::{outb, inb};
    
    // Calculate PIT divisor for frequency
    let divisor = 1193180 / freq_hz as u32;
    
    // Set up PIT channel 2
    outb(0x43, 0xB6);
    outb(0x42, (divisor & 0xFF) as u8);
    outb(0x42, ((divisor >> 8) & 0xFF) as u8);
    
    // Enable speaker
    let tmp = inb(0x61);
    outb(0x61, tmp | 3);
    
    // Sleep for duration
    // ... use scheduler sleep ...
    
    // Disable speaker
    outb(0x61, tmp);
    
    0
}
```

**Option B: Use Audio Subsystem (Complex)**
- Open `/dev/audio` or `/dev/dsp`
- Generate PCM samples for tone
- Write to audio device

---

## 3. HIGH PRIORITY ISSUES

### Issue 3.1: Cursor Position Query (CSRLIN, POS)
**Severity: HIGH** | **Location:** `oxide_main.rs:159-169`

**Problem:**
```rust
pub extern "C" fn watos_get_cursor_row() -> u8 { 0 }
pub extern "C" fn watos_get_cursor_col() -> u8 { 0 }
```

**Solution:** Use ANSI DSR (Device Status Report):
```rust
pub fn get_cursor_position() -> (u8, u8) {
    // Send DSR request: ESC [ 6 n
    libc::write(libc::STDOUT_FILENO, b"\x1b[6n");
    
    // Read response: ESC [ row ; col R
    let mut buf = [0u8; 16];
    let mut len = 0;
    
    // Need non-blocking read with timeout...
    loop {
        let n = libc::read(libc::STDIN_FILENO, &mut buf[len..len+1]);
        if n <= 0 { break; }
        len += 1;
        if buf[len-1] == b'R' { break; }
    }
    
    // Parse "ESC[row;colR"
    parse_cursor_response(&buf[..len])
}
```

**Blocker:** Requires non-blocking read (Issue 2.1)

---

### Issue 3.2: Memory Query (FRE Function)
**Severity: MEDIUM** | **Location:** `oxide_main.rs:115-119`

**Current:** Hardcoded 512KB
```rust
pub extern "C" fn watos_get_free_memory() -> usize {
    512 * 1024 // 512KB available
}
```

**Solution:** Use kernel memory info:
- Read `/proc/meminfo` if procfs exists
- Or add `SYS_GETMEMINFO` syscall

---

### Issue 3.3: Many Statements Print "not yet fully implemented"
**Severity: MEDIUM** | **Location:** `interpreter.rs:869-1057`

These statements just print a message and do nothing:

| Statement | Line | Category |
|-----------|------|----------|
| AUTO | 869 | Editor |
| DELETE | 873 | Editor |
| RENUM | 877 | Editor |
| EDIT | 881 | Editor |
| VIEW | 895 | Graphics |
| WINDOW | 899 | Graphics |
| PAINT | 914 | Graphics |
| DRAW | 918 | Graphics |
| GET (graphics) | 922 | Graphics |
| PUT (graphics) | 926 | Graphics |
| PALETTE | 930 | Graphics |
| PLAY | 936 | Sound |
| KILL | 946 | File I/O |
| NAME | 950 | File I/O |
| FILES | 954 | File I/O |
| FIELD | 958 | Random File |
| LSET/RSET | 962/966 | Random File |
| GET/PUT (file) | 970/974 | Random File |
| PRINT USING | 978 | Formatting |
| DEFSTR/INT/SNG/DBL | 999-1011 | Types |
| OPTION BASE | 1015 | Arrays |
| KEY | 1021 | Function Keys |
| ON KEY | 1037 | Traps |
| DEF SEG | 1041 | Memory |
| BLOAD/BSAVE | 1045/1049 | Binary I/O |
| CALL/USR | 1053/1057 | Machine Code |

---

## 4. MEDIUM PRIORITY ISSUES

### Issue 4.1: KILL (File Delete)
**Status:** Prints message, does nothing  
**Fix:** Use `libc::unlink(path)`

```rust
AstNode::Kill(filename) => {
    let filename = self.evaluate_expression(&filename)?.as_string();
    let result = libc::unlink(&filename);
    if result < 0 {
        return Err(Error::IoError(format!("Cannot delete file: {}", filename)));
    }
    Ok(())
}
```

### Issue 4.2: NAME (File Rename)
**Status:** Prints message, does nothing  
**Fix:** Use `libc::rename(old, new)`

### Issue 4.3: FILES (Directory Listing)
**Status:** Prints message, does nothing  
**Fix:** Use `libc::opendir()` / `libc::readdir()`

---

## 5. LOW PRIORITY ISSUES

### Issue 5.1: Editor Commands (AUTO, DELETE, RENUM, EDIT)
These require an interactive line editor environment. Low priority for running programs.

### Issue 5.2: Function Key Traps (KEY, ON KEY)
Requires keyboard interrupt handling integration.

### Issue 5.3: DEF SEG, BLOAD, BSAVE, CALL, USR
Machine code and direct memory access - security concerns, low priority.

---

## 6. IMPLEMENTATION PRIORITY

### Phase 1: Core Functionality (Required for basic programs)
1. **Fix libc fcntl()** - Enable non-blocking I/O (1-line fix)
2. **Fix watos_get_key_no_wait()** - INKEY$ for game loops
3. **Implement KILL** - File delete via unlink()
4. **Implement NAME** - File rename

### Phase 2: Graphics (Required for graphical programs)
1. **Create OxideVgaBackend** using `/dev/fb0`
2. **Add libc mmap()** if missing
3. **Test SCREEN, PSET, LINE, CIRCLE**
4. **Implement PAINT** (flood fill algorithm)
5. **Implement GET/PUT** (screen capture/blit)

### Phase 3: Sound (Required for games/music)
1. **Add SYS_BEEP syscall** for PC speaker
2. **Fix SOUND statement** to use syscall
3. **Implement PLAY** (music string parser)

### Phase 4: Random Access Files
1. **Implement FIELD** - buffer field definition
2. **Implement LSET/RSET** - string justification
3. **Implement GET#/PUT#** - record read/write

---

## 7. VERIFICATION TESTS

### Test 1: Console I/O (Should work now)
```basic
10 PRINT "Hello, OXIDE!"
20 INPUT "Enter your name: "; N$
30 PRINT "Hello, "; N$
RUN
```

### Test 2: File I/O (Should work now)
```basic
10 OPEN "test.txt" FOR OUTPUT AS #1
20 PRINT #1, "This is a test"
30 CLOSE #1
40 OPEN "test.txt" FOR INPUT AS #1
50 LINE INPUT #1, A$
60 PRINT A$
70 CLOSE #1
RUN
```

### Test 3: INKEY$ (Will fail without fix)
```basic
10 CLS
20 K$ = INKEY$
30 IF K$ = "" THEN 20
40 PRINT "You pressed: "; K$
50 IF K$ <> CHR$(27) THEN 20
RUN
```

### Test 4: Graphics (Will fail without framebuffer)
```basic
10 SCREEN 1
20 FOR I = 0 TO 319
30 PSET (I, 100), 15
40 NEXT I
50 A$ = INPUT$(1)
RUN
```

---

## 8. QUICK WINS SUMMARY

| Fix | Effort | Impact | Priority |
|-----|--------|--------|----------|
| Fix libc fcntl() | 1 line | Enables non-blocking I/O | P1 |
| Fix INKEY$ | 10 lines | Game loops work | P1 |
| Implement KILL | 5 lines | File delete | P2 |
| Implement NAME | 5 lines | File rename | P2 |
| Implement FILES | 20 lines | Directory listing | P2 |
| Add SYS_BEEP | 30 lines | Sound works | P3 |
| Create OxideVgaBackend | 100 lines | Graphics work | P3 |

---

## Appendix A: Syscall Number Reference

Current OXIDE syscall numbers used by GW-BASIC:
```
SYS_EXIT = 0
SYS_WRITE = 1
SYS_READ = 2
SYS_OPEN = 20
SYS_CLOSE = 21
SYS_LSEEK = 22
SYS_IOCTL = 40
SYS_GETTIMEOFDAY = 60
SYS_NANOSLEEP = 63
```

Needed but missing/problematic:
```
SYS_FCNTL = ?? (libc stub broken)
SYS_UNLINK = 32 (for KILL)
SYS_RENAME = 33 (for NAME)
SYS_GETDENTS = 34 (for FILES)
SYS_MMAP = 90 (for framebuffer)
SYS_BEEP = ?? (new, for SOUND)
```

---

## Appendix B: File Locations

| File | Purpose |
|------|---------|
| `apps/gwbasic/src/oxide_main.rs` | OXIDE entry point, watos_* stubs |
| `apps/gwbasic/src/platform/oxide_platform.rs` | Console, FileSystem, Graphics traits |
| `apps/gwbasic/src/graphics_backend/watos_vga.rs` | VGA backend (broken syscalls) |
| `apps/gwbasic/src/interpreter.rs` | Main interpreter with all statements |
| `apps/gwbasic/src/fileio.rs` | File I/O manager |
| `userspace/libc/src/c_exports.rs` | libc fcntl() stub at line 2085 |
| `crates/vfs/devfs/src/devices.rs` | /dev/fb0 framebuffer device |
