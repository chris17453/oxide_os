# Error: QEMU MCP monitorCommand() race condition — commands silently never sent

| Field | Value |
|-------|-------|
| ID | `cbe1a2928a05` |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-03T21:07:48.488319+00:00 |
| Keywords | QEMU, MCP, monitorCommand, sendkeys, race-condition, socket |
| Session | [010a3158f67b](../sessions/clean-up-diagnostic-traces-record-pml4-fix-and-mcp-monitorcommand-fix-in-memgram.md) |

## Error

QEMU MCP monitorCommand() race condition — commands silently never sent

## Cause

monitorCommand() connects to QEMU monitor socket, receives initial "(qemu)" banner, and the data handler resolves+closes the socket immediately on seeing "(qemu)". The 100ms setTimeout that was supposed to send the actual command fires AFTER the socket is already closed. Net result: every sendkeys/sendtext/command call via MCP silently does nothing.

## Fix

Rewrote monitorCommand to track commandSent boolean and prompt count. First "(qemu)" prompt → send the command. Second "(qemu)" prompt → command response complete, close and resolve. 2-second timeout fallback. File: tools/qemu-mcp/index.js
