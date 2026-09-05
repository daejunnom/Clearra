use crate::native::CNativeBuildVariantView;

use super::*;

#[test]
fn ffi_kick_evidence_view_respects_scope_lifetime() {
    let mut source = [CKickEvidenceView::first_success(0, 1, 1, 2, 1, -1)];
    let native = CNativeBuildVariantView {
        candidate_id: 0x99,
        build_variant_id: 1,
        canonical_operation_set_id: 0x99,
        operation_set_hash: 0x99,
        kick_evidence: source.as_ptr(),
        kick_evidence_count: source.len() as u32,
        ..Default::default()
    };

    let owned = CBuildVariantView::from_native(&native).expect("owned view");
    source[0].kick_index = 9;

    assert_eq!(source[0].kick_index, 9);
    assert_eq!(owned.operation_set_hash(), 0x99);
    assert_eq!(owned.kick_evidence().len(), 1);
    assert_eq!(owned.kick_evidence()[0].kick_index, 2);
}

#[test]
fn kick_evidence_buffer_respects_scope_lifetime() {
    let mut source = vec![
        CKickEvidenceView::first_success(0, 1, 1, 2, 1, -1),
        CKickEvidenceView::first_success(1, 2, 2, 3, -1, 1),
    ];
    let native = CNativeBuildVariantView {
        candidate_id: 0x55,
        build_variant_id: 1,
        canonical_operation_set_id: 0x55,
        operation_set_hash: 0x55,
        kick_evidence: source.as_ptr(),
        kick_evidence_count: source.len() as u32,
        trace_completeness_flags: 0,
        ..Default::default()
    };

    let owned = CBuildVariantView::from_native(&native).expect("owned view");
    source.clear();

    assert_eq!(owned.operation_set_hash(), 0x55);
    assert_eq!(owned.kick_evidence().len(), 2);
    assert_eq!(owned.kick_evidence()[1].kick_index, 3);
}

#[test]
fn ffi_build_variant_view_copies_kick_evidence_to_block_pointer_escape() {
    let mut source = [CKickEvidenceView::first_success(0, 1, 1, 2, 1, -1)];
    let native = CNativeBuildVariantView {
        candidate_id: 0x55,
        build_variant_id: 1,
        canonical_operation_set_id: 0x55,
        operation_set_hash: 0x55,
        kick_evidence: source.as_ptr(),
        kick_evidence_count: source.len() as u32,
        ..Default::default()
    };

    let owned = CBuildVariantView::from_native(&native).expect("owned view");
    assert_ne!(owned.kick_evidence().as_ptr(), source.as_ptr());

    source[0].kick_index = 9;
    assert_eq!(source[0].kick_index, 9);
    assert_eq!(owned.kick_evidence()[0].kick_index, 2);
}

#[test]
fn ffi_build_variant_copies_kick_evidence_to_owned_vec() {
    ffi_build_variant_view_copies_kick_evidence_to_block_pointer_escape();
}

#[test]
fn ffi_view_copies_native_buffers_to_owned_rust() {
    ffi_build_variant_view_copies_kick_evidence_to_block_pointer_escape();
}

#[test]
fn ffi_build_variant_rejects_kick_evidence_count_above_c_limit() {
    let source = [CKickEvidenceView::first_success(0, 1, 1, 2, 1, -1)];
    let native = CNativeBuildVariantView {
        kick_evidence: source.as_ptr(),
        kick_evidence_count: (C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT + 1) as u32,
        ..Default::default()
    };

    assert_eq!(
        CBuildVariantView::from_native(&native),
        Err(CBuildVariantViewError::KickEvidenceCountExceeded {
            count: (C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT + 1) as u32,
            max: C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT,
        })
    );
}

#[test]
fn ffi_build_variant_view_rejects_unbounded_kick_evidence_count() {
    ffi_build_variant_rejects_kick_evidence_count_above_c_limit();
}

#[test]
fn ffi_build_variant_rejects_missing_kick_evidence_pointer() {
    let native = CNativeBuildVariantView {
        kick_evidence: core::ptr::null(),
        kick_evidence_count: 1,
        ..Default::default()
    };

    assert_eq!(
        CBuildVariantView::from_native(&native),
        Err(CBuildVariantViewError::MissingKickEvidencePointer { count: 1 })
    );
}

#[test]
fn ffi_build_variant_does_not_read_pointer_when_count_exceeds_limit() {
    let native = CNativeBuildVariantView {
        kick_evidence: core::ptr::NonNull::<CKickEvidenceView>::dangling().as_ptr(),
        kick_evidence_count: (C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT + 1) as u32,
        ..Default::default()
    };

    assert_eq!(
        CBuildVariantView::from_native(&native),
        Err(CBuildVariantViewError::KickEvidenceCountExceeded {
            count: (C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT + 1) as u32,
            max: C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT,
        })
    );
}

#[test]
fn ffi_kick_evidence_count_exceeded_rejected_before_pointer_read() {
    ffi_build_variant_does_not_read_pointer_when_count_exceeds_limit();
}

#[test]
fn ffi_rejects_pointer_count_overflow_before_read() {
    ffi_build_variant_does_not_read_pointer_when_count_exceeds_limit();
}

#[test]
fn ffi_pointer_count_bound_checked_before_read() {
    ffi_build_variant_does_not_read_pointer_when_count_exceeds_limit();
}

#[test]
fn ffi_build_variant_view_preserves_hold_branch_kind() {
    let native = CNativeBuildVariantView {
        hold_branch_kind: 2,
        ..Default::default()
    };

    let owned = CBuildVariantView::from_native(&native).expect("owned view");

    assert_eq!(owned.hold_branch_kind(), 2);
}

#[test]
fn ffi_build_variant_preserves_hold_branch_kind() {
    ffi_build_variant_view_preserves_hold_branch_kind();
}

#[test]
fn ffi_build_variant_copies_success_trace_to_owned_vec() {
    let mut operation_order = [17_u16];
    let mut trace_steps = [CBuildUpTraceStep {
        operation_id: 17,
        kick_evidence_index: u8::MAX,
        adjusted_x: 3,
        adjusted_y: 4,
        ..Default::default()
    }];
    let native = CNativeBuildVariantView {
        operation_order_ids: operation_order.as_ptr(),
        operation_order_count: 1,
        trace_steps: trace_steps.as_ptr(),
        trace_step_count: 1,
        ..Default::default()
    };

    let owned = CBuildVariantView::from_native(&native).expect("owned trace");
    operation_order[0] = 99;
    trace_steps[0].adjusted_y = 9;

    assert_eq!(operation_order[0], 99);
    assert_eq!(trace_steps[0].adjusted_y, 9);
    assert_eq!(owned.operation_order_ids(), &[17]);
    assert_eq!(owned.trace_steps()[0].adjusted_x, 3);
    assert_eq!(owned.trace_steps()[0].adjusted_y, 4);
}
