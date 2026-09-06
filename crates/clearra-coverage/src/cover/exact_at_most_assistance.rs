//! One-level idle assistance. Original cursors are never restarted or cut:
//! complete child cubes race the unchanged original root proof. Registry and
//! proof closure stay in Rust; transport can only ask whether a task is stale
//! or redundant. This is not donation of a private DFS continuation.

use super::*;

const MAX_ASSIST_CHILDREN: usize = 64;
const MAX_ASSIST_TASKS: usize = 512;
const MAX_ASSIST_GROUPS: usize = 64;
// An advisory idle check must not become a second serial matrix pass. This is
// only a scheduling-work bound: a skipped pivot leaves the original proof live.
const MAX_ASSIST_PIVOT_ROW_CHECKS: usize = 16_384;

#[derive(Clone, Debug)]
pub(super) struct RootObligation {
    pub(super) closed: bool,
    pub(super) children: Option<core::ops::Range<usize>>,
}

#[derive(Clone, Debug)]
pub(super) struct AssistanceState {
    pub(super) roots: Vec<RootObligation>,
    pub(super) task_roots: Vec<usize>,
    // Unissued tasks can be retired without fabricating a worker receipt.
    pub(super) retired: Vec<bool>,
}

impl AssistanceState {
    pub(super) fn checked_retained_bytes(&self) -> Option<u128> {
        bytes::<RootObligation>(self.roots.capacity())
            .ok()?
            .checked_add(bytes::<usize>(self.task_roots.capacity()).ok()?)?
            .checked_add(bytes::<bool>(self.retired.capacity()).ok()?)
    }
}

