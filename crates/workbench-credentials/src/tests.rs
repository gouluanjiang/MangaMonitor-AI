use crate::{codec, test_support::MemoryVault, *};

fn cookie(account: &str, secret: &str) -> StoredCredential {
    StoredCredential::new(account, CredentialKind::SessionCookie, secret).unwrap()
}

#[test]
fn source_wire_labels_are_exact() {
    assert_eq!(serde_json::to_string(&Source::Jm).unwrap(), "\"JM\"");
    assert_eq!(serde_json::to_string(&Source::Pica).unwrap(), "\"Pica\"");
    assert_eq!(
        serde_json::from_str::<Source>("\"JM\"").unwrap(),
        Source::Jm
    );
    assert!(serde_json::from_str::<Source>("\"jm\"").is_err());
    assert!(serde_json::from_str::<Source>("\"pica\"").is_err());
}

#[test]
fn diagnostics_hide_account_and_secret() {
    let credential = cookie("private-mail@example.test", "private-session-fixture")
        .with_login_password("private-password-fixture")
        .unwrap();
    let cloned = credential.clone();
    assert_eq!(credential, cloned);
    let debug = format!("{credential:?}");
    assert!(!debug.contains(credential.account_name()));
    assert!(!debug.contains(credential.secret()));
    assert!(!debug.contains(credential.login_password().unwrap()));
    assert!(debug.contains("[REDACTED]"));
    assert_eq!(VaultError::UNAVAILABLE.to_string(), "VAULT_UNAVAILABLE");
    assert_eq!(
        serde_json::to_string(&VaultError::UNAVAILABLE).unwrap(),
        "{\"code\":\"VAULT_UNAVAILABLE\"}"
    );
    let vault = MemoryVault::new();
    vault.save(Source::Jm, &credential).unwrap();
    let vault_debug = format!("{vault:?}");
    assert!(!vault_debug.contains(credential.account_name()));
    assert!(!vault_debug.contains(credential.secret()));
    assert!(!vault_debug.contains(credential.login_password().unwrap()));
}

#[test]
fn rejects_empty_fields_and_header_injection_without_rewriting_valid_input() {
    for invalid in ["", "  ", "\t", "a\r\nb", "a\0b", "a\u{85}b"] {
        assert_eq!(
            StoredCredential::new(invalid, CredentialKind::SessionCookie, "sid=valid"),
            Err(VaultError::INVALID_CREDENTIAL)
        );
        assert_eq!(
            StoredCredential::new("account", CredentialKind::SessionCookie, invalid),
            Err(VaultError::INVALID_CREDENTIAL)
        );
    }
    let credential = cookie(" user@example.test ", " sid=preserved ");
    assert_eq!(credential.account_name(), " user@example.test ");
    assert_eq!(credential.secret(), " sid=preserved ");
    assert!(StoredCredential::new("a".repeat(513), CredentialKind::SessionToken, "token").is_ok());
    assert_eq!(
        StoredCredential::new("a".repeat(514), CredentialKind::SessionToken, "token"),
        Err(VaultError::INVALID_CREDENTIAL)
    );
    assert!(StoredCredential::new("😀".repeat(256), CredentialKind::SessionToken, "token").is_ok());
    assert_eq!(
        StoredCredential::new("😀".repeat(257), CredentialKind::SessionToken, "token"),
        Err(VaultError::INVALID_CREDENTIAL)
    );
}

#[test]
fn roundtrip_overwrite_and_delete_keep_source_slots_independent() {
    let vault = MemoryVault::new();
    assert_eq!(vault.load(Source::Jm).unwrap(), None);
    vault.delete(Source::Jm).unwrap();
    let jm = cookie("first", "sid=one");
    let pica = StoredCredential::new("pica", CredentialKind::SessionToken, "pica-token").unwrap();
    vault.save(Source::Jm, &jm).unwrap();
    vault.save(Source::Pica, &pica).unwrap();
    assert_eq!(vault.load(Source::Jm).unwrap(), Some(jm));
    let replacement = cookie("second", "sid=two");
    vault.save(Source::Jm, &replacement).unwrap();
    assert_eq!(vault.load(Source::Jm).unwrap(), Some(replacement));
    assert_eq!(vault.load(Source::Pica).unwrap(), Some(pica.clone()));
    vault.delete(Source::Jm).unwrap();
    vault.delete(Source::Jm).unwrap();
    assert_eq!(vault.load(Source::Jm).unwrap(), None);
    assert_eq!(vault.load(Source::Pica).unwrap(), Some(pica));
}

