//! Experimental root-branch streaming under the compiler's exclusive owner.
//! A drained branch is not a complete source. Raw arena IDs never escape here.

use super::*;
use clearra_core_domain::execution_cancellation::ExecutionControl;

#[derive(Debug)]
pub(super) struct OpenRootTraversal {
    stream_branches: bool,
    pending_branch: Option<u32>,
    current: Option<TraversalTask>,
    tasks: Vec<TraversalTask>,
    rows: [u32; MAX_BOARD64_PIECES],
    published: bool,
    compiler_complete: bool,
    poisoned: bool,
    pub(super) branches: u64,
    pub(super) candidates: u64,
}

pub(in crate::backend::wasm_cpu) struct OpenRootObservation {
    pub(in crate::backend::wasm_cpu) advance: GeometryAdvance,
    pub(in crate::backend::wasm_cpu) work_steps: u64,
    /// A conservative admitted peak, not an allocator/RSS measurement.
    pub(in crate::backend::wasm_cpu) retained_peak_upper_bound: u128,
}

impl OpenRootTraversal {
    fn new(stream_branches: bool) -> Self {
        Self {
            stream_branches,
            pending_branch: None,
            current: None,
            tasks: Vec::new(),
            rows: [0; MAX_BOARD64_PIECES],
            published: false,
            compiler_complete: false,
            poisoned: false,
            branches: 0,
            candidates: 0,
        }
    }

    pub(super) fn has_cursor(&self) -> bool {
        self.pending_branch.is_some() || self.current.is_some() || !self.tasks.is_empty()
    }

    pub(super) fn publish(&mut self, branch: u32) -> bool {
        if !self.stream_branches {
            return !self.poisoned;
        }
        self.enqueue(branch)
    }

    fn enqueue(&mut self, branch: u32) -> bool {
        if self.poisoned || self.has_cursor() {
            self.poisoned = true;
            return false;
        }
        if branch != FAMILY_INVALID {
            self.pending_branch = Some(branch);
            self.published = true;
            self.branches = self.branches.saturating_add(1);
        }
        true
    }

    pub(super) fn publish_fallback(&mut self, root: u32) -> bool {
        // Root memo/direct completion may bypass add_branch_to_parent. Normal
        // final union completion must not re-emit earlier published branches.
        if self.published {
            return !self.poisoned;
        }
        self.enqueue(root)
    }

    pub(super) fn retained_bytes(&self) -> usize {
        self.tasks.capacity() * core::mem::size_of::<TraversalTask>()
    }

    fn push(&mut self, task: TraversalTask, live: u128, limit: u128) -> Result<(), ()> {
        if self.tasks.len() == self.tasks.capacity() {
            let next = self.tasks.len().checked_add(1).ok_or(())?;
            let added = (next as u128)
                .checked_mul(core::mem::size_of::<TraversalTask>() as u128)
                .ok_or(())?;
            if live.checked_add(added).ok_or(())? > limit {
                return Err(());
            }
            let mut replacement = Vec::new();
            replacement.try_reserve_exact(next).map_err(|_| ())?;
            let actual = (replacement.capacity() as u128)
                .checked_mul(core::mem::size_of::<TraversalTask>() as u128)
                .ok_or(())?;
            if live.checked_add(actual).ok_or(())? > limit {
                return Err(());
            }
            let previous = core::mem::replace(&mut self.tasks, replacement);
            self.tasks.extend(previous);
        }
        self.tasks.push(task);
        Ok(())
    }

    fn advance_node(
        &mut self,
        family: &GeometrySolutionFamily,
        catalog: &GeometryCatalog,
        targets: &[TargetGroup],
        target_depth: u8,
        live: u128,
        limit: u128,
    ) -> Result<GeometryAdvance, ()> {
        if let Some(branch) = self.pending_branch.take() {
            if self.current.is_some() || !self.tasks.is_empty() {
                return Err(());
            }
            self.current = Some(TraversalTask {
                family: branch,
                continuations: [FAMILY_INVALID; MAX_BOARD64_PIECES],
                depth: 0,
                continuation_count: 0,
            });
        }
        let Some(task) = self.current.take().or_else(|| self.tasks.pop()) else {
            return Ok(GeometryAdvance::Pending);
        };
        match advance_traversal_task(family, catalog, &mut self.rows, target_depth, task)? {
            TraversalStep::Skip => {}
            TraversalStep::Continue(next) => self.current = Some(next),
            TraversalStep::Fork(left, right) => {
                self.push(right, live, limit)?;
                self.current = Some(left);
            }
            TraversalStep::Candidate(depth) => {
                let rows = &self.rows[..usize::from(depth)];
                let mut counts = [0_u8; 7];
                for &row in rows {
                    counts[piece_index(catalog.skeleton(row).piece)] += 1;
                }
                let key = PieceMultisetKey::from_counts(counts);
                let index = targets
                    .binary_search_by_key(&key, |target| target.key)
                    .map_err(|_| ())?;
                let candidate =
                    GeometryCandidate::from_rows(catalog, targets[index].pattern_index_id, rows)
                        .ok_or(())?;
                self.candidates = self.candidates.saturating_add(1);
                return Ok(GeometryAdvance::Candidate(candidate));
            }
        }
        Ok(GeometryAdvance::Pending)
    }
}

