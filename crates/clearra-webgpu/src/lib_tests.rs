use super::*;

#[test]
fn webgpu_adapter_inventory_and_explicit_selection_share_indices() {
    let summaries = pollster::block_on(enumerate_adapter_summaries()).expect("adapter inventory");
    for adapter in &summaries {
        eprintln!(
            "WebGPU adapter {}: {} | type={} | backend={} | pci={}",
            adapter.index(),
            adapter.name(),
            adapter.device_type().as_str(),
            adapter.backend(),
            adapter.pci_bus_id(),
        );
    }
    let auto_selected = pollster::block_on(select_adapter_summary(WebGpuAdapterSelection::Auto));
    if let Ok(adapter) = &auto_selected {
        eprintln!(
            "WebGPU auto-selected adapter {}: {} | type={} | backend={}",
            adapter.index(),
            adapter.name(),
            adapter.device_type().as_str(),
            adapter.backend(),
        );
        if summaries
            .iter()
            .any(|candidate| candidate.device_type() == WebGpuAdapterDeviceType::DiscreteGpu)
        {
            assert_eq!(adapter.device_type(), WebGpuAdapterDeviceType::DiscreteGpu);
        }
    }
    let Some(expected) = summaries
        .iter()
        .find(|adapter| adapter.device_type() != WebGpuAdapterDeviceType::Cpu)
    else {
        eprintln!("No hardware WebGPU adapter available for explicit selection test");
        return;
    };

    let selected = pollster::block_on(select_adapter_summary(WebGpuAdapterSelection::Index(
        expected.index(),
    )))
    .expect("explicit adapter selection");
    assert_eq!(selected.index(), expected.index());
    assert_eq!(selected.name(), expected.name());
}

#[test]
fn webgpu_geometry_exact_cover_runs_real_packing_batch_when_adapter_is_available() {
    let batch = WebGpuGeometryExactCoverBatch::new(
        4,
        1,
        0,
        0b1111,
        0b1111,
        0,
        [1, 0, 0, 0, 0, 0, 0],
        vec![
            WebGpuPlacementSkeleton {
                mask: 0b1111,
                piece: 1,
                rotation: 0,
                x: 0,
                y: 0,
                operation_id: 0,
            },
            WebGpuPlacementSkeleton {
                mask: 0b1111,
                piece: 2,
                rotation: 0,
                x: 0,
                y: 0,
                operation_id: 1,
            },
        ],
        16,
    )
    .expect("exact-cover batch");
    let second_family =
        WebGpuGeometryExactCoverBatch::from_shared_geometry(&batch, [0, 1, 0, 0, 0, 0, 0], 16)
            .expect("second multiset family");
    let family = [batch, second_family];
    let session = pollster::block_on(WebGpuGeometryExactCoverBackend::connect());
    let WebGpuGeometryExactCoverSessionOutcome::Connected(mut session) = session else {
        eprintln!("WebGPU adapter unavailable for runtime test");
        return;
    };

    match pollster::block_on(session.run_family(&family)).expect("WebGPU family run") {
        WebGpuGeometryExactCoverOutcome::Connected(result) => {
            let mut paths = Vec::new();
            result
                .solution_graph()
                .stream_partition_paths(0, 1, &mut |skeleton_ids| {
                    paths.push(skeleton_ids.to_vec());
                    Ok::<(), ()>(())
                })
                .expect("stream family paths");
            assert_eq!(paths, [vec![0], vec![1]]);
            assert_eq!(
                result.trust_state(),
                WebGpuPackingTrustState::TrustedCpuSampleConfirmed
            );
            assert!(result.can_claim_exact());
            assert!(result.cpu_confirmed_dispatches() > 0);
            assert!(result.cpu_confirmed_parents() > 0);
            assert!(result.shader_hash().starts_with("wgsl-fnv64:"));
        }
        WebGpuGeometryExactCoverOutcome::Unavailable(result) => {
            eprintln!(
                "WebGPU adapter unavailable for runtime test: {}",
                result.reason()
            );
        }
        other => panic!("unexpected geometry exact cover outcome: {other:?}"),
    }
    session.recycle();
}

#[test]
fn connected_session_is_reused_after_prewarm_recycle() {
    let first_started = std::time::Instant::now();
    let first = pollster::block_on(WebGpuGeometryExactCoverBackend::connect());
    let first_elapsed = first_started.elapsed();
    let WebGpuGeometryExactCoverSessionOutcome::Connected(first) = first else {
        eprintln!("No hardware WebGPU adapter available for session reuse test");
        return;
    };
    let expected_index = first.adapter().index();
    let first_reused = first.reused();
    first.recycle();

    let reused_started = std::time::Instant::now();
    let second = pollster::block_on(WebGpuGeometryExactCoverBackend::connect());
    let reused_elapsed = reused_started.elapsed();
    let WebGpuGeometryExactCoverSessionOutcome::Connected(second) = second else {
        panic!("a recycled WebGPU session must remain connected");
    };
    eprintln!(
        "WebGPU session timing: first_connect_us={} first_reused={} reused_connect_us={}",
        first_elapsed.as_micros(),
        first_reused,
        reused_elapsed.as_micros()
    );
    assert!(second.reused());
    assert_eq!(second.adapter().index(), expected_index);
    second.recycle();
}

