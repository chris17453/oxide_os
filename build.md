warning: variable `followed_pointer` is assigned to, but never used
   --> userspace/libc/src/dns.rs:725:9
    |
725 |     let mut followed_pointer = false;
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: consider using `_followed_pointer` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `followed_pointer` is never read
   --> userspace/libc/src/dns.rs:747:13
    |
747 |             followed_pointer = true;
    |             ^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/libc/src/locale.rs:138:5
    |
138 |     unsafe { &raw mut C_LCONV }
    |     ^^^^^^ unnecessary `unsafe` block
    |
    = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:57:5
   |
57 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:74:5
   |
74 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:173:5
    |
173 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:208:5
    |
208 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:539:5
    |
539 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:612:5
    |
612 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:214:5
    |
214 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:233:5
    |
233 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:245:5
    |
245 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:257:5
    |
257 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:269:5
    |
269 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:281:5
    |
281 |     unsafe { syscall::syscall3(syscall::SYS_IOCTL, fd as usize, TCSBRK as usize, 1) as i32 }
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:343:5
    |
343 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:355:5
    |
355 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:145:5
    |
145 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:156:5
    |
156 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:170:5
    |
170 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: constant `MAX_LINE_LEN` is never used
 --> userspace/libc/src/pwd.rs:9:7
  |
9 | const MAX_LINE_LEN: usize = 512;
  |       ^^^^^^^^^^^^
  |
  = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `libc` (lib) generated 21 warnings
   Compiling init v0.1.0 (/home/nd/repos/Projects/oxide_os/userspace/init)
    Finished `release` profile [optimized] target(s) in 0.36s
  Building esh (release)...
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/nd/repos/Projects/oxide_os/apps/gwbasic/Cargo.toml
workspace: /home/nd/repos/Projects/oxide_os/Cargo.toml
warning: variable `followed_pointer` is assigned to, but never used
   --> userspace/libc/src/dns.rs:725:9
    |
725 |     let mut followed_pointer = false;
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: consider using `_followed_pointer` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `followed_pointer` is never read
   --> userspace/libc/src/dns.rs:747:13
    |
747 |             followed_pointer = true;
    |             ^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/libc/src/locale.rs:138:5
    |
138 |     unsafe { &raw mut C_LCONV }
    |     ^^^^^^ unnecessary `unsafe` block
    |
    = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:57:5
   |
57 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:74:5
   |
74 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:173:5
    |
173 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:208:5
    |
208 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:539:5
    |
539 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:612:5
    |
612 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:214:5
    |
214 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:233:5
    |
233 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:245:5
    |
245 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:257:5
    |
257 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:269:5
    |
269 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:281:5
    |
281 |     unsafe { syscall::syscall3(syscall::SYS_IOCTL, fd as usize, TCSBRK as usize, 1) as i32 }
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:343:5
    |
343 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:355:5
    |
355 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:145:5
    |
145 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:156:5
    |
156 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:170:5
    |
170 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: constant `MAX_LINE_LEN` is never used
 --> userspace/libc/src/pwd.rs:9:7
  |
9 | const MAX_LINE_LEN: usize = 512;
  |       ^^^^^^^^^^^^
  |
  = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `libc` (lib) generated 21 warnings
   Compiling esh v0.1.0 (/home/nd/repos/Projects/oxide_os/userspace/shell)
warning: unnecessary `unsafe` block
   --> userspace/shell/src/main.rs:143:19
    |
143 |     let out_ptr = unsafe { &raw mut PROMPT_OUT };
    |                   ^^^^^^ unnecessary `unsafe` block
    |
    = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/shell/src/main.rs:151:23
    |
151 |         let buf_ptr = unsafe { &raw mut CWD_BUF };
    |                       ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/shell/src/main.rs:156:19
    |
156 |     let is_root = unsafe { geteuid() == 0 };
    |                   ^^^^^^ unnecessary `unsafe` block

warning[E0133]: dereference of raw pointer is unsafe and requires unsafe block
   --> userspace/shell/src/main.rs:215:11
    |
