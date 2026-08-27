use clearra_app::{
    AppCommand, PcMinimalsIngressOrigin, PcPathIngressOrigin, PcResultProjection,
    PcSaveIngressOrigin, PcScoreIngressOrigin, PcScoreMinimalsIngressOrigin, PcTilingIngressOrigin,
    PercentAppCommand, ScenarioAppCommand,
};
use clearra_core_domain::{objective::objective_kind::ObjectiveKind, piece::piece_kind::PieceKind};
use clearra_pc_graph::request::{
    PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy,
    PieceWindow,
};
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

use crate::{
    model::{GuiBackendForm, GuiScenarioPcForm},
    request::{
        execution_constraint_objective_policy, parse_piece_sequence, parse_queue_pattern,
        parse_rule_profile, score_objective_policy, BackendRequestBuilder, RequestBuildError,
        RequestBuildErrorCode,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScenarioRequestBuilder;

impl ScenarioRequestBuilder {
    pub fn build_command(
        form: &GuiScenarioPcForm,
        backend: &GuiBackendForm,
    ) -> Result<AppCommand, RequestBuildError> {
        if form.visible_height() == 0 {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::InvalidLineCount,
                "GUI scenario form requires a positive visible height",
            ));
        }
        let standard_bag_pattern = form
            .remaining_queue_is_pattern()
            .then(|| {
                QueuePatternExpression::standard_7_bag_with_optional_leading_piece(
                    form.remaining_queue(),
                )
            })
            .flatten();
        let leading_supply_piece = standard_bag_pattern
            .and_then(|(leading_piece, _)| leading_piece)
            .filter(|_| form.allow_hold() && form.hold_piece().is_none());
        let finite_standard_bag_len = standard_bag_pattern.and_then(|(leading_piece, length)| {
            (leading_piece.is_none() || leading_supply_piece.is_some()).then_some(length)
        });
        let queue = if form.remaining_queue_is_standard_bag() || finite_standard_bag_len.is_some() {
            PcQueueInput::standard_7_bag()
        } else if form.remaining_queue_is_pattern() {
            PcQueueInput::pattern_expression(parse_queue_pattern(
                form.remaining_queue(),
                backend.pattern_budget() as usize,
                "scenario queue pattern",
            )?)
        } else {
            PcQueueInput::fixed_sequence(parse_piece_sequence(
                form.remaining_queue(),
                "scenario remaining queue",
            )?)
        };
        let hold_piece = form
            .hold_piece()
            .map(PieceKind::from_ascii)
            .transpose()
            .map_err(|error| {
                RequestBuildError::new(
                    RequestBuildErrorCode::UnknownPiece,
                    format!("invalid GUI scenario hold piece: {error:?}"),
                )
            })?
            .or(leading_supply_piece);
        let mut count_policy = match form.count_policy() {
            "unique" | "count-unique" => PcCountPolicy::CountUnique,
            "all" | "count-all" => PcCountPolicy::CountAll,
            value => {
                return Err(RequestBuildError::new(
                    RequestBuildErrorCode::ValidationFailed,
                    format!("invalid GUI scenario count policy '{value}'"),
                ))
            }
        };
        let canonical_pc_path = form.score_mode() == "path";
        let canonical_pc_saves = form.score_mode() == "saves";
        let canonical_pc_best_save = form.score_mode() == "best-save";
        let canonical_pc_save = canonical_pc_saves || canonical_pc_best_save;
        let base_objective = if canonical_pc_save || canonical_pc_path {
            clearra_objectives::policy::objective_policy::ObjectivePolicy::all()
        } else {
            match count_policy {
                PcCountPolicy::CountAll => {
                    clearra_objectives::policy::objective_policy::ObjectivePolicy::all()
                }
                PcCountPolicy::FirstSolution | PcCountPolicy::CountUnique => {
                    clearra_objectives::policy::objective_policy::ObjectivePolicy::unique()
                }
            }
        };
        let objective = score_objective_policy(
            form.score_mode(),
            form.score_profile(),
            form.spin_profile(),
            form.initial_b2b(),
            base_objective,
        )?;
        let canonical_pc_tiling = form.score_mode() == "tiling";
        let canonical_pc_minimals = form.score_mode() == "minimum-cover";
        let canonical_pc_score = form.score_mode() == "summary";
        let canonical_pc_score_finder = form.score_mode() == "score-finder";
        let canonical_pc_score_minimals = form.score_mode() == "score-minimals";
        let tiling_objective = objective.kind() == ObjectiveKind::Tiling;
        if !form.allow_hold() && form.hold_piece().is_some() {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                "GUI scenario cannot use an occupied hold slot when hold is disabled",
            ));
        }
        if canonical_pc_tiling
            && (form.rule() != "srs-plus"
                || form.count_policy() != "unique"
                || form.score_profile() != "tetrio"
                || form.spin_profile() != "t-spins"
                || form.initial_b2b() != 0
                || form.preserve_b2b()
                || form.solution_probabilities()
                || form
                    .queue_observation_policy()
                    .requires_observation_policy()
                || backend.precompute_build_dependencies()
                || backend.tablebase_requested())
        {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                "canonical GUI pc tiling request contains a noncanonical inactive option",
            ));
        }
        if tiling_objective
            && (form.preserve_b2b()
                || form.solution_probabilities()
                || form
                    .queue_observation_policy()
                    .requires_observation_policy()
                || backend.precompute_build_dependencies())
        {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                "GUI tiling-only search cannot use BuildUp, coverage, probability, or dependency-analysis options",
            ));
        }
        if canonical_pc_minimals
            && (backend.memory_budget_mb() != 0
                || backend.precompute_build_dependencies()
                || backend.tablebase_requested())
        {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                "canonical GUI pc minimals request contains an unaccounted memory, tablebase, or dependency-analysis override",
            ));
        }
        if canonical_pc_path
            && (form.count_policy() != "all"
                || form.score_profile() != "tetrio"
                || form.spin_profile() != "t-spins"
                || form.initial_b2b() != 0
                || form.preserve_b2b()
                || form.solution_probabilities()
                || form
                    .queue_observation_policy()
                    .requires_observation_policy()
                || backend.memory_budget_mb() != 0
                || backend.precompute_build_dependencies()
                || backend.tablebase_requested())
        {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                "canonical GUI pc path requires objective-all/count-all and no score, probability, observation, memory, tablebase, or dependency override",
            ));
        }
        if canonical_pc_score_minimals
            && (form.count_policy() != "all"
                || form.preserve_b2b()
                || form.solution_probabilities()
                || form
                    .queue_observation_policy()
                    .requires_observation_policy()
                || backend.precompute_build_dependencies()
                || backend.tablebase_requested())
        {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                "canonical GUI pc score-minimals request requires count-all and contains no constraint, probability, observation, tablebase, or dependency-analysis override",
            ));
        }
        if canonical_pc_score_finder
            && (form.count_policy() != "all"
                || form.remaining_queue().trim().is_empty()
                || form.remaining_queue_is_pattern()
                || form.remaining_queue_is_standard_bag()
                || form.score_profile() != "jstris-ultra"
                || form.spin_profile() != "t-spins"
                || form.initial_b2b() > 1
                || form.preserve_b2b()
                || form.solution_probabilities()
                || form
                    .queue_observation_policy()
                    .requires_observation_policy()
                || backend.precompute_build_dependencies()
                || backend.tablebase_requested())
        {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                "canonical GUI pc score-finder requires one fixed queue, jstris-ultra/t-spins, initial B2B 0 or 1, count-all, and no probability, observation, tablebase, or dependency-analysis override",
            ));
        }
        if canonical_pc_save
            && (form.count_policy() != "all"
                || (!form.remaining_queue_is_pattern() && !form.remaining_queue_is_standard_bag())
                || form.preserve_b2b()
                || form.solution_probabilities()
                || form
                    .queue_observation_policy()
                    .requires_observation_policy()
                || backend.memory_budget_mb() != 0
                || backend.precompute_build_dependencies()
                || backend.tablebase_requested())
        {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                "canonical GUI pc save request requires count-all bag provenance, the full-queue oracle, and no probability, constraint, memory, tablebase, or dependency-analysis override",
            ));
        }
        let objective = execution_constraint_objective_policy(
            form.preserve_b2b(),
            form.spin_profile(),
            objective,
        )?;
        if objective.score().requested() {
            count_policy = PcCountPolicy::CountAll;
        }
        if tiling_objective || canonical_pc_minimals {
            count_policy = PcCountPolicy::CountUnique;
        }
        if canonical_pc_save || canonical_pc_path {
            count_policy = PcCountPolicy::CountAll;
        }
        let execution_policy =
            if canonical_pc_score || canonical_pc_score_finder || canonical_pc_score_minimals {
                BackendRequestBuilder::build_pc_score_execution_policy(backend)?
            } else {
                BackendRequestBuilder::build_execution_policy(backend)?
            };
        let mut query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(
                u16::from(form.visible_height()),
                form.initial_board_mask(),
            ),
            queue,
            PieceWindow::new(form.piece_window()),
        )
        .with_rule(parse_rule_profile(form.rule())?)
        .with_exact_pieces(Some(form.piece_window()))
        .with_queue_observation_policy(form.queue_observation_policy())
        .with_hold_piece(hold_piece)
        .with_allow_hold(form.allow_hold())
        .with_count_policy(count_policy)
        .with_objective(objective)
        .with_execution_policy(execution_policy);
        if canonical_pc_tiling
            || canonical_pc_score
            || canonical_pc_score_finder
            || canonical_pc_score_minimals
            || canonical_pc_save
            || canonical_pc_path
        {
            query = query.with_retained_trace_limit(1);
        }
        if canonical_pc_tiling
            && (form.remaining_queue_is_standard_bag() || finite_standard_bag_len.is_some())
        {
            let automatic_source_pieces = form
                .piece_window()
                .saturating_add(usize::from(form.allow_hold()));
            query = query.with_supply_window_size(
                clearra_pc_graph::request::SupplyWindowSize::new(7.min(automatic_source_pieces)),
            );
        } else if let Some(length) = finite_standard_bag_len {
            let automatic_source_pieces = form
                .piece_window()
                .saturating_add(usize::from(form.allow_hold()))
                .saturating_sub(usize::from(hold_piece.is_some()));
            query =
                query.with_supply_window_size(clearra_pc_graph::request::SupplyWindowSize::new(
                    length.min(automatic_source_pieces),
                ));
        }
        if form.solution_probabilities() {
            query = query.with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
        }

        if form.score_mode() == "failed-queue" {
            Ok(AppCommand::Percent(PercentAppCommand::failed_queue(query)))
        } else {
            let command = ScenarioAppCommand::new(query);
            let command = if canonical_pc_tiling {
                command.with_result_projection(PcResultProjection::TilingFamilyV1(
                    PcTilingIngressOrigin::CanonicalPcTiling,
                ))
            } else if canonical_pc_minimals {
                command.with_result_projection(PcResultProjection::MinimumCoverV2(
                    PcMinimalsIngressOrigin::CanonicalPcMinimals,
                ))
            } else if canonical_pc_path {
                command.with_result_projection(PcResultProjection::PathFamilyV2(
                    PcPathIngressOrigin::CanonicalPcPath,
                ))
            } else if canonical_pc_score {
                command.with_result_projection(PcResultProjection::ScoreSummaryV2(
                    PcScoreIngressOrigin::CanonicalPcScore,
                ))
            } else if canonical_pc_score_finder {
                command.with_result_projection(PcResultProjection::ScoreSummaryV2(
                    PcScoreIngressOrigin::CanonicalPcScoreFinder,
                ))
            } else if canonical_pc_score_minimals {
                command.with_result_projection(PcResultProjection::ScorePortfolioV2(
                    PcScoreMinimalsIngressOrigin::CanonicalPcScoreMinimals,
                ))
            } else if canonical_pc_saves {
                command.with_result_projection(PcResultProjection::SaveGroupsV2(
                    PcSaveIngressOrigin::CanonicalPcSaves,
                ))
            } else if canonical_pc_best_save {
                command.with_result_projection(PcResultProjection::BestSaveV2(
                    PcSaveIngressOrigin::CanonicalPcBestSave,
                ))
            } else {
                command
            };
            Ok(AppCommand::Scenario(command))
        }
    }
}