impl FamilyCompiler {
    pub(super) fn enable_open_root_streaming(
        &mut self,
        stream_branches: bool,
    ) -> Result<(), &'static str> {
        if !self.resource_authoritative
            || self.tablebase.is_some()
            || self.expanded_nodes != 0
            || self.stack.len() != 1
            || self.stack[0].depth != 0
            || self.used_counts != [0; 7]
            || self.open_root.is_some()
        {
            return Err("open_family_requires_fresh_bounded_root_without_tablebase");
        }
        self.open_root = Some(OpenRootTraversal::new(stream_branches));
        Ok(())
    }

    fn open_root_live_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128).checked_add(self.retained_bytes() as u128)
    }

    fn open_root_step_work(&self, catalog: &GeometryCatalog) -> Option<u64> {
        let stream = self.open_root.as_ref()?;
        let work = if stream.has_cursor() {
            // Covers one node, an entire stack-copy growth, candidate creation
            // and target lookup. No leaf-seeking inner loop is hidden here.
            (stream.tasks.capacity() as u128)
                .checked_mul(2)?
                .checked_add(self.targets.len() as u128)?
                .checked_add(256)?
        } else {
            // Bounded roots disable advanced/component/residual analysis.
            // Include conservative support/bumper scans plus worst-case hash
            // probing and rehashing for all possible union carries in a step.
            let mutations = (2 * UNION_LEVEL_COUNT + 4) as u128;
            let nodes = (self.family.node_count() as u128).checked_add(mutations)?;
            let slots = (self.family.retained_bytes() as u128 / 4)
                .checked_add(mutations.checked_mul(8192)?)?
                .checked_mul(2)?;
            nodes
                .checked_add(1)?
                .checked_mul(slots.checked_add(1)?)?
                .checked_mul(mutations)?
                .checked_add((catalog.skeleton_count() as u128).checked_mul(65536)?)?
                .checked_add((self.admissible_prefixes.len() as u128).checked_mul(64)?)?
                .checked_add((self.targets.len() as u128).checked_mul(64)?)?
                .checked_add(4096)?
        };
        u64::try_from(work).ok()
    }

    /// The caller retains catalog, target pointees and other source owners in
    /// its parent admission. This limit covers this compiler and its cursor.
    /// Exhausted work does not consume a node; structural failures poison the
    /// owner, so a later call cannot manufacture Complete from damaged state.
    pub(super) fn advance_open_root(
        &mut self,
        catalog: &GeometryCatalog,
        remaining_work: u64,
        retained_limit: u128,
        control: &ExecutionControl,
    ) -> OpenRootObservation {
        let mut observed = OpenRootObservation {
            advance: GeometryAdvance::Pending,
            work_steps: 0,
            retained_peak_upper_bound: 0,
        };
        let Some(stream) = self.open_root.as_ref() else {
            observed.advance = GeometryAdvance::ResourceIncomplete("open_family_not_enabled");
            return observed;
        };
        if stream.poisoned || control.is_cancelled() {
            self.open_root.as_mut().unwrap().poisoned = true;
            observed.advance = GeometryAdvance::ResourceIncomplete("open_family_interrupted");
            return observed;
        }
        let Some(live) = self.open_root_live_bytes() else {
            observed.advance =
                GeometryAdvance::ResourceIncomplete("open_family_retention_overflow");
            return observed;
        };
        observed.retained_peak_upper_bound = live;
        if live > retained_limit {
            observed.advance = GeometryAdvance::ResourceIncomplete("open_family_memory_limit");
            return observed;
        }
        let stream = self.open_root.as_ref().unwrap();
        if stream.compiler_complete && !stream.has_cursor() {
            observed.advance = GeometryAdvance::Complete;
            return observed;
        }
        let Some(work) = self
            .open_root_step_work(catalog)
            .filter(|work| *work <= remaining_work)
        else {
            observed.advance = GeometryAdvance::ResourceIncomplete("open_family_work_limit");
            return observed;
        };
        observed.work_steps = work;
        // All allocations inside this call enforce the admitted hard limit.
        observed.retained_peak_upper_bound = retained_limit;
        if self.open_root.as_ref().unwrap().has_cursor() {
            observed.advance = match self.open_root.as_mut().unwrap().advance_node(
                &self.family,
                catalog,
                &self.targets,
                self.target_depth,
                live,
                retained_limit,
            ) {
                Ok(advance) => advance,
                Err(()) => {
                    self.open_root.as_mut().unwrap().poisoned = true;
                    GeometryAdvance::ResourceIncomplete("open_family_cursor_failure")
                }
            };
        } else {
            // Cursor heap is already part of retained_bytes and the existing
            // compiler family-allocation guard subtracts it exactly once.
            let cursor_fixed = Some(core::mem::size_of::<Self>() as u128);
            let Some(data_limit) = cursor_fixed.and_then(|fixed| retained_limit.checked_sub(fixed))
            else {
                observed.advance = GeometryAdvance::ResourceIncomplete("open_family_memory_limit");
                return observed;
            };
            if !self.set_retained_limit_bytes(data_limit) {
                observed.advance = GeometryAdvance::ResourceIncomplete("open_family_memory_limit");
                return observed;
            }
            match self.advance(catalog) {
                CompileAdvance::Pending => {}
                CompileAdvance::Complete => {
                    self.open_root.as_mut().unwrap().compiler_complete = true
                }
                CompileAdvance::ResourceIncomplete => {
                    self.open_root.as_mut().unwrap().poisoned = true;
                    observed.advance =
                        GeometryAdvance::ResourceIncomplete("open_family_compile_failure");
                }
            }
        }
        if control.is_cancelled() {
            self.open_root.as_mut().unwrap().poisoned = true;
            observed.advance = GeometryAdvance::ResourceIncomplete("open_family_interrupted");
        }
        observed
    }
}

