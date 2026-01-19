//! V86 interrupt handlers

use crate::{V86Context, V86Error};

/// Handle software interrupt
pub fn handle_interrupt(ctx: &mut V86Context, int_num: u8) -> Result<bool, V86Error> {
    match int_num {
        0x10 => handle_int10(ctx),
        0x13 => handle_int13(ctx),
        0x16 => handle_int16(ctx),
        0x21 => return Ok(true), // DOS services handled by dos.rs
        0x33 => handle_int33(ctx),
        _ => {
            // Reflect to V86 handler if redirected
            if ctx.is_int_redirected(int_num) {
                reflect_interrupt(ctx, int_num)?;
            }
            Ok(false)
        }
    }
}

/// Reflect interrupt to V86 handler
fn reflect_interrupt(ctx: &mut V86Context, int_num: u8) -> Result<(), V86Error> {
    // Push flags, CS, IP onto stack
    ctx.regs.set_sp(ctx.regs.sp().wrapping_sub(6));
    let ss_sp = ctx.linear_addr(ctx.segments.ss, ctx.regs.sp());

    ctx.memory.write_u16(ss_sp, ctx.regs.ip())?;
    ctx.memory.write_u16(ss_sp + 2, ctx.segments.cs)?;
    ctx.memory.write_u16(ss_sp + 4, ctx.regs.flags())?;

    // Get interrupt vector
    let (seg, off) = ctx.memory.get_int_vector(int_num)?;
    ctx.segments.cs = seg;
    ctx.regs.set_ip(off);

    Ok(())
}

/// INT 10h - Video BIOS services
fn handle_int10(ctx: &mut V86Context) -> Result<bool, V86Error> {
    let ah = ctx.regs.ah();

    match ah {
        // Set video mode
        0x00 => {
            let _mode = ctx.regs.al();
            // Would configure video mode here
            Ok(false)
        }

        // Set cursor position
        0x02 => {
            let _page = ctx.regs.bx() as u8;
            let _row = ctx.regs.dh();
            let _col = ctx.regs.dl();
            // Would set cursor position
            Ok(false)
        }

        // Get cursor position
        0x03 => {
            ctx.regs.set_dh(0); // row
            ctx.regs.set_dl(0); // col
            ctx.regs.ecx = 0x0607; // cursor shape
            Ok(false)
        }

        // Write character and attribute at cursor
        0x09 => {
            let _char = ctx.regs.al();
            let _attr = ctx.regs.bx() as u8;
            let _count = ctx.regs.cx();
            // Would write characters
            Ok(false)
        }

        // Write character at cursor
        0x0A => {
            let _char = ctx.regs.al();
            let _count = ctx.regs.cx();
            // Would write characters
            Ok(false)
        }

        // Teletype output
        0x0E => {
            let _char = ctx.regs.al();
            // Would output character
            Ok(false)
        }

        // Get video mode
        0x0F => {
            ctx.regs.set_ah(80); // columns
            ctx.regs.set_al(0x03); // mode
            ctx.regs.ebx = (ctx.regs.ebx & 0xFF00) | 0; // page
            Ok(false)
        }

        _ => Ok(false),
    }
}

/// INT 13h - Disk BIOS services
fn handle_int13(ctx: &mut V86Context) -> Result<bool, V86Error> {
    let ah = ctx.regs.ah();

    match ah {
        // Reset disk system
        0x00 => {
            ctx.regs.set_ah(0);
            ctx.regs.set_carry(false);
            Ok(false)
        }

        // Get disk status
        0x01 => {
            ctx.regs.set_ah(0); // No error
            ctx.regs.set_carry(false);
            Ok(false)
        }

        // Read sectors
        0x02 => {
            // Would read from virtual disk
            ctx.regs.set_ah(0);
            ctx.regs.set_carry(false);
            Ok(false)
        }

        // Write sectors
        0x03 => {
            // Would write to virtual disk
            ctx.regs.set_ah(0);
            ctx.regs.set_carry(false);
            Ok(false)
        }

        // Get drive parameters
        0x08 => {
            // Return floppy parameters for drive 0
            if ctx.regs.dl() == 0 {
                ctx.regs.set_ah(0);
                ctx.regs.set_dh(1);  // max head number
                ctx.regs.ecx = (79 << 8) | 18; // max cylinder, sectors
                ctx.regs.set_carry(false);
            } else {
                ctx.regs.set_ah(0x01); // invalid
                ctx.regs.set_carry(true);
            }
            Ok(false)
        }

        // Check extensions present
        0x41 => {
            ctx.regs.set_carry(true); // Extensions not present
            Ok(false)
        }

        _ => {
            ctx.regs.set_carry(true);
            Ok(false)
        }
    }
}

/// INT 16h - Keyboard BIOS services
fn handle_int16(ctx: &mut V86Context) -> Result<bool, V86Error> {
    let ah = ctx.regs.ah();

    match ah {
        // Read character
        0x00 | 0x10 => {
            // Would block for keyboard input
            ctx.regs.set_al(0);
            ctx.regs.set_ah(0);
            Ok(false)
        }

        // Check for character
        0x01 | 0x11 => {
            // Check if key available
            ctx.regs.set_zero(true); // No key available
            Ok(false)
        }

        // Get shift flags
        0x02 | 0x12 => {
            ctx.regs.set_al(0); // No shift keys pressed
            Ok(false)
        }

        _ => Ok(false),
    }
}

/// INT 33h - Mouse services
fn handle_int33(ctx: &mut V86Context) -> Result<bool, V86Error> {
    let ax = ctx.regs.ax();

    match ax {
        // Reset mouse
        0x0000 => {
            ctx.regs.set_ax(0xFFFF); // Mouse installed
            ctx.regs.ebx = (ctx.regs.ebx & 0xFFFF0000) | 3; // 3 buttons
            Ok(false)
        }

        // Show cursor
        0x0001 => Ok(false),

        // Hide cursor
        0x0002 => Ok(false),

        // Get position and buttons
        0x0003 => {
            ctx.regs.ebx = 0; // buttons
            ctx.regs.ecx = 0; // x position
            ctx.regs.edx = 0; // y position
            Ok(false)
        }

        // Set position
        0x0004 => Ok(false),

        _ => Ok(false),
    }
}
