use super::*;

#[tokio::test]
async fn success_receipt_survives_failure_cancel_special_scan_and_reopen() {
    let (root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    backend.put(Source::Jm, "Author A", 1,
        page(1, 1, vec![work(Source::Jm, "100", &["Author A"])]));
    service.discovery_start(scopes.clone(), vec![]).await.unwrap();
    let first = finish(&service, &scopes).await;
    assert_eq!(first.last_successful_check, first.last_check);
    let receipt = first.last_successful_check.clone().unwrap();
    backend.0.pages.lock().unwrap().insert(key(Source::Pica, "Author A", 1),
        Err(AccountError::new("SOURCE_RESPONSE_INVALID")));
    service.discovery_start_with_mode(scopes.clone(), vec![], DiscoveryMode::Full).await.unwrap();
    let partial = finish(&service, &scopes).await;
    assert_eq!(partial.last_check.as_ref().unwrap().phase, DiscoveryCheckPhase::Partial);
    assert_eq!(partial.last_successful_check, Some(receipt.clone()));

    backend.0.block_call.store(backend.0.calls.lock().unwrap().len() + 1, Ordering::SeqCst);
    let cancelled = service.discovery_start_with_mode(scopes.clone(), vec![], DiscoveryMode::Full).await.unwrap();
    backend.0.started.notified().await;
    service.discovery_cancel(&cancelled.run_id).unwrap();
    backend.0.release.notify_one();
    let stopped = finish(&service, &scopes).await;
    assert_eq!(stopped.last_successful_check, Some(receipt.clone()));
    backend.put(Source::Pica, "Author A", 1, empty());
    let ranges = [ ("Author A".into(), storage_source(Source::Jm)), ("Author A".into(), storage_source(Source::Pica)) ].into_iter().collect();
    service.discovery_start_selected(scopes.clone(), vec!["Author A".into()], DiscoveryMode::Full, false, Some(ranges)).await.unwrap();
    let special = finish(&service, &scopes).await;
    assert_eq!(special.last_check.as_ref().unwrap().phase, DiscoveryCheckPhase::Complete);
    assert_ne!(special.last_check.as_ref().unwrap().id, receipt.id);
    assert_eq!(special.last_successful_check, Some(receipt.clone()));

    // Metadata-only changes preserve the stable first-discovery identity.
    let mut changed = work(Source::Jm, "100", &["Author A"]);
    changed.title = "Changed title".into();
    changed.tags = vec!["new metadata".into()];
    backend.put(Source::Jm, "Author A", 1, page(1, 1, vec![changed]));
    service.discovery_start_with_mode(scopes.clone(), vec![], DiscoveryMode::Full).await.unwrap();
    let next = finish(&service, &scopes).await;
    assert_eq!(next.last_successful_check, next.last_check);
    assert_ne!(next.last_successful_check.as_ref().unwrap().id, receipt.id);
    assert_eq!(next.records[0].first_discovered_run_id.as_deref(), Some(receipt.id.as_str()));
    let context = service.discovery_context(scopes.clone()).await.unwrap();
    assert_eq!(WorkbenchStore::open(root.path()).unwrap().read_successful_scan(&context.account_key).unwrap(), next.last_successful_check);
    assert_eq!(service.discovery_read(scopes).await.unwrap().last_successful_check, next.last_successful_check);
    assert!(!root.path().join(workbench_storage::PRIVATE_DIRECTORY).join("special-follows.json").exists());
}