#[test]
fn wrong_source_kind_cannot_replace_a_saved_session() {
    let vault = MemoryVault::new();
    let jm = cookie("jm", "sid=one");
    let token = StoredCredential::new("pica", CredentialKind::SessionToken, "token").unwrap();
    vault.save(Source::Jm, &jm).unwrap();
    assert_eq!(
        vault.save(Source::Jm, &token),
        Err(VaultError::INVALID_CREDENTIAL)
    );
    assert_eq!(
        vault.save(Source::Pica, &jm),
        Err(VaultError::INVALID_CREDENTIAL)
    );
    assert_eq!(vault.load(Source::Jm).unwrap(), Some(jm));
    assert_eq!(vault.load(Source::Pica).unwrap(), None);
}

#[test]
fn injected_failure_preserves_session_and_never_uses_a_fallback() {
    let vault = MemoryVault::new();
    let original = cookie("jm", "sid=one");
    vault.save(Source::Jm, &original).unwrap();
    for failure in [VaultError::ACCESS_DENIED, VaultError::UNAVAILABLE] {
        vault.set_failure(Some(failure)).unwrap();
        assert_eq!(vault.load(Source::Jm), Err(failure));
        assert_eq!(
            vault.save(Source::Jm, &cookie("changed", "sid=two")),
            Err(failure)
        );
        assert_eq!(vault.delete(Source::Jm), Err(failure));
        vault.set_failure(None).unwrap();
        assert_eq!(vault.load(Source::Jm).unwrap(), Some(original.clone()));
    }
}

#[test]
fn budget_counts_whole_record_utf8_and_json_escaping() {
    let overhead = codec::encode(Source::Jm, &cookie("account", "x"))
        .unwrap()
        .len()
        - 1;
    let fitting = "s".repeat(MAX_CREDENTIAL_BYTES - overhead);
    let credential = cookie("account", &fitting);
    let bytes = codec::encode(Source::Jm, &credential).unwrap();
    assert_eq!(bytes.len(), MAX_CREDENTIAL_BYTES);
    assert_eq!(codec::decode(Source::Jm, &bytes).unwrap(), credential);
    assert_eq!(
        StoredCredential::new(
            "account",
            CredentialKind::SessionCookie,
            format!("{fitting}x")
        ),
        Err(VaultError::TOO_LARGE)
    );
    for unit in ["\"", "\\", "界", "😀"] {
        let per_unit = serde_json::to_string(unit).unwrap().len() - 2;
        let count = (MAX_CREDENTIAL_BYTES - overhead) / per_unit;
        let fitting = unit.repeat(count);
        assert!(StoredCredential::new("account", CredentialKind::SessionCookie, &fitting).is_ok());
        assert_eq!(
            StoredCredential::new(
                "account",
                CredentialKind::SessionCookie,
                unit.repeat(count + 1)
            ),
            Err(VaultError::TOO_LARGE)
        );
    }
}

#[test]
fn corrupt_and_future_records_are_rejected_without_changing_bytes() {
    let original = codec::encode(Source::Jm, &cookie("account", "sid=test")).unwrap();
    let valid = String::from_utf8(original.to_vec()).unwrap();
    let fixtures = [
        b"not-json".to_vec(),
        vec![0xff],
        valid.replace("\"version\":1", "\"version\":0").into_bytes(),
        valid.replace("\"JM\"", "\"Pica\"").into_bytes(),
        valid.replace("sessionCookie", "sessionToken").into_bytes(),
        valid.replace("sid=test", "").into_bytes(),
        valid
            .replace("sid=test", "sid=bad\\r\\nheader")
            .into_bytes(),
        valid
            .replace("\"version\":1", "\"version\":1,\"extra\":true")
            .into_bytes(),
        valid
            .replace("\"secret\":", "\"secret\":\"duplicate\",\"secret\":")
            .into_bytes(),
    ];
    for bytes in fixtures {
        let before = bytes.clone();
        assert_eq!(codec::decode(Source::Jm, &bytes), Err(VaultError::CORRUPT));
        assert_eq!(bytes, before);
    }
    let future = valid.replace("\"version\":1", "\"version\":3").into_bytes();
    assert_eq!(
        codec::decode(Source::Jm, &future),
        Err(VaultError::UNSUPPORTED_SCHEMA)
    );
    assert_eq!(
        codec::decode(Source::Pica, &original),
        Err(VaultError::CORRUPT)
    );
    assert_eq!(
        codec::decode(Source::Jm, &vec![b' '; MAX_CREDENTIAL_BYTES + 1]),
        Err(VaultError::TOO_LARGE)
    );
}

