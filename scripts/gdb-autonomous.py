#!/usr/bin/env python3
"""
Autonomous GDB controller for OXIDE OS debugging.

— ColdCipher: Script-driven debugging because clicking through GDB
at 3 AM is how you end up debugging the debugger.

This script provides programmatic control over GDB for autonomous debugging:
- Execute GDB commands and capture output
- Set breakpoints, examine state, single-step
- Parse crashes and panics automatically
- Generate debug reports

Usage:
    # Execute a GDB command script
    ./scripts/gdb-autonomous.py --script debug-commands.gdb

    # Interactive REPL mode (still programmatic)
    ./scripts/gdb-autonomous.py --repl

    # Execute single command and exit
    ./scripts/gdb-autonomous.py --exec "bt"
"""

import subprocess
import sys
import os
import re
import time
from pathlib import Path

class GDBController:
    """Autonomous GDB controller for kernel debugging."""

    def __init__(self, kernel_binary=None, gdb_port=1234, gdb_binary="gdb"):
        self.kernel_binary = kernel_binary or self._find_kernel_binary()
        self.gdb_port = gdb_port
        self.gdb_binary = gdb_binary
        self.process = None

    def _find_kernel_binary(self):
        """Find the kernel binary in target directory."""
        repo_root = Path(__file__).parent.parent
        kernel_path = repo_root / "target/x86_64-unknown-none/debug/kernel"
        if not kernel_path.exists():
            raise FileNotFoundError(f"Kernel binary not found at {kernel_path}")
        return str(kernel_path)

    def connect(self, timeout=5):
        """Connect to QEMU GDB server."""
        print(f"[*] Connecting to GDB server at localhost:{self.gdb_port}")

        # Build GDB command list
        gdb_args = [
            self.gdb_binary,
            "-q",  # Quiet mode
            "-batch",  # Batch mode (no interactive)
            "-ex", f"target remote localhost:{self.gdb_port}",
            self.kernel_binary
        ]

        # Try to connect with retries
        for attempt in range(timeout):
            try:
                result = subprocess.run(
                    gdb_args + ["-ex", "info registers", "-ex", "quit"],
                    capture_output=True,
                    text=True,
                    timeout=2
                )
                if result.returncode == 0 or "Remote" in result.stdout:
                    print("[+] Connected to GDB server")
                    return True
            except (subprocess.TimeoutExpired, subprocess.CalledProcessError):
                pass

            if attempt < timeout - 1:
                print(f"[*] Connection attempt {attempt + 1}/{timeout} failed, retrying...")
                time.sleep(1)

        raise ConnectionError(f"Failed to connect to GDB server after {timeout} attempts")

    def execute(self, commands, timeout=30):
        """
        Execute GDB commands and return output.

        Args:
            commands: List of GDB commands or single command string
            timeout: Command timeout in seconds

        Returns:
            dict with 'stdout', 'stderr', 'returncode'
        """
        if isinstance(commands, str):
            commands = [commands]

        # Build command line
        gdb_args = [
            self.gdb_binary,
            "-q",
            "-batch",
            "-ex", f"target remote localhost:{self.gdb_port}",
            self.kernel_binary
        ]

        # Add user commands
        for cmd in commands:
            gdb_args.extend(["-ex", cmd])

        # Always quit at the end
        gdb_args.extend(["-ex", "quit"])

        print(f"[*] Executing {len(commands)} GDB command(s)")

        try:
            result = subprocess.run(
                gdb_args,
                capture_output=True,
                text=True,
                timeout=timeout
            )

            return {
                'stdout': result.stdout,
                'stderr': result.stderr,
                'returncode': result.returncode
            }
        except subprocess.TimeoutExpired:
            return {
                'stdout': '',
                'stderr': f'Command timed out after {timeout}s',
                'returncode': -1
            }

    def execute_script(self, script_path):
        """Execute GDB command script file."""
        if not Path(script_path).exists():
            raise FileNotFoundError(f"GDB script not found: {script_path}")

        print(f"[*] Executing GDB script: {script_path}")

        gdb_args = [
            self.gdb_binary,
            "-q",
            "-batch",
            "-ex", f"target remote localhost:{self.gdb_port}",
            "-x", script_path,
            self.kernel_binary
        ]

        result = subprocess.run(gdb_args, capture_output=True, text=True)
        return {
            'stdout': result.stdout,
            'stderr': result.stderr,
            'returncode': result.returncode
        }

    def get_backtrace(self):
        """Get full backtrace."""
        return self.execute("bt")

    def get_registers(self):
        """Get register dump."""
        return self.execute("info registers")

    def get_all_threads(self):
        """Get info about all threads."""
        return self.execute(["info threads", "thread apply all bt"])

    def examine_panic(self):
        """
        Examine kernel panic state.

        Returns structured info about the panic including:
        - Panic message
        - Backtrace
        - Register state
        - Memory at fault address
        """
        commands = [
            "bt",
            "info registers",
            "x/32i $rip-32",  # Instructions around RIP
            "info threads",
            "thread apply all bt"
        ]

        result = self.execute(commands)

        # Parse output to extract useful info
        output = result['stdout']

        # Extract panic message if present
        panic_msg = None
        for line in output.split('\n'):
            if 'panic' in line.lower() or 'fault' in line.lower():
                panic_msg = line.strip()
                break

        return {
            'output': output,
            'panic_message': panic_msg,
            'backtrace': self._extract_backtrace(output),
            'registers': self._extract_registers(output)
        }

    def _extract_backtrace(self, output):
        """Extract backtrace from GDB output."""
        bt_lines = []
        in_bt = False
        for line in output.split('\n'):
            if line.startswith('#'):
                in_bt = True
                bt_lines.append(line)
            elif in_bt and not line.startswith('#') and line.strip():
                in_bt = False
        return bt_lines

    def _extract_registers(self, output):
        """Extract register values from GDB output."""
        regs = {}
        for line in output.split('\n'):
            match = re.match(r'(\w+)\s+0x([0-9a-f]+)', line)
            if match:
                regs[match.group(1)] = match.group(2)
        return regs

    def continue_execution(self):
        """Continue execution (non-blocking)."""
        # For continue, we don't want to wait for output
        gdb_args = [
            self.gdb_binary,
            "-q",
            "-batch",
            "-ex", f"target remote localhost:{self.gdb_port}",
            "-ex", "continue",
            self.kernel_binary
        ]

        # Start process but don't wait
        self.process = subprocess.Popen(
            gdb_args,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True
        )
        print("[*] Execution continued")
        return self.process

