from pathlib import Path
functions = {line.strip() for line in Path('functions_list.txt').read_text().splitlines() if line.strip()}
cat = {}
add = lambda key, names: cat.setdefault(key, []).extend(names)
add('vfs_io', [
    'openat', 'close_range', 'copy_file_range', 'fdatasync', 'fsync', 'sync', 'readlink', 'readlinkat',
    'realpath', 'truncate', 'pathconf', 'readv', 'writev', 'pread', 'pwrite', 'preadv', 'pwritev',
    'preadv2', 'pwritev2', 'fseek64', 'ftell64', 'ftime', 'fstatvfs', 'statvfs', 'splice', 'sendfile', 'sendfile in -lsendfile'
])
add('vfs_metadata', [
    'faccessat', 'fchmod', 'fchmodat', 'fchown', 'fchownat', 'fdopendir', 'fdwalk', 'futimens', 'futimes', 'futimesat',
    'lutimes', 'utimensat', 'utimes', 'link', 'linkat', 'symlink', 'symlinkat', 'unlinkat', 'mkdirat', 'mkfifo', 'mkfifoat',
    'mknod', 'mknodat', 'lockf', 'plock', 'chflags', 'lchflags', 'chmod', 'chown', 'fpathconf', 'lchown', 'renameat'
])
add('device_nodes', ['major, minor, and makedev', 'makedev', 'umask'])
add('fd_event', ['dup3', 'pipe2', 'poll', 'epoll_create', 'epoll_create1', 'kqueue', 'eventfd', 'socketpair'])
add('memory', ['madvise', 'posix_fadvise', 'posix_fallocate', 'mremap', 'memfd_create', 'shm_open', 'shm_unlink'])
add('process_lifecycle', [
    'fork', 'fork1', 'vfork', 'wait3', 'wait4', 'waitid', 'execv', 'fexecve', 'posix_spawn', 'posix_spawnp', 'system', 'rtpSpawn', 'setpgrp'
])
add('scheduler', ['sched_get_priority_max', 'sched_rr_get_interval', 'sched_setaffinity', 'sched_setparam', 'sched_setscheduler'])
add('posix_semaphores', ['sem_clockwait', 'sem_getvalue', 'sem_open', 'sem_timedwait', 'sem_unlink'])
add('pthread_signals', ['pthread_getcpuclockid', 'pthread_kill'])
add('process_credentials', [
    'getgroups', 'setgroups', 'initgroups', 'getresgid', 'getresuid', 'setresgid', 'setresuid', 'setregid', 'setreuid',
    'getpwent', 'getspent', 'getspnam', 'getpriority', 'setpriority', 'prlimit'
])
add('process_misc', ['pause', 'times', 'getrusage', 'getloadavg', 'sysconf', 'getentropy'])
add('signals_timers', [
    'alarm', 'getitimer', 'setitimer', 'sigaction', 'sigaltstack', 'siginterrupt', 'sigpending', 'sigrelse', 'sigtimedwait',
    'sigwait', 'sigwaitinfo', 'clock_settime', 'clock_settime in -lrt', 'clock_nanosleep', 'clock_nanosleep in -lrt', 'working tzset()',
    'timegm', 'wcsftime', 'kill', 'killpg'
])
add('network', [
    'accept', 'accept4', 'getaddrinfo', 'getnameinfo', 'gethostbyname_r', 'inet_aton', 'inet_aton in -lc', 'inet_aton in -lresolv',
    'inet_pton', 'getpeername', 'getsockname', 'sockaddr_alg', 'socket in -lsocket', 'socklen_t', 'if_nameindex',
    'CAN_RAW_FD_FRAMES', 'CAN_RAW_JOIN_FILTERS', 'libnsl', 't_open in -lnsl'
])
add('pty_console', ['/dev/ptmx', '/dev/ptc', '_getpty', 'openpty', 'openpty in -lutil', 'openpty in -lbsd', 'forkpty', 'forkpty in -lutil', 'forkpty in -lbsd', 'ttyname', 'ctermid', 'ctermid_r'])
add('libc_misc', [
    'explicit_bzero', 'explicit_memset', 'getc_unlocked() and friends', 'getlogin', 'getwd', 'confstr', 'tempnam', 'tmpnam_r', 'case-insensitive build directory'
])
add('intl_locale', ['bind_textdomain_codeset', 'textdomain in -lintl'])
add('crypto_uuid', ['crypt or crypt_r', 'uuid >= 2.20', 'libb2'])
add('dynamic_linker', ['dlopen', 'dlopen in -ldl', 'shl_load in -ldld', 'dyld', '_dyld_shared_cache_contains_path'])
add('ffi_hooks', ['ffi_closure_alloc', 'ffi_prep_cif_var', 'ffi_prep_closure_loc'])
add('python_build', ['ensurepip', 'curses module flags', 'how to link readline', 'readline', 'panel flags'])
add('curses_funcs', ['curses function filter', 'curses function has_key', 'curses function immedok', 'curses function is_pad', 'curses function is_term_resized', 'curses function resize_term', 'curses function resizeterm', 'curses function syncok', 'curses function typeahead', 'curses function use_env', 'curses function wchgat'])
add('toolchain_abi', ['digit size for Python\'s longs', '__fpu_control', '__fpu_control in -lieee', 'x86_64-unknown-linux-gnu-llvm-profdata', 'x87-style double rounding', 'oxide-cc -pthread options needed to detect all undeclared functions'])
add('qa_checks', ['broken mbstowcs', 'broken nice()', 'broken poll()', 'broken unsetenv'])
add('security_ns', ['setns', 'unshare'])
add('hostname_misc', ['gethostname', 'sethostname'])
add('unused', [])
order = [
    'vfs_io', 'vfs_metadata', 'device_nodes', 'fd_event', 'memory', 'posix_semaphores',
    'process_lifecycle', 'scheduler', 'process_credentials', 'process_misc', 'security_ns',
    'signals_timers', 'pthread_signals', 'network', 'hostname_misc', 'pty_console',
    'libc_misc', 'intl_locale', 'crypto_uuid', 'dynamic_linker', 'ffi_hooks', 'python_build',
    'curses_funcs', 'toolchain_abi', 'qa_checks'
]
assigned = set()
for names in cat.values():
    for name in names:
        if name not in functions:
            raise SystemExit(f'Unknown name {name}')
        if name in assigned:
            raise SystemExit(f'Duplicate assignment of {name}')
        assigned.add(name)
missing = functions - assigned
if missing:
    raise SystemExit(f'Missing assignments for {len(missing)} items: {sorted(missing)[:20]}')
print('All functions assigned to categories:', len(assigned))
for key in order:
    names = cat.get(key)
    if not names:
        continue
    print(f"\n== {key} ==")
    print(', '.join(f'`{name}`' for name in sorted(names)))
