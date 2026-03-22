//! Shell Lexer — tokenizes input into a stream of Token values
//!
//! — ByteRiot: the lexer that separates signal from noise. Takes raw bytes
//! and produces typed tokens: words, operators, keywords, redirections.
//! Handles quoting (single, double, backslash), comments (#), and special
//! characters (|, &, ;, <, >, (, )).
//!
//! The lexer does NOT expand variables or globs — that's the expander's job.
//! It just identifies token boundaries and types.

use alloc::vec::Vec;
use alloc::vec;

/// Token types produced by the lexer
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A word (command name, argument, filename). May contain unexpanded $VAR.
    Word(Vec<u8>),
    /// Assignment: NAME=VALUE (no spaces around =)
    Assignment(Vec<u8>, Vec<u8>),
    /// Pipe operator |
    Pipe,
    /// AND operator &&
    And,
    /// OR operator ||
    Or,
    /// Semicolon ;
    Semi,
    /// Newline (statement terminator, like ;)
    Newline,
    /// Background operator &
    Background,
    /// Input redirection <
    RedirIn,
    /// Output redirection >
    RedirOut,
    /// Append redirection >>
    RedirAppend,
    /// File descriptor redirection (e.g., 2>)
    RedirFd(u8),
    /// Dup redirection (e.g., 2>&1)
    RedirDup(u8, u8),
    /// Left paren (
    LParen,
    /// Right paren )
    RParen,
    /// Shell keywords
    If, Then, Else, Elif, Fi,
    For, While, Until, Do, Done,
    Case, Esac, In,
    Function,
    /// Select keyword — interactive menu command
    /// — IronGhost: `select name in words; do body; done`
    Select,
    /// Bang operator ! — pipeline negation
    /// — ByteRiot: inverts the exit status of a pipeline. `! grep -q pattern` returns 0 when grep fails.
    Bang,
    /// Double semicolon ;; — case/esac pattern terminator
    /// — ByteRiot: the only escape from a case arm that doesn't make you question your life choices.
    DoubleSemi,
    /// Double left bracket [[ — extended test command
    /// — ByteRiot: bash-style conditional expressions. Cleaner than [ but less portable.
    DblLBracket,
    /// Double right bracket ]] — closes extended test
    DblRBracket,
    /// Heredoc — (delimiter, body, strip_tabs)
    /// — ByteRiot: <<DELIM for heredocs, <<-DELIM strips leading tabs. Body filled in post-tokenize.
    HereDoc(Vec<u8>, Vec<u8>, bool),
    /// Here-string <<< — feeds a string as stdin
    /// — ByteRiot: because sometimes echo | cmd is too many keystrokes.
    HereString,
    /// End of input
    Eof,
}

