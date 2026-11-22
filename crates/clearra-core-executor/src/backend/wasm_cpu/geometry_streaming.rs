//! Incremental exact-cover traversal for the explicit global-probability route.
//!
//! Every state chooses one common cell using the ordinary minimum domain.
//! A completed tiling contains exactly one allowed row covering that cell, so
//! its children form a disjoint, exhaustive partition. Piece-count prefixes
//! are the existing exact projection of the target keys. No physical-motion
//! heuristic, queue-language membership, or probabilistic pruning is used here.
use super::*;

struct StreamingShared {
    targets: Arc<[TargetGroup]>,
    admissible_prefixes: Vec<u32>,
    target_depth: u8,
}

impl StreamingShared {
    fn retained_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            .saturating_add(2 * core::mem::size_of::<usize>()) // Arc strong/weak counts
            .saturating_add(target_bytes(&self.targets))
            .saturating_add(self.admissible_prefixes.capacity() * core::mem::size_of::<u32>())
    }
}

pub(super) struct StreamingGeometry {
    shared: Arc<StreamingShared>,
    // Reuse the fresh compiler's allocation. Bounded compilers already reserve
    // the full, at-most-MAX_BOARD64_PIECES+1 depth; traversal never grows it.
    stack: Vec<CompileFrame>,
    rows: [u32; MAX_BOARD64_PIECES],
    used_counts: [u8; 7],
    accounts_shared: bool,
    pub(super) expanded_nodes: usize,
    pub(super) peak_frontier: usize,
    pub(super) domain_pruned_states: usize,
    emitted: u128,
}

impl StreamingGeometry {
    pub(super) fn can_start(compiler: &FamilyCompiler) -> bool {
        let [root] = compiler.stack.as_slice() else {
            return false;
        };
        !root.entered
            && root.depth == 0
            && root.chosen_row == NO_ROW
            && compiler.used_counts == [0; 7]
            && compiler.expanded_nodes == 0
            && compiler.compile_domain == FamilyCompileDomain::PermutationClosedGeometry
            && usize::from(compiler.target_depth) <= MAX_BOARD64_PIECES
            && compiler
                .targets
                .iter()
                .all(|target| target.key.total_count() == compiler.target_depth)
            && compiler
                .targets
                .windows(2)
                .all(|pair| pair[0].key < pair[1].key)
    }

