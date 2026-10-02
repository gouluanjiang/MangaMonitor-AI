//! Offline, generated-file native loading diagnostic. Never opens an existing library.
use image::{codecs::jpeg::JpegEncoder, Rgb, RgbImage};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Cursor, Write},
    path::Path,
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use workbench_library::{LibraryPhase, LibraryService, LocalReader, ScanAction};
use workbench_storage::WorkbenchStore;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
fn emit(file: &mut File, value: Value) {
    serde_json::to_writer(&mut *file, &value).unwrap();
    writeln!(file).unwrap();
    file.flush().unwrap();
}
fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
fn resource() -> Value {
    json!({
        "pid": std::process::id(),
        "status": fs::read_to_string("/proc/self/status").ok().map(|s| s.lines().filter(|line| line.starts_with("VmRSS:") || line.starts_with("VmHWM:") || line.starts_with("Threads:")).collect::<Vec<_>>().join("\n")),
        "fds": fs::read_dir("/proc/self/fd").ok().map(|entries| entries.count()),
    })
}
fn image() -> Vec<u8> {
    let image = RgbImage::from_fn(720, 1000, |x, y| {
        Rgb([
            ((x / 8 + y / 7) % 256) as u8,
            ((x / 13 + y / 4) % 256) as u8,
            ((y / 11) % 256) as u8,
        ])
    });
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 80)
        .encode_image(&image)
        .unwrap();
    bytes
}
fn archive(image: &[u8]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for chapter in 1..=2 {
        for page in 1..=3 {
            zip.start_file(
                format!("chapter{chapter}/{page}.jpg"),
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(image).unwrap();
        }
    }
    zip.finish().unwrap().into_inner()
}
fn hash(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        2,
        "usage: native_loading FRESH_OUTPUT_DIRECTORY"
    );
    let out = Path::new(&args[1]);
    assert!(
        out.is_absolute(),
        "use an explicit fresh absolute output path"
    );
    fs::create_dir(out).expect("output must not already exist");
    fs::write(
        out.join(".synthetic-native-loading"),
        b"generated ZIPs only; preserve on failure\n",
    )
    .unwrap();
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(revision.status.success());
    let sha = String::from_utf8(revision.stdout)
        .unwrap()
        .trim()
        .to_owned();
    let status = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .output()
        .unwrap();
    assert!(status.status.success());
    let workspace_status = String::from_utf8(status.stdout).unwrap();
    let binary_sha256 = hash(&std::env::current_exe().unwrap());
    let image = image();
    let zip = archive(&image);
    let zip_hash = format!("{:x}", Sha256::digest(&zip));
    let mut log = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(out.join("latency.ndjson"))
        .unwrap();
    let mut cases = 0;
    let mut measured_ms = 0.0;
    let mut first_start = None;
    for size in [24usize, 2000] {
        let case_root = out.join(format!("catalog-{size}"));
        let media = case_root.join("media");
        fs::create_dir_all(&media).unwrap();
        for index in 0..size {
            fs::write(media.join(format!("synthetic-{index:05}.zip")), &zip).unwrap();
        }
        let private = case_root.join("private");
        let store = WorkbenchStore::open(&private).unwrap();
        let start_at = now();
        first_start.get_or_insert(start_at);
        let case_clock = Instant::now();
        let mut service = LibraryService::new();
        let start = Instant::now();
        let mut snapshot = service.choose(&store, &media).unwrap();
        let mut first_ms = (!snapshot.items.is_empty()).then(|| ms(start));
        let mut batches = 0;
        while snapshot.phase == LibraryPhase::Reading {
            assert!(
                start.elapsed().as_secs() < 300,
                "synthetic scan did not finish in five minutes"
            );
            snapshot = service
                .scan(
                    &store,
                    snapshot.root_id.as_deref().unwrap(),
                    snapshot.generation,
                    ScanAction::Next,
                )
                .unwrap();
            batches += 1;
            if first_ms.is_none() && !snapshot.items.is_empty() {
                first_ms = Some(ms(start));
            }
        }
        let complete_ms = ms(start);
        assert_eq!(snapshot.phase, LibraryPhase::Complete);
        assert_eq!(snapshot.items.len(), size);
        assert_eq!(snapshot.skipped, 0);
        assert!(snapshot.error_code.is_none());
        emit(
            &mut log,
            json!({"scenario":"initial-native-scan", "size":size,"at":start_at,"firstMs":first_ms,"completeMs":complete_ms,"batches":batches,"items":snapshot.items.len(),"resources":resource()}),
        );
        cases += 1;
        let root_id = snapshot.root_id.as_deref().unwrap();
        let generation = snapshot.generation;
        let before = store.read_library().unwrap();
        for round in 0..5 {
            // A new store/service is a cold application cache, not a cold OS cache.
            let cold = WorkbenchStore::open(&private).unwrap();
            let mut cold_service = LibraryService::new();
            let start = Instant::now();
            let cold_list = cold_service.read(&cold).unwrap();
            let cold_ms = ms(start);
            assert_eq!(cold_list.items, snapshot.items);
            emit(
                &mut log,
                json!({"scenario":"native-list", "phase":"new-store", "size":size,"round":round,"completeMs":cold_ms}),
            );
            cases += 1;
            let start = Instant::now();
            let warm_list = cold_service.read(&cold).unwrap();
            let warm_ms = ms(start);
            assert_eq!(warm_list.items, snapshot.items);
            emit(
                &mut log,
                json!({"scenario":"native-list", "phase":"warm", "size":size,"round":round,"completeMs":warm_ms}),
            );
            cases += 1;
            let index = (round * 419 + 7) % size;
            let entry = &snapshot.items[index];
            for phase in ["first-read", "repeat-read"] {
                let start = Instant::now();
                let cover =
                    LibraryService::read_cover(&cold, root_id, generation, &entry.id).unwrap();
                let duration_ms = ms(start);
                let data = cover.data_url.unwrap();
                assert!(data.starts_with("data:image/jpeg;base64,"));
                emit(
                    &mut log,
                    json!({"scenario":"native-cover", "phase":phase,"size":size,"round":round,"completeMs":duration_ms,"dataUrlBytes":data.len(),"dataUrlSha256":format!("{:x}", Sha256::digest(data.as_bytes()))}),
                );
                cases += 1;
            }
            let start = Instant::now();
            let reader = LocalReader::open(&cold, root_id, generation, &entry.id).unwrap();
            let open_ms = ms(start);
            let chapters = reader.chapters();
            assert_eq!(chapters.len(), 2);
            assert!(chapters.iter().all(|chapter| chapter.page_count == 3));
            let first = reader.page(&cold, &chapters[0].id, 0).unwrap();
            let complete_ms = ms(start);
            assert_eq!(first.bytes, image);
            assert_eq!((first.width, first.height), (720, 1000));
            emit(
                &mut log,
                json!({"scenario":"native-reader-open-first-page", "size":size,"round":round,"firstMs":open_ms,"completeMs":complete_ms}),
            );
            cases += 1;
            for step in 0..12 {
                let chapter = &chapters[step % 2];
                let page_index = (step / 2 % 3) as u64;
                let start = Instant::now();
                let page = reader.page(&cold, &chapter.id, page_index).unwrap();
                let duration_ms = ms(start);
                assert_eq!(page.bytes, image);
                emit(
                    &mut log,
                    json!({"scenario":"native-reader-page", "size":size,"round":round,"step":step,"completeMs":duration_ms}),
                );
                cases += 1;
            }
            drop(reader);
            emit(
                &mut log,
                json!({"scenario":"round-resources-after-reader-drop", "size":size,"round":round,"resources":resource()}),
            );
        }
        assert_eq!(
            store.read_library().unwrap(),
            before,
            "read-only loading changed metadata"
        );
        assert_eq!(fs::read_dir(&media).unwrap().count(), size);
        for index in 0..size {
            assert_eq!(
                hash(&media.join(format!("synthetic-{index:05}.zip"))),
                zip_hash
            );
        }
        let duration = ms(case_clock);
        measured_ms += duration;
        emit(
            &mut log,
            json!({"scenario":"catalog-complete", "size":size,"durationMs":duration,"resources":resource(),"mediaHashesUnchanged":true,"metadataUnchanged":true}),
        );
    }
    let summary = json!({
        "sha":sha,"workspaceStatus":workspace_status,"binarySha256":binary_sha256,"startedUnixMs":first_start,"endedUnixMs":now(),"effectiveDiagnosticMs":measured_ms,
        "complete":true,"cases":cases,"releaseProfile":!cfg!(debug_assertions),"zipBytes":zip.len(),"imageBytes":image.len(),"imageDimensions":[720,1000],"zipSha256":zip_hash,
        "limits":["Generated ZIPs in fresh owned directories; no real media or source requests.","Five timing samples per condition, fixed deterministic operation order.","New-store cache is measured; host OS caches are not flushed.","Separate catalog stages; file generation between them is excluded from effectiveDiagnosticMs.","Direct native APIs; not Windows WebView IPC, actual download throughput, or a long-process leak test.","Every generated ZIP and persisted library document is verified unchanged after reads."]
    });
    fs::write(
        out.join("summary.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    println!("NATIVE_LOADING_COMPLETE {summary}");
}
