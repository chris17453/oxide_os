# GDB initialization script for OXIDE OS kernel debugging
# — ColdCipher: Pre-configured breakpoints and helpers because setting them manually
# every goddamn time is a special kind of hell.

# Connect to QEMU
target remote :1234

# Set architecture
set architecture i386:x86-64:intel

# Enable pretty printing
set print pretty on
set print array on
set print array-indexes on

# Disable pagination for autonomous debugging
set pagination off
set height 0
set width 0

# Display settings for better stack traces
set backtrace limit 50

# Kernel-specific breakpoints (uncomment as needed)

# Break on panic
# break rust_begin_unwind

# Break on page faults
# break handle_page_fault

# Break on general protection faults
# break handle_general_protection_fault

# Break on syscall entry
# break syscall_handler

# Break on scheduler
# break schedule

# Custom commands for common debug tasks

define dump-state
    info registers
    bt
    info threads
end

define dump-panic
    bt
    info registers
    x/32i $rip-32
    thread apply all bt
end

define dump-task
    # Dump current task info (adjust based on your Task struct layout)
    info threads
    bt
end

# Print helpful message
printf "OXIDE OS GDB initialized\n"
printf "Custom commands:\n"
printf "  dump-state  - Dump registers and backtrace\n"
printf "  dump-panic  - Full panic analysis\n"
printf "  dump-task   - Dump task info\n"
printf "\n"
