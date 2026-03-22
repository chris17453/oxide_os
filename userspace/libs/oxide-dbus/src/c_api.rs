//! libdbus-1 Compatible C API
//!
//! — PatchBay: The Trojan horse. GTK and glib link against libdbus-1.so and call
//! functions like dbus_bus_get(), dbus_message_new_method_call(), etc. We provide
//! these exact symbols backed by our Rust implementation. GTK doesn't know — and
//! doesn't care — that it's talking to oxide-dbus, not the reference implementation.
//!
//! We implement only the ~30 functions GTK/glib actually calls. The full libdbus-1
//! API has hundreds of functions, but most are unused in practice.

// — PatchBay: C API shim. Each function creates/manipulates our Rust types through
// opaque pointers. The C side sees `DBusConnection*` and `DBusMessage*` — really
// they're `Box<Connection>` and `Box<Message>` behind a raw pointer cast.

// NOTE: This is compiled as part of the oxide-dbus staticlib/rlib. When we build
// libdbus-1.so for the sysroot, we'll compile this crate and export these symbols.

/// Placeholder for the C API — full implementation comes when we build the .so
/// For now, define the key types and stub the functions that glib probes during
/// configure-time checks.

/// Opaque handle types (C sees these as pointers)
pub type DBusConnection = u8;
pub type DBusMessage = u8;
pub type DBusPendingCall = u8;

/// dbus_bus_get return type
pub const DBUS_BUS_SESSION: i32 = 0;
pub const DBUS_BUS_SYSTEM: i32 = 1;

/// Message type constants
pub const DBUS_MESSAGE_TYPE_METHOD_CALL: i32 = 1;
pub const DBUS_MESSAGE_TYPE_METHOD_RETURN: i32 = 2;
pub const DBUS_MESSAGE_TYPE_ERROR: i32 = 3;
pub const DBUS_MESSAGE_TYPE_SIGNAL: i32 = 4;

/// DBusError struct (C-compatible layout)
#[repr(C)]
pub struct DBusError {
    pub name: *const u8,
    pub message: *const u8,
    pub dummy1: u32,
    pub dummy2: u32,
    pub dummy3: u32,
    pub dummy4: u32,
    pub dummy5: *const u8,
}

// ============================================================================
// Core Connection Functions
// ============================================================================

/// dbus_bus_get — Get a connection to the message bus.
/// — PatchBay: This is the entry point. GTK calls this at startup.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_bus_get(_bus_type: i32, _error: *mut DBusError) -> *mut DBusConnection {
    // TODO: Connect to oxide-dbusd via AF_UNIX socket
    // For now return NULL (will signal that dbus is unavailable)
    // GTK handles this gracefully — it just disables dbus features
    core::ptr::null_mut()
}

/// dbus_bus_get_private — Get a private connection.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_bus_get_private(_bus_type: i32, _error: *mut DBusError) -> *mut DBusConnection {
    core::ptr::null_mut()
}

/// dbus_connection_ref — Increment reference count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_connection_ref(conn: *mut DBusConnection) -> *mut DBusConnection {
    conn
}

/// dbus_connection_unref — Decrement reference count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_connection_unref(_conn: *mut DBusConnection) {
}

/// dbus_connection_close — Close a connection.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_connection_close(_conn: *mut DBusConnection) {
}

/// dbus_connection_flush — Flush pending outgoing messages.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_connection_flush(_conn: *mut DBusConnection) {
}

/// dbus_connection_get_is_connected — Check if connection is alive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_connection_get_is_connected(_conn: *mut DBusConnection) -> i32 {
    0 // Not connected (safe default)
}

/// dbus_bus_get_unique_name — Get our unique name on the bus.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_bus_get_unique_name(_conn: *mut DBusConnection) -> *const u8 {
    core::ptr::null()
}

/// dbus_connection_send — Send a message (no reply expected).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_connection_send(
    _conn: *mut DBusConnection,
    _msg: *mut DBusMessage,
    _serial: *mut u32,
) -> i32 {
    0 // FALSE
}

