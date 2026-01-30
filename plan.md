checking for strings.h... (cached) no
checking for minix/config.h... no
checking whether it is safe to define __EXTENSIONS__... yes
checking whether _XOPEN_SOURCE should be defined... no
checking for the platform triplet based on compiler characteristics... x86_64-linux-gnu
checking for multiarch... 
checking for PEP 11 support tier... x86_64-unknown-linux-gnu/clang has tier 2 (supported)
checking for -Wl,--no-as-needed... yes
checking for the Android API level... not Android
checking for --with-emscripten-target... 
checking for --enable-wasm-dynamic-linking... missing
checking for --enable-wasm-pthreads... missing
checking for --with-suffix... 
checking for case-insensitive build directory... no
checking LIBRARY... libpython$(VERSION)$(ABIFLAGS).a
checking LINKCC... $(PURIFY) $(CC)
checking EXPORTSYMS... 
checking for GNU ld... Error: no input files
no
checking for --enable-shared... no
checking for --with-static-libpython... yes
checking for --enable-profiling... no
checking LDLIBRARY... checking HOSTRUNNER... 
libpython$(VERSION)$(ABIFLAGS).a
checking for x86_64-unknown-linux-gnu-ar... oxide-ar
checking for a BSD-compatible install... /usr/bin/install -c
checking for a race-free mkdir -p... /usr/bin/mkdir -p
checking for --with-pydebug... no
checking for --with-trace-refs... no
checking for --enable-pystats... no
checking for --with-assertions... no
checking for --enable-optimizations... no
checking PROFILE_TASK... -m test --pgo --timeout=$(TESTTIMEOUT)
checking for --with-lto... no
checking for x86_64-unknown-linux-gnu-llvm-profdata... no
checking for llvm-profdata... /usr/bin/llvm-profdata
checking for --enable-bolt... no
checking BOLT_INSTRUMENT_FLAGS... 
checking BOLT_APPLY_FLAGS...  -update-debug-sections -reorder-blocks=ext-tsp -reorder-functions=hfsort+ -split-functions -icf=1 -inline-all -split-eh -reorder-functions-use-hot-size -peepholes=none -jump-tables=aggressive -inline-ap -indirect-call-promotion=all -dyno-stats -use-gnu-stack -frame-opt=hot 
checking if oxide-cc supports -fstrict-overflow and -fno-strict-overflow... yes
checking for --with-strict-overflow... no
checking if oxide-cc supports -Og optimization level... yes
checking if we can add -Wextra... yes
checking whether oxide-cc -fno-strict-aliasing accepts and needs -fno-strict-aliasing... no
checking if we can disable oxide-cc unused-parameter warning... yes
checking if we can disable oxide-cc int-conversion warning... yes
checking if we can disable oxide-cc missing-field-initializers warning... yes
checking if we can enable oxide-cc sign-compare warning... yes
checking if we can enable oxide-cc unreachable-code warning... yes
checking if we can enable oxide-cc strict-prototypes warning... yes
checking if we can make implicit function declaration an error in oxide-cc -Werror=implicit-function-declaration... yes
checking if we can use visibility in oxide-cc -fvisibility=hidden... yes
checking whether pthreads are available without options... no
checking whether oxide-cc accepts -Kpthread... no
checking whether oxide-cc accepts -Kthread... no
checking whether oxide-cc accepts -pthread... (cached) yes
checking whether oxide-c++ also accepts flags for thread support... no
checking for alloca.h... no
checking for asm/types.h... no
checking for bluetooth.h... no
checking for conio.h... (cached) no
checking for crypt.h... no
checking for direct.h... (cached) no
checking for dlfcn.h... (cached) yes
checking for endian.h... yes
checking for errno.h... (cached) yes
checking for fcntl.h... (cached) yes
checking for grp.h... (cached) yes
checking for ieeefp.h... no
checking for io.h... (cached) no
checking for langinfo.h... (cached) no
checking for libintl.h... (cached) no
checking for libutil.h... no
checking for linux/auxvec.h... no
checking for sys/auxv.h... no
checking for linux/fs.h... no
checking for linux/limits.h... no
checking for linux/memfd.h... no
checking for linux/random.h... (cached) yes
checking for linux/soundcard.h... no
checking for linux/tipc.h... (cached) no
checking for linux/wait.h... no
checking for netdb.h... (cached) no
checking for net/ethernet.h... no
checking for netinet/in.h... (cached) no
checking for netpacket/packet.h... (cached) no
checking for poll.h... (cached) no
checking for process.h... (cached) no
checking for pthread.h... (cached) yes
checking for pty.h... (cached) no
checking for sched.h... no
checking for setjmp.h... (cached) yes
checking for shadow.h... no
checking for signal.h... (cached) yes
checking for spawn.h... (cached) no
checking for stropts.h... no
checking for sys/audioio.h... no
checking for sys/bsdtty.h... no
checking for sys/devpoll.h... no
checking for sys/endian.h... no
checking for sys/epoll.h... (cached) no
checking for sys/event.h... (cached) no
checking for sys/eventfd.h... no
checking for sys/file.h... (cached) no
checking for sys/ioctl.h... (cached) yes
checking for sys/kern_control.h... (cached) no
checking for sys/loadavg.h... (cached) no
checking for sys/lock.h... (cached) no
checking for sys/memfd.h... no
checking for sys/mkdev.h... (cached) no
checking for sys/mman.h... (cached) yes
checking for sys/modem.h... (cached) no
checking for sys/param.h... (cached) no
checking for sys/poll.h... no
checking for sys/random.h... (cached) yes
checking for sys/resource.h... (cached) no
checking for sys/select.h... (cached) yes
checking for sys/sendfile.h... (cached) no
checking for sys/socket.h... (cached) no
checking for sys/soundcard.h... no
checking for sys/stat.h... (cached) yes
checking for sys/statvfs.h... (cached) no
checking for sys/sys_domain.h... no
checking for sys/syscall.h... yes
checking for sys/sysmacros.h... (cached) no
checking for sys/termio.h... no
checking for sys/time.h... (cached) yes
checking for sys/times.h... (cached) yes
checking for sys/types.h... (cached) yes
checking for sys/uio.h... (cached) no
checking for sys/un.h... (cached) no
checking for sys/utsname.h... (cached) yes
checking for sys/wait.h... (cached) yes
checking for sys/xattr.h... no
checking for sysexits.h... (cached) no
checking for syslog.h... (cached) yes
checking for termios.h... (cached) yes
checking for util.h... (cached) no
checking for utime.h... yes
checking for utmp.h... (cached) no
checking for dirent.h that defines DIR... yes
checking for library containing opendir... none required
checking for sys/mkdev.h... (cached) no
checking for sys/sysmacros.h... (cached) no
checking for bluetooth/bluetooth.h... no
checking for net/if.h... no
checking for linux/netlink.h... no
checking for linux/qrtr.h... no
checking for linux/vm_sockets.h... (cached) no
checking for linux/can.h... (cached) no
checking for linux/can/bcm.h... no
checking for linux/can/j1939.h... no
checking for linux/can/raw.h... no
checking for netcan/can.h... no
checking for clock_t in time.h... yes
checking for makedev... (cached) no
checking for le64toh... yes
checking for mode_t... (cached) yes
checking for off_t... (cached) yes
checking for pid_t... (cached) yes
checking for size_t... yes
checking for uid_t in sys/types.h... (cached) yes
checking for ssize_t... (cached) yes
checking for __uint128_t... yes
checking size of int... (cached) 4
checking size of long... (cached) 8
checking alignment of long... (cached) 8
checking size of long long... (cached) 8
checking size of void *... (cached) 8
checking size of short... (cached) 2
checking size of float... (cached) 4
checking size of double... (cached) 8
checking size of fpos_t... (cached) 8
checking size of size_t... (cached) 8
checking alignment of size_t... (cached) 8
checking size of pid_t... (cached) 4
checking size of uintptr_t... (cached) 8
checking alignment of max_align_t... (cached) 16
checking for long double... yes
checking size of long double... 16
checking size of _Bool... (cached) 1
checking size of off_t... (cached) 8
checking whether to enable large file support... no
checking size of time_t... (cached) 8
checking for pthread_t... yes
checking size of pthread_t... 8
checking size of pthread_key_t... 4
checking whether pthread_key_t is compatible with int... yes
checking for --enable-framework... no
checking for --with-dsymutil... no
checking for dyld... no
checking for --with-address-sanitizer... no
checking for --with-memory-sanitizer... no
checking for --with-undefined-behavior-sanitizer... no
checking for --with-thread-sanitizer... no
checking the extension of shared libraries... .so
checking LDSHARED... $(CC) -shared
checking BLDSHARED flags... $(CC) -shared
checking CCSHARED... -fPIC
checking LINKFORSHARED... -Xlinker -export-dynamic
checking CFLAGSFORSHARED... 
checking SHLIBS... $(LIBS)
checking perf trampoline... yes
checking for sendfile in -lsendfile... no
checking for dlopen in -ldl... (cached) no
checking for shl_load in -ldld... no
checking for uuid.h... no
checking for uuid >= 2.20... no
checking for uuid/uuid.h... no
checking for uuid/uuid.h... (cached) no
checking for library containing sem_init... no
checking for textdomain in -lintl... no
checking aligned memory access is required... yes
checking for --with-hash-algorithm... default
checking for --with-tzpath... "/usr/share/zoneinfo:/usr/lib/zoneinfo:/usr/share/lib/zoneinfo:/etc/zoneinfo"
checking for t_open in -lnsl... no
checking for socket in -lsocket... no
checking for --with-libs... no
checking for --with-system-expat... no
checking for libffi... yes
checking for ffi_prep_cif_var... no
checking for ffi_prep_closure_loc... no
checking for ffi_closure_alloc... no
checking for --with-system-libmpdec... no
checking for --with-decimal-contextvar... yes
checking for decimal libmpdec machine... uint128
checking for libnsl... no
checking for library containing yp_match... no
checking for sqlite3 >= 3.7.15... yes
checking for sqlite3.h... no
checking for --enable-loadable-sqlite-extensions... no
checking for gdbm.h... no
checking for ndbm.h... no
checking for ndbm presence and linker args...  ()
checking for gdbm/ndbm.h... no
checking for gdbm-ndbm.h... no
checking for db.h... no
checking for --with-dbmliborder... gdbm:ndbm:bdb
checking for _dbm module CFLAGS and LIBS...  
checking if PTHREAD_SCOPE_SYSTEM is supported... no
checking for pthread_sigmask... yes
checking for pthread_getcpuclockid... no
checking if --enable-ipv6 is specified... no
checking CAN_RAW_FD_FRAMES... no
checking for CAN_RAW_JOIN_FILTERS... no
checking for --with-doc-strings... yes
checking for --with-pymalloc... no
checking for --with-freelists... yes
checking for --with-c-locale-coercion... yes
checking for --with-valgrind... no
checking for --with-dtrace... no
checking for dlopen... (cached) no
checking DYNLOADFILE... dynload_stub.o
checking MACHDEP_OBJS... none
checking for accept4... (cached) no
checking for alarm... (cached) no
checking for bind_textdomain_codeset... no
checking for chmod... (cached) no
checking for chown... (cached) no
checking for clock... yes
checking for close_range... no
checking for confstr... (cached) no
checking for copy_file_range... (cached) no
checking for ctermid... (cached) no
checking for dup... (cached) yes
checking for dup3... no
checking for execv... (cached) no
checking for explicit_bzero... (cached) no
checking for explicit_memset... (cached) no
checking for faccessat... (cached) no
checking for fchmod... (cached) no
checking for fchmodat... (cached) no
checking for fchown... (cached) no
checking for fchownat... (cached) no
checking for fdopendir... (cached) no
checking for fdwalk... no
checking for fexecve... no
checking for fork... (cached) no
checking for fork1... (cached) no
checking for fpathconf... (cached) no
checking for fstatat... yes
checking for ftime... no
checking for ftruncate... (cached) yes
checking for futimens... (cached) no
checking for futimes... (cached) no
checking for futimesat... no
checking for gai_strerror... yes
checking for getegid... (cached) yes
checking for getentropy... (cached) no
checking for geteuid... (cached) yes
checking for getgid... (cached) yes
checking for getgrgid... yes
checking for getgrgid_r... yes
checking for getgrnam_r... yes
checking for getgrouplist... yes
checking for getgroups... (cached) no
checking for gethostname... (cached) no
checking for getitimer... (cached) no
checking for getloadavg... (cached) no
checking for getlogin... (cached) no
checking for getpeername... (cached) no
checking for getpgid... (cached) yes
checking for getpid... (cached) yes
checking for getppid... (cached) yes
checking for getpriority... (cached) no
checking for _getpty... no
checking for getpwent... no
checking for getpwnam_r... yes
checking for getpwuid... yes
checking for getpwuid_r... yes
checking for getresgid... no
checking for getresuid... no
checking for getrusage... (cached) no
checking for getsid... yes
checking for getspent... no
checking for getspnam... no
checking for getuid... (cached) yes
checking for getwd... no
checking for if_nameindex... no
checking for initgroups... (cached) no
checking for kill... no
checking for killpg... no
checking for lchown... (cached) no
checking for linkat... (cached) no
checking for lockf... no
checking for lstat... (cached) yes
checking for lutimes... (cached) no
checking for madvise... (cached) no
checking for mbrtowc... yes
checking for memrchr... yes
checking for mkdirat... (cached) no
checking for mkfifo... (cached) no
checking for mkfifoat... (cached) no
checking for mknod... (cached) no
checking for mknodat... (cached) no
checking for mktime... yes
checking for mmap... (cached) yes
checking for mremap... no
checking for nice... yes
checking for openat... (cached) no
checking for opendir... yes
checking for pathconf... (cached) no
checking for pause... (cached) no
checking for pipe... (cached) yes
checking for pipe2... no
checking for plock... no
checking for poll... (cached) no
checking for posix_fadvise... (cached) no
checking for posix_fallocate... (cached) no
checking for posix_spawn... (cached) no
checking for posix_spawnp... (cached) no
checking for pread... (cached) no
checking for preadv... (cached) no
checking for preadv2... (cached) no
checking for pthread_condattr_setclock... yes
checking for pthread_init... (cached) yes
checking for pthread_kill... no
checking for pwrite... (cached) no
checking for pwritev... (cached) no
checking for pwritev2... (cached) no
checking for readlink... (cached) no
checking for readlinkat... (cached) no
checking for readv... (cached) no
checking for realpath... (cached) no
checking for renameat... (cached) no
checking for rtpSpawn... no
checking for sched_get_priority_max... no
checking for sched_rr_get_interval... no
checking for sched_setaffinity... (cached) no
checking for sched_setparam... no
checking for sched_setscheduler... (cached) no
checking for sem_clockwait... no
checking for sem_getvalue... (cached) no
checking for sem_open... (cached) no
checking for sem_timedwait... (cached) no
checking for sem_unlink... (cached) no
checking for sendfile... (cached) no
checking for setegid... yes
checking for seteuid... yes
checking for setgid... yes
checking for sethostname... (cached) no
checking for setitimer... (cached) no
checking for setlocale... yes
checking for setpgid... (cached) yes
checking for setpgrp... no
checking for setpriority... (cached) no
checking for setregid... no
checking for setresgid... no
checking for setresuid... no
checking for setreuid... no
checking for setsid... (cached) yes
checking for setuid... yes
checking for setvbuf... yes
checking for shutdown... yes
checking for sigaction... (cached) no
checking for sigaltstack... no
checking for sigfillset... yes
checking for siginterrupt... (cached) no
checking for sigpending... (cached) no
checking for sigrelse... (cached) no
checking for sigtimedwait... (cached) no
checking for sigwait... (cached) no
checking for sigwaitinfo... (cached) no
checking for snprintf... (cached) yes
checking for splice... (cached) no
checking for strftime... yes
checking for strlcpy... (cached) yes
checking for strsignal... yes
checking for symlinkat... (cached) no
checking for sync... (cached) no
checking for sysconf... (cached) no
checking for system... (cached) no
checking for tcgetpgrp... (cached) yes
checking for tcsetpgrp... (cached) yes
checking for tempnam... no
checking for timegm... no
checking for times... (cached) no
checking for tmpfile... yes
checking for tmpnam... yes
checking for tmpnam_r... no
checking for truncate... (cached) no
checking for ttyname... (cached) no
checking for umask... (cached) no
checking for uname... yes
checking for unlinkat... (cached) no
checking for utimensat... (cached) no
checking for utimes... (cached) no
checking for vfork... (cached) no
checking for wait... yes
checking for wait3... (cached) no
checking for wait4... (cached) no
checking for waitid... (cached) no
checking for waitpid... yes
checking for wcscoll... yes
checking for wcsftime... no
checking for wcsxfrm... yes
checking for wmemcmp... yes
checking for writev... (cached) no
checking for oxide-cc -pthread options needed to detect all undeclared functions... none needed
checking whether dirfd is declared... yes
checking for chroot... yes
checking for link... (cached) no
checking for symlink... (cached) no
checking for fchdir... yes
checking for fsync... (cached) no
checking for fdatasync... (cached) no
checking for epoll_create... (cached) no
checking for epoll_create1... (cached) no
checking for kqueue... (cached) no
checking for prlimit... no
checking for _dyld_shared_cache_contains_path... no
checking for memfd_create... (cached) no
checking for eventfd... (cached) no
checking for ctermid_r... no
checking for flock declaration... yes
checking for flock... yes
checking for getpagesize... yes
checking for broken unsetenv... no
checking for true... true
checking for inet_aton in -lc... no
checking for inet_aton in -lresolv... no
checking for chflags... cross
checking for chflags... no
checking for lchflags... cross
checking for lchflags... no
checking for zlib >= 1.2.0... yes
checking for bzip2... yes
checking for liblzma... yes
checking for hstrerror... yes
checking for getservbyname... yes
checking for getservbyport... yes
checking for gethostbyname... yes
checking for gethostbyaddr... yes
checking for getprotobyname... yes
checking for inet_aton... (cached) no
checking for inet_ntoa... yes
checking for inet_pton... (cached) no
checking for getpeername... (cached) no
checking for getsockname... (cached) no
checking for accept... (cached) no
checking for bind... yes
checking for connect... yes
checking for listen... yes
checking for recvfrom... yes
checking for sendto... yes
checking for setsockopt... yes
checking for socket... yes
checking for setgroups... (cached) no
checking for openpty... (cached) no
checking for openpty in -lutil... no
checking for openpty in -lbsd... no
checking for library containing login_tty... no
checking for forkpty... (cached) no
checking for forkpty in -lutil... no
checking for forkpty in -lbsd... no
checking for fseek64... no
checking for fseeko... yes
checking for fstatvfs... (cached) no
checking for ftell64... no
checking for ftello... yes
checking for statvfs... (cached) no
checking for dup2... (cached) yes
checking for getpgrp... yes
checking for setpgrp... (cached) no
checking for setns... no
checking for unshare... no
checking for libxcrypt >= 3.1.1... yes
checking for crypt or crypt_r... no
checking for clock_gettime... (cached) yes
checking for clock_getres... yes
checking for clock_settime... no
checking for clock_settime in -lrt... no
checking for clock_nanosleep... no
checking for clock_nanosleep in -lrt... no
checking for nanosleep... (cached) yes
checking for major, minor, and makedev... no
checking for getaddrinfo... (cached) no
checking for getnameinfo... (cached) no
checking whether struct tm is in sys/time.h or time.h... time.h
checking for struct tm.tm_zone... yes
checking for struct stat.st_rdev... yes
checking for struct stat.st_blksize... yes
checking for struct stat.st_flags... no
checking for struct stat.st_gen... no
checking for struct stat.st_birthtime... no
checking for struct stat.st_blocks... yes
checking for struct passwd.pw_gecos... yes
checking for struct passwd.pw_passwd... yes
checking for siginfo_t.si_band... yes
checking for time.h that defines altzone... no
checking for addrinfo... yes
checking for sockaddr_storage... yes
checking for sockaddr_alg... no
checking for an ANSI C-conforming const... yes
checking for working signed char... yes
checking for prototypes... yes
checking for socketpair... (cached) no
checking if sockaddr has sa_len member... no
checking for gethostbyname_r... no
checking for gethostbyname... (cached) yes
checking for __fpu_control... no
checking for __fpu_control in -lieee... no
checking for --with-libm=STRING... default LIBM="-lm"
checking for --with-libc=STRING... default LIBC=""
checking for x64 gcc inline assembler... yes
checking whether float word ordering is bigendian... no
checking whether we can use gcc inline assembler to get and set x87 control word... yes
checking whether we can use gcc inline assembler to get and set mc68881 fpcr... no
checking for x87-style double rounding... no
checking for acosh... (cached) yes
checking for asinh... (cached) yes
checking for atanh... (cached) yes
checking for erf... (cached) yes
checking for erfc... (cached) yes
checking for expm1... (cached) yes
checking for log1p... (cached) yes
checking for log2... (cached) yes
checking whether POSIX semaphores are enabled... yes
checking for broken sem_getvalue... yes
checking whether RTLD_LAZY is declared... yes
checking whether RTLD_NOW is declared... yes
checking whether RTLD_GLOBAL is declared... yes
checking whether RTLD_LOCAL is declared... yes
checking whether RTLD_NODELETE is declared... yes
checking whether RTLD_NOLOAD is declared... yes
checking whether RTLD_DEEPBIND is declared... yes
checking whether RTLD_MEMBER is declared... no
checking digit size for Python's longs... no value specified
checking for wchar.h... (cached) yes
checking size of wchar_t... (cached) 4
checking whether wchar_t is signed... yes
checking whether wchar_t is usable... no
checking whether byte ordering is bigendian... (cached) no
checking ABIFLAGS... 
checking SOABI... cpython-312-x86_64-linux-gnu
checking LDVERSION... $(VERSION)$(ABIFLAGS)
checking for --with-platlibdir... no
checking for --with-wheel-pkg-dir... no
checking whether right shift extends the sign bit... yes
checking for getc_unlocked() and friends... no
checking for readline... no
checking for readline/readline.h... no
checking how to link readline... no
checking for broken nice()... no
checking for broken poll()... no
checking for working tzset()... no
checking for tv_nsec in struct stat... yes
checking for tv_nsec2 in struct stat... no
checking for curses.h... (cached) no
checking for ncurses.h... (cached) no
checking curses module flags... no
checking for panel.h... no
checking panel flags... no
checking for term.h... (cached) no
checking whether mvwdelch is an expression... no
checking whether WINDOW has _flags... no
checking for curses function is_pad... no
checking for curses function is_term_resized... no
checking for curses function resize_term... no
checking for curses function resizeterm... no
checking for curses function immedok... no
checking for curses function syncok... no
checking for curses function wchgat... no
checking for curses function filter... no
checking for curses function has_key... no
checking for curses function typeahead... no
checking for curses function use_env... no
configure: checking for device files
checking for /dev/ptmx... (cached) no
checking for /dev/ptc... (cached) no
checking for socklen_t... no
checking for broken mbstowcs... no
checking for --with-computed-gotos... no value specified
checking whether oxide-cc -pthread supports computed gotos... (cached) yes
checking for build directories... done
checking for -O2... yes
checking for glibc _FORTIFY_SOURCE/memmove bug... undefined
checking for stdatomic.h... no
checking for builtin __atomic_load_n and __atomic_store_n functions... yes
checking for ensurepip... no
checking if the dirent structure of a d_type field... yes
checking for the Linux getrandom() syscall... yes
checking for the getrandom() function... (cached) yes
checking for library containing shm_open... no
checking for shm_open... (cached) no
checking for shm_unlink... (cached) no
checking for x86_64-unknown-linux-gnu-pkg-config... /usr/bin/pkg-config
checking whether compiling and linking against OpenSSL works... no
checking for --with-openssl-rpath... 
checking whether OpenSSL provides required ssl module APIs... no
checking whether OpenSSL provides required hashlib module APIs... no
checking for --with-ssl-default-suites... python
checking for --with-builtin-hashlib-hashes... md5,sha1,sha2,sha3,blake2
checking for libb2... no
checking for --disable-test-modules... yes
checking for stdlib extension module _multiprocessing... missing
checking for stdlib extension module _posixshmem... missing
checking for stdlib extension module fcntl... yes
checking for stdlib extension module mmap... yes
checking for stdlib extension module _socket... missing
checking for stdlib extension module grp... yes
checking for stdlib extension module ossaudiodev... missing
checking for stdlib extension module pwd... yes
checking for stdlib extension module resource... missing
checking for stdlib extension module _scproxy... n/a
checking for stdlib extension module spwd... missing
checking for stdlib extension module syslog... yes
checking for stdlib extension module termios... yes
checking for stdlib extension module pyexpat... yes
checking for stdlib extension module _elementtree... yes
checking for stdlib extension module _md5... yes
checking for stdlib extension module _sha1... yes
checking for stdlib extension module _sha2... yes
checking for stdlib extension module _sha3... yes
checking for stdlib extension module _blake2... yes
checking for stdlib extension module _crypt... missing
checking for stdlib extension module _ctypes... yes
checking for stdlib extension module _curses... missing
checking for stdlib extension module _curses_panel... missing
checking for stdlib extension module _decimal... yes
checking for stdlib extension module _dbm... missing
checking for stdlib extension module _gdbm... missing
checking for stdlib extension module nis... missing
checking for stdlib extension module readline... missing
checking for stdlib extension module _sqlite3... disabled
checking for stdlib extension module _tkinter... missing
checking for stdlib extension module _uuid... missing
checking for stdlib extension module zlib... yes
checking for stdlib extension module _bz2... yes
checking for stdlib extension module _lzma... yes
checking for stdlib extension module _ssl... missing
checking for stdlib extension module _hashlib... missing
checking for stdlib extension module _testcapi... yes
checking for stdlib extension module _testclinic... yes
checking for stdlib extension module _testinternalcapi... yes
checking for stdlib extension module _testbuffer... yes
checking for stdlib extension module _testimportmultiple... missing
checking for stdlib extension module _testmultiphase... missing
checking for stdlib extension module xxsubtype... yes
checking for stdlib extension module _xxtestfuzz... yes
checking for stdlib extension module _ctypes_test... missing
checking for stdlib extension module xxlimited... missing
checking for stdlib extension module xxlimited_35... missing
