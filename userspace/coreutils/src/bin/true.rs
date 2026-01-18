//! true - do nothing, successfully

#![no_std]
#![no_main]

#[unsafe(no_mangle)]
fn main() -> i32 {
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
