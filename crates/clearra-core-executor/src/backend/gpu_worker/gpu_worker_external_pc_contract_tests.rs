use crate::backend::gpu_worker::{
    GpuFenceEpoch, GpuMemoryTicket, GpuWorkerRequest, PackingBatchDescriptor, PackingBatchId,
};
use clearra_core_ffi::{
    gpu::CGpuPieceMultisetWindow,
    problem::{C_GPU_PIECE_SOURCE_FIXED_SEQUENCE, C_PIECE_I, C_PIECE_J, C_PIECE_O, C_PIECE_T},
};
use std::{fs, path::PathBuf};

fn ticket(id: u64) -> GpuMemoryTicket {
    GpuMemoryTicket::new(id, GpuFenceEpoch::new(3), 4096)
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn gpu_multiset(pieces: &[u8]) -> CGpuPieceMultisetWindow {
    let mut window = CGpuPieceMultisetWindow {
        total_count: pieces.len() as u8,
        exact_count: pieces.len() as u8,
        ..Default::default()
    };
    for piece in pieces {
        window.counts[usize::from(*piece)] += 1;
    }
    window
}

#[test]
fn worker_external_pc_gpu_descriptor_uses_piece_source_and_multiset() {
    let batch = PackingBatchDescriptor::new(
        PackingBatchId::new(42),
        10,
        4,
        4,
        Some(4),
        0,
        4,
        4,
        4,
        C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
        42,
        gpu_multiset(&[C_PIECE_I, C_PIECE_O, C_PIECE_T, C_PIECE_J]),
        100,
        200,
        300,
        1024,
        999,
        1001,
        2001,
    )
    .expect("packing batch descriptor");

    let c_view = batch.to_c_descriptor_view().expect("C descriptor view");
    assert_eq!(c_view.piece_count, 4);
    assert_eq!(c_view.piece_source_id, 42);
    assert_eq!(
        c_view.piece_multiset_window.counts[usize::from(C_PIECE_I)],
        1
    );
    assert_eq!(c_view.piece_multiset_window.total_count, 4);
    assert_eq!(c_view.pattern_universe_id, 1001);
    assert_eq!(c_view.pattern_weight_model_id, 2001);

    let request =
        GpuWorkerRequest::new(7, batch, 1024, ticket(9), true).expect("typed GPU worker request");
    assert!(request.cpu_confirm_required());
    assert_eq!(request.batch().piece_source_id, 42);
}

#[test]
fn pco_worker_external_pc_fixture_declares_human_verified_backend_contract() {
    let text = fs::read_to_string(
        workspace_root().join("tests/fixtures/external-pc/pco_i_hold_6p_second_bag_pc.json"),
    )
    .expect("fixture");

    assert!(text.contains("\"source_id\": \"pcinfo-korea-pco-6p-i-hold\""));
    assert!(text.contains("\"human_verified\": true"));
    assert!(text.contains("\"setup_kind\": \"i-hold-6p-pco\""));
    assert!(text.contains("\"hold_piece\": \"I\""));
    assert!(text.contains("\"backend_modes\""));
    assert!(text.contains("\"gpu\""));
    assert!(text.contains("\"hybrid\""));
    assert!(text.contains("\"coverage_row_created_after_buildup\": true"));
    assert!(text.contains("\"exact_unique_solve_count_required\": false"));
}

#[test]
fn tsar_worker_external_pc_fixture_uses_full_42_unique_set_contract() {
    let text = fs::read_to_string(
        workspace_root().join("tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.json"),
    )
    .expect("fixture");

    assert!(text.contains("\"worker_correctness_basis\": \"unique_solve_set\""));
    assert!(text.contains("\"expected_unique_solution_count\": 42"));
    assert!(text.contains("\"minimal_solve_set_is_metadata_only\": true"));
    assert!(text.contains("\"pc_probability_source_percent\": \"98.69\""));
    assert!(text.contains("\"tsd_pc_probability_source_percent\": \"73.2\""));
    assert!(text.contains("\"packing_candidate_is_solution\": false"));
    assert!(text.contains("\"coverage_row_created_after_buildup\": true"));
    assert!(text.contains("\"allow_count_incomplete\": false"));
}
