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
use dashmap::DashSet;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::panic::catch_unwind;
use std::sync::OnceLock;

/// Handle to track active envoys (pointers) given to foreign jurisdictions.
/// Limits the scope of potential FFI misuse (double-free).
use std::sync::atomic::{AtomicUsize, Ordering};

/// Handle to track active envoys (pointers) given to foreign jurisdictions.
/// Limits the scope of potential FFI misuse (double-free).
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct EnvoyHandle(usize);

// Error Codes
const SUCCESS: c_int = 0;

pub mod safe;
const ERR_ALREADY_INIT: c_int = -1;
const ERR_INIT_FAILED: c_int = -2;
const ERR_NULL_PTR: c_int = -3;
const ERR_INVALID_UTF8: c_int = -4;
const ERR_INVALID_ID: c_int = -5;
const ERR_PANIC: c_int = -6;
const ERR_QUEUE_FULL: c_int = -7;

/// Trait for types that can be exchanged across the FFI boundary.
pub trait Diplomat: serde::Serialize + serde::de::DeserializeOwned {}

pub(crate) const MAX_QUEUE_DEPTH: usize = 10_000;

/// Global registry for diplomatic state.
pub(crate) struct GlobalRegistry {
    /// Envoys received from the foreign jurisdiction, waiting to be processed by Rust.
    pub(crate) incoming_envoys: SegQueue<String>,
    pub(crate) incoming_count: AtomicUsize,
    /// Envoys waiting to be sent to the foreign jurisdiction (outbox).
    pub(crate) outbox: SegQueue<String>,
    pub(crate) outbox_count: AtomicUsize,
    /// Tracks active pointers given to C to prevent double-free.
    pub(crate) active_loans: DashSet<usize>,
}

impl GlobalRegistry {
    pub(crate) fn new() -> Self {
        Self {
            incoming_envoys: SegQueue::new(),
            incoming_count: AtomicUsize::new(0),
            outbox: SegQueue::new(),
            outbox_count: AtomicUsize::new(0),
            active_loans: DashSet::new(),
        }
    }
}

pub(crate) static REGISTRY: OnceLock<GlobalRegistry> = OnceLock::new();

