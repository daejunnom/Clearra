#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfiProblemError {
    InvalidBoardLayout {
        width: u16,
        height: u16,
    },
    UnsupportedBoardBackend {
        backend_kind: &'static str,
        cell_count: u32,
    },
    PieceWindowTooLarge {
        max_pieces: usize,
    },
    ExactPiecesTooLarge {
        exact_pieces: usize,
    },
    PieceMultisetFamilyTooLarge {
        member_count: usize,
        capacity: usize,
    },
    QueueTooLong {
        len: usize,
    },
    QueueTruncatedButExactNeeded {
        len: usize,
        stored_len: usize,
        required_pieces: usize,
    },
    PatternIdOutOfRange {
        pattern_id: usize,
        pattern_count: usize,
    },
    PatternSequenceTooLong {
        len: usize,
        capacity: usize,
    },
    InvalidSupplyDescriptor,
    DescriptorStorageAllocationFailed,
    BudgetTooLarge {
        field: &'static str,
        value: usize,
    },
    MemoryBudgetTooLarge {
        value: u64,
    },
    CandidateOperationCountTooLarge {
        operation_count: usize,
    },
    InvalidCandidatePiece {
        piece: u8,
    },
    UnverifiedRuleProfileRejected {
        rule_profile_id: u32,
    },
    VerifiedKickProfileRuleMismatch {
        rule_profile_id: u32,
        source_rule_profile_id: u32,
    },
    SpawnAwareRuleProfileRejected {
        rule_profile_id: u32,
    },
    VerifiedKickProfileMissingRequired180 {
        rule_profile_id: u32,
    },
    KickTransitionCountTooLarge {
        transition_count: usize,
    },
    KickOffsetSequenceTooLong {
        offset_count: usize,
    },
    UnverifiedCustomRuleRejectedBeforeExecution,
    CustomRuleDescriptorRuntimeNotConnected,
}
