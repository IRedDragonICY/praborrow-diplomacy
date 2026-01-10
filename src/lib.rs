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
//! void establish_relations(void);
//!
//! // Alternative name for establish_relations
//! void init_ffi(void);
//!
//! // Send an envoy (notification) to foreign jurisdiction
//! void send_envoy(uint32_t id);
//!
//! #endif // PRABORROW_H
//! ```
//!
//! # Example C Usage
//!
//! ```c
//! #include "praborrow.h"
//!
//! int main() {
//!     // Initialize the FFI layer
//!     establish_relations();
//!     
//!     // Send notifications
//!     send_envoy(42);
//!     
//!     return 0;
//! }
//! ```

/// Establishes diplomatic relations with the PraBorrow runtime.
///
/// This function initializes the FFI layer and should be called before
/// any other PraBorrow FFI functions.
///
/// # Safety
///
/// This function is safe to call from C. It performs no memory allocation
/// and has no preconditions.
#[unsafe(no_mangle)]
pub extern "C" fn establish_relations() {
    tracing::info!(
        event = "ffi_init",
        version = env!("CARGO_PKG_VERSION"),
        "Diplomatic relations established"
    );
}

/// Alternative name for `establish_relations`.
///
/// Provided for API ergonomics in C code that prefers the `init_ffi` naming convention.
pub use establish_relations as init_ffi;

/// Sends an envoy (notification) to foreign jurisdiction.
///
/// # Arguments
///
/// * `id` - The unique identifier of the envoy/notification
///
/// # Safety
///
/// This function is safe to call from C. The `id` is passed by value.
#[unsafe(no_mangle)]
pub extern "C" fn send_envoy(id: u32) {
    tracing::debug!(
        event = "envoy_sent",
        envoy_id = id,
        "Envoy sent to foreign jurisdiction"
    );
}

/// Returns the version of the PraBorrow diplomacy crate.
///
/// # Returns
///
/// A null-terminated C string containing the version. The string is statically
/// allocated and should not be freed.
///
/// # Safety
///
/// The returned pointer is valid for the lifetime of the program.
#[unsafe(no_mangle)]
pub extern "C" fn praborrow_version() -> *const core::ffi::c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const core::ffi::c_char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_establish_relations() {
        // Should not panic
        establish_relations();
    }

    #[test]
    fn test_send_envoy() {
        // Should not panic
        send_envoy(42);
    }

    #[test]
    fn test_init_ffi_alias() {
        // init_ffi should be the same as establish_relations
        init_ffi();
    }
}
