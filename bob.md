# esh Shell — Finish Everything: Progress Report

## Status: 11/11 features implemented, 2 bugs found and fixed, needs QEMU retest

## What Was Implemented (~1000 lines across 8 files)

### P0 — Arrays (eval.rs, expand.rs) — DONE
- `arrays: Vec<(Vec<u8>, Vec<Vec<u8>>)>` storage in Evaluator
- `arr=(a b c)` — array assignment in eval_simple
- `arr[n]=val` — indexed assignment
- `arr+=(more)` — array append
- `${arr[0]}` — index access
- `${arr[@]}` — all elements space-separated
- `${arr[*]}` — all elements IFS-joined
- `${#arr[@]}` — array length
- `${arr[@]:offset:length}` — array slice
- `declare -a name` — create empty array
- `unset arr[n]` — remove element, `unset arr` — remove whole array

### P1 — Local Variables (eval.rs, builtins.rs) — DONE, TESTED IN QEMU
- `local_frames: Vec<Vec<(Vec<u8>, Option<Vec<u8>>)>>` stack
- Frame pushed in `call_function`, popped+restored on exit
- `local VAR=val` builtin saves current value, sets new one
- **QEMU test result:** `inside: local` / `outside: global` — PASS

### P2 — getopts (builtins.rs) — DONE
- Full POSIX `getopts optstring name [args]`
- Tracks state via OPTIND/OPTARG env vars
- `:` prefix for silent errors, trailing `:` for required arguments

### P3 — Job Control (eval.rs, builtins.rs, jobs.rs) — DONE
- `job_table: JobTable` in Evaluator
- Background `&` forks pipeline, registers in job table
- `jobs` — lists active background jobs
- `fg %N` — brings job to foreground (tcsetpgrp + waitpid)
- `bg %N` — sends SIGCONT to stopped job
- `reap_background_jobs()` for cleanup

### P4 — history (builtins.rs) — DONE, TESTED IN QEMU
- `history` — prints numbered history from readline
- `history N` — shows last N entries
- `history -c` — clears history
- **QEMU test result:** Shows entries 8, 9, 10 correctly — PASS

### P5 — source Positional Params (builtins.rs) — DONE
- `source script.sh arg1 arg2` sets `$1`, `$2` in sourced script
- Saves and restores caller's positional params

### P6 — xtrace for Compound Commands (eval.rs) — DONE, TESTED IN QEMU
- `set -x` now traces `for`, `while`/`until`, `if`, `case`
- For loops show variable assignments per iteration
- **Bug found:** trace output used stdout (putchar) instead of stderr — FIXED (now uses prints_bytes which writes to fd 2)

### P7 — SIGINT Handling (eval.rs, main.rs) — DONE
- `sigint_handler` sets flag instead of SIG_IGN
- `eval_program`, `eval_for`, `eval_while` check SIGINT_RECEIVED
- If trap set for INT, runs trap handler instead of breaking
- Status 130 (128+SIGINT) on interrupt
- Clear flag on each prompt iteration

### P8 — select Command (token.rs, ast.rs, parser.rs, eval.rs) — DONE
- `Select` token and keyword
- `SelectCommand` AST node (same shape as ForCommand)
- `parse_select()` in parser
- `eval_select()` — prints numbered menu to stderr, reads choice, sets REPLY and variable, loops until break

### P9 — Process Substitution (expand.rs) — DONE
- `<(cmd)` — forks child, captures stdout to `/tmp/esh_ps_PID_N`, returns path
- `>(cmd)` — creates temp file (best-effort without /dev/fd)
- Detected in `expand_word` before other expansion phases

### P10 — Programmable Completion (builtins.rs, main.rs) — DONE
- `complete -W "words" cmd` — registers word completions
- `complete -f cmd` — file completions
- `complete -d cmd` — directory completions
- Static table of 32 completion specs
- `shell_completion` callback checks specs before default path completion

## Bugs Found and Fixed After QEMU Testing

### Bug 1: Function $1 not expanding (FIXED)
- **Symptom:** `greet() { echo "Hello $1"; }; greet World` printed `Hello` (no World)
- **Root cause:** `$1` mapped to `positional[1]` but positional is 0-indexed (`positional[0]` = first arg)
- **Fix:** Changed expander so `$0` hardcodes "esh", `$1` maps to `positional[0]`, `$2` to `positional[1]`, etc.
- **File:** `expand.rs` lines ~190-197

### Bug 2: xtrace output on stdout instead of stderr (FIXED)
- **Symptom:** `set -x; echo traced` showed `echotracedtraced` (concatenated stdout+trace)
- **Root cause:** `xtrace_command()` used `print_bytes()` which calls `putchar()` (stdout)
- **Fix:** Changed to `prints_bytes()` which writes to fd 2 (stderr)
- **File:** `eval.rs` xtrace_command method

## QEMU Test Results (Build 1486)

| Test | Result |
|------|--------|
| echo hello world | PASS |
| Variables ($arr0 $arr1 $arr2) | PASS |
| Local variables (local x=local) | PASS |
| For loop (for i in 1 2 3) | PASS |
| If/elif/else | PASS |
| Case/esac | PASS |
| Functions (greet World) | FAIL -> FIXED |
| set -x xtrace | FAIL (formatting) -> FIXED |
| Pipes (echo hello \| cat) | PASS |
| history 3 | PASS |
| jobs (empty, correct) | PASS |

## What Still Needs QEMU Testing

After the two bug fixes (build 1488+), these need verification:
1. **Function $1:** `greet() { echo "Hello $1"; }; greet World` should print `Hello World`
2. **xtrace:** `set -x; echo traced; set +x` should show `+ echo traced` on stderr, `traced` on stdout
3. **Arrays:** `declare -a arr; arr=(a b c); echo ${arr[1]}` should print `b`
4. **select:** `select x in a b c; do echo $x; break; done` (interactive — needs stdin)
5. **Background jobs:** `sleep 5 &; jobs` should show running job
6. **getopts:** Script-based test needed
7. **SIGINT:** `while true; do sleep 1; done` then Ctrl+C should interrupt
8. **Process substitution:** `cat <(echo hello)` should print `hello`
9. **Source positional:** `source /tmp/test.sh arg1` where test.sh echoes `$1`

## Files Modified

| File | Lines Added | Changes |
|------|------------|---------|
| `eval.rs` | ~350 | Arrays, local vars, job table, SIGINT, select, xtrace, background & |
| `expand.rs` | ~130 | Array expansion in braced vars, process substitution, $1 fix |
| `builtins.rs` | ~350 | getopts, history, complete, local, jobs/fg/bg, source fix |
| `token.rs` | ~3 | Select keyword |
| `ast.rs` | ~15 | SelectCommand struct |
| `parser.rs` | ~30 | parse_select() |
| `main.rs` | ~40 | SIGINT handler, completion hooks, select in builtins list |
| `jobs.rs` | 0 | Already existed, wired into evaluator |

## Build Status
- `cargo build -p esh` — CLEAN (0 warnings)
- `cargo build -p esh --release` — CLEAN
- `make build` — CLEAN (full OS build passes)

## Notes
- The MCP QEMU server crashed during testing (QEMU display glitch, not our code)
- Build 1486 (before $1 and xtrace fixes) booted and tested fine
- Build 1487 had a QEMU display issue ("Display output is not active") but serial showed full boot (831 lines)
- Need to restart MCP server to resume QEMU testing