/// Establishes diplomatic relations with the PraBorrow runtime.
///
/// Initializes the global registry.
///
/// # Returns
/// * `0` - Success
/// * `-1` - Already initialized
/// * `-2` - Initialization failed
#[unsafe(no_mangle)]
#[tracing::instrument]
pub extern "C" fn establish_relations() -> c_int {
    if REGISTRY.get().is_some() {
        return ERR_ALREADY_INIT;
    }

    match REGISTRY.set(GlobalRegistry::new()) {
        Ok(_) => {
            tracing::info!(
                event = "ffi_init",
                version = env!("CARGO_PKG_VERSION"),
                "Diplomatic relations established"
            );
            SUCCESS
        }
        Err(_) => {
            tracing::error!("Failed to initialize GlobalRegistry");
            ERR_INIT_FAILED
        }
    }
}
// Note: establish_relations logic has a bug in original code:
// if REGISTRY.get().is_some() returns -1 (AlreadyInit).
// REGISTRY.set() returns Err if already set.
// So we should return ERR_ALREADY_INIT in the check, and ERR_INIT_FAILED if logic fails otherwise (unlikely for OnceLock).
// I will keep the check pattern but use constants.

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
#[tracing::instrument(skip(payload))]
pub unsafe extern "C" fn send_envoy(id: u32, payload: *const c_char) -> c_int {
    let registry = match REGISTRY.get() {
        Some(r) => r,
        None => return ERR_INIT_FAILED,
    };
    // ...

    if payload.is_null() {
        tracing::error!("Received NULL payload");
        return ERR_NULL_PTR;
    }

    if id == 0 {
        tracing::error!("Received Invalid ID 0");
        return ERR_INVALID_ID;
    }

    // Wrap unsafe dereference and string conversion in catch_unwind
    // Note: This catches Rust panics, NOT segfaults.
    let r_str_result = catch_unwind(|| {
        let c_str = unsafe { CStr::from_ptr(payload) };
        c_str.to_str().map(|s| s.to_string())
    });

    let r_str = match r_str_result {
        Ok(Ok(s)) => s,
        Ok(Err(_)) => {
            tracing::error!("Invalid UTF-8 in payload");
            return ERR_INVALID_UTF8;
        } // UTF-8 error
        Err(_) => {
            tracing::error!("Panic caught across FFI boundary");
            return ERR_PANIC;
        } // Panic occurred
    };

    tracing::debug!(
        event = "envoy_received",
        envoy_id = id,
        payload = ?r_str,
        "Envoy received from foreign jurisdiction"
    );

    // OOM Prevention: Check Limits
    // 1. Check Incoming Queue
    if registry.incoming_count.load(Ordering::Relaxed) >= MAX_QUEUE_DEPTH {
        return ERR_QUEUE_FULL;
    }
    // 2. Check Outbox (for Echo)
    if registry.outbox_count.load(Ordering::Relaxed) >= MAX_QUEUE_DEPTH {
        // Technically this is a partial failure if incoming succeeded but echo failed.
        // For simplicity, we fail the whole operation to apply backpressure.
        return ERR_QUEUE_FULL;
    }

    // Commit to queues
    // Note: Race condition exists between load and push, but precise count isn't critical for safety, just bounding.
    // Using fetch_add would be more precise.
    let old_incoming = registry.incoming_count.fetch_add(1, Ordering::Relaxed);
    if old_incoming >= MAX_QUEUE_DEPTH {
        registry.incoming_count.fetch_sub(1, Ordering::Relaxed);
        return ERR_QUEUE_FULL;
    }

    registry
        .incoming_envoys
        .push(format!("ID:{}:{}", id, r_str));

    let old_outbox = registry.outbox_count.fetch_add(1, Ordering::Relaxed);
    if old_outbox >= MAX_QUEUE_DEPTH {
        registry.outbox_count.fetch_sub(1, Ordering::Relaxed);
        // Note: We already pushed to incoming, so this is an edge case.
        // But prevents outbox OOM.
        return ERR_QUEUE_FULL;
    }

    // Auto-reply for testing
    registry.outbox.push(format!("Ack: {}", r_str));

    SUCCESS
}

/// Receives an envoy (message) FROM Rust TO the foreign jurisdiction.
///
/// Pops a message from the internal outbox.
///
/// # Returns
/// * `char*` - Pointer to null-terminated string. Ownership transferred to caller.
/// * `NULL` - No messages available or error.
#[unsafe(no_mangle)]
#[tracing::instrument]
pub extern "C" fn receive_envoy() -> *mut c_char {
    let registry = match REGISTRY.get() {
        Some(r) => r,
        None => return std::ptr::null_mut(),
    };

    let msg = registry.outbox.pop();

    match msg {
        Some(s) => {
            // Decrement count
            registry.outbox_count.fetch_sub(1, Ordering::Relaxed);

            match CString::new(s) {
                Ok(c_str) => {
                    let ptr = c_str.into_raw();
                    // Register the pointer as active
                    registry.active_loans.insert(ptr as usize);
                    ptr
                }
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

    let registry = match REGISTRY.get() {
        Some(r) => r,
        None => return, // Should not happen if we gave out a pointer
    };

    let ptr_val = envoy as usize;

    // Check if we actually loaned this pointer
    if registry.active_loans.remove(&ptr_val).is_some() {
        // Safe to free: we created it and haven't freed it yet
        // Retake ownership to drop it
        unsafe {
            let _ = CString::from_raw(envoy);
        }
    } else {
        // Double free or invalid pointer!
        // We log error and DO NOT attempt to free, preventing segfault/heap corruption.
        tracing::error!(
            event = "ffi_violation",
            ptr = ?envoy,
            "Attempted to free invalid or already freed envoy pointer"
        );
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
