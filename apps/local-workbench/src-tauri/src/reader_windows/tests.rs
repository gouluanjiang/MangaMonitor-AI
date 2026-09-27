use super::*;
use workbench_storage::Source;

fn request(id: char) -> ReaderRequest {
    ReaderRequest::Library {
        root_id: "a".repeat(64),
        generation: 1,
        entry_id: id.to_string().repeat(64),
    }
}

#[test]
fn registry_deduplicates_only_exact_identity_and_shares_global_resource_limits() {
    let windows = ReaderWindows::default();
    let a = windows.reserve(request('b')).unwrap();
    let b = windows.reserve(request('c')).unwrap();
    assert!(a.created && b.created);
    assert_ne!(a.label, b.label);
    let again = windows.reserve(request('b')).unwrap();
    assert!(!again.created && !again.changed);
    assert_eq!(again.label, a.label);
    let scope_a = windows.scope(&a.label).unwrap();
    let scope_b = windows.scope(&b.label).unwrap();
    assert_ne!(scope_a.session_id(1), scope_b.session_id(1));
    assert_ne!(scope_a.session_id(1), windows.main.session_id(1));
    assert!(Arc::ptr_eq(&scope_a.pages, &scope_b.pages));
    assert!(Arc::ptr_eq(&scope_a.pages, &windows.main.pages));
    assert!(Arc::ptr_eq(&scope_a.progress, &scope_b.progress));
    let permit_a = scope_a.pages.clone().try_acquire_owned().unwrap();
    let permit_b = scope_b.pages.clone().try_acquire_owned().unwrap();
    assert!(windows.main.pages.try_acquire().is_err());
    drop(permit_a);
    assert!(windows.main.pages.try_acquire().is_ok());
    drop(permit_b);
    let before_a = scope_a.sequence.load(std::sync::atomic::Ordering::Acquire);
    let before_b = scope_b.sequence.load(std::sync::atomic::Ordering::Acquire);
    let mut newer = request('b');
    if let ReaderRequest::Library { generation, .. } = &mut newer {
        *generation = 2;
    }
    let changed = windows.reserve(newer.clone()).unwrap();
    assert!(!changed.created && changed.changed);
    assert_eq!(changed.label, a.label);
    assert!(scope_a.sequence.load(std::sync::atomic::Ordering::Acquire) > before_a);
    assert_eq!(
        scope_b.sequence.load(std::sync::atomic::Ordering::Acquire),
        before_b
    );
    assert!(windows.context(&a.label).unwrap().request == newer);
    assert!(windows.scope("reader-window-forged").is_err());
    assert!(windows.context("main").is_err());
}

#[test]
fn main_lifetime_waits_for_last_child_and_restore_revokes_logical_close() {
    let windows = ReaderWindows::default();
    let a = windows.reserve(request('b')).unwrap();
    let b = windows.reserve(request('c')).unwrap();
    assert!(!windows.ready(&a.label).unwrap());
    windows.context(&a.label).unwrap();
    assert!(windows.ready(&a.label).unwrap());
    assert!(!windows.close_main().unwrap());
    assert!(windows.scope("main").is_err());
    assert!(windows.scope(&b.label).is_ok());
    assert!(!windows.remove(&a.label).unwrap());
    windows.restore_main().unwrap();
    assert!(windows.scope("main").is_ok());
    assert!(!windows.remove(&b.label).unwrap());
    assert!(windows.close_main().unwrap());
    let windows = ReaderWindows::default();
    let child = windows.reserve(request('d')).unwrap();
    windows.close_main().unwrap();
    assert!(windows.remove(&child.label).unwrap());
    assert!(windows.scope(&child.label).is_err());
}

#[test]
fn source_identity_uses_source_id_not_account_token_and_rejects_renderer_paths() {
    let windows = ReaderWindows::default();
    let request = ReaderRequest::Source {
        source: Source::Jm,
        session_id: "account-first".into(),
        work_id: "12345".into(),
    };
    let first = windows.reserve(request.clone()).unwrap();
    let changed = ReaderRequest::Source {
        source: Source::Jm,
        session_id: "account-second".into(),
        work_id: "12345".into(),
    };
    let second = windows.reserve(changed).unwrap();
    assert_eq!(first.label, second.label);
    assert!(second.changed);
    let invalid = ReaderRequest::Source {
        source: Source::Jm,
        session_id: "account-second".into(),
        work_id: "https://example.invalid/image".into(),
    };
    assert!(windows.reserve(invalid).is_err());
    let invalid = ReaderRequest::Library {
        root_id: "C:\\library".into(),
        generation: 1,
        entry_id: "book.zip".into(),
    };
    assert!(windows.reserve(invalid).is_err());
}