/// dbus_connection_send_with_reply_and_block — Send and wait for reply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_connection_send_with_reply_and_block(
    _conn: *mut DBusConnection,
    _msg: *mut DBusMessage,
    _timeout_ms: i32,
    _error: *mut DBusError,
) -> *mut DBusMessage {
    core::ptr::null_mut()
}

// ============================================================================
// Message Functions
// ============================================================================

/// dbus_message_new_method_call — Create a new method call message.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_message_new_method_call(
    _dest: *const u8,
    _path: *const u8,
    _iface: *const u8,
    _method: *const u8,
) -> *mut DBusMessage {
    core::ptr::null_mut()
}

/// dbus_message_new_signal — Create a new signal message.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_message_new_signal(
    _path: *const u8,
    _iface: *const u8,
    _name: *const u8,
) -> *mut DBusMessage {
    core::ptr::null_mut()
}

/// dbus_message_ref — Increment message reference count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_message_ref(msg: *mut DBusMessage) -> *mut DBusMessage {
    msg
}

/// dbus_message_unref — Decrement message reference count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_message_unref(_msg: *mut DBusMessage) {
}

/// dbus_message_get_type — Get message type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_message_get_type(_msg: *mut DBusMessage) -> i32 {
    0 // INVALID
}

/// dbus_message_get_serial — Get message serial number.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_message_get_serial(_msg: *mut DBusMessage) -> u32 {
    0
}

/// dbus_message_get_path — Get object path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_message_get_path(_msg: *mut DBusMessage) -> *const u8 {
    core::ptr::null()
}

/// dbus_message_get_interface — Get interface name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_message_get_interface(_msg: *mut DBusMessage) -> *const u8 {
    core::ptr::null()
}

/// dbus_message_get_member — Get member name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_message_get_member(_msg: *mut DBusMessage) -> *const u8 {
    core::ptr::null()
}

/// dbus_message_get_sender — Get sender.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_message_get_sender(_msg: *mut DBusMessage) -> *const u8 {
    core::ptr::null()
}

/// dbus_message_get_destination — Get destination.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_message_get_destination(_msg: *mut DBusMessage) -> *const u8 {
    core::ptr::null()
}

// ============================================================================
// Error Functions
// ============================================================================

/// dbus_error_init — Initialize an error struct.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_error_init(error: *mut DBusError) {
    if !error.is_null() {
        unsafe {
            (*error).name = core::ptr::null();
            (*error).message = core::ptr::null();
        }
    }
}

/// dbus_error_is_set — Check if error is set.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_error_is_set(error: *const DBusError) -> i32 {
    if error.is_null() {
        return 0;
    }
    if unsafe { (*error).name.is_null() } { 0 } else { 1 }
}

/// dbus_error_free — Free an error struct.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_error_free(error: *mut DBusError) {
    if !error.is_null() {
        unsafe {
            (*error).name = core::ptr::null();
            (*error).message = core::ptr::null();
        }
    }
}

// ============================================================================
// Misc Functions
// ============================================================================

/// dbus_threads_init_default — Initialize threading (no-op for us).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_threads_init_default() -> i32 {
    1 // TRUE
}

/// dbus_connection_set_exit_on_disconnect — Control exit behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_connection_set_exit_on_disconnect(
    _conn: *mut DBusConnection,
    _exit_on_disconnect: i32,
) {
}

/// dbus_bus_request_name — Request a well-known name on the bus.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_bus_request_name(
    _conn: *mut DBusConnection,
    _name: *const u8,
    _flags: u32,
    _error: *mut DBusError,
) -> i32 {
    -1 // DBUS_REQUEST_NAME_REPLY_ERROR
}

/// dbus_bus_add_match — Add a match rule for signals.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_bus_add_match(
    _conn: *mut DBusConnection,
    _rule: *const u8,
    _error: *mut DBusError,
) {
}
