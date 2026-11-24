//! Real portable worker ABI between the producer and App finalizer. Child
//! processes model browser WASM instances' independent resource authorities.
use clearra_host_contract::AppStatus;
use clearra_wasm::{
    WasmCommandRuntime, WasmDistributedCompletionAdvance, WasmDistributedCoordinator,
    WasmDistributedPreparation, WasmDistributedProducerAdvance, WasmDistributedVerifierRuntime,
    WasmHostCapabilities, WasmMinimumParallelWorker,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn replay_completion_workers_preserve_compact_and_full_height_products() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(run_completion_fixtures)
        .unwrap()
        .join()
        .unwrap();
}

fn run_completion_fixtures() {
    let directory = FixtureDirectory::new();
    const HOST_MEMORY_GRANT: u64 = 512 * 1024 * 1024;
    // Match configure_host + configure_product_retention at the product ABI:
    // the same original host grant is also the App source-retention authority.
    let budget = clearra_wasm::ProductRetentionBudget::new(HOST_MEMORY_GRANT).unwrap();
    assert_eq!(budget.maximum_bytes(), HOST_MEMORY_GRANT);
    let runtime = WasmCommandRuntime::default().with_host_capabilities(
        WasmHostCapabilities::new(8, false, false).with_product_retention_budget(Some(budget)),
    );
    let compact = "clearra pc path --lines 2 --board-mask 0xfc3f0 --height 2 --pieces 2 --queue OO --no-hold --rule srs-plus --backend cpu --no-tablebase".to_owned();
    let mut field =
        clearra_core_domain::board::standard_pc_board::Board256Mask::all_cells(70).unwrap();
    for (column, start) in [(0_u16, 0_u16), (1, 3)] {
        for row in start..start + 4 {
            field = field.without(
                clearra_core_domain::board::standard_pc_board::Board256Mask::singleton(
                    row * 10 + column,
                )
                .unwrap(),
            );
        }
    }
    let [a, b, c, d] = field.words();
    let full = format!("clearra pc path --lines 7 --board-mask 0x{d:016x}{c:016x}{b:016x}{a:016x} --height 7 --pieces 2 --queue II --no-hold --rule srs-plus --backend cpu --no-tablebase");
    let compact_hold = compact.replace("--no-hold", "--hold empty");
    let full_hold = full.replace("--no-hold", "--hold empty");
    for base in [&compact, &full, &compact_hold, &full_hold] {
        let expected = runtime
            .run_command_text(&format!("{base} --workers 1"))
            .unwrap();
        assert_eq!(expected.app_response().status(), AppStatus::Success);
        let expected_payload =
            serde_json::to_value(expected.app_response().product_result_payload().unwrap())
                .unwrap();
        drop(expected);
        for workers in [1, 2, 4, 8] {
            let command = format!(
                "{base} --workers {}{}",
                workers.max(2),
                if workers == 8 {
                    " --use-all-cpu-threads"
                } else {
                    ""
                }
            );
            // The native test process cannot admit a second independent
            // source lease beside the first browser instance's authority.
            let mut source = prepare_source(&runtime, &command);
            assert_eq!(source.requested_compute_workers(), workers.max(2));
            let source_lanes = source.worker_count();
            let initialization = source.worker_initialization();
            let batches = collect_source(&mut source);
            drop(source);
            let mut verifier = match initialization {
                Some(initialization) => {
                    WasmDistributedVerifierRuntime::prepare_forward(&runtime, &initialization)
                        .unwrap()
                }
                None => WasmDistributedVerifierRuntime::prepare(&runtime, &command).unwrap(),
            };
            let mut partials = Vec::new();
            for batch in &batches {
                let mut consumed = verifier.consume(batch).unwrap();
                loop {
                    if let Some(partial) = consumed.partial.take() {
                        partials.push(partial);
                    }
                    if !consumed.has_pending_work {
                        break;
                    }
                    consumed = verifier.continue_work().unwrap();
                }
            }
            let partial = verifier.finish().unwrap();
            if !partial.is_empty() {
                partials.push(partial);
            }
            drop(verifier);
            let mut source = prepare_source(&runtime, &command);
            assert_eq!(
                collect_source(&mut source),
                batches,
                "owner-bound source ordinals reproduce"
            );
            for partial in partials {
                source.absorb_partial(&partial).unwrap();
            }
            let mut completion = source.into_cooperative_completion(workers).unwrap();
            completion
                .configure_parallel_control(8, u128::from(HOST_MEMORY_GRANT))
                .unwrap();
            let query_path = directory.0.join("query.bin");
            let mut receipts = 0;
            let mut initialized = false;
            let mut finished = false;
            for _ in 0..10_000 {
                if completion.prepare_parallel(workers * 4).unwrap().is_some() {
                    assert!(!initialized, "all count/page tasks retain one source query");
                    assert!(completion.admit_parallel_control(workers, true, 8, u128::from(HOST_MEMORY_GRANT), 0, 0).unwrap(),
                        "completion preserves requested workers {workers}, source lanes {source_lanes}: {base}");
                    assert!(
                        !completion.start_coordinator_shard(0, 0).unwrap(),
                        "manager is control only"
                    );
                    fs::write(&query_path, completion.guarded_parallel_query(0).unwrap()).unwrap();
                    initialized = true;
                }
                let mut tasks = Vec::new();
                if initialized {
                    while let Some(task) = completion.take_parallel_task().unwrap() {
                        tasks.push(task);
                    }
                }
                if !tasks.is_empty() {
                    let mut groups = vec![Vec::new(); workers];
                    for (index, task) in tasks.into_iter().rev().enumerate() {
                        groups[index % workers].push(task);
                    }
                    let result_groups = std::thread::scope(|scope| {
                        let children = groups
                            .into_iter()
                            .enumerate()
                            .filter(|(_, tasks)| !tasks.is_empty())
                            .map(|(index, tasks)| {
                                let query_path = &query_path;
                                let directory = &directory.0;
                                scope.spawn(move || {
                                    run_worker_process(directory, query_path, index, tasks)
                                })
                            })
                            .collect::<Vec<_>>();
                        children
                            .into_iter()
                            .map(|child| child.join().unwrap())
                            .collect::<Vec<_>>()
                    });
                    for mut group in result_groups {
                        let cancelled = group.remove(0);
                        assert!(
                            completion.merge_parallel_receipt(&cancelled, 0).is_err(),
                            "cancellation cannot supply an exact count"
                        );
                        for receipt in group {
                            completion.merge_parallel_receipt(&receipt, 0).unwrap();
                            assert!(
                                completion.merge_parallel_receipt(&receipt, 0).is_err(),
                                "duplicate receipt cannot satisfy another cell"
                            );
                            receipts += 1;
                        }
                    }
                }
                match completion.advance(128).unwrap() {
                    WasmDistributedCompletionAdvance::Pending => {}
                    WasmDistributedCompletionAdvance::Cancelled => panic!("uncancelled completion"),
                    WasmDistributedCompletionAdvance::Completed(actual) => {
                        assert!(
                            initialized && receipts >= 2,
                            "count and first page run on actual worker ABI"
                        );
                        assert_eq!(actual.app_response().status(), AppStatus::Success);
                        assert_eq!(
                            serde_json::to_value(
                                actual.app_response().product_result_payload().unwrap()
                            )
                            .unwrap(),
                            expected_payload,
                            "workers {workers}: {base}"
                        );
                        finished = true;
                        break;
                    }
                }
            }
            assert!(finished, "bounded fixture completed");
        }
    }
}