impl GeometrySearch {
    pub(in crate::backend::wasm_cpu) fn advance_bounded_root(
        &mut self,
        catalog: &GeometryCatalog,
        stream_branches: bool,
        remaining_work: u64,
        retained_limit: u128,
        control: &ExecutionControl,
    ) -> OpenRootObservation {
        let rejected = |reason| OpenRootObservation {
            advance: GeometryAdvance::ResourceIncomplete(reason),
            work_steps: 0,
            retained_peak_upper_bound: 0,
        };
        if control.is_cancelled() {
            return rejected("open_family_interrupted");
        }
        if !self.resource_authoritative || self.enumerator.is_some() {
            return rejected("open_family_requires_bounded_geometry_source");
        }
        if let Some(preparation) = self.target_preparation.as_ref() {
            // Target-index construction remains its existing cooperative stage.
            // Include its admitted index storage and the prefix enumeration /
            // sorting that can occur when the final target is committed.
            let mut prefixes = Some(0_u128);
            for target in &preparation.targets {
                let count = target.key.counts().iter().try_fold(1_u128, |count, piece| {
                    count.checked_mul(u128::from(*piece) + 1)
                });
                prefixes = prefixes.and_then(|sum| sum.checked_add(count?));
            }
            let work = prefixes
                .and_then(|n| n.checked_mul(n.checked_add(16)?))
                .and_then(|n| {
                    n.checked_add(
                        (preparation.retained_upper_bound_bytes as u128)
                            .checked_add(
                                (preparation.universe.pattern_count() as u128).checked_mul(64)?,
                            )?
                            .checked_add(8192)?
                            .checked_mul(256)?,
                    )
                })
                .and_then(|n| u64::try_from(n).ok());
            let Some(work) = work.filter(|work| *work <= remaining_work) else {
                return rejected("open_family_preparation_work_limit");
            };
            let advance = self.advance_with_retained_limit(catalog, retained_limit);
            return OpenRootObservation {
                advance,
                work_steps: work,
                retained_peak_upper_bound: retained_limit,
            };
        }
        let Some(compiler) = self.compiler.as_mut() else {
            return rejected("open_family_root_owner_missing");
        };
        if let Some(stream) = compiler.open_root.as_ref() {
            if stream.stream_branches != stream_branches {
                return rejected("open_family_policy_changed");
            }
        } else if let Err(reason) = compiler.enable_open_root_streaming(stream_branches) {
            return rejected(reason);
        }
        let fixed = checked_target_nested_retained_bytes(&compiler.targets)
            .and_then(|bytes| bytes.checked_add(self.shared_family_bytes as u128))
            .and_then(|bytes| {
                bytes.checked_add(
                    self.external_targets
                        .as_ref()
                        .map_or(0_u128, |targets| target_bytes(targets) as u128),
                )
            });
        let Some(internal_limit) = fixed.and_then(|fixed| retained_limit.checked_sub(fixed)) else {
            return rejected("open_family_memory_limit");
        };
        let mut observed =
            compiler.advance_open_root(catalog, remaining_work, internal_limit, control);
        observed.retained_peak_upper_bound = retained_limit;
        self.candidate_family_count = None;
        self.observe_compiler_metrics();
        observed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::pc::pc_target::PcTarget;
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{OpeningPcSearchQuery, PcHoldPolicy, PcQueueInput};
    use clearra_problem::ProblemCompiler;
    use std::collections::BTreeSet;

    fn fixture() -> (GeometryCatalog, Arc<[TargetGroup]>) {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::default())
            .with_hold_policy(PcHoldPolicy::Disabled)
            .with_objective(ObjectivePolicy::unique());
        let problem = ProblemCompiler::compile_opening_pc(&query).unwrap();
        let catalog = GeometryCatalog::compile(&problem).unwrap();
        let universe = problem.piece_source().materialized_universe().unwrap();
        let family = universe.packing_multiset_family_for_execution(
            catalog.required_cells().count_ones() as usize / 4,
            problem.initial_hold(),
            problem.supply().hold_enabled(),
            crate::backend::wasm_cpu::packing_hold_projection(&problem),
        );
        let mut search =
            GeometrySearch::new(universe, &family, catalog.required_cells(), false).unwrap();
        for _ in 0..1_000_000 {
            if let Some(compiler) = search.compiler.as_ref() {
                return (catalog, Arc::clone(&compiler.targets));
            }
            assert!(matches!(search.advance(&catalog), GeometryAdvance::Pending));
        }
        panic!("target preparation did not finish");
    }

