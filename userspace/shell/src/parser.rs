//! Shell Parser — recursive descent parser producing AST nodes
//!
//! — ByteRiot: the grammar whisperer. Takes a token stream from the lexer
//! and builds an Abstract Syntax Tree. Handles operator precedence (| binds
//! tighter than && which binds tighter than ;), nested control flow, and
//! proper error recovery.
//!
//! Grammar (POSIX sh + bash extensions):
//!   program     → compound_list (';'|'\n')* EOF
//!   compound_list → pipeline (('&&'|'||') pipeline)*
//!   pipeline    → ['!'] command ('|' command)*
//!   command     → simple_cmd | if_cmd | for_cmd | while_cmd | until_cmd
//!               | case_cmd | func_def | '(' program ')' | '{' program '}'
//!               | extended_test
//!   simple_cmd  → (assignment)* word* (redirection)*
//!   if_cmd      → 'if' compound_list ';'? 'then' program
//!                 ('elif' compound_list ';'? 'then' program)*
//!                 ('else' program)? 'fi'
//!   for_cmd     → 'for' WORD 'in' word* ';'? 'do' program 'done'
//!   while_cmd   → 'while' compound_list ';'? 'do' program 'done'
//!   case_cmd    → 'case' WORD 'in' (pattern ('|' pattern)* ')' program ';;')* 'esac'
//!   func_def    → 'function' WORD '{' program '}' | WORD '(' ')' '{' program '}'
//!   extended_test → '[[' test_expr ']]'

extern crate alloc;
use alloc::vec::Vec;
use alloc::boxed::Box;
use crate::token::Token;
use crate::ast::*;

/// Parser state
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

/// Parse error
#[derive(Debug)]
pub struct ParseError {
    pub message: &'static str,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    /// Peek at the current token without consuming it
    fn peek(&self) -> &Token {
        if self.pos < self.tokens.len() {
            &self.tokens[self.pos]
        } else {
            &Token::Eof
        }
    }

    /// Peek at token N positions ahead without consuming
    /// — ByteRiot: look-ahead for disambiguating name() vs name arg
    fn peek_at(&self, offset: usize) -> &Token {
        let idx = self.pos + offset;
        if idx < self.tokens.len() {
            &self.tokens[idx]
        } else {
            &Token::Eof
        }
    }

