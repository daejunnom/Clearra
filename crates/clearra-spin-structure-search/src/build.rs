use std::time::Instant;

#[cfg(test)]
use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    board::place_and_clear,
    minimal,
    model::{SpinStructureError, SpinStructureQuery, SpinStructureReport, SpinStructureTask},
    structural_search,
};

#[cfg(test)]
use crate::{
    board::StructureBoard,
    entry::EntryCatalog,
    logical::{apply_physical_lock, DeletedLogicalRows, LogicalBoard},
    model::{
        LayerMetrics, MinimalityPolicy, PieceInventory, SpinStructureOutcome,
        SpinStructureStageMetrics, StructureOperation, StructurePlacement,
    },
    verify,
};

#[cfg(test)]
#[derive(Clone, Debug)]
struct BuildState {
    board: StructureBoard,
    logical_board: LogicalBoard,
    deleted_rows: DeletedLogicalRows,
    remaining: PieceInventory,
    build: Vec<StructurePlacement>,
    logical_operations: Vec<StructureOperation>,
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BuildStateKey {
    logical_board: LogicalBoard,
    deleted_rows: DeletedLogicalRows,
    remaining: PieceInventory,
    operations: Vec<StructureOperation>,
}

/// Stateless entry point for the separate unordered-inventory engine.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpinStructureSearcher;

/// Stable singular alias used by host integrations.
pub type SpinStructureSearch = SpinStructureSearcher;

macro_rules! fixed_mode_search {
    ($name:ident, $mode:expr) => {
        #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
        pub struct $name;

        impl $name {
            pub const fn mode() -> crate::model::SpinStructureMode {
                $mode
            }

            pub fn run(
                mut query: SpinStructureQuery,
            ) -> Result<SpinStructureReport, SpinStructureError> {
                query.mode = $mode;
                SpinStructureSearcher::run(query)
            }

            pub fn partition(
                mut query: SpinStructureQuery,
            ) -> Result<Vec<SpinStructureTask>, SpinStructureError> {
                query.mode = $mode;
                SpinStructureSearcher::partition(query)
            }
        }
    };
}

fixed_mode_search!(
    TSpinStructureSearch,
    crate::model::SpinStructureMode::TSpins
);
fixed_mode_search!(
    TSpinPlusStructureSearch,
    crate::model::SpinStructureMode::TSpinsPlus
);
fixed_mode_search!(
    AllMiniStructureSearch,
    crate::model::SpinStructureMode::AllMini
);
fixed_mode_search!(
    AllMiniPlusStructureSearch,
    crate::model::SpinStructureMode::AllMiniPlus
);
fixed_mode_search!(
    AllSpinStructureSearch,
    crate::model::SpinStructureMode::AllSpin
);
fixed_mode_search!(
    AllSpinPlusStructureSearch,
    crate::model::SpinStructureMode::AllSpinPlus
);

impl SpinStructureSearcher {
    /// Validate and normalize the public pre-spawn field snapshot exactly as
    /// every search entry point does. Hosts that retain the query for result
    /// identity validation must retain this normalized value, not the
    /// pre-clear transport snapshot.
    pub fn normalize_query(
        query: SpinStructureQuery,
    ) -> Result<SpinStructureQuery, SpinStructureError> {
        prepare_query(query)
    }

    pub fn run(query: SpinStructureQuery) -> Result<SpinStructureReport, SpinStructureError> {
        let query = Self::normalize_query(query)?;
        let tasks = partition_prepared(query.clone());
        if tasks.is_empty() {
            return Ok(empty_report(query));
        }
        let reports = tasks
            .into_iter()
            .map(Self::run_task)
            .collect::<Result<Vec<_>, _>>()?;
        Self::merge_task_reports(reports, 1)
    }

    pub fn partition(
        query: SpinStructureQuery,
    ) -> Result<Vec<SpinStructureTask>, SpinStructureError> {
        Ok(partition_prepared(Self::normalize_query(query)?))
    }

    pub fn run_task(task: SpinStructureTask) -> Result<SpinStructureReport, SpinStructureError> {
        task.query.validate()?;
        Ok(structural_search::run_target(
            task.query,
            &task.catalog,
            task.target,
        ))
    }

