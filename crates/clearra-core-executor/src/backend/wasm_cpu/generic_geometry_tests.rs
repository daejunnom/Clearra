use std::collections::BTreeSet;

use clearra_core_domain::board::board_size::BoardSize;
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

use super::*;

type Key = Vec<(PieceKind, u64)>;

fn count(piece: PieceKind, amount: usize) -> PieceMultisetKey {
    PieceMultisetKey::from_pieces(std::iter::repeat_n(piece, amount))
}

fn request<'a>(
    width: u16,
    height: u16,
    initial: u64,
    required: u64,
    inventory: PieceMultisetKey,
) -> GeometryRequest<'a> {
    let layout = Board64Layout::new(BoardSize::new(width, height).unwrap()).unwrap();
    GeometryRequest {
        layout,
        initial,
        region: layout.all_cells_mask(),
        required,
        forbidden: 0,
        max_pieces: inventory.total_count(),
        exact_pieces: None,
        inventory,
        exact_family: None,
        terminal: ProjectedTerminal::Any,
        selection: CoverSelection::All,
    }
}

fn key(cover: &GeometryCover<'_>) -> Key {
    cover
        .representatives()
        .map(|row| (row.piece, row.target_cells))
        .collect()
}

#[test]
fn generic_geometry_keeps_inverse_rows_and_symmetric_rotations_without_duplicates() {
    // This 2x5 frame admits only vertical I ownerships, each jumping original
    // row 2. It is a geometry result even if no actual clear/lock order exists.
    let mut search = request(2, 5, 0, 0x3cf, count(PieceKind::I, 2));
    search.region = 0x3df; // one optional cell on row 2
    search.forbidden = 0x020;
    search.terminal = ProjectedTerminal::BoardAfterLineClears(0);
    let mut seen = Vec::new();
    let receipt = stream_geometry(
        search,
        GeometryLimits::default(),
        &ExecutionCancellationToken::new(),
        |cover| {
            seen.push(key(&cover));
            assert_eq!(cover.projected_board, 0);
            assert_eq!(cover.projected_deleted_rows, 0b11011);
            assert_eq!(cover.realizations(0).unwrap().len(), 2);
            for row in cover.realizations(0).unwrap() {
                assert_eq!(row.minimum_deleted_rows, 0b00100);
                assert!(row.instantiate(search.layout, 0).is_none());
                assert!(row.instantiate(search.layout, 0b00101).is_none());
                assert!(row.instantiate(search.layout, 0b10100).is_none());
                let lock = row.instantiate(search.layout, 0b00100).unwrap();
                assert_eq!((lock.mask, lock.x, lock.y), (0x55, 0, 0));
            }
            assert!(cover.realizations(2).is_none());
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert_eq!(receipt.end, GeometryEnd::Complete);
    assert!(receipt.resource.result_complete());
    assert_eq!(seen, [vec![(PieceKind::I, 0x145), (PieceKind::I, 0x28a)]]);
}

#[test]
fn generic_geometry_accepts_a_wide_board64_not_supported_by_pc_projections() {
    let mut search = request(12, 2, 1 << 23, 0xf00, count(PieceKind::I, 1));
    search.region = 0xf01;
    search.forbidden = 0x2;
    search.terminal = ProjectedTerminal::BoardAfterLineClears(0x800f00);
    let mut seen = Vec::new();
    let receipt = stream_geometry(
        search,
        GeometryLimits::default(),
        &ExecutionCancellationToken::new(),
        |cover| {
            assert_eq!(cover.projected_board, 0x800f00);
            seen.push(key(&cover));
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert_eq!(receipt.end, GeometryEnd::Complete);
    assert_eq!(seen, [vec![(PieceKind::I, 0xf00)]]);
    search.terminal = ProjectedTerminal::BoardAfterLineClears(0);
    assert_eq!(
        stream_geometry(
            search,
            GeometryLimits::default(),
            &ExecutionCancellationToken::new(),
            |_| {
                panic!("wrong terminal must not emit a cover");
            }
        )
        .unwrap()
        .emitted,
        0
    );
}

#[test]
fn generic_geometry_full_width_row_has_no_shift_overflow() {
    let search = request(64, 1, !0xf, 0xf, count(PieceKind::I, 1));
    let receipt = stream_geometry(
        search,
        GeometryLimits::default(),
        &ExecutionCancellationToken::new(),
        |cover| {
            assert_eq!(
                (cover.projected_board, cover.projected_deleted_rows),
                (0, 1)
            );
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert_eq!(receipt.emitted, 1);
}

#[test]
fn generic_geometry_complete_and_incomplete_families_are_distinct() {
    let mut search = request(4, 4, 0, 0xffff, count(PieceKind::I, 4));
    let cancellation = ExecutionCancellationToken::new();
    let none = |_: GeometryCover<'_>| ControlFlow::Continue(());
    let complete = stream_geometry(search, GeometryLimits::default(), &cancellation, none).unwrap();
    assert_eq!((complete.end, complete.emitted), (GeometryEnd::Complete, 2));
    assert!(complete.resource.peak_cpu_bytes <= complete.engine_bytes_upper_bound.unwrap());

    let exhausted = stream_geometry(
        search,
        GeometryLimits {
            candidates: Some(1),
            ..Default::default()
        },
        &cancellation,
        none,
    )
    .unwrap();
    assert_eq!(exhausted.emitted, 1);
    assert_eq!(
        exhausted.end,
        GeometryEnd::Resource(ResourceTruncationReason::CandidateBudgetExceeded)
    );
    assert!(!exhausted.resource.result_complete());

    let node_limited = stream_geometry(
        search,
        GeometryLimits {
            nodes: Some(0),
            ..Default::default()
        },
        &cancellation,
        none,
    )
    .unwrap();
    assert_eq!(node_limited.expanded_nodes, 0);
    assert_eq!(
        node_limited.end,
        GeometryEnd::Resource(ResourceTruncationReason::FrontierBudgetExceeded)
    );

    let rejected = stream_geometry(
        search,
        GeometryLimits {
            engine_bytes: Some(0),
            ..Default::default()
        },
        &cancellation,
        none,
    )
    .unwrap();
    assert_eq!(
        rejected.end,
        GeometryEnd::Resource(ResourceTruncationReason::MemoryExceeded)
    );
    assert_eq!(rejected.resource.peak_cpu_bytes, 0);

    let stopped = stream_geometry(search, GeometryLimits::default(), &cancellation, |_| {
        ControlFlow::Break(())
    })
    .unwrap();
    assert_eq!(stopped.end, GeometryEnd::SinkStopped);
    assert!(!stopped.resource.result_complete());

    let handle = cancellation.handle();
    let cancelled = stream_geometry(search, GeometryLimits::default(), &cancellation, |_| {
        handle.cancel();
        ControlFlow::Continue(())
    })
    .unwrap();
    assert_eq!(cancelled.end, GeometryEnd::Cancelled);
    assert_eq!(cancelled.emitted, 1);

    search.selection = CoverSelection::First;
    let first = stream_geometry(
        search,
        GeometryLimits::default(),
        &ExecutionCancellationToken::new(),
        none,
    )
    .unwrap();
    assert_eq!((first.end, first.emitted), (GeometryEnd::FirstSolution, 1));
    assert!(!first.resource.result_complete());
}

#[test]
fn generic_geometry_matches_deleted_row_then_fixed_anchor_oracle() {
    let family = [count(PieceKind::I, 4), count(PieceKind::O, 4)];
    let mixed = PieceMultisetKey::from_counts([4, 4, 0, 0, 0, 0, 0]);
    let mut search = request(4, 4, 0, 0xffff, mixed);
    search.max_pieces = 4;
    search.exact_family = Some(&family);
    let mut observed = BTreeSet::new();
    let receipt = stream_geometry(
        search,
        GeometryLimits::default(),
        &ExecutionCancellationToken::new(),
        |cover| {
            assert!(observed.insert(key(&cover)));
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    let expected = oracle(search);
    assert_eq!(receipt.end, GeometryEnd::Complete);
    // Two I covers. The two independent column pairs admit three possible
    // original-row pairings each for O covers (9), irrespective of BuildUp.
    assert_eq!(expected.len(), 11);
    assert_eq!(observed, expected);
}

fn oracle(search: GeometryRequest<'_>) -> BTreeSet<Key> {
    // Independent enumeration direction: enumerate actual deletion states
    // and physical anchors first, then reinsert logical rows at their
    // original positions. The production geometry enumerates inverse row
    // assignments directly without first selecting a clear state.
    let width = search.layout.width() as u8;
    let height = search.layout.height() as u8;
    let shapes = standard_tetromino_registry();
    let mut placeable = BTreeSet::new();
    for deleted in 0..1_u16 << height {
        let survivors: Vec<_> = (0..height)
            .filter(|row| deleted & (1 << row) == 0)
            .collect();
        for piece in PieceKind::STANDARD_TETROMINOES {
            if search.inventory.count(piece) == 0 {
                continue;
            }
            for rotation in RotationState::ALL {
                let shape = shapes.get(piece).unwrap().shape(rotation);
                for x in 0..width {
                    for y in 0..survivors.len() {
                        let mut mask = 0;
                        for cell in shape.cells() {
                            let tx = x + cell.x() as u8;
                            let Some(&ty) = survivors.get(y + cell.y() as usize) else {
                                break;
                            };
                            if tx >= width {
                                break;
                            }
                            mask |= 1_u64 << (ty * width + tx);
                        }
                        if mask.count_ones() == 4
                            && mask & !(search.required & !search.initial) == 0
                        {
                            placeable.insert((piece, mask));
                        }
                    }
                }
            }
        }
    }
    let placements: Vec<_> = placeable.into_iter().collect();
    let mut result = BTreeSet::new();
    for member in search.exact_family.unwrap() {
        oracle_cover(
            &placements,
            search.required & !search.initial,
            member.counts(),
            &mut Vec::new(),
            &mut result,
        );
    }
    result
}

fn oracle_cover(
    placements: &[(PieceKind, u64)],
    remaining: u64,
    mut counts: [u8; 7],
    path: &mut Key,
    result: &mut BTreeSet<Key>,
) {
    if remaining == 0 {
        let mut key = path.clone();
        key.sort_unstable();
        result.insert(key);
        return;
    }
    let pivot = 1_u64 << remaining.trailing_zeros();
    for &(piece, mask) in placements {
        let piece_slot = piece_index(piece);
        if mask & pivot != 0 && mask & !remaining == 0 && counts[piece_slot] > 0 {
            counts[piece_slot] -= 1;
            path.push((piece, mask));
            oracle_cover(placements, remaining ^ mask, counts, path, result);
            path.pop();
            counts[piece_slot] += 1;
        }
    }
}

#[cfg(feature = "native-c-core")]
#[test]
fn generic_geometry_matches_native_c_custom_regions() {
    use clearra_core_domain::pc::pc_target::PcTarget;
    use clearra_core_ffi::{CPackingProblemBuilder, CoreCNative};
    use clearra_pc_graph::request::{OpeningPcSearchQuery, PcHoldPolicy, PcQueueInput};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::I,
            PieceKind::I,
            PieceKind::I,
        ])))
        .with_hold_policy(PcHoldPolicy::Disabled);
    let seed = ProblemCompiler::compile_opening_pc(&query).unwrap();
    let mut native = CPackingProblemBuilder::from_search_problem(&seed).unwrap();
    native.board.width = 4;
    native.board.visible_height = 4;
    native.board.search_height = 4;
    native.board.cell_count = 16;
    native.board.initial_mask = 0x00f0;
    native.required_fill_mask = 0xff0f;
    native.goal_region_mask = 0xff0f;
    native.forbidden_mask = 0;
    native.max_pieces = 3;
    native.exact_pieces = 3;
    native.piece_window.max_pieces = 3;
    native.piece_window.exact_pieces = 3;
    native.piece_window.has_exact_pieces = 1;
    native.piece_multiset_window.counts[1] = 3;
    native.piece_multiset_window.total_count = 3;
    native.piece_multiset_window.exact_count = 3;
    native.piece_multiset_family.count = 1;
    native.piece_multiset_family.complete = 1;
    native.piece_multiset_family.members[0] = native.piece_multiset_window;
    let cancellation = ExecutionCancellationToken::new();
    let catalog =
        CoreCNative::compile_geometry_catalog_with_cancellation(&native, &cancellation).unwrap();
    let result = catalog
        .generate_partition(&native, 0, 1, 0, 1, 1, &cancellation)
        .unwrap();
    assert!(result.count_complete());
    let mut baseline = BTreeSet::new();
    for row in result.candidates.iter() {
        let mut pieces = row.operations[..usize::from(row.operation_count)]
            .iter()
            .map(|op| {
                let piece = PieceKind::STANDARD_TETROMINOES[usize::from(op.piece) - 1];
                (piece, op.mask)
            })
            .collect::<Key>();
        pieces.sort_unstable();
        assert_eq!((row.final_board, row.cleared_lines), (0, 4));
        baseline.insert(pieces);
    }
    let mut custom = request(4, 4, 0x00f0, 0xff0f, count(PieceKind::I, 3));
    custom.region = 0xff0f;
    let mut rust = BTreeSet::new();
    let result = stream_geometry(custom, GeometryLimits::default(), &cancellation, |cover| {
        assert_eq!(
            (
                cover.projected_board,
                cover.projected_deleted_rows.count_ones()
            ),
            (0, 4)
        );
        rust.insert(key(&cover));
        ControlFlow::Continue(())
    })
    .unwrap();
    assert_eq!(result.end, GeometryEnd::Complete);
    assert_eq!(baseline.len(), 1);
    assert_eq!(baseline, rust);
}
