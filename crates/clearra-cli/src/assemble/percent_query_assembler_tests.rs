use super::*;

#[test]
fn assembles_observed_percent_query_as_scenario_problem_input() {
    let assembly = PercentQueryAssembler::assemble(
        &PercentArgs::new("I,O,T")
            .with_mode(PercentQueueMode::Observed)
            .with_minimum_len(Some(5)),
    )
    .expect("percent query");

    assert_eq!(assembly.query().remaining_queue().mode(), "observed");
    assert_eq!(assembly.query().piece_window().max_pieces(), 5);
    assert_eq!(assembly.query().execution_policy().max_patterns(), 0);
    assert_eq!(assembly.query().initial_board().occupied_mask(), 0x3f0);
}

#[test]
fn assembles_fixed_percent_query_without_bag_boundary_contract() {
    let assembly = PercentQueryAssembler::assemble(
        &PercentArgs::new("I,I")
            .with_mode(PercentQueueMode::Fixed)
            .with_minimum_len(Some(2)),
    )
    .expect("fixed percent query");

    assert_eq!(assembly.query().remaining_queue().mode(), "fixed");
    assert_eq!(assembly.query().remaining_queue().len(), 2);
}
