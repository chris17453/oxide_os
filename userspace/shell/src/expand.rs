//! Shell Expander — variable, command, tilde, and glob expansion
//!
//! — ByteRiot: the shape-shifter. Takes raw AST words and expands them
//! into their final form. $HOME becomes /home/user, $(whoami) becomes root,
//! ~ becomes $HOME, and * becomes every file that matches.
//!
//! Expansion order (POSIX):
//!   1. Tilde expansion (~, ~user)
//!   2. Parameter/variable expansion ($VAR, ${VAR}, $?, $$, $#, $0-$9)
//!   3. Command substitution $(cmd) and `cmd`
//!   4. Field splitting (IFS)
//!   5. Pathname expansion (glob: *, ?, [])
//!   6. Quote removal

extern crate alloc;
use alloc::vec::Vec;
use libc::*;
use libc::dirent::{opendir, readdir, closedir};

/// Maximum expansion buffer size
const MAX_EXPAND: usize = 4096;

/// Shell state access (for $?, $$, positional params, arrays)
/// — ByteRiot: reaching into the shell's guts for runtime state.
/// The evaluator passes these in so we don't couple to global statics.
pub struct ExpandContext<'a> {
    pub last_status: i32,
    pub pid: i32,
    pub positional: Vec<Vec<u8>>,
    /// -u: error on unset variable expansion
    pub nounset: bool,
    /// Array storage reference — borrowed from evaluator
    /// — IronGhost: arrays live in the evaluator, but the expander needs to read them
    /// for ${arr[0]}, ${arr[@]}, ${#arr[@]}, etc.
    pub arrays: &'a Vec<(Vec<u8>, Vec<Vec<u8>>)>,
    /// — IronGhost: associative arrays (declare -A). Borrowed from evaluator.
    pub assoc_arrays: &'a Vec<(Vec<u8>, Vec<(Vec<u8>, Vec<u8>)>)>,
}

/// Expand a single word (all expansion phases)
/// — ByteRiot: the full pipeline. One word in, possibly many words out
/// (field splitting can produce multiple).
pub fn expand_word(word: &[u8], ctx: &ExpandContext<'_>) -> Vec<Vec<u8>> {
    // — ByteRiot: phase 0 — brace expansion. Must happen BEFORE everything
    // else because {a,b,c} creates multiple words from one.
    let braced = expand_braces(word);
    if braced.len() > 1 {
        let mut result = Vec::new();
        for w in &braced {
            result.extend(expand_word(w, ctx));
        }
        return result;
    }

    // — IronGhost: process substitution — <(cmd) or >(cmd)
    // Fork a child, run the command, write output to a temp file,
    // return the temp file path as the expanded word.
    if word.len() >= 3 && (word[0] == b'<' || word[0] == b'>') && word[1] == b'(' {
        if word.last() == Some(&b')') {
            let cmd = &word[2..word.len() - 1];
            let path = process_substitution(cmd, word[0] == b'<');
            return alloc::vec![path];
        }
    }

    // Phase 1: tilde expansion
    let tilded = expand_tilde(word);

    // Phase 2+3: variable + command substitution + arithmetic
    let expanded = expand_vars_and_cmdsub(&tilded, ctx);

    // Phase 4: field splitting on IFS
    let fields = field_split(&expanded);

    // Phase 5: glob expansion on each field
    let mut result = Vec::new();
    for field in fields {
        let globbed = expand_glob(&field);
        if globbed.is_empty() {
            // No glob match — keep original (POSIX behavior)
            result.push(field);
        } else {
            result.extend(globbed);
        }
    }

    if result.is_empty() && !word.is_empty() {
        result.push(expanded);
    }

    result
}

/// Expand a word but don't field-split or glob (for assignments, here-docs)
pub fn expand_word_nosplit(word: &[u8], ctx: &ExpandContext<'_>) -> Vec<u8> {
    let tilded = expand_tilde(word);
    expand_vars_and_cmdsub(&tilded, ctx)
}

/// Tilde expansion: ~ → $HOME, ~/foo → $HOME/foo
fn expand_tilde(word: &[u8]) -> Vec<u8> {
    if word.is_empty() || word[0] != b'~' {
        return word.to_vec();
    }

    // ~ alone or ~/...
    if word.len() == 1 || word[1] == b'/' {
        if let Some(home) = getenv("HOME") {
            let mut result = Vec::from(home.as_bytes());
            if word.len() > 1 {
                result.extend_from_slice(&word[1..]);
            }
            return result;
        }
    }

    word.to_vec()
}

/// Variable expansion + command substitution
/// — ByteRiot: the meat of the expander. Walks the word byte-by-byte,
/// handling $VAR, ${VAR}, $?, $$, $#, $0-$9, $(cmd), and quoting.
fn expand_vars_and_cmdsub(input: &[u8], ctx: &ExpandContext<'_>) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() * 2);
    let mut i = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while i < input.len() {
        let ch = input[i];

        // Single quote toggle (not inside double quotes)
        if ch == b'\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            i += 1;
            continue;
        }

        // Double quote toggle (not inside single quotes)
        if ch == b'"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            i += 1;
            continue;
        }

        // Backslash escape
        if ch == b'\\' && !in_single_quote && i + 1 < input.len() {
            if in_double_quote {
                // Inside double quotes: only \$, \`, \\, \", \newline are special
                let next = input[i + 1];
                if next == b'$' || next == b'`' || next == b'\\' || next == b'"' || next == b'\n' {
                    out.push(next);
                    i += 2;
                    continue;
                }
            } else {
                // Outside quotes: escape next char
                out.push(input[i + 1]);
                i += 2;
                continue;
            }
        }

        // No expansion inside single quotes
        if in_single_quote {
            out.push(ch);
            i += 1;
            continue;
        }

        // $ expansion
        if ch == b'$' && i + 1 < input.len() {
            i += 1;
            let next = input[i];

            match next {
                b'?' => {
                    // $? — last exit status
                    append_i32(&mut out, ctx.last_status);
                    i += 1;
                }
                b'$' => {
                    // $$ — PID
                    append_i32(&mut out, ctx.pid);
                    i += 1;
                }
                b'#' => {
                    // $# — number of positional parameters
                    append_i32(&mut out, ctx.positional.len() as i32);
                    i += 1;
                }
                b'0' => {
                    // $0 — shell/script name (not in positional array)
                    // — ByteRiot: $0 is always the shell name for interactive shells.
                    // For scripts via -c or source, it's the script path.
                    // Stored separately — not in the positional params array.
                    out.extend_from_slice(b"esh");
                    i += 1;
                }
                b'1'..=b'9' => {
                    // $1-$9 — positional parameters (1-indexed: $1 = positional[0])
                    let idx = (next - b'1') as usize;
                    if idx < ctx.positional.len() {
                        out.extend_from_slice(&ctx.positional[idx]);
                    }
                    i += 1;
                }
                b'@' | b'*' => {
                    // $@ / $* — all positional parameters
                    for (j, param) in ctx.positional.iter().enumerate() {
                        if j > 0 { out.push(b' '); }
                        out.extend_from_slice(param);
                    }
                    i += 1;
                }
                b'{' => {
                    // ${VAR} — braced variable, with optional modifiers
                    i += 1; // skip {
                    let var_start = i;
                    let mut depth = 1;
                    while i < input.len() && depth > 0 {
                        if input[i] == b'{' { depth += 1; }
                        if input[i] == b'}' { depth -= 1; }
                        if depth > 0 { i += 1; }
                    }
                    let var_expr = &input[var_start..i];
                    if i < input.len() { i += 1; } // skip }

                    // — ByteRiot: handle ${VAR:-default}, ${VAR:+alt}, ${VAR:=assign}
                    if let Some(val) = expand_braced_var(var_expr, ctx) {
                        out.extend_from_slice(&val);
                    }
                }
                b'(' => {
                    // — ByteRiot: disambiguate $(( arithmetic )) from $( command )
                    if i + 1 < input.len() && input[i + 1] == b'(' {
                        // $(( expr )) — arithmetic expansion
                        i += 2; // skip ((
                        let expr_start = i;
                        let mut depth = 1;
                        while i < input.len() && depth > 0 {
                            if i + 1 < input.len() && input[i] == b'(' && input[i + 1] == b'(' {
                                depth += 1; i += 2;
                            } else if i + 1 < input.len() && input[i] == b')' && input[i + 1] == b')' {
                                depth -= 1;
                                if depth > 0 { i += 2; }
                            } else {
                                i += 1;
                            }
                        }
                        let expr = &input[expr_start..i];
                        if i + 1 < input.len() { i += 2; } // skip ))
                        else if i < input.len() { i += 1; }

                        // Expand variables in the expression first
                        let expanded_expr = expand_vars_and_cmdsub(expr, ctx);
                        let val = eval_arith_expr(&expanded_expr);
                        append_i64(&mut out, val);
                    } else {
                        // $(cmd) — command substitution
                        i += 1; // skip (
                        let cmd_start = i;
                        let mut depth = 1;
                        while i < input.len() && depth > 0 {
                            if input[i] == b'(' { depth += 1; }
                            if input[i] == b')' { depth -= 1; }
                            if depth > 0 { i += 1; }
                        }
                        let cmd = &input[cmd_start..i];
                        if i < input.len() { i += 1; } // skip )

                        let captured = command_substitution(cmd);
                        out.extend_from_slice(&captured);
                    }
                }
                _ if next.is_ascii_alphabetic() || next == b'_' => {
                    // $VAR — unbraced variable name
                    let var_start = i;
                    while i < input.len() && (input[i].is_ascii_alphanumeric() || input[i] == b'_') {
                        i += 1;
                    }
                    let var_name = &input[var_start..i];
                    if let Some(val) = getenv_bytes(var_name) {
                        out.extend_from_slice(val);
                    } else if ctx.nounset {
                        // — ByteRiot: set -u says unset vars are fatal. You asked for strict mode,
                        // now live with it.
                        eprints("esh: ");
                        print_bytes_stderr(var_name);
                        eprintlns(": unbound variable");
                    }
                }
                _ => {
                    // Literal $
                    out.push(b'$');
                    // Don't consume next — let it be processed normally
                }
            }
            continue;
        }

        // Backtick command substitution `cmd`
        if ch == b'`' && !in_single_quote {
            i += 1;
            let cmd_start = i;
            while i < input.len() && input[i] != b'`' {
                i += 1;
            }
            let cmd = &input[cmd_start..i];
            if i < input.len() { i += 1; } // skip closing `

            let captured = command_substitution(cmd);
            out.extend_from_slice(&captured);
            continue;
        }

        // Regular character
        out.push(ch);
        i += 1;
    }

    out
}