#[test]
fn webgpu_backend_runs_real_batch() {
    let batch = WebGpuBitsetBatch::new(&[
        vec![0b0011, 0b1000],
        vec![0b1100, 0b0100],
        vec![0b0010, 0b0001],
    ])
    .expect("valid batch");

    let outcome =
        pollster::block_on(WebGpuBackend::run_bitset_union(&batch)).expect("valid WebGPU request");
    match outcome {
        WebGpuBatchOutcome::Connected(result) => {
            assert_eq!(result.union_words(), &[0b1111, 0b1101]);
            assert!(result.cpu_confirmed());
            assert!(result.can_claim_exact());
            assert_eq!(
                result.trust_state(),
                WebGpuTrustState::DeterministicReferenceMatched
            );
        }
        WebGpuBatchOutcome::Unavailable(unavailable) => {
            assert!(!unavailable.reason().trim().is_empty());
            assert_ne!(
                std::env::var("CLEARRA_REQUIRE_WEBGPU_HARDWARE").as_deref(),
                Ok("1"),
                "CLEARRA_REQUIRE_WEBGPU_HARDWARE=1 requires a compute adapter: {}",
                unavailable.reason()
            );
        }
        WebGpuBatchOutcome::RejectedMismatch(mismatch) => {
            panic!("deterministic reference mismatch: {mismatch:?}");
        }
    }
}

#[test]
fn every_enumerated_hardware_adapter_can_be_selected_for_execution() {
    let adapters = pollster::block_on(enumerate_adapter_summaries()).expect("adapter inventory");
    let batch = WebGpuBitsetBatch::new(&[vec![0b0011], vec![0b1100]]).expect("valid batch");

    for adapter in adapters
        .iter()
        .filter(|adapter| adapter.device_type() != WebGpuAdapterDeviceType::Cpu)
    {
        let outcome = pollster::block_on(WebGpuBackend::run_bitset_union_on(
            &batch,
            WebGpuAdapterSelection::Index(adapter.index()),
        ))
        .expect("valid WebGPU request");
        let WebGpuBatchOutcome::Connected(result) = outcome else {
            panic!(
                "enumerated hardware adapter {} ({}) could not execute",
                adapter.index(),
                adapter.name()
            );
        };
        assert_eq!(result.union_words(), &[0b1111]);
        assert!(result.can_claim_exact());
    }
}

#[test]
fn webgpu_geometry_exact_cover_recovers_combined_batch_overflow_by_exact_chunking() {
    let batch = WebGpuGeometryExactCoverBatch::new(
        4,
        2,
        0,
        0xff,
        0xff,
        0,
        [1, 1, 0, 0, 0, 0, 0],
        vec![
            WebGpuPlacementSkeleton {
                mask: 0x0f,
                piece: 1,
                rotation: 0,
                x: 0,
                y: 0,
                operation_id: 0,
            },
            WebGpuPlacementSkeleton {
                mask: 0x0f,
                piece: 2,
                rotation: 0,
                x: 0,
                y: 0,
                operation_id: 1,
            },
            WebGpuPlacementSkeleton {
                mask: 0xf0,
                piece: 1,
                rotation: 0,
                x: 1,
                y: 1,
                operation_id: 2,
            },
            WebGpuPlacementSkeleton {
                mask: 0xf0,
                piece: 1,
                rotation: 1,
                x: 1,
                y: 1,
                operation_id: 3,
            },
            WebGpuPlacementSkeleton {
                mask: 0xf0,
                piece: 2,
                rotation: 0,
                x: 1,
                y: 1,
                operation_id: 4,
            },
            WebGpuPlacementSkeleton {
                mask: 0xf0,
                piece: 2,
                rotation: 1,
                x: 1,
                y: 1,
                operation_id: 5,
            },
        ],
        2,
    )
    .expect("exact-cover batch");

    match pollster::block_on(WebGpuGeometryExactCoverBackend::run(&batch)).expect("WebGPU run") {
        WebGpuGeometryExactCoverOutcome::Connected(result) => {
            let mut paths = std::collections::BTreeSet::new();
            result
                .solution_graph()
                .stream_partition_paths(0, 1, &mut |skeleton_ids| {
                    paths.insert(skeleton_ids.to_vec());
                    Ok::<(), ()>(())
                })
                .expect("stream chunked paths");
            assert_eq!(
                paths,
                [vec![0, 4], vec![0, 5], vec![1, 2], vec![1, 3]]
                    .into_iter()
                    .collect()
            );
        }
        WebGpuGeometryExactCoverOutcome::Unavailable(result) => {
            eprintln!(
                "WebGPU adapter unavailable for chunking test: {}",
                result.reason()
            );
        }
        other => panic!("unexpected geometry exact cover outcome: {other:?}"),
    }
}
