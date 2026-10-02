//! Test-only bounded write failures for one exact output identity. This module
//! and the call in materialize.rs do not exist in release/application builds.
use std::{
    cell::RefCell,
    fs::File,
    io::{self, Write},
    path::Path,
};

struct Fault {
    identity: String,
    prefix: usize,
    code: i32,
}
thread_local! { static FAULT: RefCell<Option<Fault>> = const { RefCell::new(None) }; }
pub(crate) struct FaultGuard;
impl Drop for FaultGuard {
    fn drop(&mut self) {
        FAULT.with(|v| *v.borrow_mut() = None);
    }
}

pub(crate) fn arm(case: &Path, identity: String, prefix: usize, code: i32) -> FaultGuard {
    crate::windows_validation_support::verify_case(case);
    assert!(!identity.is_empty() && prefix <= 64 && matches!(code, 5 | 112));
    FAULT.with(|v| {
        assert!(v.borrow().is_none());
        *v.borrow_mut() = Some(Fault {
            identity,
            prefix,
            code,
        });
    });
    FaultGuard
}

pub(crate) fn write_all(target: &mut File, bytes: &[u8]) -> io::Result<()> {
    if FAULT.with(|v| v.borrow().is_none()) {
        return target.write_all(bytes);
    }
    let identity =
        crate::fs::file_key(target).map_err(|_| io::Error::other("test target unavailable"))?;
    let fault = FAULT.with(|v| {
        let mut value = v.borrow_mut();
        if value.as_ref().is_some_and(|f| f.identity == identity) {
            value.take()
        } else {
            None
        }
    });
    if let Some(fault) = fault {
        target.write_all(&bytes[..fault.prefix.min(bytes.len())])?;
        return Err(io::Error::from_raw_os_error(fault.code));
    }
    target.write_all(bytes)
}
