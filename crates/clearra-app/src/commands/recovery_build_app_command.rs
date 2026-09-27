//! Public paired-Build command. It never reconstructs an old fixed-role request.
use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    render::{AppMessage, AppRenderModel, AppResultKind},
};
use clearra_forward_search::{
    CrossStageEarlyLimit, RecoveryBuildError, RecoveryBuildExample, RecoveryBuildParallelError,
    RecoveryBuildPopulation, RecoveryBuildQuery, RecoveryBuildStatus,
};
use clearra_host_contract::{
    ProductResultPayload, ProductResultPayloadContent, RecoveryBuildExamplePayload,
    RecoveryBuildPayload, RecoveryBuildStepPayload,
};
use clearra_output::model::RenderField;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryBuildAppCommand {
    query: RecoveryBuildQuery,
}
impl RecoveryBuildAppCommand {
    pub fn new(query: RecoveryBuildQuery) -> Self {
        Self { query }
    }
    pub fn query(&self) -> &RecoveryBuildQuery {
        &self.query
    }
}
impl RunnableAppCommand for RecoveryBuildAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        #[cfg(not(target_arch = "wasm32"))]
        let searched = crate::native_recovery_build_execution::run_native_recovery_build(
            self.query.clone(),
            usize::from(context.resource_budget().workers()),
            context.execution_control,
        );
        #[cfg(target_arch = "wasm32")]
        let searched = self
            .query
            .search(context.execution_control)
            .map_err(RecoveryBuildParallelError::from);
        let report = match searched {
            Ok(report) => report,
            Err(error) => {
                let input = matches!(
                    &error,
                    RecoveryBuildParallelError::Search(
                        RecoveryBuildError::InvalidHeight
                            | RecoveryBuildError::BoardOutsideField
                            | RecoveryBuildError::MiddleOverlapsStart
                            | RecoveryBuildError::ResultOverlapsRetainedMiddle
                            | RecoveryBuildError::TargetAreaNotTetrominoes
                            | RecoveryBuildError::EmptySupply
                            | RecoveryBuildError::InvalidSupplyPattern
                            | RecoveryBuildError::UnsupportedRuleProfile
                    )
                );
                return AppResponse::failed(
                    if input {
                        AppStatus::ValidationFailed
                    } else {
                        AppStatus::ExecutionFailed
                    },
                    AppError::new(
                        if input {
                            AppErrorCode::InvalidInput
                        } else {
                            AppErrorCode::ExecutionFailed
                        },
                        format!("recovery-build: {error:?}"),
                    ),
                );
            }
        };
        recovery_build_response(&self.query, report)
    }
}

pub(crate) fn recovery_build_response(
    query: &RecoveryBuildQuery,
    report: RecoveryBuildPopulation,
) -> AppResponse {
    let identity = format!(
        "{:x}",
        Sha256::digest(format!("recovery-build.v2:{query:?}").as_bytes())
    );
    let public = RecoveryBuildPayload {
        input_identity: identity,
        height: query.fields.height,
        start_board_mask: mask(query.fields.initial.words()),
        middle_target_mask: mask(query.fields.middle.words()),
        result_target_mask: mask(query.fields.result.words()),
        first_supply: query.first_supply.clone(),
        second_supply: query.second_supply.clone(),
        early_limit: match query.early_limit {
            CrossStageEarlyLimit::Auto => None,
            CrossStageEarlyLimit::AtMost(n) => Some(n.to_string()),
        },
        allow_piece_exchange: query.allow_piece_exchange,
        hold_enabled: query.hold_enabled,
        preserve_b2b: query.preserve_b2b,
        initial_b2b: query.initial_b2b,
        rule_profile: query.rule_profile.as_str().to_owned(),
        spin_profile: query.spin_profile.as_str().to_owned(),
        complete: report.evaluated == report.possible,
        pattern_count: report.possible.to_string(),
        evaluated_pattern_count: report.evaluated.to_string(),
        normal_count: report.normal_count.to_string(),
        recovery_count: report.recovery_count.to_string(),
        no_path_count: report.no_path_count.to_string(),
        state_count: report.states.to_string(),
        normal_probability: report.normal_probability.to_string(),
        recovery_probability: report.recovery_probability.to_string(),
        no_path_probability: report.no_path_probability.to_string(),
        all_paths_enumerated: false,
        examples: report
            .normal_example
            .as_ref()
            .into_iter()
            .chain(report.recovery_example.as_ref())
            .map(example)
            .collect(),
    };
    let fields = vec![
        RenderField::new("contract", "recovery-build.v2"),
        RenderField::new("pattern_count", public.pattern_count.clone()),
        RenderField::new("normal_count", public.normal_count.clone()),
        RenderField::new("recovery_count", public.recovery_count.clone()),
        RenderField::new("no_path_count", public.no_path_count.clone()),
        RenderField::new("complete", public.complete),
    ];
    AppResponse::success(AppRenderModel::BoundaryRecovery(AppMessage::new(
        AppResultKind::BoundaryRecovery,
        fields,
    )))
    .with_public_product_result(
        ProductResultPayload::new(
            "recovery-build.v2",
            "recovery-build",
            ProductResultPayloadContent::RecoveryBuild(public),
        ),
        None,
    )
}
fn mask(words: [u64; 4]) -> String {
    format!(
        "0x{:016x}{:016x}{:016x}{:016x}",
        words[3], words[2], words[1], words[0]
    )
}
fn example(value: &RecoveryBuildExample) -> RecoveryBuildExamplePayload {
    let path = &value.path;
    RecoveryBuildExamplePayload {
        first_pattern: value.first_pattern.to_string(),
        second_pattern: value.second_pattern.to_string(),
        first_queue: value
            .first_queue
            .iter()
            .map(|piece| piece.as_ascii())
            .collect(),
        second_queue: value
            .second_queue
            .iter()
            .map(|piece| piece.as_ascii())
            .collect(),
        status: match path.status {
            RecoveryBuildStatus::Normal => "normal",
            RecoveryBuildStatus::Recovery => "recovery",
            RecoveryBuildStatus::NoPath => "no-path",
        }
        .into(),
        terminal_board_mask: mask(path.terminal_board),
        effective_max_early: path.effective_max_early.to_string(),
        actual_early: path.actual_early.to_string(),
        exchange_balance: path.exchange_balance.to_vec(),
        steps: path
            .steps
            .iter()
            .map(|step| RecoveryBuildStepPayload {
                source_index: step.source_index.to_string(),
                result_target: step.result_target,
                piece: step.piece.as_ascii().to_string(),
                rotation: step.rotation,
                x: step.x,
                y: step.y,
                hold_decision: step.hold_decision.into(),
                board_before_mask: mask(step.board_before),
                placement_mask: mask(step.placement),
                board_after_mask: mask(step.board_after),
                cleared_rows: step.cleared_rows,
                cleared_lines: step.cleared_lines,
                recognized_spin: step.recognized_spin,
                b2b_active: step.b2b_active,
                middle_complete: step.middle_complete,
            })
            .collect(),
    }
}
