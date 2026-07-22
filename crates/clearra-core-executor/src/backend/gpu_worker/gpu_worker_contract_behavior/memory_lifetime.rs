use super::*;

mod case_gpu_worker_request_rejects_missing_memory_ticket {
    use super::*;

    #[test]
    pub(crate) fn gpu_worker_request_rejects_missing_memory_ticket() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(7))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");
        let result = GpuWorkerRequest::from_optional_memory_ticket(11, descriptor, 5, None, true);

        assert_eq!(result, Err(GpuWorkerError::MissingMemoryTicket));
    }
}
pub(crate) use case_gpu_worker_request_rejects_missing_memory_ticket::gpu_worker_request_rejects_missing_memory_ticket;

mod case_gpu_worker_request_requires_memory_ticket {
    use super::*;

    #[test]
    fn gpu_worker_request_requires_memory_ticket() {
        gpu_worker_request_rejects_missing_memory_ticket();
    }
}

mod case_gpu_worker_missing_memory_ticket_rejected {
    use super::*;

    #[test]
    fn gpu_worker_missing_memory_ticket_rejected() {
        assert_eq!(
            GpuMemoryTicket::try_new(0, GpuFenceEpoch::new(3), 4096),
            Err(GpuWorkerError::InvalidMemoryTicket {
                reason: "memory ticket id must be nonzero",
            })
        );
        gpu_worker_request_rejects_missing_memory_ticket();
    }
}

mod case_gpu_worker_request_requires_cpu_confirm_for_real_gpu {
    use super::*;

    #[test]
    fn gpu_worker_request_requires_cpu_confirm_for_real_gpu() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(7))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");
        let result = GpuWorkerRequest::new(
            11,
            descriptor,
            5,
            GpuMemoryTicket::new(42, GpuFenceEpoch::new(3), 4096),
            false,
        );

        assert_eq!(result, Err(GpuWorkerError::CpuConfirmRequiredForGpuBatch));
    }
}

mod case_gpu_memory_ticket_preserves_scope_epoch_and_budget {
    use super::*;

    #[test]
    fn gpu_memory_ticket_preserves_scope_epoch_and_budget() {
        let ticket = GpuMemoryTicket::new(9, GpuFenceEpoch::new(4), 8192);

        assert_eq!(ticket.id(), 9);
        assert_eq!(ticket.scope_epoch(), GpuFenceEpoch::new(4));
        assert_eq!(ticket.byte_budget(), 8192);
    }
}

mod case_gpu_memory_ticket_rejects_missing_ticket_epoch_or_budget {
    use super::*;

    #[test]
    fn gpu_memory_ticket_rejects_missing_ticket_epoch_or_budget() {
        assert!(GpuMemoryTicket::try_new(0, GpuFenceEpoch::new(4), 8192).is_err());
        assert!(GpuMemoryTicket::try_new(9, GpuFenceEpoch::new(0), 8192).is_err());
        assert!(GpuMemoryTicket::try_new(9, GpuFenceEpoch::new(4), 0).is_err());
    }
}

mod case_gpu_submission_rejects_epoch_mismatch_before_backend_execution {
    use super::*;

    #[test]
    fn gpu_submission_rejects_epoch_mismatch_before_backend_execution() {
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
        let wrong_submission = GpuWorkerSubmission::new(11, GpuFenceEpoch::new(4), 0);

        let result = wrong_submission.validate_request(&request);

        assert!(matches!(
            result,
            Err(GpuWorkerError::MemoryTicketMismatch {
                expected: 3,
                actual: 4
            })
        ));
    }
}
