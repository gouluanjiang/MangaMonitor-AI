use crate::{codec, Result, Source, StoredCredential, Vault, VaultError, MAX_CREDENTIAL_BYTES};
use std::{fmt, ptr, slice};
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_ACCESS_DENIED, ERROR_NOT_FOUND, FILETIME},
    Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    },
};
use zeroize::Zeroize;

const NAMESPACE: &str = "MangaMonitor/WorkbenchPreview/v1/Accounts";

/// Fixed application slots in the current Windows user's local credential store.
/// No caller can supply a credential target or select a fallback backend.
#[derive(Clone, Default)]
pub struct WindowsVault {
    #[cfg(test)]
    test_namespace: Option<String>,
}

impl WindowsVault {
    pub fn new() -> Self {
        Self::default()
    }

    fn namespace(&self) -> &str {
        let namespace = NAMESPACE;
        #[cfg(test)]
        let namespace = self.test_namespace.as_deref().unwrap_or(namespace);
        namespace
    }

    fn lock(&self, source: Source) -> Result<crate::windows_lock::MutexGuard> {
        crate::windows_lock::acquire(self.namespace(), source)
    }

    fn target(&self, source: Source) -> Vec<u16> {
        let namespace = self.namespace();
        format!("{namespace}/{}", source.as_str())
            .encode_utf16()
            .chain(Some(0))
            .collect()
    }

    fn write_blob(&self, source: Source, blob: &mut [u8]) -> Result<()> {
        let _guard = self.lock(source)?;
        self.write_blob_unlocked(source, blob)
    }

    fn write_blob_unlocked(&self, source: Source, blob: &mut [u8]) -> Result<()> {
        if blob.is_empty() || blob.len() > MAX_CREDENTIAL_BYTES {
            return Err(VaultError::TOO_LARGE);
        }
        let mut target = self.target(source);
        // Private account names belong only in the protected blob, not metadata.
        let mut username: Vec<u16> = "MangaMonitor".encode_utf16().chain(Some(0)).collect();
        let credential = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            Comment: ptr::null_mut(),
            LastWritten: FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            },
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: ptr::null_mut(),
            TargetAlias: ptr::null_mut(),
            UserName: username.as_mut_ptr(),
        };
        // SAFETY: All pointers reference live, correctly terminated/bounded buffers
        // for this synchronous call. Windows copies the supplied credential.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            // SAFETY: Read the thread-local error immediately after the failed call.
            return Err(native_error(unsafe { GetLastError() }));
        }
        Ok(())
    }
}

impl fmt::Debug for WindowsVault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WindowsVault { application_slots: [REDACTED] }")
    }
}

fn native_error(error: u32) -> VaultError {
    match error {
        ERROR_ACCESS_DENIED => VaultError::ACCESS_DENIED,
        _ => VaultError::UNAVAILABLE,
    }
}

/// Owns the single allocation returned by CredReadW, including its secret blob.
struct CredentialBuffer(ptr::NonNull<CREDENTIALW>);

impl Drop for CredentialBuffer {
    fn drop(&mut self) {
        // SAFETY: Successful CredReadW returned this uniquely owned allocation.
        // Its blob remains valid until CredFree; no borrowed slice survives drop.
        unsafe {
            let credential = self.0.as_ref();
            let size = credential.CredentialBlobSize as usize;
            if !credential.CredentialBlob.is_null() && size <= MAX_CREDENTIAL_BYTES {
                slice::from_raw_parts_mut(credential.CredentialBlob, size).zeroize();
            }
            CredFree(self.0.as_ptr().cast());
        }
    }
}

impl WindowsVault {
    fn load_unlocked(&self, source: Source) -> Result<Option<StoredCredential>> {
        let target = self.target(source);
        let mut pointer = ptr::null_mut();
        // SAFETY: target is a live, NUL-terminated UTF-16 name; pointer is an
        // initialized output parameter owned exclusively by this synchronous call.
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut pointer) } == 0 {
            // SAFETY: Read the thread-local error immediately after the failed call.
            let error = unsafe { GetLastError() };
            return if error == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err(native_error(error))
            };
        }
        let owned = CredentialBuffer(ptr::NonNull::new(pointer).ok_or(VaultError::CORRUPT)?);
        // SAFETY: CredReadW succeeded and the guard owns its non-null allocation.
        let native = unsafe { owned.0.as_ref() };
        if native.Type != CRED_TYPE_GENERIC
            || native.CredentialBlobSize == 0
            || native.CredentialBlob.is_null()
        {
            return Err(VaultError::CORRUPT);
        }
        let size = native.CredentialBlobSize as usize;
        if size > MAX_CREDENTIAL_BYTES {
            return Err(VaultError::TOO_LARGE);
        }
        // SAFETY: The API returns a blob of CredentialBlobSize bytes in the same
        // live allocation. The slice is bounded and does not outlive its guard.
        let bytes = unsafe { slice::from_raw_parts(native.CredentialBlob, size) };
        codec::decode(source, bytes).map(Some)
    }

    fn delete_unlocked(&self, source: Source) -> Result<()> {
        let target = self.target(source);
        // SAFETY: The fixed target is NUL-terminated and remains live for the call.
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
            // SAFETY: Read the thread-local error immediately after the failed call.
            let error = unsafe { GetLastError() };
            if error != ERROR_NOT_FOUND {
                return Err(native_error(error));
            }
        }
        Ok(())
    }
}

