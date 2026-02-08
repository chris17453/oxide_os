# Crash capture script - runs until crash or hang, then dumps state
# — ColdCipher: Because crashes don't always happen when you're watching.

# Catch all exception handlers
catch signal SIGSEGV
catch signal SIGILL
catch signal SIGABRT

# Break on panic
break rust_begin_unwind
commands
    silent
    printf "\n=== PANIC DETECTED ===\n"
    bt
    printf "\n=== REGISTERS ===\n"
    info registers
    printf "\n=== ALL THREADS ===\n"
    info threads
    thread apply all bt
    printf "\n=== MEMORY AT RIP ===\n"
    x/32i $rip-32
    quit
end

# Break on exception handlers (x86_64 specific)
# Adjust these based on your exception handler names
break handle_page_fault
commands
    silent
    printf "\n=== PAGE FAULT ===\n"
    bt
    info registers
    # Don't quit - might be recoverable
    continue
end

break handle_general_protection_fault
commands
    silent
    printf "\n=== GPF ===\n"
    bt
    info registers
    quit
end

break handle_double_fault
commands
    silent
    printf "\n=== DOUBLE FAULT ===\n"
    bt
    info registers
    quit
end

printf "Crash capture armed. Continuing execution...\n"
continue

# If we get here without hitting a breakpoint, execution completed normally
printf "\n=== EXECUTION COMPLETED ===\n"
quit
