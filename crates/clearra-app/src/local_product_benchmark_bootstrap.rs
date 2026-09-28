// SRP rationale: this module owns only the local product benchmark's startup
// policy composition, keeping CLI hosts outside the Core implementation API.
use clearra_core_executor::{set_local_search_prune_policy, LocalSearchPrunePolicy};

/// Call once before starting any local product searches. A research binary's
/// process-wide Core policy defaults both accelerators off; match release gates
/// here while leaving the ordinary request switches and signed asset authority
/// unchanged. This function and its setter do not exist in release builds.
pub fn configure_local_product_search_benchmark() {
    set_local_search_prune_policy(product_benchmark_policy());
}

fn product_benchmark_policy() -> LocalSearchPrunePolicy {
    LocalSearchPrunePolicy::product_default()
        .with_legal_board(true)
        .with_conditioned_reachability(true)
}

#[cfg(test)]
mod tests {
    use super::product_benchmark_policy;

    #[test]
    fn local_product_bootstrap_matches_release_gates_without_changing_math_prunes() {
        // Do not mutate the global policy while other unit tests are running.
        let policy = product_benchmark_policy();
        assert!(policy.additive_parity);
        assert!(policy.apdp);
        assert!(!policy.dependency_relaxation);
        assert!(policy.legal_board);
        assert!(policy.conditioned_reachability);
    }
}
