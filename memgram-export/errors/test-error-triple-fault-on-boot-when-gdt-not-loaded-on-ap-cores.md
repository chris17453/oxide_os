# Error: Test error: triple fault on boot when GDT not loaded on AP cores

| Field | Value |
|-------|-------|
| ID | `7c0547b69b82` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-02T19:26:46.973442+00:00 |
| Keywords | smp, gdt, triple-fault, boot |
| Session | [7b09dd917a15](../sessions/test-memgram-mcp-server-functionality.md) |

## Error

Test error: triple fault on boot when GDT not loaded on AP cores

## Cause

AP cores were jumping to kernel code before GDT was initialized

## Fix

Ensure each AP loads its own GDT in the trampoline code before entering long mode