impl ExactAtMostCoordinator {
    /// Stage a complete canonical fanout for one still-running original cube.
    /// The caller must have exhausted the ordinary unissued frontier and own
    /// an idle, admitted worker. No grandchildren are minted. The callback is
    /// EXTRA live heap beyond this coordinator's unchanged retained baseline,
    /// including old/new registry coexistence. Any optional capacity/allocation
    /// decline leaves every descriptor, ID and proof obligation unchanged.
    pub fn prepare_idle_assist(
        &mut self,
        issued_prefix: usize,
        maximum_children: usize,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<bool, ExactAtMostParallelError> {
        let maximum_children = maximum_children.min(MAX_ASSIST_CHILDREN);
        if maximum_children < 2
            || issued_prefix != self.tasks.len()
            || !matches!(self.decision, ExactAtMostParallelDecision::Pending { .. })
        {
            return Ok(false);
        }
        if cancelled() {
            return Err(ExactAtMostParallelError::Cancelled);
        }
        match self.stage_idle_assist(maximum_children, memory_guard, cancelled) {
            Ok(Some(staged)) => {
                *self = staged;
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(ExactAtMostParallelError::Exact(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Validate the complete query epoch, not only its reused numeric counter.
    /// Proof and canonical phases may both start query_id/generation at one.
    pub fn task_is_redundant(
        &self,
        identity: ExactAtMostQueryIdentity,
        partition_id: u64,
    ) -> Result<bool, ExactAtMostParallelError> {
        if identity != self.query.identity() {
            return Err(ExactAtMostParallelError::StaleQuery);
        }
        let index = usize::try_from(partition_id)
            .map_err(|_| ExactAtMostParallelError::UnknownPartition)?;
        if index >= self.tasks.len() {
            return Err(ExactAtMostParallelError::UnknownPartition);
        }
        Ok(
            matches!(self.decision, ExactAtMostParallelDecision::Found(_))
                || self
                    .assistance
                    .as_ref()
                    .is_some_and(|state| state.roots[state.task_roots[index]].closed),
        )
    }

    pub(in crate::cover) fn retire_redundant_unissued(&mut self, index: usize) -> bool {
        let Some(state) = &mut self.assistance else {
            return false;
        };
        if index >= self.tasks.len()
            || self.received[index]
            || !state.roots[state.task_roots[index]].closed
        {
            return false;
        }
        state.retired[index] = true;
        self.received[index] = true;
        true
    }

    fn stage_idle_assist(
        &self,
        maximum_children: usize,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<Option<Self>, ExactAtMostParallelError> {
        let root_count = self
            .assistance
            .as_ref()
            .map_or(self.tasks.len(), |state| state.roots.len());
        let added = self
            .tasks
            .len()
            .checked_sub(root_count)
            .ok_or_else(overflow)?;
        let groups = self.assistance.as_ref().map_or(0, |state| {
            state
                .roots
                .iter()
                .filter(|root| root.children.is_some())
                .count()
        });
        if groups >= MAX_ASSIST_GROUPS || added >= MAX_ASSIST_TASKS {
            return Ok(None);
        }
        let mut selected_pivot = None;
        let mut checked_rows = 0;
        // Retain the bounded fanout while counting; reconstructing it with a
        // second matrix pass would double the advertised membership-work cap.
        let mut pivot_supporters = [0_usize; MAX_ASSIST_CHILDREN];
        'roots: for root_index in 0..root_count {
            if cancelled() {
                return Err(ExactAtMostParallelError::Cancelled);
            }
            if self.received[root_index]
                || self.assistance.as_ref().is_some_and(|state| {
                    state.roots[root_index].closed || state.roots[root_index].children.is_some()
                })
            {
                continue;
            }
            let parent = &self.tasks[root_index];
            if parent.forced_rows.len() >= self.query.limit() {
                continue;
            }
            // Any uncovered constraint yields an exhaustive first-supporter
            // partition. Rarest selection is not part of its proof, so stop at
            // the first bounded fanout instead of rescanning the whole matrix.
            for pattern in self
                .query
                .required()
                .covered_patterns_before(self.query.required().pattern_count())
            {
                if cancelled() {
                    return Err(ExactAtMostParallelError::Cancelled);
                }
                let mut covered = false;
                for &row in &parent.forced_rows {
                    if checked_rows == MAX_ASSIST_PIVOT_ROW_CHECKS {
                        break 'roots;
                    }
                    checked_rows += 1;
                    if self.query.rows()[row].contains(pattern) {
                        covered = true;
                        break;
                    }
                }
                if covered {
                    continue;
                }
                let mut count = 0;
                for (row, bits) in self.query.rows().iter().enumerate() {
                    if checked_rows == MAX_ASSIST_PIVOT_ROW_CHECKS {
                        break 'roots;
                    }
                    checked_rows += 1;
                    if bits.contains(pattern) && parent.excluded_rows.binary_search(&row).is_err() {
                        if count < MAX_ASSIST_CHILDREN {
                            pivot_supporters[count] = row;
                        }
                        count += 1;
                        if count > maximum_children || count > MAX_ASSIST_TASKS - added {
                            break;
                        }
                    }
                }
                if count >= 2 && count <= maximum_children && count <= MAX_ASSIST_TASKS - added {
                    selected_pivot = Some((root_index, count));
                    break 'roots;
                }
                // Unit supporters are left to the unchanged child solver.
                // Another constraint can still give a useful exact fanout.
            }
        }
        #[cfg(all(feature = "diagnostic-probes", not(target_arch = "wasm32")))]
        eprintln!(
            "{{\"phase\":\"idle_assist_pivot\",\"checked_rows\":{checked_rows},\"selected\":{},\"roots\":{root_count}}}",
            selected_pivot.is_some()
        );
        let Some((root_index, child_count)) = selected_pivot else {
            return Ok(None);
        };
        let task_count = self
            .tasks
            .len()
            .checked_add(child_count)
            .ok_or_else(overflow)?;
        u64::try_from(task_count).map_err(|_| overflow())?;
        let parent = &self.tasks[root_index];
        let mut live = 0;
        let mut supporters: Vec<usize> = staged_vec(child_count, &mut live, guard)?;
        supporters.extend_from_slice(&pivot_supporters[..child_count]);
        let mut tasks = staged_vec(task_count, &mut live, guard)?;
        for task in &self.tasks {
            if cancelled() {
                return Err(ExactAtMostParallelError::Cancelled);
            }
            let mut forced_rows = staged_vec(task.forced_rows.len(), &mut live, guard)?;
            forced_rows.extend_from_slice(&task.forced_rows);
            let mut excluded_rows = staged_vec(task.excluded_rows.len(), &mut live, guard)?;
            excluded_rows.extend_from_slice(&task.excluded_rows);
            tasks.push(ExactAtMostTask {
                identity: task.identity,
                partition_id: task.partition_id,
                forced_rows,
                excluded_rows,
            });
        }
        for (index, &selected) in supporters.iter().enumerate() {
            let mut forced_rows = staged_vec(
                parent
                    .forced_rows
                    .len()
                    .checked_add(1)
                    .ok_or_else(overflow)?,
                &mut live,
                guard,
            )?;
            forced_rows.extend_from_slice(&parent.forced_rows);
            forced_rows.push(selected);
            forced_rows.sort_unstable();
            let mut excluded_rows = staged_vec(
                parent
                    .excluded_rows
                    .len()
                    .checked_add(index)
                    .ok_or_else(overflow)?,
                &mut live,
                guard,
            )?;
            excluded_rows.extend_from_slice(&parent.excluded_rows);
            excluded_rows.extend_from_slice(&supporters[..index]);
            excluded_rows.sort_unstable();
            tasks.push(ExactAtMostTask::from_parts(
                self.query.identity(),
                u64::try_from(tasks.len()).map_err(|_| overflow())?,
                forced_rows,
                excluded_rows,
            )?);
        }
        let mut received = staged_vec(task_count, &mut live, guard)?;
        received.extend_from_slice(&self.received);
        received.resize(task_count, false);
        let mut roots = staged_vec(root_count, &mut live, guard)?;
        let mut task_roots = staged_vec(task_count, &mut live, guard)?;
        let mut retired = staged_vec(task_count, &mut live, guard)?;
        if let Some(state) = &self.assistance {
            roots.extend_from_slice(&state.roots);
            task_roots.extend_from_slice(&state.task_roots);
            retired.extend_from_slice(&state.retired);
        } else {
            roots.extend(self.received.iter().map(|&closed| RootObligation {
                closed,
                children: None,
            }));
            task_roots.extend(0..root_count);
            retired.resize(root_count, false);
        }
        roots[root_index].children = Some(self.tasks.len()..task_count);
        task_roots.resize(task_count, root_index);
        retired.resize(task_count, false);
        guard(live)?;
        if cancelled() {
            return Err(ExactAtMostParallelError::Cancelled);
        }
        Ok(Some(Self {
            query: self.query.clone(),
            tasks,
            received,
            decision: self.decision.clone(),
            assistance: Some(AssistanceState {
                roots,
                task_roots,
                retired,
            }),
        }))
    }
}

fn staged_vec<T>(
    count: usize,
    live: &mut u128,
    guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<Vec<T>, ExactAtMostParallelError> {
    guard(live.checked_add(bytes::<T>(count)?).ok_or_else(overflow)?)?;
    let vector = reserve(count)?;
    *live = live
        .checked_add(bytes::<T>(vector.capacity())?)
        .ok_or_else(overflow)?;
    guard(*live)?;
    Ok(vector)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(masks: &[u64], limit: usize) -> ExactAtMostQuery {
        ExactAtMostQuery::new(
            ExactAtMostQueryIdentity {
                matrix_id: [42; 32],
                generation: 1,
                query_id: 1,
            },
            PatternBitSet::all(3),
            masks
                .iter()
                .map(|&mask| PatternBitSet::from_words(3, vec![mask]).unwrap())
                .collect(),
            limit,
        )
        .unwrap()
    }

    fn coordinator(query: &ExactAtMostQuery) -> ExactAtMostCoordinator {
        ExactAtMostCoordinator::prepare(query.clone(), 1, &mut |_| Ok(()), &mut || false).unwrap()
    }

    fn run(query: &ExactAtMostQuery, task: &ExactAtMostTask) -> ExactAtMostReceipt {
        let mut worker = ExactAtMostShardSession::prepare(
            query.clone(),
            task.clone(),
            &mut |_| Ok(()),
            &mut || false,
        )
        .unwrap();
        for _ in 0..1000 {
            if let ExactAtMostShardAdvance::Terminal(receipt) =
                worker.advance(16, &mut |_| Ok(()), &mut || false).unwrap()
            {
                return receipt;
            }
        }
        panic!("tiny assistance fixture exceeded its test-only bound");
    }

    #[test]
    fn all_children_negative_closes_root_but_does_not_forge_transport_drain() {
        let query = query(&[3, 5, 6], 1);
        let mut core = coordinator(&query);
        let parent = core.tasks[0].clone();
        assert!(
            core.prepare_idle_assist(1, 64, &mut |_| Ok(()), &mut || false)
                .unwrap()
        );
        let children = core.tasks[1..].to_vec();
        assert!(
            !core
                .prepare_idle_assist(core.tasks.len(), 64, &mut |_| Ok(()), &mut || false)
                .unwrap(),
            "one level, one split per root"
        );
        for child in &children {
            core.accept(run(&query, child)).unwrap();
            if child != children.last().unwrap() {
                assert!(matches!(
                    core.decision(),
                    ExactAtMostParallelDecision::Pending { remaining: 1 }
                ));
            }
        }
        assert_eq!(core.decision(), &ExactAtMostParallelDecision::ProvedNone);
        assert!(
            !core.issued_prefix_complete(core.tasks.len()),
            "running original still owes a receipt"
        );
        assert!(
            core.task_is_redundant(query.identity(), parent.partition_id())
                .unwrap()
        );
        let cancelled =
            ExactAtMostReceipt::from_parts(parent, ExactAtMostShardOutcome::Cancelled).unwrap();
        assert_eq!(
            core.accept(cancelled.clone()).unwrap(),
            ExactAtMostParallelDecision::ProvedNone
        );
        assert!(core.issued_prefix_complete(core.tasks.len()));
        assert_eq!(
            core.accept(cancelled),
            Err(ExactAtMostParallelError::DuplicateReceipt)
        );
    }

    #[test]
    fn parent_negative_retires_unissued_children_and_accepts_first_late_issued_receipt() {
        let query = query(&[3, 5, 6], 1);
        let mut core = coordinator(&query);
        let parent = core.tasks[0].clone();
        assert!(
            core.prepare_idle_assist(1, 64, &mut |_| Ok(()), &mut || false)
                .unwrap()
        );
        let child = core.tasks[1].clone();
        core.accept(run(&query, &parent)).unwrap();
        assert_eq!(core.decision(), &ExactAtMostParallelDecision::ProvedNone);
        let late = run(&query, &child);
        core.accept(late.clone()).unwrap();
        assert_eq!(
            core.accept(late),
            Err(ExactAtMostParallelError::DuplicateReceipt)
        );
        for index in 2..core.tasks.len() {
            assert!(core.retire_redundant_unissued(index));
        }
        assert!(core.issued_prefix_complete(core.tasks.len()));
        let retired = core.tasks[2].clone();
        assert_eq!(
            core.accept(run(&query, &retired)),
            Err(ExactAtMostParallelError::UnknownPartition)
        );
    }

    #[test]
    fn active_child_cancel_stale_identity_and_contradictory_positive_fail_closed() {
        let query = query(&[3, 5, 6], 2);
        let mut core = coordinator(&query);
        core.prepare_idle_assist(1, 64, &mut |_| Ok(()), &mut || false)
            .unwrap();
        let mut stale = query.identity();
        stale.matrix_id[0] ^= 1;
        assert_eq!(
            core.task_is_redundant(stale, 1),
            Err(ExactAtMostParallelError::StaleQuery)
        );
        let cancelled = ExactAtMostReceipt::from_parts(
            core.tasks[1].clone(),
            ExactAtMostShardOutcome::Cancelled,
        )
        .unwrap();
        let mut cancelled_core = core.clone();
        assert_eq!(
            cancelled_core.accept(cancelled).unwrap(),
            ExactAtMostParallelDecision::Cancelled
        );
        // Negative worker claims use the existing trusted channel. If a later
        // replay-valid witness contradicts one, never silently accept a false
        // completed negative result, even when it is only a late helper.
        core.accept(
            ExactAtMostReceipt::from_parts(
                core.tasks[0].clone(),
                ExactAtMostShardOutcome::ProvedNone,
            )
            .unwrap(),
        )
        .unwrap();
        let positive = ExactAtMostReceipt::from_parts(
            core.tasks[1].clone(),
            ExactAtMostShardOutcome::Found(vec![0, 1]),
        )
        .unwrap();
        assert_eq!(
            core.accept(positive),
            Err(ExactAtMostParallelError::ContradictoryReceipt)
        );
    }

    #[test]
    fn optional_assist_admission_is_atomic_and_never_truncates_pivot_fanout() {
        let query = query(&[3, 5, 6], 1);
        let source = coordinator(&query);
        let original_tasks = source.tasks.clone();
        let original_bytes = source.checked_retained_bytes().unwrap();
        let mut observed = source.clone();
        let mut peak = 0;
        assert!(
            observed
                .prepare_idle_assist(
                    1,
                    64,
                    &mut |bytes| {
                        peak = peak.max(bytes);
                        Ok(())
                    },
                    &mut || false
                )
                .unwrap()
        );
        assert!(observed.checked_retained_bytes().unwrap() <= original_bytes + peak);
        let mut tight = source.clone();
        assert!(
            !tight
                .prepare_idle_assist(
                    1,
                    64,
                    &mut |required_memory_bytes| {
                        if required_memory_bytes >= peak {
                            Err(ExactMinimumCoverError::MemoryCapacityExceeded {
                                required_memory_bytes,
                                max_memory_bytes: peak - 1,
                            })
                        } else {
                            Ok(())
                        }
                    },
                    &mut || false
                )
                .unwrap()
        );
        assert_eq!(tight.tasks, original_tasks);
        assert_eq!(tight.checked_retained_bytes(), Some(original_bytes));
        assert!(tight.assistance.is_none());
        assert!(
            !tight
                .prepare_idle_assist(0, 64, &mut |_| Ok(()), &mut || false)
                .unwrap(),
            "ordinary unissued task has priority"
        );
        assert!(
            !tight
                .prepare_idle_assist(1, 1, &mut |_| Ok(()), &mut || false)
                .unwrap()
        );
        assert_eq!(
            tight.prepare_idle_assist(1, 64, &mut |_| Ok(()), &mut || true),
            Err(ExactAtMostParallelError::Cancelled)
        );
        assert_eq!(tight.tasks, original_tasks);

        let allocation_started = std::cell::Cell::new(false);
        assert_eq!(
            tight.prepare_idle_assist(
                1,
                64,
                &mut |_| {
                    allocation_started.set(true);
                    Ok(())
                },
                &mut || allocation_started.get()
            ),
            Err(ExactAtMostParallelError::Cancelled)
        );
        assert_eq!(tight.tasks, original_tasks);
        assert_eq!(tight.checked_retained_bytes(), Some(original_bytes));
        assert!(tight.assistance.is_none());
    }

    #[test]
    fn assisted_root_closure_does_not_close_other_original_obligations() {
        let masks = [3, 5, 9, 17, 6, 10, 18, 12, 20, 24];
        let query = ExactAtMostQuery::new(
            query(&[3, 5, 6], 1).identity(),
            PatternBitSet::all(5),
            masks
                .iter()
                .map(|&mask| PatternBitSet::from_words(5, vec![mask]).unwrap())
                .collect(),
            2,
        )
        .unwrap();
        let mut core =
            ExactAtMostCoordinator::prepare(query.clone(), 2, &mut |_| Ok(()), &mut || false)
                .unwrap();
        let roots = core.tasks.clone();
        assert!(roots.len() > 1);
        assert!(
            core.prepare_idle_assist(roots.len(), 64, &mut |_| Ok(()), &mut || false)
                .unwrap()
        );
        let children = core.tasks[roots.len()..].to_vec();
        for child in &children {
            core.accept(run(&query, child)).unwrap();
        }
        assert_eq!(
            core.decision(),
            &ExactAtMostParallelDecision::Pending {
                remaining: roots.len() - 1
            }
        );
        assert!(core.task_is_redundant(query.identity(), 0).unwrap());
        assert!(!core.task_is_redundant(query.identity(), 1).unwrap());
        core.accept(
            ExactAtMostReceipt::from_parts(roots[0].clone(), ExactAtMostShardOutcome::Cancelled)
                .unwrap(),
        )
        .unwrap();
        for root in &roots[1..] {
            core.accept(run(&query, root)).unwrap();
        }
        assert_eq!(core.decision(), &ExactAtMostParallelDecision::ProvedNone);
        assert!(core.issued_prefix_complete(core.tasks.len()));
    }

    #[test]
    fn an_oversized_fanout_is_skipped_even_if_caller_requests_more() {
        let query = ExactAtMostQuery::new(
            query(&[3, 5, 6], 1).identity(),
            PatternBitSet::all(1),
            vec![PatternBitSet::all(1); MAX_ASSIST_CHILDREN + 1],
            1,
        )
        .unwrap();
        let mut core = coordinator(&query);
        assert!(
            !core
                .prepare_idle_assist(1, usize::MAX, &mut |_| Ok(()), &mut || false)
                .unwrap()
        );
        assert_eq!(core.tasks.len(), 1);
        assert!(core.assistance.is_none());
    }

    #[test]
    fn a_unit_constraint_does_not_block_a_later_exact_assist_pivot() {
        let query = query(&[1, 2, 2, 4, 4], 3);
        let mut core = coordinator(&query);
        assert!(
            core.prepare_idle_assist(1, 64, &mut |_| Ok(()), &mut || false)
                .unwrap()
        );
        assert_eq!(core.tasks[1].forced_rows, [1]);
        assert_eq!(core.tasks[2].forced_rows, [2]);
        assert_eq!(core.tasks[2].excluded_rows, [1]);
        for child in core.tasks[1..].to_vec() {
            let receipt = run(&query, &child);
            assert!(matches!(
                receipt.outcome(),
                ExactAtMostShardOutcome::Found(_)
            ));
            core.accept(receipt).unwrap();
        }
    }

    #[test]
    fn a_bounded_pivot_probe_can_decline_without_touching_root_authority() {
        let query = ExactAtMostQuery::new(
            query(&[3, 5, 6], 1).identity(),
            PatternBitSet::all(1),
            vec![PatternBitSet::from_words(1, vec![0]).unwrap(); MAX_ASSIST_PIVOT_ROW_CHECKS + 1],
            1,
        )
        .unwrap();
        let mut core = coordinator(&query);
        let before = core.checked_retained_bytes();
        assert!(
            !core
                .prepare_idle_assist(1, 64, &mut |_| Ok(()), &mut || false)
                .unwrap()
        );
        assert_eq!(core.tasks.len(), 1);
        assert_eq!(core.checked_retained_bytes(), before);
        assert_eq!(
            core.decision(),
            &ExactAtMostParallelDecision::Pending { remaining: 1 }
        );
    }

    #[test]
    fn exact_child_cubes_partition_every_parent_cover_and_match_serial_small_matrices() {
        let mut assisted_cases = 0;
        for encoded in 0_u64..512 {
            let masks = [encoded & 7, (encoded >> 3) & 7, (encoded >> 6) & 7];
            for limit in 1..=3 {
                let query = query(&masks, limit);
                let mut core = coordinator(&query);
                let parent = core.tasks[0].clone();
                if !core
                    .prepare_idle_assist(1, 64, &mut |_| Ok(()), &mut || false)
                    .unwrap()
                {
                    continue;
                }
                assisted_cases += 1;
                let children = core.tasks[1..].to_vec();
                for selection in 0_usize..8 {
                    if selection.count_ones() as usize > limit {
                        continue;
                    }
                    let covered = (0..3)
                        .filter(|row| selection & (1 << row) != 0)
                        .fold(0, |bits, row| bits | masks[row]);
                    if covered != 7 {
                        continue;
                    }
                    let owners = children
                        .iter()
                        .filter(|child| {
                            child
                                .forced_rows
                                .iter()
                                .all(|row| selection & (1 << row) != 0)
                                && child
                                    .excluded_rows
                                    .iter()
                                    .all(|row| selection & (1 << row) == 0)
                        })
                        .count();
                    assert_eq!(
                        owners, 1,
                        "every original cover belongs to one complete child cube"
                    );
                }
                let serial = run(&query, &parent);
                for child in &children {
                    core.accept(run(&query, child)).unwrap();
                }
                match serial.outcome() {
                    ExactAtMostShardOutcome::Found(_) => assert!(matches!(
                        core.decision(),
                        ExactAtMostParallelDecision::Found(_)
                    )),
                    ExactAtMostShardOutcome::ProvedNone => {
                        assert_eq!(core.decision(), &ExactAtMostParallelDecision::ProvedNone)
                    }
                    ExactAtMostShardOutcome::Cancelled => panic!("uncancelled fixture"),
                }
            }
        }
        assert!(assisted_cases > 0);
    }
}
