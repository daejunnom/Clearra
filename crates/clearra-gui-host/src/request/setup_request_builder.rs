use clearra_app::{AppCommand, SetupAppCommand};
use clearra_problem::{SetupCandidatePriority, SetupCycleResetBorrowPolicy, SetupSearchQuery};

use crate::{
    model::{GuiBackendForm, GuiSetupSearchForm},
    request::{
        parse_piece_sequence, BackendRequestBuilder, RequestBuildError, RequestBuildErrorCode,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupRequestBuilder;

impl SetupRequestBuilder {
    pub fn build_command(
        form: &GuiSetupSearchForm,
        backend: &GuiBackendForm,
    ) -> Result<AppCommand, RequestBuildError> {
        if form.rule() != "srs-plus" {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::UnsupportedRule,
                format!(
                    "GUI setup request builder only supports srs-plus, got {}",
                    form.rule()
                ),
            ));
        }
        BackendRequestBuilder::validate_form(backend)?;

        let sequence = parse_piece_sequence(form.remaining_pieces(), "setup remaining pieces")?;
        let pieces = sequence.pieces().to_vec();
        let borrow_policy = if form.allow_post_cycle_borrow() {
            SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
        } else {
            SetupCycleResetBorrowPolicy::ForbidPostCyclePieceUse
        };
        let candidate_priority = SetupCandidatePriority::from_keyword(form.candidate_priority())
            .ok_or_else(|| {
                RequestBuildError::new(
                    RequestBuildErrorCode::ValidationFailed,
                    format!(
                        "unsupported setup candidate priority {}",
                        form.candidate_priority()
                    ),
                )
            })?;

        Ok(AppCommand::Setup(SetupAppCommand::new(
            SetupSearchQuery::default()
                .with_remaining_pieces(pieces)
                .with_cycle_reset_borrow_policy(borrow_policy)
                .with_candidate_priority(candidate_priority),
        )))
    }
}
