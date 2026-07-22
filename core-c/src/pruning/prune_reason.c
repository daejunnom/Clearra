#include "clr_pruning.h"

#include <string.h>
const char *clr_prune_reason_name(clr_prune_reason reason) {
    switch (reason) {
        case CLR_PRUNE_AREA_OVERFLOW:
            return "AreaOverflow";
        case CLR_PRUNE_PIECE_COUNT_OVERFLOW:
            return "PieceCountOverflow";
        case CLR_PRUNE_PLACEMENT_COLLISION:
            return "PlacementCollision";
        case CLR_PRUNE_TARGET_MASK_OVERFLOW:
            return "TargetMaskOverflow";
        case CLR_PRUNE_ROW_CAPACITY_OVERFLOW:
            return "RowCapacityOverflow";
        case CLR_PRUNE_CELL_DOMAIN_EMPTY_UNDER_CLEAR_STATE:
            return "CellDomainEmptyUnderClearState";
        case CLR_PRUNE_CELL_DOMAIN_EMPTY_FOR_ALL_REACHABLE_CLEAR_STATES:
            return "CellDomainEmptyForAllReachableClearStates";
        case CLR_PRUNE_FORCED_PIECE_FAMILY_UNDER_CLEAR_STATE:
            return "ForcedPieceFamilyUnderClearState";
        case CLR_PRUNE_FORCED_PIECE_FAMILY_FOR_ALL_REACHABLE_CLEAR_STATES:
            return "ForcedPieceFamilyForAllReachableClearStates";
        case CLR_PRUNE_COMPONENT_EXACT_COVER_IMPOSSIBLE:
            return "ComponentExactCoverImpossible";
        case CLR_PRUNE_HOLD_AUTOMATON_IMPOSSIBLE:
            return "HoldAutomatonImpossible";
        case CLR_PRUNE_REACHABILITY_IMPOSSIBLE:
            return "ReachabilityImpossible";
        case CLR_PRUNE_BUILD_ORDERS_HOLD_REACHABLE_INTERSECTION_EMPTY:
            return "BuildOrdersHoldReachableIntersectionEmpty";
        case CLR_PRUNE_RESOURCE_BUDGET_EXCEEDED:
            return "ResourceBudgetExceeded";
        case CLR_PRUNE_LINE_CLEAR_ORDER_IMPOSSIBLE:
            return "LineClearOrderImpossible";
        case CLR_PRUNE_COLUMN_DEMAND_OVERFLOW:
            return "ColumnDemandOverflow";
        case CLR_PRUNE_FULL_PARENT_DOMAIN_EMPTY:
            return "FullParentDomainEmpty";
        case CLR_PRUNE_SAME_TILE_PARENT_DOMAIN_EMPTY:
            return "SameTileParentDomainEmpty";
        case CLR_PRUNE_ADDITIVE_INVARIANT_MISMATCH:
            return "AdditiveInvariantMismatch";
        case CLR_PRUNE_SEPARATOR_COMPONENT_INFEASIBLE:
            return "SeparatorComponentInfeasible";
        case CLR_PRUNE_PARENT_DOMAIN_HALL_VIOLATION:
            return "ParentDomainHallViolation";
        case CLR_PRUNE_COLUMN_DEMAND_UNREACHABLE:
            return "ColumnDemandUnreachable";
        case CLR_PRUNE_BUMPER_DOMAIN_EMPTY:
            return "BumperDomainEmpty";
        case CLR_PRUNE_BUMPER_BRIDGE_INCOMPATIBLE:
            return "BumperBridgeIncompatible";
        case CLR_PRUNE_REALIZATION_DOMAIN_EMPTY:
            return "RealizationDomainEmpty";
        default:
            return "UnknownPruneReason";
    }
}bool clr_prune_reason_is_forbidden_name(const char *name) {
    static const char *forbidden[] = {
        "LooksBad",
        "RareShape",
        "ProbablyImpossible",
        "MctsLowScore",
        "NoImmediatePlacement",
        "ThisCellLooksLikeLOnly",
        "FloatingInTargetFrame",
        "ScoreTooLow",
        "SpinUnknown",
    };

    if (name == 0) {
        return false;
    }

    for (size_t index = 0; index < sizeof(forbidden) / sizeof(forbidden[0]); ++index) {
        if (strcmp(name, forbidden[index]) == 0) {
            return true;
        }
    }
    return false;
}bool clr_prune_reason_has_connected_engine_factory(clr_prune_reason reason) {
    return reason == CLR_PRUNE_PIECE_COUNT_OVERFLOW ||
           reason == CLR_PRUNE_PLACEMENT_COLLISION ||
           reason == CLR_PRUNE_TARGET_MASK_OVERFLOW ||
           reason == CLR_PRUNE_COMPONENT_EXACT_COVER_IMPOSSIBLE ||
           reason == CLR_PRUNE_LINE_CLEAR_ORDER_IMPOSSIBLE ||
           reason == CLR_PRUNE_COLUMN_DEMAND_OVERFLOW ||
           reason == CLR_PRUNE_FULL_PARENT_DOMAIN_EMPTY ||
           reason == CLR_PRUNE_SAME_TILE_PARENT_DOMAIN_EMPTY ||
           reason == CLR_PRUNE_ADDITIVE_INVARIANT_MISMATCH ||
           reason == CLR_PRUNE_SEPARATOR_COMPONENT_INFEASIBLE ||
           reason == CLR_PRUNE_PARENT_DOMAIN_HALL_VIOLATION ||
           reason == CLR_PRUNE_COLUMN_DEMAND_UNREACHABLE ||
           reason == CLR_PRUNE_BUMPER_DOMAIN_EMPTY ||
           reason == CLR_PRUNE_BUMPER_BRIDGE_INCOMPATIBLE ||
           reason == CLR_PRUNE_REALIZATION_DOMAIN_EMPTY;
}bool clr_prune_proof_level_allows_global_prune(clr_prune_proof_level level) {
    return level == CLR_PRUNE_PROOF_GLOBAL_SAFE;
}
