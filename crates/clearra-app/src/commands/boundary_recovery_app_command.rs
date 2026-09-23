use clearra_forward_search::{
    BoundaryRecoveryError, BoundaryRecoveryPatternError, BoundaryRecoveryPatternQuery,
    BoundaryRecoveryPopulationError, BoundaryRecoveryQuery, BoundaryRecoveryStatus,
    BoundaryRecoveryStep,
};
use clearra_host_contract::{
    BoundaryRecoveryPayload, BoundaryRecoveryPopulationExamplePayload,
    BoundaryRecoveryPopulationPayload, BoundaryRecoveryStepPayload, ProductResultPayload,
    ProductResultPayloadContent,
};
use clearra_output::model::{RenderField, RenderFieldValue};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    render::{AppMessage, AppRenderModel, AppResultKind},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryRecoveryAppCommand {
    query: BoundaryRecoveryQuery,
    pattern: Option<BoundaryRecoveryPatternOptions>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BoundaryRecoveryPatternOptions {
    source: String,
    max_pattern_evaluations: usize,
    max_total_states: usize,
}

impl BoundaryRecoveryAppCommand {
    pub fn new(query: BoundaryRecoveryQuery) -> Self {
        Self {
            query,
            pattern: None,
        }
    }

    pub fn new_pattern(
        query: BoundaryRecoveryQuery,
        source: String,
        max_pattern_evaluations: usize,
        max_total_states: usize,
    ) -> Self {
        Self {
            query,
            pattern: Some(BoundaryRecoveryPatternOptions {
                source,
                max_pattern_evaluations,
                max_total_states,
            }),
        }
    }

    pub fn query(&self) -> &BoundaryRecoveryQuery {
        &self.query
    }
}

impl RunnableAppCommand for BoundaryRecoveryAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        if let Some(pattern) = &self.pattern {
            return self.run_pattern(context, pattern);
        }
        let report = match self.query.search(context.execution_control) {
            Ok(report) => report,
            Err(BoundaryRecoveryError::Cancelled) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ExecutionFailed, "boundary recovery cancelled"),
                )
            }
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ValidationFailed,
                    AppError::new(
                        AppErrorCode::InvalidInput,
                        format!("invalid boundary recovery query: {error:?}"),
                    ),
                )
            }
        };
        let status = match report.status {
            BoundaryRecoveryStatus::Normal => "normal",
            BoundaryRecoveryStatus::PcPreservingRecovery => "pc-preserving-recovery",
            BoundaryRecoveryStatus::NonPcRecovery => "non-pc-recovery",
            BoundaryRecoveryStatus::NoPath => "no-path-within-declared-scope",
            BoundaryRecoveryStatus::Incomplete => "incomplete",
        };
        let public_steps = report.steps.iter().map(public_step).collect();
        let public = BoundaryRecoveryPayload {
            status: status.to_owned(),
            knowledge_basis: "full-fixed-queue".to_owned(),
            placement_role_scope: if self.query.placement_role_masks.is_empty() {
                "occupancy-only"
            } else {
                "exact-lock-time"
            }
            .to_owned(),
            max_early_placements: self.query.max_early_placements,
            borrow_role_index: self.query.borrow_role_index as u8,
            borrow_placement_mask: mask_hex(self.query.borrow_placement_mask.words()),
            normal_states: report.normal_states,
            recovery_states: report.recovery_states,
            stage_one_checkpoint_step: report.stage_one_checkpoint_step,
            checkpoint_is_pc: report.checkpoint_is_pc,
            borrowed_stage_two_count: report.borrowed_stage_two_count,
            steps: public_steps,
            population: None,
        };
        let steps = report.steps.iter().map(|step| {
            RenderFieldValue::object([
                (
                    "source_queue_index",
                    RenderFieldValue::from(step.source_queue_index),
                ),
                (
                    "placement_role_index",
                    RenderFieldValue::from(step.placement_role_index),
                ),
                (
                    "piece",
                    RenderFieldValue::string(step.piece.as_ascii().to_string()),
                ),
                (
                    "rotation",
                    RenderFieldValue::from(step.rotation.quarter_turns()),
                ),
                ("x", RenderFieldValue::from(step.x)),
                ("y", RenderFieldValue::from(step.y)),
                (
                    "hold_decision",
                    RenderFieldValue::string(step.hold_decision),
                ),
                ("cleared_lines", RenderFieldValue::from(step.cleared_lines)),
                (
                    "cleared_row_mask",
                    RenderFieldValue::from(step.cleared_row_mask),
                ),
                (
                    "recognized_spin",
                    RenderFieldValue::bool(step.recognized_spin),
                ),
                (
                    "b2b_active_after",
                    RenderFieldValue::bool(step.b2b_active_after),
                ),
                (
                    "stage_one_complete_after",
                    RenderFieldValue::bool(step.stage_one_complete_after),
                ),
                (
                    "placement_mask_words",
                    RenderFieldValue::array(step.placement_mask.map(RenderFieldValue::from)),
                ),
                (
                    "board_after_words",
                    RenderFieldValue::array(step.board_after.map(RenderFieldValue::from)),
                ),
            ])
        });
        let fields = vec![
            RenderField::new("contract", "boundary-recovery.v1"),
            RenderField::new("status", status),
            RenderField::new("knowledge_basis", "full-fixed-queue"),
            RenderField::new(
                "placement_role_scope",
                if self.query.placement_role_masks.is_empty() {
                    "occupancy-only"
                } else {
                    "exact-lock-time"
                },
            ),
            RenderField::new("max_early_placements", self.query.max_early_placements),
            RenderField::new("borrow_role_index", self.query.borrow_role_index),
            RenderField::new(
                "borrow_placement_mask",
                mask_hex(self.query.borrow_placement_mask.words()),
            ),
            RenderField::new("normal_states", report.normal_states),
            RenderField::new("recovery_states", report.recovery_states),
            RenderField::new(
                "stage_one_checkpoint_step",
                report
                    .stage_one_checkpoint_step
                    .map_or(RenderFieldValue::Null, RenderFieldValue::from),
            ),
            RenderField::new(
                "checkpoint_is_pc",
                report
                    .checkpoint_is_pc
                    .map_or(RenderFieldValue::Null, RenderFieldValue::bool),
            ),
            RenderField::new("borrowed_stage_two_count", report.borrowed_stage_two_count),
            RenderField::new("steps", RenderFieldValue::array(steps)),
        ];
        AppResponse::success(AppRenderModel::BoundaryRecovery(AppMessage::new(
            AppResultKind::BoundaryRecovery,
            fields,
        )))
        .with_public_product_result(
            ProductResultPayload::new(
                "boundary-recovery.v1",
                AppResultKind::BoundaryRecovery.as_str(),
                ProductResultPayloadContent::BoundaryRecovery(public),
            ),
            None,
        )
    }
}