/// Tokenize a byte slice into a Vec of Tokens.
/// — ByteRiot: this is the entry point. Feed it a line (or multi-line input)
/// and get back a token stream the parser can consume.
pub fn tokenize(input: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    // — ByteRiot: POSIX rule — assignments are only valid BEFORE the command
    // name. Once we see a non-assignment word, everything after is a word arg.
    // Reset on statement separators (;, newline, |, &&, ||) since a new command starts.
    let mut seen_word = false;
    // — ByteRiot: heredoc body collection — real shells do this in two phases.
    // Phase 1: tokenize the command line, note which HereDoc tokens need bodies.
    // Phase 2: when we hit a newline with pending heredocs, consume body lines
    // from input until the delimiter appears alone on a line. No body left behind.
    let mut pending_heredocs: Vec<usize> = Vec::new();

    while pos < input.len() {
        // Skip whitespace (not newlines — those are significant)
        while pos < input.len() && (input[pos] == b' ' || input[pos] == b'\t') {
            pos += 1;
        }
        if pos >= input.len() { break; }

        let ch = input[pos];

        // Comment — skip to end of line
        if ch == b'#' {
            while pos < input.len() && input[pos] != b'\n' { pos += 1; }
            continue;
        }

        // Newline
        if ch == b'\n' {
            tokens.push(Token::Newline);
            seen_word = false; // — ByteRiot: new command starts after newline
            pos += 1;

            // — ByteRiot: heredoc body harvest time. The command line is done,
            // now we devour lines from input until each pending delimiter shows
            // up alone on a line. Multiple heredocs on one line? We eat them in
            // order — first <<EOF1, then <<EOF2. POSIX says so, and who are we
            // to argue with the dead.
            for hdoc_idx in pending_heredocs.drain(..) {
                if let Token::HereDoc(ref delim, ref mut body, strip_tabs) = tokens[hdoc_idx] {
                    let delim_copy = delim.clone();
                    let mut collected = Vec::new();
                    loop {
                        if pos >= input.len() {
                            // — ByteRiot: ran out of input before finding delimiter.
                            // Unterminated heredoc — take what we got and move on.
                            // The parser can scream about it later.
                            break;
                        }
                        // Read one line
                        let line_start = pos;
                        while pos < input.len() && input[pos] != b'\n' {
                            pos += 1;
                        }
                        let line = &input[line_start..pos];
                        if pos < input.len() {
                            pos += 1; // — ByteRiot: skip the newline, we're not savages
                        }

                        // — ByteRiot: check if this line IS the delimiter.
                        // <<- strips leading tabs before comparison — because
                        // indented heredocs in functions shouldn't look like garbage.
                        let check_line = if strip_tabs {
                            let mut start = 0;
                            while start < line.len() && line[start] == b'\t' {
                                start += 1;
                            }
                            &line[start..]
                        } else {
                            line
                        };

                        if check_line == delim_copy.as_slice() {
                            // — ByteRiot: delimiter found. This heredoc is sealed.
                            break;
                        }

                        // — ByteRiot: not the delimiter — this line is body content.
                        // For <<-, strip leading tabs from body lines too.
                        if strip_tabs {
                            let mut start = 0;
                            while start < line.len() && line[start] == b'\t' {
                                start += 1;
                            }
                            collected.extend_from_slice(&line[start..]);
                        } else {
                            collected.extend_from_slice(line);
                        }
                        collected.push(b'\n');
                    }
                    *body = collected;
                }
            }

            continue;
        }

        // Operators
        match ch {
            b'|' => {
                pos += 1;
                if pos < input.len() && input[pos] == b'|' {
                    tokens.push(Token::Or);
                    pos += 1;
                } else {
                    tokens.push(Token::Pipe);
                }
                seen_word = false; // new command after pipe/||
                continue;
            }
            b'&' => {
                pos += 1;
                if pos < input.len() && input[pos] == b'&' {
                    tokens.push(Token::And);
                    pos += 1;
                } else {
                    tokens.push(Token::Background);
                }
                seen_word = false; // new command after &&/&
                continue;
            }
            b'!' => { tokens.push(Token::Bang); seen_word = false; pos += 1; continue; }
            b';' => {
                pos += 1;
                if pos < input.len() && input[pos] == b';' {
                    tokens.push(Token::DoubleSemi);
                    pos += 1;
                } else {
                    tokens.push(Token::Semi);
                }
                seen_word = false;
                continue;
            }
            b'(' => { tokens.push(Token::LParen); seen_word = false; pos += 1; continue; }
            b')' => { tokens.push(Token::RParen); pos += 1; continue; }
            b'<' => {
                pos += 1;
                if pos < input.len() && input[pos] == b'<' {
                    pos += 1;
                    if pos < input.len() && input[pos] == b'<' {
                        // — ByteRiot: <<< here-string — one extra chevron, infinite convenience
                        tokens.push(Token::HereString);
                        pos += 1;
                    } else {
                        // — ByteRiot: << heredoc — read delimiter, pray for matching EOF
                        let strip_tabs = if pos < input.len() && input[pos] == b'-' {
                            pos += 1;
                            true
                        } else {
                            false
                        };
                        // Skip whitespace before delimiter
                        while pos < input.len() && (input[pos] == b' ' || input[pos] == b'\t') {
                            pos += 1;
                        }
                        let delim = read_word(input, &mut pos);
                        // Strip quotes from delimiter (quoted = no expansion)
                        let clean_delim = strip_heredoc_quotes(&delim);
                        // — ByteRiot: body gets collected when we hit the next newline.
                        // Stash the token index so the newline handler knows which
                        // heredocs are starving for content. Feed them or they bite.
                        tokens.push(Token::HereDoc(clean_delim, Vec::new(), strip_tabs));
                        pending_heredocs.push(tokens.len() - 1);
                    }
                } else if pos < input.len() && input[pos] == b'(' {
                    // — ByteRiot: <(cmd) process substitution — collect the entire
                    // <(...) as a single Word token so the expander can detect it.
                    // Without this, < becomes RedirIn and ( becomes LParen, and the
                    // process substitution pattern is lost.
                    let mut word = Vec::new();
                    word.push(b'<');
                    word.push(b'(');
                    pos += 1; // skip '('
                    let mut depth = 1u32;
                    while pos < input.len() && depth > 0 {
                        let c = input[pos];
                        if c == b'(' { depth += 1; }
                        if c == b')' { depth -= 1; }
                        word.push(c);
                        pos += 1;
                    }
                    tokens.push(Token::Word(word));
                } else {
                    tokens.push(Token::RedirIn);
                }
                continue;
            }
            b'>' => {
                pos += 1;
                if pos < input.len() && input[pos] == b'>' {
                    tokens.push(Token::RedirAppend);
                    pos += 1;
                } else if pos < input.len() && input[pos] == b'(' {
                    // — ByteRiot: >(cmd) output process substitution
                    let mut word = Vec::new();
                    word.push(b'>');
                    word.push(b'(');
                    pos += 1;
                    let mut depth = 1u32;
                    while pos < input.len() && depth > 0 {
                        let c = input[pos];
                        if c == b'(' { depth += 1; }
                        if c == b')' { depth -= 1; }
                        word.push(c);
                        pos += 1;
                    }
                    tokens.push(Token::Word(word));
                } else {
                    tokens.push(Token::RedirOut);
                }
                continue;
            }
            _ => {}
        }

        // — ByteRiot: fd redirection (e.g., 2> or 2>&1)
        if ch.is_ascii_digit() && pos + 1 < input.len() && input[pos + 1] == b'>' {
            let fd = ch - b'0';
            pos += 2;
            if pos + 1 < input.len() && input[pos] == b'&' && input[pos + 1].is_ascii_digit() {
                let target_fd = input[pos + 1] - b'0';
                tokens.push(Token::RedirDup(fd, target_fd));
                pos += 2;
            } else if pos < input.len() && input[pos] == b'>' {
                tokens.push(Token::RedirAppend); // 2>> style
                pos += 1;
            } else {
                tokens.push(Token::RedirFd(fd));
            }
            continue;
        }

        // Word (including quoted strings)
        let word = read_word(input, &mut pos);
        if !word.is_empty() {
            // — ByteRiot: POSIX rule — only recognize assignments BEFORE the
            // command name. `echo FOO=bar` is an argument, not an assignment.
            // `FOO=bar echo` IS a prefix assignment.
            if !seen_word {
                if let Some(eq_pos) = word.iter().position(|&b| b == b'=') {
                    // — ByteRiot: allow brackets in assignment names for indexed arrays: x[0]=val
                    // Also allow trailing + for append: arr+=val, arr+=(a b c)
                    let name_valid = eq_pos > 0 && word[..eq_pos].iter().all(|&b|
                        b.is_ascii_alphanumeric() || b == b'_' || b == b'[' || b == b']' || b == b'+'
                    );
                    if name_valid {
                        let name = word[..eq_pos].to_vec();
                        let mut value = word[eq_pos + 1..].to_vec();

                        // — ByteRiot: arr=(a b c) — array assignment. The word stopped
                        // at '(' because it's an unquoted terminator. Collect the entire
                        // parenthesized list as the assignment value so the evaluator
                        // can parse it as an array. Without this, we produce
                        // Assignment("arr","") + LParen + Words... which explodes.
                        if value.is_empty() && pos < input.len() && input[pos] == b'(' {
                            value.push(b'(');
                            pos += 1; // skip '('
                            let mut depth = 1u32;
                            while pos < input.len() && depth > 0 {
                                let c = input[pos];
                                if c == b'(' { depth += 1; }
                                if c == b')' { depth -= 1; }
                                value.push(c);
                                pos += 1;
                            }
                        }

                        tokens.push(Token::Assignment(name, value));
                        continue;
                    }
                }
            }

            // Check for keywords
            // — ByteRiot: keywords that start new commands reset seen_word.
            // Regular words set seen_word so subsequent NAME=VALUE is not
            // treated as an assignment.
            let tok = match word.as_slice() {
                b"if" => { seen_word = false; Token::If },
                b"then" => { seen_word = false; Token::Then },
                b"else" => { seen_word = false; Token::Else },
                b"elif" => { seen_word = false; Token::Elif },
                b"fi" => Token::Fi,
                b"for" => { seen_word = false; Token::For },
                b"while" => { seen_word = false; Token::While },
                b"until" => { seen_word = false; Token::Until },
                b"do" => { seen_word = false; Token::Do },
                b"done" => Token::Done,
                b"case" => { seen_word = false; Token::Case },
                b"esac" => Token::Esac,
                b"in" => Token::In,
                b"function" => { seen_word = false; Token::Function },
                b"select" => { seen_word = false; Token::Select },
                b"[[" => { seen_word = false; Token::DblLBracket },
                b"]]" => Token::DblRBracket,
                _ => { seen_word = true; Token::Word(word) },
            };
            tokens.push(tok);
        }
    }

    // — ByteRiot: edge case — input ended without a trailing newline but we
    // still have heredocs waiting for their bodies. This happens when the shell
    // receives multi-line input without a final \n. Drain them now or their
    // bodies stay empty forever, haunting the AST like ghost packets.
    for hdoc_idx in pending_heredocs.drain(..) {
        if let Token::HereDoc(ref delim, ref mut body, strip_tabs) = tokens[hdoc_idx] {
            let delim_copy = delim.clone();
            let mut collected = Vec::new();
            loop {
                if pos >= input.len() { break; }
                let line_start = pos;
                while pos < input.len() && input[pos] != b'\n' {
                    pos += 1;
                }
                let line = &input[line_start..pos];
                if pos < input.len() { pos += 1; }

                let check_line = if strip_tabs {
                    let mut start = 0;
                    while start < line.len() && line[start] == b'\t' { start += 1; }
                    &line[start..]
                } else {
                    line
                };

                if check_line == delim_copy.as_slice() { break; }

                if strip_tabs {
                    let mut start = 0;
                    while start < line.len() && line[start] == b'\t' { start += 1; }
                    collected.extend_from_slice(&line[start..]);
                } else {
                    collected.extend_from_slice(line);
                }
                collected.push(b'\n');
            }
            *body = collected;
        }
    }

    tokens.push(Token::Eof);
    tokens
}

