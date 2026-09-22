use clearra_forward_search::{
    BoundaryRecoveryError, BoundaryRecoveryQuery, BoundaryRecoveryStatus,
};
use clearra_host_contract::{
    BoundaryRecoveryPayload, BoundaryRecoveryStepPayload, ProductResultPayload,
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
}

impl BoundaryRecoveryAppCommand {
    pub fn new(query: BoundaryRecoveryQuery) -> Self {
        Self { query }
    }

    pub fn query(&self) -> &BoundaryRecoveryQuery {
        &self.query
    }
}

impl RunnableAppCommand for BoundaryRecoveryAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
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
        let public_steps = report
            .steps
            .iter()
            .map(|step| BoundaryRecoveryStepPayload {
                source_queue_index: step.source_queue_index as u8,
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
            })
            .collect();
        let public = BoundaryRecoveryPayload {
            status: status.to_owned(),
            knowledge_basis: "full-fixed-queue".to_owned(),
            max_early_placements: self.query.max_early_placements,
            borrow_source_index: self.query.borrow_source_index as u8,
            borrow_placement_mask: mask_hex(self.query.borrow_placement_mask.words()),
            normal_states: report.normal_states,
            recovery_states: report.recovery_states,
            stage_one_checkpoint_step: report.stage_one_checkpoint_step,
            checkpoint_is_pc: report.checkpoint_is_pc,
            borrowed_stage_two_count: report.borrowed_stage_two_count,
            steps: public_steps,
        };
        let steps = report.steps.iter().map(|step| {
            RenderFieldValue::object([
                (
                    "source_queue_index",
                    RenderFieldValue::from(step.source_queue_index),
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
            RenderField::new("max_early_placements", self.query.max_early_placements),
            RenderField::new("borrow_source_index", self.query.borrow_source_index),
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

fn mask_hex(words: [u64; 4]) -> String {
    format!(
        "0x{:x}{:016x}{:016x}{:016x}",
        words[3], words[2], words[1], words[0]
    )
}
