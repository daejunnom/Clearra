use std::fmt;

use clearra_build_coverage::{
    domain::{slot_constraint::SlotConstraint, slot_domain::SlotDomain},
    query::{build_coverage_limits::BuildCoverageLimits, build_coverage_query::BuildCoverageQuery},
    template::{
        build_slot::{BuildSlot, BuildSlotId},
        build_template::BuildTemplate,
        template_import::{TemplateImport, TemplateJsonError},
    },
};
use clearra_core_domain::{board::cell::CellCoord, piece::piece_kind::PieceKind};

use crate::{
    args::cover_args::CoverArgs,
    input::file_input_guard::{display_input_path, read_json_file},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CoverQueryAssembler;

impl CoverQueryAssembler {
    pub fn assemble(args: &CoverArgs) -> Result<BuildCoverageQuery, CoverQueryAssemblyError> {
        if args.template_json().is_some() && args.template_file().is_some() {
            return Err(CoverQueryAssemblyError::ConflictingTemplateSources);
        }
        if args.template().is_some()
            && (args.template_json().is_some() || args.template_file().is_some())
        {
            return Err(CoverQueryAssemblyError::ConflictingTemplateSources);
        }

        if let Some(template_json) = args.template_json() {
            return TemplateImport::from_json("cli --template-json", template_json)
                .map(|import| Self::from_template(import.into_template()))
                .map_err(CoverQueryAssemblyError::TemplateJson);
        }

        if let Some(path) = args.template_file() {
            let display_path = display_input_path(path);
            let template_json = read_json_file(path).map_err(|error| {
                CoverQueryAssemblyError::TemplateFileRead {
                    path: display_path.clone(),
                    reason: error.to_string(),
                }
            })?;
            return TemplateImport::from_json(display_path, &template_json)
                .map(|import| Self::from_template(import.into_template()))
                .map_err(CoverQueryAssemblyError::TemplateJson);
        }

        let slot_id = BuildSlotId::new(0);
        let template_id = args.template().unwrap_or("cli-default");

        Ok(BuildCoverageQuery::new(
            BuildTemplate::new(
                template_id,
                vec![BuildSlot::new(
                    slot_id,
                    vec![CellCoord::new_unchecked(0, 0)],
                )],
            ),
            vec![SlotDomain::new(slot_id, vec![PieceKind::I])],
            Vec::new(),
            1,
            BuildCoverageLimits::default(),
        ))
    }
}
impl CoverQueryAssembler {
    fn from_template(template: BuildTemplate) -> BuildCoverageQuery {
        let domains = template
            .slots()
            .iter()
            .map(|slot| SlotDomain::new(slot.id(), slot.allowed_pieces().to_vec()))
            .collect::<Vec<_>>();
        let constraints = template
            .slots()
            .iter()
            .filter_map(|slot| {
                slot.required_piece()
                    .map(|piece| SlotConstraint::required(slot.id(), piece))
            })
            .collect::<Vec<_>>();

        BuildCoverageQuery::new(
            template,
            domains,
            constraints,
            1,
            BuildCoverageLimits::default(),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoverQueryAssemblyError {
    ConflictingTemplateSources,
    TemplateFileRead { path: String, reason: String },
    TemplateJson(TemplateJsonError),
}

impl fmt::Display for CoverQueryAssemblyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConflictingTemplateSources => {
                write!(formatter, "cover accepts exactly one template source")
            }
            Self::TemplateFileRead { path, reason } => {
                write!(formatter, "failed to read template file '{path}': {reason}")
            }
            Self::TemplateJson(error) => write!(formatter, "{error}"),
        }
    }
}

#[cfg(test)]
#[path = "cover_query_assembler_tests.rs"]
mod tests;
