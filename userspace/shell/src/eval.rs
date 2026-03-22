//! Shell Evaluator — walks AST and executes commands
//!
//! — ByteRiot: the executioner. Takes the parse tree and makes it real.
//! Forks processes, sets up pipes, handles redirections, runs builtins
//! in-process, and manages exit status propagation through &&/|| chains.
//!
//! This is where the AST becomes side effects.

extern crate alloc;
use alloc::vec::Vec;
use alloc::boxed::Box;
use libc::*;

use crate::ast::*;
use crate::expand::{ExpandContext, expand_word, expand_word_nosplit, glob_match};
use crate::builtins;
use crate::jobs::{JobTable, JobState};

/// Maximum number of pipe stages
const MAX_PIPES: usize = 16;

/// Shell options — set -e/-x/-u/-o pipefail
/// — ByteRiot: the knobs that turn a friendly REPL into a strict CI executor.
/// One wrong `set -e` and your whole pipeline explodes. Beautiful.
pub struct ShellOpts {
    /// -e: exit on error (non-zero status not in if/while/&&/||)
    pub errexit: bool,
    /// -x: print commands before execution (xtrace)
    pub xtrace: bool,
    /// -u: error on unset variable expansion
    pub nounset: bool,
    /// -o pipefail: pipeline fails if ANY stage fails
    pub pipefail: bool,
    // — ByteRiot: shopt options. Bash extensions that every script assumes exist.
    /// nullglob: unmatched globs expand to nothing (not the literal pattern)
    pub nullglob: bool,
    /// dotglob: globs match files starting with .
    pub dotglob: bool,
    /// nocaseglob: case-insensitive glob matching
    pub nocaseglob: bool,
    /// failglob: unmatched glob is an error
    pub failglob: bool,
    /// globstar: ** matches recursively
    pub globstar: bool,
    /// extglob: extended glob patterns ?(pat), *(pat), +(pat), @(pat), !(pat)
    pub extglob: bool,
}

impl ShellOpts {
    pub fn new() -> Self {
        ShellOpts {
            errexit: false, xtrace: false, nounset: false, pipefail: false,
            nullglob: false, dotglob: false, nocaseglob: false,
            failglob: false, globstar: false, extglob: false,
        }
    }
}

/// Global SIGINT flag — set by signal handler, checked by eval loops
/// — ThreadRogue: the red button. When the user slams Ctrl+C, this flag
/// goes hot. Every loop iteration checks it, every pipeline watches for it.
/// AtomicBool because signal handlers run asynchronously — a plain bool
/// lets the compiler cache the false value across loop iterations, making
/// Ctrl+C invisible. SeqCst ordering ensures the store in the signal handler
/// is visible to the main thread immediately.
static SIGINT_RECEIVED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// SIGINT handler — sets the global flag, doesn't kill the shell
/// — ThreadRogue: catch the bullet, don't eat it. Set the flag and let
/// the evaluator decide what to do. Much better than SIG_IGN which swallows
/// the signal and leaves users wondering if their keyboard works.
pub unsafe extern "C" fn sigint_handler(_sig: i32) {
    SIGINT_RECEIVED.store(true, core::sync::atomic::Ordering::SeqCst);
}

/// Check if SIGINT was received, clear the flag if so
pub fn check_sigint() -> bool {
    // — ThreadRogue: swap(false) atomically reads and clears in one shot.
    // No window where another signal could slip through uncounted.
    SIGINT_RECEIVED.swap(false, core::sync::atomic::Ordering::SeqCst)
}

/// Clear SIGINT flag without checking
pub fn clear_sigint() {
    SIGINT_RECEIVED.store(false, core::sync::atomic::Ordering::SeqCst);
}

/// Set SIGINT flag programmatically (e.g., when child died from SIGINT)
/// — ThreadRogue: when the foreground child eats the SIGINT and dies, the
/// shell's own handler never fires (child was the foreground PGID). We need
/// to manually set the flag so the while loop knows to bail.
pub fn sigint_handler_set() {
    SIGINT_RECEIVED.store(true, core::sync::atomic::Ordering::SeqCst);
}

/// Evaluator state — wraps the mutable shell environment
pub struct Evaluator {
    /// Last command exit status ($?)
    pub last_status: i32,
    /// Shell PID ($$)
    pub pid: i32,
    /// Positional parameters ($1, $2, ...)
    pub positional: Vec<Vec<u8>>,
    /// Shell functions: (name, body)
    /// — ByteRiot: user-defined functions. Searched AFTER builtins, BEFORE PATH.
    pub functions: Vec<(Vec<u8>, Program)>,
    /// Shell options (set -e/-x/-u/-o pipefail)
    pub opts: ShellOpts,
    /// Signal traps: indexed by signal number. None = default, Some(empty) = ignore
    /// — ByteRiot: trap handlers. 32 slots because that's all POSIX guarantees.
    pub traps: [Option<Vec<u8>>; 32],
    /// Current loop nesting depth (for break/continue validation)
    pub loop_depth: usize,
    /// break count — how many loops to break out of
    pub break_count: i32,
    /// continue count — how many loops to continue
    pub continue_count: i32,
    /// return was requested (inside function body)
    pub return_requested: bool,
    /// return status value
    pub return_status: i32,
    /// Whether we're currently inside a function call
    pub in_function: bool,
    /// Whether we're inside an if/while/&&/|| condition (suppresses errexit)
    pub in_condition: bool,
    /// Bash-style indexed arrays: (name, elements)
    /// — IronGhost: the array dimension. Bash scripts live and die by arrays.
    /// Without them, every package manager script falls apart like wet cardboard.
    pub arrays: Vec<(Vec<u8>, Vec<Vec<u8>>)>,
    /// — IronGhost: associative arrays (declare -A). Each entry is (name, [(key, value)]).
    /// Bash scripts use these for lookup tables, option parsing, config maps.
    /// ${assoc[key]}, ${!assoc[@]} for keys, ${#assoc[@]} for count.
    pub assoc_arrays: Vec<(Vec<u8>, Vec<(Vec<u8>, Vec<u8>)>)>,
    /// Local variable frames — stack of (name, saved_value) pairs
    /// — IronGhost: function-scoped variables. Push a frame on function entry,
    /// pop+restore on exit. Without this, every function clobbers the caller's state.
    pub local_frames: Vec<Vec<(Vec<u8>, Option<Vec<u8>>)>>,
    /// Job table — background process tracking
    /// — ThreadRogue: the job overseer. Tracks background processes for jobs/fg/bg.
    pub job_table: JobTable,
    /// — ByteRiot: FUNCNEST — recursion depth guard. Incremented on function entry,
    /// decremented on exit. If it exceeds max_funcnest (default 1000), the function
    /// call is rejected with an error. Without this, `f() { f; }; f` eats the stack
    /// until the process segfaults. — ByteRiot
    pub funcnest: usize,
    pub max_funcnest: usize,
    /// — ByteRiot: PIPESTATUS — per-stage exit codes from the last pipeline.
    /// ${PIPESTATUS[0]} is the first stage, ${PIPESTATUS[@]} is all of them.
    /// Updated after every pipeline, even single commands (for consistency).
    pub pipestatus: Vec<i32>,
}

const NONE_TRAP: Option<Vec<u8>> = None;

impl Evaluator {
    pub fn new() -> Self {
        Evaluator {
            last_status: 0,
            pid: getpid(),
            positional: Vec::new(),
            functions: Vec::new(),
            opts: ShellOpts::new(),
            traps: [NONE_TRAP; 32],
            loop_depth: 0,
            break_count: 0,
            continue_count: 0,
            return_requested: false,
            return_status: 0,
            in_function: false,
            in_condition: false,
            arrays: Vec::new(),
            assoc_arrays: Vec::new(),
            local_frames: Vec::new(),
            job_table: JobTable::new(),
            funcnest: 0,
            max_funcnest: 1000,
            pipestatus: Vec::new(),
        }
    }

