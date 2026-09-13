// These are pure fixed-queue/hold union contracts, not upstream qualification
// or a claim that the HTTP runtime already routes observation families.
use super::*;

const I_COLUMN: u64 = 1 | (1 << 10) | (1 << 20) | (1 << 30);

struct ColumnMaterializer(Pc4RuleProfile);

impl Pc4PlacementMaterializer for ColumnMaterializer {
    type Error = Infallible;

    fn profile(&self) -> Pc4RuleProfile {
        self.0
    }

    fn enumerate(
        &mut self,
        edge: &QualifiedPc4GraphEdge,
    ) -> Result<MaterializationOutput, Self::Error> {
        assert_eq!(edge.piece(), Pc4GraphPiece::I);
        Ok(MaterializationOutput {
            snapshot: edge.snapshot().clone(),
            profile: edge.profile(),
            source_field_id: edge.source_field_id(),
            piece: edge.piece(),
            target_field_id: edge.target_field_id(),
            placements: vec![ClearraPlacementIdentity::new(
                Pc4GraphPiece::I,
                PlacementRotation::Right,
                0,
                0,
                I_COLUMN,
            )
            .expect("vertical I")],
        })
    }
}

fn prepared_fixed(
    target: &QualifiedPc4TargetIdentity,
    queue: &[Pc4GraphPiece],
) -> Pc4PreparedOnlineInput {
    match prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
        target.clone(),
        Pc4InputSurface::Cli,
        Pc4QueueDisclosure::FixedExplicit(queue.to_vec()),
    ))
    .expect("explicit queue input")
    {
        Pc4InputDisclosureDecision::Ready(prepared) => prepared,
        _ => panic!("fixed queue cannot request bag disclosure"),
    }
}

fn initial_board(placements: usize) -> StandardPcBoard {
    let holes = if placements == 1 {
        I_COLUMN
    } else {
        I_COLUMN | (I_COLUMN << 1)
    };
    StandardPcBoard::from_words(4, [((1_u64 << 40) - 1) ^ holes, 0, 0, 0]).expect("column wells")
}

fn fixed_frontier(
    queue: &[Pc4GraphPiece],
    hold: FixedQueueHoldState,
    placements: usize,
) -> clearra_pc4_tablebase::Pc4ObservationFrontierFamily {
    prepare_pc4_observation_frontier(
        Pc4ObservationFrontierRequest::fixed_queue(queue, hold, placements, frontier_budgets()),
        &|| false,
    )
    .expect("bag-free finite queue")
}

