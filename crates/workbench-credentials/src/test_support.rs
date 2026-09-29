//! Explicit, in-memory dependency injection for tests. Never select this on an OS error.

use crate::{codec, Result, Source, StoredCredential, Vault, VaultError};
use std::{collections::HashMap, fmt, sync::Mutex};
use zeroize::Zeroizing;

#[derive(Default)]
struct State {
    entries: HashMap<Source, Zeroizing<Vec<u8>>>,
    failure: Option<VaultError>,
}

#[derive(Default)]
pub struct MemoryVault {
    state: Mutex<State>,
}

impl MemoryVault {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a stable native failure without changing any saved fixture.
    pub fn set_failure(&self, failure: Option<VaultError>) -> Result<()> {
        self.state
            .lock()
            .map_err(|_| VaultError::UNAVAILABLE)?
            .failure = failure;
        Ok(())
    }
}

impl fmt::Debug for MemoryVault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MemoryVault { entries: [REDACTED] }")
    }
}

impl Vault for MemoryVault {
    fn load(&self, source: Source) -> Result<Option<StoredCredential>> {
        let state = self.state.lock().map_err(|_| VaultError::UNAVAILABLE)?;
        if let Some(error) = state.failure {
            return Err(error);
        }
        state
            .entries
            .get(&source)
            .map(|bytes| codec::decode(source, bytes))
            .transpose()
    }

    fn save(&self, source: Source, credential: &StoredCredential) -> Result<()> {
        let bytes = codec::encode(source, credential)?;
        let mut state = self.state.lock().map_err(|_| VaultError::UNAVAILABLE)?;
        if let Some(error) = state.failure {
            return Err(error);
        }
        state.entries.insert(source, bytes);
        Ok(())
    }

    fn delete(&self, source: Source) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| VaultError::UNAVAILABLE)?;
        if let Some(error) = state.failure {
            return Err(error);
        }
        state.entries.remove(&source);
        Ok(())
    }

    fn compare_exchange(
        &self,
        source: Source,
        expected: Option<[u8; 32]>,
        next: Option<&StoredCredential>,
    ) -> Result<()> {
        let next = next
            .map(|credential| codec::encode(source, credential))
            .transpose()?;
        let mut state = self.state.lock().map_err(|_| VaultError::UNAVAILABLE)?;
        if let Some(error) = state.failure {
            return Err(error);
        }
        let current = state
            .entries
            .get(&source)
            .map(|bytes| codec::decode(source, bytes))
            .transpose()?;
        if current.as_ref().map(StoredCredential::fingerprint) != expected {
            return Err(VaultError::CHANGED);
        }
        match next {
            Some(bytes) => {
                state.entries.insert(source, bytes);
            }
            None => {
                state.entries.remove(&source);
            }
        }
        Ok(())
    }
}