    /// Build expansion context from current state
    pub fn expand_ctx(&self) -> ExpandContext<'_> {
        ExpandContext {
            last_status: self.last_status,
            pid: self.pid,
            positional: self.positional.clone(),
            nounset: self.opts.nounset,
            arrays: &self.arrays,
            assoc_arrays: &self.assoc_arrays,
        }
    }

    /// Evaluate a complete program (list of compound commands)
    pub fn eval_program(&mut self, prog: &Program) {
        for compound in &prog.commands {
            // — ThreadRogue: check SIGINT between commands. If the user hammered
            // Ctrl+C, bail out now. Run trap handler if one is set, otherwise
            // set status 130 (128 + SIGINT) and stop executing.
            if check_sigint() {
                if let Some(ref trap_cmd) = self.traps[2] {
                    let cmd = trap_cmd.clone();
                    let tokens = crate::token::tokenize(&cmd);
                    if let Ok(trap_prog) = crate::parser::parse(tokens) {
                        self.eval_program(&trap_prog);
                    }
                } else {
                    self.last_status = 130;
                    return;
                }
            }

            self.eval_compound_list(compound);
            self.check_traps();

            // — ByteRiot: short-circuit on break/continue/return
            if self.break_count > 0 || self.continue_count > 0 || self.return_requested {
                return;
            }

            // — ByteRiot: errexit check — bail on non-zero unless in condition context
            if self.opts.errexit && self.last_status != 0 && !self.in_condition {
                return;
            }
        }
    }

    /// Evaluate a compound list (pipelines joined by && / ||)
    /// — ByteRiot: short-circuit evaluation. && only runs next if prev
    /// succeeded, || only runs next if prev failed. Just like C.
    pub fn eval_compound_list(&mut self, list: &CompoundList) {
        // — ThreadRogue: background execution — fork the entire pipeline
        // evaluation. Parent registers the child in the job table and moves on.
        // Child runs the pipeline and exits.
        if list.background {
            // Reconstruct a rough command string for job display
            let mut cmd_str = Vec::new();
            if let Some(Command::Simple(sc)) = list.first.commands.first() {
                for (j, word) in sc.words.iter().enumerate() {
                    if j > 0 { cmd_str.push(b' '); }
                    cmd_str.extend_from_slice(word);
                }
            }
            cmd_str.extend_from_slice(b" &");

            let pid = fork();
            if pid == 0 {
                // Child: run the pipeline and exit
                setpgid(0, 0);
                signal(SIGINT, SIG_DFL);
                self.eval_pipeline(&list.first);
                _exit(self.last_status);
            } else if pid > 0 {
                setpgid(pid, pid);
                let job_id = self.job_table.add(pid, &[pid], cmd_str);
                prints("[");
                print_i64_stderr(job_id as i64);
                prints("] ");
                print_i64_stderr(pid as i64);
                eprintlns("");
                self.last_status = 0;
            }
            return;
        }

        // — ByteRiot: &&/|| chains suppress errexit for the condition side
        let had_rest = !list.rest.is_empty();
        if had_rest { self.in_condition = true; }

        self.eval_pipeline(&list.first);

        for (i, (op, pipeline)) in list.rest.iter().enumerate() {
            // Last element in chain is NOT a condition
            let is_last = i == list.rest.len() - 1;
            if is_last { self.in_condition = false; }

            match op {
                ListOp::And => {
                    if self.last_status != 0 { continue; }
                    self.eval_pipeline(pipeline);
                }
                ListOp::Or => {
                    if self.last_status == 0 { continue; }
                    self.eval_pipeline(pipeline);
                }
            }
        }

        if had_rest { self.in_condition = false; }
    }

    /// Evaluate a pipeline (commands joined by |)
    /// — ByteRiot: the pipe dream. Each command gets its own process,
    /// with stdout→stdin plumbing between them.
    fn eval_pipeline(&mut self, pipeline: &Pipeline) {
        let cmds = &pipeline.commands;

        if cmds.len() == 1 {
            // Single command — might be a builtin (no fork needed)
            // — ByteRiot: xtrace — print command before executing
            if self.opts.xtrace {
                self.xtrace_command(&cmds[0]);
            }
            let status = self.eval_command(&cmds[0]);
            self.last_status = if pipeline.negated { if status == 0 { 1 } else { 0 } } else { status };
            // — ByteRiot: PIPESTATUS for single commands — just one element.
            self.pipestatus = alloc::vec![self.last_status];
            self.update_pipestatus_array();
            return;
        }

        // Multi-command pipeline: fork each, plumb pipes
        let num = cmds.len().min(MAX_PIPES);
        let mut pipes: [[i32; 2]; MAX_PIPES] = [[0; 2]; MAX_PIPES];
        for i in 0..(num - 1) {
            if pipe(&mut pipes[i]) < 0 {
                eprintlns("esh: pipe failed");
                self.last_status = 1;
                return;
            }
        }

        let mut pids = [0i32; MAX_PIPES];
        let mut pgid: i32 = 0;
        let mut statuses = [0i32; MAX_PIPES];

        for i in 0..num {
            let pid = fork();
            if pid == 0 {
                // — ByteRiot: child process. Wire up pipes, close extras, exec.
                if i == 0 {
                    setpgid(0, 0);
                } else {
                    setpgid(0, pids[0]);
                }

                // stdin from previous pipe
                if i > 0 {
                    dup2(pipes[i - 1][0], 0);
                }
                // stdout to next pipe
                if i < num - 1 {
                    dup2(pipes[i][1], 1);
                }

                // Close all pipe fds
                for j in 0..(num - 1) {
                    close(pipes[j][0]);
                    close(pipes[j][1]);
                }

                // Apply redirections for this command
                self.apply_redirections(&cmds[i]);

                // — ThreadRogue: reset ALL job-control signals to default.
                // The shell ignores SIGTSTP/SIGTTIN/SIGTTOU for itself, but
                // children must get SIG_DFL so Ctrl+Z actually stops them.
                signal(SIGINT, SIG_DFL);
                signal(SIGQUIT, SIG_DFL);
                signal(SIGTSTP, SIG_DFL);
                signal(SIGTTIN, SIG_DFL);
                signal(SIGTTOU, SIG_DFL);
                const SIG_SETMASK: i32 = 2;
                let empty_mask: u64 = 0;
                let _ = sys_sigprocmask(SIG_SETMASK, &empty_mask as *const u64, core::ptr::null_mut());

                // Execute
                self.exec_command_in_child(&cmds[i]);
                _exit(127);
            } else if pid > 0 {
                pids[i] = pid;
                if i == 0 {
                    pgid = pid;
                    setpgid(pid, pid);
                    let _ = tcsetpgrp(0, pgid);
                } else {
                    setpgid(pid, pgid);
                }
            } else {
                eprintlns("esh: fork failed");
            }
        }

        // Parent: close all pipe fds
        for i in 0..(num - 1) {
            close(pipes[i][0]);
            close(pipes[i][1]);
        }

        // — ByteRiot: Wait for all children with proper signal handling.
        // WUNTRACED lets us detect Ctrl+Z (SIGTSTP) stopped children.
        // On EINTR + SIGINT, kill the entire process group — don't leave
        // orphaned pipeline stages burning CPU. Reap all zombies before
        // restoring the terminal. Linux shells get this wrong for decades
        // and people just accept it. Not us. — ByteRiot
        let mut any_stopped = false;
        for i in 0..num {
            let mut status = 0;
            loop {
                let ret = waitpid(pids[i], &mut status, WUNTRACED);
                if ret == pids[i] {
                    break;
                }
                if ret < 0 && ret != -(libc::errno::EINTR as i32) {
                    break;
                }
                // — ThreadRogue: EINTR means a signal arrived. If it's SIGINT,
                // nuke the entire process group and reap the corpse.
                if check_sigint() {
                    sys_kill(-(pgid as i32), 9); // SIGKILL whole group
                    waitpid(pids[i], &mut status, 0); // reap this child
                    sigint_handler_set(); // re-set for eval loop
                    break;
                }
            }

            // — ByteRiot: decode wait status the Linux way.
            if wifstopped(status) {
                // Child stopped (Ctrl+Z) — create a job entry
                let stop_sig = wstopsig(status);
                let cmd_str = self.reconstruct_pipeline_cmd(cmds);
                self.job_table.add(pgid, &pids[..num], cmd_str);
                self.job_table.mark_stopped(pgid);
                let jid = self.job_table.find_by_pgid(pgid).map(|j| j.id).unwrap_or(0);
                eprints("\n[");
                print_i64_stderr(jid as i64);
                eprints("]+  Stopped                 ");
                if let Some(j) = self.job_table.find_by_pgid(pgid) {
                    libc::write(2, &j.command);
                }
                eprintlns("");
                any_stopped = true;
                self.last_status = 128 + stop_sig;
                break; // Don't wait for remaining stages — they're all in the same pgid
            } else if wifsignaled(status) {
                statuses[i] = 128 + wtermsig(status);
                // — ThreadRogue: if killed by SIGINT, propagate to eval loop
                if wtermsig(status) == 2 {
                    sigint_handler_set();
                }
            } else {
                statuses[i] = wexitstatus(status);
            }
        }

        // — ByteRiot: update PIPESTATUS with per-stage exit codes.
        // Stored both in the Evaluator field and as a named array for ${PIPESTATUS[@]}.
        self.pipestatus = statuses[..num].to_vec();
        self.update_pipestatus_array();

        if !any_stopped {
            // — ByteRiot: pipefail — any non-zero stage fails the whole pipeline
            if self.opts.pipefail {
                let mut fail_status = 0;
                for i in 0..num {
                    if statuses[i] != 0 { fail_status = statuses[i]; }
                }
                self.last_status = fail_status;
            } else {
                // Traditional: last stage determines status
                self.last_status = statuses[num - 1];
            }
        }

        // — ByteRiot: Restore shell as foreground process group.
        for _ in 0..8 {
            if tcsetpgrp(0, getpid()) == 0 { break; }
            sched_yield();
        }

        if pipeline.negated {
            self.last_status = if self.last_status == 0 { 1 } else { 0 };
        }

        // — FuzzStatic: clean up any process substitution temp files.
        crate::expand::cleanup_process_substitution();
    }

    /// — ByteRiot: sync PIPESTATUS field into the named arrays list.
    /// This makes ${PIPESTATUS[@]} and ${PIPESTATUS[N]} work through
    /// the standard array expansion path. No special-casing needed.
    fn update_pipestatus_array(&mut self) {
        let elements: Vec<Vec<u8>> = self.pipestatus.iter().map(|&s| {
            let mut buf = Vec::new();
            crate::expand::append_i64(&mut buf, s as i64);
            buf
        }).collect();
        // Remove existing PIPESTATUS entry if any
        self.arrays.retain(|(name, _)| name != b"PIPESTATUS");
        self.arrays.push((b"PIPESTATUS".to_vec(), elements));
    }

    /// — ByteRiot: reconstruct a pipeline command string for job display.
    /// Not perfect — just enough for `jobs` to show something recognizable.
    fn reconstruct_pipeline_cmd(&self, cmds: &[Command]) -> Vec<u8> {
        let mut result = Vec::new();
        for (i, cmd) in cmds.iter().enumerate() {
            if i > 0 { result.extend_from_slice(b" | "); }
            if let Command::Simple(sc) = cmd {
                for (j, w) in sc.words.iter().enumerate() {
                    if j > 0 { result.push(b' '); }
                    result.extend_from_slice(w);
                }
            }
        }
        result
    }

    /// — ByteRiot: reconstruct a simple command string for job display.
    fn reconstruct_simple_cmd(&self, sc: &SimpleCommand) -> Vec<u8> {
        let mut result = Vec::new();
        for (j, w) in sc.words.iter().enumerate() {
            if j > 0 { result.push(b' '); }
            result.extend_from_slice(w);
        }
        result
    }

    /// Evaluate a single command — dispatches to builtin or external
    /// Returns exit status.
    fn eval_command(&mut self, cmd: &Command) -> i32 {
        match cmd {
            Command::Simple(sc) => self.eval_simple(sc),
            Command::If(ic) => self.eval_if(ic),
            Command::For(fc) => self.eval_for(fc),
            Command::While(wc) => self.eval_while(wc, false),
            Command::Until(wc) => self.eval_while(wc, true),
            Command::Subshell(prog) => self.eval_subshell(prog),
            Command::Group(prog) => {
                self.eval_program(prog);
                self.last_status
            }
            Command::FunctionDef { name, body } => {
                self.define_function(name, body);
                0
            }
            Command::Case(cc) => self.eval_case(cc),
            Command::ExtendedTest(expr) => self.eval_extended_test(expr),
            Command::Select(sc) => self.eval_select(sc),
        }
    }

    /// Evaluate a simple command
    /// — ByteRiot: the workhorse. Handles assignments, builtins, functions, and externals.
    fn eval_simple(&mut self, sc: &SimpleCommand) -> i32 {
        // — IronGhost: process assignments first, handling array assignments.
        // We need a fresh ctx for each expand because array mutations
        // invalidate the borrowed ctx reference.
        let mut assignments = Vec::new();
        let mut array_assignments: Vec<(Vec<u8>, Vec<Vec<u8>>, bool)> = Vec::new(); // (name, elements, append)
        let mut indexed_assignments: Vec<(Vec<u8>, usize, Vec<u8>)> = Vec::new();

        for asgn in &sc.assignments {
            let name = asgn.name.clone();
            let ctx = self.expand_ctx();
            let value = expand_word_nosplit(&asgn.value, &ctx);
            if value.starts_with(b"(") && value.ends_with(b")") {
                let inner = &value[1..value.len() - 1];
                let elements = self.split_array_elements(inner);
                array_assignments.push((name, elements, false));
                continue;
            }
            if let Some(bracket_pos) = name.iter().position(|&b| b == b'[') {
                if name.ends_with(b"]") {
                    let arr_name = name[..bracket_pos].to_vec();
                    let key_bytes = name[bracket_pos + 1..name.len() - 1].to_vec();
                    // — IronGhost: if this is an associative array, use string key directly.
                    // Otherwise parse as numeric index for indexed arrays.
                    if self.is_assoc(&arr_name) {
                        self.set_assoc(&arr_name, &key_bytes, value);
                    } else {
                        let idx = parse_i64(&key_bytes) as usize;
                        indexed_assignments.push((arr_name, idx, value));
                    }
                    continue;
                }
            }
            assignments.push((name, value));
        }

        // Apply array/indexed assignments
        for (name, elements, append) in array_assignments {
            if append { self.append_array(&name, elements); }
            else { self.set_array(&name, elements); }
        }
        for (name, idx, value) in indexed_assignments {
            self.set_array_element(&name, idx, value);
        }

        // Expand all words
        let mut argv: Vec<Vec<u8>> = Vec::new();
        {
            let ctx = self.expand_ctx();
            for word in &sc.words {
                let expanded = expand_word(word, &ctx);
                argv.extend(expanded);
            }
        }

        // — IronGhost: detect array assignment in words: arr=(a b c) or arr+=(x y)
        if !argv.is_empty() {
            let first = &argv[0];
            // Check for name=(...)
            if let Some(eq_pos) = first.iter().position(|&b| b == b'=') {
                let name = &first[..eq_pos];
                let after_eq = &first[eq_pos + 1..];
                if after_eq.starts_with(b"(") {
                    let mut collected = after_eq.to_vec();
                    let mut wi = 1;
                    while !collected.ends_with(b")") && wi < argv.len() {
                        collected.push(b' ');
                        collected.extend_from_slice(&argv[wi]);
                        wi += 1;
                    }
                    if collected.starts_with(b"(") && collected.ends_with(b")") {
                        let inner = &collected[1..collected.len() - 1];
                        let elements = self.split_array_elements(inner);
                        self.set_array(&name.to_vec(), elements);
                        return 0;
                    }
                }
                // Check for name+=(...)
                if eq_pos >= 2 && first[eq_pos - 1] == b'+' {
                    let name = &first[..eq_pos - 1];
                    let after_eq = &first[eq_pos + 1..];
                    if after_eq.starts_with(b"(") {
                        let mut collected = after_eq.to_vec();
                        let mut wi = 1;
                        while !collected.ends_with(b")") && wi < argv.len() {
                            collected.push(b' ');
                            collected.extend_from_slice(&argv[wi]);
                            wi += 1;
                        }
                        if collected.starts_with(b"(") && collected.ends_with(b")") {
                            let inner = &collected[1..collected.len() - 1];
                            let elements = self.split_array_elements(inner);
                            self.append_array(&name.to_vec(), elements);
                            return 0;
                        }
                    }
                }
            }
        }

        // Assignment-only command: set variables in current shell
        if argv.is_empty() {
            for (name, value) in &assignments {
                if let (Ok(n), Ok(v)) = (core::str::from_utf8(name), core::str::from_utf8(value)) {
                    setenv(n, v);
                }
            }
            return 0;
        }

        // Check for builtin
        if let Some(status) = builtins::try_exec_builtin(&argv, &sc.redirections, self) {
            return status;
        }

        // — ByteRiot: check function table AFTER builtins, BEFORE externals.
        // User functions can shadow external commands but never builtins.
        if let Some(func_body) = self.lookup_function(&argv[0]) {
            return self.call_function(func_body, &argv[1..]);
        }

        // External command — fork and exec
        self.exec_external(&argv, &sc.redirections, &assignments)
    }

    /// Define a shell function
    /// — ByteRiot: store the function body in our table. If it already exists,
    /// replace it. Functions are just programs we replay later.
    fn define_function(&mut self, name: &[u8], body: &Program) {
        // Replace existing or add new
        for entry in self.functions.iter_mut() {
            if entry.0 == name {
                entry.1 = body.clone();
                return;
            }
        }
        self.functions.push((name.to_vec(), body.clone()));
    }

    /// Look up a function by name
    fn lookup_function(&self, name: &[u8]) -> Option<Program> {
        for (fname, body) in &self.functions {
            if fname == name {
                return Some(body.clone());
            }
        }
        None
    }

    /// Call a shell function
    /// — ByteRiot: save positional params, set new ones from args, eval body,
    /// restore positional params. return builtin sets return_requested.
    /// — IronGhost: now with local variable frame support. Push a frame before
    /// eval, pop+restore on exit. `local VAR=val` saves the old value in the frame.
    fn call_function(&mut self, body: Program, args: &[Vec<u8>]) -> i32 {
        // — ByteRiot: recursion guard. Without this, `f(){ f; }; f` eats the
        // entire stack and segfaults. Bash defaults FUNCNEST=unset (no limit)
        // but we're not insane — 1000 is plenty for any real script. — ByteRiot
        if self.funcnest >= self.max_funcnest {
            eprintlns("esh: maximum function nesting level exceeded");
            return 1;
        }
        self.funcnest += 1;

        // Save state
        let saved_positional = core::mem::take(&mut self.positional);
        let saved_in_function = self.in_function;
        let saved_return_requested = self.return_requested;

        // — IronGhost: push a new local variable frame
        self.local_frames.push(Vec::new());

        // Set up function environment
        self.positional = args.to_vec();
        self.in_function = true;
        self.return_requested = false;

        // Execute function body
        self.eval_program(&body);

        // Handle return
        let status = if self.return_requested {
            self.return_status
        } else {
            self.last_status
        };

        // — IronGhost: pop the local frame and restore saved values.
        // Each (name, saved_value) pair either restores the old value or
        // unsets the variable if it didn't exist before `local` was called.
        if let Some(frame) = self.local_frames.pop() {
            for (name, saved_val) in frame {
                if let Ok(n) = core::str::from_utf8(&name) {
                    match saved_val {
                        Some(v) => {
                            if let Ok(vs) = core::str::from_utf8(&v) {
                                setenv(n, vs);
                            }
                        }
                        None => { libc::unsetenv(n); }
                    }
                }
            }
        }

        // Restore state
        self.funcnest -= 1;
        self.positional = saved_positional;
        self.in_function = saved_in_function;
        self.return_requested = saved_return_requested;
        self.last_status = status;

        status
    }

    /// Case command evaluation
    /// — ByteRiot: pattern matching via glob. First matching arm wins.
    /// `case $ext in *.c) echo C;; *.rs) echo Rust;; *) echo unknown;; esac`
    fn eval_case(&mut self, cc: &CaseCommand) -> i32 {
        let ctx = self.expand_ctx();
        let word = expand_word_nosplit(&cc.word, &ctx);
        if self.opts.xtrace {
            eprints("+ case ");
            prints_bytes(&word);
            eprintlns("");
        }

        for arm in &cc.arms {
            for pattern in &arm.patterns {
                let expanded_pat = expand_word_nosplit(pattern, &ctx);
                if glob_match(&expanded_pat, &word) {
                    self.eval_program(&arm.body);
                    return self.last_status;
                }
            }
        }

        // No match — status 0
        self.last_status = 0;
        0
    }

    /// Extended test [[ ]] evaluation
    /// — ByteRiot: recursive evaluation of boolean test expressions.
    /// == uses glob matching, =~ uses simple prefix matching.
    fn eval_extended_test(&mut self, expr: &TestExpr) -> i32 {
        let result = self.eval_test_expr(expr);
        self.last_status = if result { 0 } else { 1 };
        self.last_status
    }

    /// Evaluate a test expression recursively
    fn eval_test_expr(&self, expr: &TestExpr) -> bool {
        let ctx = self.expand_ctx();
        match expr {
            TestExpr::Literal(s) => {
                let expanded = expand_word_nosplit(s, &ctx);
                !expanded.is_empty()
            }
            TestExpr::Not(inner) => !self.eval_test_expr(inner),
            TestExpr::And(left, right) => self.eval_test_expr(left) && self.eval_test_expr(right),
            TestExpr::Or(left, right) => self.eval_test_expr(left) || self.eval_test_expr(right),
            TestExpr::Unary(op, operand) => {
                let val = expand_word_nosplit(operand, &ctx);
                let val_str = bytes_to_str(&val);
                match op.as_slice() {
                    b"-z" => val.is_empty(),
                    b"-n" => !val.is_empty(),
                    b"-f" => { let fd = open2(val_str, O_RDONLY); if fd >= 0 { close(fd); true } else { false } }
                    b"-d" => {
                        use libc::dirent::opendir;
                        use libc::dirent::closedir;
                        if let Some(d) = opendir(val_str) { closedir(d); true } else { false }
                    }
                    b"-e" => { let fd = open2(val_str, O_RDONLY); if fd >= 0 { close(fd); true } else { false } }
                    b"-x" => {
                        // — ByteRiot: check executable permission via stat
                        let mut st = libc::stat::Stat::zeroed();
                        if libc::stat::stat(val_str, &mut st) == 0 {
                            (st.mode & 0o111) != 0
                        } else {
                            false
                        }
                    }
                    b"-r" => { let fd = open2(val_str, O_RDONLY); if fd >= 0 { close(fd); true } else { false } }
                    b"-w" => {
                        let fd = open(val_str, O_WRONLY, 0);
                        if fd >= 0 { close(fd); true } else { false }
                    }
                    b"-s" => {
                        let mut st = libc::stat::Stat::zeroed();
                        if libc::stat::stat(val_str, &mut st) == 0 { st.size > 0 } else { false }
                    }
                    b"-L" | b"-h" => {
                        let mut st = libc::stat::Stat::zeroed();
                        if libc::stat::lstat(val_str, &mut st) == 0 { st.is_symlink() } else { false }
                    }
                    b"-p" => {
                        let mut st = libc::stat::Stat::zeroed();
                        if libc::stat::stat(val_str, &mut st) == 0 { st.is_fifo() } else { false }
                    }
                    b"-b" => {
                        let mut st = libc::stat::Stat::zeroed();
                        if libc::stat::stat(val_str, &mut st) == 0 { st.is_block_device() } else { false }
                    }
                    b"-c" => {
                        let mut st = libc::stat::Stat::zeroed();
                        if libc::stat::stat(val_str, &mut st) == 0 { st.is_char_device() } else { false }
                    }
                    b"-S" => {
                        let mut st = libc::stat::Stat::zeroed();
                        if libc::stat::stat(val_str, &mut st) == 0 { st.is_socket() } else { false }
                    }
                    // — CrashBloom: -v VAR — true if the variable is set.
                    // Bash extension that every modern script uses.
                    b"-v" => {
                        if let Ok(name) = core::str::from_utf8(&val) {
                            getenv(name).is_some()
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            }
            TestExpr::Binary(left_raw, op, right_raw) => {
                let left = expand_word_nosplit(left_raw, &ctx);
                let right = expand_word_nosplit(right_raw, &ctx);
                match op.as_slice() {
                    b"==" | b"=" => glob_match(&right, &left),
                    b"!=" => !glob_match(&right, &left),
                    b"=~" => {
                        // — ByteRiot: simple pattern match — treat as prefix glob
                        glob_match(&right, &left)
                    }
                    b"-eq" => parse_i64(&left) == parse_i64(&right),
                    b"-ne" => parse_i64(&left) != parse_i64(&right),
                    b"-lt" => parse_i64(&left) < parse_i64(&right),
                    b"-le" => parse_i64(&left) <= parse_i64(&right),
                    b"-gt" => parse_i64(&left) > parse_i64(&right),
                    b"-ge" => parse_i64(&left) >= parse_i64(&right),
                    // — CrashBloom: string ordering — lexicographic byte comparison.
                    // Bash's [[ uses locale-aware collation but we're honest about
                    // doing raw byte comparison. Close enough for ASCII. — CrashBloom
                    b"<" => left < right,
                    b">" => left > right,
                    // — CrashBloom: file comparison operators. Parsed by the parser
                    // since forever, never evaluated until now. — CrashBloom
                    b"-nt" => {
                        let left_str = bytes_to_str(&left);
                        let right_str = bytes_to_str(&right);
                        let mut st1 = libc::stat::Stat::zeroed();
                        let mut st2 = libc::stat::Stat::zeroed();
                        if libc::stat::stat(left_str, &mut st1) == 0 && libc::stat::stat(right_str, &mut st2) == 0 {
                            st1.mtime > st2.mtime
                        } else {
                            // POSIX: if either file doesn't exist, -nt is false
                            false
                        }
                    }
                    b"-ot" => {
                        let left_str = bytes_to_str(&left);
                        let right_str = bytes_to_str(&right);
                        let mut st1 = libc::stat::Stat::zeroed();
                        let mut st2 = libc::stat::Stat::zeroed();
                        if libc::stat::stat(left_str, &mut st1) == 0 && libc::stat::stat(right_str, &mut st2) == 0 {
                            st1.mtime < st2.mtime
                        } else {
                            false
                        }
                    }
                    b"-ef" => {
                        let left_str = bytes_to_str(&left);
                        let right_str = bytes_to_str(&right);
                        let mut st1 = libc::stat::Stat::zeroed();
                        let mut st2 = libc::stat::Stat::zeroed();
                        if libc::stat::stat(left_str, &mut st1) == 0 && libc::stat::stat(right_str, &mut st2) == 0 {
                            st1.dev == st2.dev && st1.ino == st2.ino
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            }
        }
    }

    /// Print xtrace (set -x) output for a command
    fn xtrace_command(&self, cmd: &Command) {
        if let Command::Simple(sc) = cmd {
            eprints("+ ");
            let ctx = self.expand_ctx();
            for (i, word) in sc.words.iter().enumerate() {
                if i > 0 { eprints(" "); }
                let expanded = expand_word_nosplit(word, &ctx);
                prints_bytes(&expanded);
            }
            eprintlns("");
        }
    }

    /// Execute external command (fork+exec) with permission checking
    /// — ByteRiot: now checks executable bit and permissions before exec.
    /// No more mysterious "not found" errors for permission-denied files.
    pub fn exec_external(&mut self, argv: &[Vec<u8>], redirs: &[Redirect], assignments: &[(Vec<u8>, Vec<u8>)]) -> i32 {
        let pid = fork();
        if pid == 0 {
            // Child process
            setpgid(0, 0);

            // — ThreadRogue: reset all job-control signals for child.
            signal(SIGINT, SIG_DFL);
            signal(SIGQUIT, SIG_DFL);
            signal(SIGTSTP, SIG_DFL);
            signal(SIGTTIN, SIG_DFL);
            signal(SIGTTOU, SIG_DFL);
            const SIG_SETMASK: i32 = 2;
            let empty_mask: u64 = 0;
            let _ = sys_sigprocmask(SIG_SETMASK, &empty_mask as *const u64, core::ptr::null_mut());

            // Set prefix assignments in child environment
            for (name, value) in assignments {
                if let (Ok(n), Ok(v)) = (core::str::from_utf8(name), core::str::from_utf8(value)) {
                    setenv(n, v);
                }
            }

            // Apply redirections
            self.apply_redirections_from_ast(redirs);

            // Build null-terminated argv
            let mut c_argv: Vec<*const u8> = Vec::new();
            // Ensure each arg is null-terminated
            let mut owned_args: Vec<Vec<u8>> = Vec::new();
            for arg in argv {
                let mut a = arg.clone();
                if a.last() != Some(&0) { a.push(0); }
                owned_args.push(a);
            }
            for arg in &owned_args {
                c_argv.push(arg.as_ptr());
            }
            c_argv.push(core::ptr::null());

            let cmd = &argv[0];

            // Direct path execution
            if cmd.first() == Some(&b'/') || cmd.first() == Some(&b'.') {
                let path = bytes_to_str(cmd);
                // — ByteRiot: check permissions BEFORE execv so we get a useful error
                match check_exec_permission(path) {
                    ExecCheck::Ok => {}
                    ExecCheck::NotFound => {
                        eprints("esh: ");
                        print_bytes(cmd);
                        eprintlns(": No such file or directory");
                        _exit(127);
                    }
                    ExecCheck::PermissionDenied => {
                        eprints("esh: ");
                        print_bytes(cmd);
                        eprintlns(": Permission denied");
                        _exit(126);
                    }
                }
                execv(path, c_argv.as_ptr());
                eprints("esh: ");
                print_bytes(cmd);
                eprintlns(": exec failed");
                _exit(127);
            }

            // PATH search
            let path_env = getenv("PATH").unwrap_or("/bin:/usr/bin");
            let path_bytes = path_env.as_bytes();
            let cmd_len = cmd.len();

            let mut start = 0;
            let mut found_no_perm = false;
            while start <= path_bytes.len() {
                let mut end = start;
                while end < path_bytes.len() && path_bytes[end] != b':' {
                    end += 1;
                }

                let dir = &path_bytes[start..end];
                if !dir.is_empty() && dir.len() + cmd_len + 2 < 256 {
                    let mut full = Vec::with_capacity(dir.len() + 1 + cmd_len + 1);
                    full.extend_from_slice(dir);
                    if full.last() != Some(&b'/') { full.push(b'/'); }
                    full.extend_from_slice(cmd);
                    full.push(0);

                    let path_str = bytes_to_str(&full);
                    // — ByteRiot: check exec permission for each PATH candidate
                    match check_exec_permission(path_str) {
                        ExecCheck::Ok => {
                            c_argv[0] = full.as_ptr();
                            execv(path_str, c_argv.as_ptr());
                            // execv failed even though file exists+perms ok — fall through
                        }
                        ExecCheck::PermissionDenied => {
                            found_no_perm = true;
                            // — ByteRiot: continue searching PATH — might find
                            // an executable copy in a later directory
                        }
                        ExecCheck::NotFound => {
                            // Not in this directory, try next
                        }
                    }
                }

                start = end + 1;
            }

            // — ByteRiot: distinguish "not found" from "permission denied"
            if found_no_perm {
                eprints("esh: ");
                print_bytes(cmd);
                eprintlns(": Permission denied");
                _exit(126);
            }
            eprints("esh: ");
            print_bytes(cmd);
            eprintlns(": command not found");
            _exit(127);
        } else if pid > 0 {
            // Parent
            setpgid(pid, pid);
            let _ = tcsetpgrp(0, pid);

            let mut status = 0;
            loop {
                let ret = waitpid(pid, &mut status, WUNTRACED);
                if ret == pid {
                    break;
                }
                if ret < 0 && ret != -(libc::errno::EINTR as i32) {
                    break;
                }
                // — ThreadRogue: EINTR + SIGINT = nuke it from orbit
                if check_sigint() {
                    sys_kill(-(pid as i32), 9);
                    waitpid(pid, &mut status, 0);
                    sigint_handler_set();
                    break;
                }
            }

            // Restore shell foreground
            for _ in 0..8 {
                if tcsetpgrp(0, getpid()) == 0 { break; }
                sched_yield();
            }

            // — ByteRiot: decode wait status properly — stopped, signaled, or exited.
            if wifstopped(status) {
                let stop_sig = wstopsig(status);
                // Reconstruct command string from argv for job display
                let mut cmd_str = Vec::new();
                for (j, w) in argv.iter().enumerate() {
                    if j > 0 { cmd_str.push(b' '); }
                    cmd_str.extend_from_slice(w);
                }
                self.job_table.add(pid, &[pid], cmd_str);
                self.job_table.mark_stopped(pid);
                let jid = self.job_table.find_by_pgid(pid).map(|j| j.id).unwrap_or(0);
                eprints("\n[");
                print_i64_stderr(jid as i64);
                eprints("]+  Stopped                 ");
                if let Some(j) = self.job_table.find_by_pgid(pid) {
                    libc::write(2, &j.command);
                }
                eprintlns("");
                let exit_code = 128 + stop_sig;
                self.last_status = exit_code;
                return exit_code;
            } else if wifsignaled(status) {
                let exit_code = 128 + wtermsig(status);
                if wtermsig(status) == 2 { sigint_handler_set(); }
                self.last_status = exit_code;
                return exit_code;
            } else {
                let exit_code = wexitstatus(status);
                self.last_status = exit_code;
                return exit_code;
            }
        }

        eprintlns("esh: fork failed");
        self.last_status = 1;
        1
    }

    /// If command evaluation
    fn eval_if(&mut self, ic: &IfCommand) -> i32 {
        if self.opts.xtrace { eprintlns("+ if ..."); }
        for (cond, body) in &ic.branches {
            // — ByteRiot: condition is evaluated in condition context (suppresses errexit)
            let saved = self.in_condition;
            self.in_condition = true;
            self.eval_compound_list(cond);
            self.in_condition = saved;

            if self.last_status == 0 {
                self.eval_program(body);
                return self.last_status;
            }
        }

        if let Some(ref else_body) = ic.else_body {
            self.eval_program(else_body);
        }

        self.last_status
    }

    /// For loop evaluation
    fn eval_for(&mut self, fc: &ForCommand) -> i32 {
        let ctx = self.expand_ctx();
        let var_name = match core::str::from_utf8(&fc.var_name) {
            Ok(s) => s,
            Err(_) => { self.last_status = 1; return 1; }
        };

        // Expand word list
        let mut items = Vec::new();
        for word in &fc.words {
            items.extend(expand_word(word, &ctx));
        }

        // If no words, iterate over positional parameters
        if items.is_empty() {
            items = self.positional.clone();
        }

        // — ThreadRogue: xtrace for `for` loops — show each iteration
        if self.opts.xtrace {
            eprints("+ for ");
            prints_bytes(&fc.var_name);
            eprints(" in");
            for item in &items {
                eprints(" ");
                prints_bytes(item);
            }
            eprintlns("");
        }

        self.loop_depth += 1;
        for item in &items {
            // — ThreadRogue: SIGINT check in for loop — bail on Ctrl+C
            if check_sigint() {
                if let Some(ref trap_cmd) = self.traps[2] {
                    let cmd = trap_cmd.clone();
                    let tokens = crate::token::tokenize(&cmd);
                    if let Ok(trap_prog) = crate::parser::parse(tokens) {
                        self.eval_program(&trap_prog);
                    }
                } else {
                    self.last_status = 130;
                    self.loop_depth -= 1;
                    return 130;
                }
            }

            if self.opts.xtrace {
                eprints("+ ");
                prints_bytes(&fc.var_name);
                eprints("=");
                prints_bytes(item);
                eprintlns("");
            }

            if let Ok(val) = core::str::from_utf8(item) {
                setenv(var_name, val);
            }
            self.eval_program(&fc.body);

            // — ByteRiot: break/continue handling. break 2 pops two loop levels.
            if self.break_count > 0 {
                self.break_count -= 1;
                break;
            }
            if self.continue_count > 0 {
                self.continue_count -= 1;
                if self.continue_count > 0 { break; } // propagate to outer loop
                continue;
            }
            if self.return_requested { break; }
        }
        self.loop_depth -= 1;

        self.last_status
    }

    /// While/until loop evaluation
    fn eval_while(&mut self, wc: &WhileCommand, invert: bool) -> i32 {
        if self.opts.xtrace {
            eprints(if invert { "+ until ...\n" } else { "+ while ...\n" });
        }

        self.loop_depth += 1;
        loop {
            // — ThreadRogue: SIGINT check in while/until loop
            if check_sigint() {
                if let Some(ref trap_cmd) = self.traps[2] {
                    let cmd = trap_cmd.clone();
                    let tokens = crate::token::tokenize(&cmd);
                    if let Ok(trap_prog) = crate::parser::parse(tokens) {
                        self.eval_program(&trap_prog);
                    }
                } else {
                    self.last_status = 130;
                    self.loop_depth -= 1;
                    return 130;
                }
            }

            // — ByteRiot: condition check in condition context
            let saved = self.in_condition;
            self.in_condition = true;
            self.eval_compound_list(&wc.condition);
            self.in_condition = saved;

            let cond_met = if invert {
                self.last_status != 0  // until: loop while condition fails
            } else {
                self.last_status == 0  // while: loop while condition succeeds
            };

            if !cond_met { break; }
            self.eval_program(&wc.body);

            // — ByteRiot: break/continue
            if self.break_count > 0 {
                self.break_count -= 1;
                break;
            }
            if self.continue_count > 0 {
                self.continue_count -= 1;
                if self.continue_count > 0 { break; }
                continue;
            }
            if self.return_requested { break; }
        }
        self.loop_depth -= 1;

        self.last_status
    }

    /// Subshell evaluation — fork, eval in child
    fn eval_subshell(&mut self, prog: &Program) -> i32 {
        let pid = fork();
        if pid == 0 {
            self.eval_program(prog);
            _exit(self.last_status);
        } else if pid > 0 {
            let mut status = 0;
            waitpid(pid, &mut status, 0);
            self.last_status = (status >> 8) & 0xFF;
        } else {
            eprintlns("esh: fork failed");
            self.last_status = 1;
        }
        self.last_status
    }

    /// Apply redirections from AST Redirect nodes
    pub fn apply_redirections_from_ast(&self, redirs: &[Redirect]) {
        for redir in redirs {
            match redir.rtype {
                RedirectType::Input => {
                    let target = bytes_to_str(&redir.target);
                    let fd = open2(target, O_RDONLY);
                    if fd >= 0 {
                        dup2(fd, redir.fd);
                        close(fd);
                    } else {
                        eprints("esh: ");
                        print_bytes(&redir.target);
                        eprintlns(": No such file");
                    }
                }
                RedirectType::Output => {
                    let target = bytes_to_str(&redir.target);
                    let fd = open(target, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
                    if fd >= 0 {
                        dup2(fd, redir.fd);
                        close(fd);
                    } else {
                        eprints("esh: ");
                        print_bytes(&redir.target);
                        eprintlns(": Cannot create file");
                    }
                }
                RedirectType::Append => {
                    let target = bytes_to_str(&redir.target);
                    let fd = open(target, O_WRONLY | O_CREAT | O_APPEND, 0o644);
                    if fd >= 0 {
                        dup2(fd, redir.fd);
                        close(fd);
                    } else {
                        eprints("esh: ");
                        print_bytes(&redir.target);
                        eprintlns(": Cannot create file");
                    }
                }
                RedirectType::DupOut => {
                    // Target is fd number (e.g., "1" for 2>&1)
                    if let Some(&b) = redir.target.first() {
                        let target_fd = (b - b'0') as i32;
                        dup2(target_fd, redir.fd);
                    }
                }
                RedirectType::DupIn => {
                    if let Some(&b) = redir.target.first() {
                        let target_fd = (b - b'0') as i32;
                        dup2(target_fd, redir.fd);
                    }
                }
                RedirectType::HereDoc | RedirectType::HereDocStrip => {
                    // — ByteRiot: heredoc — pipe the body to stdin.
                    // Create a pipe, write the body to the write end,
                    // connect the read end to the target fd.
                    let mut pipefd = [0i32; 2];
                    if pipe(&mut pipefd) == 0 {
                        let mut body = redir.target.clone();
                        if redir.rtype == RedirectType::HereDocStrip {
                            // Strip leading tabs from each line
                            body = strip_leading_tabs(&body);
                        }
                        // Expand variables in body (unless delimiter was quoted)
                        let ctx = self.expand_ctx();
                        let expanded = expand_word_nosplit(&body, &ctx);
                        let _ = libc::write(pipefd[1], &expanded);
                        close(pipefd[1]);
                        dup2(pipefd[0], redir.fd);
                        close(pipefd[0]);
                    }
                }
                RedirectType::HereString => {
                    // — ByteRiot: here-string — `<<< "hello world"` pipes string to stdin
                    let mut pipefd = [0i32; 2];
                    if pipe(&mut pipefd) == 0 {
                        let ctx = self.expand_ctx();
                        let expanded = expand_word_nosplit(&redir.target, &ctx);
                        let _ = libc::write(pipefd[1], &expanded);
                        // Add trailing newline (POSIX convention)
                        let _ = libc::write(pipefd[1], b"\n");
                        close(pipefd[1]);
                        dup2(pipefd[0], redir.fd);
                        close(pipefd[0]);
                    }
                }
            }
        }
    }

    /// Apply redirections when evaluating a Command (for pipeline children)
    fn apply_redirections(&self, cmd: &Command) {
        if let Command::Simple(sc) = cmd {
            self.apply_redirections_from_ast(&sc.redirections);
        }
    }

    /// Execute a command directly in child process (for pipeline stages)
    fn exec_command_in_child(&mut self, cmd: &Command) {
        match cmd {
            Command::Simple(sc) => {
                let ctx = self.expand_ctx();
                let mut argv: Vec<Vec<u8>> = Vec::new();
                for word in &sc.words {
                    argv.extend(expand_word(word, &ctx));
                }
                if argv.is_empty() { _exit(0); }

                // Check for builtin (some builtins make sense in pipelines)
                if let Some(status) = builtins::try_exec_builtin(&argv, &sc.redirections, self) {
                    _exit(status);
                }

                // — ByteRiot: check functions in pipeline children too
                if let Some(func_body) = self.lookup_function(&argv[0]) {
                    let status = self.call_function(func_body, &argv[1..]);
                    _exit(status);
                }

                // External command
                let mut c_argv: Vec<*const u8> = Vec::new();
                let mut owned: Vec<Vec<u8>> = Vec::new();
                for arg in &argv {
                    let mut a = arg.clone();
                    if a.last() != Some(&0) { a.push(0); }
                    owned.push(a);
                }
                for arg in &owned {
                    c_argv.push(arg.as_ptr());
                }
                c_argv.push(core::ptr::null());

                let cmd_bytes = &argv[0];
                if cmd_bytes.first() == Some(&b'/') || cmd_bytes.first() == Some(&b'.') {
                    let path = bytes_to_str(cmd_bytes);
                    match check_exec_permission(path) {
                        ExecCheck::Ok => { execv(path, c_argv.as_ptr()); }
                        ExecCheck::PermissionDenied => {
                            eprints("esh: ");
                            print_bytes(cmd_bytes);
                            eprintlns(": Permission denied");
                        }
                        ExecCheck::NotFound => {
                            eprints("esh: ");
                            print_bytes(cmd_bytes);
                            eprintlns(": No such file or directory");
                        }
                    }
                    _exit(if matches!(check_exec_permission(path), ExecCheck::PermissionDenied) { 126 } else { 127 });
                }

                // PATH search
                let path_env = getenv("PATH").unwrap_or("/bin:/usr/bin");
                let path_bytes = path_env.as_bytes();
                let mut start = 0;
                while start <= path_bytes.len() {
                    let mut end = start;
                    while end < path_bytes.len() && path_bytes[end] != b':' { end += 1; }
                    let dir = &path_bytes[start..end];
                    if !dir.is_empty() {
                        let mut full = Vec::with_capacity(dir.len() + 1 + cmd_bytes.len() + 1);
                        full.extend_from_slice(dir);
                        if full.last() != Some(&b'/') { full.push(b'/'); }
                        full.extend_from_slice(cmd_bytes);
                        full.push(0);
                        let path_str = bytes_to_str(&full);
                        if matches!(check_exec_permission(path_str), ExecCheck::Ok) {
                            c_argv[0] = full.as_ptr();
                            execv(path_str, c_argv.as_ptr());
                        }
                    }
                    start = end + 1;
                }
                _exit(127);
            }
            Command::Subshell(prog) => {
                self.eval_program(prog);
                _exit(self.last_status);
            }
            Command::Group(prog) => {
                self.eval_program(prog);
                _exit(self.last_status);
            }
            _ => {
                // Control flow in pipeline — eval and exit
                let status = self.eval_command(cmd);
                _exit(status);
            }
        }
    }

    /// Check and run pending signal traps
    /// — ByteRiot: called between commands in eval_program. Checks if any
    /// signals have been caught and runs their trap handlers.
    pub fn check_traps(&mut self) {
        // — ByteRiot: checked between commands in eval_program. For EXIT trap (0),
        // that's handled separately via fire_exit_trap. For other signals, we'd
        // check a global atomic flag set by the signal handler — but we don't
        // have async signal delivery yet, so this is best-effort.
    }

    /// Select command evaluation — interactive menu
    /// — IronGhost: the menu maker. Prints numbered choices to stderr,
    /// reads selection from stdin, sets variable, loops until break.
    /// REPLY gets raw input, named var gets the selected word.
    fn eval_select(&mut self, sc: &SelectCommand) -> i32 {
        let ctx = self.expand_ctx();

        // Expand word list
        let mut items = Vec::new();
        for word in &sc.words {
            items.extend(expand_word(word, &ctx));
        }
        if items.is_empty() { return 0; }

        let var_name = match core::str::from_utf8(&sc.var_name) {
            Ok(s) => s,
            Err(_) => { self.last_status = 1; return 1; }
        };

        self.loop_depth += 1;
        loop {
            // Print numbered menu to stderr
            for (i, item) in items.iter().enumerate() {
                let num = i + 1;
                let mut nbuf = [0u8; 8];
                let nlen = format_usize(num, &mut nbuf);
                libc::write(2, &nbuf[..nlen]);
                libc::write(2, b") ");
                libc::write(2, item);
                libc::write(2, b"\n");
            }
            // Print PS3 prompt (or default "#? ")
            let ps3 = getenv("PS3").unwrap_or("#? ");
            libc::write(2, ps3.as_bytes());

            // Read choice from stdin
            let mut line = [0u8; 64];
            let mut pos = 0;
            loop {
                let mut ch = [0u8; 1];
                let n = read(0, &mut ch);
                if n <= 0 {
                    // EOF — exit select
                    self.loop_depth -= 1;
                    return self.last_status;
                }
                if ch[0] == b'\n' { break; }
                if pos < line.len() - 1 {
                    line[pos] = ch[0];
                    pos += 1;
                }
            }

            // Set REPLY to raw input
            let reply_str = bytes_to_str(&line[..pos]);
            setenv("REPLY", reply_str);

            // Parse choice as number
            let choice = parse_i64(&line[..pos]) as usize;
            if choice >= 1 && choice <= items.len() {
                if let Ok(val) = core::str::from_utf8(&items[choice - 1]) {
                    setenv(var_name, val);
                }
            } else {
                // Invalid choice — set var to empty
                setenv(var_name, "");
            }

            self.eval_program(&sc.body);

            if self.break_count > 0 {
                self.break_count -= 1;
                break;
            }
            if self.continue_count > 0 {
                self.continue_count -= 1;
                if self.continue_count > 0 { break; }
                continue;
            }
            if self.return_requested { break; }
        }
        self.loop_depth -= 1;
        self.last_status
    }

    /// Split array elements from a parenthesized assignment
    /// — IronGhost: splits "a b c" into vec!["a", "b", "c"] respecting quotes.
    pub fn split_array_elements(&self, input: &[u8]) -> Vec<Vec<u8>> {
        let mut elements = Vec::new();
        let mut current = Vec::new();
        let mut i = 0;
        let mut in_sq = false;
        let mut in_dq = false;

        while i < input.len() {
            let ch = input[i];
            if ch == b'\'' && !in_dq { in_sq = !in_sq; i += 1; continue; }
            if ch == b'"' && !in_sq { in_dq = !in_dq; i += 1; continue; }
            if (ch == b' ' || ch == b'\t') && !in_sq && !in_dq {
                if !current.is_empty() {
                    elements.push(core::mem::take(&mut current));
                }
                i += 1;
                continue;
            }
            current.push(ch);
            i += 1;
        }
        if !current.is_empty() {
            elements.push(current);
        }
        elements
    }

    /// Set an array by name
    pub fn set_array(&mut self, name: &[u8], elements: Vec<Vec<u8>>) {
        for entry in self.arrays.iter_mut() {
            if entry.0 == name {
                entry.1 = elements;
                return;
            }
        }
        self.arrays.push((name.to_vec(), elements));
    }

    /// Append to an array
    pub fn append_array(&mut self, name: &[u8], elements: Vec<Vec<u8>>) {
        for entry in self.arrays.iter_mut() {
            if entry.0 == name {
                entry.1.extend(elements);
                return;
            }
        }
        self.arrays.push((name.to_vec(), elements));
    }

    /// Set a single array element by index
    pub fn set_array_element(&mut self, name: &[u8], idx: usize, value: Vec<u8>) {
        for entry in self.arrays.iter_mut() {
            if entry.0 == name {
                while entry.1.len() <= idx {
                    entry.1.push(Vec::new());
                }
                entry.1[idx] = value;
                return;
            }
        }
        let mut elements = Vec::new();
        while elements.len() <= idx {
            elements.push(Vec::new());
        }
        elements[idx] = value;
        self.arrays.push((name.to_vec(), elements));
    }

    /// Get an array by name
    pub fn get_array(&self, name: &[u8]) -> Option<&Vec<Vec<u8>>> {
        for entry in &self.arrays {
            if entry.0 == name {
                return Some(&entry.1);
            }
        }
        None
    }

    /// Remove an array element by index
    pub fn unset_array_element(&mut self, name: &[u8], idx: usize) {
        for entry in self.arrays.iter_mut() {
            if entry.0 == name {
                if idx < entry.1.len() {
                    entry.1[idx] = Vec::new();
                }
                return;
            }
        }
    }

    /// Remove an entire array
    pub fn unset_array(&mut self, name: &[u8]) {
        self.arrays.retain(|entry| entry.0 != name);
        self.assoc_arrays.retain(|entry| entry.0 != name);
    }

    /// — IronGhost: set an associative array entry. Creates the array if it doesn't exist.
    pub fn set_assoc(&mut self, name: &[u8], key: &[u8], value: Vec<u8>) {
        for entry in self.assoc_arrays.iter_mut() {
            if entry.0 == name {
                // Update existing key or add new
                for kv in entry.1.iter_mut() {
                    if kv.0 == key {
                        kv.1 = value;
                        return;
                    }
                }
                entry.1.push((key.to_vec(), value));
                return;
            }
        }
        self.assoc_arrays.push((name.to_vec(), alloc::vec![(key.to_vec(), value)]));
    }

    /// — IronGhost: get an associative array value by key.
    pub fn get_assoc(&self, name: &[u8], key: &[u8]) -> Option<&[u8]> {
        for entry in &self.assoc_arrays {
            if entry.0 == name {
                for kv in &entry.1 {
                    if kv.0 == key {
                        return Some(&kv.1);
                    }
                }
                return None;
            }
        }
        None
    }

    /// — IronGhost: get all keys of an associative array.
    pub fn get_assoc_keys(&self, name: &[u8]) -> Vec<Vec<u8>> {
        for entry in &self.assoc_arrays {
            if entry.0 == name {
                return entry.1.iter().map(|kv| kv.0.clone()).collect();
            }
        }
        Vec::new()
    }

    /// — IronGhost: get all values of an associative array.
    pub fn get_assoc_values(&self, name: &[u8]) -> Vec<Vec<u8>> {
        for entry in &self.assoc_arrays {
            if entry.0 == name {
                return entry.1.iter().map(|kv| kv.1.clone()).collect();
            }
        }
        Vec::new()
    }

    /// — IronGhost: get count of associative array entries.
    pub fn get_assoc_count(&self, name: &[u8]) -> usize {
        for entry in &self.assoc_arrays {
            if entry.0 == name {
                return entry.1.len();
            }
        }
        0
    }

    /// — IronGhost: create an empty associative array.
    pub fn create_assoc(&mut self, name: &[u8]) {
        if !self.assoc_arrays.iter().any(|e| e.0 == name) {
            self.assoc_arrays.push((name.to_vec(), Vec::new()));
        }
    }

    /// — IronGhost: check if a name is an associative array.
    pub fn is_assoc(&self, name: &[u8]) -> bool {
        self.assoc_arrays.iter().any(|e| e.0 == name)
    }

    /// Save a variable's current value in the top local frame
    /// — IronGhost: called by the `local` builtin. Stashes the current value
    /// (or None if unset) so it can be restored on function exit.
    pub fn save_local(&mut self, name: &[u8]) {
        if let Some(frame) = self.local_frames.last_mut() {
            // Don't save twice in the same frame
            if frame.iter().any(|(n, _)| n == name) { return; }
            let saved = if let Ok(n) = core::str::from_utf8(name) {
                getenv(n).map(|v| v.as_bytes().to_vec())
            } else {
                None
            };
            frame.push((name.to_vec(), saved));
        }
    }

    /// Fire the EXIT trap if one is set
    /// — ByteRiot: the last will and testament. Runs when the shell exits,
    /// whether by `exit`, EOF, or set -e bailout.
    pub fn fire_exit_trap(&mut self) {
        if let Some(ref cmd) = self.traps[0] {
            let cmd = cmd.clone();
            let tokens = crate::token::tokenize(&cmd);
            if let Ok(prog) = crate::parser::parse(tokens) {
                self.eval_program(&prog);
            }
        }
    }
}

/// Permission check result
/// — ByteRiot: three-state result. "Not found" != "permission denied".
/// Exit code 126 = found but can't execute. 127 = not found at all.
enum ExecCheck {
    Ok,
    NotFound,
    PermissionDenied,
}

/// Check if a file exists and is executable by the current user
/// — ByteRiot: the bouncer at the exec gate. Checks:
/// 1. File exists (stat succeeds)
/// 2. Is a regular file (not a directory)
/// 3. User has execute permission (owner/group/other bits)
fn check_exec_permission(path: &str) -> ExecCheck {
    let mut st = libc::stat::Stat::zeroed();
    if libc::stat::stat(path, &mut st) != 0 {
        return ExecCheck::NotFound;
    }

    // — ByteRiot: must be a regular file. Directories, symlinks to dirs,
    // and special files aren't executable in the traditional sense.
    if !st.is_file() {
        return ExecCheck::PermissionDenied;
    }

    // — ByteRiot: check execute permission bits against our uid/gid.
    // Owner gets S_IXUSR, group members get S_IXGRP, everyone else S_IXOTH.
    let uid = getuid();
    let gid = getgid();
    let mode = st.mode;

    // Root can execute anything with any exec bit set
    if uid == 0 {
        if (mode & 0o111) != 0 { return ExecCheck::Ok; }
        return ExecCheck::PermissionDenied;
    }

    // Check owner
    if st.uid == uid {
        return if (mode & S_IXUSR) != 0 { ExecCheck::Ok } else { ExecCheck::PermissionDenied };
    }

    // Check group — primary gid match
    if st.gid == gid {
        return if (mode & S_IXGRP) != 0 { ExecCheck::Ok } else { ExecCheck::PermissionDenied };
    }

    // — ByteRiot: check supplementary groups. A user can be in multiple
    // groups — the file's gid might match one of our supplementary groups.
    let mut groups = [0u32; 32];
    let ngroups = libc::syscall::sys_getgroups(32, groups.as_mut_ptr());
    if ngroups > 0 {
        for i in 0..ngroups as usize {
            if groups[i] == st.gid {
                return if (mode & S_IXGRP) != 0 { ExecCheck::Ok } else { ExecCheck::PermissionDenied };
            }
        }
    }

    // Other
    if (mode & S_IXOTH) != 0 { ExecCheck::Ok } else { ExecCheck::PermissionDenied }
}

/// Strip leading tabs from each line of heredoc body
fn strip_leading_tabs(body: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(body.len());
    let mut at_line_start = true;
    for &b in body {
        if at_line_start && b == b'\t' { continue; }
        at_line_start = b == b'\n';
        result.push(b);
    }
    result
}

/// Parse a byte slice as i64
fn parse_i64(s: &[u8]) -> i64 {
    let mut i = 0;
    while i < s.len() && s[i] == b' ' { i += 1; }
    let neg = if i < s.len() && s[i] == b'-' { i += 1; true } else { false };
    let mut result: i64 = 0;
    while i < s.len() && s[i] >= b'0' && s[i] <= b'9' {
        result = result * 10 + (s[i] - b'0') as i64;
        i += 1;
    }
    if neg { -result } else { result }
}

/// Convert bytes to str (NUL-terminated safe)
fn bytes_to_str(bytes: &[u8]) -> &str {
    let mut len = 0;
    while len < bytes.len() && bytes[len] != 0 {
        len += 1;
    }
    unsafe { core::str::from_utf8_unchecked(&bytes[..len]) }
}

/// Print bytes to stderr
fn print_bytes(s: &[u8]) {
    let mut i = 0;
    while i < s.len() && s[i] != 0 {
        putchar(s[i]);
        i += 1;
    }
}

/// Print bytes to stderr (for xtrace)
/// — ThreadRogue: like print_bytes but uses stderr fd directly
fn prints_bytes(s: &[u8]) {
    let mut end = s.len();
    for i in 0..s.len() {
        if s[i] == 0 { end = i; break; }
    }
    libc::write(2, &s[..end]);
}

/// Print i64 to stderr
fn print_i64_stderr(n: i64) {
    let mut buf = [0u8; 20];
    let mut val = if n < 0 { -(n as i64) as u64 } else { n as u64 };
    let mut len = 0;
    if val == 0 { buf[0] = b'0'; len = 1; }
    else {
        while val > 0 {
            buf[len] = b'0' + (val % 10) as u8;
            val /= 10;
            len += 1;
        }
        if n < 0 { buf[len] = b'-'; len += 1; }
        buf[..len].reverse();
    }
    libc::write(2, &buf[..len]);
}

/// Format usize into buffer, return bytes written
fn format_usize(mut n: usize, buf: &mut [u8]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut digits = [0u8; 10];
    let mut len = 0;
    while n > 0 {
        digits[len] = b'0' + (n % 10) as u8;
        n /= 10;
        len += 1;
    }
    for i in 0..len {
        buf[i] = digits[len - 1 - i];
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::tokenize;
    use crate::parser::parse;

    #[test]
    fn test_eval_simple_true() {
        let prog = parse(tokenize(b"true")).unwrap();
        let mut eval = Evaluator::new();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 0);
    }

    #[test]
    fn test_eval_simple_false() {
        let prog = parse(tokenize(b"false")).unwrap();
        let mut eval = Evaluator::new();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 1);
    }

    #[test]
    fn test_eval_and_short_circuit() {
        // false && echo nope — should NOT run echo
        let prog = parse(tokenize(b"false && true")).unwrap();
        let mut eval = Evaluator::new();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 1);
    }

    #[test]
    fn test_eval_or_short_circuit() {
        // true || false — should NOT run false
        let prog = parse(tokenize(b"true || false")).unwrap();
        let mut eval = Evaluator::new();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 0);
    }

    #[test]
    fn test_eval_assignment() {
        let prog = parse(tokenize(b"FOO_TEST_EVAL=hello")).unwrap();
        let mut eval = Evaluator::new();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 0);
        // Variable should be set
        assert_eq!(getenv("FOO_TEST_EVAL"), Some("hello"));
    }

    #[test]
    fn test_eval_semicolons() {
        let prog = parse(tokenize(b"true; true; false")).unwrap();
        let mut eval = Evaluator::new();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 1);
    }

    #[test]
    fn test_function_define_and_call() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"function myfunc { true; }")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.functions.len(), 1);
        assert_eq!(eval.functions[0].0, b"myfunc");
    }

    // — DeadLoop: comprehensive evaluator tests

    #[test]
    fn test_function_call_returns_status() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"ok() { true; }; ok")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 0);
    }

    #[test]
    fn test_function_call_false() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"fail() { false; }; fail")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 1);
    }

    #[test]
    fn test_function_parens_syntax() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"myfunc() { true; }")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.functions.len(), 1);
    }

    #[test]
    fn test_function_redefine() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"f() { true; }; f() { false; }; f")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 1);
        assert_eq!(eval.functions.len(), 1);
    }

    #[test]
    fn test_pipeline_negation_true() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"! false")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 0);
    }

    #[test]
    fn test_pipeline_negation_false() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"! true")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 1);
    }

    #[test]
    fn test_break_in_for() {
        let mut eval = Evaluator::new();
        // for i in 1 2 3; do if true; then break; fi; done
        // After loop, break_count should be 0
        let prog = parse(tokenize(b"for x in 1 2 3; do break; done")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.break_count, 0);
        assert_eq!(eval.loop_depth, 0);
    }

    #[test]
    fn test_continue_in_for() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"for x in 1 2 3; do continue; done")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.continue_count, 0);
        assert_eq!(eval.loop_depth, 0);
    }

    #[test]
    fn test_return_outside_function() {
        let mut eval = Evaluator::new();
        // return outside function should not set return_requested
        let prog = parse(tokenize(b"return")).unwrap();
        eval.eval_program(&prog);
        assert!(!eval.return_requested);
    }

    #[test]
    fn test_case_match() {
        let mut eval = Evaluator::new();
        setenv("CASE_TEST_VAR", "hello");
        let prog = parse(tokenize(b"case hello in hello) true;; *) false;; esac")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 0);
    }

    #[test]
    fn test_case_default() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"case xyz in hello) false;; *) true;; esac")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 0);
    }

    #[test]
    fn test_case_no_match() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"case xyz in hello) false;; esac")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 0);
    }

    #[test]
    fn test_case_glob_pattern() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"case test.c in *.rs) false;; *.c) true;; esac")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 0);
    }

    #[test]
    fn test_set_errexit() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"set -e")).unwrap();
        eval.eval_program(&prog);
        assert!(eval.opts.errexit);
    }

    #[test]
    fn test_set_xtrace() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"set -x")).unwrap();
        eval.eval_program(&prog);
        assert!(eval.opts.xtrace);
    }

    #[test]
    fn test_set_nounset() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"set -u")).unwrap();
        eval.eval_program(&prog);
        assert!(eval.opts.nounset);
    }

    #[test]
    fn test_set_pipefail() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"set -o pipefail")).unwrap();
        eval.eval_program(&prog);
        assert!(eval.opts.pipefail);
    }

    #[test]
    fn test_set_disable_options() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"set -e; set +e")).unwrap();
        eval.eval_program(&prog);
        assert!(!eval.opts.errexit);
    }

    #[test]
    fn test_set_positional_params() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"set -- a b c")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.positional.len(), 3);
        assert_eq!(eval.positional[0], b"a");
        assert_eq!(eval.positional[1], b"b");
        assert_eq!(eval.positional[2], b"c");
    }

    #[test]
    fn test_if_true_branch() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"if true; then true; else false; fi")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 0);
    }

    #[test]
    fn test_if_false_branch() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"if false; then true; else false; fi")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 1);
    }

    #[test]
    fn test_while_runs_body() {
        let mut eval = Evaluator::new();
        // while false runs body 0 times — status is from condition
        let prog = parse(tokenize(b"while false; do true; done")).unwrap();
        eval.eval_program(&prog);
        // last_status is from the condition check that failed
        assert_eq!(eval.last_status, 1);
    }

    #[test]
    fn test_group_command() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"{ true; false; }")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 1);
    }

    #[test]
    fn test_strip_leading_tabs() {
        let result = strip_leading_tabs(b"\thello\n\t\tworld\n");
        assert_eq!(result, b"hello\nworld\n");
    }

    #[test]
    fn test_strip_leading_tabs_no_tabs() {
        let result = strip_leading_tabs(b"hello\nworld\n");
        assert_eq!(result, b"hello\nworld\n");
    }

    #[test]
    fn test_parse_i64_basic() {
        assert_eq!(parse_i64(b"42"), 42);
        assert_eq!(parse_i64(b"-7"), -7);
        assert_eq!(parse_i64(b"0"), 0);
        assert_eq!(parse_i64(b"  123"), 123);
    }

    #[test]
    fn test_trap_storage() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"trap 'echo bye' EXIT")).unwrap();
        eval.eval_program(&prog);
        assert!(eval.traps[0].is_some());
        assert_eq!(eval.traps[0].as_ref().unwrap(), b"echo bye");
    }

    #[test]
    fn test_trap_reset() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"trap 'echo bye' EXIT; trap - EXIT")).unwrap();
        eval.eval_program(&prog);
        assert!(eval.traps[0].is_none());
    }

    #[test]
    fn test_and_or_chain() {
        let mut eval = Evaluator::new();
        // false || true && true — should succeed
        let prog = parse(tokenize(b"false || true && true")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 0);
    }

    #[test]
    fn test_and_or_chain_fail() {
        let mut eval = Evaluator::new();
        // true && false || false — should fail
        let prog = parse(tokenize(b"true && false || false")).unwrap();
        eval.eval_program(&prog);
        assert_eq!(eval.last_status, 1);
    }

    #[test]
    fn test_function_lookup() {
        let mut eval = Evaluator::new();
        let prog = parse(tokenize(b"myfn() { true; }")).unwrap();
        eval.eval_program(&prog);
        assert!(eval.lookup_function(b"myfn").is_some());
        assert!(eval.lookup_function(b"nonexistent").is_none());
    }

    #[test]
    fn test_errexit_in_condition_suppressed() {
        let mut eval = Evaluator::new();
        // set -e; if false; then ... fi — should NOT exit shell
        let prog = parse(tokenize(b"set -e; if false; then true; fi; true")).unwrap();
        eval.eval_program(&prog);
        // Shell should still be alive, last command was true
        assert_eq!(eval.last_status, 0);
    }
}