#[test]
fn native_service_types_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<StoredCredential>();
    assert_send_sync::<MemoryVault>();
    #[cfg(windows)]
    assert_send_sync::<WindowsVault>();
}

#[test]
fn conditional_update_and_delete_reject_stale_generations() {
    let vault = MemoryVault::new();
    let old = cookie("old-account", "sid=old");
    let next = cookie("new-account", "sid=next");
    vault
        .compare_exchange(Source::Jm, None, Some(&old))
        .unwrap();
    assert_eq!(
        vault.compare_exchange(Source::Jm, None, Some(&next)),
        Err(VaultError::CHANGED)
    );
    vault
        .compare_exchange(Source::Jm, Some(old.fingerprint()), Some(&next))
        .unwrap();
    assert_eq!(
        vault.compare_exchange(Source::Jm, Some(old.fingerprint()), None),
        Err(VaultError::CHANGED)
    );
    assert_eq!(vault.load(Source::Jm).unwrap(), Some(next.clone()));
    vault
        .compare_exchange(Source::Jm, Some(next.fingerprint()), None)
        .unwrap();
    vault.compare_exchange(Source::Jm, None, None).unwrap();
    assert_eq!(vault.load(Source::Jm).unwrap(), None);
}

#[test]
fn concurrent_conditional_writers_have_exactly_one_winner() {
    use std::sync::{Arc, Barrier};
    let vault = Arc::new(MemoryVault::new());
    let original = cookie("original", "sid=original");
    vault.save(Source::Jm, &original).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let workers: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|name| {
            let vault = Arc::clone(&vault);
            let barrier = Arc::clone(&barrier);
            let expected = original.fingerprint();
            std::thread::spawn(move || {
                let next = cookie(name, &format!("sid={name}"));
                barrier.wait();
                vault
                    .compare_exchange(Source::Jm, Some(expected), Some(&next))
                    .map(|()| next)
            })
        })
        .collect();
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results.iter().find_map(|result| result.as_ref().err()),
        Some(&VaultError::CHANGED)
    );
    let winner = results
        .into_iter()
        .find_map(std::result::Result::ok)
        .unwrap();
    assert_eq!(vault.load(Source::Jm).unwrap(), Some(winner));
}

#[test]
fn conditional_failures_and_invalid_source_preserve_both_slots() {
    let vault = MemoryVault::new();
    let jm = cookie("jm", "sid=original");
    let pica = StoredCredential::new("pica", CredentialKind::SessionToken, "token").unwrap();
    vault.save(Source::Jm, &jm).unwrap();
    vault.save(Source::Pica, &pica).unwrap();
    assert_eq!(
        vault.compare_exchange(Source::Jm, Some(jm.fingerprint()), Some(&pica)),
        Err(VaultError::INVALID_CREDENTIAL)
    );
    vault.set_failure(Some(VaultError::ACCESS_DENIED)).unwrap();
    assert_eq!(
        vault.compare_exchange(Source::Jm, Some(jm.fingerprint()), None),
        Err(VaultError::ACCESS_DENIED)
    );
    vault.set_failure(None).unwrap();
    assert_eq!(vault.load(Source::Jm).unwrap(), Some(jm));
    assert_eq!(vault.load(Source::Pica).unwrap(), Some(pica));
}

