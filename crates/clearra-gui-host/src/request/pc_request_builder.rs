use clearra_app::{
    AppCommand, PcAppCommand, PcMinimalsIngressOrigin, PcPathIngressOrigin, PcResultProjection,
    PcSaveIngressOrigin, PcScoreIngressOrigin, PcScoreMinimalsIngressOrigin, PcTilingIngressOrigin,
    PercentAppCommand,
};
use clearra_core_domain::{objective::objective_kind::ObjectiveKind, pc::pc_target::PcTarget};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcHoldPolicy, PcQueueInput, PcSolutionProbabilityPolicy,
    SupplyWindowSize,
};
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

use crate::{
    model::{GuiBackendForm, GuiOpeningPcForm},
    request::{
        execution_constraint_objective_policy, parse_piece_sequence, parse_queue_pattern,
        parse_rule_profile, score_objective_policy, BackendRequestBuilder, RequestBuildError,
        RequestBuildErrorCode,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcRequestBuilder;

impl PcRequestBuilder {
    pub fn build_command(
        form: &GuiOpeningPcForm,
        backend: &GuiBackendForm,
    ) -> Result<AppCommand, RequestBuildError> {
        let target = PcTarget::new(form.lines()).map_err(|error| {
            RequestBuildError::new(
                RequestBuildErrorCode::InvalidLineCount,
                format!("invalid GUI PC line target: {error:?}"),
            )
        })?;
        if form.score_mode() == "score-finder" {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                "canonical GUI pc score-finder requires an explicit initial-field scenario",
            ));
        }
        let canonical_pc_path = form.score_mode() == "path";
        let canonical_pc_saves = form.score_mode() == "saves";
        let canonical_pc_best_save = form.score_mode() == "best-save";
        let canonical_pc_save = canonical_pc_saves || canonical_pc_best_save;
        let base_objective =
            if form.score_mode() == "failed-queue" || canonical_pc_save || canonical_pc_path {
                clearra_objectives::policy::objective_policy::ObjectivePolicy::all()
            } else {
                clearra_objectives::policy::objective_policy::ObjectivePolicy::unique()
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
        let canonical_pc_score_minimals = form.score_mode() == "score-minimals";
        let tiling_objective = objective.kind() == ObjectiveKind::Tiling;
        if canonical_pc_tiling
            && (form.rule() != "srs-plus"
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
            && (form.score_profile() != "tetrio"
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
            && (form.preserve_b2b()
                || form.solution_probabilities()
                || form
                    .queue_observation_policy()
                    .requires_observation_policy()
                || backend.precompute_build_dependencies()
                || backend.tablebase_requested())
        {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                "canonical GUI pc score-minimals request contains a noncanonical constraint, probability, observation, tablebase, or dependency-analysis option",
            ));
        }
        if canonical_pc_save
            && (form.fixed_queue().is_some()
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
                "canonical GUI pc save request requires bag provenance, the full-queue oracle, and no probability, constraint, memory, tablebase, or dependency-analysis override",
            ));
        }
        let policy = if canonical_pc_score || canonical_pc_score_minimals {
            BackendRequestBuilder::build_pc_score_execution_policy(backend)?
        } else {
            BackendRequestBuilder::build_execution_policy(backend)?
        };
        let objective = execution_constraint_objective_policy(
            form.preserve_b2b(),
            form.spin_profile(),
            objective,
        )?;
        let mut query = OpeningPcSearchQuery::new(target)
            .with_queue(PcQueueInput::standard_7_bag())
            .with_rule(parse_rule_profile(form.rule())?)
            .with_objective(objective)
            .with_queue_observation_policy(form.queue_observation_policy())
            .with_hold_policy(if form.hold_enabled() {
                PcHoldPolicy::EnabledEmpty
            } else {
                PcHoldPolicy::Disabled
            })
            .with_execution_policy(policy);
        if canonical_pc_minimals {
            // The named product counts concrete geometry identities once and
            // applies minimum-cover reduction as an independent objective.
            // Keep that contract explicit on the legacy host builder too;
            // deriving count policy from MinimumCover would silently widen it
            // to CountAll before the shared App boundary.
            query = query.with_count_policy(PcCountPolicy::CountUnique);
        }
        if let Some(queue) = form.fixed_queue() {
            query = query.with_queue(PcQueueInput::fixed_sequence(parse_piece_sequence(
                queue,
                "opening fixed queue",
            )?));
        }
        if let Some(pattern) = form.queue_pattern() {
            let finite_standard_bag_len =
                QueuePatternExpression::standard_7_bag_draw_count(pattern);
            let queue = if finite_standard_bag_len.is_some() {
                PcQueueInput::standard_7_bag()
            } else {
                PcQueueInput::pattern_expression(parse_queue_pattern(
                    pattern,
                    backend.pattern_budget() as usize,
                    "opening queue pattern",
                )?)
            };
            query = query.with_queue(queue);
            if let Some(length) = finite_standard_bag_len {
                let required_pieces = usize::from(target.lines()) * 10 / 4;
                query =
                    query
                        .with_supply_window_size(SupplyWindowSize::new(length.min(
                            required_pieces.saturating_add(usize::from(form.hold_enabled())),
                        )));
            }
        }
        if form.solution_probabilities() {
            query = query.with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
        }

        if form.score_mode() == "failed-queue" {
            Ok(AppCommand::Percent(
                PercentAppCommand::failed_queue_opening(query),
            ))
        } else {
            let command = PcAppCommand::new(query);
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
            Ok(AppCommand::Pc(command))
        }
    }
}