impl Vault for WindowsVault {
    fn load(&self, source: Source) -> Result<Option<StoredCredential>> {
        let _guard = self.lock(source)?;
        self.load_unlocked(source)
    }

    fn save(&self, source: Source, credential: &StoredCredential) -> Result<()> {
        let mut bytes = codec::encode(source, credential)?;
        self.write_blob(source, &mut bytes)
    }

    fn delete(&self, source: Source) -> Result<()> {
        let _guard = self.lock(source)?;
        self.delete_unlocked(source)
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
        let _guard = self.lock(source)?;
        let current = self.load_unlocked(source)?;
        if current.as_ref().map(StoredCredential::fingerprint) != expected {
            return Err(VaultError::CHANGED);
        }
        match next {
            Some(mut bytes) => self.write_blob_unlocked(source, &mut bytes),
            None => self.delete_unlocked(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CredentialKind;
    use rand::RngCore;

    #[test]
    fn native_errors_are_stable_and_redacted() {
        assert_eq!(native_error(ERROR_ACCESS_DENIED), VaultError::ACCESS_DENIED);
        assert_eq!(native_error(1312), VaultError::UNAVAILABLE);
        assert_eq!(native_error(u32::MAX), VaultError::UNAVAILABLE);
    }

    #[test]
    #[ignore = "Requires a disposable Windows GitHub Actions runner"]
    fn windows_credential_manager_roundtrip_uses_only_random_ci_slots() {
        // Explicit opt-in and a disposable CI environment are both mandatory.
        assert_eq!(std::env::var("CI").as_deref(), Ok("true"));
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        if let Ok(suffix) = std::env::var("WORKBENCH_CREDENTIAL_CI_CHILD") {
            // Only this test binary accepts the parent's random CI slot suffix.
            assert_eq!(suffix.len(), 32);
            assert!(suffix.bytes().all(|byte| byte.is_ascii_hexdigit()));
            let vault = WindowsVault {
                test_namespace: Some(format!("MangaMonitor/WorkbenchPreview/CI/{suffix}")),
            };
            assert_eq!(vault.delete(Source::Jm), Err(VaultError::BUSY));
            return;
        }
        let mut random = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut random);
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let vault = WindowsVault {
            test_namespace: Some(format!("MangaMonitor/WorkbenchPreview/CI/{suffix}")),
        };
        struct Cleanup(WindowsVault);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = self.0.delete(Source::Jm);
                let _ = self.0.delete(Source::Pica);
            }
        }
        let _cleanup = Cleanup(vault.clone());
        assert_eq!(vault.load(Source::Jm).unwrap(), None);
        vault.delete(Source::Jm).unwrap();
        let conditional =
            StoredCredential::new("ci-cas", CredentialKind::SessionCookie, "sid=cas").unwrap();
        vault
            .compare_exchange(Source::Jm, None, Some(&conditional))
            .unwrap();
        assert_eq!(
            vault.compare_exchange(Source::Jm, None, None),
            Err(VaultError::CHANGED)
        );
        // A child process must observe the same user's mutex, then time out. It
        // cannot delete even this CI fixture while the parent owns the operation.
        {
            let _guard = vault.lock(Source::Jm).unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["windows::tests::windows_credential_manager_roundtrip_uses_only_random_ci_slots", "--exact", "--ignored"])
                .env("WORKBENCH_CREDENTIAL_CI_CHILD", &suffix)
                .status().unwrap();
            assert!(status.success());
        }
        assert_eq!(vault.load(Source::Jm).unwrap(), Some(conditional.clone()));
        vault
            .compare_exchange(Source::Jm, Some(conditional.fingerprint()), None)
            .unwrap();
        assert_eq!(vault.load(Source::Jm).unwrap(), None);
        let first =
            StoredCredential::new("ci-account", CredentialKind::SessionCookie, "sid=one").unwrap();
        let replacement =
            StoredCredential::new("ci-other", CredentialKind::SessionCookie, "sid=two").unwrap();
        let pica =
            StoredCredential::new("ci-pica", CredentialKind::SessionToken, "ci-token").unwrap();
        vault.save(Source::Jm, &first).unwrap();
        assert_eq!(vault.load(Source::Jm).unwrap(), Some(first));
        assert_eq!(vault.load(Source::Pica).unwrap(), None);
        vault.save(Source::Pica, &pica).unwrap();
        vault.save(Source::Jm, &replacement).unwrap();
        let reopened = vault.clone();
        assert_eq!(reopened.load(Source::Jm).unwrap(), Some(replacement));
        assert_eq!(reopened.load(Source::Pica).unwrap(), Some(pica.clone()));
        vault
            .write_blob(Source::Jm, &mut b"broken-json".to_vec())
            .unwrap();
        assert_eq!(vault.load(Source::Jm), Err(VaultError::CORRUPT));
        assert_eq!(
            vault.compare_exchange(Source::Jm, None, None),
            Err(VaultError::CORRUPT)
        );
        // A failed read preserves the original invalid record and the other source.
        assert_eq!(reopened.load(Source::Jm), Err(VaultError::CORRUPT));
        assert_eq!(vault.load(Source::Pica).unwrap(), Some(pica));
        vault.delete(Source::Jm).unwrap();
        vault.delete(Source::Jm).unwrap();
        assert_eq!(vault.load(Source::Jm).unwrap(), None);
        vault.delete(Source::Pica).unwrap();
        assert_eq!(vault.load(Source::Pica).unwrap(), None);
    }
}