/// Expand ${VAR} with optional modifiers and string manipulation
/// — ByteRiot: the swiss army knife of parameter expansion. Supports:
/// ${VAR:-default}, ${VAR:+alt}, ${VAR:=assign}, ${VAR:?error}
/// ${#VAR} — string length
/// ${VAR#pat} / ${VAR##pat} — strip shortest/longest prefix
/// ${VAR%pat} / ${VAR%%pat} — strip shortest/longest suffix
/// ${VAR/old/new} / ${VAR//old/new} — replace first/all
/// ${VAR:offset} / ${VAR:offset:length} — substring
/// ${VAR^} / ${VAR^^} — uppercase first/all
/// ${VAR,} / ${VAR,,} — lowercase first/all
fn expand_braced_var(expr: &[u8], ctx: &ExpandContext<'_>) -> Option<Vec<u8>> {
    if expr.is_empty() { return Some(Vec::new()); }

    // — IronGhost: ${#arr[@]} — array length (indexed or associative)
    if expr[0] == b'#' && expr.len() > 1 {
        let inner = &expr[1..];
        if let Some(bracket_pos) = inner.iter().position(|&b| b == b'[') {
            let arr_name = &inner[..bracket_pos];
            let subscript = &inner[bracket_pos + 1..];
            if subscript == b"@]" || subscript == b"*]" {
                // Check associative arrays first
                for (name, entries) in ctx.assoc_arrays.iter() {
                    if name.as_slice() == arr_name {
                        let mut out = Vec::new();
                        append_i32(&mut out, entries.len() as i32);
                        return Some(out);
                    }
                }
                // Then indexed arrays
                for (name, elements) in ctx.arrays.iter() {
                    if name.as_slice() == arr_name {
                        let mut out = Vec::new();
                        append_i32(&mut out, elements.len() as i32);
                        return Some(out);
                    }
                }
                let mut out = Vec::new();
                append_i32(&mut out, 0);
                return Some(out);
            }
        }
    }

    // — IronGhost: ${!arr[@]} — array keys (indexed: indices, associative: keys)
    if expr[0] == b'!' && expr.len() > 1 {
        let inner = &expr[1..];
        if let Some(bracket_pos) = inner.iter().position(|&b| b == b'[') {
            let arr_name = &inner[..bracket_pos];
            let subscript = &inner[bracket_pos + 1..];
            if subscript == b"@]" || subscript == b"*]" {
                // Check associative arrays
                for (name, entries) in ctx.assoc_arrays.iter() {
                    if name.as_slice() == arr_name {
                        let mut out = Vec::new();
                        for (j, (key, _)) in entries.iter().enumerate() {
                            if j > 0 { out.push(b' '); }
                            out.extend_from_slice(key);
                        }
                        return Some(out);
                    }
                }
                // Indexed arrays: keys are indices
                for (name, elements) in ctx.arrays.iter() {
                    if name.as_slice() == arr_name {
                        let mut out = Vec::new();
                        for (j, _) in elements.iter().enumerate() {
                            if j > 0 { out.push(b' '); }
                            append_i32(&mut out, j as i32);
                        }
                        return Some(out);
                    }
                }
                return Some(Vec::new());
            }
        }
    }

    // — IronGhost: ${arr[idx]}, ${arr[@]}, ${arr[*]}, ${arr[@]:offset:length}
    // Detect array access by looking for name[
    {
        let mut var_end = 0;
        while var_end < expr.len() && (expr[var_end].is_ascii_alphanumeric() || expr[var_end] == b'_') {
            var_end += 1;
        }
        if var_end < expr.len() && expr[var_end] == b'[' {
            let arr_name = &expr[..var_end];
            let after_bracket = &expr[var_end + 1..];
            // Find closing ]
            if let Some(close_pos) = after_bracket.iter().position(|&b| b == b']') {
                let subscript = &after_bracket[..close_pos];
                let after_close = &after_bracket[close_pos + 1..];

                // — IronGhost: check associative arrays first, then indexed.
                // Associative arrays use string keys, indexed use numeric indices.
                let assoc = ctx.assoc_arrays.iter()
                    .find(|(name, _)| name.as_slice() == arr_name);

                if let Some((_, entries)) = assoc {
                    if subscript == b"@" || subscript == b"*" {
                        let sep = if subscript == b"*" {
                            let ifs = getenv("IFS").map(|s| s.as_bytes()).unwrap_or(b" ");
                            if !ifs.is_empty() { ifs[0] } else { b' ' }
                        } else { b' ' };
                        let mut out = Vec::new();
                        for (j, (_, val)) in entries.iter().enumerate() {
                            if j > 0 { out.push(sep); }
                            out.extend_from_slice(val);
                        }
                        return Some(out);
                    } else {
                        // ${assoc[key]} — string key lookup
                        for (key, val) in entries {
                            if key.as_slice() == subscript {
                                return Some(val.clone());
                            }
                        }
                        return Some(Vec::new());
                    }
                }

                // Look up indexed array
                let elements = ctx.arrays.iter()
                    .find(|(name, _)| name.as_slice() == arr_name)
                    .map(|(_, e)| e.as_slice());

                if subscript == b"@" || subscript == b"*" {
                    // ${arr[@]} or ${arr[*]}
                    let elems = elements.unwrap_or(&[]);

                    // — IronGhost: ${arr[@]:offset:length} — array slice
                    if after_close.starts_with(b":") {
                        let slice_spec = &after_close[1..];
                        let total = elems.len() as i64;
                        if let Some(colon2) = slice_spec.iter().position(|&b| b == b':') {
                            let offset = parse_slice_num(&slice_spec[..colon2]);
                            let length = parse_slice_num(&slice_spec[colon2 + 1..]);
                            let start = if offset < 0 { (total + offset).max(0) as usize } else { (offset as usize).min(elems.len()) };
                            let end = (start + length as usize).min(elems.len());
                            let mut out = Vec::new();
                            for (j, elem) in elems[start..end].iter().enumerate() {
                                if j > 0 { out.push(b' '); }
                                out.extend_from_slice(elem);
                            }
                            return Some(out);
                        } else {
                            let offset = parse_slice_num(slice_spec);
                            let start = if offset < 0 { (total + offset).max(0) as usize } else { (offset as usize).min(elems.len()) };
                            let mut out = Vec::new();
                            for (j, elem) in elems[start..].iter().enumerate() {
                                if j > 0 { out.push(b' '); }
                                out.extend_from_slice(elem);
                            }
                            return Some(out);
                        }
                    }

                    let sep = if subscript == b"*" {
                        // ${arr[*]} — IFS-joined as single word
                        let ifs = getenv("IFS").map(|s| s.as_bytes()).unwrap_or(b" ");
                        if !ifs.is_empty() { ifs[0] } else { b' ' }
                    } else {
                        b' ' // ${arr[@]} — space-separated
                    };
                    let mut out = Vec::new();
                    for (j, elem) in elems.iter().enumerate() {
                        if j > 0 { out.push(sep); }
                        out.extend_from_slice(elem);
                    }
                    return Some(out);
                } else {
                    // ${arr[N]} — index access
                    let idx = parse_slice_num(subscript) as usize;
                    if let Some(elems) = elements {
                        if idx < elems.len() {
                            return Some(elems[idx].clone());
                        }
                    }
                    return Some(Vec::new());
                }
            }
        }
    }

    // — ByteRiot: ${#VAR} — string length. The # must be first and NOT
    // followed by another # (that would be ${##pat} which is different)
    if expr[0] == b'#' && expr.len() > 1 {
        let var_name = &expr[1..];
        // Make sure it's not a modifier like ${var#pat}
        if !var_name.iter().any(|&b| b == b'#' || b == b'%' || b == b'/' || b == b':') {
            let val = getenv_bytes(var_name).unwrap_or(&[]);
            let len = val.len();
            let mut out = Vec::new();
            append_i32(&mut out, len as i32);
            return Some(out);
        }
    }

    // Find the variable name part (before any operator)
    let mut var_end = 0;
    while var_end < expr.len() && (expr[var_end].is_ascii_alphanumeric() || expr[var_end] == b'_') {
        var_end += 1;
    }
    let var_name = &expr[..var_end];
    let rest = &expr[var_end..];

    if rest.is_empty() {
        // Simple ${VAR}
        return Some(getenv_bytes(var_name).unwrap_or(&[]).to_vec());
    }

    let val = getenv_bytes(var_name).unwrap_or(&[]);

    // — ByteRiot: string manipulation operators
    match rest[0] {
        b'#' => {
            // ${VAR#pat} — strip shortest prefix; ${VAR##pat} — strip longest
            let longest = rest.len() > 1 && rest[1] == b'#';
            let pat = if longest { &rest[2..] } else { &rest[1..] };
            return Some(strip_prefix(val, pat, longest));
        }
        b'%' => {
            // ${VAR%pat} — strip shortest suffix; ${VAR%%pat} — strip longest
            let longest = rest.len() > 1 && rest[1] == b'%';
            let pat = if longest { &rest[2..] } else { &rest[1..] };
            return Some(strip_suffix(val, pat, longest));
        }
        b'/' => {
            // ${VAR/old/new} — replace first; ${VAR//old/new} — replace all
            let replace_all = rest.len() > 1 && rest[1] == b'/';
            let search_start = if replace_all { 2 } else { 1 };
            let search_rest = &rest[search_start..];
            // Find the / separator between old and new
            if let Some(sep) = search_rest.iter().position(|&b| b == b'/') {
                let old = &search_rest[..sep];
                let new = &search_rest[sep + 1..];
                return Some(string_replace(val, old, new, replace_all));
            } else {
                // ${VAR/old} — delete occurrences
                return Some(string_replace(val, search_rest, &[], replace_all));
            }
        }
        b':' => {
            if rest.len() > 1 {
                match rest[1] {
                    b'-' | b'+' | b'=' | b'?' => {
                        // ${VAR:-default}, ${VAR:+alt}, ${VAR:=assign}, ${VAR:?error}
                        let op = rest[1];
                        let default_val = &rest[2..];
                        let is_unset_or_null = val.is_empty();
                        return match op {
                            b'-' => Some(if is_unset_or_null { default_val.to_vec() } else { val.to_vec() }),
                            b'+' => Some(if is_unset_or_null { Vec::new() } else { default_val.to_vec() }),
                            b'=' => {
                                if is_unset_or_null {
                                    if let Ok(n) = core::str::from_utf8(var_name) {
                                        if let Ok(v) = core::str::from_utf8(default_val) {
                                            setenv(n, v);
                                        }
                                    }
                                    Some(default_val.to_vec())
                                } else {
                                    Some(val.to_vec())
                                }
                            }
                            b'?' => {
                                if is_unset_or_null {
                                    eprints("esh: ");
                                    print_bytes_stderr(var_name);
                                    eprints(": ");
                                    if default_val.is_empty() {
                                        eprintlns("parameter null or not set");
                                    } else {
                                        print_bytes_stderr(default_val);
                                        eprintlns("");
                                    }
                                    Some(Vec::new())
                                } else {
                                    Some(val.to_vec())
                                }
                            }
                            _ => Some(val.to_vec()),
                        };
                    }
                    _ => {
                        // ${VAR:offset} or ${VAR:offset:length} — substring
                        let substr_spec = &rest[1..];
                        if let Some(colon2) = substr_spec.iter().position(|&b| b == b':') {
                            let offset = parse_substr_num(&substr_spec[..colon2]);
                            let length = parse_substr_num(&substr_spec[colon2 + 1..]);
                            return Some(substring(val, offset, Some(length)));
                        } else {
                            let offset = parse_substr_num(substr_spec);
                            return Some(substring(val, offset, None));
                        }
                    }
                }
            }
        }
        b'-' | b'+' | b'=' => {
            // Without colon: ${VAR-default} etc — only triggers on truly unset
            let op = rest[0];
            let default_val = &rest[1..];
            let is_unset = getenv_bytes(var_name).is_none();
            return match op {
                b'-' => Some(if is_unset { default_val.to_vec() } else { val.to_vec() }),
                b'+' => Some(if is_unset { Vec::new() } else { default_val.to_vec() }),
                b'=' => {
                    if is_unset {
                        if let Ok(n) = core::str::from_utf8(var_name) {
                            if let Ok(v) = core::str::from_utf8(default_val) {
                                setenv(n, v);
                            }
                        }
                        Some(default_val.to_vec())
                    } else {
                        Some(val.to_vec())
                    }
                }
                _ => Some(val.to_vec()),
            };
        }
        b'^' => {
            // ${VAR^} — uppercase first; ${VAR^^} — uppercase all
            let all = rest.len() > 1 && rest[1] == b'^';
            return Some(case_transform(val, true, all));
        }
        b',' => {
            // ${VAR,} — lowercase first; ${VAR,,} — lowercase all
            let all = rest.len() > 1 && rest[1] == b',';
            return Some(case_transform(val, false, all));
        }
        _ => {}
    }

    // Fallback: just return the variable value
    Some(val.to_vec())
}

