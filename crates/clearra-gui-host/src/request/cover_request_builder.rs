use clearra_app::{AppCommand, CoverAppCommand};
use clearra_build_coverage::{
    domain::slot_domain::SlotDomain,
    query::{build_coverage_limits::BuildCoverageLimits, build_coverage_query::BuildCoverageQuery},
    template::{
        build_slot::{BuildSlot, BuildSlotId},
        build_template::BuildTemplate,
    },
};
use clearra_core_domain::{board::cell::CellCoord, piece::piece_kind::PieceKind};

use crate::{
    model::{GuiBackendForm, GuiBuildCoverageForm},
    request::{BackendRequestBuilder, RequestBuildError, RequestBuildErrorCode},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CoverRequestBuilder;

impl CoverRequestBuilder {
    pub fn build_command(
        form: &GuiBuildCoverageForm,
        backend: &GuiBackendForm,
    ) -> Result<AppCommand, RequestBuildError> {
        if form.rule() != "srs-plus" {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::UnsupportedRule,
                format!(
                    "GUI cover request builder only supports srs-plus, got {}",
                    form.rule()
                ),
            ));
        }
        BackendRequestBuilder::validate_form(backend)?;

        let slot_id = BuildSlotId::new(0);
        let query = BuildCoverageQuery::new(
            BuildTemplate::new(
                form.template_id(),
                vec![BuildSlot::new(
                    slot_id,
                    vec![CellCoord::new_unchecked(0, 0)],
                )],
            ),
            vec![SlotDomain::new(
                slot_id,
                PieceKind::STANDARD_TETROMINOES.to_vec(),
            )],
            Vec::new(),
            1,
            BuildCoverageLimits::default(),
        );

        Ok(AppCommand::Cover(CoverAppCommand::new(query)))
    }
}
