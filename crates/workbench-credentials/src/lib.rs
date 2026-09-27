//! Application-owned session credentials and optional JM login, with no filesystem
//! or browser fallback.
//!
//! Only the native application should use this API. Stored credentials deliberately
//! do not implement serde traits and must never be returned to a renderer command.

mod codec;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_lock;
#[cfg(windows)]
pub use windows::WindowsVault;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use zeroize::Zeroizing;

/// Windows CRED_MAX_CREDENTIAL_BLOB_SIZE, including our entire encoded envelope.
pub const MAX_CREDENTIAL_BYTES: usize = 5 * 512;
pub const MAX_ACCOUNT_NAME_UTF16: usize = 513;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Source {
    #[serde(rename = "JM")]
    Jm,
    #[serde(rename = "Pica")]
    Pica,
}

impl Source {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Jm => "JM",
            Self::Pica => "Pica",
        }
    }

    const fn credential_kind(self) -> CredentialKind {
        match self {
            Self::Jm => CredentialKind::SessionCookie,
            Self::Pica => CredentialKind::SessionToken,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialKind {
    SessionToken,
    SessionCookie,
}

impl CredentialKind {
    const fn source(self) -> Source {
        match self {
            Self::SessionCookie => Source::Jm,
            Self::SessionToken => Source::Pica,
        }
    }
}

/// An explicitly owned session secret, optionally carrying an authorized JM login.
#[derive(Clone, Eq, PartialEq)]
pub struct StoredCredential {
    account_name: Zeroizing<String>,
    kind: CredentialKind,
    secret: Zeroizing<String>,
    login_password: Option<Zeroizing<String>>,
}

impl StoredCredential {
    pub fn new(
        account_name: impl Into<String>,
        kind: CredentialKind,
        secret: impl Into<String>,
    ) -> Result<Self> {
        let credential = Self {
            account_name: Zeroizing::new(account_name.into()),
            kind,
            secret: Zeroizing::new(secret.into()),
            login_password: None,
        };
        // Check the actual serialized byte budget, including Unicode and escapes.
        codec::encode(kind.source(), &credential)?;
        Ok(credential)
    }

    /// Attach a native-only JM password to this same session record. The caller
    /// must obtain explicit permission before persisting it; Pica is session-only.
    pub fn with_login_password(mut self, password: impl Into<String>) -> Result<Self> {
        self.login_password = Some(Zeroizing::new(password.into()));
        codec::encode(self.kind.source(), &self)?;
        Ok(self)
    }

    /// Borrow only for native JM authentication; never log or return to a UI.
    pub fn login_password(&self) -> Option<&str> {
        self.login_password
            .as_ref()
            .map(|password| password.as_str())
    }

    /// Preserve the session/account exactly, without copying the optional password.
    pub fn session_only(&self) -> Self {
        Self {
            account_name: self.account_name.clone(),
            kind: self.kind,
            secret: self.secret.clone(),
            login_password: None,
        }
    }

    pub fn account_name(&self) -> &str {
        self.account_name.as_str()
    }

    pub const fn kind(&self) -> CredentialKind {
        self.kind
    }

    /// Borrow only inside native authentication code; never log or send to a UI.
    pub fn secret(&self) -> &str {
        self.secret.as_str()
    }

    /// Native-only generation comparison; never send the fingerprint to a UI.
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        if self.login_password.is_some() {
            // Keep the v1 session-only fingerprint unchanged. A separate domain
            // and explicit lengths bind password presence and all v2 fields.
            hash.update(b"MangaMonitor/JM-login-credential/v2\0");
            hash.update([1]);
        }
        hash.update((self.account_name().len() as u64).to_le_bytes());
        hash.update(self.account_name().as_bytes());
        if self.login_password.is_some() {
            hash.update((self.secret().len() as u64).to_le_bytes());
        }
        hash.update(self.secret().as_bytes());
        if let Some(password) = self.login_password() {
            hash.update((password.len() as u64).to_le_bytes());
            hash.update(password.as_bytes());
        }
        hash.finalize().into()
    }

    fn validate_for(&self, source: Source) -> Result<()> {
        if self.kind != source.credential_kind()
            || self.account_name.trim().is_empty()
            || self.account_name.chars().any(char::is_control)
            || self.account_name.encode_utf16().count() > MAX_ACCOUNT_NAME_UTF16
            || self.secret.trim().is_empty()
            || self.secret.chars().any(char::is_control)
            || self.login_password().is_some_and(|password| {
                source != Source::Jm
                    || password.is_empty()
                    || password.chars().any(char::is_control)
            })
        {
            return Err(VaultError::INVALID_CREDENTIAL);
        }
        Ok(())
    }
}

impl fmt::Debug for StoredCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredCredential")
            .field("account_name", &"[REDACTED]")
            .field("kind", &self.kind)
            .field("secret", &"[REDACTED]")
            .field("login_password", &"[REDACTED]")
            .finish()
    }
}

/// Stable errors never contain an account name, target, native message, or secret.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct VaultError {
    pub code: &'static str,
}

impl VaultError {
    pub const INVALID_CREDENTIAL: Self = Self {
        code: "CREDENTIAL_INVALID",
    };
    pub const TOO_LARGE: Self = Self {
        code: "CREDENTIAL_TOO_LARGE",
    };
    pub const CORRUPT: Self = Self {
        code: "CREDENTIAL_CORRUPT",
    };
    pub const UNSUPPORTED_SCHEMA: Self = Self {
        code: "CREDENTIAL_SCHEMA_UNSUPPORTED",
    };
    pub const ACCESS_DENIED: Self = Self {
        code: "VAULT_ACCESS_DENIED",
    };
    pub const UNAVAILABLE: Self = Self {
        code: "VAULT_UNAVAILABLE",
    };
    pub const BUSY: Self = Self { code: "VAULT_BUSY" };
    pub const CHANGED: Self = Self {
        code: "CREDENTIAL_CHANGED",
    };
}

impl fmt::Display for VaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for VaultError {}

pub type Result<T> = std::result::Result<T, VaultError>;

/// The caller authorizes persistence of a session and any optional JM password.
/// Each source has one slot; save replaces it, and deleting a missing slot succeeds.
/// A service coordinating concurrent account operations must order its own writes.
pub trait Vault: Send + Sync {
    fn load(&self, source: Source) -> Result<Option<StoredCredential>>;
    fn save(&self, source: Source, credential: &StoredCredential) -> Result<()>;
    fn delete(&self, source: Source) -> Result<()>;

    /// Atomically replace/delete only the observed generation; None means absent.
    /// Implementations must share the same lock with all ordinary writes/deletes.
    /// There is intentionally no non-atomic default implementation.
    fn compare_exchange(
        &self,
        source: Source,
        expected: Option<[u8; 32]>,
        next: Option<&StoredCredential>,
    ) -> Result<()>;
}

#[cfg(test)]
mod tests;
