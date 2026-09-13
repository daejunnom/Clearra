use core::cell::Cell;

use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};
use clearra_pc_graph::request::{PcExecutionPolicy, PcScenarioBoard, PcScenarioQuery, PieceWindow};
use clearra_problem::compile::problem_compiler::ProblemCompiler;
use clearra_supply::queue::{
    fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression,
};

use super::*;

fn nz(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

fn limits(page: usize) -> Pc4CompiledPatternLimits {
    Pc4CompiledPatternLimits::new(nz(10_000), nz(11), nz(page))
}

fn compile_queue(queue: PcQueueInput, pieces: usize, max_patterns: usize) -> Arc<SearchProblem> {
    // Leave exactly 4*pieces cells free, without a pre-cleared full row.
    let mut occupied_cells = 40 - 4 * pieces;
    let mut initial = 0u64;
    for row in 0..4 {
        let cells = occupied_cells.min(9);
        initial |= ((1u64 << cells) - 1) << (10 * row);
        occupied_cells -= cells;
    }
    assert_eq!(occupied_cells, 0);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, initial),
        queue,
        PieceWindow::new(pieces),
    )
    .with_exact_pieces(Some(pieces))
    .with_allow_hold(false)
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_workers(1)
            .with_max_patterns(max_patterns),
    );
    Arc::new(ProblemCompiler::compile_scenario_pc(&query).expect("compile exact finite pattern"))
}

fn compile(pattern: &str, pieces: usize) -> Arc<SearchProblem> {
    compile_queue(
        PcQueueInput::pattern_expression(QueuePatternExpression::parse(pattern, 10_000).unwrap()),
        pieces,
        10_000,
    )
}

fn prepare(problem: Arc<SearchProblem>, page: usize) -> Pc4CompiledPatternSource {
    let mut preparation = Pc4CompiledPatternPreparation::begin(problem, limits(page)).unwrap();
    while !preparation.advance(nz(page), &|| false).unwrap() {}
    preparation.finish().unwrap()
}

#[test]
fn pc4_compiled_pattern_identity_binds_constraints_not_just_problem_ids() {
    let left = compile("II;IO", 2);
    let right = compile("II;IT", 2);
    // A real collision in the old coarse problem label, not a manufactured ID.
    assert_eq!(left.problem_id(), right.problem_id());
    let left = prepare(left, 1);
    let right = prepare(right, 1);
    assert_eq!(left.pattern_count(), right.pattern_count());
    assert_ne!(left.identity(), right.identity());
    assert_ne!(
        left.read_queue(1).unwrap().pieces(),
        right.read_queue(1).unwrap().pieces()
    );
    // Source-normalization whitespace does not alter the compiled contract.
    assert_eq!(
        left.identity(),
        prepare(compile(" i i ; i o ", 2), 2).identity()
    );
}