/// Strip matching prefix from value using glob pattern
fn strip_prefix(val: &[u8], pattern: &[u8], longest: bool) -> Vec<u8> {
    if longest {
        // Try from longest prefix down
        for len in (0..=val.len()).rev() {
            if glob_match(pattern, &val[..len]) {
                return val[len..].to_vec();
            }
        }
    } else {
        // Try from shortest prefix up
        for len in 0..=val.len() {
            if glob_match(pattern, &val[..len]) {
                return val[len..].to_vec();
            }
        }
    }
    val.to_vec()
}

/// Strip matching suffix from value using glob pattern
fn strip_suffix(val: &[u8], pattern: &[u8], longest: bool) -> Vec<u8> {
    if longest {
        // Try from longest suffix down
        for start in 0..=val.len() {
            if glob_match(pattern, &val[start..]) {
                return val[..start].to_vec();
            }
        }
    } else {
        // Try from shortest suffix up
        for start in (0..=val.len()).rev() {
            if glob_match(pattern, &val[start..]) {
                return val[..start].to_vec();
            }
        }
    }
    val.to_vec()
}

/// Replace occurrences of old with new in value
fn string_replace(val: &[u8], old: &[u8], new: &[u8], all: bool) -> Vec<u8> {
    if old.is_empty() { return val.to_vec(); }
    let mut result = Vec::new();
    let mut i = 0;
    let mut replaced = false;
    while i < val.len() {
        if i + old.len() <= val.len() && &val[i..i + old.len()] == old {
            result.extend_from_slice(new);
            i += old.len();
            replaced = true;
            if !all { // Only replace first
                result.extend_from_slice(&val[i..]);
                return result;
            }
        } else {
            result.push(val[i]);
            i += 1;
        }
    }
    result
}

/// Substring extraction: val[offset..offset+length]
fn substring(val: &[u8], offset: i64, length: Option<i64>) -> Vec<u8> {
    let len = val.len() as i64;
    let start = if offset < 0 { (len + offset).max(0) as usize } else { (offset as usize).min(val.len()) };
    match length {
        Some(l) if l >= 0 => {
            let end = (start + l as usize).min(val.len());
            val[start..end].to_vec()
        }
        Some(l) => {
            // Negative length: count from end
            let end = (len + l).max(start as i64) as usize;
            val[start..end].to_vec()
        }
        None => val[start..].to_vec(),
    }
}

/// Case transformation: uppercase or lowercase, first char or all
fn case_transform(val: &[u8], upper: bool, all: bool) -> Vec<u8> {
    let mut result = val.to_vec();
    for (i, b) in result.iter_mut().enumerate() {
        if upper {
            if b.is_ascii_lowercase() { *b = b.to_ascii_uppercase(); }
        } else {
            if b.is_ascii_uppercase() { *b = b.to_ascii_lowercase(); }
        }
        if !all { break; }
    }
    result
}