// Invoked by the parent test binary with one finite packet set. An ordinary
// test run has no packet environment and does not spawn additional children.
#[test]
fn replay_parallel_worker_process() {
    let Ok(query_path) = std::env::var("CLEARRA_TEST_REPLAY_QUERY") else {
        return;
    };
    let query = fs::read(query_path).unwrap();
    let tasks = read_packets(Path::new(
        &std::env::var("CLEARRA_TEST_REPLAY_TASKS").unwrap(),
    ));
    let mut rebound = query.clone();
    rebound[28 + 9 + 32] ^= 1;
    resign_nested_packet(&mut rebound, 28);
    let rejected = match WasmMinimumParallelWorker::initialize(&rebound) {
        Err(error) => error,
        Ok(_) => panic!("rebound source accepted"),
    };
    assert_eq!(
        rejected.code(),
        "E_WASM_COMPLETION_REPLAY",
        "packet checksum cannot rebind the producer source"
    );
    let mut worker = WasmMinimumParallelWorker::initialize(&query).unwrap();
    let mut receipts = Vec::new();
    for (index, task) in tasks.into_iter().enumerate() {
        let mut stale = task.clone();
        stale[12 + 9 + 32] ^= 2;
        resign_nested_packet(&mut stale, 12);
        assert!(
            worker.start(&stale).is_err(),
            "stale generation is not executable"
        );
        if index == 0 {
            worker.start(&task).unwrap();
            receipts.push(worker.cancel().unwrap());
            assert!(
                !worker.has_active_shard(),
                "cancel drains this worker owner before reuse"
            );
        }
        worker.start(&task).unwrap();
        let mut terminal = None;
        for _ in 0..100_000 {
            if let Some(receipt) = worker.advance(128).unwrap() {
                terminal = Some(receipt);
                break;
            }
        }
        receipts.push(terminal.expect("tiny actual worker drains"));
    }
    write_packets(
        Path::new(&std::env::var("CLEARRA_TEST_REPLAY_RECEIPTS").unwrap()),
        &receipts,
    );
}