    /// Consume and return the current token
    fn next(&mut self) -> Token {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            tok
        } else {
            Token::Eof
        }
    }

    /// Check if current token matches expected, consume if so
    fn expect(&mut self, expected: &Token) -> bool {
        if self.peek() == expected {
            self.next();
            true
        } else {
            false
        }
    }

    /// Skip semicolons and newlines (statement terminators)
    fn skip_terminators(&mut self) {
        while matches!(self.peek(), Token::Semi | Token::Newline) {
            self.next();
        }
    }

    /// Is the current token a statement terminator?
    fn is_terminator(&self) -> bool {
        matches!(self.peek(), Token::Semi | Token::Newline | Token::Eof)
    }

    /// Parse a complete program
    /// — ByteRiot: also stops on DoubleSemi for case arm bodies
    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut commands = Vec::new();
        self.skip_terminators();

        while !matches!(self.peek(),
            Token::Eof | Token::Fi | Token::Done | Token::Else | Token::Elif
            | Token::RParen | Token::Esac | Token::DoubleSemi
        ) {
            // — ByteRiot: detect brace-group closing `}` as a Word token
            if let Token::Word(w) = self.peek() {
                if w.as_slice() == b"}" { break; }
            }
            let list = self.parse_compound_list()?;
            commands.push(list);
            self.skip_terminators();
        }

        Ok(Program { commands })
    }

    /// Parse a compound list: pipeline (&&||| pipeline)*
    fn parse_compound_list(&mut self) -> Result<CompoundList, ParseError> {
        let first = self.parse_pipeline()?;
        let mut rest = Vec::new();

        loop {
            match self.peek() {
                Token::And => {
                    self.next();
                    self.skip_terminators();
                    let pipe = self.parse_pipeline()?;
                    rest.push((ListOp::And, pipe));
                }
                Token::Or => {
                    self.next();
                    self.skip_terminators();
                    let pipe = self.parse_pipeline()?;
                    rest.push((ListOp::Or, pipe));
                }
                _ => break,
            }
        }

        // Check for background &
        let background = self.expect(&Token::Background);

        Ok(CompoundList { first, rest, background })
    }

    /// Parse a pipeline: ['!'] command ('|' command)*
    /// — ByteRiot: now with negation support. `! cmd` inverts the exit status.
    fn parse_pipeline(&mut self) -> Result<Pipeline, ParseError> {
        // — ByteRiot: check for ! prefix — negates the pipeline exit status.
        // Every script that does `if ! grep ...` depends on this.
        let negated = self.expect(&Token::Bang);

        let mut commands = Vec::new();
        let cmd = self.parse_command()?;
        commands.push(cmd);

        while self.expect(&Token::Pipe) {
            self.skip_terminators();
            let cmd = self.parse_command()?;
            commands.push(cmd);
        }

        Ok(Pipeline { commands, negated })
    }

    /// Parse a single command (simple, if, for, while, case, function, subshell, group, [[]])
    /// — ByteRiot: the dispatcher. Figures out what kind of command we're looking
    /// at and routes to the right parser. The trickiest part: distinguishing
    /// `name() { body }` from `name arg1 arg2`.
    fn parse_command(&mut self) -> Result<Command, ParseError> {
        match self.peek() {
            Token::If => self.parse_if(),
            Token::For => self.parse_for(),
            Token::While => self.parse_while(),
            Token::Until => self.parse_until(),
            Token::Case => self.parse_case(),
            Token::Select => self.parse_select(),
            Token::Function => self.parse_function(),
            Token::LParen => self.parse_subshell(),
            Token::DblLBracket => self.parse_extended_test(),
            Token::Word(w) => {
                // — ByteRiot: disambiguation hell. Is `foo(` the start of a
                // function definition or just a word? Peek ahead 2 tokens:
                // Word + LParen + RParen = function def.
                if w.as_slice() == b"{" {
                    return self.parse_group();
                }
                if matches!(self.peek_at(1), Token::LParen)
                    && matches!(self.peek_at(2), Token::RParen)
                {
                    return self.parse_function_shorthand();
                }
                self.parse_simple_command()
            }
            _ => self.parse_simple_command(),
        }
    }

    /// Parse a simple command: [assignments] words [redirections]
    fn parse_simple_command(&mut self) -> Result<Command, ParseError> {
        let mut assignments = Vec::new();
        let mut words = Vec::new();
        let mut redirections = Vec::new();

        // Collect prefix assignments
        while let Token::Assignment(name, value) = self.peek() {
            let name = name.clone();
            let value = value.clone();
            self.next();
            assignments.push(Assignment { name, value });
        }

        // Collect words and redirections
        loop {
            match self.peek() {
                Token::Word(w) => {
                    let w = w.clone();
                    self.next();
                    words.push(w);
                }
                Token::RedirIn => {
                    self.next();
                    if let Token::Word(target) = self.peek() {
                        let target = target.clone();
                        self.next();
                        redirections.push(Redirect { fd: 0, rtype: RedirectType::Input, target });
                    }
                }
                Token::RedirOut => {
                    self.next();
                    if let Token::Word(target) = self.peek() {
                        let target = target.clone();
                        self.next();
                        redirections.push(Redirect { fd: 1, rtype: RedirectType::Output, target });
                    }
                }
                Token::RedirAppend => {
                    self.next();
                    if let Token::Word(target) = self.peek() {
                        let target = target.clone();
                        self.next();
                        redirections.push(Redirect { fd: 1, rtype: RedirectType::Append, target });
                    }
                }
                Token::RedirFd(fd) => {
                    let fd = *fd;
                    self.next();
                    if let Token::Word(target) = self.peek() {
                        let target = target.clone();
                        self.next();
                        redirections.push(Redirect { fd: fd as i32, rtype: RedirectType::Output, target });
                    }
                }
                Token::RedirDup(src, dst) => {
                    let src = *src;
                    let dst = *dst;
                    self.next();
                    redirections.push(Redirect {
                        fd: src as i32,
                        rtype: RedirectType::DupOut,
                        target: alloc::vec![dst + b'0'],
                    });
                }
                // — ByteRiot: heredoc redirections land here from the tokenizer
                Token::HereDoc(delim, body, strip) => {
                    let body = body.clone();
                    let strip = *strip;
                    self.next();
                    let rtype = if strip { RedirectType::HereDocStrip } else { RedirectType::HereDoc };
                    redirections.push(Redirect { fd: 0, rtype, target: body });
                }
                // — ByteRiot: here-string — `<<< word` pipes a string to stdin
                Token::HereString => {
                    self.next();
                    if let Token::Word(target) = self.peek() {
                        let target = target.clone();
                        self.next();
                        redirections.push(Redirect { fd: 0, rtype: RedirectType::HereString, target });
                    }
                }
                _ => break,
            }
        }

        if words.is_empty() && assignments.is_empty() {
            return Err(ParseError { message: "expected command" });
        }

        Ok(Command::Simple(SimpleCommand { assignments, words, redirections }))
    }

    /// Parse if command
    fn parse_if(&mut self) -> Result<Command, ParseError> {
        self.next(); // consume 'if'
        let mut branches = Vec::new();

        // First if condition + body
        let cond = self.parse_compound_list()?;
        self.skip_terminators();
        if !self.expect(&Token::Then) {
            return Err(ParseError { message: "expected 'then'" });
        }
        let body = self.parse_program()?;
        branches.push((cond, body));

        // elif branches
        while self.expect(&Token::Elif) {
            let cond = self.parse_compound_list()?;
            self.skip_terminators();
            if !self.expect(&Token::Then) {
                return Err(ParseError { message: "expected 'then' after 'elif'" });
            }
            let body = self.parse_program()?;
            branches.push((cond, body));
        }

        // else branch
        let else_body = if self.expect(&Token::Else) {
            Some(self.parse_program()?)
        } else {
            None
        };

        if !self.expect(&Token::Fi) {
            return Err(ParseError { message: "expected 'fi'" });
        }

        Ok(Command::If(IfCommand { branches, else_body }))
    }

    /// Parse for command
    fn parse_for(&mut self) -> Result<Command, ParseError> {
        self.next(); // consume 'for'

        let var_name = match self.next() {
            Token::Word(w) => w,
            _ => return Err(ParseError { message: "expected variable name after 'for'" }),
        };

        // Optional 'in' word list
        let words = if self.expect(&Token::In) {
            let mut words = Vec::new();
            while let Token::Word(w) = self.peek() {
                words.push(w.clone());
                self.next();
            }
            words
        } else {
            Vec::new() // defaults to "$@"
        };

        self.skip_terminators();
        if !self.expect(&Token::Do) {
            return Err(ParseError { message: "expected 'do'" });
        }

        let body = self.parse_program()?;

        if !self.expect(&Token::Done) {
            return Err(ParseError { message: "expected 'done'" });
        }

        Ok(Command::For(ForCommand { var_name, words, body }))
    }

    /// Parse while command
    fn parse_while(&mut self) -> Result<Command, ParseError> {
        self.next(); // consume 'while'
        let condition = self.parse_compound_list()?;
        self.skip_terminators();
        if !self.expect(&Token::Do) {
            return Err(ParseError { message: "expected 'do'" });
        }
        let body = self.parse_program()?;
        if !self.expect(&Token::Done) {
            return Err(ParseError { message: "expected 'done'" });
        }
        Ok(Command::While(WhileCommand { condition, body }))
    }

    /// Parse until command (like while but inverted)
    fn parse_until(&mut self) -> Result<Command, ParseError> {
        self.next(); // consume 'until'
        let condition = self.parse_compound_list()?;
        self.skip_terminators();
        if !self.expect(&Token::Do) {
            return Err(ParseError { message: "expected 'do'" });
        }
        let body = self.parse_program()?;
        if !self.expect(&Token::Done) {
            return Err(ParseError { message: "expected 'done'" });
        }
        Ok(Command::Until(WhileCommand { condition, body }))
    }

    /// Parse case command
    /// — ByteRiot: pattern matching, shell-style. The grammar here is
    /// delightfully cursed: `case $x in pat1|pat2) body;; pat3) body;; esac`
    /// Where | inside case arms is a pattern separator, NOT a pipe.
    fn parse_case(&mut self) -> Result<Command, ParseError> {
        self.next(); // consume 'case'

        let word = match self.next() {
            Token::Word(w) => w,
            _ => return Err(ParseError { message: "expected word after 'case'" }),
        };

        self.skip_terminators();
        if !self.expect(&Token::In) {
            return Err(ParseError { message: "expected 'in' after case word" });
        }
        self.skip_terminators();

        let mut arms = Vec::new();

        while !matches!(self.peek(), Token::Esac | Token::Eof) {
            // — ByteRiot: optional leading ( before patterns — some scripts use it
            let _ = self.expect(&Token::LParen);

            // Parse patterns: PAT (| PAT)*
            let mut patterns = Vec::new();
            match self.next() {
                Token::Word(w) => patterns.push(w),
                Token::Esac => break, // empty case
                _ => return Err(ParseError { message: "expected pattern in case arm" }),
            }

            // Additional patterns separated by |
            while self.expect(&Token::Pipe) {
                match self.next() {
                    Token::Word(w) => patterns.push(w),
                    _ => return Err(ParseError { message: "expected pattern after '|'" }),
                }
            }

            // Expect )
            if !self.expect(&Token::RParen) {
                return Err(ParseError { message: "expected ')' after case pattern" });
            }

            self.skip_terminators();

            // Parse body (stops at ;; or esac)
            let body = self.parse_program()?;

            arms.push(CaseArm { patterns, body });

            // Expect ;; or esac
            if self.expect(&Token::DoubleSemi) {
                self.skip_terminators();
            } else {
                // — ByteRiot: last arm before esac doesn't need ;;
                break;
            }
        }

        if !self.expect(&Token::Esac) {
            return Err(ParseError { message: "expected 'esac'" });
        }

        Ok(Command::Case(CaseCommand { word, arms }))
    }

    /// Parse select command: `select name in words; do body; done`
    /// — IronGhost: interactive menu. Same grammar as for-loop, different semantics.
    fn parse_select(&mut self) -> Result<Command, ParseError> {
        self.next(); // consume 'select'

        let var_name = match self.next() {
            Token::Word(w) => w,
            _ => return Err(ParseError { message: "expected variable name after 'select'" }),
        };

        let words = if self.expect(&Token::In) {
            let mut words = Vec::new();
            while let Token::Word(w) = self.peek() {
                words.push(w.clone());
                self.next();
            }
            words
        } else {
            Vec::new()
        };

        self.skip_terminators();
        if !self.expect(&Token::Do) {
            return Err(ParseError { message: "expected 'do' in select" });
        }

        let body = self.parse_program()?;

        if !self.expect(&Token::Done) {
            return Err(ParseError { message: "expected 'done' in select" });
        }

        Ok(Command::Select(SelectCommand { var_name, words, body }))
    }

    /// Parse function definition: `function name { body; }`
    /// — ByteRiot: the `function` keyword form. Bash extension, widely used.
    fn parse_function(&mut self) -> Result<Command, ParseError> {
        self.next(); // consume 'function'

        let name = match self.next() {
            Token::Word(w) => w,
            _ => return Err(ParseError { message: "expected function name" }),
        };

        // Optional () after name
        if self.expect(&Token::LParen) {
            if !self.expect(&Token::RParen) {
                return Err(ParseError { message: "expected ')' after '('" });
            }
        }

        self.skip_terminators();

        // Expect { body }
        let body = self.parse_function_body()?;

        Ok(Command::FunctionDef { name, body: Box::new(body) })
    }

    /// Parse function shorthand: `name() { body; }`
    /// — ByteRiot: POSIX form. name() is the indicator, then { body; }.
    fn parse_function_shorthand(&mut self) -> Result<Command, ParseError> {
        let name = match self.next() {
            Token::Word(w) => w,
            _ => return Err(ParseError { message: "expected function name" }),
        };

        self.next(); // consume (
        self.next(); // consume )
        self.skip_terminators();

        let body = self.parse_function_body()?;

        Ok(Command::FunctionDef { name, body: Box::new(body) })
    }

    /// Parse function body: `{ program }`
    /// — ByteRiot: shared between `function name { ... }` and `name() { ... }`
    fn parse_function_body(&mut self) -> Result<Program, ParseError> {
        // Expect opening {
        match self.peek() {
            Token::Word(w) if w.as_slice() == b"{" => { self.next(); }
            _ => return Err(ParseError { message: "expected '{' for function body" }),
        }

        let body = self.parse_program()?;

        // Expect closing }
        match self.peek() {
            Token::Word(w) if w.as_slice() == b"}" => { self.next(); }
            _ => return Err(ParseError { message: "expected '}' to close function body" }),
        }

        Ok(body)
    }

    /// Parse brace group: { program; }
    /// — ByteRiot: brace groups execute in the CURRENT shell, unlike subshells.
    /// `{ cd /tmp; ls; }` actually changes the working directory.
    fn parse_group(&mut self) -> Result<Command, ParseError> {
        // Consume opening {
        self.next();
        self.skip_terminators();

        let prog = self.parse_program()?;

        // Expect closing }
        match self.peek() {
            Token::Word(w) if w.as_slice() == b"}" => { self.next(); }
            _ => return Err(ParseError { message: "expected '}'" }),
        }

        Ok(Command::Group(prog))
    }

    /// Parse subshell: ( program )
    fn parse_subshell(&mut self) -> Result<Command, ParseError> {
        self.next(); // consume '('
        let prog = self.parse_program()?;
        if !self.expect(&Token::RParen) {
            return Err(ParseError { message: "expected ')'" });
        }
        Ok(Command::Subshell(prog))
    }

    /// Parse extended test: [[ expr ]]
    /// — ByteRiot: bash's improved test syntax. No word splitting, no globbing
    /// on the operands, && and || as logical operators inside.
    fn parse_extended_test(&mut self) -> Result<Command, ParseError> {
        self.next(); // consume [[
        let expr = self.parse_test_or()?;
        if !self.expect(&Token::DblRBracket) {
            return Err(ParseError { message: "expected ']]'" });
        }
        Ok(Command::ExtendedTest(expr))
    }

    /// Parse test OR expression: expr || expr
    fn parse_test_or(&mut self) -> Result<TestExpr, ParseError> {
        let mut left = self.parse_test_and()?;
        while self.expect(&Token::Or) {
            let right = self.parse_test_and()?;
            left = TestExpr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parse test AND expression: expr && expr
    fn parse_test_and(&mut self) -> Result<TestExpr, ParseError> {
        let mut left = self.parse_test_not()?;
        while self.expect(&Token::And) {
            let right = self.parse_test_not()?;
            left = TestExpr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parse test NOT expression: ! expr
    fn parse_test_not(&mut self) -> Result<TestExpr, ParseError> {
        if self.expect(&Token::Bang) {
            let inner = self.parse_test_primary()?;
            return Ok(TestExpr::Not(Box::new(inner)));
        }
        self.parse_test_primary()
    }

    /// Parse test primary: unary, binary, or literal
    /// — ByteRiot: the leaf-level of the test expression tree.
    fn parse_test_primary(&mut self) -> Result<TestExpr, ParseError> {
        // Parenthesized sub-expression
        if self.expect(&Token::LParen) {
            let inner = self.parse_test_or()?;
            if !self.expect(&Token::RParen) {
                return Err(ParseError { message: "expected ')' in test expression" });
            }
            return Ok(inner);
        }

        match self.peek() {
            Token::Word(w) => {
                let first = w.clone();
                self.next();

                // — ByteRiot: check if this is a unary operator (-f, -d, -z, -n, etc.)
                if first.len() == 2 && first[0] == b'-' {
                    let op_char = first[1];
                    if matches!(op_char, b'f' | b'd' | b'e' | b'r' | b'w' | b'x' | b's'
                                | b'z' | b'n' | b'L' | b'S' | b'p' | b'b' | b'c'
                                | b'v' | b'h')
                    {
                        // It's a unary op — next token is the operand
                        if matches!(self.peek(), Token::DblRBracket | Token::Eof) {
                            // No operand — treat as literal
                            return Ok(TestExpr::Literal(first));
                        }
                        match self.next() {
                            Token::Word(operand) => return Ok(TestExpr::Unary(first, operand)),
                            _ => return Err(ParseError { message: "expected operand for test operator" }),
                        }
                    }
                }

                // Check if next token is a binary operator
                match self.peek() {
                    Token::Word(op) if is_test_binary_op(op) => {
                        let op = op.clone();
                        self.next();
                        match self.next() {
                            Token::Word(right) => Ok(TestExpr::Binary(first, op, right)),
                            _ => Err(ParseError { message: "expected operand after binary test op" }),
                        }
                    }
                    _ => {
                        // Literal — true if non-empty string
                        Ok(TestExpr::Literal(first))
                    }
                }
            }
            _ => Err(ParseError { message: "expected test expression" }),
        }
    }
}

/// Check if a word is a binary test operator
fn is_test_binary_op(w: &Vec<u8>) -> bool {
    match w.as_slice() {
        b"==" | b"=" | b"!=" | b"=~"
        | b"-eq" | b"-ne" | b"-lt" | b"-le" | b"-gt" | b"-ge"
        | b"-nt" | b"-ot" | b"-ef"
        | b"<" | b">" => true,
        _ => false,
    }
}

/// Convenience: parse a token stream into a Program
pub fn parse(tokens: Vec<Token>) -> Result<Program, ParseError> {
    Parser::new(tokens).parse_program()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::tokenize;

    #[test]
    fn test_simple_command() {
        let prog = parse(tokenize(b"ls -la")).unwrap();
        assert_eq!(prog.commands.len(), 1);
        if let Command::Simple(ref sc) = prog.commands[0].first.commands[0] {
            assert_eq!(sc.words.len(), 2);
            assert_eq!(sc.words[0], b"ls");
            assert_eq!(sc.words[1], b"-la");
        } else {
            panic!("expected simple command");
        }
    }

    #[test]
    fn test_pipeline() {
        let prog = parse(tokenize(b"ls | grep foo | wc -l")).unwrap();
        assert_eq!(prog.commands[0].first.commands.len(), 3);
    }

    #[test]
    fn test_and_or() {
        let prog = parse(tokenize(b"true && echo yes || echo no")).unwrap();
        assert_eq!(prog.commands[0].rest.len(), 2);
        assert_eq!(prog.commands[0].rest[0].0, ListOp::And);
        assert_eq!(prog.commands[0].rest[1].0, ListOp::Or);
    }

    #[test]
    fn test_if_then_fi() {
        let prog = parse(tokenize(b"if true; then echo yes; fi")).unwrap();
        if let Command::If(ref ic) = prog.commands[0].first.commands[0] {
            assert_eq!(ic.branches.len(), 1);
            assert!(ic.else_body.is_none());
        } else {
            panic!("expected if command");
        }
    }

    #[test]
    fn test_if_else() {
        let prog = parse(tokenize(b"if false; then echo no; else echo yes; fi")).unwrap();
        if let Command::If(ref ic) = prog.commands[0].first.commands[0] {
            assert!(ic.else_body.is_some());
        } else {
            panic!("expected if command");
        }
    }

    #[test]
    fn test_for_loop() {
        let prog = parse(tokenize(b"for x in a b c; do echo $x; done")).unwrap();
        if let Command::For(ref fc) = prog.commands[0].first.commands[0] {
            assert_eq!(fc.var_name, b"x");
            assert_eq!(fc.words.len(), 3);
        } else {
            panic!("expected for command");
        }
    }

    #[test]
    fn test_while_loop() {
        let prog = parse(tokenize(b"while true; do echo loop; done")).unwrap();
        if let Command::While(_) = prog.commands[0].first.commands[0] {
            // ok
        } else {
            panic!("expected while command");
        }
    }

    #[test]
    fn test_semicolons() {
        let prog = parse(tokenize(b"echo a; echo b; echo c")).unwrap();
        assert_eq!(prog.commands.len(), 3);
    }

    #[test]
    fn test_redirect() {
        let prog = parse(tokenize(b"cat < input > output")).unwrap();
        if let Command::Simple(ref sc) = prog.commands[0].first.commands[0] {
            assert_eq!(sc.redirections.len(), 2);
            assert_eq!(sc.redirections[0].rtype, RedirectType::Input);
            assert_eq!(sc.redirections[1].rtype, RedirectType::Output);
        } else {
            panic!("expected simple command");
        }
    }

    #[test]
    fn test_assignment() {
        let prog = parse(tokenize(b"FOO=bar cmd")).unwrap();
        if let Command::Simple(ref sc) = prog.commands[0].first.commands[0] {
            assert_eq!(sc.assignments.len(), 1);
            assert_eq!(sc.assignments[0].name, b"FOO");
            assert_eq!(sc.assignments[0].value, b"bar");
        } else {
            panic!("expected simple command");
        }
    }

    #[test]
    fn test_pipeline_negation() {
        let prog = parse(tokenize(b"! grep error log")).unwrap();
        assert!(prog.commands[0].first.negated);
    }

    #[test]
    fn test_case_simple() {
        let prog = parse(tokenize(b"case x in a) echo a;; b) echo b;; esac")).unwrap();
        if let Command::Case(ref cc) = prog.commands[0].first.commands[0] {
            assert_eq!(cc.word, b"x");
            assert_eq!(cc.arms.len(), 2);
            assert_eq!(cc.arms[0].patterns[0], b"a");
        } else {
            panic!("expected case command");
        }
    }

    #[test]
    fn test_function_keyword() {
        let prog = parse(tokenize(b"function greet { echo hello; }")).unwrap();
        if let Command::FunctionDef { ref name, .. } = prog.commands[0].first.commands[0] {
            assert_eq!(name, b"greet");
        } else {
            panic!("expected function def");
        }
    }

    #[test]
    fn test_function_parens() {
        let prog = parse(tokenize(b"greet() { echo hello; }")).unwrap();
        if let Command::FunctionDef { ref name, .. } = prog.commands[0].first.commands[0] {
            assert_eq!(name, b"greet");
        } else {
            panic!("expected function def");
        }
    }

    // — FuzzStatic: comprehensive parser tests for all new features

    #[test]
    fn test_case_multiple_patterns() {
        // case x in a|b|c) echo match;; esac
        let prog = parse(tokenize(b"case x in a|b|c) echo match;; esac")).unwrap();
        if let Command::Case(ref cc) = prog.commands[0].first.commands[0] {
            assert_eq!(cc.arms.len(), 1);
            assert_eq!(cc.arms[0].patterns.len(), 3);
            assert_eq!(cc.arms[0].patterns[0], b"a");
            assert_eq!(cc.arms[0].patterns[1], b"b");
            assert_eq!(cc.arms[0].patterns[2], b"c");
        } else {
            panic!("expected case command");
        }
    }

    #[test]
    fn test_case_wildcard_default() {
        let prog = parse(tokenize(b"case x in foo) echo foo;; *) echo default;; esac")).unwrap();
        if let Command::Case(ref cc) = prog.commands[0].first.commands[0] {
            assert_eq!(cc.arms.len(), 2);
            assert_eq!(cc.arms[1].patterns[0], b"*");
        } else {
            panic!("expected case command");
        }
    }

    #[test]
    fn test_case_no_trailing_doublesemi() {
        // Last arm before esac doesn't need ;;
        let prog = parse(tokenize(b"case x in a) echo a esac")).unwrap();
        if let Command::Case(ref cc) = prog.commands[0].first.commands[0] {
            assert_eq!(cc.arms.len(), 1);
        } else {
            panic!("expected case command");
        }
    }

    #[test]
    fn test_pipeline_negation_false() {
        let prog = parse(tokenize(b"grep pattern file")).unwrap();
        assert!(!prog.commands[0].first.negated);
    }

    #[test]
    fn test_function_with_parens_after_keyword() {
        // function name() { body; }
        let prog = parse(tokenize(b"function greet() { echo hi; }")).unwrap();
        if let Command::FunctionDef { ref name, .. } = prog.commands[0].first.commands[0] {
            assert_eq!(name, b"greet");
        } else {
            panic!("expected function def");
        }
    }

    #[test]
    fn test_function_body_multiple_commands() {
        let prog = parse(tokenize(b"foo() { echo a; echo b; echo c; }")).unwrap();
        if let Command::FunctionDef { ref body, .. } = prog.commands[0].first.commands[0] {
            assert_eq!(body.commands.len(), 3);
        } else {
            panic!("expected function def");
        }
    }

    #[test]
    fn test_brace_group() {
        let prog = parse(tokenize(b"{ echo a; echo b; }")).unwrap();
        if let Command::Group(ref body) = prog.commands[0].first.commands[0] {
            assert_eq!(body.commands.len(), 2);
        } else {
            panic!("expected brace group");
        }
    }

    #[test]
    fn test_subshell() {
        let prog = parse(tokenize(b"(echo sub)")).unwrap();
        if let Command::Subshell(ref body) = prog.commands[0].first.commands[0] {
            assert_eq!(body.commands.len(), 1);
        } else {
            panic!("expected subshell");
        }
    }

    #[test]
    fn test_until_loop() {
        let prog = parse(tokenize(b"until false; do echo x; done")).unwrap();
        if let Command::Until(_) = prog.commands[0].first.commands[0] {
            // ok
        } else {
            panic!("expected until command");
        }
    }

    #[test]
    fn test_extended_test_unary() {
        let prog = parse(tokenize(b"[[ -f /tmp ]]")).unwrap();
        if let Command::ExtendedTest(ref expr) = prog.commands[0].first.commands[0] {
            if let TestExpr::Unary(op, operand) = expr {
                assert_eq!(op, b"-f");
                assert_eq!(operand, b"/tmp");
            } else {
                panic!("expected unary test expr");
            }
        } else {
            panic!("expected extended test");
        }
    }

    #[test]
    fn test_extended_test_binary() {
        let prog = parse(tokenize(b"[[ foo == bar ]]")).unwrap();
        if let Command::ExtendedTest(ref expr) = prog.commands[0].first.commands[0] {
            if let TestExpr::Binary(left, op, right) = expr {
                assert_eq!(left, b"foo");
                assert_eq!(op, b"==");
                assert_eq!(right, b"bar");
            } else {
                panic!("expected binary test expr");
            }
        } else {
            panic!("expected extended test");
        }
    }

    #[test]
    fn test_extended_test_not() {
        let prog = parse(tokenize(b"[[ ! -f /tmp ]]")).unwrap();
        if let Command::ExtendedTest(ref expr) = prog.commands[0].first.commands[0] {
            if let TestExpr::Not(_) = expr {
                // ok
            } else {
                panic!("expected Not test expr");
            }
        } else {
            panic!("expected extended test");
        }
    }

    #[test]
    fn test_extended_test_and() {
        let prog = parse(tokenize(b"[[ -f /bin/sh && -f /bin/esh ]]")).unwrap();
        if let Command::ExtendedTest(ref expr) = prog.commands[0].first.commands[0] {
            if let TestExpr::And(_, _) = expr {
                // ok
            } else {
                panic!("expected And test expr");
            }
        } else {
            panic!("expected extended test");
        }
    }

    #[test]
    fn test_extended_test_or() {
        let prog = parse(tokenize(b"[[ -f /a || -f /b ]]")).unwrap();
        if let Command::ExtendedTest(ref expr) = prog.commands[0].first.commands[0] {
            if let TestExpr::Or(_, _) = expr {
                // ok
            } else {
                panic!("expected Or test expr");
            }
        } else {
            panic!("expected extended test");
        }
    }

    #[test]
    fn test_nested_if_for() {
        let prog = parse(tokenize(b"if true; then for x in 1 2; do echo $x; done; fi")).unwrap();
        if let Command::If(ref ic) = prog.commands[0].first.commands[0] {
            assert_eq!(ic.branches.len(), 1);
            if let Command::For(ref fc) = ic.branches[0].1.commands[0].first.commands[0] {
                assert_eq!(fc.var_name, b"x");
            } else {
                panic!("expected for inside if");
            }
        } else {
            panic!("expected if command");
        }
    }

    #[test]
    fn test_heredoc_parse() {
        let prog = parse(tokenize(b"cat <<EOF\nhello\nEOF\n")).unwrap();
        if let Command::Simple(ref sc) = prog.commands[0].first.commands[0] {
            assert_eq!(sc.words[0], b"cat");
            assert_eq!(sc.redirections.len(), 1);
            assert_eq!(sc.redirections[0].rtype, RedirectType::HereDoc);
        } else {
            panic!("expected simple command with heredoc");
        }
    }

    #[test]
    fn test_here_string_parse() {
        let prog = parse(tokenize(b"cat <<< hello")).unwrap();
        if let Command::Simple(ref sc) = prog.commands[0].first.commands[0] {
            assert_eq!(sc.words[0], b"cat");
            assert_eq!(sc.redirections.len(), 1);
            assert_eq!(sc.redirections[0].rtype, RedirectType::HereString);
            assert_eq!(sc.redirections[0].target, b"hello");
        } else {
            panic!("expected simple command with herestring");
        }
    }

    #[test]
    fn test_elif_chain() {
        let prog = parse(tokenize(b"if false; then echo 1; elif true; then echo 2; elif false; then echo 3; fi")).unwrap();
        if let Command::If(ref ic) = prog.commands[0].first.commands[0] {
            assert_eq!(ic.branches.len(), 3);
            assert!(ic.else_body.is_none());
        } else {
            panic!("expected if command");
        }
    }

    #[test]
    fn test_for_no_in() {
        // for x; do ... done — defaults to positional params
        let prog = parse(tokenize(b"for x; do echo $x; done")).unwrap();
        if let Command::For(ref fc) = prog.commands[0].first.commands[0] {
            assert_eq!(fc.var_name, b"x");
            assert!(fc.words.is_empty());
        } else {
            panic!("expected for command");
        }
    }

    // =========================================================================
    // — FuzzStatic: P0-P10 feature parsing coverage
    // =========================================================================

    #[test]
    fn test_select_command() {
        // — FuzzStatic: select x in a b c; do echo $x; break; done
        let prog = parse(tokenize(b"select x in a b c; do echo $x; break; done")).unwrap();
        if let Command::Select(ref sc) = prog.commands[0].first.commands[0] {
            assert_eq!(sc.var_name, b"x");
            assert_eq!(sc.words.len(), 3);
            assert_eq!(sc.words[0], b"a");
            assert_eq!(sc.words[1], b"b");
            assert_eq!(sc.words[2], b"c");
        } else {
            panic!("expected select command, got {:?}", prog.commands[0].first.commands[0]);
        }
    }

    #[test]
    fn test_function_definition_parens() {
        // — FuzzStatic: greet() { echo hello; } must parse as function def
        let prog = parse(tokenize(b"greet() { echo hello; }")).unwrap();
        if let Command::FunctionDef { ref name, .. } = prog.commands[0].first.commands[0] {
            assert_eq!(name, b"greet");
        } else {
            panic!("expected function definition, got {:?}", prog.commands[0].first.commands[0]);
        }
    }

    #[test]
    fn test_function_keyword_syntax() {
        // — FuzzStatic: function greet { echo hello; } alternate syntax
        let prog = parse(tokenize(b"function greet { echo hello; }")).unwrap();
        if let Command::FunctionDef { ref name, .. } = prog.commands[0].first.commands[0] {
            assert_eq!(name, b"greet");
        } else {
            panic!("expected function definition");
        }
    }

    #[test]
    fn test_while_loop_variant() {
        // — FuzzStatic: while true produces While variant
        let prog = parse(tokenize(b"while true; do echo loop; done")).unwrap();
        if let Command::While(_) = prog.commands[0].first.commands[0] {
        } else {
            panic!("expected while command");
        }
    }

    #[test]
    fn test_until_loop_variant() {
        // — FuzzStatic: until false produces Until variant
        let prog = parse(tokenize(b"until false; do echo loop; done")).unwrap();
        if let Command::Until(_) = prog.commands[0].first.commands[0] {
        } else {
            panic!("expected until command");
        }
    }

    #[test]
    fn test_background_job() {
        // — FuzzStatic: sleep 5 & must parse as background pipeline
        let prog = parse(tokenize(b"sleep 5 &")).unwrap();
        assert!(prog.commands[0].background);
    }

    #[test]
    fn test_pipeline_with_semicolons() {
        // — FuzzStatic: multiple commands separated by ;
        let prog = parse(tokenize(b"echo a; echo b; echo c")).unwrap();
        assert_eq!(prog.commands.len(), 3);
    }

    #[test]
    fn test_case_esac() {
        let prog = parse(tokenize(b"case $x in\na) echo a;;\nb) echo b;;\nesac")).unwrap();
        if let Command::Case(ref cc) = prog.commands[0].first.commands[0] {
            assert_eq!(cc.arms.len(), 2);
        } else {
            panic!("expected case command");
        }
    }

    #[test]
    fn test_function_call_with_args() {
        // — FuzzStatic: greet() { echo $1; }; greet World — must parse as two commands
        let prog = parse(tokenize(b"greet() { echo $1; }; greet World")).unwrap();
        assert_eq!(prog.commands.len(), 2);
        // First: function definition
        if let Command::FunctionDef { ref name, .. } = prog.commands[0].first.commands[0] {
            assert_eq!(name, b"greet");
        } else {
            panic!("expected function def first");
        }
        // Second: simple command (the call)
        if let Command::Simple(ref sc) = prog.commands[1].first.commands[0] {
            assert_eq!(sc.words[0], b"greet");
            assert_eq!(sc.words[1], b"World");
        } else {
            panic!("expected simple command call");
        }
    }

    #[test]
    fn test_assignment_before_command() {
        // — FuzzStatic: FOO=bar echo $FOO — assignment + command on same line
        let prog = parse(tokenize(b"FOO=bar echo hello")).unwrap();
        if let Command::Simple(ref sc) = prog.commands[0].first.commands[0] {
            assert_eq!(sc.assignments.len(), 1);
            assert_eq!(sc.words[0], b"echo");
        } else {
            panic!("expected simple command with assignment");
        }
    }
}
