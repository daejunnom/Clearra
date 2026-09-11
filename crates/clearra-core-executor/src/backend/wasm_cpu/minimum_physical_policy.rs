//! Private CPU policy selection, configured before an isolated worker starts.
//! Policy values describe implementations, never physical-validation authority.

use super::geometry_component::{set_component_join_policy, ComponentJoinPolicy};
use super::geometry_domain::{set_geometry_apdp_scan_policy, GeometryApdpScanPolicy};
use super::geometry_domain::{geometry_apdp_scan_counters, set_geometry_apdp_scan_diagnostics};
use super::inverse_projection::{set_inverse_projection_policy, InverseProjectionPolicy};
use super::realization_feasibility::{set_realization_feasibility_policy, RealizationFeasibilityPolicy};
use super::inverse_parent::{set_inverse_parent_policy, InverseParentPolicy};

/// 0 = eager parents/tables, 1 = eager parents without temporal tables,
/// 2 = per-logical-row parent reconstruction without temporal tables.
pub fn set_minimum_physical_parent_ab_policy(policy: u32) -> bool {
    let policy = match policy {
        0 => InverseParentPolicy::EagerTable,
        1 => InverseParentPolicy::EagerRaw,
        2 => InverseParentPolicy::Deferred,
        _ => return false,
    };
    set_inverse_parent_policy(policy);
    true
}

/// Both inputs are validated before either thread-local setting is changed.
/// The WASM ABI additionally rejects mutation while a job owns this worker.
pub fn set_minimum_physical_ab_policy(apdp: u32, component: u32) -> bool {
    let apdp = match apdp {
        0 => GeometryApdpScanPolicy::Legacy,
        1 => GeometryApdpScanPolicy::Off,
        2 => GeometryApdpScanPolicy::Fused,
        _ => return false,
    };
    let component = match component {
        0 => ComponentJoinPolicy::Legacy,
        1 => ComponentJoinPolicy::Off,
        2 => ComponentJoinPolicy::Complement,
        _ => return false,
    };
    set_geometry_apdp_scan_policy(apdp);
    set_component_join_policy(component);
    true
}

pub fn set_minimum_physical_apdp_diagnostics(enabled: bool) {
    set_geometry_apdp_scan_diagnostics(enabled);
}

/// v1 order: advanced, minimum, eligible, legacy, off, fused,
/// completeness rows, recount rows, incomplete parent domains.
pub fn minimum_physical_apdp_scan_counters() -> [u64; 9] {
    let value = geometry_apdp_scan_counters();
    [value.advanced_domain_calls, value.minimum_domain_calls, value.eligible_pivot_calls,
        value.legacy_queries, value.off_queries, value.fused_queries,
        value.completeness_rows, value.recount_rows, value.incomplete_domains]
}

/// Configure before starting catalog compilation in each isolated worker.
pub fn set_minimum_physical_inverse_ab_policy(inverse: u32) -> bool {
    let inverse = match inverse {
        0 => InverseProjectionPolicy::Legacy,
        1 => InverseProjectionPolicy::Off,
        2 => InverseProjectionPolicy::Early,
        _ => return false,
    };
    set_inverse_projection_policy(inverse);
    true
}

/// Keep ordinary physical validation while ablating optional feasibility work.
pub fn set_minimum_physical_feasibility_ab_policy(feasibility: u32) -> bool {
    let policy = match feasibility {
        0 => RealizationFeasibilityPolicy::Legacy,
        1 => RealizationFeasibilityPolicy::Off,
        2 => RealizationFeasibilityPolicy::RelaxationOnly,
        _ => return false,
    };
    set_realization_feasibility_policy(policy);
    true
}
