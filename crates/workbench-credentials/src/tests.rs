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
    let credential = cookie("private-mail@example.test", "private-session-fixture");
    let cloned = credential.clone();
    assert_eq!(credential, cloned);
    let debug = format!("{credential:?}");
    assert!(!debug.contains(credential.account_name()));
    assert!(!debug.contains(credential.secret()));
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
    let future = valid.replace("\"version\":1", "\"version\":2").into_bytes();
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
