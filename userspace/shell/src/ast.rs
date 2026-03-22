//! Shell AST — Abstract Syntax Tree node definitions
//!
//! — ByteRiot: the blueprint of intent. Every shell command, pipeline, conditional,
//! and loop is represented as a tree of these nodes. The parser builds the tree,
//! the evaluator walks it.

extern crate alloc;
use alloc::vec::Vec;
use alloc::boxed::Box;

/// A complete shell program — a list of complete commands separated by ; or newline
#[derive(Debug, Clone)]
pub struct Program {
    pub commands: Vec<CompoundList>,
}

/// A compound list — commands connected by && or ||
/// — ByteRiot: left-to-right evaluation with short-circuit semantics.
/// `cmd1 && cmd2 || cmd3` is [(cmd1, None), (cmd2, And), (cmd3, Or)]
#[derive(Debug, Clone)]
pub struct CompoundList {
    pub first: Pipeline,
    pub rest: Vec<(ListOp, Pipeline)>,
    pub background: bool,
}

/// Operators between pipelines in a compound list
#[derive(Debug, Clone, PartialEq)]
pub enum ListOp {
    And,  // &&
    Or,   // ||
}

/// A pipeline — one or more simple commands connected by |
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub commands: Vec<Command>,
    pub negated: bool, // ! prefix
}

/// A single command — can be simple, compound, or a function definition
#[derive(Debug, Clone)]
pub enum Command {
    /// Simple command: words + redirections + prefix assignments
    Simple(SimpleCommand),
    /// If/elif/else/fi
    If(IfCommand),
    /// For loop
    For(ForCommand),
    /// While loop
    While(WhileCommand),
    /// Until loop (like while but inverted condition)
    Until(WhileCommand),
    /// Subshell: ( list )
    Subshell(Program),
    /// Brace group: { list; }
    Group(Program),
    /// Function definition: name() { body; } or function name { body; }
    /// — ByteRiot: first-class named code blocks. Shell's version of a function pointer,
    /// except it's stored as raw AST and re-evaluated every call. Live code injection.
    FunctionDef { name: Vec<u8>, body: Box<Program> },
    /// Case command: case word in pattern) body;; esac
    /// — ByteRiot: glorified goto table with glob-powered dispatch keys
    Case(CaseCommand),
    /// Extended test: [[ expr ]]
    /// — ByteRiot: the shell finally admits it needs real boolean logic
    ExtendedTest(TestExpr),
    /// Select command: select name in words; do body; done
    /// — IronGhost: interactive menu-driven loops. The only shell construct
    /// that actually talks to the user. Used in installers and config scripts.
    Select(SelectCommand),
}

/// A simple command: name + args + redirections
#[derive(Debug, Clone)]
pub struct SimpleCommand {
    /// Prefix variable assignments (VAR=val before command)
    pub assignments: Vec<Assignment>,
    /// Command words (first is the command name)
    pub words: Vec<Vec<u8>>,
    /// I/O redirections
    pub redirections: Vec<Redirect>,
}

/// Variable assignment: NAME=VALUE
#[derive(Debug, Clone)]
pub struct Assignment {
    pub name: Vec<u8>,
    pub value: Vec<u8>,
}

/// I/O redirection
#[derive(Debug, Clone)]
pub struct Redirect {
    /// Source file descriptor (0=stdin, 1=stdout, 2=stderr, -1=default)
    pub fd: i32,
    /// Redirection type
    pub rtype: RedirectType,
    /// Target (filename or fd number for dup)
    pub target: Vec<u8>,
}

/// Types of I/O redirection
#[derive(Debug, Clone, PartialEq)]
pub enum RedirectType {
    /// < file
    Input,
    /// > file
    Output,
    /// >> file
    Append,
    /// n>&m (duplicate fd)
    DupOut,
    /// n<&m (duplicate fd)
    DupIn,
    /// << word (here document)
    /// — ByteRiot: inline stdin. Embed a whole document right in the script.
    HereDoc,
    /// <<- word (here document, strip leading tabs)
    /// — ByteRiot: same trick but tab-stripped, for the aesthetically inclined
    HereDocStrip,
    /// <<< word (here string)
    /// — ByteRiot: one-liner heredoc. Feed a string straight into stdin.
    HereString,
}

/// If command: if cond; then body; [elif cond; then body;]* [else body;] fi
#[derive(Debug, Clone)]
pub struct IfCommand {
    /// (condition, body) pairs — first is the if, rest are elif
    pub branches: Vec<(CompoundList, Program)>,
    /// Optional else body
    pub else_body: Option<Program>,
}

/// For command: for name in words; do body; done
#[derive(Debug, Clone)]
pub struct ForCommand {
    /// Loop variable name
    pub var_name: Vec<u8>,
    /// Words to iterate over
    pub words: Vec<Vec<u8>>,
    /// Loop body
    pub body: Program,
}

/// While/Until command: while cond; do body; done
#[derive(Debug, Clone)]
pub struct WhileCommand {
    /// Loop condition
    pub condition: CompoundList,
    /// Loop body
    pub body: Program,
}

/// Select command: select name in words; do body; done
/// — IronGhost: same shape as ForCommand, different evaluation semantics.
/// Prints numbered menu, reads choice, sets variable, loops.
#[derive(Debug, Clone)]
pub struct SelectCommand {
    /// Variable to set with the selected word
    pub var_name: Vec<u8>,
    /// Words to present as menu choices
    pub words: Vec<Vec<u8>>,
    /// Loop body
    pub body: Program,
}

/// Case command: case word in (pat1|pat2) body;; esac
/// — ByteRiot: pattern-dispatch. The shell's switch statement,
/// except patterns are globs, not constants. Beautiful and terrifying.
#[derive(Debug, Clone)]
pub struct CaseCommand {
    /// The word being matched
    pub word: Vec<u8>,
    /// List of pattern→body arms
    pub arms: Vec<CaseArm>,
}

/// A single case arm: pattern list → body, terminated by ;;
#[derive(Debug, Clone)]
pub struct CaseArm {
    /// Patterns (separated by | in syntax)
    pub patterns: Vec<Vec<u8>>,
    /// Body to execute if any pattern matches
    pub body: Program,
}

/// Expression inside [[ ]] extended test
/// — ByteRiot: boolean algebra meets shell syntax. Recursive
/// structure handles arbitrary nesting of and/or/not.
#[derive(Debug, Clone)]
pub enum TestExpr {
    /// Unary test: -f file, -z string, etc.
    Unary(Vec<u8>, Vec<u8>),
    /// Binary test: str == str, num -eq num, str =~ pat
    Binary(Vec<u8>, Vec<u8>, Vec<u8>),
    /// Logical NOT
    Not(Box<TestExpr>),
    /// Logical AND
    And(Box<TestExpr>, Box<TestExpr>),
    /// Logical OR
    Or(Box<TestExpr>, Box<TestExpr>),
    /// Literal string (non-empty = true, empty = false)
    Literal(Vec<u8>),
}