def main():
    import argparse

    parser = argparse.ArgumentParser(
        description="Autonomous GDB controller for OXIDE OS"
    )
    parser.add_argument(
        '--script', '-s',
        help='GDB command script to execute'
    )
    parser.add_argument(
        '--exec', '-e',
        help='Single GDB command to execute'
    )
    parser.add_argument(
        '--repl', '-r',
        action='store_true',
        help='Interactive REPL mode'
    )
    parser.add_argument(
        '--panic', '-p',
        action='store_true',
        help='Examine panic state'
    )
    parser.add_argument(
        '--backtrace', '-bt',
        action='store_true',
        help='Get backtrace'
    )
    parser.add_argument(
        '--registers', '-reg',
        action='store_true',
        help='Dump registers'
    )
    parser.add_argument(
        '--port',
        type=int,
        default=1234,
        help='GDB server port (default: 1234)'
    )
    parser.add_argument(
        '--kernel',
        help='Path to kernel binary'
    )

    args = parser.parse_args()

    # Create controller
    gdb = GDBController(kernel_binary=args.kernel, gdb_port=args.port)

    try:
        # Connect to GDB server
        gdb.connect()

        if args.script:
            result = gdb.execute_script(args.script)
            print(result['stdout'])
            if result['stderr']:
                print("STDERR:", result['stderr'], file=sys.stderr)
            sys.exit(result['returncode'])

        elif args.exec:
            result = gdb.execute(args.exec)
            print(result['stdout'])
            if result['stderr']:
                print("STDERR:", result['stderr'], file=sys.stderr)
            sys.exit(result['returncode'])

        elif args.panic:
            info = gdb.examine_panic()
            print("=== PANIC ANALYSIS ===")
            if info['panic_message']:
                print(f"Panic: {info['panic_message']}")
            print("\n=== BACKTRACE ===")
            for line in info['backtrace']:
                print(line)
            print("\n=== REGISTERS ===")
            for reg, val in info['registers'].items():
                print(f"{reg:8} = 0x{val}")
            sys.exit(0)

        elif args.backtrace:
            result = gdb.get_backtrace()
            print(result['stdout'])
            sys.exit(result['returncode'])

        elif args.registers:
            result = gdb.get_registers()
            print(result['stdout'])
            sys.exit(result['returncode'])

        elif args.repl:
            print("=== Autonomous GDB REPL ===")
            print("Enter GDB commands (or 'quit' to exit):")
            while True:
                try:
                    cmd = input("(gdb) ").strip()
                    if cmd in ('quit', 'exit', 'q'):
                        break
                    if not cmd:
                        continue

                    result = gdb.execute(cmd)
                    print(result['stdout'])
                    if result['stderr']:
                        print("ERROR:", result['stderr'], file=sys.stderr)

                except EOFError:
                    break
                except KeyboardInterrupt:
                    print("\nInterrupted")
                    break

        else:
            parser.print_help()
            print("\nNo action specified. Use --help for usage.")
            sys.exit(1)

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

if __name__ == '__main__':
    main()