    pub fn merge_task_reports(
        reports: impl IntoIterator<Item = SpinStructureReport>,
        workers_used: u16,
    ) -> Result<SpinStructureReport, SpinStructureError> {
        let mut reports = reports.into_iter();
        let Some(mut merged) = reports.next() else {
            return Ok(SpinStructureReport {
                workers_used: workers_used.max(1),
                complete: true,
                ..SpinStructureReport::default()
            });
        };
        for report in reports {
            merged = merge_reports(merged, report, workers_used)?;
        }
        merged.workers_used = workers_used.max(1);
        let finalization_started = Instant::now();
        let mut merged = minimal::finalize(merged);
        structural_search::refresh_layer_acceptance(&mut merged);
        merged.timings.finalization_ns = merged
            .timings
            .finalization_ns
            .saturating_add(duration_ns(finalization_started.elapsed()));
        Ok(merged)
    }
}

fn prepare_query(mut query: SpinStructureQuery) -> Result<SpinStructureQuery, SpinStructureError> {
    // A field handed to a search represents the state immediately before the
    // next piece spawns. Completed input rows have therefore already cleared.
    // Normalize once at the engine boundary so serial runs, independently
    // scheduled tasks, reports, and exported solution keys share one board.
    query.validate()?;
    query.initial_board = place_and_clear(query.height, query.initial_board).0;
    Ok(query)
}

fn partition_prepared(query: SpinStructureQuery) -> Vec<SpinStructureTask> {
    let catalog = structural_search::compile_catalog(&query);
    structural_search::target_operations(&query, &catalog)
        .into_iter()
        .map(|target| SpinStructureTask {
            query: query.clone(),
            catalog: catalog.clone(),
            target,
        })
        .collect()
}

/// Slow exact forward oracle retained for small structural regression fixtures.
/// Product dispatch uses the target-first pipeline above.
#[cfg(test)]
pub(crate) fn run_exhaustive(
    query: SpinStructureQuery,
) -> Result<SpinStructureReport, SpinStructureError> {
    let query = SpinStructureSearcher::normalize_query(query)?;
    if query.mode.t_only() && query.inventory.count(PieceKind::T) == 0 {
        return Ok(empty_report(query));
    }
    let (logical_board, deleted_rows, normalized_initial) = initial_context(&query);
    let initial = BuildState {
        board: normalized_initial,
        logical_board,
        deleted_rows,
        remaining: query.inventory,
        build: Vec::new(),
        logical_operations: Vec::new(),
    };
    Ok(run_states(query, vec![initial], 1))
}

#[cfg(test)]
fn run_states(
    query: SpinStructureQuery,
    mut layer: Vec<BuildState>,
    starting_depth: u8,
) -> SpinStructureReport {
    let mut report = SpinStructureReport {
        query: Some(query.clone()),
        workers_used: 1,
        complete: true,
        ..SpinStructureReport::default()
    };
    let mut entry = EntryCatalog::new(query.height, query.rule_profile);
    let limit = query.placement_limit();
    for depth in starting_depth..=limit {
        if layer.is_empty() {
            break;
        }
        let mut metrics = LayerMetrics {
            depth,
            input_states: layer.len() as u64,
            ..LayerMetrics::default()
        };
        report.stages.build_states += layer.len() as u64;
        let mut next = Vec::new();
        for state in layer {
            if query.mode.t_only() && state.remaining.count(PieceKind::T) == 0 {
                continue;
            }
            for piece in state.remaining.available() {
                metrics.piece_choices += 1;
                let remaining = state.remaining.take(piece).expect("available piece");
                let target_capable = !query.mode.t_only() || piece == PieceKind::T;
                let entry_result =
                    entry.reachable_locks(state.board, piece, target_capable, target_capable);
                report.stages.entry_states += entry_result.visited_states;
                report.stages.support_locks += entry_result.locks.len() as u64;
                metrics.reachable_locks += entry_result.locks.len() as u64;
                for lock in entry_result.locks {
                    let occupied = state.board.union(lock.mask);
                    let (board_after, cleared_rows, cleared_lines) =
                        place_and_clear(query.height, occupied);
                    let Some(logical_lock) = apply_physical_lock(
                        state.logical_board,
                        state.deleted_rows,
                        query.height,
                        piece,
                        lock.rotation,
                        lock.x,
                        lock.mask,
                    ) else {
                        continue;
                    };
                    debug_assert_eq!(
                        logical_lock
                            .board_after
                            .compact(logical_lock.deleted_after, query.height),
                        board_after
                    );
                    let placement =
                        verify::placement_from_lock(piece, lock, cleared_rows, cleared_lines);
                    report.stages.fill_checks += 1;
                    report.stages.corner_checks += 1;
                    report.stages.verification_checks += 1;
                    metrics.terminal_candidates += 1;
                    let event = verify::classify_lock(
                        &query,
                        state.board,
                        board_after,
                        piece,
                        lock,
                        cleared_rows,
                        logical_lock.newly_deleted_rows,
                        cleared_lines,
                    );
                    if let Some(event) = event {
                        let mut build = state.build.clone();
                        build.push(placement);
                        let mut logical_operations = state.logical_operations.clone();
                        logical_operations.push(logical_lock.identity);
                        let outcome = SpinStructureOutcome {
                            board_before_spin: state.board,
                            final_board: board_after,
                            spin: placement,
                            build,
                            mini: event.is_mini(),
                            logical_operations,
                            logical_spin: logical_lock.identity,
                            logical_spin_cleared_rows: logical_lock.newly_deleted_rows,
                        };
                        if event.is_mini() {
                            metrics.accepted_mini += 1;
                            report.mini.push(outcome);
                        } else {
                            metrics.accepted_regular += 1;
                            report.regular.push(outcome);
                        }
                    }
                    if depth < limit && (!query.mode.t_only() || remaining.count(PieceKind::T) != 0)
                    {
                        let mut build = state.build.clone();
                        build.push(placement);
                        let mut logical_operations = state.logical_operations.clone();
                        logical_operations.push(logical_lock.identity);
                        next.push(BuildState {
                            board: board_after,
                            logical_board: logical_lock.board_after,
                            deleted_rows: logical_lock.deleted_after,
                            remaining,
                            build,
                            logical_operations,
                        });
                        metrics.generated_states += 1;
                    }
                }
            }
        }
        deduplicate_states(&mut next, &mut metrics, &mut report.stages);
        let accepted = metrics.accepted_regular + metrics.accepted_mini != 0;
        report.layers.push(metrics);
        if accepted && query.minimality == MinimalityPolicy::MinimumPieceCount {
            break;
        }
        layer = next;
    }
    minimal::finalize(report)
}

#[cfg(test)]
fn deduplicate_states(
    states: &mut Vec<BuildState>,
    layer: &mut LayerMetrics,
    stages: &mut SpinStructureStageMetrics,
) {
    states.sort_by_key(state_key);
    let before = states.len();
    states.dedup_by(|left, right| state_key(left) == state_key(right));
    let duplicates = (before - states.len()) as u64;
    layer.exact_duplicates += duplicates;
    stages.exact_state_deduplications += duplicates;
}

#[cfg(test)]
fn state_key(state: &BuildState) -> BuildStateKey {
    let mut operations = state.logical_operations.clone();
    operations.sort();
    BuildStateKey {
        logical_board: state.logical_board,
        deleted_rows: state.deleted_rows,
        remaining: state.remaining,
        operations,
    }
}

#[cfg(test)]
fn initial_context(
    query: &SpinStructureQuery,
) -> (LogicalBoard, DeletedLogicalRows, StructureBoard) {
    let logical_board = LogicalBoard::from_initial(query.initial_board);
    let deleted_rows = logical_board.initial_deleted_rows(query.height);
    let normalized_initial = logical_board.compact(deleted_rows, query.height);
    debug_assert_eq!(
        normalized_initial,
        place_and_clear(query.height, query.initial_board).0
    );
    (logical_board, deleted_rows, normalized_initial)
}

fn merge_reports(
    mut left: SpinStructureReport,
    mut right: SpinStructureReport,
    workers_used: u16,
) -> Result<SpinStructureReport, SpinStructureError> {
    if left.query != right.query {
        return Err(SpinStructureError::IncompatibleTaskReports);
    }
    left.timings.absorb(right.timings);
    left.regular.append(&mut right.regular);
    left.mini.append(&mut right.mini);
    for right_layer in right.layers {
        if let Some(left_layer) = left
            .layers
            .iter_mut()
            .find(|left_layer| left_layer.depth == right_layer.depth)
        {
            left_layer.input_states += right_layer.input_states;
            left_layer.piece_choices += right_layer.piece_choices;
            left_layer.reachable_locks += right_layer.reachable_locks;
            left_layer.generated_states += right_layer.generated_states;
            left_layer.exact_duplicates += right_layer.exact_duplicates;
            left_layer.terminal_candidates += right_layer.terminal_candidates;
            left_layer.accepted_regular += right_layer.accepted_regular;
            left_layer.accepted_mini += right_layer.accepted_mini;
        } else {
            left.layers.push(right_layer);
        }
    }
    left.layers.sort_by_key(|layer| layer.depth);
    minimal::merge_stage_metrics(&mut left.stages, right.stages);
    left.complete &= right.complete;
    left.workers_used = workers_used.max(1);
    Ok(left)
}

fn duration_ns(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn empty_report(query: SpinStructureQuery) -> SpinStructureReport {
    SpinStructureReport {
        query: Some(query),
        workers_used: 1,
        complete: true,
        ..SpinStructureReport::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PieceInventory, SpinLineRequirement, SpinStructureMode};
    use clearra_rules::profile::rule_profile::RuleProfileId;

    type ExactOutcomeKey = (Vec<StructureOperation>, StructureOperation, u32, bool);

    fn exact_outcomes(report: &SpinStructureReport) -> Vec<ExactOutcomeKey> {
        let mut outcomes = report
            .outcomes()
            .map(|outcome| {
                let mut operations = outcome.logical_operations().to_vec();
                operations.sort_unstable();
                (
                    operations,
                    outcome.logical_spin(),
                    outcome.logical_spin_cleared_rows(),
                    outcome.is_mini(),
                )
            })
            .collect::<Vec<_>>();
        outcomes.sort_unstable();
        outcomes
    }

    fn assert_product_matches_exhaustive(query: SpinStructureQuery) -> SpinStructureReport {
        let product = SpinStructureSearcher::run(query.clone()).expect("target-first search");
        let exhaustive = run_exhaustive(query).expect("exhaustive oracle");
        assert_eq!(exact_outcomes(&product), exact_outcomes(&exhaustive));
        product
    }

    fn one_piece_query(
        piece: PieceKind,
        mode: SpinStructureMode,
        initial_cells: &[(u8, u8)],
    ) -> SpinStructureQuery {
        let inventory = PieceInventory::from_pieces([piece]).expect("one-piece inventory");
        let mut query = SpinStructureQuery::new(inventory, mode);
        query.height = 4;
        query.fill_top = 4;
        query.line_requirement = SpinLineRequirement::Any;
        query.max_placements = Some(1);
        query.initial_board = initial_cells
            .iter()
            .copied()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).expect("fixture cell")
            });
        query
    }

    #[test]
    fn no_t_supply_is_an_exact_empty_t_only_result() {
        let inventory =
            PieceInventory::from_pieces([PieceKind::I, PieceKind::O]).expect("inventory");
        let report = SpinStructureSearcher::run(SpinStructureQuery::new(
            inventory,
            SpinStructureMode::TSpins,
        ))
        .expect("search");
        assert!(report.complete);
        assert_eq!(report.outcome_count(), 0);
    }

    #[test]
    fn mode_parsers_are_stable() {
        for mode in crate::SpinStructureMode::ALL {
            assert_eq!(crate::SpinStructureMode::parse(mode.as_str()), Some(mode));
        }
    }

    #[test]
    fn serial_and_independent_task_outcomes_match_on_small_exhaustive_fixture() {
        let inventory = PieceInventory::from_pieces([PieceKind::T]).expect("inventory");
        let mut query = SpinStructureQuery::new(inventory, SpinStructureMode::TSpins);
        query.height = 4;
        query.fill_top = 4;
        query.line_requirement = SpinLineRequirement::Any;
        query.max_placements = Some(1);
        query.initial_board = [(4, 2), (6, 2), (4, 0)]
            .into_iter()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).expect("fixture cell")
            });

        let serial = SpinStructureSearcher::run(query.clone()).expect("serial search");
        let task_reports = SpinStructureSearcher::partition(query)
            .expect("tasks")
            .into_iter()
            .map(SpinStructureSearcher::run_task)
            .collect::<Result<Vec<_>, _>>()
            .expect("task reports");
        let parallel = SpinStructureSearcher::merge_task_reports(task_reports, 4)
            .expect("merged task reports");

        assert!(!serial.regular.is_empty());
        assert_eq!(serial.regular, parallel.regular);
        assert_eq!(serial.mini, parallel.mini);
        assert_eq!(parallel.workers_used(), 4);
    }

    #[test]
    fn completed_input_rows_clear_before_serial_and_partitioned_search() {
        let inventory = PieceInventory::from_pieces([PieceKind::T]).expect("inventory");
        let mut normalized = SpinStructureQuery::new(inventory, SpinStructureMode::TSpins);
        normalized.height = 4;
        normalized.fill_top = 4;
        normalized.line_requirement = SpinLineRequirement::Any;
        normalized.max_placements = Some(1);
        normalized.initial_board = [(4, 2), (6, 2), (4, 0)]
            .into_iter()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).expect("fixture cell")
            });

        let mut with_completed_bottom_row = normalized.clone();
        with_completed_bottom_row.initial_board = StructureBoard::from_rows(&[
            0x03ff,
            normalized.initial_board.row_bits(0),
            normalized.initial_board.row_bits(1),
            normalized.initial_board.row_bits(2),
        ])
        .expect("completed-row fixture");

        let expected = SpinStructureSearcher::run(normalized).expect("normalized search");
        let actual = SpinStructureSearcher::run(with_completed_bottom_row.clone())
            .expect("search with completed input row");
        assert_eq!(actual.query, expected.query);
        assert_eq!(actual.regular, expected.regular);
        assert_eq!(actual.mini, expected.mini);
        assert_eq!(actual.layers, expected.layers);

        let tasks = SpinStructureSearcher::partition(with_completed_bottom_row)
            .expect("partitioned search");
        assert!(!tasks.is_empty());
        assert!(tasks
            .iter()
            .all(|task| task.query.initial_board == actual.query.as_ref().unwrap().initial_board));
    }

    #[test]
    fn target_first_matches_the_t_oracle_for_both_t_profiles() {
        let fixture = [(4, 2), (6, 2), (4, 0)];
        let ordinary = assert_product_matches_exhaustive(one_piece_query(
            PieceKind::T,
            SpinStructureMode::TSpins,
            &fixture,
        ));
        let plus = assert_product_matches_exhaustive(one_piece_query(
            PieceKind::T,
            SpinStructureMode::TSpinsPlus,
            &fixture,
        ));

        assert!(!ordinary.regular.is_empty());
        assert_eq!(exact_outcomes(&ordinary), exact_outcomes(&plus));
    }

    #[test]
    fn target_first_matches_all_four_non_t_oracles_and_only_changes_the_label() {
        // Horizontal I at x=0 is locked by the floor, left wall, (4, 0), and
        // the upward blocker at (1, 1). The exact entry witness ends in a
        // rotation and is immobile before clear.
        let fixture = [(4, 0), (1, 1)];
        let mut reports = Vec::new();
        for mode in [
            SpinStructureMode::AllMini,
            SpinStructureMode::AllMiniPlus,
            SpinStructureMode::AllSpin,
            SpinStructureMode::AllSpinPlus,
        ] {
            let report =
                assert_product_matches_exhaustive(one_piece_query(PieceKind::I, mode, &fixture));
            assert_eq!(report.outcome_count(), 1, "mode={}", mode.as_str());
            let outcome = report.outcomes().next().expect("I-spin outcome");
            assert_eq!(outcome.spin.piece, PieceKind::I);
            assert!(outcome.spin.evidence.last_action_was_rotation());
            assert!(outcome.spin.evidence.immobile_before_clear());
            reports.push((mode, report));
        }

        let geometry = |report: &SpinStructureReport| {
            exact_outcomes(report)
                .into_iter()
                .map(|(operations, target, cleared_rows, _mini)| (operations, target, cleared_rows))
                .collect::<Vec<_>>()
        };
        for (_, report) in reports.iter().skip(1) {
            assert_eq!(geometry(&reports[0].1), geometry(report));
        }
        for (mode, report) in reports {
            let outcome = report.outcomes().next().expect("I-spin outcome");
            assert_eq!(
                outcome.is_mini(),
                matches!(
                    mode,
                    SpinStructureMode::AllMini | SpinStructureMode::AllMiniPlus
                ),
                "mode={}",
                mode.as_str(),
            );
        }
    }

    #[test]
    fn reference_depth_two_has_two_minis_and_task_merge_parity() {
        let inventory = PieceInventory::parse("IOTSZ").expect("inventory");
        let mut query = SpinStructureQuery::new(inventory, SpinStructureMode::TSpins);
        query.initial_board = StructureBoard::from_words([0x0000_0280_f8ff_ff8f, 0, 0, 0]);
        query.height = 7;
        query.fill_bottom = 0;
        query.fill_top = 5;
        query.line_requirement = SpinLineRequirement::AtLeast(1);
        query.rule_profile = RuleProfileId::Srs;
        query.max_placements = Some(2);
        query.minimality = MinimalityPolicy::SubsetMinimal;

        let serial = SpinStructureSearcher::run(query.clone()).expect("serial search");
        assert!(serial.regular.is_empty());
        assert_eq!(serial.mini.len(), 2);

        let task_reports = SpinStructureSearcher::partition(query)
            .expect("tasks")
            .into_iter()
            .map(SpinStructureSearcher::run_task)
            .collect::<Result<Vec<_>, _>>()
            .expect("task reports");
        let merged = SpinStructureSearcher::merge_task_reports(task_reports, 4)
            .expect("merged task reports");
        assert_eq!(serial.regular, merged.regular);
        assert_eq!(serial.mini, merged.mini);
        assert_eq!(merged.workers_used(), 4);
    }
}