    fn closed_candidates(
        catalog: &GeometryCatalog,
        targets: Arc<[TargetGroup]>,
    ) -> Vec<GeometryCandidate> {
        let mut compiler =
            FamilyCompiler::try_new(catalog.required_cells(), targets, None).unwrap();
        let mut done = false;
        for _ in 0..1_000_000 {
            match compiler.advance(catalog) {
                CompileAdvance::Pending => {}
                CompileAdvance::Complete => {
                    done = true;
                    break;
                }
                CompileAdvance::ResourceIncomplete => panic!("closed fixture failed"),
            }
        }
        assert!(done);
        let mut enumerator = compiler.into_enumerator();
        let mut result = Vec::new();
        while let Some(candidate) = enumerator.next_candidate(catalog).unwrap() {
            result.push(candidate);
        }
        result
    }

    fn drain(
        compiler: &mut FamilyCompiler,
        catalog: &GeometryCatalog,
    ) -> (Vec<GeometryCandidate>, bool) {
        let control = ExecutionControl::default();
        let mut result = Vec::new();
        let mut early = false;
        for _ in 0..1_000_000 {
            let observed =
                compiler.advance_open_root(catalog, u64::MAX, 64 * 1024 * 1024, &control);
            assert!(observed.retained_peak_upper_bound <= 64 * 1024 * 1024);
            match observed.advance {
                GeometryAdvance::Pending => {}
                GeometryAdvance::Candidate(candidate) => {
                    early |= !compiler.open_root.as_ref().unwrap().compiler_complete;
                    result.push(candidate);
                }
                GeometryAdvance::Complete => return (result, early),
                GeometryAdvance::ResourceIncomplete(reason) => {
                    panic!("open fixture failed: {reason}")
                }
            }
        }
        panic!("bounded root traversal did not finish");
    }

