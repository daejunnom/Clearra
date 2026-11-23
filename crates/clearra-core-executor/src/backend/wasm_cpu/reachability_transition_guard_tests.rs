//! Finite truth-table and primitive comparisons for compiled movement guards.
//! These are correctness fixtures, not search or small-input timing samples.

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_rules::kicks::KickTableProfileId;

use super::{
    compile_free_endpoint_guard, downward_occupancy_shadow, reverse_rotation_is_first_success,
    reverse_rotation_is_first_success_on_free_endpoints, ReachabilityTemplate,
    ReverseRotationSource, NO_PRIOR_GUARD, ORDERED_GUARD, SINGLE_BIT_GUARD_BASE,
    SINGLE_MASK_GUARD_BASE,
};

#[test]
fn free_endpoint_guard_matches_full_ordered_kicks_for_every_builtin_profile() {
    let mut optimized = 0;
    let mut ordered = 0;
    for profile in [
        KickTableProfileId::Srs90,
        KickTableProfileId::NoKick,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
    ] {
        for piece in PieceKind::STANDARD_TETROMINOES {
            let template = ReachabilityTemplate::compile(6, 4, piece, profile);
            for target in 0..template.state_masks.len() {
                let start = template.reverse_rotation_offsets[target] as usize;
                let end = template.reverse_rotation_offsets[target + 1] as usize;
                for &source in &template.reverse_rotation_sources[start..end] {
                    let free = template.state_masks[usize::from(source.source)]
                        | template.state_masks[target];
                    let mut sample = 0x9e37_79b9_7f4a_7c15_u64;
                    for _ in 0..12 {
                        sample ^= sample << 13;
                        sample ^= sample >> 7;
                        sample ^= sample << 17;
                        let board = sample & ((1 << 24) - 1) & !free;
                        assert_eq!(
                            reverse_rotation_is_first_success_on_free_endpoints(
                                &template, board, source,
                            ),
                            reverse_rotation_is_first_success(&template, board, source),
                            "{profile:?} {piece:?} target={target} source={source:?} board={board:#x}",
                        );
                    }
                    optimized += usize::from(source.free_endpoint_guard != ORDERED_GUARD);
                    ordered += usize::from(source.free_endpoint_guard == ORDERED_GUARD);
                }
            }
        }
    }
    assert!(optimized > 0);
    assert!(
        ordered > 0,
        "general conjunctions must retain ordered evaluation"
    );
    assert_eq!(core::mem::size_of::<ReverseRotationSource>(), 8);
}

fn guard_template(masks: &[u64]) -> (ReachabilityTemplate, ReverseRotationSource) {
    let mut template =
        ReachabilityTemplate::compile(4, 4, PieceKind::T, KickTableProfileId::NoKick);
    template.state_masks = std::iter::once(0).chain(masks.iter().copied()).collect();
    template.rotation_targets = (1..=masks.len()).map(|index| index as u16).collect();
    template.rotation_target_offsets = vec![
        0,
        masks.len() as u32,
        masks.len() as u32,
        masks.len() as u32,
    ];
    let source = ReverseRotationSource {
        source: 0,
        slot: 0,
        free_endpoint_guard: compile_free_endpoint_guard(
            &template.state_masks,
            0,
            0,
            &template.rotation_targets,
        ),
        prior_target_count: masks.len() as u32,
    };
    (template, source)
}

#[test]
fn guard_reduction_requires_a_real_minimum_clause_not_just_intersection() {
    for (masks, expected) in [
        (vec![], NO_PRIOR_GUARD),
        (vec![0b001, 0b011], SINGLE_BIT_GUARD_BASE),
        (vec![0b011, 0b111], SINGLE_MASK_GUARD_BASE),
        (vec![0b011, 0b101], ORDERED_GUARD),
    ] {
        let (template, source) = guard_template(&masks);
        assert_eq!(source.free_endpoint_guard, expected);
        for board in 0..8 {
            let expected = masks.iter().all(|&mask| board & mask != 0);
            assert_eq!(
                reverse_rotation_is_first_success_on_free_endpoints(&template, board, source),
                expected,
            );
        }
    }
    // Both earlier targets collide although their intersection stays empty.
    assert_eq!(0b110 & (0b011 & 0b101), 0);
}

#[test]
fn endpoint_conditioning_and_long_imported_prefix_keep_exact_authority() {
    assert_eq!(
        compile_free_endpoint_guard(&[0b101, 0b111], 0b100, 0b010, &[0, 1]),
        SINGLE_BIT_GUARD_BASE,
    );
    let mut masks = vec![0b111; 321];
    masks[300] = 0b011;
    let (template, source) = guard_template(&masks);
    assert_eq!(source.free_endpoint_guard, ORDERED_GUARD);
    assert_eq!(source.prior_target_count, 321);
    for board in 0..8 {
        assert_eq!(
            reverse_rotation_is_first_success_on_free_endpoints(&template, board, source),
            masks.iter().all(|&mask| board & mask != 0),
        );
    }
}

#[test]
fn downward_shadow_matches_scalar_columns_including_partial_top_rows() {
    for width in 1..=16 {
        for board in [0, 1, 1 << 63, 0x0450_30a4_8321_0b80, u64::MAX] {
            let mut expected = board;
            let mut shift = u32::from(width);
            while shift < u64::BITS {
                expected |= board >> shift;
                shift += u32::from(width);
            }
            assert_eq!(downward_occupancy_shadow(board, width), expected);
        }
    }
    // A roof can obstruct entry without colliding with the target itself.
    let roof = 1_u64 << 30;
    let target = 1_u64 << 10;
    assert_eq!(roof & target, 0);
    assert_ne!(downward_occupancy_shadow(roof, 10) & target, 0);
}