    pub(super) fn from_compiler(
        mut compiler: FamilyCompiler,
    ) -> Result<Self, WasmExactSearchError> {
        if !Self::can_start(&compiler) {
            return Err(streaming_state_error());
        }
        let required_capacity = usize::from(compiler.target_depth) + 1;
        let new_shared_bytes =
            core::mem::size_of::<StreamingShared>() + 2 * core::mem::size_of::<usize>();
        if compiler.resource_authoritative {
            // The transition introduces only the small shared owner. Retire
            // the family's caches before allocating it; never grow the live
            // payload beyond the already-admitted fresh compiler's footprint.
            let retained_after = target_bytes(&compiler.targets)
                .saturating_add(
                    compiler.admissible_prefixes.capacity() * core::mem::size_of::<u32>(),
                )
                .saturating_add(compiler.stack.capacity() * core::mem::size_of::<CompileFrame>())
                .saturating_add(new_shared_bytes);
            if compiler.stack.capacity() < required_capacity
                || retained_after > compiler.retained_bytes()
            {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_geometry_streaming_transition_memory_capacity_exceeded",
                ));
            }
        }
        let targets = Arc::clone(&compiler.targets);
        let admissible_prefixes = std::mem::take(&mut compiler.admissible_prefixes);
        let mut stack = std::mem::take(&mut compiler.stack);
        let target_depth = compiler.target_depth;
        drop(compiler);
        stack
            .try_reserve_exact(required_capacity - stack.len())
            .map_err(|_| streaming_storage_error())?;
        Ok(Self {
            shared: Arc::new(StreamingShared {
                targets,
                admissible_prefixes,
                target_depth,
            }),
            stack,
            rows: [0; MAX_BOARD64_PIECES],
            used_counts: [0; 7],
            accounts_shared: true,
            expanded_nodes: 0,
            peak_frontier: 1,
            domain_pruned_states: 0,
            emitted: 0,
        })
    }

    pub(super) fn targets(&self) -> &[TargetGroup] {
        &self.shared.targets
    }

    pub(super) fn checked_target_nested_retained_bytes(&self) -> Option<u128> {
        if self.accounts_shared {
            checked_target_nested_retained_bytes(self.targets())
        } else {
            // The parallel plan owns this same immutable backing once.
            Some(0)
        }
    }

    pub(super) fn retained_bytes(&self) -> usize {
        (self.stack.capacity() * core::mem::size_of::<CompileFrame>()).saturating_add(
            if self.accounts_shared {
                self.shared.retained_bytes()
            } else {
                0
            },
        )
    }

    pub(super) fn completed_count(&self) -> Option<u128> {
        self.stack.is_empty().then_some(self.emitted)
    }

    fn pop(&mut self, catalog: &GeometryCatalog) {
        let frame = self.stack.pop().expect("streaming frame exists");
        if frame.chosen_row != NO_ROW {
            self.used_counts[piece_index(catalog.skeleton(frame.chosen_row).piece)] -= 1;
        }
    }

    pub(super) fn advance(&mut self, catalog: &GeometryCatalog) -> GeometryAdvance {
        let Some(frame) = self.stack.last().copied() else {
            return GeometryAdvance::Complete;
        };
        if !frame.entered {
            self.expanded_nodes = self.expanded_nodes.saturating_add(1);
            if frame.remaining == 0 {
                let target_index = (frame.depth == self.shared.target_depth)
                    .then(|| {
                        let key = PieceMultisetKey::from_counts(self.used_counts);
                        self.shared
                            .targets
                            .binary_search_by_key(&key, |target| target.key)
                            .ok()
                    })
                    .flatten();
                let candidate = target_index.map(|index| {
                    u32::try_from(index).ok().and_then(|index| {
                        GeometryCandidate::from_rows(
                            catalog,
                            index,
                            &self.rows[..usize::from(frame.depth)],
                        )
                    })
                });
                self.pop(catalog);
                return match candidate {
                    Some(Some(candidate)) => {
                        let Some(count) = self.emitted.checked_add(1) else {
                            return GeometryAdvance::ResourceIncomplete(
                                "wasm_geometry_streaming_count_overflow",
                            );
                        };
                        self.emitted = count;
                        GeometryAdvance::Candidate(candidate)
                    }
                    Some(None) => GeometryAdvance::ResourceIncomplete(
                        "wasm_geometry_streaming_candidate_invalid",
                    ),
                    None => GeometryAdvance::Pending,
                };
            }
            if frame.depth >= self.shared.target_depth
                || frame.remaining.count_ones()
                    != 4 * u32::from(self.shared.target_depth - frame.depth)
            {
                self.pop(catalog);
                return GeometryAdvance::Pending;
            }
            let feasible =
                feasible_piece_mask_for(&self.shared.admissible_prefixes, self.used_counts);
            let (status, domain) =
                DomainPropagation::compile_minimum(catalog, frame.remaining, feasible);
            if status == DomainStatus::Empty {
                self.domain_pruned_states = self.domain_pruned_states.saturating_add(1);
                self.pop(catalog);
                return GeometryAdvance::Pending;
            }
            let frame = self.stack.last_mut().expect("streaming frame exists");
            frame.entered = true;
            frame.domain = domain;
            frame.feasible_piece_mask = feasible;
            frame.support_cell = domain.pivot_cell;
            frame.support_end = catalog.support(domain.pivot_cell).len();
            return GeometryAdvance::Pending;
        }
        if frame.support_cursor == frame.support_end {
            self.pop(catalog);
            return GeometryAdvance::Pending;
        }
        let row_id = catalog.support(frame.support_cell)[frame.support_cursor];
        self.stack
            .last_mut()
            .expect("streaming frame exists")
            .support_cursor += 1;
        if !frame
            .domain
            .row_allowed(catalog, row_id, frame.remaining, frame.feasible_piece_mask)
        {
            return GeometryAdvance::Pending;
        }
        if self.stack.len() == self.stack.capacity()
            || usize::from(frame.depth) >= MAX_BOARD64_PIECES
        {
            return GeometryAdvance::ResourceIncomplete(
                "wasm_geometry_streaming_stack_capacity_exceeded",
            );
        }
        let row = catalog.skeleton(row_id);
        self.rows[usize::from(frame.depth)] = row_id;
        self.used_counts[piece_index(row.piece)] += 1;
        self.stack.push(CompileFrame::child(
            frame.remaining ^ row.cells,
            frame.depth + 1,
            row_id,
            0,
        ));
        self.peak_frontier = self.peak_frontier.max(self.stack.len());
        GeometryAdvance::Pending
    }
}

