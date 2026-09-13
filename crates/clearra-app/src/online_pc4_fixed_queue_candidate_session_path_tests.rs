//! Synthetic end-to-end range-to-candidate row-frame contracts, not HF qualification.
//! SRP rationale: multi-clear App composition is separate from session lifecycle tests.

use clearra_pc4_tablebase::{
    clearra_board64_mask_to_hydra_field_hash_v1, Pc4ArtifactRole, Pc4RuleProfile, Pc4TargetLines,
    Pc4TerminalUseCase, PlacementRotation,
};

use super::{
    tests::{
        activated_snapshot_for_dataset, attempt, canonical_source, hydra_record, index_header,
        nonzero, partial_input, pin, range_limits, Guard, RangeDataset,
    },
    *,
};
use crate::{
    prepare_pc4_input_disclosure, Pc4InputDisclosureDecision, Pc4InputDisclosureRequest,
    Pc4InputSurface, Pc4QueueDisclosure,
};

const ROW: u64 = 1023;

#[path = "pc4_range_product_contract_tests.rs"]
mod product_contracts;

#[path = "online_pc4_observation_candidate_session_tests.rs"]
mod observation_contracts;

struct ClearPath {
    dataset: RangeDataset,
    initial_board: u64,
    ids_by_step: Vec<u32>,
    original_placements: Vec<u64>,
}

fn full_rows(lines: u8) -> u64 {
    (1_u64 << (10 * u32::from(lines))) - 1
}

// All lower rows have a left I hole; the top row has a right I hole.
// The top row clears first, then lower rows clear bottom-up. Sorted graph
// IDs are deliberately not traversal order; normalization can decrease hashes.
fn clear_path(lines: u8) -> ClearPath {
    let lower_rows = |count| (0..count).fold(0, |mask, row| mask | (1008 << (row * 10)));
    let initial_board = lower_rows(lines - 1) | (63 << ((lines - 1) * 10));
    let mut hashes: Vec<_> = (0..=lines)
        .map(|step| {
            let normalized = if step == 0 {
                initial_board
            } else {
                full_rows(step) | (lower_rows(lines - step) << (step * 10))
            };
            (
                clearra_board64_mask_to_hydra_field_hash_v1(normalized).unwrap(),
                step,
            )
        })
        .collect();
    hashes.sort_unstable();
    let mut ids_by_step = vec![0; usize::from(lines) + 1];
    for (id, (_, step)) in hashes.iter().enumerate() {
        ids_by_step[usize::from(*step)] = id as u32;
    }
    let count = hashes.len() as u32;
    let mut field_index = index_header(*b"FHIDIDX1", count);
    let mut graph_offsets = index_header(*b"GOFFIDX1", count);
    let mut graph = Vec::new();
    for (id, (hash, step)) in hashes.into_iter().enumerate() {
        field_index.extend_from_slice(&hash.to_le_bytes()[..5]);
        field_index.extend_from_slice(&(id as u32).to_le_bytes()[..3]);
        graph_offsets.extend_from_slice(&(graph.len() as u32).to_le_bytes());
        let targets = if step == lines {
            Vec::new()
        } else {
            vec![ids_by_step[usize::from(step) + 1]]
        };
        graph.extend(hydra_record(hash, [&targets, &[], &[], &[], &[], &[], &[]]));
    }
    graph_offsets.extend_from_slice(&(graph.len() as u32).to_le_bytes());
    let original_placements = std::iter::once(960 << ((lines - 1) * 10))
        .chain((0..lines - 1).map(|row| 15 << (row * 10)))
        .collect();
    ClearPath {
        dataset: RangeDataset {
            field_index,
            graph_offsets,
            graph,
        },
        initial_board,
        ids_by_step,
        original_placements,
    }
}