impl BoundaryRecoveryAppCommand {
    fn run_pattern(
        &self,
        context: &AppExecutionContext<'_>,
        options: &BoundaryRecoveryPatternOptions,
    ) -> AppResponse {
        let search = BoundaryRecoveryPatternQuery {
            reference: self.query.clone(),
            queue_pattern: options.source.clone(),
            max_pattern_evaluations: options.max_pattern_evaluations,
            max_total_states: options.max_total_states,
        };
        let report = match search.search(context.execution_control) {
            Ok(report) => report,
            Err(BoundaryRecoveryPatternError::Population(
                BoundaryRecoveryPopulationError::Cancelled,
            )) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ExecutionFailed, "boundary recovery cancelled"),
                );
            }
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ValidationFailed,
                    AppError::new(
                        AppErrorCode::InvalidInput,
                        format!("invalid boundary recovery pattern: {error:?}"),
                    ),
                );
            }
        };
        let status = if report.complete {
            "population-complete"
        } else {
            "population-incomplete"
        };
        let population = BoundaryRecoveryPopulationPayload {
            materialized_pattern_count: report.materialized_pattern_count,
            total_possible_pattern_count: report.total_possible_pattern_count.to_string(),
            evaluated_pattern_count: report.evaluated_pattern_count,
            state_count: report.state_count,
            complete: report.complete,
            normal_count: report.normal_count,
            pc_preserving_recovery_count: report.pc_preserving_recovery_count,
            non_pc_recovery_count: report.non_pc_recovery_count,
            no_path_count: report.no_path_count,
            incomplete_count: report.incomplete_count,
            diagram_unavailable_count: report.diagram_unavailable_count,
            normal_probability: probability_text(report.normal_probability.get()),
            pc_preserving_recovery_probability: probability_text(
                report.pc_preserving_recovery_probability.get(),
            ),
            non_pc_recovery_probability: probability_text(report.non_pc_recovery_probability.get()),
            additional_recovery_probability: probability_text(
                report.additional_recovery_probability.get(),
            ),
            total_response_probability: probability_text(report.total_response_probability.get()),
            no_path_probability: probability_text(report.no_path_probability.get()),
            unknown_probability: probability_text(report.unknown_probability.get()),
            normal_example: report.normal_example.map(public_example),
            recovery_example: report.recovery_example.map(public_example),
        };
        let public = BoundaryRecoveryPayload {
            status: status.to_owned(),
            knowledge_basis: "full-pattern-universe".to_owned(),
            placement_role_scope: "bag-piece-exact-lock-time".to_owned(),
            max_early_placements: self.query.max_early_placements,
            borrow_role_index: self.query.borrow_role_index as u8,
            borrow_placement_mask: mask_hex(self.query.borrow_placement_mask.words()),
            normal_states: 0,
            recovery_states: 0,
            stage_one_checkpoint_step: None,
            checkpoint_is_pc: None,
            borrowed_stage_two_count: 0,
            steps: Vec::new(),
            population: Some(Box::new(population)),
        };
        let population = public
            .population
            .as_ref()
            .expect("population constructed above");
        let fields = vec![
            RenderField::new("contract", "boundary-recovery.v1"),
            RenderField::new("status", status),
            RenderField::new("knowledge_basis", "full-pattern-universe"),
            RenderField::new("placement_role_scope", "bag-piece-exact-lock-time"),
            RenderField::new("complete", population.complete),
            RenderField::new(
                "evaluated_pattern_count",
                population.evaluated_pattern_count,
            ),
            RenderField::new(
                "total_possible_pattern_count",
                population.total_possible_pattern_count.clone(),
            ),
            RenderField::new("normal_probability", population.normal_probability.clone()),
            RenderField::new(
                "pc_preserving_recovery_probability",
                population.pc_preserving_recovery_probability.clone(),
            ),
            RenderField::new(
                "non_pc_recovery_probability",
                population.non_pc_recovery_probability.clone(),
            ),
            RenderField::new(
                "additional_recovery_probability",
                population.additional_recovery_probability.clone(),
            ),
            RenderField::new(
                "total_response_probability",
                population.total_response_probability.clone(),
            ),
            RenderField::new(
                "unknown_probability",
                population.unknown_probability.clone(),
            ),
        ];
        AppResponse::success(AppRenderModel::BoundaryRecovery(AppMessage::new(
            AppResultKind::BoundaryRecovery,
            fields,
        )))
        .with_public_product_result(
            ProductResultPayload::new(
                "boundary-recovery.v1",
                AppResultKind::BoundaryRecovery.as_str(),
                ProductResultPayloadContent::BoundaryRecovery(public),
            ),
            None,
        )
    }
}