/// Read a word from input, handling quoting.
/// — ByteRiot: single quotes preserve everything literally.
/// Double quotes allow $expansion (but we don't expand here — just collect).
/// Backslash escapes the next character.
/// Strip surrounding quotes from a heredoc delimiter
/// — ByteRiot: quoted delimiters suppress expansion in the body.
/// 'EOF' and "EOF" both become EOF, but the quoting tells the expander to keep its hands off.
fn strip_heredoc_quotes(delim: &[u8]) -> Vec<u8> {
    if delim.len() >= 2 {
        let first = delim[0];
        let last = delim[delim.len() - 1];
        if (first == b'\'' && last == b'\'') || (first == b'"' && last == b'"') {
            return delim[1..delim.len() - 1].to_vec();
        }
    }
    delim.to_vec()
}

fn read_word(input: &[u8], pos: &mut usize) -> Vec<u8> {
    let mut word = Vec::new();

    while *pos < input.len() {
        let ch = input[*pos];

        // Unquoted terminators
        if ch == b' ' || ch == b'\t' || ch == b'\n'
            || ch == b'|' || ch == b'&' || ch == b';'
            || ch == b'<' || ch == b'>' || ch == b'('
            || ch == b')' || ch == b'#'
        {
            break;
        }

        // Backslash escape
        if ch == b'\\' && *pos + 1 < input.len() {
            *pos += 1;
            word.push(input[*pos]);
            *pos += 1;
            continue;
        }

        // Single quote — everything literal until closing '
        if ch == b'\'' {
            *pos += 1;
            while *pos < input.len() && input[*pos] != b'\'' {
                word.push(input[*pos]);
                *pos += 1;
            }
            if *pos < input.len() { *pos += 1; } // skip closing '
            continue;
        }

        // Double quote — collect until closing ", preserve $VAR for later expansion
        if ch == b'"' {
            *pos += 1;
            while *pos < input.len() && input[*pos] != b'"' {
                if input[*pos] == b'\\' && *pos + 1 < input.len() {
                    *pos += 1;
                    word.push(input[*pos]);
                    *pos += 1;
                    continue;
                }
                word.push(input[*pos]);
                *pos += 1;
            }
            if *pos < input.len() { *pos += 1; } // skip closing "
            continue;
        }

        // Regular character
        word.push(ch);
        *pos += 1;
    }

    word
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command() {
        let tokens = tokenize(b"ls -la /tmp");
        assert_eq!(tokens[0], Token::Word(b"ls".to_vec()));
        assert_eq!(tokens[1], Token::Word(b"-la".to_vec()));
        assert_eq!(tokens[2], Token::Word(b"/tmp".to_vec()));
        assert_eq!(tokens[3], Token::Eof);
    }

    #[test]
    fn test_pipe() {
        let tokens = tokenize(b"ls | grep foo");
        assert_eq!(tokens[0], Token::Word(b"ls".to_vec()));
        assert_eq!(tokens[1], Token::Pipe);
        assert_eq!(tokens[2], Token::Word(b"grep".to_vec()));
        assert_eq!(tokens[3], Token::Word(b"foo".to_vec()));
    }

    #[test]
    fn test_and_or() {
        let tokens = tokenize(b"true && echo yes || echo no");
        assert_eq!(tokens[0], Token::Word(b"true".to_vec()));
        assert_eq!(tokens[1], Token::And);
        assert_eq!(tokens[2], Token::Word(b"echo".to_vec()));
        assert_eq!(tokens[3], Token::Word(b"yes".to_vec()));
        assert_eq!(tokens[4], Token::Or);
    }

    #[test]
    fn test_semicolons() {
        let tokens = tokenize(b"echo a; echo b; echo c");
        assert_eq!(tokens[0], Token::Word(b"echo".to_vec()));
        assert_eq!(tokens[1], Token::Word(b"a".to_vec()));
        assert_eq!(tokens[2], Token::Semi);
        assert_eq!(tokens[3], Token::Word(b"echo".to_vec()));
    }

    #[test]
    fn test_redirections() {
        let tokens = tokenize(b"cat < input > output");
        assert_eq!(tokens[0], Token::Word(b"cat".to_vec()));
        assert_eq!(tokens[1], Token::RedirIn);
        assert_eq!(tokens[2], Token::Word(b"input".to_vec()));
        assert_eq!(tokens[3], Token::RedirOut);
        assert_eq!(tokens[4], Token::Word(b"output".to_vec()));
    }

    #[test]
    fn test_append_redirect() {
        let tokens = tokenize(b"echo hi >> log");
        assert_eq!(tokens[0], Token::Word(b"echo".to_vec()));
        assert_eq!(tokens[1], Token::Word(b"hi".to_vec()));
        assert_eq!(tokens[2], Token::RedirAppend);
        assert_eq!(tokens[3], Token::Word(b"log".to_vec()));
    }

    #[test]
    fn test_keywords() {
        let tokens = tokenize(b"if true; then echo yes; fi");
        assert_eq!(tokens[0], Token::If);
        assert_eq!(tokens[1], Token::Word(b"true".to_vec()));
        assert_eq!(tokens[2], Token::Semi);
        assert_eq!(tokens[3], Token::Then);
    }

    #[test]
    fn test_for_loop() {
        let tokens = tokenize(b"for x in a b c; do echo $x; done");
        assert_eq!(tokens[0], Token::For);
        assert_eq!(tokens[1], Token::Word(b"x".to_vec()));
        assert_eq!(tokens[2], Token::In);
        assert_eq!(tokens[3], Token::Word(b"a".to_vec()));
    }

    #[test]
    fn test_single_quotes() {
        let tokens = tokenize(b"echo 'hello world'");
        assert_eq!(tokens[0], Token::Word(b"echo".to_vec()));
        assert_eq!(tokens[1], Token::Word(b"hello world".to_vec()));
    }

    #[test]
    fn test_double_quotes() {
        let tokens = tokenize(b"echo \"hello $USER\"");
        assert_eq!(tokens[0], Token::Word(b"echo".to_vec()));
        assert_eq!(tokens[1], Token::Word(b"hello $USER".to_vec()));
    }

    #[test]
    fn test_assignment() {
        let tokens = tokenize(b"FOO=bar echo $FOO");
        assert_eq!(tokens[0], Token::Assignment(b"FOO".to_vec(), b"bar".to_vec()));
        assert_eq!(tokens[1], Token::Word(b"echo".to_vec()));
    }

    #[test]
    fn test_background() {
        let tokens = tokenize(b"sleep 10 &");
        assert_eq!(tokens[0], Token::Word(b"sleep".to_vec()));
        assert_eq!(tokens[1], Token::Word(b"10".to_vec()));
        assert_eq!(tokens[2], Token::Background);
    }

    #[test]
    fn test_fd_redirect() {
        let tokens = tokenize(b"cmd 2>/dev/null");
        assert_eq!(tokens[0], Token::Word(b"cmd".to_vec()));
        assert_eq!(tokens[1], Token::RedirFd(2));
        assert_eq!(tokens[2], Token::Word(b"/dev/null".to_vec()));
    }

    #[test]
    fn test_comment() {
        let tokens = tokenize(b"echo hello # this is a comment");
        assert_eq!(tokens[0], Token::Word(b"echo".to_vec()));
        assert_eq!(tokens[1], Token::Word(b"hello".to_vec()));
        assert_eq!(tokens[2], Token::Eof);
    }

    #[test]
    fn test_empty() {
        let tokens = tokenize(b"");
        assert_eq!(tokens[0], Token::Eof);
    }

    #[test]
    fn test_backslash_escape() {
        let tokens = tokenize(b"echo hello\\ world");
        assert_eq!(tokens[0], Token::Word(b"echo".to_vec()));
        assert_eq!(tokens[1], Token::Word(b"hello world".to_vec()));
    }

    #[test]
    fn test_heredoc_body_collection() {
        // — ByteRiot: the main event — heredoc body should actually contain content
        let tokens = tokenize(b"cat <<EOF\nhello world\ngoodbye\nEOF\n");
        assert_eq!(tokens[0], Token::Word(b"cat".to_vec()));
        if let Token::HereDoc(ref delim, ref body, strip_tabs) = tokens[1] {
            assert_eq!(delim, b"EOF");
            assert_eq!(body, b"hello world\ngoodbye\n");
            assert_eq!(strip_tabs, false);
        } else {
            panic!("expected HereDoc token");
        }
    }

    #[test]
    fn test_heredoc_strip_tabs() {
        // — ByteRiot: <<- should strip leading tabs from body and delimiter line
        let tokens = tokenize(b"cat <<-EOF\n\thello\n\t\tindented\n\tEOF\n");
        if let Token::HereDoc(ref delim, ref body, strip_tabs) = tokens[1] {
            assert_eq!(delim, b"EOF");
            assert_eq!(body, b"hello\nindented\n");
            assert_eq!(strip_tabs, true);
        } else {
            panic!("expected HereDoc token");
        }
    }

    #[test]
    fn test_heredoc_with_redirect() {
        // — ByteRiot: heredoc mid-line with redirect after — cat <<EOF > out
        let tokens = tokenize(b"cat <<EOF > out\nline1\nEOF\n");
        assert_eq!(tokens[0], Token::Word(b"cat".to_vec()));
        // HereDoc token
        if let Token::HereDoc(ref delim, ref body, _) = tokens[1] {
            assert_eq!(delim, b"EOF");
            assert_eq!(body, b"line1\n");
        } else {
            panic!("expected HereDoc token");
        }
        assert_eq!(tokens[2], Token::RedirOut);
        assert_eq!(tokens[3], Token::Word(b"out".to_vec()));
    }

    #[test]
    fn test_heredoc_empty_body() {
        // — ByteRiot: delimiter immediately on next line — empty body is valid
        let tokens = tokenize(b"cat <<EOF\nEOF\n");
        if let Token::HereDoc(_, ref body, _) = tokens[1] {
            assert!(body.is_empty());
        } else {
            panic!("expected HereDoc token");
        }
    }

    #[test]
    fn test_heredoc_no_trailing_newline() {
        // — ByteRiot: input ends without trailing newline after delimiter
        let tokens = tokenize(b"cat <<EOF\nstuff\nEOF");
        if let Token::HereDoc(_, ref body, _) = tokens[1] {
            assert_eq!(body, b"stuff\n");
        } else {
            panic!("expected HereDoc token");
        }
    }

    // — CrashBloom: new token tests for Phase 1-3 features

    #[test]
    fn test_bang_token() {
        let tokens = tokenize(b"! grep error log");
        assert_eq!(tokens[0], Token::Bang);
        assert_eq!(tokens[1], Token::Word(b"grep".to_vec()));
    }

    #[test]
    fn test_double_semi() {
        let tokens = tokenize(b"case x in a) echo a;; esac");
        // Find DoubleSemi
        let has_doublesemi = tokens.iter().any(|t| *t == Token::DoubleSemi);
        assert!(has_doublesemi, "expected DoubleSemi token");
    }

    #[test]
    fn test_double_bracket() {
        let tokens = tokenize(b"[[ -f /tmp ]]");
        assert_eq!(tokens[0], Token::DblLBracket);
        assert_eq!(tokens[3], Token::DblRBracket);
    }

    #[test]
    fn test_here_string() {
        let tokens = tokenize(b"cat <<< hello");
        assert_eq!(tokens[0], Token::Word(b"cat".to_vec()));
        assert_eq!(tokens[1], Token::HereString);
        assert_eq!(tokens[2], Token::Word(b"hello".to_vec()));
    }

    #[test]
    fn test_function_keyword_token() {
        let tokens = tokenize(b"function foo");
        assert_eq!(tokens[0], Token::Function);
        assert_eq!(tokens[1], Token::Word(b"foo".to_vec()));
    }

    #[test]
    fn test_case_esac_in_tokens() {
        let tokens = tokenize(b"case x in esac");
        assert_eq!(tokens[0], Token::Case);
        assert_eq!(tokens[1], Token::Word(b"x".to_vec()));
        assert_eq!(tokens[2], Token::In);
        assert_eq!(tokens[3], Token::Esac);
    }

    #[test]
    fn test_bang_in_pipeline() {
        let tokens = tokenize(b"! cmd1 | cmd2");
        assert_eq!(tokens[0], Token::Bang);
        assert_eq!(tokens[1], Token::Word(b"cmd1".to_vec()));
        assert_eq!(tokens[2], Token::Pipe);
        assert_eq!(tokens[3], Token::Word(b"cmd2".to_vec()));
    }

    #[test]
    fn test_semi_vs_doublesemi() {
        // — CrashBloom: ;; is case terminator, ; is statement separator — don't confuse them
        let tokens = tokenize(b"echo a;; echo b");
        assert_eq!(tokens[0], Token::Word(b"echo".to_vec()));
        assert_eq!(tokens[1], Token::Word(b"a".to_vec()));
        assert_eq!(tokens[2], Token::DoubleSemi);
    }

    // =========================================================================
    // — CrashBloom: P0-P10 feature coverage — tokenizer must handle all new syntax
    // =========================================================================

    #[test]
    fn test_select_keyword() {
        // — CrashBloom: select must tokenize as keyword, not a word
        let tokens = tokenize(b"select x in a b c; do echo $x; done");
        assert_eq!(tokens[0], Token::Select);
        assert_eq!(tokens[1], Token::Word(b"x".to_vec()));
        assert_eq!(tokens[2], Token::In);
    }

    #[test]
    fn test_array_assignment_tokenizes() {
        // — CrashBloom: arr=(a b c) should tokenize the assignment part
        let tokens = tokenize(b"arr=hello");
        assert_eq!(tokens[0], Token::Assignment(b"arr".to_vec(), b"hello".to_vec()));
    }

    #[test]
    fn test_indexed_assignment_tokenizes() {
        // — CrashBloom: x[0]=val must tokenize as Assignment with bracket in name
        let tokens = tokenize(b"x[0]=val");
        assert_eq!(tokens[0], Token::Assignment(b"x[0]".to_vec(), b"val".to_vec()));
    }

    #[test]
    fn test_array_assignment_parens() {
        // — CrashBloom: arr=(a b c) must tokenize as a single Assignment token
        // with the parenthesized list as the value
        let tokens = tokenize(b"arr=(a b c)");
        match &tokens[0] {
            Token::Assignment(name, value) => {
                assert_eq!(name, b"arr");
                assert_eq!(value, b"(a b c)");
            }
            _ => panic!("expected Assignment for arr=(a b c), got {:?}", tokens[0]),
        }
    }

    #[test]
    fn test_array_assignment_then_command() {
        // — CrashBloom: arr=(a b c); echo test — semicolon after array assignment
        let tokens = tokenize(b"arr=(a b c); echo test");
        assert!(matches!(&tokens[0], Token::Assignment(n, v) if n == b"arr" && v == b"(a b c)"));
        assert_eq!(tokens[1], Token::Semi);
        assert_eq!(tokens[2], Token::Word(b"echo".to_vec()));
    }

    #[test]
    fn test_array_append_assignment() {
        // — CrashBloom: arr+=(d e) must tokenize correctly
        let tokens = tokenize(b"arr+=(d e)");
        match &tokens[0] {
            Token::Assignment(name, value) => {
                assert_eq!(name, b"arr+");
                assert_eq!(value, b"(d e)");
            }
            _ => panic!("expected Assignment for arr+=(d e), got {:?}", tokens[0]),
        }
    }

    #[test]
    fn test_function_def_braces() {
        // — CrashBloom: greet() { echo hello; } — tokenizer strips () from name
        let tokens = tokenize(b"greet() { echo hello; }");
        assert_eq!(tokens[0], Token::Word(b"greet".to_vec()));
        assert_eq!(tokens[1], Token::LParen);
        assert_eq!(tokens[2], Token::RParen);
    }

    #[test]
    fn test_dollar_braced_expansion_in_word() {
        // — CrashBloom: ${arr[1]} should be a single word token
        let tokens = tokenize(b"echo ${arr[1]}");
        assert_eq!(tokens[0], Token::Word(b"echo".to_vec()));
        // The ${arr[1]} should be a word containing the unexpanded expression
        match &tokens[1] {
            Token::Word(w) => assert!(w.starts_with(b"$"), "word should start with $, got {:?}", w),
            _ => panic!("expected Word token for ${{}}, got {:?}", tokens[1]),
        }
    }

    #[test]
    fn test_local_keyword_not_special() {
        // — CrashBloom: 'local' is a builtin, not a keyword — should tokenize as Word
        let tokens = tokenize(b"local x=5");
        assert_eq!(tokens[0], Token::Word(b"local".to_vec()));
    }

    #[test]
    fn test_getopts_tokenizes() {
        // — CrashBloom: getopts is a builtin — should tokenize as regular words
        let tokens = tokenize(b"getopts ab: opt");
        assert_eq!(tokens[0], Token::Word(b"getopts".to_vec()));
        assert_eq!(tokens[1], Token::Word(b"ab:".to_vec()));
        assert_eq!(tokens[2], Token::Word(b"opt".to_vec()));
    }

    #[test]
    fn test_process_substitution_input() {
        // — CrashBloom: <(echo hello) must tokenize as a single Word, not RedirIn + LParen
        let tokens = tokenize(b"cat <(echo hello)");
        assert_eq!(tokens[0], Token::Word(b"cat".to_vec()));
        assert_eq!(tokens[1], Token::Word(b"<(echo hello)".to_vec()));
    }

    #[test]
    fn test_process_substitution_output() {
        // — CrashBloom: >(cmd) must tokenize as a single Word
        let tokens = tokenize(b"tee >(grep foo)");
        assert_eq!(tokens[0], Token::Word(b"tee".to_vec()));
        assert_eq!(tokens[1], Token::Word(b">(grep foo)".to_vec()));
    }

    #[test]
    fn test_heredoc_multi_line_body() {
        let tokens = tokenize(b"cat <<MARKER\nline one\nline two\nline three\nMARKER\n");
        if let Token::HereDoc(ref delim, ref body, _) = tokens[1] {
            assert_eq!(delim, b"MARKER");
            assert_eq!(body, b"line one\nline two\nline three\n");
        } else {
            panic!("expected HereDoc token");
        }
    }
}
