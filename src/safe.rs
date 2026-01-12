use crate::{GlobalRegistry, REGISTRY};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum DiplomacyError {
    #[error("Already initialized")]
    AlreadyInitialized,
    #[error("Initialization failed")]
    InitFailed,
    #[error("Registry not initialized")]
    NotInitialized,
}

/// A safe wrapper for the Diplomatic Relations FFI.
///
/// This struct ensures type safety and error handling while interfacing
/// with the underlying foreign function interface components.
pub struct Diplomat;

impl Diplomat {
    /// Initializes the diplomatic registry.
    ///
    /// This effectively calls the internal `establish_relations` logic safely.
    pub fn init() -> Result<(), DiplomacyError> {
        if REGISTRY.get().is_some() {
            return Err(DiplomacyError::AlreadyInitialized);
        }
        match REGISTRY.set(GlobalRegistry::new()) {
            Ok(_) => {
                tracing::info!("Diplomatic relations established via Safe Wrapper");
                Ok(())
            }
            Err(_) => Err(DiplomacyError::InitFailed),
        }
    }

    /// Sends a message TO the foreign jurisdiction (C world).
    ///
    /// Pushes the message to the internal outbox, formatted as "{id}:{payload}".
    /// C-side `receive_envoy` will pop this message.
    pub fn send(id: u32, payload: &str) -> Result<(), DiplomacyError> {
        let registry = REGISTRY.get().ok_or(DiplomacyError::NotInitialized)?;
        registry.outbox.push(format!("{}:{}", id, payload));
        Ok(())
    }

    /// Receives a message FROM the foreign jurisdiction (C world).
    ///
    /// Pops from the internal incoming queue, which is populated by C-side `send_envoy`.
    pub fn receive() -> Option<String> {
        let registry = REGISTRY.get()?;
        registry.incoming_envoys.pop()
    }
}