fn assert_clear_path(lines: u8) {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let fixture = clear_path(lines);
    let target_lines = Pc4TargetLines::new(lines).unwrap();
    for profile in Pc4RuleProfile::ALL {
        // A synthetic qualified slot tests composition only, not the format,
        // edge completeness or kick provenance of a real uploaded profile.
        let generation = pin(activated_snapshot_for_dataset(
            "multi-clear-fixture",
            Some(profile),
            target_lines,
            u32::from(lines) + 1,
            *fixture.ids_by_step.last().unwrap(),
            &fixture.dataset,
        ));
        let target = generation
            .activated_snapshot()
            .qualified_target(profile, Pc4TerminalUseCase::PcSearch, target_lines)
            .unwrap();
        let prepared = match prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
            target,
            Pc4InputSurface::Gui,
            Pc4QueueDisclosure::FixedExplicit(vec![Pc4GraphPiece::I; usize::from(lines)]),
        ))
        .unwrap()
        {
            Pc4InputDisclosureDecision::Ready(prepared) => prepared,
            other => panic!("explicit queue must be ready: {other:?}"),
        };
        let board = StandardPcBoard::from_words(lines, [fixture.initial_board, 0, 0, 0]).unwrap();
        let source = canonical_source(&prepared, board, FixedQueueHoldState::Disabled);
        let guard = Guard::new(source.clone());
        let request = AppOnlinePc4FixedQueueCandidateRequest::for_prepared_input(
            &source,
            &prepared,
            board,
            FixedQueueHoldState::Disabled,
            LookupSessionId::new(1).unwrap(),
            fixture.ids_by_step[0],
            TerminalDepthContract::QueueExhaustedOnly,
            FixedQueueTraversalBudgets::new(
                nonzero(64),
                nonzero(64),
                nonzero(usize::from(lines)),
                nonzero(16),
            ),
            FixedQueueTraversalPageBudgets::new(nonzero(64), nonzero(64)),
            ConcretePathMaterializationBudgets::new(
                nonzero(usize::from(lines)),
                nonzero(16),
                nonzero(64),
                nonzero(64),
            ),
            Pc4GraphCandidateAdapterBudgets::new(
                nonzero(1),
                nonzero(1),
                nonzero(16),
                nonzero(64),
                nonzero(256),
                nonzero(1),
            ),
            Pc4LookupGraphCacheLimits::new(nonzero(5), nonzero(1024), nonzero(1024)),
            nonzero(1),
            range_limits(),
        )
        .unwrap();
        let mut session =
            AppOnlinePc4FixedQueueCandidateSession::start(generation, request, &guard).unwrap();
        let mut active_lookup = None;
        let mut ordinal = 0;
        let mut completed = false;
        let mut progress_pages = 0;
        for _ in 0..512 {
            match session.step(&guard) {
                AppOnlinePc4FixedQueueCandidateStep::NeedRange(request) => {
                    assert!(session.completed_reducer_input().is_none());
                    if active_lookup != Some(request.lookup_session()) {
                        active_lookup = Some(request.lookup_session());
                        ordinal = 0;
                    }
                    ordinal += 1;
                    session
                        .admit_range(
                            attempt(ordinal),
                            partial_input(&request, &fixture.dataset),
                            &guard,
                        )
                        .unwrap();
                }
                AppOnlinePc4FixedQueueCandidateStep::Progress { .. } => {
                    progress_pages += 1;
                    assert!(session.completed_reducer_input().is_none());
                }
                AppOnlinePc4FixedQueueCandidateStep::Complete {
                    canonical_candidates,
                    ..
                } => {
                    assert_eq!(canonical_candidates, 1, "{lines}L {profile:?}");
                    completed = true;
                    break;
                }
                other => panic!("{lines}L {profile:?}: {other:?}"),
            }
        }
        assert!(completed, "bounded fixture stalled: {lines}L {profile:?}");
        let reducer = session.completed_reducer_input().unwrap();
        let candidate = reducer.candidates()[0];
        let mut expected_masks = fixture.original_placements.clone();
        expected_masks.sort_unstable();
        let mut actual_masks = candidate.placement_masks().to_vec();
        actual_masks.sort_unstable();
        assert_eq!(
            actual_masks, expected_masks,
            "fixed original frame: {lines}L {profile:?}"
        );
        assert_eq!(candidate.initial_board_mask(), fixture.initial_board);
        assert_eq!(
            actual_masks
                .iter()
                .fold(fixture.initial_board, |board, mask| {
                    assert_eq!(
                        board & mask,
                        0,
                        "initial cells and placements cannot overlap"
                    );
                    board | mask
                }),
            full_rows(lines)
        );

        let family = session.runtime.completed_candidate_family().unwrap();
        assert_eq!(family.candidates().len(), 1);
        let replays = family.candidates()[0].replay_provenances();
        assert!(!replays.is_empty());
        if replays.len() > 1 {
            assert!(
                progress_pages > 0,
                "one-path paging must preserve the row frame"
            );
        }
        for replay in replays {
            assert_eq!(replay.start_field_id(), fixture.ids_by_step[0]);
            assert_eq!(replay.target_field_ids(), &fixture.ids_by_step[1..]);
            assert_eq!(replay.placements().len(), usize::from(lines));
            let mut physical_board = fixture.initial_board;
            for (step, placement) in replay.placements().iter().enumerate() {
                assert_eq!(placement.piece(), Pc4GraphPiece::I);
                assert_eq!(
                    placement.occupied_cells(),
                    fixture.original_placements[step]
                );
                let physical_lock = if step == 0 {
                    960 << ((lines - 1) * 10)
                } else {
                    15
                };
                assert!(matches!(
                    placement.rotation(),
                    PlacementRotation::Zero | PlacementRotation::Two
                ));
                assert_eq!(
                    15_u64 << (u32::from(placement.y()) * 10 + u32::from(placement.x())),
                    physical_lock,
                    "replay poses stay physical while candidate masks stay in the original frame"
                );
                assert_eq!(physical_board & physical_lock, 0);
                let placed = physical_board | physical_lock;
                physical_board = 0;
                let mut output_row = 0;
                let mut clears = 0;
                for row in 0..lines {
                    let cells = (placed >> (row * 10)) & ROW;
                    if cells == ROW {
                        clears += 1;
                    } else {
                        physical_board |= cells << (output_row * 10);
                        output_row += 1;
                    }
                }
                assert_eq!(clears, 1);
            }
            assert_eq!(
                physical_board, 0,
                "every retained provenance must replay to PC"
            );
        }
        product_contracts::assert_product_parity(
            lines,
            profile,
            fixture.initial_board,
            reducer,
            &guard,
        );
        let candidate_allocation = reducer.candidates().as_ptr();
        let owned_input = session.into_completed_reducer_input(&guard).unwrap();
        assert_eq!(
            owned_input.candidates().as_ptr(),
            candidate_allocation,
            "completion moves, rather than clones, the candidate allocation"
        );
        product_contracts::assert_owned_score_product_parity(
            lines,
            profile,
            fixture.initial_board,
            owned_input,
            &guard,
        );
    }
}

