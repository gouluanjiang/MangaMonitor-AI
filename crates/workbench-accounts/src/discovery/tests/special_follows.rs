use super::*;

async fn finish_special(service: &TestService, scopes: &[DiscoveryScope]) -> crate::SpecialSnapshot {
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let run = service.special_run().unwrap();
            if !matches!(run.phase.as_str(), "waiting" | "checking") { break; }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }).await.unwrap();
    service.special_read(scopes.to_vec()).await.unwrap()
}

#[tokio::test]
async fn baseline_unread_failure_retry_and_ordinary_follow_are_independent() {
    let (root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    let following = WorkbenchStore::open(root.path()).unwrap().read_following().unwrap();
    catalog(&backend, &[100, 101]);
    service.special_set(scopes.clone(), "Author A".into(), true).await.unwrap();
    let baseline = finish_special(&service, &scopes).await;
    assert_eq!(baseline.run.phase, "complete");
    assert_eq!(baseline.authors[0].baselines_complete, 2);
    assert!(baseline.updates.is_empty());
    catalog(&backend, &[102, 100, 101]);
    service.special_start().unwrap();
    let newer = finish_special(&service, &scopes).await;
    assert_eq!(newer.updates.len(), 1);
    assert_eq!(newer.updates[0].work.work_id, "102");
    service.special_mark_read(scopes.clone(), Some(workbench_storage::LibraryReference {source: workbench_storage::Source::Jm, work_id: "102".into()})).await.unwrap();
    backend.0.pages.lock().unwrap().insert(key(Source::Jm, "Author A", 1), Err(AccountError::new("SOURCE_TIMEOUT")));
    service.special_start().unwrap();
    let failed = finish_special(&service, &scopes).await;
    assert_ne!(failed.run.phase, "complete");
    assert_eq!(failed.updates.len(), 1);
    assert!(failed.updates[0].read_at.is_some());
    catalog(&backend, &[103, 102, 100, 101]);
    service.special_start().unwrap();
    assert_eq!(finish_special(&service, &scopes).await.updates.len(), 2);
    service.special_set(scopes.clone(), "Author A".into(), false).await.unwrap();
    assert!(service.special_read(scopes).await.unwrap().updates.is_empty());
    assert_eq!(WorkbenchStore::open(root.path()).unwrap().read_following().unwrap(), following);
}

#[tokio::test]
async fn special_checks_join_manual_scan_and_cancellation_does_not_cancel_its_owner() {
    let (root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    let context = service.discovery_context(scopes.clone()).await.unwrap();
    WorkbenchStore::open(root.path()).unwrap().edit_special_follows(&context.account_key, (context.following_revision, context.policy_revision), |account| account.set_author("Author A", true)).unwrap();
    backend.0.block_call.store(1, Ordering::SeqCst);
    let manual = service.discovery_start(scopes.clone(), vec![]).await.unwrap();
    backend.0.started.notified().await;
    let first = service.special_start().unwrap();
    assert_eq!(service.special_start().unwrap().id, first.id);
    service.special_cancel().unwrap();
    assert_eq!(service.discovery_progress(scopes.clone()).await.unwrap().run.unwrap().id, manual.run_id);
    backend.0.release.notify_one();
    finish(&service, &scopes).await;
    let count = backend.0.calls.lock().unwrap().len();
    assert_eq!(count, 2);
    assert_eq!(service.special_run().unwrap().phase, "cancelled");
}

#[tokio::test]
async fn cold_start_gate_is_process_owned_and_saved_specials_are_restored() {
    let (root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    service.special_set(scopes.clone(), "Author A".into(), true).await.unwrap();
    finish_special(&service, &scopes).await;
    service.special_cold_start();
    let first = service.special_run().unwrap().id;
    service.special_cold_start();
    finish_special(&service, &scopes).await;
    service.special_cold_start();
    assert_eq!(service.special_run().unwrap().id, first);
    let restarted = Arc::new(AccountService::new(backend, MemoryVault::new(), root.path().into()));
    let mut scopes2 = vec![];
    for source in [Source::Jm, Source::Pica] {
        let account = restarted.login(source, format!("{}-fixture", label(source)), "fixture-only".into(), false).await.unwrap();
        scopes2.push(DiscoveryScope {source, session_id: account.session_id.unwrap()});
    }
    assert_eq!(restarted.special_read(scopes2.clone()).await.unwrap().authors[0].baselines_complete, 2);
    restarted.special_cold_start();
    assert_eq!(restarted.special_run().unwrap().id, 1);
    finish_special(&restarted, &scopes2).await;
}

#[tokio::test]
async fn a_manual_range_finished_after_special_start_is_reused_without_another_query() {
    let (root, backend, service, scopes) = setup().await;
    follow(&service, &scopes[0], "Author A", true).await;
    let context = service.discovery_context(scopes.clone()).await.unwrap();
    WorkbenchStore::open(root.path()).unwrap().edit_special_follows(&context.account_key, (context.following_revision, context.policy_revision), |account| account.set_author("Author A", true)).unwrap();
    backend.0.block_call.store(1, Ordering::SeqCst);
    service.discovery_start(scopes.clone(), vec![]).await.unwrap();
    backend.0.started.notified().await;
    service.special_start().unwrap();
    backend.0.release.notify_one();
    let result = finish_special(&service, &scopes).await;
    assert_eq!(result.run.phase, "complete");
    assert_eq!(result.authors[0].baselines_complete, 2);
    assert!(result.updates.is_empty());
    assert_eq!(backend.0.calls.lock().unwrap().len(), 2);
}
