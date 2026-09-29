use crate::{Result, Source, VaultError};
use sha2::{Digest, Sha256};
use std::{mem, ptr, slice};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_NO_TOKEN, HANDLE, WAIT_ABANDONED,
        WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Security::{GetLengthSid, GetTokenInformation, IsValidSid, TokenUser, TOKEN_QUERY, TOKEN_USER},
    System::Threading::{
        CreateMutexW, GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
        ReleaseMutex, WaitForSingleObject,
    },
};

const WAIT_MILLISECONDS: u32 = 5_000;
const MAX_TOKEN_BYTES: usize = 65_536;

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: This wrapper uniquely owns a non-null native handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

// Native HANDLE is not Send: mutex acquisition and release stay on this thread.
pub(crate) struct MutexGuard {
    handle: OwnedHandle,
}

impl Drop for MutexGuard {
    fn drop(&mut self) {
        // SAFETY: A successful wait granted this thread ownership. The owned
        // handle remains live until after this Drop body has completed.
        unsafe {
            ReleaseMutex(self.handle.0);
        }
    }
}

fn last_error() -> VaultError {
    // SAFETY: Called immediately after a failed Win32 API operation.
    match unsafe { GetLastError() } {
        ERROR_ACCESS_DENIED => VaultError::ACCESS_DENIED,
        _ => VaultError::UNAVAILABLE,
    }
}

fn effective_token() -> Result<OwnedHandle> {
    let mut token = ptr::null_mut();
    // SAFETY: Both thread/process pseudo-handles are valid for these calls, and
    // token is a live output parameter. Prefer an impersonating thread's user.
    unsafe {
        if OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) == 0 {
            if GetLastError() != ERROR_NO_TOKEN {
                return Err(last_error());
            }
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(last_error());
            }
        }
    }
    if token.is_null() {
        return Err(VaultError::UNAVAILABLE);
    }
    Ok(OwnedHandle(token))
}

fn user_digest() -> Result<[u8; 32]> {
    let token = effective_token()?;
    let mut required = 0u32;
    // SAFETY: A null buffer and zero length request the required size. The token
    // handle and output size remain live for this synchronous query.
    unsafe {
        GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut required);
    }
    if required as usize > MAX_TOKEN_BYTES || (required as usize) < mem::size_of::<TOKEN_USER>() {
        return Err(VaultError::UNAVAILABLE);
    }
    // usize alignment is sufficient for TOKEN_USER and its pointer-bearing fields.
    let mut buffer = vec![0usize; (required as usize).div_ceil(mem::size_of::<usize>())];
    // SAFETY: The aligned backing allocation covers the requested size and remains
    // live until all TOKEN_USER/SID references below have been consumed.
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        return Err(last_error());
    }
    // SAFETY: Successful TokenUser query initialized the aligned TOKEN_USER prefix.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    if user.User.Sid.is_null() {
        return Err(VaultError::UNAVAILABLE);
    }
    // SAFETY: The OS returned this SID as part of the still-owned token buffer.
    let valid = unsafe { IsValidSid(user.User.Sid) };
    if valid == 0 {
        return Err(VaultError::UNAVAILABLE);
    }
    // SAFETY: IsValidSid verified this SID, whose allocation remains live.
    let length = unsafe { GetLengthSid(user.User.Sid) } as usize;
    let start = buffer.as_ptr() as usize;
    let sid = user.User.Sid as usize;
    let end = start + buffer.len() * mem::size_of::<usize>();
    if sid < start || sid.checked_add(length).is_none_or(|sid_end| sid_end > end) {
        return Err(VaultError::UNAVAILABLE);
    }
    // SAFETY: The validated SID range lies inside the owned allocation above.
    let bytes = unsafe { slice::from_raw_parts(user.User.Sid.cast::<u8>(), length) };
    Ok(Sha256::digest(bytes).into())
}

pub(crate) fn acquire(namespace: &str, source: Source) -> Result<MutexGuard> {
    let mut digest = Sha256::new();
    digest.update(user_digest()?);
    digest.update(namespace.as_bytes());
    digest.update([0]);
    digest.update(source.as_str().as_bytes());
    let suffix: String = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    // Global coordinates this Windows user's app instances across logon sessions.
    // No account name, secret, or arbitrary caller target appears in the name.
    let name: Vec<u16> = format!("Global\\MangaMonitor.WorkbenchCredentials.{suffix}")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: Name is NUL-terminated and live. Null security attributes use the
    // current token's default DACL and prevent child-process handle inheritance.
    let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(last_error());
    }
    let owned = OwnedHandle(handle);
    // SAFETY: The owned mutex handle remains live for the bounded wait.
    match unsafe { WaitForSingleObject(owned.0, WAIT_MILLISECONDS) } {
        WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(MutexGuard { handle: owned }),
        WAIT_TIMEOUT => Err(VaultError::BUSY),
        WAIT_FAILED => Err(last_error()),
        _ => Err(VaultError::UNAVAILABLE),
    }
}
