//! V86 mode monitor

use crate::{V86Action, V86Context, V86Error, EmulatedOp};

/// V86 monitor for handling GPFs and privileged operations
pub struct V86Monitor {
    /// Virtual interrupt flag (since real IF can't be changed in V86)
    virtual_if: bool,
    /// Instruction count
    instruction_count: u64,
    /// I/O callback
    io_handler: Option<fn(u16, bool, u8, u32) -> u32>,
}

impl V86Monitor {
    /// Create new monitor
    pub fn new() -> Self {
        V86Monitor {
            virtual_if: true,
            instruction_count: 0,
            io_handler: None,
        }
    }

    /// Set I/O handler
    pub fn set_io_handler(&mut self, handler: fn(u16, bool, u8, u32) -> u32) {
        self.io_handler = Some(handler);
    }

    /// Handle General Protection Fault from V86 mode
    pub fn handle_gpf(&mut self, ctx: &mut V86Context) -> Result<V86Action, V86Error> {
        let ip = ctx.current_ip();
        let opcode = ctx.memory.read_u8(ip)?;

        self.instruction_count += 1;

        match opcode {
            // CLI - Clear interrupt flag
            0xFA => {
                self.virtual_if = false;
                ctx.regs.eip += 1;
                Ok(V86Action::Emulate(EmulatedOp::Cli))
            }

            // STI - Set interrupt flag
            0xFB => {
                self.virtual_if = true;
                ctx.regs.eip += 1;
                Ok(V86Action::Emulate(EmulatedOp::Sti))
            }

            // PUSHF - Push flags
            0x9C => {
                let flags = ctx.regs.flags();
                let mut virtual_flags = flags;
                if self.virtual_if {
                    virtual_flags |= 0x200; // IF
                } else {
                    virtual_flags &= !0x200;
                }

                ctx.regs.set_sp(ctx.regs.sp().wrapping_sub(2));
                let ss_sp = ctx.linear_addr(ctx.segments.ss, ctx.regs.sp());
                ctx.memory.write_u16(ss_sp, virtual_flags)?;
                ctx.regs.eip += 1;
                Ok(V86Action::Emulate(EmulatedOp::Pushf))
            }

            // POPF - Pop flags
            0x9D => {
                let ss_sp = ctx.linear_addr(ctx.segments.ss, ctx.regs.sp());
                let flags = ctx.memory.read_u16(ss_sp)?;
                ctx.regs.set_sp(ctx.regs.sp().wrapping_add(2));

                // Update virtual IF
                self.virtual_if = flags & 0x200 != 0;

                // Update safe flags (not IOPL, VM)
                let safe_flags = flags & 0x0DD5;
                ctx.regs.eflags = (ctx.regs.eflags & !0x0DD5) | safe_flags as u32;
                ctx.regs.eip += 1;
                Ok(V86Action::Emulate(EmulatedOp::Popf))
            }

            // INT n
            0xCD => {
                let int_num = ctx.memory.read_u8(ip + 1)?;
                ctx.regs.eip += 2;
                Ok(V86Action::Emulate(EmulatedOp::Int(int_num)))
            }

            // IRET
            0xCF => {
                let ss_sp = ctx.linear_addr(ctx.segments.ss, ctx.regs.sp());
                let new_ip = ctx.memory.read_u16(ss_sp)?;
                let new_cs = ctx.memory.read_u16(ss_sp + 2)?;
                let new_flags = ctx.memory.read_u16(ss_sp + 4)?;

                ctx.regs.set_sp(ctx.regs.sp().wrapping_add(6));
                ctx.regs.set_ip(new_ip);
                ctx.segments.cs = new_cs;

                // Update virtual IF and safe flags
                self.virtual_if = new_flags & 0x200 != 0;
                let safe_flags = new_flags & 0x0DD5;
                ctx.regs.eflags = (ctx.regs.eflags & !0x0DD5) | safe_flags as u32;

                Ok(V86Action::Emulate(EmulatedOp::Iret))
            }

            // IN AL, imm8
            0xE4 => {
                let port = ctx.memory.read_u8(ip + 1)? as u16;
                if ctx.is_io_allowed(port) {
                    if let Some(handler) = self.io_handler {
                        let val = handler(port, true, 1, 0);
                        ctx.regs.set_al(val as u8);
                    }
                    ctx.regs.eip += 2;
                    Ok(V86Action::Emulate(EmulatedOp::In { port, size: 1 }))
                } else {
                    Err(V86Error::IoNotAllowed)
                }
            }

            // IN AX, imm8
            0xE5 => {
                let port = ctx.memory.read_u8(ip + 1)? as u16;
                if ctx.is_io_allowed(port) && ctx.is_io_allowed(port + 1) {
                    if let Some(handler) = self.io_handler {
                        let val = handler(port, true, 2, 0);
                        ctx.regs.set_ax(val as u16);
                    }
                    ctx.regs.eip += 2;
                    Ok(V86Action::Emulate(EmulatedOp::In { port, size: 2 }))
                } else {
                    Err(V86Error::IoNotAllowed)
                }
            }

            // OUT imm8, AL
            0xE6 => {
                let port = ctx.memory.read_u8(ip + 1)? as u16;
                if ctx.is_io_allowed(port) {
                    if let Some(handler) = self.io_handler {
                        handler(port, false, 1, ctx.regs.al() as u32);
                    }
                    ctx.regs.eip += 2;
                    Ok(V86Action::Emulate(EmulatedOp::Out {
                        port,
                        size: 1,
                        value: ctx.regs.al() as u32
                    }))
                } else {
                    Err(V86Error::IoNotAllowed)
                }
            }

            // OUT imm8, AX
            0xE7 => {
                let port = ctx.memory.read_u8(ip + 1)? as u16;
                if ctx.is_io_allowed(port) && ctx.is_io_allowed(port + 1) {
                    if let Some(handler) = self.io_handler {
                        handler(port, false, 2, ctx.regs.ax() as u32);
                    }
                    ctx.regs.eip += 2;
                    Ok(V86Action::Emulate(EmulatedOp::Out {
                        port,
                        size: 2,
                        value: ctx.regs.ax() as u32
                    }))
                } else {
                    Err(V86Error::IoNotAllowed)
                }
            }

            // IN AL, DX
            0xEC => {
                let port = ctx.regs.dx();
                if ctx.is_io_allowed(port) {
                    if let Some(handler) = self.io_handler {
                        let val = handler(port, true, 1, 0);
                        ctx.regs.set_al(val as u8);
                    }
                    ctx.regs.eip += 1;
                    Ok(V86Action::Emulate(EmulatedOp::In { port, size: 1 }))
                } else {
                    Err(V86Error::IoNotAllowed)
                }
            }

            // IN AX, DX
            0xED => {
                let port = ctx.regs.dx();
                if ctx.is_io_allowed(port) && ctx.is_io_allowed(port + 1) {
                    if let Some(handler) = self.io_handler {
                        let val = handler(port, true, 2, 0);
                        ctx.regs.set_ax(val as u16);
                    }
                    ctx.regs.eip += 1;
                    Ok(V86Action::Emulate(EmulatedOp::In { port, size: 2 }))
                } else {
                    Err(V86Error::IoNotAllowed)
                }
            }

            // OUT DX, AL
            0xEE => {
                let port = ctx.regs.dx();
                if ctx.is_io_allowed(port) {
                    if let Some(handler) = self.io_handler {
                        handler(port, false, 1, ctx.regs.al() as u32);
                    }
                    ctx.regs.eip += 1;
                    Ok(V86Action::Emulate(EmulatedOp::Out {
                        port,
                        size: 1,
                        value: ctx.regs.al() as u32
                    }))
                } else {
                    Err(V86Error::IoNotAllowed)
                }
            }

            // OUT DX, AX
            0xEF => {
                let port = ctx.regs.dx();
                if ctx.is_io_allowed(port) && ctx.is_io_allowed(port + 1) {
                    if let Some(handler) = self.io_handler {
                        handler(port, false, 2, ctx.regs.ax() as u32);
                    }
                    ctx.regs.eip += 1;
                    Ok(V86Action::Emulate(EmulatedOp::Out {
                        port,
                        size: 2,
                        value: ctx.regs.ax() as u32
                    }))
                } else {
                    Err(V86Error::IoNotAllowed)
                }
            }

            // HLT
            0xF4 => {
                ctx.regs.eip += 1;
                Ok(V86Action::Emulate(EmulatedOp::Hlt))
            }

            _ => Err(V86Error::InvalidOpcode),
        }
    }

    /// Get instruction count
    pub fn instruction_count(&self) -> u64 {
        self.instruction_count
    }

    /// Get virtual IF state
    pub fn virtual_if(&self) -> bool {
        self.virtual_if
    }

    /// Set virtual IF state
    pub fn set_virtual_if(&mut self, val: bool) {
        self.virtual_if = val;
    }
}

impl Default for V86Monitor {
    fn default() -> Self {
        Self::new()
    }
}
