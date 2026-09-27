use super::*;

const SAVED_PASSWORD: &str = "synthetic-saved-password-canary";

fn remembered_credential() -> StoredCredential {
    StoredCredential::new(
        "original-login",
        CredentialKind::SessionCookie,
        "old-session",
    )
    .unwrap()
    .with_login_password(SAVED_PASSWORD)
    .unwrap()
}

#[tokio::test]
async fn jm_password_requires_opt_in_and_never_appears_in_account_summary() {
    let root = TempDir::new().unwrap();
    let vault = SharedVault::default();
    let backend = FakeBackend::default();
    let service = service(&root, backend.clone(), vault.clone());
    let result = service
        .login_remembered(
            Source::Jm,
            "original-login".into(),
            SAVED_PASSWORD.into(),
            true,
            true,
        )
        .await
        .unwrap();
    assert!(result.remembered && result.remember_login);
    let saved = vault.load(Source::Jm).unwrap().unwrap();
    assert_eq!(saved.login_password(), Some(SAVED_PASSWORD));
    assert_eq!(saved.secret(), "server-session-original-login");
    let summary = serde_json::to_string(&result).unwrap();
    assert!(summary.contains("\"rememberLogin\":true"));
    assert!(!summary.contains(SAVED_PASSWORD));
    assert!(!summary.contains(saved.secret()));
    assert_eq!(backend.0.login_calls.load(Ordering::SeqCst), 1);

    // The previous API opts out even when replacing a remembered-login slot.
    service
        .login(
            Source::Jm,
            "original-login".into(),
            SAVED_PASSWORD.into(),
            true,
        )
        .await
        .unwrap();
    assert!(vault
        .load(Source::Jm)
        .unwrap()
        .unwrap()
        .login_password()
        .is_none());
    assert!(!service.accounts(false).await[0].remember_login);
}

#[tokio::test]
async fn invalid_remember_choices_are_rejected_before_login_or_persistence() {
    let root = TempDir::new().unwrap();
    let vault = SharedVault::default();
    let before = remembered_credential();
    vault.save(Source::Jm, &before).unwrap();
    let backend = FakeBackend::default();
    let service = service(&root, backend.clone(), vault.clone());
    for (source, remember) in [
        (Source::Jm, false),
        (Source::Pica, true),
        (Source::Pica, false),
    ] {
        assert_eq!(
            error(
                service
                    .login_remembered(source, "new".into(), "new-password".into(), remember, true)
                    .await
            ),
            "LOGIN_REMEMBER_INVALID"
        );
    }
    assert_eq!(backend.0.login_calls.load(Ordering::SeqCst), 0);
    assert!(vault.load(Source::Jm).unwrap().unwrap() == before);
    assert!(vault.load(Source::Pica).unwrap().is_none());
}

#[tokio::test]
async fn both_sources_restore_without_password_login_and_v1_choice_stays_false() {
    for include_password in [false, true] {
        let root = TempDir::new().unwrap();
        let vault = SharedVault::default();
        let jm = if include_password {
            remembered_credential()
        } else {
            credential(Source::Jm, "legacy-jm")
        };
        vault.save(Source::Jm, &jm).unwrap();
        vault
            .save(Source::Pica, &credential(Source::Pica, "legacy-pica"))
            .unwrap();
        let backend = FakeBackend::default();
        let service = service(&root, backend.clone(), vault.clone());
        let accounts = service.accounts(false).await;
        assert!(accounts
            .iter()
            .all(|item| item.state == AccountState::Connected && item.remembered));
        assert_eq!(accounts[0].remember_login, include_password);
        assert!(!accounts[1].remember_login);
        assert_eq!(backend.0.restore_calls.load(Ordering::SeqCst), 2);
        assert_eq!(backend.0.login_calls.load(Ordering::SeqCst), 0);
        assert!(!backend.0.restore_received_password.load(Ordering::SeqCst));
        assert!(vault.load(Source::Jm).unwrap().unwrap() == jm);
    }
}