#[test]
fn pc4_one_line_range_to_candidate_and_replay() {
    assert_clear_path(1);
}

#[test]
fn pc4_two_line_upper_clear_range_to_candidate_and_replay() {
    assert_clear_path(2);
}

#[test]
fn pc4_three_line_upper_clear_range_to_candidate_and_replay() {
    assert_clear_path(3);
}

#[test]
fn pc4_four_line_upper_clear_range_to_candidate_and_replay() {
    assert_clear_path(4);
}

#[test]
fn pc4_host_transport_runs_fixed_and_pattern_requests_through_the_shared_product() {
    use crate::{AppContext, AppStatus, CooperativeAppAdvance};
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let fixture = clear_path(4);
    let snapshot = activated_snapshot_for_dataset(
        "host-transport-fixture",
        Some(Pc4RuleProfile::Jstris180),
        Pc4TargetLines::new(4).unwrap(),
        5,
        *fixture.ids_by_step.last().unwrap(),
        &fixture.dataset,
    );
    let fixed = product_contracts::request(
        4,
        Pc4RuleProfile::Jstris180,
        fixture.initial_board,
        product_contracts::Product::All,
    );
    let pattern = product_contracts::pattern_request(
        4,
        Pc4RuleProfile::Jstris180,
        fixture.initial_board,
        product_contracts::Product::All,
        "IIII",
        FixedQueueHoldState::Disabled,
    );
    for request in [fixed, pattern] {
        let context = AppContext::default();
        let control = clearra_core_domain::execution_cancellation::ExecutionControl::default();
        let mut execution = context
            .start_online_pc4_execution(request, snapshot.clone())
            .unwrap();
        let mut completed = false;
        let mut transfers = 0;
        for _ in 0..10_000 {
            match execution.advance(64, &control).unwrap() {
                CooperativeAppAdvance::Completed(response) => {
                    assert_eq!(response.status(), AppStatus::Success);
                    completed = true;
                    break;
                }
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                _ => panic!("unexpected host execution termination"),
            }
            if let Some(range) = execution.pending_range() {
                let data = match range.artifact() {
                    Pc4ArtifactRole::FieldHashIndex => &fixture.dataset.field_index,
                    Pc4ArtifactRole::GraphOffsets => &fixture.dataset.graph_offsets,
                    Pc4ArtifactRole::Graph => &fixture.dataset.graph,
                };
                let offset = range.offset() as usize;
                let length = range.length() as usize;
                let bytes = data[offset..offset + length].to_vec();
                let session = range.lookup_session().get();
                let id = range.request_id();
                let header = format!("bytes {}-{}/{}", offset, offset + length - 1, data.len());
                assert!(execution
                    .admit_range(
                        session,
                        id + 1,
                        206,
                        Some(header.clone()),
                        bytes.clone(),
                        &control
                    )
                    .is_err());
                execution
                    .admit_range(session, id, 206, Some(header), bytes, &control)
                    .unwrap();
                transfers += 1;
            }
        }
        assert!(completed, "host transport must not wait indefinitely");
        assert!(
            transfers > 0,
            "success must have consumed online graph bytes"
        );
    }
}
