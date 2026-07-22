use clearra_app::{AppCommand, PcAppCommand};
use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcHoldPolicy, PcQueueInput, PcSolutionProbabilityPolicy, SupplyWindowSize,
};
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

use crate::{
    model::{GuiBackendForm, GuiOpeningPcForm},
    request::{
        parse_piece_sequence, parse_queue_pattern, parse_rule_profile, score_objective_policy,
        BackendRequestBuilder, RequestBuildError, RequestBuildErrorCode,
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
        let policy = BackendRequestBuilder::build_execution_policy(backend)?;
        let objective = score_objective_policy(
            form.score_mode(),
            form.score_profile(),
            form.spin_profile(),
            form.initial_b2b(),
            clearra_objectives::policy::objective_policy::ObjectivePolicy::unique(),
        )?;
        let mut query = OpeningPcSearchQuery::new(target)
            .with_rule(parse_rule_profile(form.rule())?)
            .with_objective(objective)
            .with_execution_policy(policy);
        if let Some(queue) = form.fixed_queue() {
            query = query
                .with_queue(PcQueueInput::fixed_sequence(parse_piece_sequence(
                    queue,
                    "opening fixed queue",
                )?))
                .with_hold_policy(if form.hold_enabled() {
                    PcHoldPolicy::EnabledEmpty
                } else {
                    PcHoldPolicy::Disabled
                });
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
            query = query
                .with_queue(queue)
                .with_hold_policy(if form.hold_enabled() {
                    PcHoldPolicy::EnabledEmpty
                } else {
                    PcHoldPolicy::Disabled
                });
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

        Ok(AppCommand::Pc(PcAppCommand::new(query)))
    }
}