/// Parse a number for array slice offset/length
fn parse_slice_num(s: &[u8]) -> i64 {
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

/// Parse a number for substring offset/length
fn parse_substr_num(s: &[u8]) -> i64 {
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

/// Print bytes to stderr
fn print_bytes_stderr(s: &[u8]) {
    for &b in s { if b == 0 { break; } libc::write(2, &[b]); }
}

/// Command substitution: fork, exec, capture stdout
/// — ByteRiot: the inception moment — a shell within a shell.
fn command_substitution(cmd: &[u8]) -> Vec<u8> {
    if cmd.is_empty() { return Vec::new(); }

    let mut pipefd = [0i32; 2];
    if pipe(&mut pipefd) < 0 {
        return Vec::new();
    }

    let pid = fork();
    if pid == 0 {
        // Child: redirect stdout to pipe, exec command via /bin/esh -c
        close(pipefd[0]);
        dup2(pipefd[1], 1);
        close(pipefd[1]);

        // Build command string for exec
        let mut cmd_str = [0u8; 512];
        let len = cmd.len().min(510);
        cmd_str[..len].copy_from_slice(&cmd[..len]);
        cmd_str[len] = 0;

        // exec esh -c "cmd"
        let esh_path = b"/bin/esh\0";
        let c_flag = b"-c\0";
        let argv: [*const u8; 4] = [
            esh_path.as_ptr(),
            c_flag.as_ptr(),
            cmd_str.as_ptr(),
            core::ptr::null(),
        ];
        let path_str = bytes_to_str(esh_path);
        execv(path_str, argv.as_ptr());
        _exit(127);
    } else if pid > 0 {
        // Parent: read from pipe
        close(pipefd[1]);
        let mut result = Vec::new();
        let mut buf = [0u8; 512];
        loop {
            let n = read(pipefd[0], &mut buf);
            if n <= 0 { break; }
            result.extend_from_slice(&buf[..n as usize]);
        }
        close(pipefd[0]);
        let mut status = 0;
        waitpid(pid, &mut status, 0);

        // Strip trailing newlines (POSIX)
        while result.last() == Some(&b'\n') {
            result.pop();
        }
        return result;
    }

    // Fork failed
    close(pipefd[0]);
    close(pipefd[1]);
    Vec::new()
}

/// Process substitution: <(cmd) → run cmd, capture to temp file, return path
/// — IronGhost: the temp file gambit. Since OXIDE OS lacks /dev/fd, we write
/// command output to /tmp/esh_procsub_NNNN and return that path. The file gets
/// cleaned up... eventually. Good enough for `diff <(cmd1) <(cmd2)`.
/// — FuzzStatic: process substitution temp file cleanup list.
/// Every temp file created by <(cmd) or >(cmd) gets registered here.
/// Call cleanup_process_substitution() to unlink them all.
static mut PROCSUB_PATHS: Option<Vec<Vec<u8>>> = None;

/// — FuzzStatic: register a process substitution temp file for cleanup.
fn procsub_register(path: &[u8]) {
    unsafe {
        use core::ptr::addr_of_mut;
        let ptr = addr_of_mut!(PROCSUB_PATHS);
        if (*ptr).is_none() {
            *ptr = Some(Vec::new());
        }
        if let Some(v) = &mut *ptr {
            v.push(path.to_vec());
        }
    }
}

/// — FuzzStatic: clean up all process substitution temp files.
/// Called after command completion and on shell exit.
/// Silently ignores unlink failures (file might not exist).
pub fn cleanup_process_substitution() {
    unsafe {
        use core::ptr::addr_of_mut;
        let ptr = addr_of_mut!(PROCSUB_PATHS);
        if let Some(v) = &mut *ptr {
            for path in v.iter() {
                let mut buf = path.clone();
                buf.push(0);
                libc::syscall::sys_unlink(bytes_to_str_safe(&buf));
            }
            v.clear();
        }
    }
}

fn process_substitution(cmd: &[u8], is_input: bool) -> Vec<u8> {
    if cmd.is_empty() { return b"/dev/null".to_vec(); }

    // Generate a temp path using PID + a counter
    static mut PROCSUB_COUNTER: u32 = 0;
    let counter = unsafe {
        PROCSUB_COUNTER += 1;
        PROCSUB_COUNTER
    };
    let pid = getpid();

    let mut path = Vec::with_capacity(32);
    path.extend_from_slice(b"/tmp/esh_ps_");
    append_i32(&mut path, pid);
    path.push(b'_');
    append_i32(&mut path, counter as i32);
    let path_str = {
        let mut buf = path.clone();
        buf.push(0);
        buf
    };

    if is_input {
        // <(cmd) — run command, capture stdout to temp file
        let pid_child = fork();
        if pid_child == 0 {
            // Child: redirect stdout to the temp file
            let fd = open(bytes_to_str_safe(&path_str), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
            if fd >= 0 {
                dup2(fd, 1);
                close(fd);
            }
            // exec esh -c "cmd"
            let mut cmd_str = [0u8; 512];
            let len = cmd.len().min(510);
            cmd_str[..len].copy_from_slice(&cmd[..len]);
            cmd_str[len] = 0;
            let esh_path = b"/bin/esh\0";
            let c_flag = b"-c\0";
            let argv: [*const u8; 4] = [
                esh_path.as_ptr(),
                c_flag.as_ptr(),
                cmd_str.as_ptr(),
                core::ptr::null(),
            ];
            execv(bytes_to_str_safe(esh_path), argv.as_ptr());
            _exit(127);
        } else if pid_child > 0 {
            let mut status = 0;
            waitpid(pid_child, &mut status, 0);
        }
    } else {
        // — FuzzStatic: >(cmd) — output process substitution. The calling command
        // writes to the temp file; a background child reads from it and feeds
        // into cmd's stdin. We use a pipe: write end returned as the path
        // "filename", read end goes to the child's stdin. Since OXIDE doesn't
        // have /dev/fd, we fork a child that reads the temp file after the
        // parent has written to it (child waits for parent to close the file
        // by checking file size in a loop — not elegant but functional).
        //
        // Actually, simpler: create the temp file now, return its path.
        // The calling command writes to it. After the pipeline completes
        // (and cleanup fires), we fork a child to process it.
        // For >(cmd) the command typically doesn't need to run synchronously
        // — it's a fire-and-forget sink. So we just create the file.
        // The child reads it after the pipeline ends.
        //
        // Even simpler and correct: fork child NOW, child blocks on reading
        // the file (which doesn't exist yet — parent will write).
        // But this has race conditions. Let's just create the file and
        // fork a reader child that execs after the parent writes.
        let fd = open(bytes_to_str_safe(&path_str), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
        if fd >= 0 { close(fd); }

        // Fork a background child that will process the file contents
        let cmd_copy = cmd.to_vec();
        let path_copy = path_str.clone();
        let pid_child = fork();
        if pid_child == 0 {
            // Child: wait briefly for parent to write, then exec
            // — FuzzStatic: naive wait — let the parent pipeline run first.
            // A proper implementation would use inotify or a pipe.
            libc::syscall::sys_nanosleep(0, 100_000_000); // 100ms

            // Redirect stdin from the temp file
            let fd_in = open2(bytes_to_str_safe(&path_copy), O_RDONLY);
            if fd_in >= 0 {
                dup2(fd_in, 0);
                close(fd_in);
            }

            // exec esh -c "cmd"
            let mut cmd_str = [0u8; 512];
            let len = cmd_copy.len().min(510);
            cmd_str[..len].copy_from_slice(&cmd_copy[..len]);
            cmd_str[len] = 0;
            let esh_path = b"/bin/esh\0";
            let c_flag = b"-c\0";
            let argv: [*const u8; 4] = [
                esh_path.as_ptr(),
                c_flag.as_ptr(),
                cmd_str.as_ptr(),
                core::ptr::null(),
            ];
            execv(bytes_to_str_safe(esh_path), argv.as_ptr());
            _exit(127);
        }
        // Parent: don't wait — the child runs in the background
    }

    // — FuzzStatic: register for cleanup after the command finishes.
    procsub_register(&path);
    path
}

/// Convert bytes to str safely (NUL-terminated)
fn bytes_to_str_safe(bytes: &[u8]) -> &str {
    let mut len = 0;
    while len < bytes.len() && bytes[len] != 0 { len += 1; }
    unsafe { core::str::from_utf8_unchecked(&bytes[..len]) }
}

/// Field splitting based on IFS
/// — ByteRiot: IFS is the invisible surgeon that carves words apart.
fn field_split(input: &[u8]) -> Vec<Vec<u8>> {
    if input.is_empty() {
        return Vec::new();
    }

    let ifs = getenv("IFS").map(|s| s.as_bytes()).unwrap_or(b" \t\n");

    let mut fields = Vec::new();
    let mut current = Vec::new();
    let mut i = 0;

    while i < input.len() {
        if ifs.contains(&input[i]) {
            if !current.is_empty() {
                fields.push(core::mem::take(&mut current));
            }
            // Skip consecutive IFS whitespace
            while i < input.len() && ifs.contains(&input[i]) &&
                  (input[i] == b' ' || input[i] == b'\t' || input[i] == b'\n') {
                i += 1;
            }
        } else {
            current.push(input[i]);
            i += 1;
        }
    }

    if !current.is_empty() {
        fields.push(current);
    }

    fields
}

/// Glob/pathname expansion
/// — ByteRiot: the wildcard whisperer. *, ?, and [] turn words into
/// directory listings. Simple implementation — no recursive ** yet.
fn expand_glob(word: &[u8]) -> Vec<Vec<u8>> {
    // Check if word contains glob characters
    let has_glob = word.iter().any(|&b| b == b'*' || b == b'?' || b == b'[');
    if !has_glob {
        return Vec::new(); // No expansion needed
    }

    // Split into directory + pattern
    let (dir, pattern) = if let Some(slash_pos) = word.iter().rposition(|&b| b == b'/') {
        (&word[..slash_pos + 1], &word[slash_pos + 1..])
    } else {
        (&b"."[..], &word[..])
    };

    let dir_str = if dir == b"." {
        "."
    } else {
        bytes_to_str(dir)
    };

    let mut matches = Vec::new();

    if let Some(mut d) = opendir(dir_str) {
        while let Some(entry) = readdir(&mut d) {
            let name_str = entry.name();
            let name = name_str.as_bytes();
            // Skip . and .. unless pattern starts with .
            if name.first() == Some(&b'.') && pattern.first() != Some(&b'.') {
                continue;
            }
            if glob_match(pattern, name) {
                let mut full = Vec::new();
                if dir != b"." {
                    full.extend_from_slice(dir);
                }
                full.extend_from_slice(name);
                matches.push(full);
            }
        }
        closedir(d);
    }

    // Sort matches alphabetically
    matches.sort();
    matches
}

/// Simple glob pattern matching (* and ? only)
/// — ByteRiot: recursive backtracking matcher. Good enough for shell globs.
/// Made pub for case/esac pattern matching and [[ ]] tests.
/// — FuzzStatic: glob pattern matching with full character class support.
/// Supports *, ?, [abc], [a-z], [!abc] (negated), [^abc] (negated).
/// Handles ] as first char in class (literal), ranges with -, and
/// backtracking for * wildcards. The old version detected [ but never
/// matched it. Now it actually does something. Revolutionary. — FuzzStatic
pub fn glob_match(pattern: &[u8], text: &[u8]) -> bool {
    let mut pi = 0;
    let mut ti = 0;
    let mut star_pi = usize::MAX;
    let mut star_ti = 0;

    while ti < text.len() {
        if pi < pattern.len() && pattern[pi] == b'[' {
            // — FuzzStatic: character class matching. Parse [abc], [a-z],
            // [!abc], [^abc]. POSIX says ] as first char is literal.
            match glob_match_class(&pattern[pi..], text[ti]) {
                Some(class_len) => {
                    pi += class_len;
                    ti += 1;
                }
                None => {
                    // Class didn't match — try star backtrack
                    if star_pi != usize::MAX {
                        pi = star_pi + 1;
                        star_ti += 1;
                        ti = star_ti;
                    } else {
                        return false;
                    }
                }
            }
        } else if pi < pattern.len() && (pattern[pi] == b'?' || pattern[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            star_pi = pi;
            star_ti = ti;
            pi += 1;
        } else if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }

    pi == pattern.len()
}

/// — FuzzStatic: match a single byte against a character class pattern starting at '['.
/// Returns Some(bytes_consumed) if the class matched, None if it didn't.
/// If the class is malformed (no closing ']'), returns None (treat '[' as literal).
fn glob_match_class(pattern: &[u8], ch: u8) -> Option<usize> {
    if pattern.is_empty() || pattern[0] != b'[' {
        return None;
    }

    let mut i = 1; // skip opening '['
    let mut negated = false;
    let mut matched = false;

    // Check for negation: [!...] or [^...]
    if i < pattern.len() && (pattern[i] == b'!' || pattern[i] == b'^') {
        negated = true;
        i += 1;
    }

    // POSIX: ] as first char (after optional !) is a literal ]
    let first = true;
    if i < pattern.len() && pattern[i] == b']' && first {
        if ch == b']' { matched = true; }
        i += 1;
    }

    // Parse class members until closing ]
    while i < pattern.len() && pattern[i] != b']' {
        let c = pattern[i];

        // Check for range: a-z
        if i + 2 < pattern.len() && pattern[i + 1] == b'-' && pattern[i + 2] != b']' {
            let lo = c;
            let hi = pattern[i + 2];
            if ch >= lo && ch <= hi {
                matched = true;
            }
            i += 3;
        } else {
            if ch == c {
                matched = true;
            }
            i += 1;
        }
    }

    // Must find closing ]
    if i >= pattern.len() || pattern[i] != b']' {
        return None; // Malformed — treat as literal
    }

    let consumed = i + 1; // include the ']'
    if negated { matched = !matched; }

    if matched { Some(consumed) } else { None }
}

/// Look up environment variable by byte name
fn getenv_bytes(name: &[u8]) -> Option<&'static [u8]> {
    if let Ok(s) = core::str::from_utf8(name) {
        if let Some(val) = getenv(s) {
            return Some(val.as_bytes());
        }
    }
    None
}

/// Convert bytes to str (NUL-terminated safe)
fn bytes_to_str(bytes: &[u8]) -> &str {
    let mut len = 0;
    while len < bytes.len() && bytes[len] != 0 {
        len += 1;
    }
    unsafe { core::str::from_utf8_unchecked(&bytes[..len]) }
}

/// Brace expansion: {a,b,c} and {1..5}
/// — ByteRiot: the multiplicator. One word becomes many. `pre{a,b}suf`
/// explodes into `preasuf prebsuf`. Must NOT trigger on ${var} (those are
/// parameter expansions, not brace expansions).
pub fn expand_braces(word: &[u8]) -> Vec<Vec<u8>> {
    // — ByteRiot: find the outermost { that's NOT a ${
    let mut depth = 0;
    let mut brace_start = None;
    let mut brace_end = None;
    let mut has_comma = false;
    let mut has_dotdot = false;

    let mut i = 0;
    while i < word.len() {
        if word[i] == b'$' && i + 1 < word.len() && word[i + 1] == b'{' {
            // Skip ${...} — not a brace expansion
            i += 2;
            let mut d = 1;
            while i < word.len() && d > 0 {
                if word[i] == b'{' { d += 1; }
                if word[i] == b'}' { d -= 1; }
                i += 1;
            }
            continue;
        }
        if word[i] == b'{' {
            if depth == 0 { brace_start = Some(i); }
            depth += 1;
        } else if word[i] == b'}' {
            depth -= 1;
            if depth == 0 {
                brace_end = Some(i);
                break;
            }
        } else if depth == 1 {
            if word[i] == b',' { has_comma = true; }
            if word[i] == b'.' && i + 1 < word.len() && word[i + 1] == b'.' {
                has_dotdot = true;
            }
        }
        i += 1;
    }

    let (start, end) = match (brace_start, brace_end) {
        (Some(s), Some(e)) if has_comma || has_dotdot => (s, e),
        _ => return alloc::vec![word.to_vec()], // No valid brace expression
    };

    let prefix = &word[..start];
    let suffix = &word[end + 1..];
    let inner = &word[start + 1..end];

    if has_dotdot && !has_comma {
        // Range expansion: {a..z} or {1..10}
        if let Some(results) = expand_range(inner) {
            let mut out = Vec::new();
            for item in results {
                let mut w = Vec::new();
                w.extend_from_slice(prefix);
                w.extend_from_slice(&item);
                w.extend_from_slice(suffix);
                out.push(w);
            }
            return out;
        }
    }

    // Comma-separated list: {a,b,c}
    let items = split_brace_items(inner);
    let mut out = Vec::new();
    for item in &items {
        let mut w = Vec::new();
        w.extend_from_slice(prefix);
        w.extend_from_slice(item);
        w.extend_from_slice(suffix);
        // — ByteRiot: recursively expand nested braces in each result
        out.extend(expand_braces(&w));
    }
    out
}

/// Split brace items by commas, respecting nesting
fn split_brace_items(input: &[u8]) -> Vec<Vec<u8>> {
    let mut items = Vec::new();
    let mut current = Vec::new();
    let mut depth = 0;

    for &b in input {
        if b == b'{' { depth += 1; }
        if b == b'}' { depth -= 1; }
        if b == b',' && depth == 0 {
            items.push(core::mem::take(&mut current));
        } else {
            current.push(b);
        }
    }
    items.push(current);
    items
}

/// Expand range expression: 1..5 or a..z, with optional zero-padding
fn expand_range(inner: &[u8]) -> Option<Vec<Vec<u8>>> {
    // Find .. separator
    let mut dotdot_pos = None;
    for i in 0..inner.len().saturating_sub(1) {
        if inner[i] == b'.' && inner[i + 1] == b'.' {
            dotdot_pos = Some(i);
            break;
        }
    }
    let dp = dotdot_pos?;
    let left = &inner[..dp];
    let right = &inner[dp + 2..];

    // Try numeric range
    if let (Some(start), Some(end)) = (parse_range_num(left), parse_range_num(right)) {
        let mut results = Vec::new();
        // Detect zero-padding: {01..10} pads to width 2
        let pad_width = if left.len() > 1 && left[0] == b'0' { left.len() } else { 0 };
        let step: i64 = if start <= end { 1 } else { -1 };
        let mut cur = start;
        loop {
            let formatted = format_padded_i64(cur, pad_width);
            results.push(formatted);
            if cur == end { break; }
            cur += step;
            if results.len() > 1000 { break; } // safety limit
        }
        return Some(results);
    }

    // Try single-char range: {a..z}
    if left.len() == 1 && right.len() == 1 {
        let start = left[0];
        let end = right[0];
        if start.is_ascii_alphabetic() && end.is_ascii_alphabetic() {
            let mut results = Vec::new();
            let step: i8 = if start <= end { 1 } else { -1 };
            let mut cur = start as i16;
            loop {
                results.push(alloc::vec![cur as u8]);
                if cur as u8 == end { break; }
                cur += step as i16;
                if results.len() > 256 { break; }
            }
            return Some(results);
        }
    }

    None
}

/// Parse a number for range expansion
fn parse_range_num(s: &[u8]) -> Option<i64> {
    if s.is_empty() { return None; }
    let mut i = 0;
    let neg = if s[0] == b'-' { i += 1; true } else { false };
    let mut result: i64 = 0;
    let mut any = false;
    while i < s.len() {
        if s[i] < b'0' || s[i] > b'9' { return None; }
        result = result * 10 + (s[i] - b'0') as i64;
        any = true;
        i += 1;
    }
    if !any { return None; }
    Some(if neg { -result } else { result })
}

/// Format i64 with optional zero-padding
fn format_padded_i64(mut val: i64, pad: usize) -> Vec<u8> {
    let neg = val < 0;
    if neg { val = -val; }
    let mut digits = Vec::new();
    if val == 0 {
        digits.push(b'0');
    } else {
        while val > 0 {
            digits.push(b'0' + (val % 10) as u8);
            val /= 10;
        }
    }
    if neg { digits.push(b'-'); }
    digits.reverse();
    // Zero-pad if needed
    if pad > 0 {
        let neg_offset = if neg { 1 } else { 0 };
        while digits.len() - neg_offset < pad {
            digits.insert(neg_offset, b'0');
        }
    }
    digits
}

/// Recursive descent arithmetic evaluator for $(( ))
/// — ByteRiot: full arithmetic parser. Supports + - * / % ** << >> & | ^ ~ !
/// comparisons < > <= >= == !=, logical && ||, ternary ? :, assignment = += -=,
/// pre/post ++ --, parens, and variable references (implicit $).
pub fn eval_arith_expr(expr: &[u8]) -> i64 {
    let expr = trim_whitespace(expr);
    if expr.is_empty() { return 0; }
    let mut parser = ArithParser { input: expr, pos: 0 };
    parser.parse_assignment()
}

/// Arithmetic expression parser state
struct ArithParser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> ArithParser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.input.len() && (self.input[self.pos] == b' ' || self.input[self.pos] == b'\t') {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        if self.pos < self.input.len() { Some(self.input[self.pos]) } else { None }
    }

    fn peek2(&self) -> Option<u8> {
        if self.pos + 1 < self.input.len() { Some(self.input[self.pos + 1]) } else { None }
    }

    /// Assignment: VAR = expr, VAR += expr, etc.
    fn parse_assignment(&mut self) -> i64 {
        self.parse_ternary()
    }

    /// Ternary: cond ? true_val : false_val
    fn parse_ternary(&mut self) -> i64 {
        let cond = self.parse_logical_or();
        self.skip_ws();
        if self.peek() == Some(b'?') {
            self.pos += 1;
            let true_val = self.parse_ternary();
            self.skip_ws();
            if self.peek() == Some(b':') { self.pos += 1; }
            let false_val = self.parse_ternary();
            return if cond != 0 { true_val } else { false_val };
        }
        cond
    }

    /// Logical OR: ||
    fn parse_logical_or(&mut self) -> i64 {
        let mut left = self.parse_logical_and();
        loop {
            self.skip_ws();
            if self.peek() == Some(b'|') && self.peek2() == Some(b'|') {
                self.pos += 2;
                let right = self.parse_logical_and();
                left = if left != 0 || right != 0 { 1 } else { 0 };
            } else {
                break;
            }
        }
        left
    }

    /// Logical AND: &&
    fn parse_logical_and(&mut self) -> i64 {
        let mut left = self.parse_bitwise_or();
        loop {
            self.skip_ws();
            if self.peek() == Some(b'&') && self.peek2() == Some(b'&') {
                self.pos += 2;
                let right = self.parse_bitwise_or();
                left = if left != 0 && right != 0 { 1 } else { 0 };
            } else {
                break;
            }
        }
        left
    }

    /// Bitwise OR: |
    fn parse_bitwise_or(&mut self) -> i64 {
        let mut left = self.parse_bitwise_xor();
        loop {
            self.skip_ws();
            if self.peek() == Some(b'|') && self.peek2() != Some(b'|') {
                self.pos += 1;
                let right = self.parse_bitwise_xor();
                left |= right;
            } else {
                break;
            }
        }
        left
    }

    /// Bitwise XOR: ^
    fn parse_bitwise_xor(&mut self) -> i64 {
        let mut left = self.parse_bitwise_and();
        loop {
            self.skip_ws();
            if self.peek() == Some(b'^') {
                self.pos += 1;
                let right = self.parse_bitwise_and();
                left ^= right;
            } else {
                break;
            }
        }
        left
    }

    /// Bitwise AND: &
    fn parse_bitwise_and(&mut self) -> i64 {
        let mut left = self.parse_equality();
        loop {
            self.skip_ws();
            if self.peek() == Some(b'&') && self.peek2() != Some(b'&') {
                self.pos += 1;
                let right = self.parse_equality();
                left &= right;
            } else {
                break;
            }
        }
        left
    }

    /// Equality: == !=
    fn parse_equality(&mut self) -> i64 {
        let mut left = self.parse_relational();
        loop {
            self.skip_ws();
            if self.peek() == Some(b'=') && self.peek2() == Some(b'=') {
                self.pos += 2;
                let right = self.parse_relational();
                left = if left == right { 1 } else { 0 };
            } else if self.peek() == Some(b'!') && self.peek2() == Some(b'=') {
                self.pos += 2;
                let right = self.parse_relational();
                left = if left != right { 1 } else { 0 };
            } else {
                break;
            }
        }
        left
    }

    /// Relational: < > <= >=
    fn parse_relational(&mut self) -> i64 {
        let mut left = self.parse_shift();
        loop {
            self.skip_ws();
            if self.peek() == Some(b'<') && self.peek2() == Some(b'=') {
                self.pos += 2;
                let right = self.parse_shift();
                left = if left <= right { 1 } else { 0 };
            } else if self.peek() == Some(b'>') && self.peek2() == Some(b'=') {
                self.pos += 2;
                let right = self.parse_shift();
                left = if left >= right { 1 } else { 0 };
            } else if self.peek() == Some(b'<') && self.peek2() != Some(b'<') {
                self.pos += 1;
                let right = self.parse_shift();
                left = if left < right { 1 } else { 0 };
            } else if self.peek() == Some(b'>') && self.peek2() != Some(b'>') {
                self.pos += 1;
                let right = self.parse_shift();
                left = if left > right { 1 } else { 0 };
            } else {
                break;
            }
        }
        left
    }

    /// Shift: << >>
    fn parse_shift(&mut self) -> i64 {
        let mut left = self.parse_additive();
        loop {
            self.skip_ws();
            if self.peek() == Some(b'<') && self.peek2() == Some(b'<') {
                self.pos += 2;
                let right = self.parse_additive();
                left <<= right;
            } else if self.peek() == Some(b'>') && self.peek2() == Some(b'>') {
                self.pos += 2;
                let right = self.parse_additive();
                left >>= right;
            } else {
                break;
            }
        }
        left
    }

    /// Additive: + -
    fn parse_additive(&mut self) -> i64 {
        let mut left = self.parse_multiplicative();
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'+') if self.peek2() != Some(b'+') => {
                    self.pos += 1;
                    left += self.parse_multiplicative();
                }
                Some(b'-') if self.peek2() != Some(b'-') => {
                    self.pos += 1;
                    left -= self.parse_multiplicative();
                }
                _ => break,
            }
        }
        left
    }

    /// Multiplicative: * / %
    fn parse_multiplicative(&mut self) -> i64 {
        let mut left = self.parse_exponent();
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'*') if self.peek2() != Some(b'*') => {
                    self.pos += 1;
                    left *= self.parse_exponent();
                }
                Some(b'/') => {
                    self.pos += 1;
                    let right = self.parse_exponent();
                    left = if right != 0 { left / right } else { 0 };
                }
                Some(b'%') => {
                    self.pos += 1;
                    let right = self.parse_exponent();
                    left = if right != 0 { left % right } else { 0 };
                }
                _ => break,
            }
        }
        left
    }

    /// Exponentiation: **
    fn parse_exponent(&mut self) -> i64 {
        let base = self.parse_unary();
        self.skip_ws();
        if self.peek() == Some(b'*') && self.peek2() == Some(b'*') {
            self.pos += 2;
            let exp = self.parse_exponent(); // right-associative
            return pow_i64(base, exp);
        }
        base
    }

    /// Unary: - + ~ ! ++ --
    fn parse_unary(&mut self) -> i64 {
        self.skip_ws();
        match self.peek() {
            Some(b'-') if self.peek2() != Some(b'-') => { self.pos += 1; -self.parse_unary() }
            Some(b'+') if self.peek2() != Some(b'+') => { self.pos += 1; self.parse_unary() }
            Some(b'~') => { self.pos += 1; !self.parse_unary() }
            Some(b'!') if self.peek2() != Some(b'=') => { self.pos += 1; if self.parse_unary() == 0 { 1 } else { 0 } }
            _ => self.parse_primary(),
        }
    }

    /// Primary: number, variable, or parenthesized expression
    fn parse_primary(&mut self) -> i64 {
        self.skip_ws();
        match self.peek() {
            Some(b'(') => {
                self.pos += 1;
                let val = self.parse_assignment();
                self.skip_ws();
                if self.peek() == Some(b')') { self.pos += 1; }
                val
            }
            Some(b'0'..=b'9') => self.parse_number(),
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => {
                // Variable reference (implicit $)
                let start = self.pos;
                while self.pos < self.input.len() &&
                    (self.input[self.pos].is_ascii_alphanumeric() || self.input[self.pos] == b'_') {
                    self.pos += 1;
                }
                let name = &self.input[start..self.pos];

                // Check for assignment: VAR = expr
                self.skip_ws();
                if self.peek() == Some(b'=') && self.peek2() != Some(b'=') {
                    self.pos += 1;
                    let val = self.parse_assignment();
                    let val_str = format_padded_i64(val, 0);
                    if let (Ok(n), Ok(v)) = (core::str::from_utf8(name), core::str::from_utf8(&val_str)) {
                        setenv(n, v);
                    }
                    return val;
                }

                // Look up variable value
                if let Some(val_bytes) = getenv_bytes(name) {
                    parse_substr_num(val_bytes)
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    /// Parse a number (decimal, hex 0x, octal 0)
    fn parse_number(&mut self) -> i64 {
        self.skip_ws();
        if self.peek() == Some(b'0') {
            if self.peek2() == Some(b'x') || self.peek2() == Some(b'X') {
                // Hex
                self.pos += 2;
                let mut val: i64 = 0;
                while self.pos < self.input.len() {
                    let c = self.input[self.pos];
                    if c >= b'0' && c <= b'9' { val = val * 16 + (c - b'0') as i64; }
                    else if c >= b'a' && c <= b'f' { val = val * 16 + (c - b'a' + 10) as i64; }
                    else if c >= b'A' && c <= b'F' { val = val * 16 + (c - b'A' + 10) as i64; }
                    else { break; }
                    self.pos += 1;
                }
                return val;
            }
        }
        // Decimal
        let mut val: i64 = 0;
        while self.pos < self.input.len() && self.input[self.pos] >= b'0' && self.input[self.pos] <= b'9' {
            val = val * 10 + (self.input[self.pos] - b'0') as i64;
            self.pos += 1;
        }
        val
    }
}

/// Integer power
fn pow_i64(mut base: i64, mut exp: i64) -> i64 {
    if exp < 0 { return 0; }
    let mut result: i64 = 1;
    while exp > 0 {
        if exp & 1 == 1 { result *= base; }
        base *= base;
        exp >>= 1;
    }
    result
}

/// Trim whitespace from byte slice
fn trim_whitespace(s: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < s.len() && (s[start] == b' ' || s[start] == b'\t') { start += 1; }
    let mut end = s.len();
    while end > start && (s[end - 1] == b' ' || s[end - 1] == b'\t') { end -= 1; }
    &s[start..end]
}

/// Append decimal representation of i64 to a Vec
pub fn append_i64(out: &mut Vec<u8>, mut val: i64) {
    if val < 0 {
        out.push(b'-');
        val = -val;
    }
    if val == 0 {
        out.push(b'0');
        return;
    }
    let start = out.len();
    while val > 0 {
        out.push(b'0' + (val % 10) as u8);
        val /= 10;
    }
    out[start..].reverse();
}

/// Append decimal representation of i32 to a Vec
fn append_i32(out: &mut Vec<u8>, mut val: i32) {
    if val < 0 {
        out.push(b'-');
        val = -val;
    }
    if val == 0 {
        out.push(b'0');
        return;
    }
    let start = out.len();
    while val > 0 {
        out.push(b'0' + (val % 10) as u8);
        val /= 10;
    }
    out[start..].reverse();
}

#[cfg(test)]
mod tests {
    use super::*;

    // — IronGhost: static empty arrays vec for test context
    static EMPTY_ARRAYS: Vec<(Vec<u8>, Vec<Vec<u8>>)> = Vec::new();

    fn make_ctx() -> ExpandContext<'static> {
        ExpandContext {
            last_status: 0,
            pid: 42,
            positional: Vec::new(),
            nounset: false,
            arrays: &EMPTY_ARRAYS,
        }
    }

    #[test]
    fn test_expand_tilde() {
        let result = expand_tilde(b"/foo/bar");
        assert_eq!(result, b"/foo/bar");
        // ~ expansion depends on HOME env, skip in unit test
    }

    #[test]
    fn test_expand_no_vars() {
        let ctx = make_ctx();
        let result = expand_vars_and_cmdsub(b"hello world", &ctx);
        assert_eq!(result, b"hello world");
    }

    #[test]
    fn test_expand_dollar_question() {
        let ctx = ExpandContext { last_status: 42, pid: 1, positional: Vec::new(), nounset: false };
        let result = expand_vars_and_cmdsub(b"status=$?", &ctx);
        assert_eq!(result, b"status=42");
    }

    #[test]
    fn test_expand_dollar_dollar() {
        let ctx = ExpandContext { last_status: 0, pid: 1234, positional: Vec::new(), nounset: false };
        let result = expand_vars_and_cmdsub(b"pid=$$", &ctx);
        assert_eq!(result, b"pid=1234");
    }

    #[test]
    fn test_expand_dollar_hash() {
        let ctx = ExpandContext {
            last_status: 0, pid: 1,
            positional: alloc::vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()],
            nounset: false,
        };
        let result = expand_vars_and_cmdsub(b"count=$#", &ctx);
        assert_eq!(result, b"count=3");
    }

    #[test]
    fn test_expand_positional() {
        let ctx = ExpandContext {
            last_status: 0, pid: 1,
            positional: alloc::vec![b"foo".to_vec(), b"bar".to_vec()],
            nounset: false,
        };
        let result = expand_vars_and_cmdsub(b"$0 and $1", &ctx);
        assert_eq!(result, b"foo and bar");
    }

    #[test]
    fn test_single_quote_no_expand() {
        let ctx = make_ctx();
        let result = expand_vars_and_cmdsub(b"'$HOME'", &ctx);
        assert_eq!(result, b"$HOME");
    }

    #[test]
    fn test_field_split_basic() {
        let fields = field_split(b"hello world foo");
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0], b"hello");
        assert_eq!(fields[1], b"world");
        assert_eq!(fields[2], b"foo");
    }

    #[test]
    fn test_field_split_empty() {
        let fields = field_split(b"");
        assert!(fields.is_empty());
    }

    #[test]
    fn test_field_split_leading_trailing() {
        let fields = field_split(b"  hello  ");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0], b"hello");
    }

    #[test]
    fn test_glob_match_star() {
        assert!(glob_match(b"*.rs", b"main.rs"));
        assert!(glob_match(b"*.rs", b"test.rs"));
        assert!(!glob_match(b"*.rs", b"main.c"));
    }

    #[test]
    fn test_glob_match_question() {
        assert!(glob_match(b"?.c", b"a.c"));
        assert!(!glob_match(b"?.c", b"ab.c"));
    }

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match(b"hello", b"hello"));
        assert!(!glob_match(b"hello", b"world"));
    }

    #[test]
    fn test_glob_match_complex() {
        assert!(glob_match(b"test_*_ok", b"test_foo_ok"));
        assert!(glob_match(b"*.*", b"file.txt"));
        assert!(!glob_match(b"*.*", b"noext"));
    }

    #[test]
    fn test_append_i32() {
        let mut v = Vec::new();
        append_i32(&mut v, 42);
        assert_eq!(v, b"42");

        let mut v = Vec::new();
        append_i32(&mut v, -7);
        assert_eq!(v, b"-7");

        let mut v = Vec::new();
        append_i32(&mut v, 0);
        assert_eq!(v, b"0");
    }

    #[test]
    fn test_braced_var_default() {
        // ${NONEXISTENT:-fallback} should produce "fallback"
        let ctx = make_ctx();
        let result = expand_vars_and_cmdsub(b"${NONEXISTENT_VAR_XYZ:-fallback}", &ctx);
        assert_eq!(result, b"fallback");
    }

    #[test]
    fn test_backslash_escape() {
        let ctx = make_ctx();
        let result = expand_vars_and_cmdsub(b"hello\\$world", &ctx);
        assert_eq!(result, b"hello$world");
    }

    #[test]
    fn test_literal_dollar_at_end() {
        let ctx = make_ctx();
        let result = expand_vars_and_cmdsub(b"cost is $", &ctx);
        assert_eq!(result, b"cost is $");
    }

    // — StaticRiot: arithmetic expansion tests

    #[test]
    fn test_arith_basic_add() {
        assert_eq!(eval_arith_expr(b"2 + 3"), 5);
    }

    #[test]
    fn test_arith_precedence() {
        assert_eq!(eval_arith_expr(b"2 + 3 * 4"), 14);
    }

    #[test]
    fn test_arith_parens() {
        assert_eq!(eval_arith_expr(b"(2 + 3) * 4"), 20);
    }

    #[test]
    fn test_arith_subtract() {
        assert_eq!(eval_arith_expr(b"10 - 3"), 7);
    }

    #[test]
    fn test_arith_negative() {
        assert_eq!(eval_arith_expr(b"10 - 20"), -10);
    }

    #[test]
    fn test_arith_multiply() {
        assert_eq!(eval_arith_expr(b"6 * 7"), 42);
    }

    #[test]
    fn test_arith_divide() {
        assert_eq!(eval_arith_expr(b"20 / 4"), 5);
    }

    #[test]
    fn test_arith_divide_by_zero() {
        assert_eq!(eval_arith_expr(b"10 / 0"), 0);
    }

    #[test]
    fn test_arith_modulo() {
        assert_eq!(eval_arith_expr(b"17 % 5"), 2);
    }

    #[test]
    fn test_arith_exponent() {
        assert_eq!(eval_arith_expr(b"2 ** 10"), 1024);
    }

    #[test]
    fn test_arith_bitwise_and() {
        assert_eq!(eval_arith_expr(b"0xFF & 0x0F"), 15);
    }

    #[test]
    fn test_arith_bitwise_or() {
        assert_eq!(eval_arith_expr(b"0xF0 | 0x0F"), 255);
    }

    #[test]
    fn test_arith_bitwise_xor() {
        assert_eq!(eval_arith_expr(b"0xFF ^ 0x0F"), 240);
    }

    #[test]
    fn test_arith_shift_left() {
        assert_eq!(eval_arith_expr(b"1 << 4"), 16);
    }

    #[test]
    fn test_arith_shift_right() {
        assert_eq!(eval_arith_expr(b"16 >> 2"), 4);
    }

    #[test]
    fn test_arith_comparison_gt() {
        assert_eq!(eval_arith_expr(b"5 > 3"), 1);
        assert_eq!(eval_arith_expr(b"3 > 5"), 0);
    }

    #[test]
    fn test_arith_comparison_lt() {
        assert_eq!(eval_arith_expr(b"3 < 5"), 1);
        assert_eq!(eval_arith_expr(b"5 < 3"), 0);
    }

    #[test]
    fn test_arith_comparison_eq() {
        assert_eq!(eval_arith_expr(b"5 == 5"), 1);
        assert_eq!(eval_arith_expr(b"5 == 3"), 0);
    }

    #[test]
    fn test_arith_comparison_ne() {
        assert_eq!(eval_arith_expr(b"5 != 3"), 1);
        assert_eq!(eval_arith_expr(b"5 != 5"), 0);
    }

    #[test]
    fn test_arith_comparison_le_ge() {
        assert_eq!(eval_arith_expr(b"5 <= 5"), 1);
        assert_eq!(eval_arith_expr(b"5 >= 5"), 1);
        assert_eq!(eval_arith_expr(b"4 <= 5"), 1);
        assert_eq!(eval_arith_expr(b"6 >= 5"), 1);
    }

    #[test]
    fn test_arith_logical_and() {
        assert_eq!(eval_arith_expr(b"1 && 1"), 1);
        assert_eq!(eval_arith_expr(b"1 && 0"), 0);
        assert_eq!(eval_arith_expr(b"0 && 1"), 0);
    }

    #[test]
    fn test_arith_logical_or() {
        assert_eq!(eval_arith_expr(b"0 || 1"), 1);
        assert_eq!(eval_arith_expr(b"0 || 0"), 0);
        assert_eq!(eval_arith_expr(b"1 || 0"), 1);
    }

    #[test]
    fn test_arith_logical_not() {
        assert_eq!(eval_arith_expr(b"!0"), 1);
        assert_eq!(eval_arith_expr(b"!1"), 0);
        assert_eq!(eval_arith_expr(b"!42"), 0);
    }

    #[test]
    fn test_arith_bitwise_not() {
        assert_eq!(eval_arith_expr(b"~0"), -1);
    }

    #[test]
    fn test_arith_ternary() {
        assert_eq!(eval_arith_expr(b"1 ? 42 : 99"), 42);
        assert_eq!(eval_arith_expr(b"0 ? 42 : 99"), 99);
    }

    #[test]
    fn test_arith_unary_minus() {
        assert_eq!(eval_arith_expr(b"-5"), -5);
        assert_eq!(eval_arith_expr(b"-5 + 10"), 5);
    }

    #[test]
    fn test_arith_hex() {
        assert_eq!(eval_arith_expr(b"0xFF"), 255);
        assert_eq!(eval_arith_expr(b"0x10"), 16);
    }

    #[test]
    fn test_arith_nested_parens() {
        assert_eq!(eval_arith_expr(b"((2 + 3) * (4 + 1))"), 25);
    }

    #[test]
    fn test_arith_complex() {
        assert_eq!(eval_arith_expr(b"2 ** 3 + 1"), 9);
        assert_eq!(eval_arith_expr(b"10 % 3 * 2"), 2);
    }

    // — StaticRiot: brace expansion tests

    #[test]
    fn test_brace_comma_simple() {
        let result = expand_braces(b"{a,b,c}");
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], b"a");
        assert_eq!(result[1], b"b");
        assert_eq!(result[2], b"c");
    }

    #[test]
    fn test_brace_with_prefix_suffix() {
        let result = expand_braces(b"pre{a,b}suf");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], b"preasuf");
        assert_eq!(result[1], b"prebsuf");
    }

    #[test]
    fn test_brace_numeric_range() {
        let result = expand_braces(b"{1..5}");
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], b"1");
        assert_eq!(result[4], b"5");
    }

    #[test]
    fn test_brace_char_range() {
        let result = expand_braces(b"{a..e}");
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], b"a");
        assert_eq!(result[4], b"e");
    }

    #[test]
    fn test_brace_reverse_range() {
        let result = expand_braces(b"{5..1}");
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], b"5");
        assert_eq!(result[4], b"1");
    }

    #[test]
    fn test_brace_no_expansion() {
        // No comma or .. means no brace expansion
        let result = expand_braces(b"hello");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], b"hello");
    }

    #[test]
    fn test_brace_skip_dollar_brace() {
        // ${VAR} should NOT be treated as brace expansion
        let result = expand_braces(b"${HOME}");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], b"${HOME}");
    }

    #[test]
    fn test_brace_zero_padded() {
        let result = expand_braces(b"{01..03}");
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], b"01");
        assert_eq!(result[1], b"02");
        assert_eq!(result[2], b"03");
    }

    // — StaticRiot: string manipulation tests

    #[test]
    fn test_strip_prefix_shortest() {
        assert_eq!(strip_prefix(b"/usr/local/bin", b"*/", false), b"usr/local/bin");
    }

    #[test]
    fn test_strip_prefix_longest() {
        assert_eq!(strip_prefix(b"/usr/local/bin", b"*/", true), b"bin");
    }

    #[test]
    fn test_strip_suffix_shortest() {
        assert_eq!(strip_suffix(b"archive.tar.gz", b".*", false), b"archive.tar");
    }

    #[test]
    fn test_strip_suffix_longest() {
        assert_eq!(strip_suffix(b"archive.tar.gz", b".*", true), b"archive");
    }

    #[test]
    fn test_string_replace_first() {
        assert_eq!(string_replace(b"hello world hello", b"hello", b"bye", false), b"bye world hello");
    }

    #[test]
    fn test_string_replace_all() {
        assert_eq!(string_replace(b"hello world hello", b"hello", b"bye", true), b"bye world bye");
    }

    #[test]
    fn test_substring_offset() {
        assert_eq!(substring(b"abcdefgh", 2, None), b"cdefgh");
    }

    #[test]
    fn test_substring_offset_length() {
        assert_eq!(substring(b"abcdefgh", 2, Some(3)), b"cde");
    }

    #[test]
    fn test_substring_negative_offset() {
        assert_eq!(substring(b"abcdefgh", -3, None), b"fgh");
    }

    #[test]
    fn test_case_transform_upper_first() {
        assert_eq!(case_transform(b"hello", true, false), b"Hello");
    }

    #[test]
    fn test_case_transform_upper_all() {
        assert_eq!(case_transform(b"hello", true, true), b"HELLO");
    }

    #[test]
    fn test_case_transform_lower_first() {
        assert_eq!(case_transform(b"HELLO", false, false), b"hELLO");
    }

    #[test]
    fn test_case_transform_lower_all() {
        assert_eq!(case_transform(b"HELLO", false, true), b"hello");
    }

    // — StaticRiot: glob match edge cases

    #[test]
    fn test_glob_match_empty_pattern() {
        assert!(glob_match(b"", b""));
        assert!(!glob_match(b"", b"x"));
    }

    #[test]
    fn test_glob_match_star_empty() {
        assert!(glob_match(b"*", b""));
        assert!(glob_match(b"*", b"anything"));
    }

    #[test]
    fn test_glob_match_multi_star() {
        assert!(glob_match(b"*.tar.*", b"file.tar.gz"));
        assert!(!glob_match(b"*.tar.*", b"file.zip"));
    }

    // — StaticRiot: pow and helpers

    #[test]
    fn test_pow_i64() {
        assert_eq!(pow_i64(2, 0), 1);
        assert_eq!(pow_i64(2, 1), 2);
        assert_eq!(pow_i64(2, 8), 256);
        assert_eq!(pow_i64(3, 3), 27);
        assert_eq!(pow_i64(2, -1), 0);
    }

    #[test]
    fn test_append_i64() {
        let mut v = Vec::new();
        append_i64(&mut v, 1234567890);
        assert_eq!(v, b"1234567890");

        let mut v = Vec::new();
        append_i64(&mut v, -42);
        assert_eq!(v, b"-42");

        let mut v = Vec::new();
        append_i64(&mut v, 0);
        assert_eq!(v, b"0");
    }

    #[test]
    fn test_format_padded_i64() {
        assert_eq!(format_padded_i64(42, 0), b"42");
        assert_eq!(format_padded_i64(5, 3), b"005");
        assert_eq!(format_padded_i64(-3, 0), b"-3");
    }

    // — FuzzStatic: glob character class tests. Comprehensive coverage
    // because off-by-one in bracket parsing = security hole in PATH expansion.

    #[test]
    fn test_glob_basic_star_question() {
        assert!(glob_match(b"*.txt", b"hello.txt"));
        assert!(!glob_match(b"*.txt", b"hello.rs"));
        assert!(glob_match(b"h?llo", b"hello"));
        assert!(!glob_match(b"h?llo", b"hllo"));
    }

    #[test]
    fn test_glob_char_class_simple() {
        assert!(glob_match(b"[abc]", b"a"));
        assert!(glob_match(b"[abc]", b"b"));
        assert!(glob_match(b"[abc]", b"c"));
        assert!(!glob_match(b"[abc]", b"d"));
        assert!(!glob_match(b"[abc]", b""));
    }

    #[test]
    fn test_glob_char_class_range() {
        assert!(glob_match(b"[a-z]", b"m"));
        assert!(glob_match(b"[a-z]", b"a"));
        assert!(glob_match(b"[a-z]", b"z"));
        assert!(!glob_match(b"[a-z]", b"A"));
        assert!(!glob_match(b"[a-z]", b"0"));
        assert!(glob_match(b"[0-9]", b"5"));
        assert!(!glob_match(b"[0-9]", b"a"));
    }

    #[test]
    fn test_glob_negated_class() {
        assert!(!glob_match(b"[!abc]", b"a"));
        assert!(glob_match(b"[!abc]", b"d"));
        assert!(glob_match(b"[!abc]", b"z"));
        assert!(!glob_match(b"[^abc]", b"b"));
        assert!(glob_match(b"[^abc]", b"x"));
    }

    #[test]
    fn test_glob_negated_range() {
        assert!(!glob_match(b"[!0-9]", b"5"));
        assert!(glob_match(b"[!0-9]", b"a"));
    }

    #[test]
    fn test_glob_class_literal_bracket() {
        // ] as first char in class is literal
        assert!(glob_match(b"[]abc]", b"]"));
        assert!(glob_match(b"[]abc]", b"a"));
        assert!(!glob_match(b"[]abc]", b"x"));
    }

    #[test]
    fn test_glob_class_in_pattern() {
        assert!(glob_match(b"file[0-9].txt", b"file3.txt"));
        assert!(!glob_match(b"file[0-9].txt", b"fileA.txt"));
        assert!(glob_match(b"*.[ch]", b"main.c"));
        assert!(glob_match(b"*.[ch]", b"main.h"));
        assert!(!glob_match(b"*.[ch]", b"main.o"));
    }

    #[test]
    fn test_glob_class_combined_with_star() {
        assert!(glob_match(b"[a-z]*", b"hello"));
        assert!(!glob_match(b"[a-z]*", b"123"));
        assert!(glob_match(b"*[0-9]", b"file9"));
        assert!(!glob_match(b"*[0-9]", b"filea"));
    }

    #[test]
    fn test_glob_malformed_class() {
        // Unclosed [ treated as literal
        assert!(!glob_match(b"[abc", b"a"));
        assert!(glob_match(b"[abc", b"[abc"));
    }

    #[test]
    fn test_glob_mixed_range_and_chars() {
        assert!(glob_match(b"[a-z0-9_]", b"m"));
        assert!(glob_match(b"[a-z0-9_]", b"5"));
        assert!(glob_match(b"[a-z0-9_]", b"_"));
        assert!(!glob_match(b"[a-z0-9_]", b"A"));
    }
}