215 |     while *p != 0 {
    |           ^^ dereference of raw pointer
    |
    = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html>
    = note: raw pointers may be null, dangling or unaligned; they can violate aliasing rules and cause data races: all of these are undefined behavior
note: an unsafe function restricts its caller, but its body is safe by default
   --> userspace/shell/src/main.rs:213:1
    |
213 | unsafe fn prints_raw(ptr: *const i8) {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    = note: `#[warn(unsafe_op_in_unsafe_fn)]` (part of `#[warn(rust_2024_compatibility)]`) on by default

warning[E0133]: dereference of raw pointer is unsafe and requires unsafe block
   --> userspace/shell/src/main.rs:216:17
    |
216 |         putchar(*p as u8);
    |                 ^^ dereference of raw pointer
    |
    = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html>
    = note: raw pointers may be null, dangling or unaligned; they can violate aliasing rules and cause data races: all of these are undefined behavior

warning[E0133]: call to unsafe function `core::ptr::const_ptr::<impl *const T>::add` is unsafe and requires unsafe block
   --> userspace/shell/src/main.rs:217:13
    |
217 |         p = p.add(1);
    |             ^^^^^^^^ call to unsafe function
    |
    = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html>
    = note: consult the function's documentation for information on how to avoid undefined behavior

warning[E0133]: dereference of raw pointer is unsafe and requires unsafe block
   --> userspace/shell/src/main.rs:224:11
    |
224 |     while *ptr.add(len) != 0 {
    |           ^^^^^^^^^^^^^ dereference of raw pointer
    |
    = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html>
    = note: raw pointers may be null, dangling or unaligned; they can violate aliasing rules and cause data races: all of these are undefined behavior
note: an unsafe function restricts its caller, but its body is safe by default
   --> userspace/shell/src/main.rs:222:1
    |
222 | unsafe fn cstr_to_str<'a>(ptr: *const i8) -> &'a str {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning[E0133]: call to unsafe function `core::ptr::const_ptr::<impl *const T>::add` is unsafe and requires unsafe block
   --> userspace/shell/src/main.rs:224:12
    |
224 |     while *ptr.add(len) != 0 {
    |            ^^^^^^^^^^^^ call to unsafe function
    |
    = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html>
    = note: consult the function's documentation for information on how to avoid undefined behavior

warning[E0133]: call to unsafe function `core::str::from_utf8_unchecked` is unsafe and requires unsafe block
   --> userspace/shell/src/main.rs:227:5
    |
227 |     core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr as *const u8, len))
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ call to unsafe function
    |
    = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html>
    = note: consult the function's documentation for information on how to avoid undefined behavior

warning[E0133]: call to unsafe function `core::slice::from_raw_parts` is unsafe and requires unsafe block
   --> userspace/shell/src/main.rs:227:36
    |
227 |     core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr as *const u8, len))
    |                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ call to unsafe function
    |
    = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html>
    = note: consult the function's documentation for information on how to avoid undefined behavior

warning: unused variable: `len`
   --> userspace/shell/src/main.rs:682:60
    |
682 | fn move_prev_word(cursor: &mut usize, buf: [u8; MAX_LINE], len: usize) {
    |                                                            ^^^ help: if this is intentional, prefix it with an underscore: `_len`
    |
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `num_completions` is never read
   --> userspace/shell/src/main.rs:753:31
    |
753 |     let mut num_completions = 0;
    |                               ^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: constant `CSI` is never used
  --> userspace/shell/src/main.rs:39:7
   |
39 | const CSI: u8 = b'[';
   |       ^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: function `prints_raw` is never used
   --> userspace/shell/src/main.rs:213:11
    |
213 | unsafe fn prints_raw(ptr: *const i8) {
    |           ^^^^^^^^^^

warning: function `cstr_to_str` is never used
   --> userspace/shell/src/main.rs:222:11
    |
222 | unsafe fn cstr_to_str<'a>(ptr: *const i8) -> &'a str {
    |           ^^^^^^^^^^^

For more information about this error, try `rustc --explain E0133`.
warning: `esh` (bin "esh") generated 15 warnings (run `cargo fix --bin "esh" -p esh` to apply 3 suggestions)
    Finished `release` profile [optimized] target(s) in 0.40s
  Building login (release)...
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/nd/repos/Projects/oxide_os/apps/gwbasic/Cargo.toml
workspace: /home/nd/repos/Projects/oxide_os/Cargo.toml
warning: variable `followed_pointer` is assigned to, but never used
   --> userspace/libc/src/dns.rs:725:9
    |
725 |     let mut followed_pointer = false;
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: consider using `_followed_pointer` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `followed_pointer` is never read
   --> userspace/libc/src/dns.rs:747:13
    |
747 |             followed_pointer = true;
    |             ^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/libc/src/locale.rs:138:5
    |
138 |     unsafe { &raw mut C_LCONV }
    |     ^^^^^^ unnecessary `unsafe` block
    |
    = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:57:5
   |
57 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:74:5
   |
74 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:173:5
    |
173 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:208:5
    |
208 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:539:5
    |
539 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:612:5
    |
612 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:214:5
    |
214 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:233:5
    |
233 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:245:5
    |
245 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:257:5
    |
257 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:269:5
    |
269 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:281:5
    |
281 |     unsafe { syscall::syscall3(syscall::SYS_IOCTL, fd as usize, TCSBRK as usize, 1) as i32 }
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:343:5
    |
343 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:355:5
    |
355 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:145:5
    |
145 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:156:5
    |
156 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:170:5
    |
170 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: constant `MAX_LINE_LEN` is never used
 --> userspace/libc/src/pwd.rs:9:7
  |
9 | const MAX_LINE_LEN: usize = 512;
  |       ^^^^^^^^^^^^
  |
  = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `libc` (lib) generated 21 warnings
   Compiling login v0.1.0 (/home/nd/repos/Projects/oxide_os/userspace/login)
    Finished `release` profile [optimized] target(s) in 0.16s
  Building coreutils (release)...
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/nd/repos/Projects/oxide_os/apps/gwbasic/Cargo.toml
workspace: /home/nd/repos/Projects/oxide_os/Cargo.toml
warning: variable `followed_pointer` is assigned to, but never used
   --> userspace/libc/src/dns.rs:725:9
    |
725 |     let mut followed_pointer = false;
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: consider using `_followed_pointer` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `followed_pointer` is never read
   --> userspace/libc/src/dns.rs:747:13
    |
747 |             followed_pointer = true;
    |             ^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/libc/src/locale.rs:138:5
    |
138 |     unsafe { &raw mut C_LCONV }
    |     ^^^^^^ unnecessary `unsafe` block
    |
    = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:57:5
   |
57 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:74:5
   |
74 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:173:5
    |
173 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:208:5
    |
208 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:539:5
    |
539 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:612:5
    |
612 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:214:5
    |
214 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:233:5
    |
233 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:245:5
    |
245 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:257:5
    |
257 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:269:5
    |
269 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:281:5
    |
281 |     unsafe { syscall::syscall3(syscall::SYS_IOCTL, fd as usize, TCSBRK as usize, 1) as i32 }
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:343:5
    |
343 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:355:5
    |
355 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:145:5
    |
145 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:156:5
    |
156 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:170:5
    |
170 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: constant `MAX_LINE_LEN` is never used
 --> userspace/libc/src/pwd.rs:9:7
  |
9 | const MAX_LINE_LEN: usize = 512;
  |       ^^^^^^^^^^^^
  |
  = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `libc` (lib) generated 21 warnings
   Compiling coreutils v0.1.0 (/home/nd/repos/Projects/oxide_os/userspace/coreutils)
warning: function `print_hex` is never used
   --> userspace/coreutils/src/bin/fbtest.rs:117:4
    |
117 | fn print_hex(n: u32) {
    |    ^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `first` is never read
   --> userspace/coreutils/src/bin/uname.rs:195:9
    |
195 |         first = false;
    |         ^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: trait `StrExt` is never used
   --> userspace/coreutils/src/bin/dd.rs:185:7
    |
185 | trait StrExt {
    |       ^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: function `strip_suffix` is never used
  --> userspace/coreutils/src/bin/basename.rs:79:4
   |
79 | fn strip_suffix<'a>(name: &'a str, suffix: Option<&'a str>) -> &'a str {
   |    ^^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: variable `total_received` is assigned to, but never used
   --> userspace/coreutils/src/bin/wget.rs:318:9
    |
318 |     let mut total_received = 0;
    |         ^^^^^^^^^^^^^^^^^^
    |
    = note: consider using `_total_received` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `total_received` is never read
   --> userspace/coreutils/src/bin/wget.rs:335:9
    |
335 |         total_received += received as usize;
    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `arg_idx` is never read
   --> userspace/coreutils/src/bin/wget.rs:457:13
    |
457 |             arg_idx += 1;
    |             ^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?

warning: constant `MAX_URL` is never used
  --> userspace/coreutils/src/bin/wget.rs:22:7
   |
22 | const MAX_URL: usize = 256;
   |       ^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: constant `DT_REG` is never used
  --> userspace/coreutils/src/bin/du.rs:30:7
   |
30 | const DT_REG: u8 = 8;
   |       ^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: variable `total_lines_shown` is assigned to, but never used
   --> userspace/coreutils/src/bin/more.rs:184:9
    |
184 |     let mut total_lines_shown = 0;
    |         ^^^^^^^^^^^^^^^^^^^^^
    |
    = note: consider using `_total_lines_shown` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `line_count` is never read
   --> userspace/coreutils/src/bin/more.rs:200:17
    |
200 |                 line_count += 1;
    |                 ^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `total_lines_shown` is never read
   --> userspace/coreutils/src/bin/more.rs:229:17
    |
229 |                 total_lines_shown += 1;
    |                 ^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?

warning: value assigned to `total_lines_shown` is never read
   --> userspace/coreutils/src/bin/more.rs:201:17
    |
201 |                 total_lines_shown += 1;
    |                 ^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?

warning: field `context_lines` is never read
  --> userspace/coreutils/src/bin/diff.rs:31:5
   |
23 | struct DiffConfig {
   |        ---------- field in this struct
...
31 |     context_lines: usize,
   |     ^^^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: function `format` is never used
   --> userspace/coreutils/src/bin/ip.rs:190:4
    |
190 | fn format(args: core::fmt::Arguments) -> String {
    |    ^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: function `parse_chain` is never used
   --> userspace/coreutils/src/bin/fw.rs:157:4
    |
157 | fn parse_chain(s: &str) -> Option<u8> {
    |    ^^^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: function `parse_action` is never used
   --> userspace/coreutils/src/bin/fw.rs:166:4
    |
166 | fn parse_action(s: &str) -> Option<u8> {
    |    ^^^^^^^^^^^^

warning: function `parse_proto` is never used
   --> userspace/coreutils/src/bin/fw.rs:175:4
    |
175 | fn parse_proto(s: &str) -> Option<u8> {
    |    ^^^^^^^^^^^

warning: function `parse_state` is never used
   --> userspace/coreutils/src/bin/fw.rs:185:4
    |
185 | fn parse_state(s: &str) -> Option<u8> {
    |    ^^^^^^^^^^^

warning: trait `ToAsciiLowercaseManual` is never used
   --> userspace/coreutils/src/bin/fw.rs:195:7
    |
195 | trait ToAsciiLowercaseManual {
    |       ^^^^^^^^^^^^^^^^^^^^^^

warning: variable `show_all` is assigned to, but never used
   --> userspace/coreutils/src/bin/ifconfig.rs:392:9
    |
392 |     let mut show_all = true;
    |         ^^^^^^^^^^^^
    |
    = note: consider using `_show_all` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `show_all` is never read
   --> userspace/coreutils/src/bin/ifconfig.rs:404:17
    |
404 |                 show_all = true;
    |                 ^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `show_all` is never read
   --> userspace/coreutils/src/bin/ifconfig.rs:409:21
    |
409 |                     show_all = false;
    |                     ^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?

warning: value assigned to `num_len` is never read
   --> userspace/coreutils/src/bin/nslookup.rs:189:27
    |
189 |         let mut num_len = 0;
    |                           ^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: unused variable: `original_pos`
   --> userspace/coreutils/src/bin/nslookup.rs:235:9
    |
235 |     let original_pos = pos;
    |         ^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_original_pos`
    |
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: unused variable: `i`
   --> userspace/coreutils/src/bin/nslookup.rs:371:13
    |
371 |         for i in 0..ancount {
    |             ^ help: if this is intentional, prefix it with an underscore: `_i`

warning: unused variable: `rclass`
   --> userspace/coreutils/src/bin/nslookup.rs:421:9
    |
421 |     let rclass = ((buf[*pos] as u16) << 8) | (buf[*pos + 1] as u16);
    |         ^^^^^^ help: if this is intentional, prefix it with an underscore: `_rclass`

warning: constant `DNS_FLAG_QR` is never used
  --> userspace/coreutils/src/bin/nslookup.rs:38:7
   |
38 | const DNS_FLAG_QR: u16 = 0x8000; // Query/Response
   |       ^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: constant `DNS_FLAG_TC` is never used
  --> userspace/coreutils/src/bin/nslookup.rs:40:7
   |
40 | const DNS_FLAG_TC: u16 = 0x0200; // Truncated
   |       ^^^^^^^^^^^

warning: constant `DNS_FLAG_RD` is never used
  --> userspace/coreutils/src/bin/nslookup.rs:41:7
   |
41 | const DNS_FLAG_RD: u16 = 0x0100; // Recursion desired
   |       ^^^^^^^^^^^

warning: constant `DNS_FLAG_RA` is never used
  --> userspace/coreutils/src/bin/nslookup.rs:42:7
   |
42 | const DNS_FLAG_RA: u16 = 0x0080; // Recursion available
   |       ^^^^^^^^^^^

warning: `coreutils` (bin "uname") generated 1 warning
warning: `coreutils` (bin "dd") generated 1 warning
warning: function `parse_number` is never used
   --> userspace/coreutils/src/bin/sed.rs:107:4
    |
107 | fn parse_number(s: &str) -> Option<u64> {
    |    ^^^^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `coreutils` (bin "fbtest") generated 1 warning
warning: trait `StrExt` is never used
   --> userspace/coreutils/src/bin/pkill.rs:100:7
    |
100 | trait StrExt {
    |       ^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `coreutils` (bin "basename") generated 1 warning
warning: `coreutils` (bin "du") generated 1 warning
warning: unused import: `send`
  --> userspace/coreutils/src/bin/nc.rs:16:85
   |
16 |     SOCKADDR_IN_SIZE, SockAddrIn, accept, af, bind, connect, ipproto, listen, recv, send, shut,
   |                                                                                     ^^^^
   |
   = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default

warning: `coreutils` (bin "more") generated 4 warnings
warning: unnecessary `unsafe` block
  --> userspace/coreutils/src/bin/testcolors.rs:90:5
   |
90 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block
   |
   = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: `coreutils` (bin "diff") generated 1 warning
warning: unnecessary `unsafe` block
   --> userspace/coreutils/src/bin/testcolors.rs:110:5
    |
110 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: `coreutils` (bin "wget") generated 4 warnings
warning: variable `nr` is assigned to, but never used
   --> userspace/coreutils/src/bin/awk.rs:355:9
    |
355 |     let mut nr = 0u64; // Record number
    |         ^^^^^^
    |
    = note: consider using `_nr` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `nr` is never read
   --> userspace/coreutils/src/bin/awk.rs:365:17
    |
365 |                 nr += 1;
    |                 ^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `nr` is never read
   --> userspace/coreutils/src/bin/awk.rs:434:9
    |
434 |         nr += 1;
    |         ^^^^^^^
    |
    = help: maybe it is overwritten before being read?

warning: constant `MAX_PROGRAM` is never used
  --> userspace/coreutils/src/bin/awk.rs:26:7
   |
26 | const MAX_PROGRAM: usize = 2048;
   |       ^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: unused variable: `config`
   --> userspace/coreutils/src/bin/xargs.rs:267:5
    |
267 |     config: &XargsConfig,
    |     ^^^^^^ help: if this is intentional, prefix it with an underscore: `_config`
    |
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: constant `DT_UNKNOWN` is never used
  --> userspace/coreutils/src/bin/ls.rs:30:7
   |
30 | const DT_UNKNOWN: u8 = 0;
   |       ^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `coreutils` (bin "pkill") generated 1 warning
warning: `coreutils` (bin "sed") generated 1 warning
warning: `coreutils` (bin "nslookup") generated 8 warnings (run `cargo fix --bin "nslookup" -p coreutils` to apply 3 suggestions)
warning: `coreutils` (bin "testcolors") generated 2 warnings
warning: `coreutils` (bin "fw") generated 5 warnings
warning: `coreutils` (bin "nc") generated 1 warning (run `cargo fix --bin "nc" -p coreutils` to apply 1 suggestion)
warning: `coreutils` (bin "awk") generated 4 warnings
warning: `coreutils` (bin "xargs") generated 1 warning (run `cargo fix --bin "xargs" -p coreutils` to apply 1 suggestion)
warning: `coreutils` (bin "ls") generated 1 warning
warning: `coreutils` (bin "ip") generated 1 warning
warning: `coreutils` (bin "ifconfig") generated 3 warnings
    Finished `release` profile [optimized] target(s) in 0.60s
  Building ssh (release)...
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/nd/repos/Projects/oxide_os/apps/gwbasic/Cargo.toml
workspace: /home/nd/repos/Projects/oxide_os/Cargo.toml
warning: variable `followed_pointer` is assigned to, but never used
   --> userspace/libc/src/dns.rs:725:9
    |
725 |     let mut followed_pointer = false;
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: consider using `_followed_pointer` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `followed_pointer` is never read
   --> userspace/libc/src/dns.rs:747:13
    |
747 |             followed_pointer = true;
    |             ^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/libc/src/locale.rs:138:5
    |
138 |     unsafe { &raw mut C_LCONV }
    |     ^^^^^^ unnecessary `unsafe` block
    |
    = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:57:5
   |
57 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:74:5
   |
74 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:173:5
    |
173 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:208:5
    |
208 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:539:5
    |
539 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:612:5
    |
612 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:214:5
    |
214 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:233:5
    |
233 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:245:5
    |
245 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:257:5
    |
257 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:269:5
    |
269 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:281:5
    |
281 |     unsafe { syscall::syscall3(syscall::SYS_IOCTL, fd as usize, TCSBRK as usize, 1) as i32 }
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:343:5
    |
343 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:355:5
    |
355 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:145:5
    |
145 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:156:5
    |
156 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:170:5
    |
170 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: constant `MAX_LINE_LEN` is never used
 --> userspace/libc/src/pwd.rs:9:7
  |
9 | const MAX_LINE_LEN: usize = 512;
  |       ^^^^^^^^^^^^
  |
  = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `libc` (lib) generated 21 warnings
   Compiling oxide-std v0.1.0 (/home/nd/repos/Projects/oxide_os/userspace/oxide-std)
warning: unnecessary `unsafe` block
  --> userspace/oxide-std/src/sync.rs:85:17
   |
85 |                 unsafe {
   |                 ^^^^^^ unnecessary `unsafe` block
   |
   = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/sync.rs:147:13
    |
147 |             unsafe {
    |             ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/sync.rs:210:17
    |
210 |                 unsafe {
    |                 ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/sync.rs:234:17
    |
234 |                 unsafe {
    |                 ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/sync.rs:296:13
    |
296 |             unsafe {
    |             ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/sync.rs:328:9
    |
328 |         unsafe {
    |         ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/sync.rs:364:9
    |
364 |         unsafe {
    |         ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/sync.rs:376:9
    |
376 |         unsafe {
    |         ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/sync.rs:385:9
    |
385 |         unsafe {
    |         ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/sync.rs:452:25
    |
452 |                         unsafe {
    |                         ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/sync.rs:461:21
    |
461 |                     unsafe {
    |                     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/sync.rs:522:13
    |
522 |             unsafe {
    |             ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/sync.rs:531:17
    |
531 |                 unsafe {
    |                 ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
  --> userspace/oxide-std/src/thread.rs:83:13
   |
83 |             unsafe {
   |             ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/thread.rs:155:9
    |
155 |         unsafe {
    |         ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/thread.rs:287:5
    |
287 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/oxide-std/src/thread.rs:297:5
    |
297 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: constant `READER_MASK` is never used
   --> userspace/oxide-std/src/sync.rs:172:7
    |
172 | const READER_MASK: u32 = 0x7FFF_FFFF;
    |       ^^^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `oxide-std` (lib) generated 18 warnings
   Compiling ssh v0.2.0 (/home/nd/repos/Projects/oxide_os/userspace/ssh)
warning: constant `CHACHA_KEY_LEN` is never used
  --> userspace/ssh/src/crypto.rs:11:11
   |
11 | pub const CHACHA_KEY_LEN: usize = 32;
   |           ^^^^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: field `0` is never read
  --> userspace/ssh/src/transport.rs:51:8
   |
51 |     Io(io::Error),
   |     -- ^^^^^^^^^
   |     |
   |     field in this variant
   |
   = note: `TransportError` has a derived impl for the trait `Debug`, but this is intentionally ignored during dead code analysis
help: consider changing the field to be of unit type to suppress this warning while preserving the field numbering, or remove the field
   |
51 -     Io(io::Error),
51 +     Io(()),
   |

warning: variant `KeyExchange` is never constructed
  --> userspace/ssh/src/transport.rs:55:5
   |
50 | pub enum TransportError {
   |          -------------- variant in this enum
...
55 |     KeyExchange,
   |     ^^^^^^^^^^^
   |
   = note: `TransportError` has a derived impl for the trait `Debug`, but this is intentionally ignored during dead code analysis

warning: constant `DISCONNECT` is never used
  --> userspace/ssh/src/transport.rs:21:15
   |
21 |     pub const DISCONNECT: u8 = 1;
   |               ^^^^^^^^^^

warning: constant `IGNORE` is never used
  --> userspace/ssh/src/transport.rs:22:15
   |
22 |     pub const IGNORE: u8 = 2;
   |               ^^^^^^

warning: constant `UNIMPLEMENTED` is never used
  --> userspace/ssh/src/transport.rs:23:15
   |
23 |     pub const UNIMPLEMENTED: u8 = 3;
   |               ^^^^^^^^^^^^^

warning: constant `DEBUG` is never used
  --> userspace/ssh/src/transport.rs:24:15
   |
24 |     pub const DEBUG: u8 = 4;
   |               ^^^^^

warning: `ssh` (bin "ssh") generated 7 warnings
    Finished `release` profile [optimized] target(s) in 0.57s
  Building sshd (release)...
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/nd/repos/Projects/oxide_os/apps/gwbasic/Cargo.toml
workspace: /home/nd/repos/Projects/oxide_os/Cargo.toml
warning: variable `followed_pointer` is assigned to, but never used
   --> userspace/libc/src/dns.rs:725:9
    |
725 |     let mut followed_pointer = false;
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: consider using `_followed_pointer` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `followed_pointer` is never read
   --> userspace/libc/src/dns.rs:747:13
    |
747 |             followed_pointer = true;
    |             ^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/libc/src/locale.rs:138:5
    |
138 |     unsafe { &raw mut C_LCONV }
    |     ^^^^^^ unnecessary `unsafe` block
    |
    = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:57:5
   |
57 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:74:5
   |
74 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:173:5
    |
173 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:208:5
    |
208 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:539:5
    |
539 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:612:5
    |
612 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:214:5
    |
214 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:233:5
    |
233 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:245:5
    |
245 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:257:5
    |
257 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:269:5
    |
269 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:281:5
    |
281 |     unsafe { syscall::syscall3(syscall::SYS_IOCTL, fd as usize, TCSBRK as usize, 1) as i32 }
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:343:5
    |
343 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:355:5
    |
355 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:145:5
    |
145 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:156:5
    |
156 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:170:5
    |
170 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: constant `MAX_LINE_LEN` is never used
 --> userspace/libc/src/pwd.rs:9:7
  |
9 | const MAX_LINE_LEN: usize = 512;
  |       ^^^^^^^^^^^^
  |
  = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `libc` (lib) generated 21 warnings
   Compiling sshd v0.1.0 (/home/nd/repos/Projects/oxide_os/userspace/sshd)
warning: unused import: `host_public_key`
 --> userspace/sshd/src/kex.rs:9:71
  |
9 |     SshCipher, derive_keys, encode_host_public_key, encode_signature, host_public_key,
  |                                                                       ^^^^^^^^^^^^^^^
  |
  = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default

warning: unused import: `decode_u8`
  --> userspace/sshd/src/kex.rs:13:67
   |
13 |     SshTransport, TransportError, TransportResult, decode_string, decode_u8, encode_name_list,
   |                                                                   ^^^^^^^^^

warning: unused import: `alloc::string::String`
 --> userspace/sshd/src/transport.rs:9:5
  |
9 | use alloc::string::String;
  |     ^^^^^^^^^^^^^^^^^^^^^

warning: unused import: `libc::*`
  --> userspace/sshd/src/transport.rs:12:5
   |
12 | use libc::*;
   |     ^^^^^^^

warning: unused import: `host_public_key`
  --> userspace/sshd/src/transport.rs:14:32
   |
14 | use crate::crypto::{SshCipher, host_public_key};
   |                                ^^^^^^^^^^^^^^^

warning: unused import: `alloc::vec::Vec`
  --> userspace/sshd/src/main.rs:21:5
   |
21 | use alloc::vec::Vec;
   |     ^^^^^^^^^^^^^^^

warning: unused variable: `pty_master`
   --> userspace/sshd/src/session.rs:199:9
    |
199 |     let pty_master = match channel.pty_master {
    |         ^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_pty_master`
    |
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: unused variable: `pty_master`
   --> userspace/sshd/src/session.rs:266:9
    |
266 |     let pty_master = match channel.pty_master {
    |         ^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_pty_master`

warning: unused variable: `e`
  --> userspace/sshd/src/main.rs:71:13
   |
71 |         Err(e) => {
   |             ^ help: if this is intentional, prefix it with an underscore: `_e`

warning: unused variable: `e`
  --> userspace/sshd/src/main.rs:78:16
   |
78 |     if let Err(e) = transport.version_exchange() {
   |                ^ help: if this is intentional, prefix it with an underscore: `_e`

warning: unused variable: `e`
  --> userspace/sshd/src/main.rs:85:16
   |
85 |     if let Err(e) = transport.key_exchange() {
   |                ^ help: if this is intentional, prefix it with an underscore: `_e`

warning: unused variable: `e`
  --> userspace/sshd/src/main.rs:92:16
   |
92 |     if let Err(e) = auth::authenticate(&mut transport) {
   |                ^ help: if this is intentional, prefix it with an underscore: `_e`

warning: unused variable: `e`
  --> userspace/sshd/src/main.rs:99:16
   |
99 |     if let Err(e) = session::run_session(&mut transport) {
   |                ^ help: if this is intentional, prefix it with an underscore: `_e`

warning: unused variable: `e`
   --> userspace/sshd/src/main.rs:116:16
    |
116 |     if let Err(e) = crypto::init_host_key() {
    |                ^ help: if this is intentional, prefix it with an underscore: `_e`

warning: variant `Opening` is never constructed
  --> userspace/sshd/src/channel.rs:21:5
   |
20 | pub enum ChannelState {
   |          ------------ variant in this enum
21 |     Opening,
   |     ^^^^^^^
   |
   = note: `ChannelState` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: fields `local_id` and `max_packet` are never read
  --> userspace/sshd/src/channel.rs:30:9
   |
28 | pub struct Channel {
   |            ------- fields in this struct
29 |     /// Local channel ID
30 |     pub local_id: u32,
   |         ^^^^^^^^
...
40 |     pub max_packet: u32,
   |         ^^^^^^^^^^

warning: method `remove` is never used
   --> userspace/sshd/src/channel.rs:278:12
    |
 70 | impl ChannelManager {
    | ------------------- method in this implementation
...
278 |     pub fn remove(&mut self, local_id: u32) {
    |            ^^^^^^

warning: constant `CHACHA_KEY_LEN` is never used
  --> userspace/sshd/src/crypto.rs:17:11
   |
17 | pub const CHACHA_KEY_LEN: usize = 32;
   |           ^^^^^^^^^^^^^^

warning: constant `CHACHA_NONCE_LEN` is never used
  --> userspace/sshd/src/crypto.rs:20:11
   |
20 | pub const CHACHA_NONCE_LEN: usize = 12;
   |           ^^^^^^^^^^^^^^^^

warning: variants `KeyExchange` and `Closed` are never constructed
  --> userspace/sshd/src/transport.rs:89:5
   |
79 | pub enum TransportError {
   |          -------------- variants in this enum
...
89 |     KeyExchange,
   |     ^^^^^^^^^^^
90 |     /// Connection closed
91 |     Closed,
   |     ^^^^^^
   |
   = note: `TransportError` has a derived impl for the trait `Debug`, but this is intentionally ignored during dead code analysis

warning: methods `recv_raw`, `recv_exact`, and `fd` are never used
   --> userspace/sshd/src/transport.rs:196:12
    |
118 | impl SshTransport {
    | ----------------- methods in this implementation
...
196 |     pub fn recv_raw(&mut self, buf: &mut [u8]) -> TransportResult<usize> {
    |            ^^^^^^^^
...
215 |     pub fn recv_exact(&mut self, buf: &mut [u8]) -> TransportResult<()> {
    |            ^^^^^^^^^^
...
284 |     pub fn fd(&self) -> i32 {
    |            ^^

warning: constant `KEXDH_INIT` is never used
  --> userspace/sshd/src/transport.rs:34:15
   |
34 |     pub const KEXDH_INIT: u8 = 30;
   |               ^^^^^^^^^^

warning: constant `KEXDH_REPLY` is never used
  --> userspace/sshd/src/transport.rs:35:15
   |
35 |     pub const KEXDH_REPLY: u8 = 31;
   |               ^^^^^^^^^^^

warning: constant `USERAUTH_BANNER` is never used
  --> userspace/sshd/src/transport.rs:43:15
   |
43 |     pub const USERAUTH_BANNER: u8 = 53;
   |               ^^^^^^^^^^^^^^^

warning: constant `CHANNEL_EXTENDED_DATA` is never used
  --> userspace/sshd/src/transport.rs:50:15
   |
50 |     pub const CHANNEL_EXTENDED_DATA: u8 = 95;
   |               ^^^^^^^^^^^^^^^^^^^^^

warning: constant `HOST_NOT_ALLOWED_TO_CONNECT` is never used
  --> userspace/sshd/src/transport.rs:60:15
   |
60 |     pub const HOST_NOT_ALLOWED_TO_CONNECT: u32 = 1;
   |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning: constant `PROTOCOL_ERROR` is never used
  --> userspace/sshd/src/transport.rs:61:15
   |
61 |     pub const PROTOCOL_ERROR: u32 = 2;
   |               ^^^^^^^^^^^^^^

warning: constant `KEY_EXCHANGE_FAILED` is never used
  --> userspace/sshd/src/transport.rs:62:15
   |
62 |     pub const KEY_EXCHANGE_FAILED: u32 = 3;
   |               ^^^^^^^^^^^^^^^^^^^

warning: constant `RESERVED` is never used
  --> userspace/sshd/src/transport.rs:63:15
   |
63 |     pub const RESERVED: u32 = 4;
   |               ^^^^^^^^

warning: constant `MAC_ERROR` is never used
  --> userspace/sshd/src/transport.rs:64:15
   |
64 |     pub const MAC_ERROR: u32 = 5;
   |               ^^^^^^^^^

warning: constant `COMPRESSION_ERROR` is never used
  --> userspace/sshd/src/transport.rs:65:15
   |
65 |     pub const COMPRESSION_ERROR: u32 = 6;
   |               ^^^^^^^^^^^^^^^^^

warning: constant `SERVICE_NOT_AVAILABLE` is never used
  --> userspace/sshd/src/transport.rs:66:15
   |
66 |     pub const SERVICE_NOT_AVAILABLE: u32 = 7;
   |               ^^^^^^^^^^^^^^^^^^^^^

warning: constant `PROTOCOL_VERSION_NOT_SUPPORTED` is never used
  --> userspace/sshd/src/transport.rs:67:15
   |
67 |     pub const PROTOCOL_VERSION_NOT_SUPPORTED: u32 = 8;
   |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning: constant `HOST_KEY_NOT_VERIFIABLE` is never used
  --> userspace/sshd/src/transport.rs:68:15
   |
68 |     pub const HOST_KEY_NOT_VERIFIABLE: u32 = 9;
   |               ^^^^^^^^^^^^^^^^^^^^^^^

warning: constant `CONNECTION_LOST` is never used
  --> userspace/sshd/src/transport.rs:69:15
   |
69 |     pub const CONNECTION_LOST: u32 = 10;
   |               ^^^^^^^^^^^^^^^

warning: constant `BY_APPLICATION` is never used
  --> userspace/sshd/src/transport.rs:70:15
   |
70 |     pub const BY_APPLICATION: u32 = 11;
   |               ^^^^^^^^^^^^^^

warning: constant `TOO_MANY_CONNECTIONS` is never used
  --> userspace/sshd/src/transport.rs:71:15
   |
71 |     pub const TOO_MANY_CONNECTIONS: u32 = 12;
   |               ^^^^^^^^^^^^^^^^^^^^

warning: constant `AUTH_CANCELLED_BY_USER` is never used
  --> userspace/sshd/src/transport.rs:72:15
   |
72 |     pub const AUTH_CANCELLED_BY_USER: u32 = 13;
   |               ^^^^^^^^^^^^^^^^^^^^^^

warning: constant `NO_MORE_AUTH_METHODS_AVAILABLE` is never used
  --> userspace/sshd/src/transport.rs:73:15
   |
73 |     pub const NO_MORE_AUTH_METHODS_AVAILABLE: u32 = 14;
   |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning: constant `ILLEGAL_USER_NAME` is never used
  --> userspace/sshd/src/transport.rs:74:15
   |
74 |     pub const ILLEGAL_USER_NAME: u32 = 15;
   |               ^^^^^^^^^^^^^^^^^

warning: `sshd` (bin "sshd") generated 40 warnings (run `cargo fix --bin "sshd" -p sshd` to apply 14 suggestions)
    Finished `release` profile [optimized] target(s) in 0.41s
  Building service (release)...
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/nd/repos/Projects/oxide_os/apps/gwbasic/Cargo.toml
workspace: /home/nd/repos/Projects/oxide_os/Cargo.toml
warning: variable `followed_pointer` is assigned to, but never used
   --> userspace/libc/src/dns.rs:725:9
    |
725 |     let mut followed_pointer = false;
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: consider using `_followed_pointer` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `followed_pointer` is never read
   --> userspace/libc/src/dns.rs:747:13
    |
747 |             followed_pointer = true;
    |             ^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/libc/src/locale.rs:138:5
    |
138 |     unsafe { &raw mut C_LCONV }
    |     ^^^^^^ unnecessary `unsafe` block
    |
    = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:57:5
   |
57 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:74:5
   |
74 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:173:5
    |
173 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:208:5
    |
208 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:539:5
    |
539 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:612:5
    |
612 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:214:5
    |
214 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:233:5
    |
233 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:245:5
    |
245 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:257:5
    |
257 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:269:5
    |
269 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:281:5
    |
281 |     unsafe { syscall::syscall3(syscall::SYS_IOCTL, fd as usize, TCSBRK as usize, 1) as i32 }
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:343:5
    |
343 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:355:5
    |
355 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:145:5
    |
145 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:156:5
    |
156 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:170:5
    |
170 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: constant `MAX_LINE_LEN` is never used
 --> userspace/libc/src/pwd.rs:9:7
  |
9 | const MAX_LINE_LEN: usize = 512;
  |       ^^^^^^^^^^^^
  |
  = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `libc` (lib) generated 21 warnings
   Compiling service v0.1.0 (/home/nd/repos/Projects/oxide_os/userspace/servicemgr)
warning: value assigned to `new_len` is never read
   --> userspace/servicemgr/src/main.rs:736:23
    |
736 |     let mut new_len = 0;
    |                       ^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `new_len` is never read
   --> userspace/servicemgr/src/main.rs:829:23
    |
829 |     let mut new_len = 0;
    |                       ^
    |
    = help: maybe it is overwritten before being read?

warning: `service` (bin "service") generated 2 warnings
    Finished `release` profile [optimized] target(s) in 0.22s
  Building networkd (release)...
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/nd/repos/Projects/oxide_os/apps/gwbasic/Cargo.toml
workspace: /home/nd/repos/Projects/oxide_os/Cargo.toml
warning: variable `followed_pointer` is assigned to, but never used
   --> userspace/libc/src/dns.rs:725:9
    |
725 |     let mut followed_pointer = false;
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: consider using `_followed_pointer` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `followed_pointer` is never read
   --> userspace/libc/src/dns.rs:747:13
    |
747 |             followed_pointer = true;
    |             ^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/libc/src/locale.rs:138:5
    |
138 |     unsafe { &raw mut C_LCONV }
    |     ^^^^^^ unnecessary `unsafe` block
    |
    = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:57:5
   |
57 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:74:5
   |
74 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:173:5
    |
173 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:208:5
    |
208 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:539:5
    |
539 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:612:5
    |
612 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:214:5
    |
214 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:233:5
    |
233 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:245:5
    |
245 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:257:5
    |
257 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:269:5
    |
269 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:281:5
    |
281 |     unsafe { syscall::syscall3(syscall::SYS_IOCTL, fd as usize, TCSBRK as usize, 1) as i32 }
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:343:5
    |
343 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:355:5
    |
355 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:145:5
    |
145 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:156:5
    |
156 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:170:5
    |
170 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: constant `MAX_LINE_LEN` is never used
 --> userspace/libc/src/pwd.rs:9:7
  |
9 | const MAX_LINE_LEN: usize = 512;
  |       ^^^^^^^^^^^^
  |
  = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `libc` (lib) generated 21 warnings
   Compiling networkd v0.1.0 (/home/nd/repos/Projects/oxide_os/userspace/networkd)
warning: trait `ToAsciiLowercase` is never used
   --> userspace/networkd/src/main.rs:322:7
    |
322 | trait ToAsciiLowercase {
    |       ^^^^^^^^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `networkd` (bin "networkd") generated 1 warning
    Finished `release` profile [optimized] target(s) in 0.25s
  Building gwbasic (release)...
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/nd/repos/Projects/oxide_os/apps/gwbasic/Cargo.toml
workspace: /home/nd/repos/Projects/oxide_os/Cargo.toml
warning: variable `followed_pointer` is assigned to, but never used
   --> userspace/libc/src/dns.rs:725:9
    |
725 |     let mut followed_pointer = false;
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: consider using `_followed_pointer` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `followed_pointer` is never read
   --> userspace/libc/src/dns.rs:747:13
    |
747 |             followed_pointer = true;
    |             ^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/libc/src/locale.rs:138:5
    |
138 |     unsafe { &raw mut C_LCONV }
    |     ^^^^^^ unnecessary `unsafe` block
    |
    = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:57:5
   |
57 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:74:5
   |
74 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:173:5
    |
173 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:208:5
    |
208 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:539:5
    |
539 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:612:5
    |
612 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:214:5
    |
214 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:233:5
    |
233 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:245:5
    |
245 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:257:5
    |
257 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:269:5
    |
269 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:281:5
    |
281 |     unsafe { syscall::syscall3(syscall::SYS_IOCTL, fd as usize, TCSBRK as usize, 1) as i32 }
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:343:5
    |
343 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:355:5
    |
355 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:145:5
    |
145 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:156:5
    |
156 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:170:5
    |
170 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: constant `MAX_LINE_LEN` is never used
 --> userspace/libc/src/pwd.rs:9:7
  |
9 | const MAX_LINE_LEN: usize = 512;
  |       ^^^^^^^^^^^^
  |
  = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `libc` (lib) generated 21 warnings
   Compiling oxide-gwbasic v0.1.0 (/home/nd/repos/Projects/oxide_os/apps/gwbasic)
warning: unexpected `cfg` condition value: `watos`
  --> apps/gwbasic/src/platform/mod.rs:11:7
   |
11 | #[cfg(feature = "watos")]
   |       ^^^^^^^^^^^^^^^^^
   |
   = note: expected values for `feature` are: `default`, `host`, `oxide`, and `std`
   = help: consider adding `watos` as a feature in `Cargo.toml`
   = note: see <https://doc.rust-lang.org/nightly/rustc/check-cfg/cargo-specifics.html> for more information about checking conditional configuration
   = note: `#[warn(unexpected_cfgs)]` on by default

warning: unexpected `cfg` condition value: `watos`
  --> apps/gwbasic/src/platform/mod.rs:13:7
   |
13 | #[cfg(feature = "watos")]
   |       ^^^^^^^^^^^^^^^^^
   |
   = note: expected values for `feature` are: `default`, `host`, `oxide`, and `std`
   = help: consider adding `watos` as a feature in `Cargo.toml`
   = note: see <https://doc.rust-lang.org/nightly/rustc/check-cfg/cargo-specifics.html> for more information about checking conditional configuration

warning: unexpected `cfg` condition value: `watos`
  --> apps/gwbasic/src/platform/mod.rs:23:37
   |
23 | #[cfg(all(not(feature = "std"), not(feature = "watos"), not(feature = "oxide")))]
   |                                     ^^^^^^^^^^^^^^^^^
   |
   = note: expected values for `feature` are: `default`, `host`, `oxide`, and `std`
   = help: consider adding `watos` as a feature in `Cargo.toml`
   = note: see <https://doc.rust-lang.org/nightly/rustc/check-cfg/cargo-specifics.html> for more information about checking conditional configuration

warning: unexpected `cfg` condition value: `watos`
  --> apps/gwbasic/src/platform/mod.rs:25:37
   |
25 | #[cfg(all(not(feature = "std"), not(feature = "watos"), not(feature = "oxide")))]
   |                                     ^^^^^^^^^^^^^^^^^
   |
   = note: expected values for `feature` are: `default`, `host`, `oxide`, and `std`
   = help: consider adding `watos` as a feature in `Cargo.toml`
   = note: see <https://doc.rust-lang.org/nightly/rustc/check-cfg/cargo-specifics.html> for more information about checking conditional configuration

warning: unused doc comment
 --> apps/gwbasic/src/console.rs:5:1
  |
5 |   /// External syscall for WATOS console write
  |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
6 |   #[cfg(not(feature = "std"))]
7 | / extern "C" {
8 | |     pub fn watos_console_write(buf: *const u8, len: usize);
9 | | }
  | |_- rustdoc does not generate documentation for extern blocks
  |
  = help: use `//` for a plain comment
  = note: `#[warn(unused_doc_comments)]` (part of `#[warn(unused)]`) on by default

warning: method `get_pixel_local` is never used
   --> apps/gwbasic/src/graphics_backend/watos_vga.rs:324:8
    |
200 | impl WatosVgaBackend {
    | -------------------- method in this implementation
...
324 |     fn get_pixel_local(&self, x: usize, y: usize) -> u8 {
    |        ^^^^^^^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: constant `SYS_VGA_SET_MODE` is never used
   --> apps/gwbasic/src/graphics_backend/watos_vga.rs:116:15
    |
116 |     pub const SYS_VGA_SET_MODE: u32 = 30;
    |               ^^^^^^^^^^^^^^^^

warning: constant `SYS_VGA_SET_PIXEL` is never used
   --> apps/gwbasic/src/graphics_backend/watos_vga.rs:117:15
    |
117 |     pub const SYS_VGA_SET_PIXEL: u32 = 31;
    |               ^^^^^^^^^^^^^^^^^

warning: constant `SYS_VGA_GET_PIXEL` is never used
   --> apps/gwbasic/src/graphics_backend/watos_vga.rs:118:15
    |
118 |     pub const SYS_VGA_GET_PIXEL: u32 = 32;
    |               ^^^^^^^^^^^^^^^^^

warning: constant `SYS_VGA_SET_PALETTE` is never used
   --> apps/gwbasic/src/graphics_backend/watos_vga.rs:122:15
    |
122 |     pub const SYS_VGA_SET_PALETTE: u32 = 36;
    |               ^^^^^^^^^^^^^^^^^^^

warning: constant `SYS_VGA_GET_SESSION_INFO` is never used
   --> apps/gwbasic/src/graphics_backend/watos_vga.rs:126:15
    |
126 |     pub const SYS_VGA_GET_SESSION_INFO: u32 = 40;
    |               ^^^^^^^^^^^^^^^^^^^^^^^^

warning: constant `SYS_VGA_ENUMERATE_MODES` is never used
   --> apps/gwbasic/src/graphics_backend/watos_vga.rs:127:15
    |
127 |     pub const SYS_VGA_ENUMERATE_MODES: u32 = 41;
    |               ^^^^^^^^^^^^^^^^^^^^^^^

warning: function `syscall2` is never used
   --> apps/gwbasic/src/graphics_backend/watos_vga.rs:155:19
    |
155 |     pub unsafe fn syscall2(num: u32, arg1: u64, arg2: u64) -> u64 {
    |                   ^^^^^^^^

warning: field `graphics_mode` is never read
  --> apps/gwbasic/src/interpreter.rs:58:5
   |
29 | pub struct Interpreter {
   |            ----------- field in this struct
...
58 |     graphics_mode: GraphicsMode,
   |     ^^^^^^^^^^^^^

warning: `extern` block uses type `(u16, u8, u8)`, which is not FFI-safe
   --> apps/gwbasic/src/functions.rs:712:28
    |
712 |     fn watos_get_date() -> (u16, u8, u8);
    |                            ^^^^^^^^^^^^^ not FFI-safe
    |
    = help: consider using a struct instead
    = note: tuples have unspecified layout
    = note: `#[warn(improper_ctypes)]` on by default

warning: `extern` block uses type `(u8, u8, u8)`, which is not FFI-safe
   --> apps/gwbasic/src/functions.rs:713:28
    |
713 |     fn watos_get_time() -> (u8, u8, u8);
    |                            ^^^^^^^^^^^^ not FFI-safe
    |
    = help: consider using a struct instead
    = note: tuples have unspecified layout

warning: `oxide-gwbasic` (lib) generated 16 warnings
warning: unused import: `alloc::string::String`
  --> apps/gwbasic/src/oxide_main.rs:11:5
   |
11 | use alloc::string::String;
   |     ^^^^^^^^^^^^^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default

warning: `extern` fn uses type `(u16, u8, u8)`, which is not FFI-safe
   --> apps/gwbasic/src/oxide_main.rs:131:39
    |
131 | pub extern "C" fn watos_get_date() -> (u16, u8, u8) {
    |                                       ^^^^^^^^^^^^^ not FFI-safe
    |
    = help: consider using a struct instead
    = note: tuples have unspecified layout
    = note: `#[warn(improper_ctypes_definitions)]` on by default

warning: `extern` fn uses type `(u8, u8, u8)`, which is not FFI-safe
   --> apps/gwbasic/src/oxide_main.rs:148:39
    |
148 | pub extern "C" fn watos_get_time() -> (u8, u8, u8) {
    |                                       ^^^^^^^^^^^^ not FFI-safe
    |
    = help: consider using a struct instead
    = note: tuples have unspecified layout

warning: `oxide-gwbasic` (bin "gwbasic") generated 3 warnings (run `cargo fix --bin "gwbasic" -p oxide-gwbasic` to apply 1 suggestion)
    Finished `release` profile [optimized] target(s) in 1.62s
  Building testcolors (release)...
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/nd/repos/Projects/oxide_os/apps/gwbasic/Cargo.toml
workspace: /home/nd/repos/Projects/oxide_os/Cargo.toml
warning: variable `followed_pointer` is assigned to, but never used
   --> userspace/libc/src/dns.rs:725:9
    |
725 |     let mut followed_pointer = false;
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: consider using `_followed_pointer` instead
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: value assigned to `followed_pointer` is never read
   --> userspace/libc/src/dns.rs:747:13
    |
747 |             followed_pointer = true;
    |             ^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/libc/src/locale.rs:138:5
    |
138 |     unsafe { &raw mut C_LCONV }
    |     ^^^^^^ unnecessary `unsafe` block
    |
    = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:57:5
   |
57 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
  --> userspace/libc/src/poll.rs:74:5
   |
74 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:173:5
    |
173 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/poll.rs:208:5
    |
208 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:539:5
    |
539 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/pwd.rs:612:5
    |
612 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:214:5
    |
214 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:233:5
    |
233 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:245:5
    |
245 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:257:5
    |
257 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:269:5
    |
269 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:281:5
    |
281 |     unsafe { syscall::syscall3(syscall::SYS_IOCTL, fd as usize, TCSBRK as usize, 1) as i32 }
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:343:5
    |
343 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/termios.rs:355:5
    |
355 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:145:5
    |
145 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:156:5
    |
156 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: unnecessary `unsafe` block
   --> userspace/libc/src/time.rs:170:5
    |
170 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: constant `MAX_LINE_LEN` is never used
 --> userspace/libc/src/pwd.rs:9:7
  |
9 | const MAX_LINE_LEN: usize = 512;
  |       ^^^^^^^^^^^^
  |
  = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `libc` (lib) generated 21 warnings
warning: unnecessary `unsafe` block
  --> userspace/coreutils/src/bin/testcolors.rs:90:5
   |
90 |     unsafe {
   |     ^^^^^^ unnecessary `unsafe` block
   |
   = note: `#[warn(unused_unsafe)]` (part of `#[warn(unused)]`) on by default

warning: unnecessary `unsafe` block
   --> userspace/coreutils/src/bin/testcolors.rs:110:5
    |
110 |     unsafe {
    |     ^^^^^^ unnecessary `unsafe` block

warning: `coreutils` (bin "testcolors") generated 2 warnings
    Finished `release` profile [optimized] target(s) in 0.03s
Stripping binaries...
Userspace programs built (release).
Creating initramfs (release)...
Initramfs created: target/initramfs.cpio
-rw-r--r-- 1 nd nd 1252864 Jan 25 07:55 target/initramfs.cpio
Building bootloader (release)...
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/nd/repos/Projects/oxide_os/apps/gwbasic/Cargo.toml
workspace: /home/nd/repos/Projects/oxide_os/Cargo.toml
   Compiling fb v0.1.0 (/home/nd/repos/Projects/oxide_os/crates/graphics/fb)
warning: value assigned to `param_count` is never read
   --> crates/graphics/fb/src/console.rs:452:25
    |
452 |                         param_count += 1;
    |                         ^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: `fb` (lib) generated 1 warning
   Compiling boot-uefi v0.1.0 (/home/nd/repos/Projects/oxide_os/bootloader/boot-uefi)
warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:165:27
    |
165 |     let st = uefi::table::system_table_boot().expect("Boot services not available");
    |                           ^^^^^^^^^^^^^^^^^
    |
    = note: `#[warn(deprecated)]` on by default

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:190:40
    |
190 |     if let Some(mut st) = uefi::table::system_table_boot() {
    |                                        ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:198:33
    |
198 |     let st = match uefi::table::system_table_boot() {
    |                                 ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:395:40
    |
395 |     if let Some(mut st) = uefi::table::system_table_boot() {
    |                                        ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:403:36
    |
403 |     if let Some(st) = uefi::table::system_table_boot() {
    |                                    ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:663:27
    |
663 |     let st = uefi::table::system_table_boot().ok_or("No boot services")?;
    |                           ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:711:27
    |
711 |     let st = uefi::table::system_table_boot().ok_or("No boot services")?;
    |                           ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:770:27
    |
770 |     let st = uefi::table::system_table_boot()?;
    |                           ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:779:27
    |
779 |     let st = uefi::table::system_table_boot().expect("Boot services not available");
    |                           ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:812:27
    |
812 |     let st = uefi::table::system_table_boot()?;
    |                           ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:844:27
    |
844 |     let st = uefi::table::system_table_boot()?;
    |                           ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:942:40
    |
942 |     if let Some(mut st) = uefi::table::system_table_boot() {
    |                                        ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:950:40
    |
950 |     if let Some(mut st) = uefi::table::system_table_boot() {
    |                                        ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/main.rs:967:40
    |
967 |     if let Some(mut st) = uefi::table::system_table_boot() {
    |                                        ^^^^^^^^^^^^^^^^^

warning: use of deprecated function `uefi::table::system_table_boot`: Use the uefi::boot module instead. See https://github.com/rust-osdev/uefi-rs/blob/HEAD/docs/funcs_migration.md
   --> bootloader/boot-uefi/src/paging.rs:170:27
    |
170 |     let st = uefi::table::system_table_boot().expect("Boot services not available");
    |                           ^^^^^^^^^^^^^^^^^

warning: constant `KERNEL_PATH` is never used
  --> bootloader/boot-uefi/src/main.rs:30:7
   |
30 | const KERNEL_PATH: &str = "\\EFI\\OXIDE\\kernel.elf";
   |       ^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: constant `INITRAMFS_PATH` is never used
  --> bootloader/boot-uefi/src/main.rs:33:7
   |
33 | const INITRAMFS_PATH: &str = "\\EFI\\OXIDE\\initramfs.cpio";
   |       ^^^^^^^^^^^^^^

warning: function `display_ascii_logo` is never used
   --> bootloader/boot-uefi/src/main.rs:362:4
    |
362 | fn display_ascii_logo() {
    |    ^^^^^^^^^^^^^^^^^^

warning: unused return value of `SystemTable::<Boot>::exit_boot_services` that must be used
   --> bootloader/boot-uefi/src/main.rs:167:9
    |
167 |         st.exit_boot_services(uefi::table::boot::MemoryType::LOADER_DATA);
    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = note: `#[warn(unused_must_use)]` (part of `#[warn(unused)]`) on by default
help: use `let _ = ...` to ignore the resulting value
    |
167 |         let _ = st.exit_boot_services(uefi::table::boot::MemoryType::LOADER_DATA);
    |         +++++++

warning: `boot-uefi` (bin "boot-uefi") generated 19 warnings
    Finished `release` profile [optimized] target(s) in 0.55s
Building kernel with debug features...
warning: profiles for the non root package will be ignored, specify profiles at the workspace root:
package:   /home/nd/repos/Projects/oxide_os/apps/gwbasic/Cargo.toml
workspace: /home/nd/repos/Projects/oxide_os/Cargo.toml
warning: constant `SYSTEM_FLAG` is never used
  --> crates/drivers/input/ps2/src/lib.rs:30:15
   |
30 |     pub const SYSTEM_FLAG: u8 = 0x04;
   |               ^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: constant `COMMAND_DATA` is never used
  --> crates/drivers/input/ps2/src/lib.rs:31:15
   |
31 |     pub const COMMAND_DATA: u8 = 0x08;
   |               ^^^^^^^^^^^^

warning: constant `KEYBOARD_LOCK` is never used
  --> crates/drivers/input/ps2/src/lib.rs:32:15
   |
32 |     pub const KEYBOARD_LOCK: u8 = 0x10;
   |               ^^^^^^^^^^^^^

warning: constant `MOUSE_DATA` is never used
  --> crates/drivers/input/ps2/src/lib.rs:33:15
   |
33 |     pub const MOUSE_DATA: u8 = 0x20;
   |               ^^^^^^^^^^

warning: constant `TIMEOUT_ERROR` is never used
  --> crates/drivers/input/ps2/src/lib.rs:34:15
   |
34 |     pub const TIMEOUT_ERROR: u8 = 0x40;
   |               ^^^^^^^^^^^^^

warning: constant `PARITY_ERROR` is never used
  --> crates/drivers/input/ps2/src/lib.rs:35:15
   |
35 |     pub const PARITY_ERROR: u8 = 0x80;
   |               ^^^^^^^^^^^^

warning: constant `DISABLE_PORT2` is never used
  --> crates/drivers/input/ps2/src/lib.rs:42:15
   |
42 |     pub const DISABLE_PORT2: u8 = 0xA7;
   |               ^^^^^^^^^^^^^

warning: constant `TEST_PORT2` is never used
  --> crates/drivers/input/ps2/src/lib.rs:44:15
   |
44 |     pub const TEST_PORT2: u8 = 0xA9;
   |               ^^^^^^^^^^

warning: constant `SELF_TEST` is never used
  --> crates/drivers/input/ps2/src/lib.rs:45:15
   |
45 |     pub const SELF_TEST: u8 = 0xAA;
   |               ^^^^^^^^^

warning: constant `TEST_PORT1` is never used
  --> crates/drivers/input/ps2/src/lib.rs:46:15
   |
46 |     pub const TEST_PORT1: u8 = 0xAB;
   |               ^^^^^^^^^^

warning: constant `DISABLE_PORT1` is never used
  --> crates/drivers/input/ps2/src/lib.rs:47:15
   |
47 |     pub const DISABLE_PORT1: u8 = 0xAD;
   |               ^^^^^^^^^^^^^

warning: constant `ENABLE_PORT1` is never used
  --> crates/drivers/input/ps2/src/lib.rs:48:15
   |
48 |     pub const ENABLE_PORT1: u8 = 0xAE;
   |               ^^^^^^^^^^^^

warning: constant `ECHO` is never used
  --> crates/drivers/input/ps2/src/lib.rs:55:15
   |
55 |     pub const ECHO: u8 = 0xEE;
   |               ^^^^

warning: constant `GET_SET_SCANCODE` is never used
  --> crates/drivers/input/ps2/src/lib.rs:56:15
   |
56 |     pub const GET_SET_SCANCODE: u8 = 0xF0;
   |               ^^^^^^^^^^^^^^^^

warning: constant `IDENTIFY` is never used
  --> crates/drivers/input/ps2/src/lib.rs:57:15
   |
57 |     pub const IDENTIFY: u8 = 0xF2;
   |               ^^^^^^^^

warning: constant `SET_TYPEMATIC` is never used
  --> crates/drivers/input/ps2/src/lib.rs:58:15
   |
58 |     pub const SET_TYPEMATIC: u8 = 0xF3;
   |               ^^^^^^^^^^^^^

warning: constant `ENABLE_SCANNING` is never used
  --> crates/drivers/input/ps2/src/lib.rs:59:15
   |
59 |     pub const ENABLE_SCANNING: u8 = 0xF4;
   |               ^^^^^^^^^^^^^^^

warning: constant `DISABLE_SCANNING` is never used
  --> crates/drivers/input/ps2/src/lib.rs:60:15
   |
60 |     pub const DISABLE_SCANNING: u8 = 0xF5;
   |               ^^^^^^^^^^^^^^^^

warning: constant `SET_DEFAULTS` is never used
  --> crates/drivers/input/ps2/src/lib.rs:61:15
   |
61 |     pub const SET_DEFAULTS: u8 = 0xF6;
   |               ^^^^^^^^^^^^

warning: constant `RESEND` is never used
  --> crates/drivers/input/ps2/src/lib.rs:62:15
   |
62 |     pub const RESEND: u8 = 0xFE;
   |               ^^^^^^

warning: constant `RESET` is never used
  --> crates/drivers/input/ps2/src/lib.rs:63:15
   |
63 |     pub const RESET: u8 = 0xFF;
   |               ^^^^^

warning: constant `SET_SCALING_1_1` is never used
  --> crates/drivers/input/ps2/src/lib.rs:68:15
   |
68 |     pub const SET_SCALING_1_1: u8 = 0xE6;
   |               ^^^^^^^^^^^^^^^

warning: constant `SET_SCALING_2_1` is never used
  --> crates/drivers/input/ps2/src/lib.rs:69:15
   |
69 |     pub const SET_SCALING_2_1: u8 = 0xE7;
   |               ^^^^^^^^^^^^^^^

warning: constant `SET_RESOLUTION` is never used
  --> crates/drivers/input/ps2/src/lib.rs:70:15
   |
70 |     pub const SET_RESOLUTION: u8 = 0xE8;
   |               ^^^^^^^^^^^^^^

warning: constant `STATUS_REQUEST` is never used
  --> crates/drivers/input/ps2/src/lib.rs:71:15
   |
71 |     pub const STATUS_REQUEST: u8 = 0xE9;
   |               ^^^^^^^^^^^^^^

warning: constant `SET_STREAM_MODE` is never used
  --> crates/drivers/input/ps2/src/lib.rs:72:15
   |
72 |     pub const SET_STREAM_MODE: u8 = 0xEA;
   |               ^^^^^^^^^^^^^^^

warning: constant `READ_DATA` is never used
  --> crates/drivers/input/ps2/src/lib.rs:73:15
   |
73 |     pub const READ_DATA: u8 = 0xEB;
   |               ^^^^^^^^^

warning: constant `SET_REMOTE_MODE` is never used
  --> crates/drivers/input/ps2/src/lib.rs:74:15
   |
74 |     pub const SET_REMOTE_MODE: u8 = 0xF0;
   |               ^^^^^^^^^^^^^^^

warning: constant `DISABLE_DATA` is never used
  --> crates/drivers/input/ps2/src/lib.rs:78:15
   |
78 |     pub const DISABLE_DATA: u8 = 0xF5;
   |               ^^^^^^^^^^^^

warning: constant `SET_DEFAULTS` is never used
  --> crates/drivers/input/ps2/src/lib.rs:79:15
   |
79 |     pub const SET_DEFAULTS: u8 = 0xF6;
   |               ^^^^^^^^^^^^

warning: constant `RESEND` is never used
  --> crates/drivers/input/ps2/src/lib.rs:80:15
   |
80 |     pub const RESEND: u8 = 0xFE;
   |               ^^^^^^

   Compiling net v0.1.0 (/home/nd/repos/Projects/oxide_os/crates/net/net)
   Compiling fb v0.1.0 (/home/nd/repos/Projects/oxide_os/crates/graphics/fb)
   Compiling crypto v0.1.0 (/home/nd/repos/Projects/oxide_os/crates/security/crypto)
warning: function `mode_to_vtype` is never used
  --> crates/vfs/initramfs/src/lib.rs:30:4
   |
30 | fn mode_to_vtype(mode: u32) -> VnodeType {
   |    ^^^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

   Compiling mm-heap v0.1.0 (/home/nd/repos/Projects/oxide_os/crates/mm/mm-heap)
warning: `ps2` (lib) generated 31 warnings
warning: `initramfs` (lib) generated 1 warning
warning: fields `session` and `master_to_slave` are never read
  --> crates/tty/pty/src/lib.rs:75:5
   |
67 | struct PtyPair {
   |        ------- fields in this struct
...
75 |     session: i32,
   |     ^^^^^^^
76 |     /// Data from master to slave (input to slave)
77 |     master_to_slave: Vec<u8>,
   |     ^^^^^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `pty` (lib) generated 1 warning
warning: constant `OFFSET_CR3` is never used
  --> crates/arch/arch-x86_64/src/ap_boot.rs:16:7
   |
16 | const OFFSET_CR3: usize = 0; // Will be calculated from symbol
   |       ^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: constant `OFFSET_STACK` is never used
  --> crates/arch/arch-x86_64/src/ap_boot.rs:17:7
   |
17 | const OFFSET_STACK: usize = 8;
   |       ^^^^^^^^^^^^

warning: constant `OFFSET_ENTRY` is never used
  --> crates/arch/arch-x86_64/src/ap_boot.rs:18:7
   |
18 | const OFFSET_ENTRY: usize = 16;
   |       ^^^^^^^^^^^^

warning: constant `LVT_LINT0` is never used
  --> crates/arch/arch-x86_64/src/apic.rs:22:15
   |
22 |     pub const LVT_LINT0: u32 = 0x350; // LVT LINT0
   |               ^^^^^^^^^

warning: constant `LVT_LINT1` is never used
  --> crates/arch/arch-x86_64/src/apic.rs:23:15
   |
23 |     pub const LVT_LINT1: u32 = 0x360; // LVT LINT1
   |               ^^^^^^^^^

warning: constant `LVT_ERROR` is never used
  --> crates/arch/arch-x86_64/src/apic.rs:24:15
   |
24 |     pub const LVT_ERROR: u32 = 0x370; // LVT Error
   |               ^^^^^^^^^

warning: constant `ID` is never used
   --> crates/arch/arch-x86_64/src/apic.rs:307:15
    |
307 |     pub const ID: u32 = 0x00;
    |               ^^

warning: `arch-x86_64` (lib) generated 7 warnings
warning: methods `pml4` and `pml4_mut` are never used
  --> crates/mm/mm-paging/src/mapper.rs:44:8
   |
28 | impl PageMapper {
   | --------------- methods in this implementation
...
44 |     fn pml4(&self) -> &PageTable {
   |        ^^^^
...
50 |     fn pml4_mut(&mut self) -> &mut PageTable {
   |        ^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `mm-paging` (lib) generated 1 warning
warning: unused import: `AtomicI32`
  --> crates/proc/proc/src/process.rs:14:26
   |
14 | use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};
   |                          ^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default

warning: unused variable: `options`
  --> crates/proc/proc/src/wait.rs:81:5
   |
81 |     options: WaitOptions,
   |     ^^^^^^^ help: if this is intentional, prefix it with an underscore: `_options`
   |
   = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: function `futex_clear_and_wake` is never used
   --> crates/proc/proc/src/futex.rs:163:8
    |
163 | pub fn futex_clear_and_wake(addr: u64) {
    |        ^^^^^^^^^^^^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: constant `FUTEX_WAIT` is never used
  --> crates/proc/proc/src/futex.rs:32:15
   |
32 |     pub const FUTEX_WAIT: i32 = 0;
   |               ^^^^^^^^^^

warning: constant `FUTEX_WAKE` is never used
  --> crates/proc/proc/src/futex.rs:33:15
   |
33 |     pub const FUTEX_WAKE: i32 = 1;
   |               ^^^^^^^^^^

warning: constant `FUTEX_WAIT_PRIVATE` is never used
  --> crates/proc/proc/src/futex.rs:34:15
   |
34 |     pub const FUTEX_WAIT_PRIVATE: i32 = 128;
   |               ^^^^^^^^^^^^^^^^^^

warning: constant `FUTEX_WAKE_PRIVATE` is never used
  --> crates/proc/proc/src/futex.rs:35:15
   |
35 |     pub const FUTEX_WAKE_PRIVATE: i32 = 129;
   |               ^^^^^^^^^^^^^^^^^^

warning: `proc` (lib) generated 7 warnings (run `cargo fix --lib -p proc` to apply 2 suggestions)
warning: value assigned to `pid_len` is never read
   --> crates/vfs/procfs/src/lib.rs:240:27
    |
240 |         let mut pid_len = 0;
    |                           ^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

warning: unused variable: `used_kb`
   --> crates/vfs/procfs/src/lib.rs:820:13
    |
820 |         let used_kb = total_kb.saturating_sub(free_kb);
    |             ^^^^^^^ help: if this is intentional, prefix it with an underscore: `_used_kb`
    |
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: field `pid` is never read
   --> crates/vfs/procfs/src/lib.rs:656:5
    |
655 | pub struct ProcPidExe {
    |            ---------- field in this struct
656 |     pid: Pid,
    |     ^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: field `pid` is never read
   --> crates/vfs/procfs/src/lib.rs:733:5
    |
732 | pub struct ProcPidCwd {
    |            ---------- field in this struct
733 |     pid: Pid,
    |     ^^^

warning: `procfs` (lib) generated 4 warnings (run `cargo fix --lib -p procfs` to apply 1 suggestion)
warning: value assigned to `param_count` is never read
   --> crates/graphics/fb/src/console.rs:452:25
    |
452 |                         param_count += 1;
    |                         ^^^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default

   Compiling tcpip v0.1.0 (/home/nd/repos/Projects/oxide_os/crates/net/tcpip)
   Compiling virtio-net v0.1.0 (/home/nd/repos/Projects/oxide_os/crates/drivers/net/virtio-net)
   Compiling terminal v0.1.0 (/home/nd/repos/Projects/oxide_os/crates/terminal)
warning: struct `GePrecomp` is never constructed
   --> crates/security/crypto/src/ed25519.rs:720:8
    |
720 | struct GePrecomp {
    |        ^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: associated function `zero` is never used
   --> crates/security/crypto/src/ed25519.rs:763:8
    |
762 | impl GeP2 {
    | --------- associated function in this implementation
763 |     fn zero() -> Self {
    |        ^^^^

warning: method `to_p2` is never used
   --> crates/security/crypto/src/ed25519.rs:792:8
    |
791 | impl GeP1P1 {
    | ----------- method in this implementation
792 |     fn to_p2(&self) -> GeP2 {
    |        ^^^^^

warning: function `ge_sub_cached` is never used
   --> crates/security/crypto/src/ed25519.rs:832:4
    |
832 | fn ge_sub_cached(p: &GeP3, q: &GeCached) -> GeP1P1 {
    |    ^^^^^^^^^^^^^

warning: fields `rx_idx` and `tx_idx` are never read
   --> crates/drivers/net/virtio-net/src/lib.rs:177:5
    |
159 | pub struct VirtioNet {
    |            --------- fields in this struct
...
177 |     rx_idx: AtomicU32,
    |     ^^^^^^
178 |     /// TX virtqueue index
179 |     tx_idx: AtomicU32,
    |     ^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: multiple associated constants are never used
   --> crates/drivers/net/virtio-net/src/lib.rs:189:11
    |
184 | impl VirtioNet {
    | -------------- associated constants in this implementation
...
189 |     const MMIO_VENDOR_ID: usize = 0x00C;
    |           ^^^^^^^^^^^^^^
...
194 |     const MMIO_QUEUE_SEL: usize = 0x030;
    |           ^^^^^^^^^^^^^^
195 |     const MMIO_QUEUE_NUM_MAX: usize = 0x034;
    |           ^^^^^^^^^^^^^^^^^^
196 |     const MMIO_QUEUE_NUM: usize = 0x038;
    |           ^^^^^^^^^^^^^^
197 |     const MMIO_QUEUE_READY: usize = 0x044;
    |           ^^^^^^^^^^^^^^^^
198 |     const MMIO_QUEUE_NOTIFY: usize = 0x050;
199 |     const MMIO_INTERRUPT_STATUS: usize = 0x060;
    |           ^^^^^^^^^^^^^^^^^^^^^
200 |     const MMIO_INTERRUPT_ACK: usize = 0x064;
    |           ^^^^^^^^^^^^^^^^^^
201 |     const MMIO_STATUS: usize = 0x070;
202 |     const MMIO_QUEUE_DESC_LOW: usize = 0x080;
    |           ^^^^^^^^^^^^^^^^^^^
203 |     const MMIO_QUEUE_DESC_HIGH: usize = 0x084;
    |           ^^^^^^^^^^^^^^^^^^^^
204 |     const MMIO_QUEUE_AVAIL_LOW: usize = 0x090;
    |           ^^^^^^^^^^^^^^^^^^^^
205 |     const MMIO_QUEUE_AVAIL_HIGH: usize = 0x094;
    |           ^^^^^^^^^^^^^^^^^^^^^
206 |     const MMIO_QUEUE_USED_LOW: usize = 0x0A0;
    |           ^^^^^^^^^^^^^^^^^^^
207 |     const MMIO_QUEUE_USED_HIGH: usize = 0x0A4;
    |           ^^^^^^^^^^^^^^^^^^^^
...
213 |     const PCI_IO_QUEUE_ADDRESS: u16 = 0x08;
    |           ^^^^^^^^^^^^^^^^^^^^
214 |     const PCI_IO_QUEUE_SIZE: u16 = 0x0C;
    |           ^^^^^^^^^^^^^^^^^
215 |     const PCI_IO_QUEUE_SELECT: u16 = 0x0E;
    |           ^^^^^^^^^^^^^^^^^^^
...
218 |     const PCI_IO_ISR_STATUS: u16 = 0x13;
    |           ^^^^^^^^^^^^^^^^^

warning: unused doc comment
  --> crates/terminal/src/handler.rs:13:1
   |
13 | /// Terminal mode flags
   | ^^^^^^^^^^^^^^^^^^^^^^^ rustdoc does not generate documentation for macro invocations
   |
   = help: to document an item produced by a macro, the macro must produce the documentation as part of its expansion
   = note: `#[warn(unused_doc_comments)]` (part of `#[warn(unused)]`) on by default

warning: unused import: `NetworkDevice`
  --> crates/net/tcpip/src/lib.rs:26:56
   |
26 |     IpAddr, Ipv4Addr, MacAddress, NetError, NetResult, NetworkDevice, NetworkInterface,
   |                                                        ^^^^^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default

warning: `fb` (lib) generated 1 warning
warning: `virtio-net` (lib) generated 2 warnings
warning: unreachable pattern
   --> crates/net/tcpip/src/conntrack.rs:249:13
    |
242 |             (TcpState::Established, false, _, true, _) => TcpState::FinWait1,
    |             ------------------------------------------ matches all the relevant values
...
249 |             (TcpState::Established, false, _, true, true) => TcpState::CloseWait,
    |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ no value can reach this
    |
    = note: `#[warn(unreachable_patterns)]` (part of `#[warn(unused)]`) on by default

warning: `crypto` (lib) generated 4 warnings
warning: field `rx_buffer` is never read
  --> crates/net/tcpip/src/lib.rs:60:5
   |
46 | pub struct TcpIpStack {
   |            ---------- field in this struct
...
60 |     rx_buffer: Mutex<Vec<u8>>,
   |     ^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: fields `snd_wnd`, `rcv_wnd`, `retransmit_queue`, and `last_activity` are never read
   --> crates/net/tcpip/src/tcp.rs:201:5
    |
181 | pub struct TcpConnection {
    |            ------------- fields in this struct
...
201 |     snd_wnd: AtomicU32,
    |     ^^^^^^^
202 |     /// Receive window
203 |     rcv_wnd: AtomicU32,
    |     ^^^^^^^
...
209 |     retransmit_queue: Mutex<Vec<(u32, Vec<u8>)>>,
    |     ^^^^^^^^^^^^^^^^
210 |     /// Last activity timestamp
211 |     last_activity: AtomicU64,
    |     ^^^^^^^^^^^^^

warning: associated function `seq_ge` is never used
   --> crates/net/tcpip/src/tcp.rs:482:8
    |
214 | impl TcpConnection {
    | ------------------ associated function in this implementation
...
482 |     fn seq_ge(a: u32, b: u32) -> bool {
    |        ^^^^^^

warning: module `TcpFlags` should have a snake case name
  --> crates/net/tcpip/src/tcp.rs:23:9
   |
23 | pub mod TcpFlags {
   |         ^^^^^^^^ help: convert the identifier to snake case: `tcp_flags`
   |
   = note: `#[warn(non_snake_case)]` (part of `#[warn(nonstandard_style)]`) on by default

warning: field `back_buffer` is never read
  --> crates/terminal/src/renderer.rs:78:5
   |
68 | pub struct Renderer {
   |            -------- field in this struct
...
78 |     back_buffer: Option<Vec<u8>>,
   |     ^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

   Compiling dhcp v0.1.0 (/home/nd/repos/Projects/oxide_os/crates/net/dhcp)
   Compiling syscall v0.1.0 (/home/nd/repos/Projects/oxide_os/crates/syscall/syscall)
   Compiling devfs v0.1.0 (/home/nd/repos/Projects/oxide_os/crates/vfs/devfs)
warning: unused import: `get_policy`
  --> crates/syscall/syscall/src/firewall.rs:17:49
   |
17 |     connection_count, delete_rule, flush_chain, get_policy, rule_count, set_policy, with_rules,
   |                                                 ^^^^^^^^^^
   |
   = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default

warning: unused import: `crate::copy_to_user`
 --> crates/syscall/syscall/src/memory.rs:5:5
  |
5 | use crate::copy_to_user;
  |     ^^^^^^^^^^^^^^^^^^^

warning: unused import: `core::ptr::addr_of`
    --> crates/syscall/syscall/src/lib.rs:1498:9
     |
1498 |     use core::ptr::addr_of;
     |         ^^^^^^^^^^^^^^^^^^

warning: struct `BufWriter` is never constructed
   --> crates/vfs/devfs/src/devices.rs:116:8
    |
116 | struct BufWriter<'a> {
    |        ^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: associated items `new` and `as_slice` are never used
   --> crates/vfs/devfs/src/devices.rs:122:8
    |
121 | impl<'a> BufWriter<'a> {
    | ---------------------- associated items in this implementation
122 |     fn new(buf: &'a mut [u8]) -> Self {
    |        ^^^
...
126 |     fn as_slice(&self) -> &[u8] {
    |        ^^^^^^^^

warning: `terminal` (lib) generated 2 warnings
warning: `tcpip` (lib) generated 6 warnings
warning: unused variable: `listener_fd`
   --> crates/syscall/syscall/src/socket.rs:474:10
    |
474 |     let (listener_fd, listener_socket) = match listener {
    |          ^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_listener_fd`
    |
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: unused variable: `flags`
   --> crates/syscall/syscall/src/socket.rs:697:48
    |
697 | pub fn sys_recv(fd: i32, buf: u64, len: usize, flags: i32) -> i64 {
    |                                                ^^^^^ help: if this is intentional, prefix it with an underscore: `_flags`

warning: unreachable pattern
   --> crates/syscall/syscall/src/lib.rs:434:9
    |
336 |         nr::CLONE => sys_clone(arg1 as u32, arg2, arg3, arg4, arg5),
    |         --------- matches all the relevant values
...
434 |         nr::SIGRETURN => signal::sys_sigreturn(),
    |         ^^^^^^^^^^^^^ no value can reach this
    |
    = note: `#[warn(unreachable_patterns)]` (part of `#[warn(unused)]`) on by default

warning: unreachable pattern
   --> crates/syscall/syscall/src/lib.rs:487:9
    |
338 |         nr::FUTEX => sys_futex(arg1, arg2 as i32, arg3 as u32, arg4, arg5, arg6 as u32),
    |         --------- matches all the relevant values
...
487 |         nr::FW_LIST_RULES => firewall::sys_fw_list_rules(VirtAddr::new(arg1), arg2 as usize),
    |         ^^^^^^^^^^^^^^^^^ no value can reach this

warning: `devfs` (lib) generated 2 warnings
warning[E0133]: use of inline assembly is unsafe and requires unsafe block
   --> crates/syscall/syscall/src/lib.rs:689:13
    |
689 | /             core::arch::asm!(
690 | |                 "stac",                    // Enable user page access
691 | |                 "mov al, byte ptr [rdi]",  // Read current value
692 | |                 "mov byte ptr [rdi], al",  // Write it back (triggers COW)
...   |
696 | |                 options(nostack)
697 | |             );
    | |_____________^ use of inline assembly
    |
    = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html>
    = note: inline assembly is entirely unchecked and can cause undefined behavior
note: an unsafe function restricts its caller, but its body is safe by default
   --> crates/syscall/syscall/src/lib.rs:669:1
    |
669 | unsafe fn prefault_pages(user_ptr: u64, len: usize) {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    = note: `#[warn(unsafe_op_in_unsafe_fn)]` (part of `#[warn(rust_2024_compatibility)]`) on by default

warning[E0133]: call to unsafe function `prefault_pages` is unsafe and requires unsafe block
   --> crates/syscall/syscall/src/lib.rs:730:5
    |
730 |     prefault_pages(user_ptr, len);
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ call to unsafe function
    |
    = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html>
    = note: consult the function's documentation for information on how to avoid undefined behavior
note: an unsafe function restricts its caller, but its body is safe by default
   --> crates/syscall/syscall/src/lib.rs:712:1
    |
712 | pub(crate) unsafe fn copy_to_user(user_ptr: u64, kernel_data: &[u8]) -> bool {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning[E0133]: use of inline assembly is unsafe and requires unsafe block
   --> crates/syscall/syscall/src/lib.rs:738:9
    |
738 | /         core::arch::asm!(
739 | |             "stac",                                      // Enable user page access
740 | |             "mov rcx, {len}",                           // Length in RCX
741 | |             "mov rsi, {src}",                           // Source (kernel) in RSI
...   |
751 | |             options(nostack)
752 | |         );
    | |_________^ use of inline assembly
    |
    = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html>
    = note: inline assembly is entirely unchecked and can cause undefined behavior

warning[E0133]: call to unsafe function `copy_to_user` is unsafe and requires unsafe block
   --> crates/syscall/syscall/src/lib.rs:768:5
    |
768 |     copy_to_user(user_ptr, &bytes)
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ call to unsafe function
    |
    = note: for more information, see <https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html>
    = note: consult the function's documentation for information on how to avoid undefined behavior
note: an unsafe function restricts its caller, but its body is safe by default
   --> crates/syscall/syscall/src/lib.rs:766:1
    |
766 | unsafe fn write_user_i32(user_ptr: u64, value: i32) -> bool {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning: unused variable: `tls`
    --> crates/syscall/syscall/src/lib.rs:1483:71
     |
1483 | fn sys_clone(flags: u32, stack: u64, parent_tid: u64, child_tid: u64, tls: u64) -> i64 {
     |                                                                       ^^^ help: if this is intentional, prefix it with an underscore: `_tls`

warning: unused variable: `flags`
    --> crates/syscall/syscall/src/lib.rs:2045:43
     |
2045 | fn sys_getrandom(buf: u64, buflen: usize, flags: u32) -> i64 {
     |                                           ^^^^^ help: if this is intentional, prefix it with an underscore: `_flags`

warning: constant `GRND_RANDOM` is never used
    --> crates/syscall/syscall/src/lib.rs:2029:15
     |
2029 |     pub const GRND_RANDOM: u32 = 0x0002;
     |               ^^^^^^^^^^^
     |
     = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: constant `GRND_NONBLOCK` is never used
    --> crates/syscall/syscall/src/lib.rs:2031:15
     |
2031 |     pub const GRND_NONBLOCK: u32 = 0x0001;
     |               ^^^^^^^^^^^^^

warning: constant `GRND_INSECURE` is never used
    --> crates/syscall/syscall/src/lib.rs:2033:15
     |
2033 |     pub const GRND_INSECURE: u32 = 0x0004;
     |               ^^^^^^^^^^^^^

For more information about this error, try `rustc --explain E0133`.
warning: `syscall` (lib) generated 16 warnings (run `cargo fix --lib -p syscall` to apply 10 suggestions)
   Compiling kernel v0.1.0 (/home/nd/repos/Projects/oxide_os/kernel)
warning: struct `InterruptMutex` is never constructed
  --> kernel/src/globals.rs:38:12
   |
38 | pub struct InterruptMutex<T> {
   |            ^^^^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: associated items `new` and `lock` are never used
  --> kernel/src/globals.rs:44:18
   |
42 | impl<T> InterruptMutex<T> {
   | ------------------------- associated items in this implementation
43 |     /// Create a new interrupt-safe mutex
44 |     pub const fn new(value: T) -> Self {
   |                  ^^^
...
51 |     pub fn lock(&self) -> InterruptMutexGuard<'_, T> {
   |            ^^^^

warning: struct `InterruptMutexGuard` is never constructed
  --> kernel/src/globals.rs:64:12
   |
64 | pub struct InterruptMutexGuard<'a, T> {
   |            ^^^^^^^^^^^^^^^^^^^

warning: function `block_current` is never used
   --> kernel/src/scheduler.rs:290:8
    |
290 | pub fn block_current(state: TaskState) {
    |        ^^^^^^^^^^^^^

warning: function `set_scheduler` is never used
   --> kernel/src/scheduler.rs:431:8
    |
431 | pub fn set_scheduler(pid: u32, policy: SchedPolicy, priority: u8) {
    |        ^^^^^^^^^^^^^

warning: function `get_scheduler` is never used
   --> kernel/src/scheduler.rs:436:8
    |
436 | pub fn get_scheduler(pid: u32) -> Option<(SchedPolicy, u8)> {
    |        ^^^^^^^^^^^^^

warning: function `set_nice` is never used
   --> kernel/src/scheduler.rs:441:8
    |
441 | pub fn set_nice(pid: u32, nice: i8) {
    |        ^^^^^^^^

warning: function `get_nice` is never used
   --> kernel/src/scheduler.rs:446:8
    |
446 | pub fn get_nice(pid: u32) -> Option<i8> {
    |        ^^^^^^^^

warning: function `set_affinity` is never used
   --> kernel/src/scheduler.rs:451:8
    |
451 | pub fn set_affinity(pid: u32, cpuset: sched::CpuSet) {
    |        ^^^^^^^^^^^^

warning: function `get_affinity` is never used
   --> kernel/src/scheduler.rs:456:8
    |
456 | pub fn get_affinity(pid: u32) -> Option<sched::CpuSet> {
    |        ^^^^^^^^^^^^

warning: direct cast of function item into an integer
   --> kernel/src/init.rs:267:46
    |
267 |                 arch::ap_boot::ap_entry_rust as u64,
    |                                              ^^^^^^
    |
    = note: `#[warn(function_casts_as_integer)]` on by default
help: first cast to a pointer `as *const ()`
    |
267 |                 arch::ap_boot::ap_entry_rust as *const () as u64,
    |                                              ++++++++++++
