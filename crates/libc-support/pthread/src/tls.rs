//! Thread-local storage (TLS)

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::ffi::{c_int, c_void};
use spin::Mutex;

use crate::{ESUCCESS, EINVAL, EAGAIN, pthread_self};

/// TLS key type
pub type pthread_key_t = u32;

/// Maximum number of TLS keys
const PTHREAD_KEYS_MAX: usize = 1024;

/// TLS key destructor
type Destructor = Option<extern "C" fn(*mut c_void)>;

/// TLS key metadata
struct TlsKey {
    allocated: bool,
    destructor: Destructor,
}

/// Wrapper for raw pointer to make it Send/Sync
/// Safety: TLS values are protected by mutex and only accessed from their owning thread
#[derive(Clone, Copy)]
struct TlsValue(usize);

unsafe impl Send for TlsValue {}
unsafe impl Sync for TlsValue {}

impl TlsValue {
    fn from_ptr(ptr: *mut c_void) -> Self {
        TlsValue(ptr as usize)
    }

    fn to_ptr(self) -> *mut c_void {
        self.0 as *mut c_void
    }
}

/// Global TLS key registry
static TLS_KEYS: Mutex<Option<Vec<TlsKey>>> = Mutex::new(None);

/// Per-thread TLS values: thread_id -> (key -> value)
static TLS_VALUES: Mutex<Option<BTreeMap<u64, BTreeMap<pthread_key_t, TlsValue>>>> = Mutex::new(None);

fn get_keys() -> spin::MutexGuard<'static, Option<Vec<TlsKey>>> {
    let mut keys = TLS_KEYS.lock();
    if keys.is_none() {
        let mut v = Vec::with_capacity(PTHREAD_KEYS_MAX);
        for _ in 0..PTHREAD_KEYS_MAX {
            v.push(TlsKey {
                allocated: false,
                destructor: None,
            });
        }
        *keys = Some(v);
    }
    keys
}

fn get_values() -> spin::MutexGuard<'static, Option<BTreeMap<u64, BTreeMap<pthread_key_t, TlsValue>>>> {
    let mut values = TLS_VALUES.lock();
    if values.is_none() {
        *values = Some(BTreeMap::new());
    }
    values
}

/// Create a TLS key
#[no_mangle]
pub unsafe extern "C" fn pthread_key_create(
    key: *mut pthread_key_t,
    destructor: Option<extern "C" fn(*mut c_void)>,
) -> c_int {
    if key.is_null() {
        return EINVAL;
    }

    let mut keys = get_keys();
    if let Some(ref mut keys_vec) = *keys {
        for (i, k) in keys_vec.iter_mut().enumerate() {
            if !k.allocated {
                k.allocated = true;
                k.destructor = destructor;
                *key = i as pthread_key_t;
                return ESUCCESS;
            }
        }
    }

    EAGAIN
}

/// Delete a TLS key
#[no_mangle]
pub unsafe extern "C" fn pthread_key_delete(key: pthread_key_t) -> c_int {
    let mut keys = get_keys();
    if let Some(ref mut keys_vec) = *keys {
        let idx = key as usize;
        if idx >= keys_vec.len() || !keys_vec[idx].allocated {
            return EINVAL;
        }
        keys_vec[idx].allocated = false;
        keys_vec[idx].destructor = None;
    }

    let mut values = get_values();
    if let Some(ref mut thread_map) = *values {
        for (_, key_map) in thread_map.iter_mut() {
            key_map.remove(&key);
        }
    }

    ESUCCESS
}

/// Get thread-specific value
#[no_mangle]
pub unsafe extern "C" fn pthread_getspecific(key: pthread_key_t) -> *mut c_void {
    let tid = pthread_self();

    let values = get_values();
    if let Some(ref thread_map) = *values {
        if let Some(key_map) = thread_map.get(&tid) {
            if let Some(&value) = key_map.get(&key) {
                return value.to_ptr();
            }
        }
    }

    core::ptr::null_mut()
}

/// Set thread-specific value
#[no_mangle]
pub unsafe extern "C" fn pthread_setspecific(key: pthread_key_t, value: *mut c_void) -> c_int {
    {
        let keys = get_keys();
        if let Some(ref keys_vec) = *keys {
            let idx = key as usize;
            if idx >= keys_vec.len() || !keys_vec[idx].allocated {
                return EINVAL;
            }
        }
    }

    let tid = pthread_self();

    let mut values = get_values();
    if let Some(ref mut thread_map) = *values {
        let key_map = thread_map.entry(tid).or_insert_with(BTreeMap::new);
        key_map.insert(key, TlsValue::from_ptr(value));
    }

    ESUCCESS
}

/// Call destructors for thread exit
pub unsafe fn thread_tls_cleanup(tid: u64) {
    const MAX_ITERATIONS: usize = 4;

    for _ in 0..MAX_ITERATIONS {
        let mut any_called = false;

        let thread_values: Option<BTreeMap<pthread_key_t, TlsValue>> = {
            let mut values = get_values();
            if let Some(ref mut thread_map) = *values {
                thread_map.remove(&tid)
            } else {
                None
            }
        };

        if let Some(key_map) = thread_values {
            for (key, value) in key_map {
                let ptr = value.to_ptr();
                if ptr.is_null() {
                    continue;
                }

                let destructor = {
                    let keys = get_keys();
                    if let Some(ref keys_vec) = *keys {
                        let idx = key as usize;
                        if idx < keys_vec.len() && keys_vec[idx].allocated {
                            keys_vec[idx].destructor
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                if let Some(dtor) = destructor {
                    any_called = true;
                    dtor(ptr);
                }
            }
        }

        if !any_called {
            break;
        }
    }
}
