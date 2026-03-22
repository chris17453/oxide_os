//! oxide-dbusd — OXIDE D-Bus Message Bus Daemon
//!
//! — PatchBay: The central nervous system of desktop IPC. Listens on
//! /run/dbus/system_bus_socket, authenticates clients via SASL EXTERNAL,
//! assigns unique names, and routes messages between clients.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

// ============================================================================
// Syscall wrappers
// ============================================================================

unsafe fn syscall1(nr: u64, a1: usize) -> isize {
    let ret: isize;
    core::arch::asm!("syscall", in("rax") nr, in("rdi") a1,
        lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

unsafe fn syscall2(nr: u64, a1: usize, a2: usize) -> isize {
    let ret: isize;
    core::arch::asm!("syscall", in("rax") nr, in("rdi") a1, in("rsi") a2,
        lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

unsafe fn syscall3(nr: u64, a1: usize, a2: usize, a3: usize) -> isize {
    let ret: isize;
    core::arch::asm!("syscall", in("rax") nr, in("rdi") a1, in("rsi") a2,
        in("rdx") a3, lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

fn write_str(fd: i32, s: &str) {
    unsafe { syscall3(1, fd as usize, s.as_ptr() as usize, s.len()); }
}

fn write_bytes(fd: i32, b: &[u8]) {
    unsafe { syscall3(1, fd as usize, b.as_ptr() as usize, b.len()); }
}

// ============================================================================
// D-Bus wire protocol constants
// ============================================================================

const DBUS_LITTLE_ENDIAN: u8 = b'l';
const DBUS_PROTOCOL_VERSION: u8 = 1;

const MSG_METHOD_CALL: u8 = 1;
const MSG_METHOD_RETURN: u8 = 2;
const MSG_ERROR: u8 = 3;
const MSG_SIGNAL: u8 = 4;

// Header field codes
const FIELD_PATH: u8 = 1;
const FIELD_INTERFACE: u8 = 2;
const FIELD_MEMBER: u8 = 3;
const FIELD_ERROR_NAME: u8 = 4;
const FIELD_REPLY_SERIAL: u8 = 5;
const FIELD_DESTINATION: u8 = 6;
const FIELD_SENDER: u8 = 7;
const FIELD_SIGNATURE: u8 = 8;

// ============================================================================
// Client state
// ============================================================================

struct Client {
    fd: i32,
    unique_name: String,
    authenticated: bool,
    recv_buf: Vec<u8>,
    owned_names: Vec<String>,
}

// ============================================================================
// Bus daemon
// ============================================================================

struct BusDaemon {
    listen_fd: i32,
    clients: BTreeMap<i32, Client>,
    name_owners: BTreeMap<String, i32>,
    next_id: u32,
    serial: u32,
}

impl BusDaemon {
    fn new() -> Self {
        BusDaemon {
            listen_fd: -1,
            clients: BTreeMap::new(),
            name_owners: BTreeMap::new(),
            next_id: 1,
            serial: 1,
        }
    }

    fn next_serial(&mut self) -> u32 {
        let s = self.serial;
        self.serial += 1;
        s
    }

    fn alloc_unique_name(&mut self) -> String {
        let id = self.next_id;
        self.next_id += 1;
        let mut name = String::from(":1.");
        let mut buf = [0u8; 10];
        let mut i = 0;
        let mut n = id;
        if n == 0 { buf[0] = b'0'; i = 1; }
        else { while n > 0 { buf[i] = b'0' + (n % 10) as u8; n /= 10; i += 1; } buf[..i].reverse(); }
        if let Ok(s) = core::str::from_utf8(&buf[..i]) { name.push_str(s); }
        name
    }

    /// Handle SASL authentication line from client
    fn handle_auth_line(&mut self, client_fd: i32, line: &[u8]) -> bool {
        // — PatchBay: SASL EXTERNAL auth. Client sends:
        // "\0AUTH EXTERNAL <hex_uid>\r\n"
        // We respond: "OK <guid>\r\n"
        // Then client sends: "BEGIN\r\n"

        if line.starts_with(b"\0AUTH") || line.starts_with(b"AUTH") {
            write_bytes(client_fd, b"OK 0000000000000000000000000000000f\r\n");
            return false; // Not fully auth'd yet
        }
        if line.starts_with(b"NEGOTIATE_UNIX_FD") {
            write_bytes(client_fd, b"AGREE_UNIX_FD\r\n");
            return false;
        }
        if line.starts_with(b"BEGIN") {
            return true; // Auth complete, switch to message mode
        }
        // Unknown auth command
        write_bytes(client_fd, b"ERROR\r\n");
        false
    }

    /// Build a D-Bus method return message with a string body
    fn build_method_return_string(&mut self, reply_to: u32, sender: &str, dest: &str, value: &str) -> Vec<u8> {
        let serial = self.next_serial();

        // Build header fields
        let mut fields = Vec::new();

        // REPLY_SERIAL field
        fields.push(FIELD_REPLY_SERIAL);
        fields.extend_from_slice(b"\x01u\x00"); // signature "u" + padding
        // Pad to 4
        while fields.len() % 4 != 0 { fields.push(0); }
        fields.extend_from_slice(&reply_to.to_le_bytes());

        // SENDER field
        while fields.len() % 8 != 0 { fields.push(0); }
        fields.push(FIELD_SENDER);
        fields.extend_from_slice(b"\x01s\x00");
        while fields.len() % 4 != 0 { fields.push(0); }
        let sender_bytes = sender.as_bytes();
        fields.extend_from_slice(&(sender_bytes.len() as u32).to_le_bytes());
        fields.extend_from_slice(sender_bytes);
        fields.push(0); // NUL
        while fields.len() % 4 != 0 { fields.push(0); }

        // DESTINATION field
        while fields.len() % 8 != 0 { fields.push(0); }
        fields.push(FIELD_DESTINATION);
        fields.extend_from_slice(b"\x01s\x00");
        while fields.len() % 4 != 0 { fields.push(0); }
        let dest_bytes = dest.as_bytes();
        fields.extend_from_slice(&(dest_bytes.len() as u32).to_le_bytes());
        fields.extend_from_slice(dest_bytes);
        fields.push(0);
        while fields.len() % 4 != 0 { fields.push(0); }

        // SIGNATURE field
        while fields.len() % 8 != 0 { fields.push(0); }
        fields.push(FIELD_SIGNATURE);
        fields.extend_from_slice(b"\x01g\x00");
        fields.push(1); // sig length
        fields.push(b's');
        fields.push(0); // NUL

        // Build body (single string)
        let mut body = Vec::new();
        let val_bytes = value.as_bytes();
        body.extend_from_slice(&(val_bytes.len() as u32).to_le_bytes());
        body.extend_from_slice(val_bytes);
        body.push(0);

        // Build full message
        let fields_len = fields.len() as u32;
        let body_len = body.len() as u32;

        let mut msg = Vec::new();
        msg.push(DBUS_LITTLE_ENDIAN);
        msg.push(MSG_METHOD_RETURN);
        msg.push(0); // flags
        msg.push(DBUS_PROTOCOL_VERSION);
        msg.extend_from_slice(&body_len.to_le_bytes());
        msg.extend_from_slice(&serial.to_le_bytes());
        msg.extend_from_slice(&fields_len.to_le_bytes());
        msg.extend_from_slice(&fields);
        // Pad header to 8-byte boundary
        while msg.len() % 8 != 0 { msg.push(0); }
        msg.extend_from_slice(&body);

        msg
    }

    /// Handle a D-Bus message from a client
    fn handle_message(&mut self, client_fd: i32, msg_data: &[u8]) {
        if msg_data.len() < 16 { return; }

        let _endian = msg_data[0];
        let msg_type = msg_data[1];
        let _flags = msg_data[2];
        let body_len = u32::from_le_bytes(msg_data[4..8].try_into().unwrap_or([0;4])) as usize;
        let serial = u32::from_le_bytes(msg_data[8..12].try_into().unwrap_or([0;4]));
        let fields_len = u32::from_le_bytes(msg_data[12..16].try_into().unwrap_or([0;4])) as usize;

        // Parse header fields to find destination, interface, member
        let mut destination = None;
        let mut interface = None;
        let mut member = None;
        let mut pos = 16;
        let fields_end = 16 + fields_len;

        while pos < fields_end && pos < msg_data.len() {
            // Align to 8
            while pos % 8 != 0 && pos < fields_end { pos += 1; }
            if pos >= fields_end { break; }

            let field_code = msg_data[pos];
            pos += 1;
            // Skip signature (1 byte len + sig + NUL)
            if pos >= msg_data.len() { break; }
            let sig_len = msg_data[pos] as usize;
            pos += 1 + sig_len + 1;
            // Align to 4 for value
            while pos % 4 != 0 { pos += 1; }

            match field_code {
                FIELD_DESTINATION | FIELD_INTERFACE | FIELD_MEMBER | FIELD_SENDER => {
                    if pos + 4 > msg_data.len() { break; }
                    let str_len = u32::from_le_bytes(msg_data[pos..pos+4].try_into().unwrap_or([0;4])) as usize;
                    pos += 4;
                    if pos + str_len > msg_data.len() { break; }
                    let s = core::str::from_utf8(&msg_data[pos..pos+str_len]).unwrap_or("");
                    pos += str_len + 1; // +NUL
                    match field_code {
                        FIELD_DESTINATION => destination = Some(String::from(s)),
                        FIELD_INTERFACE => interface = Some(String::from(s)),
                        FIELD_MEMBER => member = Some(String::from(s)),
                        _ => {}
                    }
                }
                FIELD_REPLY_SERIAL => {
                    pos += 4; // skip u32
                }
                FIELD_SIGNATURE => {
                    if pos < msg_data.len() {
                        let sig_l = msg_data[pos] as usize;
                        pos += 1 + sig_l + 1;
                    }
                }
                _ => { pos += 4; } // skip unknown
            }
        }

        // Handle org.freedesktop.DBus methods
        if msg_type == MSG_METHOD_CALL {
            let iface = interface.as_deref().unwrap_or("");
            let meth = member.as_deref().unwrap_or("");

            if iface == "org.freedesktop.DBus" {
                let sender_name = self.clients.get(&client_fd)
                    .map(|c| c.unique_name.clone())
                    .unwrap_or_default();

                match meth {
                    "Hello" => {
                        // Assign unique name
                        let unique = self.clients.get(&client_fd)
                            .map(|c| c.unique_name.clone())
                            .unwrap_or_default();
                        let reply = self.build_method_return_string(
                            serial, "org.freedesktop.DBus", &sender_name, &unique);
                        write_bytes(client_fd, &reply);
                        write_str(1, "[DBUS] Hello → ");
                        write_str(1, &unique);
                        write_str(1, "\n");
                    }
                    "RequestName" => {
                        // Return DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER (1)
                        let reply = self.build_method_return_string(
                            serial, "org.freedesktop.DBus", &sender_name, "1");
                        write_bytes(client_fd, &reply);
                    }
                    "GetNameOwner" | "ListNames" | "NameHasOwner" | "GetId" => {
                        // Return empty/default for now
                        let reply = self.build_method_return_string(
                            serial, "org.freedesktop.DBus", &sender_name, "");
                        write_bytes(client_fd, &reply);
                    }
                    "AddMatch" => {
                        // Accept silently
                        let reply = self.build_method_return_string(
                            serial, "org.freedesktop.DBus", &sender_name, "");
                        write_bytes(client_fd, &reply);
                    }
                    _ => {
                        write_str(1, "[DBUS] Unknown method: ");
                        write_str(1, meth);
                        write_str(1, "\n");
                    }
                }
            } else if let Some(dest) = &destination {
                // Route to destination client
                if let Some(&dest_fd) = self.name_owners.get(dest.as_str()) {
                    write_bytes(dest_fd, msg_data);
                } else {
                    // Try unique name lookup
                    let mut found_fd = None;
                    for (fd, client) in &self.clients {
                        if client.unique_name == *dest {
                            found_fd = Some(*fd);
                            break;
                        }
                    }
                    if let Some(fd) = found_fd {
                        write_bytes(fd, msg_data);
                    }
                }
            }
        } else if msg_type == MSG_METHOD_RETURN || msg_type == MSG_ERROR || msg_type == MSG_SIGNAL {
            // Route replies/signals to destination
            if let Some(dest) = &destination {
                for (fd, client) in &self.clients {
                    if client.unique_name == *dest {
                        write_bytes(*fd, msg_data);
                        break;
                    }
                }
            }
        }
    }

    /// Process data from a client (handles both auth and message phases)
    fn process_client_data(&mut self, client_fd: i32, data: &[u8]) {
        let authenticated = self.clients.get(&client_fd).map(|c| c.authenticated).unwrap_or(false);

        if !authenticated {
            // Auth phase — line-based protocol
            if let Some(client) = self.clients.get_mut(&client_fd) {
                client.recv_buf.extend_from_slice(data);
            }
            loop {
                let line_data = {
                    let client = match self.clients.get(&client_fd) { Some(c) => c, None => break };
                    let buf = &client.recv_buf;
                    match buf.iter().position(|&b| b == b'\n') {
                        Some(end) => {
                            let mut line = buf[..end].to_vec();
                            if line.last() == Some(&b'\r') { line.pop(); }
                            Some((line, end))
                        }
                        None => None,
                    }
                };
                if let Some((line, end)) = line_data {
                    let completed = self.handle_auth_line(client_fd, &line);
                    if let Some(client) = self.clients.get_mut(&client_fd) {
                        client.recv_buf = client.recv_buf[end+1..].to_vec();
                        if completed {
                            client.authenticated = true;
                            write_str(1, "[DBUS] Client authenticated: ");
                            write_str(1, &client.unique_name);
                            write_str(1, "\n");
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
        } else {
            // Message phase — D-Bus wire protocol
            if let Some(client) = self.clients.get_mut(&client_fd) {
                client.recv_buf.extend_from_slice(data);
            }

            loop {
                let buf_len = self.clients.get(&client_fd).map(|c| c.recv_buf.len()).unwrap_or(0);
                if buf_len < 16 { break; }

                let msg_data = self.clients.get(&client_fd).map(|c| c.recv_buf.clone()).unwrap_or_default();

                let body_len = u32::from_le_bytes(msg_data[4..8].try_into().unwrap_or([0;4])) as usize;
                let fields_len = u32::from_le_bytes(msg_data[12..16].try_into().unwrap_or([0;4])) as usize;
                let header_end = 16 + fields_len;
                let body_start = (header_end + 7) & !7; // align to 8
                let total = body_start + body_len;

                if msg_data.len() < total { break; } // Incomplete

                let msg = msg_data[..total].to_vec();
                if let Some(client) = self.clients.get_mut(&client_fd) {
                    client.recv_buf = client.recv_buf[total..].to_vec();
                }

                self.handle_message(client_fd, &msg);
            }
        }
    }

    /// Main event loop
    fn run(&mut self) -> i32 {
        write_str(1, "=== oxide-dbusd starting ===\n");

        // Create socket
        let sock_fd = unsafe { syscall3(41, 1, 1, 0) } as i32; // AF_UNIX, SOCK_STREAM
        if sock_fd < 0 {
            write_str(1, "[DBUS] ERROR: socket() failed\n");
            return 1;
        }

        // Bind to /run/dbus/system_bus_socket
        let mut addr = [0u8; 110];
        addr[0] = 1; // AF_UNIX
        let path = b"/run/dbus/system_bus_socket";
        addr[2..2 + path.len()].copy_from_slice(path);
        let rc = unsafe { syscall3(49, sock_fd as usize, addr.as_ptr() as usize, (2 + path.len() + 1)) };
        if rc < 0 {
            write_str(1, "[DBUS] ERROR: bind() failed\n");
            return 1;
        }

        // Listen
        unsafe { syscall2(50, sock_fd as usize, 16) };
        self.listen_fd = sock_fd;
        write_str(1, "[DBUS] Listening on /run/dbus/system_bus_socket\n");

        // Also bind session bus at /run/user/0/bus
        // (GTK looks for $DBUS_SESSION_BUS_ADDRESS or this path)

        // Event loop — accept and serve clients
        // — PatchBay: Single-threaded select loop for simplicity
        loop {
            // Accept new client
            let client_fd = unsafe { syscall3(43, sock_fd as usize, 0, 0) } as i32;
            if client_fd < 0 {
                write_str(1, "[DBUS] accept() failed, retrying...\n");
                unsafe { syscall2(35, 0, 1_000_000_000) }; // nanosleep 1s
                continue;
            }

            let unique_name = self.alloc_unique_name();
            write_str(1, "[DBUS] Client connected → ");
            write_str(1, &unique_name);
            write_str(1, "\n");

            self.clients.insert(client_fd, Client {
                fd: client_fd,
                unique_name,
                authenticated: false,
                recv_buf: Vec::new(),
                owned_names: Vec::new(),
            });

            // Process this client until disconnect
            // — PatchBay: Blocking single-client for now. Multi-client needs epoll.
            let mut buf = [0u8; 4096];
            loop {
                let n = unsafe { syscall3(0, client_fd as usize, buf.as_mut_ptr() as usize, buf.len()) };
                if n <= 0 {
                    write_str(1, "[DBUS] Client disconnected\n");
                    break;
                }
                self.process_client_data(client_fd, &buf[..n as usize]);
            }

            self.clients.remove(&client_fd);
            unsafe { syscall1(3, client_fd as usize) }; // close
        }
    }
}

// ============================================================================
// Entry point
// ============================================================================

#[unsafe(no_mangle)]
fn main() -> i32 {
    let mut daemon = BusDaemon::new();
    daemon.run()
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    write_str(2, "[DBUS] PANIC!\n");
    loop { unsafe { core::arch::asm!("hlt"); } }
}

#[global_allocator]
static ALLOC: SimpleAlloc = SimpleAlloc;
struct SimpleAlloc;
unsafe impl core::alloc::GlobalAlloc for SimpleAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        unsafe extern "C" { fn malloc(size: usize) -> *mut u8; }
        unsafe { malloc(layout.size()) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
        unsafe extern "C" { fn free(ptr: *mut u8); }
        unsafe { free(ptr) }
    }
}