fn streaming_state_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("wasm_geometry_streaming_fresh_root_required")
}

fn streaming_storage_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("wasm_geometry_streaming_storage_unavailable")
}

#[cfg(feature = "parallel")]
#[derive(Clone, Copy)]
struct StreamingSeed {
    remaining: u64,
    rows: [u32; MAX_BOARD64_PIECES],
    used_counts: [u8; 7],
    depth: u8,
    splittable: bool,
}

#[cfg(feature = "parallel")]
impl StreamingSeed {
    fn child(self, catalog: &GeometryCatalog, row_id: u32) -> Self {
        let row = catalog.skeleton(row_id);
        let mut child = self;
        child.remaining ^= row.cells;
        child.rows[usize::from(self.depth)] = row_id;
        child.used_counts[piece_index(row.piece)] += 1;
        child.depth += 1;
        child
    }
}

#[cfg(feature = "parallel")]
impl GeometrySearch {
    pub(super) fn prepare_streaming_parallel(
        self,
        catalog: &GeometryCatalog,
        control: &ExecutionControl,
        requested_workers: usize,
    ) -> Result<ParallelGeometryPreparation, WasmExactSearchError> {
        let streaming = self.streaming.as_ref().ok_or_else(streaming_state_error)?;
        if requested_workers < 2
            || streaming.expanded_nodes != 0
            || streaming.stack.len() != 1
            || streaming.stack[0].depth != 0
        {
            return Ok(ParallelGeometryPreparation::Serial {
                geometry: self,
                reason: "streaming-root-not-splittable",
            });
        }
        let desired = requested_workers
            .checked_mul(8)
            .ok_or_else(streaming_storage_error)?;
        let mut seeds = Vec::new();
        seeds
            .try_reserve_exact(desired)
            .map_err(|_| streaming_storage_error())?;
        seeds.push(StreamingSeed {
            remaining: streaming.stack[0].remaining,
            rows: streaming.rows,
            used_counts: streaming.used_counts,
            depth: 0,
            splittable: true,
        });
        let mut expanded_nodes = 0_usize;
        let mut domain_pruned_states = 0_usize;
        while seeds.len() < desired {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            // Breadth-first prefix refinement keeps coordinator work bounded:
            // at most target_depth refinements per final partition, without
            // walking the full candidate domain to estimate its size.
            let Some(index) = seeds
                .iter()
                .enumerate()
                .filter(|(_, seed)| seed.splittable)
                .min_by_key(|(_, seed)| seed.depth)
                .map(|(index, _)| index)
            else {
                break;
            };
            let seed = seeds[index];
            if seed.remaining == 0
                || seed.depth >= streaming.shared.target_depth
                || seed.remaining.count_ones()
                    != 4 * u32::from(streaming.shared.target_depth - seed.depth)
            {
                seeds[index].splittable = false;
                continue;
            }
            expanded_nodes = expanded_nodes.saturating_add(1);
            let feasible =
                feasible_piece_mask_for(&streaming.shared.admissible_prefixes, seed.used_counts);
            let (status, domain) =
                DomainPropagation::compile_minimum(catalog, seed.remaining, feasible);
            if status == DomainStatus::Empty {
                domain_pruned_states = domain_pruned_states.saturating_add(1);
                seeds[index].splittable = false;
                continue;
            }
            let rows = catalog.support(domain.pivot_cell);
            let allowed = |row_id| domain.row_allowed(catalog, row_id, seed.remaining, feasible);
            let count = rows
                .iter()
                .copied()
                .filter(|&row_id| allowed(row_id))
                .count();
            if count == 0 {
                seeds[index].splittable = false;
                continue;
            }
            seeds
                .try_reserve_exact(count - 1)
                .map_err(|_| streaming_storage_error())?;
            for (offset, row_id) in rows
                .iter()
                .copied()
                .filter(|&row_id| allowed(row_id))
                .enumerate()
            {
                let child = seed.child(catalog, row_id);
                if offset == 0 {
                    seeds[index] = child;
                } else {
                    seeds.push(child);
                }
            }
        }
        if seeds.len() < 2 {
            return Ok(ParallelGeometryPreparation::Serial {
                geometry: self,
                reason: "small-streaming-root-domain",
            });
        }
        seeds.sort_unstable_by(|left, right| {
            left.rows[..usize::from(left.depth)].cmp(&right.rows[..usize::from(right.depth)])
        });
        let balanced_workers = requested_workers.min(seeds.len());
        let mut searches = Vec::new();
        searches
            .try_reserve_exact(seeds.len())
            .map_err(|_| streaming_storage_error())?;
        for (index, seed) in seeds.into_iter().enumerate() {
            let mut stack = Vec::new();
            stack
                .try_reserve_exact(usize::from(streaming.shared.target_depth - seed.depth) + 1)
                .map_err(|_| streaming_storage_error())?;
            // The chosen prefix belongs to this partition, so its root has
            // NO_ROW and popping it never subtracts the frozen prefix counts.
            stack.push(CompileFrame::child(seed.remaining, seed.depth, NO_ROW, 0));
            let mut search = Self::placeholder();
            search.streaming_probability_requested = true;
            search.resource_authoritative = self.resource_authoritative;
            search.candidate_family_count = None;
            search.expanded_nodes = usize::from(index == 0) * expanded_nodes;
            search.peak_frontier = 1;
            search.domain_pruned_states = usize::from(index == 0) * domain_pruned_states;
            search.streaming = Some(StreamingGeometry {
                shared: Arc::clone(&streaming.shared),
                stack,
                rows: seed.rows,
                used_counts: seed.used_counts,
                accounts_shared: false,
                expanded_nodes: search.expanded_nodes,
                peak_frontier: 1,
                domain_pruned_states: search.domain_pruned_states,
                emitted: 0,
            });
            searches.push(search);
        }
        Ok(ParallelGeometryPreparation::Ready {
            plan: ParallelGeometryPlan {
                targets: Arc::clone(&streaming.shared.targets),
                searches,
                group_pattern_index_bytes: self.group_pattern_index_bytes,
                shared_family_bytes: streaming.shared.retained_bytes(),
            },
            balanced_workers,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use clearra_pc_graph::request::{
        PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
        SupplyWindowSize,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_rules::profile::builtin_rules::srs_plus;

    use super::*;

    type CandidateSet = BTreeSet<(u32, StandardBoard64TilingIdentity)>;

    fn fixture(
        initial: u64,
    ) -> (
        GeometryCatalog,
        MaterializedPatternUniverse,
        PackingMultisetFamily,
    ) {
        let locks = (40 - initial.count_ones()) as usize / 4;
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, initial),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(locks),
        )
        .with_rule(srs_plus())
        .with_allow_hold(true)
        .with_exact_pieces(Some(locks))
        .with_supply_window_size(SupplyWindowSize::new(7))
        .with_count_policy(PcCountPolicy::CountUnique);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("small P7 geometry");
        let catalog = GeometryCatalog::compile(&problem).expect("four-line catalog");
        let universe = problem
            .piece_source()
            .materialized_universe()
            .unwrap()
            .clone();
        let family = universe.packing_multiset_family_for_execution(
            locks,
            problem.initial_hold(),
            true,
            crate::backend::wasm_cpu::packing_hold_projection(&problem),
        );
        (catalog, universe, family)
    }

    fn collect(
        search: &mut GeometrySearch,
        catalog: &GeometryCatalog,
        limit: Option<u128>,
    ) -> CandidateSet {
        let mut candidates = CandidateSet::new();
        let streaming = search.streaming_probability_requested;
        let retained = search.retained_bytes();
        for _ in 0..10_000_000 {
            if streaming {
                assert_eq!(
                    search.candidate_family_count(),
                    None,
                    "no full-family count before exhaustion"
                );
                assert_eq!(
                    search.retained_bytes(),
                    retained,
                    "fixed traversal storage never grows"
                );
            }
            let advance = if let Some(limit) = limit {
                search.advance_with_retained_limit(catalog, limit)
            } else {
                search.advance(catalog)
            };
            match advance {
                GeometryAdvance::Pending => {}
                GeometryAdvance::Candidate(candidate) => {
                    assert!(search.target(candidate.target_index).is_some());
                    assert!(
                        candidates.insert((candidate.target_index, candidate.identity)),
                        "duplicate exact cover"
                    );
                }
                GeometryAdvance::Complete => {
                    if streaming {
                        assert_eq!(
                            search.candidate_family_count(),
                            Some(candidates.len() as u128)
                        );
                    }
                    return candidates;
                }
                GeometryAdvance::ResourceIncomplete(reason) => {
                    panic!("unexpected geometry truncation: {reason}")
                }
            }
        }
        panic!("small fixture must exhaust within finite test work");
    }

    #[test]
    fn streaming_probability_matches_compiled_p7_and_joint_clear_ilc_families() {
        for initial in [0x3c0f_03c0f, 0x90_747d_83e5] {
            let (catalog, universe, family) = fixture(initial);
            assert!(
                (0..catalog.skeleton_count() as u32).any(|row| {
                    catalog
                        .realizations(row)
                        .iter()
                        .any(|realization| realization.required_deleted_rows != 0)
                }),
                "fixture exercises row-lift catalog entries"
            );
            let mut compiled =
                GeometrySearch::new(&universe, &family, catalog.required_cells(), false).unwrap();
            let expected = collect(&mut compiled, &catalog, None);
            assert!(!expected.is_empty());
            let mut streaming =
                GeometrySearch::new(&universe, &family, catalog.required_cells(), false).unwrap();
            let prefix_pointer = streaming
                .compiler
                .as_ref()
                .unwrap()
                .admissible_prefixes
                .as_ptr();
            let target_pointer = streaming.compiler.as_ref().unwrap().targets.as_ptr();
            streaming.enable_streaming_probability().unwrap();
            let owner = streaming.streaming.as_ref().unwrap();
            assert_eq!(owner.shared.admissible_prefixes.as_ptr(), prefix_pointer);
            assert_eq!(owner.shared.targets.as_ptr(), target_pointer);
            assert!(streaming.compiler.is_none() && streaming.enumerator.is_none());
            assert_eq!(collect(&mut streaming, &catalog, None), expected);
        }
    }

    #[test]
    fn streaming_probability_exact_retained_cap_and_one_byte_short() {
        let (catalog, universe, family) = fixture(0x90_747d_83e5);
        let mut compiled =
            GeometrySearch::new(&universe, &family, catalog.required_cells(), false).unwrap();
        let expected = collect(&mut compiled, &catalog, None);
        let mut streaming = GeometrySearch::new_with_memory_limit(
            &universe,
            &family,
            catalog.required_cells(),
            false,
            0,
            u128::MAX,
        )
        .unwrap();
        let fresh_bytes = streaming.retained_bytes();
        streaming.enable_streaming_probability().unwrap();
        let cap = streaming.retained_bytes() as u128;
        assert!(
            cap < fresh_bytes as u128,
            "fresh family scratch was retired"
        );
        assert!(matches!(
            streaming.advance_with_retained_limit(&catalog, cap - 1),
            GeometryAdvance::ResourceIncomplete("geometry_memory_capacity_exceeded")
        ));
        assert_eq!(
            streaming.expanded_nodes(),
            0,
            "rejected admission must not consume a branch"
        );
        assert_eq!(streaming.candidate_family_count(), None);
        assert_eq!(collect(&mut streaming, &catalog, Some(cap)), expected);
    }

    #[test]
    fn streaming_probability_deferred_inputs_activate_without_compiling_family() {
        let (catalog, universe, family) = fixture(0x90_747d_83e5);
        let mut deferred =
            GeometrySearch::new(&universe, &family, catalog.required_cells(), true).unwrap();
        deferred.enable_streaming_probability().unwrap();
        assert!(deferred.target_preparation_pending());
        for _ in 0..10_000 {
            if !deferred.target_preparation_pending() {
                break;
            }
            assert!(matches!(
                deferred.advance(&catalog),
                GeometryAdvance::Pending
            ));
        }
        assert!(!deferred.target_preparation_pending());
        assert_eq!(deferred.expanded_nodes(), 0);
        assert!(deferred.streaming.is_some() && deferred.compiler.is_none());
        assert!(deferred
            .targets()
            .unwrap()
            .iter()
            .all(|target| target.pattern_index.is_some()));
        let mut direct =
            GeometrySearch::new(&universe, &family, catalog.required_cells(), false).unwrap();
        assert_eq!(
            collect(&mut deferred, &catalog, None),
            collect(&mut direct, &catalog, None)
        );
    }

    #[test]
    fn streaming_probability_rejects_started_and_external_geometry() {
        let (catalog, universe, family) = fixture(0x90_747d_83e5);
        let mut started =
            GeometrySearch::new(&universe, &family, catalog.required_cells(), false).unwrap();
        assert!(matches!(
            started.advance(&catalog),
            GeometryAdvance::Pending
        ));
        assert!(started.enable_streaming_probability().is_err());
        assert!(
            started.compiler.is_some(),
            "rejecting a mode change preserves its owner"
        );
        let mut external =
            GeometrySearch::external_deferred(&universe, &family, true, None).unwrap();
        assert!(external.enable_streaming_probability().is_err());
    }

    #[cfg(feature = "parallel")]
    #[test]
    fn streaming_probability_parallel_shards_are_disjoint_exhaustive_and_share_inputs() {
        let (catalog, universe, family) = fixture(0x3c0f_03c0f);
        let mut compiled =
            GeometrySearch::new(&universe, &family, catalog.required_cells(), false).unwrap();
        let expected = collect(&mut compiled, &catalog, None);
        let mut root =
            GeometrySearch::new(&universe, &family, catalog.required_cells(), false).unwrap();
        root.enable_streaming_probability().unwrap();
        let ParallelGeometryPreparation::Ready {
            mut plan,
            balanced_workers,
        } = root
            .prepare_parallel(&catalog, &ExecutionControl::default(), 9, |_, _| {
                panic!("streaming must not invent a full-family count for worker balancing")
            })
            .unwrap()
        else {
            panic!("P7 fixture has parallel prefix partitions");
        };
        assert_eq!(balanced_workers, 9);
        assert!(plan.searches.len() >= 9);
        assert_eq!(
            plan.group_pattern_index_bytes as u128,
            checked_target_nested_retained_bytes(&plan.targets).unwrap()
        );
        let shared = Arc::clone(&plan.searches[0].streaming.as_ref().unwrap().shared);
        assert_eq!(plan.shared_family_bytes, shared.retained_bytes());
        assert_eq!(plan.targets.as_ptr(), shared.targets.as_ptr());
        assert_eq!(
            GeometrySearch::from_parallel_searches(
                &plan.searches,
                plan.group_pattern_index_bytes,
                plan.shared_family_bytes,
            )
            .candidate_family_count(),
            None
        );
        let mut merged = CandidateSet::new();
        for search in &mut plan.searches {
            let streaming = search.streaming.as_ref().unwrap();
            assert!(Arc::ptr_eq(&shared, &streaming.shared));
            assert_eq!(streaming.checked_target_nested_retained_bytes(), Some(0));
            assert_eq!(
                search.retained_bytes(),
                streaming.stack.capacity() * core::mem::size_of::<CompileFrame>()
            );
            for candidate in collect(search, &catalog, None) {
                assert!(
                    merged.insert(candidate),
                    "sibling prefix partitions overlap"
                );
            }
        }
        assert_eq!(merged, expected);
        assert_eq!(
            GeometrySearch::from_parallel_searches(
                &plan.searches,
                plan.group_pattern_index_bytes,
                plan.shared_family_bytes,
            )
            .candidate_family_count(),
            Some(expected.len() as u128)
        );
    }
}