#[test]
fn credential_fingerprint_is_stable_and_unambiguous() {
    use sha2::{Digest, Sha256};
    let original = cookie("ab", "c");
    assert_eq!(original.fingerprint(), original.clone().fingerprint());
    assert_ne!(original.fingerprint(), cookie("a", "bc").fingerprint());
    assert_ne!(original.fingerprint(), cookie("ab", "d").fingerprint());
    let mut bytes = 2u64.to_le_bytes().to_vec();
    bytes.extend_from_slice(b"abc");
    assert_eq!(
        original.fingerprint(),
        <[u8; 32]>::from(Sha256::digest(bytes))
    );
}

#[test]
fn legacy_wire_and_fingerprint_survive_jm_login_upgrade_and_password_removal() {
    let legacy = br#"{"version":1,"source":"JM","accountName":"ci-account","kind":"sessionCookie","secret":"sid=legacy"}"#;
    let session = codec::decode(Source::Jm, legacy).unwrap();
    assert_eq!(session.login_password(), None);
    assert_eq!(
        codec::encode(Source::Jm, &session).unwrap().as_slice(),
        legacy
    );
    let remembered = session
        .clone()
        .with_login_password(" preserved 密码 ")
        .unwrap();
    let encoded = codec::encode(Source::Jm, &remembered).unwrap();
    let wire: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(wire["version"], 2);
    assert_eq!(wire["loginPassword"], " preserved 密码 ");
    let restored = codec::decode(Source::Jm, &encoded).unwrap();
    assert_eq!(restored, remembered);
    assert_eq!(restored.session_only(), session);
    assert_eq!(restored.session_only().fingerprint(), session.fingerprint());
    assert_eq!(
        codec::encode(Source::Jm, &restored.session_only())
            .unwrap()
            .as_slice(),
        legacy
    );
    assert_eq!(restored.login_password(), Some(" preserved 密码 "));
}

#[test]
fn optional_login_rejects_pica_empty_or_control_passwords_and_binds_all_fingerprint_fields() {
    let pica = StoredCredential::new("ci-pica", CredentialKind::SessionToken, "token").unwrap();
    assert_eq!(
        pica.with_login_password("password"),
        Err(VaultError::INVALID_CREDENTIAL)
    );
    for invalid in ["", "bad\npass", "bad\0pass", "bad\u{85}pass"] {
        assert_eq!(
            cookie("account", "sid=one").with_login_password(invalid),
            Err(VaultError::INVALID_CREDENTIAL)
        );
    }
    let session = cookie("account", "sid=a");
    let first = session.clone().with_login_password("bc").unwrap();
    let replacement = first.clone().with_login_password("changed").unwrap();
    assert_ne!(first.fingerprint(), session.fingerprint());
    assert_ne!(first.fingerprint(), replacement.fingerprint());
    assert_ne!(
        first.fingerprint(),
        cookie("account", "sid=ab")
            .with_login_password("c")
            .unwrap()
            .fingerprint()
    );
    assert_ne!(
        first.fingerprint(),
        cookie("other-account", "sid=a")
            .with_login_password("bc")
            .unwrap()
            .fingerprint()
    );
    assert_eq!(first.fingerprint(), first.clone().fingerprint());
    assert_eq!(
        first.session_only().fingerprint(),
        replacement.session_only().fingerprint()
    );
}

#[test]
fn login_budget_includes_cookie_password_unicode_and_escaping_in_one_blob() {
    let session = cookie("account", "sid=retained");
    let overhead = codec::encode(
        Source::Jm,
        &session.clone().with_login_password("x").unwrap(),
    )
    .unwrap()
    .len()
        - 1;
    let fitting = session
        .clone()
        .with_login_password("p".repeat(MAX_CREDENTIAL_BYTES - overhead))
        .unwrap();
    let bytes = codec::encode(Source::Jm, &fitting).unwrap();
    assert_eq!(bytes.len(), MAX_CREDENTIAL_BYTES);
    assert_eq!(codec::decode(Source::Jm, &bytes).unwrap(), fitting);
    assert_eq!(
        session
            .clone()
            .with_login_password("p".repeat(MAX_CREDENTIAL_BYTES - overhead + 1)),
        Err(VaultError::TOO_LARGE)
    );
    for unit in ["\"", "\\", "界", "😀"] {
        let per_unit = serde_json::to_string(unit).unwrap().len() - 2;
        let count = (MAX_CREDENTIAL_BYTES - overhead) / per_unit;
        assert!(session
            .clone()
            .with_login_password(unit.repeat(count))
            .is_ok());
        assert_eq!(
            session.clone().with_login_password(unit.repeat(count + 1)),
            Err(VaultError::TOO_LARGE)
        );
    }
    // A cookie that fits alone cannot silently drop/split itself to add a password.
    let session_overhead = codec::encode(Source::Jm, &cookie("account", "x"))
        .unwrap()
        .len()
        - 1;
    let full_session = cookie(
        "account",
        &"s".repeat(MAX_CREDENTIAL_BYTES - session_overhead),
    );
    assert_eq!(
        full_session.with_login_password("x"),
        Err(VaultError::TOO_LARGE)
    );
}

