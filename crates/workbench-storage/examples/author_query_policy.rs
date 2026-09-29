//! Explicit offline maintenance. No account secrets, source clients or media IO.
use serde_json::json;
use std::{io::Read, path::PathBuf};
use workbench_storage::{AuthorQueryDocument, DiscoveryRangeState, WorkbenchStore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 2 && arguments.len() != 6 {
        return Err("usage: author_query_policy ROOT INPUT [--apply POLICY_REV FOLLOWING_REV DISCOVERY_REV]; close the app before applying".into());
    }
    let root = PathBuf::from(&arguments[0]);
    let input = PathBuf::from(&arguments[1]);
    let metadata = std::fs::symlink_metadata(&input)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 32 * 1024 * 1024
    {
        return Err("invalid bounded input file".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&input)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("input too large".into());
    }
    let incoming: AuthorQueryDocument = serde_json::from_slice(&bytes)?;
    let store = WorkbenchStore::open(&root)?;
    let current = store.read_author_query_policies()?;
    let following = store.read_following()?;
    let discovery = store.read_discovery()?;
    let next = current.value.merged_import(&incoming, &following.value)?;
    let mut query_changes = 0usize;
    let mut credit_changes = 0usize;
    let mut work_credit_changes = 0usize;
    for account in &incoming.accounts {
        let old_account = current.value.accounts.iter().find(|existing| {
            existing.source == account.source && existing.account_key == account.account_key
        });
        work_credit_changes += account
            .work_credits
            .iter()
            .filter(|rule| {
                old_account.and_then(|existing| {
                    existing
                        .work_credits
                        .iter()
                        .find(|old| old.work_id == rule.work_id)
                }) != Some(*rule)
            })
            .count();
        for profile in &account.profiles {
            let before =
                current
                    .value
                    .resolve(account.source, &account.account_key, &profile.author);
            let after = next.resolve(account.source, &account.account_key, &profile.author);
            query_changes += usize::from(before.query_fingerprint != after.query_fingerprint);
            credit_changes += usize::from(
                before.verified_aliases != after.verified_aliases
                    || before.exact_credits != after.exact_credits,
            );
        }
    }
    let scan_idle = !discovery
        .value
        .accounts
        .iter()
        .flat_map(|account| &account.authors)
        .any(|range| range.state == DiscoveryRangeState::Checking);
    let applied = arguments.len() == 6;
    let mut revision = current.revision;
    if applied {
        if arguments[2] != "--apply" {
            return Err("expected --apply".into());
        }
        let expected: Vec<u64> = arguments[3..]
            .iter()
            .map(|argument| {
                argument
                    .to_str()
                    .ok_or("invalid revision")?
                    .parse::<u64>()
                    .map_err(|_| "invalid revision")
            })
            .collect::<Result<_, _>>()?;
        revision = store
            .import_author_query_policies(expected[0], expected[1], expected[2], incoming)?
            .revision;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "applied": applied, "policyRevision": revision,
            "followingRevision": following.revision, "discoveryRevision": discovery.revision,
            "scanIdle": scan_idle, "queryChangedScopes": query_changes,
            "creditChangedScopes": credit_changes,
            "workCreditChangedRules": work_credit_changes,
            "followingChanged": false, "libraryChanged": false, "historyDeleted": false
        }))?
    );
    Ok(())
}
