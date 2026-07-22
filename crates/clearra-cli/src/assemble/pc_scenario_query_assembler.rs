use crate::{
    args::pc_scenario_args::PcScenarioArgs,
    assemble::{
        execution_policy_assembler::ExecutionPolicyAssembler,
        pc_scenario_fixture_assembler::query_from_fixture,
        pc_scenario_supply_assembler::inline_query,
        pc_scenario_validation_material::{PcScenarioAssembly, PcScenarioQueryAssemblyError},
    },
    fixture::PcScenarioFixture,
    input::file_input_guard::display_input_path,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcScenarioQueryAssembler;

impl PcScenarioQueryAssembler {
    pub fn assemble(
        args: &PcScenarioArgs,
    ) -> Result<PcScenarioAssembly, PcScenarioQueryAssemblyError> {
        if let Some(fixture_path) = args.fixture() {
            return assemble_fixture_query(args, fixture_path);
        }

        let query = inline_query(args)
            .map_err(|message| PcScenarioQueryAssemblyError::InvalidInline { message })?;
        Ok(PcScenarioAssembly::inline(query))
    }
}

fn assemble_fixture_query(
    args: &PcScenarioArgs,
    fixture_path: &str,
) -> Result<PcScenarioAssembly, PcScenarioQueryAssemblyError> {
    let display_path = display_input_path(fixture_path);
    let fixture = PcScenarioFixture::read(fixture_path)
        .map_err(|message| invalid_fixture(&display_path, message))?;
    let mut query =
        query_from_fixture(&fixture).map_err(|message| invalid_fixture(&display_path, message))?;
    if args.has_execution_options() {
        let fixture_policy = query.execution_policy().clone();
        query = query.with_execution_policy(
            ExecutionPolicyAssembler::overlay_pc_scenario_args(fixture_policy, args)
                .map_err(|error| invalid_fixture(&display_path, error.message()))?,
        );
    }
    if args.solution_probabilities() {
        query = query.with_solution_probability_policy(
            clearra_pc_graph::request::PcSolutionProbabilityPolicy::Include,
        );
    }

    let mut input_fields = vec![
        ("input_mode".to_owned(), "fixture".to_owned()),
        ("fixture_name".to_owned(), fixture.name().to_owned()),
        ("fixture_path".to_owned(), display_path.clone()),
    ];
    input_fields.extend(fixture.source_fields());

    Ok(PcScenarioAssembly::from_fixture(
        query,
        fixture,
        display_path,
        input_fields,
    ))
}

fn invalid_fixture(path: &str, message: String) -> PcScenarioQueryAssemblyError {
    PcScenarioQueryAssemblyError::InvalidFixture {
        path: path.to_owned(),
        message,
    }
}

#[cfg(test)]
#[path = "pc_scenario_query_assembler_tests.rs"]
mod tests;