fn run_worker_process(
    directory: &Path,
    query: &Path,
    index: usize,
    tasks: Vec<Vec<u8>>,
) -> Vec<Vec<u8>> {
    let input = directory.join(format!("tasks-{index}.bin"));
    let output = directory.join(format!("receipts-{index}.bin"));
    write_packets(&input, &tasks);
    let execution = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "replay_parallel_worker_process", "--nocapture"])
        .env("CLEARRA_TEST_REPLAY_QUERY", query)
        .env("CLEARRA_TEST_REPLAY_TASKS", &input)
        .env("CLEARRA_TEST_REPLAY_RECEIPTS", &output)
        .output()
        .unwrap();
    assert!(
        execution.status.success(),
        "worker ABI failed: {} {}",
        String::from_utf8_lossy(&execution.stdout),
        String::from_utf8_lossy(&execution.stderr)
    );
    read_packets(&output)
}

struct FixtureDirectory(PathBuf);
impl FixtureDirectory {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/test-fixtures");
        fs::create_dir_all(&root).unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = root.join(format!("replay-completion-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn write_packets(path: &Path, packets: &[Vec<u8>]) {
    let mut bytes = (packets.len() as u64).to_le_bytes().to_vec();
    for packet in packets {
        bytes.extend_from_slice(&(packet.len() as u64).to_le_bytes());
        bytes.extend_from_slice(packet);
    }
    fs::write(path, bytes).unwrap();
}
fn read_packets(path: &Path) -> Vec<Vec<u8>> {
    let bytes = fs::read(path).unwrap();
    assert!(bytes.len() <= 4 * 1024 * 1024);
    let mut cursor = 0;
    fn number(bytes: &[u8], cursor: &mut usize) -> usize {
        let value = u64::from_le_bytes(bytes[*cursor..*cursor + 8].try_into().unwrap());
        *cursor += 8;
        usize::try_from(value).unwrap()
    }
    let count = number(&bytes, &mut cursor);
    assert!(count <= 1024);
    let packets = (0..count)
        .map(|_| {
            let length = number(&bytes, &mut cursor);
            let value = bytes[cursor..cursor + length].to_vec();
            cursor += length;
            value
        })
        .collect();
    assert_eq!(cursor, bytes.len());
    packets
}
fn resign_nested_packet(packet: &mut [u8], inner_start: usize) {
    let outer_end = packet.len() - 32;
    let inner_end = outer_end - 32;
    let inner = Sha256::digest(&packet[inner_start..inner_end]);
    packet[inner_end..outer_end].copy_from_slice(&inner);
    let outer = Sha256::digest(&packet[..outer_end]);
    packet[outer_end..].copy_from_slice(&outer);
}
fn prepare_source(runtime: &WasmCommandRuntime, command: &str) -> WasmDistributedCoordinator {
    match WasmDistributedCoordinator::prepare(runtime, command).unwrap() {
        WasmDistributedPreparation::Coordinator(source) => source,
        _ => panic!("distributed source required"),
    }
}
fn collect_source(source: &mut WasmDistributedCoordinator) -> Vec<Vec<u8>> {
    let mut batches = Vec::new();
    for _ in 0..10_000 {
        match source.advance_producer(4096, 16).unwrap() {
            WasmDistributedProducerAdvance::Pending
            | WasmDistributedProducerAdvance::Initialization(_) => {}
            WasmDistributedProducerAdvance::Batch(batch) => batches.push(batch),
            WasmDistributedProducerAdvance::Completed => return batches,
            WasmDistributedProducerAdvance::Cancelled => panic!("uncancelled source"),
        }
    }
    panic!("tiny source did not complete");
}
