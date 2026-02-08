# Quick boot check script
# — ColdCipher: Fast health check to see if we even make it past POST.

# Breakpoint at kernel entry (kernel_main is the ELF entry point)
break kernel_main
commands
    silent
    printf "✓ Reached kernel_main (entry point)\n"
    bt 3
    continue
end

# Break on panic
break rust_begin_unwind
commands
    silent
    printf "✗ PANIC detected\n"
    bt
    info registers
end

# Continue execution
printf "Starting boot check...\n"
continue