#[tokio::test]
async fn expired_jm_session_uses_one_saved_login_then_next_start_restores_new_session() {
    for failure in ["AUTH_REQUIRED", "SESSION_EXPIRED"] {
        let root = TempDir::new().unwrap();
        let vault = SharedVault::default();
        vault.save(Source::Jm, &remembered_credential()).unwrap();
        let backend = FakeBackend::default();
        *backend.0.restore_failure.lock().unwrap() = Some(failure);
        *backend.0.login_secret.lock().unwrap() = Some("renewed-session".into());
        let app = service(&root, backend.clone(), vault.clone());
        let first = app.accounts(false).await;
        assert_eq!(first[0].state, AccountState::Connected);
        assert!(first[0].remember_login);
        let again = app.accounts(false).await;
        assert_eq!(again[0].session_id, first[0].session_id);
        assert_eq!(backend.0.restore_calls.load(Ordering::SeqCst), 1);
        assert_eq!(backend.0.login_calls.load(Ordering::SeqCst), 1);
        assert!(!backend.0.restore_received_password.load(Ordering::SeqCst));
        assert_eq!(
            *backend.0.last_login.lock().unwrap(),
            Some((Source::Jm, "original-login".into(), SAVED_PASSWORD.into()))
        );
        let saved = vault.load(Source::Jm).unwrap().unwrap();
        assert_eq!(saved.secret(), "renewed-session");
        assert_eq!(saved.account_name(), "original-login");
        assert_eq!(saved.login_password(), Some(SAVED_PASSWORD));

        let next_backend = FakeBackend::default();
        let reopened = service(&root, next_backend.clone(), vault);
        assert_eq!(
            reopened.accounts(false).await[0].state,
            AccountState::Connected
        );
        assert_eq!(next_backend.0.restore_calls.load(Ordering::SeqCst), 1);
        assert_eq!(next_backend.0.login_calls.load(Ordering::SeqCst), 0);
        assert!(!next_backend
            .0
            .restore_received_password
            .load(Ordering::SeqCst));
    }
}

#[tokio::test]
async fn restore_network_access_rate_and_format_failures_never_submit_saved_password() {
    for failure in [
        "SOURCE_TIMEOUT",
        "SOURCE_CONNECTION_FAILED",
        "SOURCE_REQUEST_FAILED",
        "SOURCE_ACCESS_DENIED",
        "SOURCE_RATE_LIMITED",
        "SOURCE_RESPONSE_INVALID",
        "SOURCE_API_REJECTED",
    ] {
        let root = TempDir::new().unwrap();
        let vault = SharedVault::default();
        let original = remembered_credential();
        vault.save(Source::Jm, &original).unwrap();
        let backend = FakeBackend::default();
        *backend.0.restore_failure.lock().unwrap() = Some(failure);
        let service = service(&root, backend.clone(), vault.clone());
        for _ in 0..2 {
            let state = service.accounts(false).await.remove(0);
            assert_eq!(state.state, AccountState::Unavailable);
            assert_eq!(state.error_code, Some(failure));
            assert!(state.remembered && state.remember_login);
        }
        assert_eq!(backend.0.restore_calls.load(Ordering::SeqCst), 1);
        assert_eq!(backend.0.login_calls.load(Ordering::SeqCst), 0);
        assert!(vault.load(Source::Jm).unwrap().unwrap() == original);
    }
}

