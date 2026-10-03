//! Local observation commit cost, using only a fresh generated store.
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::Path,
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use workbench_storage::{DiscoveryWork, ObservedWork, Source, WorkbenchStore, PRIVATE_DIRECTORY};

fn record(id: usize, at: u64) -> ObservedWork {
    ObservedWork {
        work: DiscoveryWork {
            source: Source::Jm,
            work_id: id.to_string(),
            title: format!("Synthetic work {id}"),
            authors: vec![format!("Synthetic author {}", id % 32)],
            description: Some("Generated local observation benchmark".into()),
            tags: vec!["synthetic".into()],
            favorite: None,
            chapter_count: Some(2),
            page_count: Some(6),
            source_updated_at: None,
            cover_available: false,
        },
        categories: None,
        observed_at: at,
        metadata_detail_at: None,
        via: vec!["recent".into()],
    }
}
fn main() {
    let now = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    };
    let started = now();
    let git = |args: &[&str]| {
        let output = Command::new("git").args(args).output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let sha = git(&["rev-parse", "HEAD"]);
    let workspace = git(&["status", "--porcelain=v1"]);
    let binary_sha256 = format!(
        "{:x}",
        Sha256::digest(fs::read(std::env::current_exe().unwrap()).unwrap())
    );
    let out = std::env::args()
        .nth(1)
        .expect("fresh absolute output directory required");
    let out = Path::new(&out);
    assert!(out.is_absolute());
    fs::create_dir(out).expect("must not reuse an existing directory");
    fs::write(
        out.join(".synthetic-observation-cost"),
        b"generated metadata only\n",
    )
    .unwrap();
    let mut log = fs::File::create(out.join("latency.ndjson")).unwrap();
    let account = "a".repeat(64);
    let mut operations = 0;
    for size in [32usize, 20_000] {
        let root = out.join(format!("catalog-{size}"));
        let store = WorkbenchStore::open(&root).unwrap();
        let path = root.join(PRIVATE_DIRECTORY).join("observed-works.json");
        let fixture = json!({"schemaVersion":1,"revision":1,"value":{"version":1,"accounts":[{"accountKey":account,"source":"JM","records":(1..=size).map(|id|record(id,1)).collect::<Vec<_>>(),"recentIds":[],"coverage":{"headIds":[],"checkedAt":null,"pagesRead":0,"reachedEnd":false,"joinedPrevious":false,"initialWindow":false,"errorCode":null}}]}});
        // The fixture is validated by the real store before any measurement.
        fs::write(&path, serde_json::to_vec(&fixture).unwrap()).unwrap();
        let seeded = store.read_observed_works().unwrap();
        assert_eq!(seeded.value.accounts[0].records.len(), size);
        let bytes = fs::metadata(&path).unwrap().len();
        for round in 0..10 {
            // Account observation commits open a new handle; include this path
            // as well as repeated reads on a retained handle.
            let fresh = WorkbenchStore::open(&root).unwrap();
            for (scenario, handle) in [
                ("fresh-read", &fresh),
                // The last round committed different bytes. Retaining a handle
                // alone must never turn that change into a stale cache hit.
                ("retained-handle-changed-file-read", &store),
                ("warm-unchanged-file-read", &store),
            ] {
                let start = Instant::now();
                let value = handle.read_observed_works().unwrap();
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(value.value.accounts[0].records.len(), size);
                writeln!(log,"{}",json!({"size":size,"round":round,"scenario":scenario,"ms":elapsed,"bytes":bytes})).unwrap();
                operations += 1;
            }
            for (scenario, count) in [("merge-page-30", 30usize), ("merge-detail-1", 1usize)] {
                let fresh = WorkbenchStore::open(&root).unwrap();
                let mut incoming: Vec<_> = (1..=count).map(|id| record(id, 2 + round)).collect();
                if count == 1 {
                    incoming[0].metadata_detail_at = Some(2 + round);
                    incoming[0].via = vec!["detail".into()];
                }
                let start = Instant::now();
                let saved = fresh
                    .merge_observed_works(&account, Source::Jm, incoming, None)
                    .unwrap();
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(saved.value.accounts[0].records.len(), size);
                assert_eq!(saved.value.accounts[0].records[0].observed_at, 2 + round);
                assert!(saved.value.accounts[0]
                    .records
                    .iter()
                    .enumerate()
                    .all(|(i, r)| r.work.work_id == (i + 1).to_string()));
                writeln!(log,"{}",json!({"size":size,"round":round,"scenario":scenario,"ms":elapsed,"bytes":bytes,"revision":saved.revision})).unwrap();
                log.flush().unwrap();
                operations += 1;
            }
        }
        assert_eq!(
            WorkbenchStore::open(&root)
                .unwrap()
                .read_observed_works()
                .unwrap()
                .value
                .accounts[0]
                .records
                .len(),
            size
        );
    }
    fs::write(out.join("summary.json"),serde_json::to_vec_pretty(&json!({"complete":true,"operations":operations,"profile":if cfg!(debug_assertions){"debug"}else{"release"},"startedUnixMs":started,"endedUnixMs":now(),"checkoutRevision":sha,"workspaceStatus":workspace,"binarySha256":binary_sha256,"limits":"Local metadata parse/commit only; no real source network, no schema or integrity checks bypassed."})).unwrap()).unwrap();
}