#[test]
fn pc4_compiled_pattern_prefix_preserves_duplicate_ordinals_weights_and_denominator() {
    let problem = compile("[IO][TZ]", 1);
    let source = prepare(Arc::clone(&problem), 1);
    assert_eq!(source.pattern_count(), 4);
    assert_eq!(source.sequence_pieces(), 1);
    let queues = (0..4)
        .map(|index| source.read_queue(index).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(queues[0].pieces(), queues[1].pieces());
    assert_eq!(queues[2].pieces(), queues[3].pieces());
    assert_ne!(queues[0].pieces(), queues[2].pieces());
    for (index, queue) in queues.iter().enumerate() {
        assert_eq!(queue.pattern_index(), index);
        assert_eq!(queue.weight().get().to_bits(), 0.25f64.to_bits());
        assert_eq!(queue.weight(), universe(&problem).unwrap().weight_at(index));
    }
    assert_eq!(
        queues.iter().map(|queue| queue.weight().get()).sum::<f64>(),
        1.0
    );
    assert_ne!(
        source.identity(),
        prepare(compile("[IO][TZ]", 2), 4).identity()
    );
    assert!(matches!(
        source.read_queue(4),
        Err(Pc4CompiledPatternError::PatternIndexOutOfBounds)
    ));
}

#[test]
fn pc4_compiled_pattern_p7_remains_lazy_and_identical_for_every_ordinal() {
    let problem = compile("P7", 7);
    let original = universe(&problem).unwrap();
    assert_eq!(original.pattern_count(), 5040);
    assert!(original.lazy_sequence_storage_retained_bytes().is_some());
    let mut preparation =
        Pc4CompiledPatternPreparation::begin(Arc::clone(&problem), limits(64)).unwrap();
    assert_eq!(preparation.audited_patterns(), 0);
    assert!(!preparation.is_complete());
    assert!(!preparation.advance(nz(64), &|| false).unwrap());
    assert_eq!(preparation.audited_patterns(), 64);
    let mut advances = 1;
    while !preparation.advance(nz(64), &|| false).unwrap() {
        advances += 1;
    }
    advances += 1;
    assert_eq!(advances, 79);
    let source = preparation.finish().unwrap();
    assert!(
        Arc::ptr_eq(&source.problem, &problem),
        "retain the exact problem, never clone its universe"
    );
    for index in 0..5040 {
        let queue = source.read_queue(index).unwrap();
        let expected = original
            .sequence_at(index)
            .iter()
            .copied()
            .map(graph_piece)
            .collect::<Vec<_>>();
        assert_eq!(queue.pieces(), expected);
        assert_eq!(queue.weight(), original.weight_at(index));
    }
    assert_eq!(
        source.identity(),
        prepare(Arc::clone(&problem), 7).identity()
    );
}

#[test]
fn pc4_compiled_pattern_page_limit_is_retryable_but_cancellation_is_terminal() {
    let mut preparation =
        Pc4CompiledPatternPreparation::begin(compile("[IO][TZ]", 2), limits(2)).unwrap();
    assert_eq!(
        preparation.advance(nz(3), &|| false),
        Err(Pc4CompiledPatternError::AdvanceLimit {
            limit: 2,
            attempted: 3
        })
    );
    assert_eq!(preparation.audited_patterns(), 0);
    assert!(!preparation.advance(nz(1), &|| false).unwrap());
    let calls = Cell::new(0);
    assert_eq!(
        preparation.advance(nz(2), &|| {
            calls.set(calls.get() + 1);
            calls.get() == 4 // revoke after both records were staged, before commit
        }),
        Err(Pc4CompiledPatternError::Cancelled)
    );
    assert_eq!(preparation.audited_patterns(), 1);
    assert!(!preparation.is_complete());
    assert_eq!(
        preparation.advance(nz(1), &|| false),
        Err(Pc4CompiledPatternError::PreparationTerminated)
    );
    assert!(matches!(
        preparation.finish(),
        Err(Pc4CompiledPatternError::PreparationTerminated)
    ));
}

#[test]
fn pc4_compiled_pattern_no_early_or_cancelled_completion() {
    let preparation = Pc4CompiledPatternPreparation::begin(compile("II;IO", 2), limits(2)).unwrap();
    assert!(matches!(
        preparation.finish(),
        Err(Pc4CompiledPatternError::PreparationIncomplete)
    ));
    let mut preparation =
        Pc4CompiledPatternPreparation::begin(compile("II;IO", 2), limits(2)).unwrap();
    assert!(preparation.advance(nz(2), &|| false).unwrap());
    assert_eq!(
        preparation.advance(nz(1), &|| true),
        Err(Pc4CompiledPatternError::Cancelled)
    );
    assert!(matches!(
        preparation.finish(),
        Err(Pc4CompiledPatternError::PreparationTerminated)
    ));
}

#[test]
fn pc4_compiled_pattern_rejects_truncated_or_out_of_budget_universes_before_reading() {
    let truncated = compile_queue(PcQueueInput::standard_7_bag(), 7, 1);
    assert!(!truncated.piece_source().complete());
    assert!(matches!(
        Pc4CompiledPatternPreparation::begin(truncated, limits(1)),
        Err(Pc4CompiledPatternError::IncompleteUniverse)
    ));
    let problem = compile("P7", 7);
    assert!(matches!(
        Pc4CompiledPatternPreparation::begin(
            Arc::clone(&problem),
            Pc4CompiledPatternLimits::new(nz(5039), nz(7), nz(1))
        ),
        Err(Pc4CompiledPatternError::PatternLimit {
            limit: 5039,
            attempted: 5040
        })
    ));
    assert!(matches!(
        Pc4CompiledPatternPreparation::begin(
            problem,
            Pc4CompiledPatternLimits::new(nz(5040), nz(6), nz(1))
        ),
        Err(Pc4CompiledPatternError::SequencePieceLimit {
            limit: 6,
            attempted: 7
        })
    ));
}

#[test]
fn pc4_compiled_pattern_does_not_relabel_fixed_queues_or_invent_bag_state() {
    let fixed = compile_queue(
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I, PieceKind::I])),
        2,
        10,
    );
    assert!(matches!(
        Pc4CompiledPatternPreparation::begin(fixed, limits(1)),
        Err(Pc4CompiledPatternError::UnsupportedQueueSource)
    ));
    let source = prepare(compile_queue(PcQueueInput::standard_7_bag(), 7, 5040), 64);
    assert_eq!(source.pattern_count(), 5040);
    assert_eq!(
        source.read_queue(0).unwrap().weight().get().to_bits(),
        (1.0 / 5040.0f64).to_bits()
    );
}

#[test]
fn pc4_compiled_pattern_digest_preserves_weight_bits_even_with_reused_numeric_ids() {
    fn digest(weights: [f64; 2]) -> [u8; 32] {
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(11),
            PatternWeightModelId::new(22),
            vec![vec![PieceKind::I], vec![PieceKind::O]],
            weights
                .map(|value| ProbabilityValue::new(value).unwrap())
                .to_vec(),
            2,
            true,
            None,
        )
        .unwrap();
        let mut hasher = Sha256::new();
        for index in 0..2 {
            hash_record(&mut hasher, &universe, index, 1).unwrap();
        }
        hasher.finalize().into()
    }
    assert_ne!(digest([0.25, 0.75]), digest([0.75, 0.25]));
    assert_ne!(digest([0.5, 0.5]), digest([0.0, 1.0]));
}
