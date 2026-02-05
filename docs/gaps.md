
  arch-x86_64/src/lib.rs:813 │ Calibrate TSC properly using APIC/HPET      
  src/scheduler.rs:137       │ Re-enable debug buffer flush after race fix 
  sched/src/core.rs:253      │ Send IPI for cross-CPU reschedule           
  sched/src/core.rs:615      │ Migrate task if CPU affinity changes        
  src/mount.rs:120             │ Implement full remount support      
  src/mount.rs:219             │ Implement real sysfs                
  src/mount.rs:305             │ Check for open files before unmount 
  vfs/tmpfs/src/lib.rs:263,390 │ Get UID from creation context       
  syscall/src/time.rs:308        │ Track actual per-process CPU time (CLOCK_PROCESS_CPUTIME_ID) 
  syscall/src/socket.rs:577,1361 │ Remove legacy BOUND_SOCKETS compatibility                    
  tty/terminal/src/lib.rs:662 │ Respond to terminal state query 
  boot/boot-proto/src/arcs.rs:219 │ Call firmware GetMemoryDescriptor 
  proc/src/exec.rs:202 │ TEMP HACK: Force TLS setup (should be cleaned up) 

  ⚠️ Minor Gaps

  ┌─────────────────────┬────────┬────────────────────────┐
  │ Missing             │ Impact │ Notes                  │
  ├─────────────────────┼────────┼────────────────────────┤
  │ CSI t (XTWINOPS)    │ MEDIUM │ Window size queries    │
  ├─────────────────────┼────────┼────────────────────────┤
  │ OSC 7 (working dir) │ MEDIUM │ Shell integration      │
  ├─────────────────────┼────────┼────────────────────────┤
  │ OSC 8 (hyperlinks)  │ MEDIUM │ Modern terminals       │
  ├─────────────────────┼────────┼────────────────────────┤
  │ CSI b (REP)         │ LOW    │ Character repeat       │
  ├─────────────────────┼────────┼────────────────────────┤
  │ CSI Z (CBT)         │ LOW    │ Backward tab           │
  ├─────────────────────┼────────┼────────────────────────┤
  │ F13-F24 keys        │ LOW    │ Extended function keys │
  └─────────────────────┴────────┴────────────────────────┘
