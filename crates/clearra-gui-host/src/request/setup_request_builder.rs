use clearra_app::{AppCommand, SetupAppCommand};
use clearra_problem::{SetupCycleResetBorrowPolicy, SetupSearchQuery};

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

        Ok(AppCommand::Setup(SetupAppCommand::new(
            SetupSearchQuery::default()
                .with_remaining_pieces(pieces)
                .with_cycle_reset_borrow_policy(borrow_policy),
        )))
    }
}