fn public_step(step: &BoundaryRecoveryStep) -> BoundaryRecoveryStepPayload {
    BoundaryRecoveryStepPayload {
        source_queue_index: step.source_queue_index as u8,
        placement_role_index: step.placement_role_index as u8,
        piece: step.piece.as_ascii().to_string(),
        rotation: step.rotation.quarter_turns(),
        x: step.x,
        y: step.y,
        hold_decision: step.hold_decision.to_owned(),
        placement_mask: mask_hex(step.placement_mask),
        cleared_row_mask: step.cleared_row_mask,
        board_after_mask: mask_hex(step.board_after),
        cleared_lines: step.cleared_lines,
        recognized_spin: step.recognized_spin,
        b2b_active_after: step.b2b_active_after,
        stage_one_complete_after: step.stage_one_complete_after,
    }
}

fn public_example(
    (pattern_index, queue, report): (
        usize,
        Vec<clearra_core_domain::piece::piece_kind::PieceKind>,
        clearra_forward_search::BoundaryRecoveryReport,
    ),
) -> BoundaryRecoveryPopulationExamplePayload {
    let status = match report.status {
        BoundaryRecoveryStatus::Normal => "normal",
        BoundaryRecoveryStatus::PcPreservingRecovery => "pc-preserving-recovery",
        BoundaryRecoveryStatus::NonPcRecovery => "non-pc-recovery",
        BoundaryRecoveryStatus::NoPath => "no-path-within-declared-scope",
        BoundaryRecoveryStatus::Incomplete => "incomplete",
    };
    BoundaryRecoveryPopulationExamplePayload {
        pattern_index,
        queue: queue.iter().map(|piece| piece.as_ascii()).collect(),
        status: status.to_owned(),
        stage_one_checkpoint_step: report.stage_one_checkpoint_step,
        checkpoint_is_pc: report.checkpoint_is_pc,
        borrowed_stage_two_count: report.borrowed_stage_two_count,
        steps: report.steps.iter().map(public_step).collect(),
    }
}

fn probability_text(value: f64) -> String {
    format!("{value:.17}")
}

fn mask_hex(words: [u64; 4]) -> String {
    format!(
        "0x{:x}{:016x}{:016x}{:016x}",
        words[3], words[2], words[1], words[0]
    )
}
