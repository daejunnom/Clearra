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
    assert_eq!(assembly.query().exact_pieces(), Some(1));
    assert_eq!(
        assembly.query().supply_window_size(),
        Some(SupplyWindowSize::new(5))
    );
    assert_eq!(assembly.query().execution_policy().max_patterns(), 0);
    assert_eq!(assembly.query().initial_board().occupied_mask(), 0x3f0);
    assert_eq!(assembly.query().initial_board().visible_height(), 1);
    assert_eq!(assembly.query().count_policy(), PcCountPolicy::CountUnique);
    assert_eq!(assembly.query().retained_trace_limit(), 0);
    assert_eq!(assembly.failed_pattern_limit(), 100);
}

#[test]
fn carries_failed_pattern_output_limit_without_changing_search_universe() {
    let assembly =
        PercentQueryAssembler::assemble(&PercentArgs::new("I,O,T").with_failed_pattern_limit(17))
            .expect("percent query");

    assert_eq!(assembly.failed_pattern_limit(), 17);
    assert_eq!(assembly.query().execution_policy().max_patterns(), 0);
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
    assert_eq!(assembly.query().piece_window().max_pieces(), 2);
    assert_eq!(assembly.query().exact_pieces(), Some(1));
    assert_eq!(
        assembly.query().supply_window_size(),
        Some(SupplyWindowSize::new(2))
    );
}
