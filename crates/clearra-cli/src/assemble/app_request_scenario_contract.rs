use clearra_app::{ScenarioAppExpected, ScenarioAppRenderContract};

pub(crate) fn scenario_render_contract(
    args: &crate::args::PcScenarioArgs,
    assembly: &crate::assemble::PcScenarioAssembly,
) -> ScenarioAppRenderContract {
    ScenarioAppRenderContract::new(args.verify_expected(), assembly.input_fields())
        .with_fixture_path(assembly.fixture_path().map(ToOwned::to_owned))
        .with_expected(assembly.fixture().map(|fixture| {
            let expected = fixture.expected();
            ScenarioAppExpected::new(expected.solution_exists(), expected.count_complete())
                .with_expected_total_solution_count(expected.expected_total_solution_count())
                .with_unsupported(
                    expected.unsupported(),
                    expected.unsupported_reason().map(ToOwned::to_owned),
                )
                .with_accepted_retained_trace_keys(expected.accepted_retained_trace_keys().to_vec())
                .with_normalized_solution_set(
                    expected.normalized_solution_oracle().map(ToOwned::to_owned),
                    expected
                        .expected_normalized_solution_set_hash()
                        .map(ToOwned::to_owned),
                    expected.expected_normalized_solution_keys().to_vec(),
                    expected.operation_replay_available(),
                )
        }))
}
