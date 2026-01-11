//! FFI bindings for cross-language integration.
//!
//! Provides C-compatible functions for establishing inter-process communication
//! with non-Rust systems.
//!
//! # C Header Declaration
//!
//! To use these functions from C, declare them as:
//!
//! ```c
//! // praborrow.h
//! #ifndef PRABORROW_H
//! #define PRABORROW_H
//!
//! #include <stdint.h>
//!
//! // Initialize diplomatic relations with the PraBorrow runtime
//! // Returns: 0 on success, negative value on error
//! int32_t establish_relations(void);
//!
//! // Alternative name for establish_relations
//! int32_t init_ffi(void);
//!
//! // Send an envoy (notification) to foreign jurisdiction
//! // Returns: 0 on success, negative value on error
//! int32_t send_envoy(uint32_t id, const char* payload);
//!
//! // Receive an envoy from the PraBorrow runtime
//! // Returns: pointer to C string (caller must free), or NULL if no envoys
//! char* receive_envoy(void);
//!
//! // Free a string returned by receive_envoy
//! void free_envoy(char* envoy);
//!
//! #endif // PRABORROW_H
//! ```

use crossbeam_queue::SegQueue;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

/// Global registry for diplomatic state.
struct GlobalRegistry {
    /// Envoys received from the foreign jurisdiction, waiting to be processed by Rust.
    /// (Note: In a real system, this might be a channel receiver)
    incoming_envoys: SegQueue<String>,
    /// Envoys waiting to be sent to the foreign jurisdiction (outbox).
    /// For this FFI demo, we use this to store messages FROM Rust TO C.
    outbox: SegQueue<String>,
}

impl GlobalRegistry {
    fn new() -> Self {
        Self {
            incoming_envoys: SegQueue::new(),
            outbox: SegQueue::new(),
        }
    }
}

static REGISTRY: OnceLock<GlobalRegistry> = OnceLock::new();

/// Establishes diplomatic relations with the PraBorrow runtime.
///
/// Initializes the global registry.
///
/// # Returns
/// * `0` - Success
/// * `-1` - Already initialized
/// * `-2` - Initialization failed
#[unsafe(no_mangle)]
pub extern "C" fn establish_relations() -> c_int {
    if REGISTRY.get().is_some() {
        return -1;
    }

    match REGISTRY.set(GlobalRegistry::new()) {
        Ok(_) => {
            tracing::info!(
                event = "ffi_init",
                version = env!("CARGO_PKG_VERSION"),
                "Diplomatic relations established"
            );
            0
        }
        Err(_) => -2,
    }
}

/// Alternative name for `establish_relations`.
#[unsafe(no_mangle)]
pub extern "C" fn init_ffi() -> c_int {
    establish_relations()
}

/// Sends an envoy (notification) FROM the foreign jurisdiction TO Rust.
///
/// # Arguments
/// * `id` - Unique identifier
/// * `payload` - Null-terminated C string message
///
/// # Returns
/// * `0` - Success
/// * `-1` - Registry not initialized
/// * `-2` - Invalid string encoding
///
/// # Safety
///
/// * `payload` must be a valid pointer to a null-terminated C string.
/// * The memory pointed to by `payload` must remain valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn send_envoy(id: u32, payload: *const c_char) -> c_int {
    let registry = match REGISTRY.get() {
        Some(r) => r,
        None => return -1,
    };

    if payload.is_null() {
        return -2;
    }

    let c_str = unsafe { CStr::from_ptr(payload) };
    let r_str = match c_str.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return -2,
    };

    tracing::debug!(
        event = "envoy_received",
        envoy_id = id,
        payload = ?r_str,
        "Envoy received from foreign jurisdiction"
    );

    // In a real app, we'd do something with this.
    // For the sake of the 'receive_envoy' demo, let's echo it back
    // or store it in the outbox so C can read it back?
    // Let's store it in `incoming_envoys` for Rust to read (internally)
    // AND echo a response to `outbox` for C to read.
    
    registry.incoming_envoys.push(format!("ID:{}:{}", id, r_str));
    
    // Auto-reply for testing
    registry.outbox.push(format!("Ack: {}", r_str));

    0
}

/// Receives an envoy (message) FROM Rust TO the foreign jurisdiction.
///
/// Pops a message from the internal outbox.
///
/// # Returns
/// * `char*` - Pointer to null-terminated string. Ownership transferred to caller.
/// * `NULL` - No messages available or error.
#[unsafe(no_mangle)]
pub extern "C" fn receive_envoy() -> *mut c_char {
    let registry = match REGISTRY.get() {
        Some(r) => r,
        None => return std::ptr::null_mut(),
    };

    let msg = registry.outbox.pop();

    match msg {
        Some(s) => {
            match CString::new(s) {
                Ok(c_str) => c_str.into_raw(),
                Err(_) => std::ptr::null_mut(),
            }
        }
        None => std::ptr::null_mut(),
    }
}

/// Frees a string returned by `receive_envoy`.
///
/// # Safety
/// * `envoy` must be a pointer returned by `receive_envoy`.
/// * Must not be called more than once for the same pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_envoy(envoy: *mut c_char) {
    if envoy.is_null() {
        return;
    }
    // Retake ownership to drop it
    unsafe {
        let _ = CString::from_raw(envoy);
    }
}

/// Returns the version of the PraBorrow diplomacy crate.
#[unsafe(no_mangle)]
pub extern "C" fn praborrow_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diplomacy_flow() {
        // 1. Establish relations
        // Note: tests run in parallel, so REGISTRY might already be set.
        // We handle returns gracefully.
        let status = establish_relations();
        assert!(status == 0 || status == -1);

        // 2. Send envoy (C -> Rust)
        let msg = CString::new("Hello from C").unwrap();
        let send_status = unsafe { send_envoy(101, msg.as_ptr()) };
        assert_eq!(send_status, 0);

        // 3. Receive envoy (Rust -> C) - should contain the Ack
        let received_ptr = receive_envoy();
        assert!(!received_ptr.is_null());

        let received_str = unsafe { CStr::from_ptr(received_ptr).to_str().unwrap() };
        assert_eq!(received_str, "Ack: Hello from C");

        // 4. Free envoy
        unsafe { free_envoy(received_ptr) };
        
        // 5. Receive empty
        let empty = receive_envoy();
        assert!(empty.is_null());
    }
}