    #[test]
    fn open_root_branches_match_closed_source_and_publish_before_whole_completion() {
        let (catalog, targets) = fixture();
        let closed = closed_candidates(&catalog, Arc::clone(&targets));
        let expected: BTreeSet<_> = closed
            .iter()
            .map(|candidate| (candidate.identity, candidate.target_index))
            .collect();
        assert!(expected.len() > 1);
        let mut compiler =
            FamilyCompiler::try_new(catalog.required_cells(), targets, None).unwrap();
        compiler.enable_open_root_streaming(true).unwrap();
        let paused =
            compiler.advance_open_root(&catalog, 0, 64 * 1024 * 1024, &ExecutionControl::default());
        assert!(matches!(
            paused.advance,
            GeometryAdvance::ResourceIncomplete("open_family_work_limit")
        ));
        assert_eq!(paused.work_steps, 0);
        assert_eq!(compiler.expanded_nodes, 0);
        let (open, early) = drain(&mut compiler, &catalog);
        let actual: BTreeSet<_> = open
            .iter()
            .map(|candidate| (candidate.identity, candidate.target_index))
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(
            open.len(),
            actual.len(),
            "root union carry must not re-publish old branches"
        );
        assert!(early);
        assert!(compiler.open_root.as_ref().unwrap().branches > 1);

        // Same node decoder, budget accounting and interning policy, with
        // publication deferred to the whole root: the controlled A/B reference.
        let mut deferred = FamilyCompiler::try_new(
            catalog.required_cells(),
            Arc::clone(&compiler.targets),
            None,
        )
        .unwrap();
        deferred.enable_open_root_streaming(false).unwrap();
        let (deferred_rows, early) = drain(&mut deferred, &catalog);
        assert!(!early);
        assert_eq!(deferred.open_root.as_ref().unwrap().branches, 1);
        assert_eq!(
            deferred_rows
                .iter()
                .map(|candidate| (candidate.identity, candidate.target_index))
                .collect::<BTreeSet<_>>(),
            expected
        );

        // A directly completed Product root exercises fallback publication and
        // every continuation using a candidate from the independent closed run.
        let candidate = closed[0];
        let mut direct = FamilyCompiler::try_new(
            catalog.required_cells(),
            Arc::clone(&compiler.targets),
            None,
        )
        .unwrap();
        direct.enable_open_root_streaming(true).unwrap();
        let mut left = FAMILY_EMPTY;
        let mut right = FAMILY_EMPTY;
        for &row in &candidate.row_ids()[..2] {
            left = direct.family.append(row, left).unwrap();
        }
        for &row in &candidate.row_ids()[2..] {
            right = direct.family.append(row, right).unwrap();
        }
        let root = direct.family.product(left, right).unwrap();
        assert!(matches!(
            direct.finish_top(&catalog, root, false),
            CompileAdvance::Complete
        ));
        direct.open_root.as_mut().unwrap().compiler_complete = true;
        let (single, early) = drain(&mut direct, &catalog);
        assert!(!early);
        assert_eq!(single.len(), 1);
        assert_eq!(
            (single[0].identity, single[0].target_index),
            (candidate.identity, candidate.target_index)
        );
    }

    #[test]
    fn interrupted_or_structurally_failed_open_root_never_resumes_as_complete() {
        let (catalog, targets) = fixture();
        let mut cancelled =
            FamilyCompiler::try_new(catalog.required_cells(), Arc::clone(&targets), None).unwrap();
        cancelled.enable_open_root_streaming(true).unwrap();
        let control = ExecutionControl::default();
        control.cancellation.handle().cancel();
        let stopped = cancelled.advance_open_root(&catalog, u64::MAX, 64 * 1024 * 1024, &control);
        assert!(matches!(
            stopped.advance,
            GeometryAdvance::ResourceIncomplete("open_family_interrupted")
        ));
        assert!(matches!(
            cancelled
                .advance_open_root(
                    &catalog,
                    u64::MAX,
                    64 * 1024 * 1024,
                    &ExecutionControl::default()
                )
                .advance,
            GeometryAdvance::ResourceIncomplete("open_family_interrupted")
        ));

        let mut limited = FamilyCompiler::try_new(catalog.required_cells(), targets, None).unwrap();
        limited.enable_open_root_streaming(true).unwrap();
        let cap = limited.open_root_live_bytes().unwrap();
        let mut failed = false;
        for _ in 0..1_000_000 {
            match limited
                .advance_open_root(&catalog, u64::MAX, cap, &ExecutionControl::default())
                .advance
            {
                GeometryAdvance::Pending => {}
                GeometryAdvance::ResourceIncomplete(_) => {
                    failed = true;
                    break;
                }
                _ => panic!("a nonempty family cannot finish without node storage"),
            }
        }
        assert!(failed);
        assert!(limited.open_root.as_ref().unwrap().poisoned);
        assert!(matches!(
            limited
                .advance_open_root(
                    &catalog,
                    u64::MAX,
                    64 * 1024 * 1024,
                    &ExecutionControl::default()
                )
                .advance,
            GeometryAdvance::ResourceIncomplete("open_family_interrupted")
        ));
    }
}
