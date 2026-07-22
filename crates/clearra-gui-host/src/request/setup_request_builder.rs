use clearra_app::{AppCommand, SetupAppCommand};
use clearra_problem::{SetupQueueInput, SetupSearchQuery};
use clearra_supply::queue::observed_queue::ObservedQueue;

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

        let sequence = parse_piece_sequence(form.queue(), "setup queue")?;
        let pieces = sequence.pieces().to_vec();
        let queue = if form.fixed_queue() {
            SetupQueueInput::fixed_sequence(sequence)
        } else {
            SetupQueueInput::observed(ObservedQueue::new(pieces))
        };

        Ok(AppCommand::Setup(SetupAppCommand::new(
            SetupSearchQuery::default().with_queue(queue),
        )))
    }
}
