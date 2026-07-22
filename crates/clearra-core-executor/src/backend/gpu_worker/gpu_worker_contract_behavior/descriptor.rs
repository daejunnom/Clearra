use super::*;

mod case_gpu_worker_request_preserves_batch_descriptor {

    use super::*;

    #[test]

    fn gpu_worker_request_preserves_batch_descriptor() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(7))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");

        let request = GpuWorkerRequest::new(
            11,
            descriptor,
            5,
            GpuMemoryTicket::new(42, GpuFenceEpoch::new(3), 4096),
            true,
        )
        .expect("GPU request");

        assert_eq!(request.batch(), descriptor);

        assert_eq!(request.request_id(), 11);

        assert_eq!(request.candidate_count_hint(), 5);
    }
}

mod case_rust_gpu_batch_descriptor_maps_to_c_descriptor {

    use super::*;

    #[test]

    fn rust_gpu_batch_descriptor_maps_to_c_descriptor() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(7))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");

        let c_descriptor = descriptor.to_c_descriptor_view().expect("C descriptor");

        assert_eq!(c_descriptor.batch_id, 7);

        assert_eq!(c_descriptor.board_width, descriptor.board_width);

        assert_eq!(c_descriptor.board_height, descriptor.board_height);

        assert_eq!(
            c_descriptor.active_packing_rows,
            descriptor.active_packing_rows
        );

        assert_eq!(c_descriptor.goal_clear_lines_hint, 0);

        assert_eq!(c_descriptor.piece_window, descriptor.piece_window);

        assert_eq!(c_descriptor.piece_count, descriptor.piece_count);

        assert_eq!(c_descriptor.exact_piece_count, descriptor.exact_piece_count);

        assert_eq!(c_descriptor.piece_source_kind, descriptor.piece_source_kind);

        assert_eq!(
            descriptor.product_source_of_truth(),
            (
                descriptor.piece_source_id,
                descriptor.pattern_universe_id,
                descriptor.pattern_weight_model_id,
                descriptor.piece_multiset_window,
            )
        );

        assert_eq!(
            c_descriptor.initial_board_mask,
            descriptor.initial_board_mask
        );

        assert_eq!(
            c_descriptor.operation_table_id,
            descriptor.operation_table_id
        );

        assert_eq!(c_descriptor.rule_profile_id, descriptor.rule_profile_id);

        assert_eq!(c_descriptor.kick_profile_id, descriptor.kick_profile_id);

        assert_eq!(
            c_descriptor.candidate_capacity,
            descriptor.candidate_capacity
        );

        assert_eq!(c_descriptor.shape_hash_seed, descriptor.shape_hash_seed);

        assert_eq!(
            c_descriptor.pattern_universe_id,
            descriptor.pattern_universe_id
        );

        assert_eq!(
            c_descriptor.pattern_weight_model_id,
            descriptor.pattern_weight_model_id
        );
    }
}

mod case_packing_batch_descriptor_preserves_piece_window_exact_count_and_source {

    use super::*;

    #[test]

    fn packing_batch_descriptor_preserves_piece_window_exact_count_and_source() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(7))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");

        assert_eq!(descriptor.piece_window, 5);

        assert_eq!(descriptor.piece_count, 5);

        assert_eq!(descriptor.exact_piece_count, 5);

        assert_eq!(
            descriptor.piece_source_kind,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE
        );
    }
}

mod case_packing_batch_descriptor_preserves_active_rows_and_clear_hint {

    use super::*;

    #[test]

    fn packing_batch_descriptor_preserves_active_rows_and_clear_hint() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(7))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");

        assert_eq!(descriptor.active_packing_rows, 2);

        assert_eq!(descriptor.goal_clear_lines_hint, None);
    }
}

mod case_rust_gpu_batch_descriptor_maps_piece_source_and_multiset_to_c_descriptor {

    use super::*;

    #[test]

    fn rust_gpu_batch_descriptor_maps_piece_source_and_multiset_to_c_descriptor() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(7))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");

        let c_descriptor = descriptor.to_c_descriptor_view().expect("C descriptor");

        assert_eq!(c_descriptor.piece_source_id, descriptor.piece_source_id);

        assert_eq!(
            c_descriptor.piece_multiset_window,
            descriptor.piece_multiset_window
        );

        assert_eq!(
            descriptor.product_source_of_truth().3.counts[usize::from(C_PIECE_I)],
            1
        );
    }
}

mod case_packing_batch_descriptor_rejects_missing_piece_multiset_window {

    use super::*;

    #[test]

    fn packing_batch_descriptor_rejects_missing_piece_multiset_window() {
        let mut compact = compact_problem();

        compact.piece_multiset_window.total_count = 2;

        let result = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(7))
            .from_compact_problem_with_identity(&compact, 1001, 2001);

        assert_eq!(
            result,
            Err(PackingBatchValidationError::MissingPieceMultisetWindow {
                piece_count: 5,

                stored_len: 2,
            })
        );
    }
}
