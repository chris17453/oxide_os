//! EFFLUX OS GW-BASIC Entry Point
//!
//! This is the main entry point for the GW-BASIC interpreter when running
//! as a native application on EFFLUX OS.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use rust_gwbasic::{Lexer, Parser, Interpreter};
use rust_gwbasic::platform::{EffluxConsole, Console};

/// Main function called by libc's _start
#[no_mangle]
pub extern "Rust" fn main() -> i32 {
    let mut console = EffluxConsole::new();

    console.print("GW-BASIC (Rust) v");
    console.print(rust_gwbasic::VERSION);
    console.print(" for EFFLUX OS\n");
    console.print("Type BASIC statements or 'EXIT' to quit\n\n");

    let mut interpreter = Interpreter::new();

    loop {
        console.print("> ");

        let input = console.read_line();
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input.eq_ignore_ascii_case("EXIT") || input.eq_ignore_ascii_case("QUIT") {
            break;
        }

        // Try to tokenize, parse, and execute
        let mut lexer = Lexer::new(input);
        let tokens = match lexer.tokenize() {
            Ok(t) => t,
            Err(e) => {
                console.print("Lexer error: ");
                console.print(&alloc::format!("{:?}\n", e));
                continue;
            }
        };

        let mut parser = Parser::new(tokens);
        let ast = match parser.parse() {
            Ok(a) => a,
            Err(e) => {
                console.print("Parser error: ");
                console.print(&alloc::format!("{:?}\n", e));
                continue;
            }
        };

        if let Err(e) = interpreter.execute(ast) {
            console.print("Runtime error: ");
            console.print(&alloc::format!("{:?}\n", e));
        }
    }

    console.print("Goodbye!\n");
    0
}

// Note: Panic handler is provided by libc crate

/// Global allocator - uses libc mmap for heap
#[global_allocator]
static ALLOCATOR: EffluxAllocator = EffluxAllocator;

struct EffluxAllocator;

// Static heap for the allocator
static mut HEAP: [u8; 2 * 1024 * 1024] = [0; 2 * 1024 * 1024]; // 2MB heap
static mut HEAP_POS: usize = 0;

unsafe impl alloc::alloc::GlobalAlloc for EffluxAllocator {
    unsafe fn alloc(&self, layout: alloc::alloc::Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();

        // Align the position
        let aligned_pos = (HEAP_POS + align - 1) & !(align - 1);

        if aligned_pos + size > HEAP.len() {
            return core::ptr::null_mut();
        }

        HEAP_POS = aligned_pos + size;
        HEAP.as_mut_ptr().add(aligned_pos)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: alloc::alloc::Layout) {
        // Simple bump allocator doesn't deallocate
    }
}
