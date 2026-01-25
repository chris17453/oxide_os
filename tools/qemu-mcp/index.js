#!/usr/bin/env node

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { spawn, exec, execSync } from "child_process";
import { createWriteStream, readFileSync, existsSync, unlinkSync, writeFileSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import net from "net";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = join(__dirname, "../..");

// State
let qemuProcess = null;
let serialBuffer = [];
const MAX_SERIAL_LINES = 1000;
let monitorSocket = null;
let currentMode = null; // 'fedora' or 'rhel'

// Paths
const SERIAL_LOG = join(PROJECT_ROOT, "target/serial.log");
const SCREENSHOT_PATH = join(PROJECT_ROOT, "target/screenshot.ppm");
const MONITOR_SOCKET = join(PROJECT_ROOT, "target/qemu-monitor.sock");
const PID_FILE = join(PROJECT_ROOT, "target/qemu.pid");
const BOOT_IMAGE = join(PROJECT_ROOT, "target/boot.img");
const OVMF_VARS = join(PROJECT_ROOT, "target/OVMF_VARS.fd");

// Detect environment (Fedora vs RHEL)
function detectEnvironment() {
  // Check if qemu-system-x86_64 is available (Fedora)
  try {
    execSync("which qemu-system-x86_64", { encoding: "utf8", stdio: "pipe" });
    return "fedora";
  } catch {
    // Fall back to RHEL mode with qemu-kvm
    if (existsSync("/usr/libexec/qemu-kvm")) {
      return "rhel";
    }
    return "fedora"; // Default
  }
}

// Find OVMF firmware
function findOvmf(mode) {
  if (mode === "rhel") {
    // RHEL uses split OVMF files
    if (existsSync("/usr/share/edk2/ovmf/OVMF_CODE.fd")) {
      return {
        code: "/usr/share/edk2/ovmf/OVMF_CODE.fd",
        vars: "/usr/share/edk2/ovmf/OVMF_VARS.fd",
      };
    }
  }

  // Fedora/Ubuntu style - single file
  const paths = [
    "/usr/share/OVMF/OVMF_CODE.fd",
    "/usr/share/edk2-ovmf/x64/OVMF_CODE.fd",
    "/usr/share/edk2/ovmf/OVMF_CODE.fd",
    "/usr/share/qemu/OVMF.fd",
  ];
  for (const p of paths) {
    if (existsSync(p)) return { code: p, vars: null };
  }
  return null;
}

// Find QEMU binary
function findQemu(mode) {
  if (mode === "rhel") {
    return "/usr/libexec/qemu-kvm";
  }
  return "qemu-system-x86_64";
}

// Connect to QEMU monitor
async function connectMonitor() {
  return new Promise((resolve, reject) => {
    if (!existsSync(MONITOR_SOCKET)) {
      reject(new Error("QEMU monitor socket not found"));
      return;
    }
    const sock = net.createConnection(MONITOR_SOCKET);
    sock.on("connect", () => {
      monitorSocket = sock;
      resolve(sock);
    });
    sock.on("error", reject);
  });
}

// Send command to QEMU monitor
async function monitorCommand(cmd) {
  return new Promise(async (resolve, reject) => {
    try {
      const sock = await connectMonitor();
      let response = "";

      sock.on("data", (data) => {
        response += data.toString();
        // Wait for (qemu) prompt
        if (response.includes("(qemu)")) {
          sock.end();
          resolve(response);
        }
      });

      // Wait a bit for initial prompt, then send command
      setTimeout(() => {
        sock.write(cmd + "\n");
      }, 100);

      setTimeout(() => {
        sock.end();
        resolve(response);
      }, 2000);
    } catch (err) {
      reject(err);
    }
  });
}

// Build the kernel
async function build(target = "boot-quick") {
  return new Promise((resolve, reject) => {
    const make = spawn("make", [target], {
      cwd: PROJECT_ROOT,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";

    make.stdout.on("data", (data) => { stdout += data.toString(); });
    make.stderr.on("data", (data) => { stderr += data.toString(); });

    make.on("close", (code) => {
      if (code === 0) {
        resolve({ success: true, output: stdout });
      } else {
        reject(new Error(`Build failed (exit ${code}): ${stderr || stdout}`));
      }
    });
  });
}

// Start QEMU
async function startQemu(options = {}) {
  if (qemuProcess) {
    return { success: false, error: "QEMU is already running" };
  }

  // Detect or use specified mode
  const mode = options.mode || detectEnvironment();
  currentMode = mode;

  // Build first unless explicitly disabled (ensures fresh code)
  if (options.build !== false) {
    const buildTarget = mode === "rhel" ? "boot-image" : "boot-quick";
    try {
      await build(buildTarget);
    } catch (err) {
      return { success: false, error: `Build failed: ${err.message}` };
    }
  }

  const ovmf = findOvmf(mode);
  if (!ovmf) {
    return { success: false, error: "OVMF firmware not found" };
  }

  const qemu = findQemu(mode);
  const bootDir = join(PROJECT_ROOT, "target/boot");

  // Clean up old sockets/files
  if (existsSync(MONITOR_SOCKET)) unlinkSync(MONITOR_SOCKET);
  if (existsSync(SERIAL_LOG)) unlinkSync(SERIAL_LOG);

  serialBuffer = [];

  let args;

  if (mode === "rhel") {
    // RHEL mode: Use disk image (qemu-kvm doesn't support fat: protocol)
    // First ensure boot.img exists
    if (!existsSync(BOOT_IMAGE)) {
      return {
        success: false,
        error: "Boot image not found. Run 'make boot-image' first for RHEL mode.",
      };
    }

    // Copy OVMF_VARS for this session
    if (ovmf.vars && existsSync(ovmf.vars)) {
      try {
        execSync(`cp "${ovmf.vars}" "${OVMF_VARS}"`, { stdio: "pipe" });
      } catch (e) {
        // Ignore if already exists
      }
    }

    args = [
      "-machine", "q35,accel=kvm:tcg",
      "-cpu", "max",
      "-smp", "2",
      "-m", options.memory || "256M",
      "-drive", `if=pflash,format=raw,readonly=on,file=${ovmf.code}`,
      "-drive", `if=pflash,format=raw,file=${OVMF_VARS}`,
      "-drive", `file=${BOOT_IMAGE},format=raw,if=none,id=bootdisk`,
      "-device", "ide-hd,drive=bootdisk,bus=ide.0",
      "-serial", `file:${SERIAL_LOG}`,
      "-monitor", `unix:${MONITOR_SOCKET},server,nowait`,
      "-display", "none",
      "-no-reboot",
    ];
  } else {
    // Fedora mode: Use fat: protocol
    args = [
      "-machine", "q35",
      "-cpu", "qemu64,+smap,+smep",
      "-m", options.memory || "256M",
      "-bios", ovmf.code,
      "-drive", `format=raw,file=fat:rw:${bootDir},if=none,id=disk`,
      "-device", "ide-hd,drive=disk",
      "-serial", `file:${SERIAL_LOG}`,
      "-monitor", `unix:${MONITOR_SOCKET},server,nowait`,
      "-display", "none",
      "-no-reboot",
    ];
  }

  if (options.networking !== false) {
    args.push(
      "-device", "virtio-net-pci,netdev=net0",
      "-netdev", "user,id=net0,hostfwd=tcp::2222-:22"
    );
  }

  qemuProcess = spawn(qemu, args, {
    cwd: PROJECT_ROOT,
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, TMPDIR: "/tmp/qemu-oxide" },
  });

  // Save PID
  writeFileSync(PID_FILE, qemuProcess.pid.toString());

  qemuProcess.on("close", (code) => {
    qemuProcess = null;
    if (existsSync(PID_FILE)) unlinkSync(PID_FILE);
  });

  // Wait a moment for QEMU to start
  await new Promise((r) => setTimeout(r, 500));

  return {
    success: true,
    pid: qemuProcess.pid,
    mode: mode,
    message: `QEMU started in ${mode} mode`,
  };
}

// Stop QEMU
async function stopQemu() {
  if (!qemuProcess) {
    // Check for orphaned process
    if (existsSync(PID_FILE)) {
      try {
        const pid = parseInt(readFileSync(PID_FILE, "utf8"));
        process.kill(pid, "SIGTERM");
        unlinkSync(PID_FILE);
        return { success: true, message: "Killed orphaned QEMU process" };
      } catch {
        if (existsSync(PID_FILE)) unlinkSync(PID_FILE);
      }
    }
    return { success: false, error: "QEMU is not running" };
  }

  qemuProcess.kill("SIGTERM");

  // Wait for process to exit
  await new Promise((resolve) => {
    const timeout = setTimeout(() => {
      if (qemuProcess) qemuProcess.kill("SIGKILL");
      resolve();
    }, 3000);

    qemuProcess.on("close", () => {
      clearTimeout(timeout);
      resolve();
    });
  });

  qemuProcess = null;
  return { success: true, message: "QEMU stopped" };
}

// Get QEMU status
function getStatus() {
  const running = qemuProcess !== null;
  let pid = null;

  if (running) {
    pid = qemuProcess.pid;
  } else if (existsSync(PID_FILE)) {
    try {
      pid = parseInt(readFileSync(PID_FILE, "utf8"));
      // Check if process exists
      process.kill(pid, 0);
    } catch {
      pid = null;
      if (existsSync(PID_FILE)) unlinkSync(PID_FILE);
    }
  }

  const detectedMode = detectEnvironment();

  return {
    running: running || pid !== null,
    pid,
    mode: currentMode,
    detectedEnvironment: detectedMode,
    serialLogExists: existsSync(SERIAL_LOG),
    bootImageExists: existsSync(BOOT_IMAGE),
  };
}

// Read serial output
function readSerial(options = {}) {
  const { lines = 100, fromStart = false } = options;

  if (!existsSync(SERIAL_LOG)) {
    return { success: false, error: "No serial log found", output: "" };
  }

  try {
    const content = readFileSync(SERIAL_LOG, "utf8");
    const allLines = content.split("\n");

    let output;
    if (fromStart) {
      output = allLines.slice(0, lines).join("\n");
    } else {
      output = allLines.slice(-lines).join("\n");
    }

    return {
      success: true,
      output,
      totalLines: allLines.length,
    };
  } catch (err) {
    return { success: false, error: err.message, output: "" };
  }
}

// Take screenshot
async function takeScreenshot() {
  try {
    await monitorCommand(`screendump ${SCREENSHOT_PATH}`);

    if (existsSync(SCREENSHOT_PATH)) {
      // Convert PPM to base64 for potential display
      const data = readFileSync(SCREENSHOT_PATH);
      return {
        success: true,
        path: SCREENSHOT_PATH,
        size: data.length,
        message: `Screenshot saved to ${SCREENSHOT_PATH}`,
      };
    } else {
      return { success: false, error: "Screenshot file not created" };
    }
  } catch (err) {
    return { success: false, error: err.message };
  }
}

// Send keystrokes
async function sendKeys(keys) {
  try {
    // QEMU sendkey format: sendkey key1-key2-...
    // For text, we need to send each character
    const result = await monitorCommand(`sendkey ${keys}`);
    return { success: true, message: `Sent keys: ${keys}` };
  } catch (err) {
    return { success: false, error: err.message };
  }
}

// Send text (converts to key sequences)
async function sendText(text) {
  const keyMap = {
    " ": "spc",
    "\n": "ret",
    "\t": "tab",
    "-": "minus",
    "=": "equal",
    "[": "bracket_left",
    "]": "bracket_right",
    ";": "semicolon",
    "'": "apostrophe",
    ",": "comma",
    ".": "dot",
    "/": "slash",
    "\\": "backslash",
    "`": "grave_accent",
  };

  const results = [];
  for (const char of text) {
    let key;
    if (/[a-z]/.test(char)) {
      key = char;
    } else if (/[A-Z]/.test(char)) {
      key = `shift-${char.toLowerCase()}`;
    } else if (/[0-9]/.test(char)) {
      key = char;
    } else if (keyMap[char]) {
      key = keyMap[char];
    } else {
      continue; // Skip unsupported chars
    }

    try {
      await monitorCommand(`sendkey ${key}`);
      results.push(key);
      await new Promise((r) => setTimeout(r, 50)); // Small delay between keys
    } catch (err) {
      // Continue on error
    }
  }

  return { success: true, message: `Sent ${results.length} keys` };
}

// MCP Server setup
const server = new Server(
  { name: "oxide-qemu-mcp", version: "1.0.0" },
  { capabilities: { tools: {} } }
);

// List tools
server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [
    {
      name: "qemu_build",
      description: "Build the OXIDE kernel and bootloader. Run this before starting QEMU.",
      inputSchema: {
        type: "object",
        properties: {
          target: {
            type: "string",
            description: "Make target (default: boot-quick). Options: build, boot-quick, boot-dir, initramfs",
            default: "boot-quick",
          },
        },
      },
    },
    {
      name: "qemu_start",
      description: "Build and start QEMU with the OXIDE kernel. Auto-detects Fedora vs RHEL mode. Runs headless with serial output captured.",
      inputSchema: {
        type: "object",
        properties: {
          mode: {
            type: "string",
            description: "QEMU mode: 'fedora' (qemu-system-x86_64 with fat:) or 'rhel' (qemu-kvm with disk image). Auto-detected if not specified.",
            enum: ["fedora", "rhel"],
          },
          build: {
            type: "boolean",
            description: "Build before starting (default: true). Set false to skip build.",
            default: true,
          },
          memory: {
            type: "string",
            description: "Memory size (default: 256M)",
            default: "256M",
          },
          networking: {
            type: "boolean",
            description: "Enable networking (default: true)",
            default: true,
          },
        },
      },
    },
    {
      name: "qemu_stop",
      description: "Stop the running QEMU instance",
      inputSchema: { type: "object", properties: {} },
    },
    {
      name: "qemu_status",
      description: "Check if QEMU is running and get status information",
      inputSchema: { type: "object", properties: {} },
    },
    {
      name: "qemu_serial",
      description: "Read serial output from the running or last QEMU session",
      inputSchema: {
        type: "object",
        properties: {
          lines: {
            type: "number",
            description: "Number of lines to return (default: 100)",
            default: 100,
          },
          fromStart: {
            type: "boolean",
            description: "If true, return lines from start instead of end",
            default: false,
          },
        },
      },
    },
    {
      name: "qemu_screenshot",
      description: "Take a screenshot of the QEMU display",
      inputSchema: { type: "object", properties: {} },
    },
    {
      name: "qemu_sendkeys",
      description: "Send keystrokes to QEMU (QEMU key format, e.g., 'ret', 'a', 'shift-a')",
      inputSchema: {
        type: "object",
        properties: {
          keys: {
            type: "string",
            description: "Keys to send (QEMU format)",
          },
        },
        required: ["keys"],
      },
    },
    {
      name: "qemu_sendtext",
      description: "Send text to QEMU (converts text to key sequences)",
      inputSchema: {
        type: "object",
        properties: {
          text: {
            type: "string",
            description: "Text to type",
          },
        },
        required: ["text"],
      },
    },
    {
      name: "qemu_command",
      description: "Send a raw command to the QEMU monitor",
      inputSchema: {
        type: "object",
        properties: {
          command: {
            type: "string",
            description: "QEMU monitor command",
          },
        },
        required: ["command"],
      },
    },
  ],
}));

// Handle tool calls
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  try {
    let result;

    switch (name) {
      case "qemu_build":
        result = await build(args?.target || "boot-quick");
        break;

      case "qemu_start":
        result = await startQemu(args || {});
        break;

      case "qemu_stop":
        result = await stopQemu();
        break;

      case "qemu_status":
        result = getStatus();
        break;

      case "qemu_serial":
        result = readSerial(args || {});
        break;

      case "qemu_screenshot":
        result = await takeScreenshot();
        break;

      case "qemu_sendkeys":
        result = await sendKeys(args.keys);
        break;

      case "qemu_sendtext":
        result = await sendText(args.text);
        break;

      case "qemu_command":
        result = { response: await monitorCommand(args.command) };
        break;

      default:
        result = { error: `Unknown tool: ${name}` };
    }

    return {
      content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    };
  } catch (err) {
    return {
      content: [{ type: "text", text: JSON.stringify({ error: err.message }) }],
      isError: true,
    };
  }
});

// Start server
async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch(console.error);
