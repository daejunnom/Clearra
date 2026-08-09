pub mod render_board;
pub mod render_field_value;
pub mod render_message;
pub mod render_setup;
pub mod render_solution;

pub use render_board::RenderBoard;
pub use render_field_value::{is_json_number, RenderField, RenderFieldValue};
pub use render_message::RenderMessage;
pub use render_setup::RenderSetup;
pub use render_solution::{
    RenderCoverSelection, RenderCoverSelectionOptimality, RenderCoverSelectionStrategy,
    RenderExactSearchBudget,
};

#[cfg(test)]
mod tests {
    use clearra_coverage::{cover::CoverSelection, pattern::pattern_bitset::PatternBitSet};

    use super::*;

    #[test]
    fn cover_selection_render_model_is_exported_from_model_surface() {
        let selection =
            CoverSelection::greedy_fallback(vec![0], PatternBitSet::new(1), true, 21, 20);

        let summary = RenderCoverSelection::from_selection(&selection);

        assert_eq!(
            summary.strategy(),
            RenderCoverSelectionStrategy::GreedyFallback
        );
        assert_eq!(
            summary.optimality(),
            RenderCoverSelectionOptimality::Approximate
        );
        assert_eq!(
            summary.exact_search_budget(),
            Some(RenderExactSearchBudget::new(21, 20))
        );
    }
}