#[tokio::test]
async fn expired_session_without_jm_opt_in_never_attempts_password_login() {
    let root = TempDir::new().unwrap();
    let vault = SharedVault::default();
    for source in [Source::Jm, Source::Pica] {
        vault
            .save(source, &credential(source, "session-only"))
            .unwrap();
    }
    let backend = FakeBackend::default();
    *backend.0.restore_failure.lock().unwrap() = Some("SESSION_EXPIRED");
    let service = service(&root, backend.clone(), vault);
    let accounts = service.accounts(false).await;
    assert!(accounts
        .iter()
        .all(|account| account.state == AccountState::Expired
            && account.remembered
            && !account.remember_login));
    assert_eq!(backend.0.login_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn failed_automatic_login_preserves_saved_choice_and_requires_explicit_retry() {
    for (failure, state) in [
        ("LOGIN_REJECTED", AccountState::Expired),
        ("SOURCE_TIMEOUT", AccountState::Unavailable),
    ] {
        let root = TempDir::new().unwrap();
        let vault = SharedVault::default();
        let original = remembered_credential();
        vault.save(Source::Jm, &original).unwrap();
        let backend = FakeBackend::default();
        *backend.0.restore_failure.lock().unwrap() = Some("SESSION_EXPIRED");
        *backend.0.login_failure.lock().unwrap() = Some(failure);
        let service = service(&root, backend.clone(), vault.clone());
        for _ in 0..2 {
            let account = service.accounts(false).await.remove(0);
            assert_eq!(account.state, state);
            assert_eq!(account.error_code, Some(failure));
            assert!(account.remembered && account.remember_login);
        }
        assert_eq!(backend.0.login_calls.load(Ordering::SeqCst), 1);
        assert!(vault.load(Source::Jm).unwrap().unwrap() == original);
        *backend.0.login_failure.lock().unwrap() = None;
        assert_eq!(
            service.accounts(true).await[0].state,
            AccountState::Connected
        );
        assert_eq!(backend.0.login_calls.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn unremembered_login_and_logout_delete_password_with_the_session() {
    let root = TempDir::new().unwrap();
    let vault = SharedVault::default();
    vault.save(Source::Jm, &remembered_credential()).unwrap();
    let service = service(&root, FakeBackend::default(), vault.clone());
    let temporary = service
        .login_remembered(
            Source::Jm,
            "temporary".into(),
            "synthetic".into(),
            false,
            false,
        )
        .await
        .unwrap();
    assert!(!temporary.remembered && !temporary.remember_login);
    assert!(vault.load(Source::Jm).unwrap().is_none());
    let remembered = service
        .login_remembered(
            Source::Jm,
            "remembered".into(),
            SAVED_PASSWORD.into(),
            true,
            true,
        )
        .await
        .unwrap();
    let logged_out = service
        .logout(Source::Jm, remembered.session_id.as_deref())
        .await
        .unwrap();
    assert!(!logged_out.remembered && !logged_out.remember_login);
    assert!(vault.load(Source::Jm).unwrap().is_none());
}

#[tokio::test]
async fn rejected_or_oversized_remembered_login_preserves_the_previous_record() {
    let root = TempDir::new().unwrap();
    let vault = SharedVault::default();
    let service = service(&root, FakeBackend::default(), vault.clone());
    let original = service
        .login_remembered(
            Source::Jm,
            "original".into(),
            SAVED_PASSWORD.into(),
            true,
            true,
        )
        .await
        .unwrap();
    let saved = vault.load(Source::Jm).unwrap().unwrap();
    for (password, expected) in [
        ("rejected".to_owned(), "LOGIN_REJECTED"),
        ("x".repeat(3000), "CREDENTIAL_TOO_LARGE"),
    ] {
        assert_eq!(
            error(
                service
                    .login_remembered(Source::Jm, "replacement".into(), password, true, true)
                    .await
            ),
            expected
        );
        assert!(vault.load(Source::Jm).unwrap().unwrap() == saved);
        assert_eq!(
            service.accounts(false).await[0].session_id,
            original.session_id
        );
    }
}

#[tokio::test]
async fn a_replaced_credential_during_restore_prevents_old_password_submission() {
    let root = TempDir::new().unwrap();
    let vault = SharedVault::default();
    vault.save(Source::Jm, &remembered_credential()).unwrap();
    let backend = FakeBackend::default();
    *backend.0.restore_failure.lock().unwrap() = Some("SESSION_EXPIRED");
    backend.0.block_restore.store(true, Ordering::SeqCst);
    let service = Arc::new(service(&root, backend.clone(), vault.clone()));
    let running = {
        let service = Arc::clone(&service);
        tokio::spawn(async move { service.accounts(false).await })
    };
    backend.0.restore_started.notified().await;
    let replacement = credential(Source::Jm, "other-process");
    vault.save(Source::Jm, &replacement).unwrap();
    backend.0.restore_release.notify_one();
    let account = running.await.unwrap().remove(0);
    assert_eq!(account.error_code, Some("SESSION_CHANGED"));
    assert!(!account.remember_login);
    assert_eq!(backend.0.login_calls.load(Ordering::SeqCst), 0);
    assert!(vault.load(Source::Jm).unwrap().unwrap() == replacement);
}

#[tokio::test]
async fn automatic_login_cas_cannot_overwrite_a_concurrent_password_change() {
    let root = TempDir::new().unwrap();
    let vault = RacingVault {
        inner: SharedVault::default(),
        replacement: Arc::new(Mutex::new(None)),
    };
    let original = remembered_credential();
    vault.save(Source::Jm, &original).unwrap();
    // Same name and session, changed password: full-blob generation must differ.
    let replacement = original
        .session_only()
        .with_login_password("new-private-password")
        .unwrap();
    *vault.replacement.lock().unwrap() = Some(replacement.clone());
    let backend = FakeBackend::default();
    *backend.0.restore_failure.lock().unwrap() = Some("SESSION_EXPIRED");
    let service = AccountService::new(backend.clone(), vault.clone(), root.path().to_path_buf());
    let account = service.accounts(false).await.remove(0);
    assert_eq!(account.state, AccountState::Unavailable);
    assert_eq!(account.error_code, Some("CREDENTIAL_CHANGED"));
    assert!(account.session_id.is_none());
    assert_eq!(backend.0.login_calls.load(Ordering::SeqCst), 1);
    assert!(vault.load(Source::Jm).unwrap().unwrap() == replacement);
}

#[tokio::test]
async fn simultaneous_account_reads_share_one_automatic_login() {
    let root = TempDir::new().unwrap();
    let vault = SharedVault::default();
    vault.save(Source::Jm, &remembered_credential()).unwrap();
    let backend = FakeBackend::default();
    *backend.0.restore_failure.lock().unwrap() = Some("SESSION_EXPIRED");
    backend.0.block_login.store(true, Ordering::SeqCst);
    let service = Arc::new(service(&root, backend.clone(), vault));
    let first = {
        let service = Arc::clone(&service);
        tokio::spawn(async move { service.accounts(false).await })
    };
    backend.0.login_started.notified().await;
    let second = {
        let service = Arc::clone(&service);
        tokio::spawn(async move { service.accounts(false).await })
    };
    backend.0.login_release.notify_one();
    let (first, second) = tokio::join!(first, second);
    assert_eq!(first.unwrap()[0].session_id, second.unwrap()[0].session_id);
    assert_eq!(backend.0.restore_calls.load(Ordering::SeqCst), 1);
    assert_eq!(backend.0.login_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_query_expiry_revokes_leases_without_automatic_login_or_query_replay() {
    let root = TempDir::new().unwrap();
    let backend = FakeBackend::default();
    let service = service(&root, backend.clone(), SharedVault::default());
    let account = service
        .login_remembered(
            Source::Jm,
            "fixture".into(),
            SAVED_PASSWORD.into(),
            true,
            true,
        )
        .await
        .unwrap();
    let session = account.session_id.unwrap();
    let lease = service.session_lease(Source::Jm, &session).await.unwrap();
    *backend.0.query_failure.lock().unwrap() = Some("SESSION_EXPIRED");
    assert_eq!(
        error(query(&service, Source::Jm, &session).await),
        "SESSION_EXPIRED"
    );
    assert_eq!(error(lease.require_current()), "SESSION_CHANGED");
    assert_eq!(
        service.accounts(false).await[0].state,
        AccountState::Expired
    );
    assert_eq!(backend.0.login_calls.load(Ordering::SeqCst), 1);
    assert_eq!(backend.0.restore_calls.load(Ordering::SeqCst), 0);
    assert_eq!(backend.0.query_calls.load(Ordering::SeqCst), 1);
}