#[test]
fn fixed_hold_choices_seal_one_complete_outcome_in_every_profile_and_page_size() {
    for profile in Pc4RuleProfile::ALL {
        let target = target_for_profile(profile, Pc4TerminalUseCase::PcSearch);
        for (queue, hold, placements, decision) in [
            (
                vec![Pc4GraphPiece::I],
                FixedQueueHoldState::Disabled,
                1,
                Some(FixedQueueHoldDecision::UseCurrent),
            ),
            (
                vec![Pc4GraphPiece::O, Pc4GraphPiece::I],
                FixedQueueHoldState::Empty,
                1,
                Some(FixedQueueHoldDecision::StoreCurrentUseNext),
            ),
            (
                vec![Pc4GraphPiece::O],
                FixedQueueHoldState::Occupied(Pc4GraphPiece::I),
                1,
                Some(FixedQueueHoldDecision::SwapHeld),
            ),
            (
                vec![Pc4GraphPiece::I],
                FixedQueueHoldState::Occupied(Pc4GraphPiece::O),
                1,
                Some(FixedQueueHoldDecision::UseCurrent),
            ),
            (
                vec![Pc4GraphPiece::I],
                FixedQueueHoldState::Occupied(Pc4GraphPiece::I),
                1,
                Some(FixedQueueHoldDecision::UseCurrent),
            ),
            (
                vec![Pc4GraphPiece::O],
                FixedQueueHoldState::Disabled,
                1,
                None,
            ),
            // Hold cannot create a second current piece after the queue ends.
            (
                vec![Pc4GraphPiece::O],
                FixedQueueHoldState::Occupied(Pc4GraphPiece::I),
                2,
                None,
            ),
        ] {
            let prepared = prepared_fixed(&target, &queue);
            let source =
                source_with_board_and_hold(&target, &prepared, initial_board(placements), hold);
            let guard = Guard::new(source.clone());
            let mut previous_candidates: Option<Vec<_>> = None;
            for page_size in [1, 8] {
                let graph = graph_family_from_frontier(
                    &target,
                    &guard,
                    fixed_frontier(&queue, hold, placements),
                );
                let prepare_session = || {
                    prepare_pc4_observation_candidate_session(
                        Pc4ObservationCandidateAdapterRequest::new(
                            &target,
                            &prepared,
                            &source,
                            0,
                            materialization_budgets(),
                            adapter_budgets(16),
                        ),
                        &graph,
                        &guard,
                    )
                    .expect("fixed candidate session")
                };
                assert!(
                    prepare_session().finish(&guard).is_err(),
                    "an unfinished union is not complete"
                );
                let mut session = prepare_session();
                let mut provider = provider(&target);
                let mut terminal = ManifestQualifiedPc4ObservationTerminal::new(target.clone());
                let mut materializer = ColumnMaterializer(profile);
                while !session.is_exhausted() {
                    session
                        .advance(
                            nonzero(page_size),
                            &mut provider,
                            &mut terminal,
                            &mut materializer,
                            &guard,
                        )
                        .expect("complete fixed hold union");
                }
                let family = session.finish(&guard).expect("all hold branches completed");
                assert_eq!(family.source(), &source);
                assert_eq!(
                    family.total_reveal_probability(),
                    Pc4ExactProbability::one()
                );
                assert_eq!(family.reveal_outcomes().len(), 1);
                let outcome = &family.reveal_outcomes()[0];
                assert_eq!(outcome.reveal().reveal_rank(), 0);
                assert!(outcome.reveal().revealed_pieces().is_empty());
                assert_eq!(outcome.reveal().terminal_bag_state(), None);
                assert_eq!(outcome.candidates().len(), usize::from(decision.is_some()));
                if let Some(decision) = decision {
                    assert_eq!(outcome.candidates()[0].provenances().len(), 1);
                    assert_eq!(
                        outcome.candidates()[0].provenances()[0].hold_steps()[0].decision(),
                        decision
                    );
                }
                let reducer = family.reducer_input().expect("complete reducer input");
                assert_eq!(
                    reducer.universe_identity().request_identity(),
                    source.request_identity()
                );
                assert_eq!(reducer.candidates(), family.canonical_candidates());
                if let Some(previous) = previous_candidates.as_ref() {
                    assert_eq!(reducer.candidates(), previous.as_slice());
                }
                previous_candidates = Some(reducer.candidates().to_vec());
            }
        }
    }
}

#[test]
fn fixed_union_cannot_change_queue_hold_or_truncate_the_board_placement_horizon() {
    let target = target(Pc4TerminalUseCase::PcSearch);
    let prepared = prepared_fixed(&target, &[Pc4GraphPiece::I]);
    let source = source_with_board_and_hold(
        &target,
        &prepared,
        initial_board(1),
        FixedQueueHoldState::Disabled,
    );
    let guard = Guard::new(source.clone());
    for (frontier, expected) in [
        (
            fixed_frontier(&[Pc4GraphPiece::I], FixedQueueHoldState::Disabled, 0),
            Pc4ObservationCandidateBindingError::QueueScopeMismatch,
        ),
        (
            fixed_frontier(&[Pc4GraphPiece::I], FixedQueueHoldState::Disabled, 2),
            Pc4ObservationCandidateBindingError::QueueScopeMismatch,
        ),
        (
            fixed_frontier(&[Pc4GraphPiece::O], FixedQueueHoldState::Disabled, 1),
            Pc4ObservationCandidateBindingError::QueueScopeMismatch,
        ),
        (
            fixed_frontier(&[Pc4GraphPiece::I], FixedQueueHoldState::Empty, 1),
            Pc4ObservationCandidateBindingError::RequestIdentityMismatch,
        ),
        (
            frontier_with(1, FixedQueueHoldState::Disabled, 1),
            Pc4ObservationCandidateBindingError::QueueScopeMismatch,
        ),
    ] {
        let graph = graph_family_from_frontier(&target, &guard, frontier);
        assert!(matches!(prepare_pc4_observation_candidate_session(
            Pc4ObservationCandidateAdapterRequest::new(&target, &prepared, &source, 0,
                materialization_budgets(), adapter_budgets(16)), &graph, &guard,
        ), Err(Pc4ObservationCandidateError::Binding(actual)) if actual == expected));
    }
}
