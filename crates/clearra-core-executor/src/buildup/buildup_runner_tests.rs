#![cfg_attr(not(feature = "native-c-core"), allow(dead_code, unused_imports))]

use clearra_core_domain::{
    pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
    probability::probability_value::ProbabilityValue,
};
use clearra_core_ffi::{
    CBuildUpResult, CBuildVariantView, CNativeBuildVariantView, CPackingCandidate,
};
use clearra_coverage::matrix::coverage_row_bridge::CoverageRowBridgeError;
use clearra_coverage::{
    pattern::{
        pattern_bitset::PatternBitSet, pattern_id::PatternId,
        weighted_pattern_set::WeightedPatternSet,
    },
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcHoldPolicy, PcQueueInput, PcScenarioBoard,
    PcScenarioQuery, PieceWindow,
};
use clearra_problem::ProblemCompiler;
use clearra_replay::ReplayEvent;
use clearra_supply::{
    mixed::{BagBoundaryEvidence, SupplyProvenance},
    piece_source::{MaterializedPatternUniverse, PieceSource},
    queue::fixed_sequence::FixedSequence,
};

use crate::buildup::BuildUpRunner;
use crate::{
    buildup::{
        buildup_coverage_bridge::{
            coverage_rows_from_build_variants, coverage_universe_identity,
            CoverageUniverseIdentity, PatternCoverageVerification, PatternVerifiedBuildVariant,
        },
        buildup_native_bridge::{
            buildup_enumeration_limits, buildup_witness_from_c_results, retain_execution_variants,
        },
        buildup_objective_bridge::reduce_objectives,
        buildup_replay_bridge::{
            representative_trace_selection::RepresentativeTraceSelection,
            trace_material_for_execution,
        },
        buildup_trace_retention::trace_key_for_build_variant,
        candidate_execution_aggregate::CandidateExecutionAggregate,
        candidate_execution_aggregate_builder::aggregate_candidate_executions,
        BuildUpExecutionMode, BuildUpRunnerError, ExecutionVariantSet,
    },
    packing::{scenario_packing_witness::ScenarioPackingWitness, PackingRunner},
};

fn owned_build_variant(native: CNativeBuildVariantView) -> CBuildVariantView {
    CBuildVariantView::from_native(&native).expect("owned BuildVariant test view")
}

#[path = "buildup_runner_behavior/coverage_source.rs"]
mod coverage_source;
#[path = "buildup_runner_behavior/execution_retention.rs"]
mod execution_retention;
#[path = "buildup_runner_behavior/native_behavior.rs"]
mod native_behavior;
#[path = "buildup_runner_behavior/objective.rs"]
mod objective;
#[path = "buildup_runner_behavior/replay_trace.rs"]
mod replay_trace;