#[test]
fn login_envelope_rejects_downgrades_missing_or_duplicate_password_and_wrong_source() {
    let credential = cookie("account", "sid=test")
        .with_login_password("safe-pass")
        .unwrap();
    let bytes = codec::encode(Source::Jm, &credential).unwrap();
    let valid = String::from_utf8(bytes.to_vec()).unwrap();
    for invalid in [
        valid.replace("\"version\":2", "\"version\":1"),
        valid.replace(",\"loginPassword\":\"safe-pass\"", ""),
        valid.replace("\"safe-pass\"", "null"),
        valid.replace("\"safe-pass\"", "\"\""),
        valid.replace("safe-pass", "bad\\npass"),
        valid.replace(
            "\"loginPassword\":",
            "\"loginPassword\":\"duplicate\",\"loginPassword\":",
        ),
        valid.replace("\"version\":2", "\"version\":2,\"extra\":true"),
    ] {
        let before = invalid.clone();
        assert_eq!(
            codec::decode(Source::Jm, invalid.as_bytes()),
            Err(VaultError::CORRUPT)
        );
        assert_eq!(invalid, before);
    }
    let pica = valid
        .replace("\"JM\"", "\"Pica\"")
        .replace("sessionCookie", "sessionToken");
    assert_eq!(
        codec::decode(Source::Pica, pica.as_bytes()),
        Err(VaultError::CORRUPT)
    );
    let legacy_with_null_password = valid
        .replace("\"version\":2", "\"version\":1")
        .replace("\"safe-pass\"", "null");
    assert_eq!(
        codec::decode(Source::Jm, legacy_with_null_password.as_bytes()),
        Err(VaultError::CORRUPT)
    );
}

#[test]
fn same_slot_cas_detects_password_only_changes_and_atomically_clears_login() {
    let vault = MemoryVault::new();
    let session = cookie("account", "sid=unchanged");
    let first = session.clone().with_login_password("old-password").unwrap();
    let replacement = session.clone().with_login_password("new-password").unwrap();
    let pica = StoredCredential::new("pica", CredentialKind::SessionToken, "token").unwrap();
    vault.save(Source::Jm, &session).unwrap();
    vault.save(Source::Pica, &pica).unwrap();
    vault
        .compare_exchange(Source::Jm, Some(session.fingerprint()), Some(&first))
        .unwrap();
    assert_eq!(
        vault.compare_exchange(Source::Jm, Some(session.fingerprint()), None),
        Err(VaultError::CHANGED)
    );
    vault
        .compare_exchange(Source::Jm, Some(first.fingerprint()), Some(&replacement))
        .unwrap();
    assert_eq!(
        vault.compare_exchange(Source::Jm, Some(first.fingerprint()), Some(&session)),
        Err(VaultError::CHANGED)
    );
    vault.set_failure(Some(VaultError::ACCESS_DENIED)).unwrap();
    assert_eq!(
        vault.compare_exchange(Source::Jm, Some(replacement.fingerprint()), Some(&session)),
        Err(VaultError::ACCESS_DENIED)
    );
    vault.set_failure(None).unwrap();
    assert_eq!(vault.load(Source::Jm).unwrap(), Some(replacement.clone()));
    vault
        .compare_exchange(
            Source::Jm,
            Some(replacement.fingerprint()),
            Some(&replacement.session_only()),
        )
        .unwrap();
    assert_eq!(vault.load(Source::Jm).unwrap(), Some(session));
    assert_eq!(vault.load(Source::Pica).unwrap(), Some(pica));
}
