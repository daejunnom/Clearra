//! Native predictor of the portable core's idle-assistance contract. Both A/B
//! arms use this same ready-worker scheduler; only optional assistance differs.
//! This diagnostic host is not evidence that the CLI owns this scheduler.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Instant,
};

use clearra_coverage::cover::exact_minimum_cover::{
    ExactMinimumCoverHotCostDiagnostics, ExactMinimumCoverPivotExhaustionDiagnostics,
};
use clearra_coverage::cover::{
    ExactAtMostQuery, ExactAtMostQueryIdentity, ExactAtMostReceipt, ExactAtMostShardAdvance,
    ExactAtMostShardSession, ExactAtMostTask, ExactMinimumCoverPortfolioEnumerator,
    ExactMinimumCoverPortfolioPreparationSession,
};

pub(super) enum Owner<'a> {
    Proof(&'a mut ExactMinimumCoverPortfolioPreparationSession),
    Canonical(&'a mut ExactMinimumCoverPortfolioEnumerator),
}

impl Owner<'_> {
    fn take(&mut self) -> Option<ExactAtMostTask> {
        match self {
            Self::Proof(owner) => owner.take_parallel_task(),
            Self::Canonical(owner) => owner.take_parallel_task(),
        }
    }

    fn assist(&mut self) -> bool {
        match self {
            Self::Proof(owner) => owner.prepare_parallel_idle_assist(64, &mut |_| Ok(())),
            Self::Canonical(owner) => owner.prepare_parallel_idle_assist(64, &mut |_| Ok(())),
        }
        .unwrap()
    }

    fn accept(&mut self, receipt: ExactAtMostReceipt) {
        match self {
            Self::Proof(owner) => owner.accept_parallel_receipt(receipt),
            Self::Canonical(owner) => owner.accept_parallel_receipt(receipt),
        }
        .unwrap();
    }

    fn redundant(&self, identity: ExactAtMostQueryIdentity, partition: u64) -> bool {
        match self {
            Self::Proof(owner) => owner.parallel_task_is_redundant(identity, partition),
            Self::Canonical(owner) => owner.parallel_task_is_redundant(identity, partition),
        }
        .unwrap()
    }
}

struct Finished {
    worker: usize,
    receipt: ExactAtMostReceipt,
    micros: u64,
    proposal_iterations: u64,
    exact_prunes: u64,
    cache_prunes: u64,
    hot_cost: Option<ExactMinimumCoverHotCostDiagnostics>,
    pivot_exhaustion: Option<ExactMinimumCoverPivotExhaustionDiagnostics>,
}

#[derive(Default)]
struct HotCostTotals {
    prepare: u128,
    mirror_prox: u128,
    softmax: u128,
    gradients: u128,
    log_update: u128,
    averaging: u128,
    certificate: u128,
    memo: u128,
    rarest: u128,
    top_gain: u128,
    root_certificate: u128,
    packing: u128,
    branch: u128,
}

impl HotCostTotals {
    fn add(&mut self, sample: ExactMinimumCoverHotCostDiagnostics) {
        self.prepare += sample.residual_prepare_nanoseconds;
        self.mirror_prox += sample.mirror_prox_nanoseconds;
        self.softmax += sample.softmax_p_nanoseconds
            + sample.softmax_q_nanoseconds
            + sample.softmax_middle_p_nanoseconds
            + sample.softmax_middle_q_nanoseconds;
        self.gradients += sample.first_gradient_nanoseconds + sample.middle_gradient_nanoseconds;
        self.log_update += sample.log_update_nanoseconds;
        self.averaging += sample.averaging_nanoseconds;
        self.certificate += sample.exact_recertification_nanoseconds;
        self.memo += sample.memo_nanoseconds;
        self.rarest += sample.rarest_support_nanoseconds;
        self.top_gain += sample.top_gain_nanoseconds;
        self.root_certificate += sample.root_certificate_nanoseconds;
        self.packing += sample.packing_nanoseconds;
        self.branch += sample.branch_nanoseconds;
    }
}

pub(super) fn run(
    query: ExactAtMostQuery,
    owner: Owner<'_>,
    workers: usize,
    assistance: bool,
) -> usize {
    let deadline = Instant::now() + super::configured_probe_timeout();
    run_until(query, owner, workers, assistance, deadline)
}

/// A caller-owned arm deadline must not restart for every parallel query.
/// This remains a fixture watchdog, not an executor or product timeout.
pub(super) fn run_until(
    query: ExactAtMostQuery,
    mut owner: Owner<'_>,
    workers: usize,
    assistance: bool,
    deadline: Instant,
) -> usize {
    assert!(Instant::now() < deadline, "parallel predictor arm deadline");
    let wave_started = Instant::now();
    thread::scope(|scope| {
        let (finished_sender, finished_receiver) = mpsc::channel();
        let mut workers_state = Vec::new();
        for worker in 0..workers {
            let (task_sender, task_receiver) = mpsc::channel::<ExactAtMostTask>();
            let query = query.clone();
            let finished_sender = finished_sender.clone();
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = stop.clone();
            scope.spawn(move || {
                while let Ok(task) = task_receiver.recv() {
                    let task_started = Instant::now();
                    let mut shard = ExactAtMostShardSession::prepare(
                        query.clone(),
                        task,
                        &mut |_| Ok(()),
                        &mut || Instant::now() >= deadline || worker_stop.load(Ordering::Relaxed),
                    )
                    .unwrap();
                    let mut last_residual = shard.diagnostic_residual_progress();
                    let mut last_hot_cost = shard.diagnostic_hot_cost();
                    let mut last_pivot_exhaustion = shard.diagnostic_cached_pivot_exhaustion();
                    loop {
                        if let Some(current) = shard.diagnostic_residual_progress() {
                            last_residual = Some(current);
                        }
                        if let Some(current) = shard.diagnostic_hot_cost() {
                            last_hot_cost = Some(current);
                        }
                        if let Some(current) = shard.diagnostic_cached_pivot_exhaustion() {
                            last_pivot_exhaustion = Some(current);
                        }
                        match shard
                            .advance(128, &mut |_| Ok(()), &mut || {
                                Instant::now() >= deadline || worker_stop.load(Ordering::Relaxed)
                            })
                            .unwrap()
                        {
                            ExactAtMostShardAdvance::Pending { .. } => {}
                            ExactAtMostShardAdvance::Terminal(receipt) => {
                                let (proposal_iterations, exact_prunes, cache_prunes) =
                                    last_residual.map_or((0, 0, 0), |residual| {
                                        let proposal_prunes: u64 =
                                            residual.certified_prunes_by_checkpoint.iter().sum();
                                        (
                                            residual.proposal_iterations,
                                            residual.certified_prunes,
                                            residual
                                                .certified_prunes
                                                .saturating_sub(proposal_prunes),
                                        )
                                    });
                                finished_sender
                                    .send(Finished {
                                        worker,
                                        receipt,
                                        micros: task_started
                                            .elapsed()
                                            .as_micros()
                                            .try_into()
                                            .unwrap(),
                                        proposal_iterations,
                                        exact_prunes,
                                        cache_prunes,
                                        hot_cost: last_hot_cost,
                                        pivot_exhaustion: last_pivot_exhaustion,
                                    })
                                    .unwrap();
                                break;
                            }
                        }
                    }
                }
            });
            workers_state.push((task_sender, stop, None::<u64>));
        }
        drop(finished_sender);
        let mut tasks = 0;
        let mut assisted_groups = 0;
        let mut selective_cancellations = 0;
        let mut work_micros = 0_u64;
        let mut maximum_task_micros = 0;
        let mut proposal_iterations = 0;
        let mut exact_prunes = 0;
        let mut cache_prunes = 0;
        let mut hot_cost = HotCostTotals::default();
        let mut pivot_samples = 0_usize;
        let mut pivot_unavailable = 0_usize;
        let mut pivot_totals = ExactMinimumCoverPivotExhaustionDiagnostics::default();
        loop {
            // Source issuance occurs only after a worker is ready. Closed-root
            // cancellation follows accepted exact receipts, never host guesses.
            for index in 0..workers_state.len() {
                if workers_state[index].2.is_some() {
                    continue;
                }
                let mut task = owner.take();
                if task.is_none()
                    && assistance
                    && workers_state.iter().any(|worker| worker.2.is_some())
                    && owner.assist()
                {
                    assisted_groups += 1;
                    task = owner.take();
                }
                if let Some(task) = task {
                    let (sender, stop, active) = &mut workers_state[index];
                    stop.store(false, Ordering::Relaxed);
                    *active = Some(task.partition_id());
                    sender.send(task).unwrap();
                    tasks += 1;
                }
            }
            if workers_state.iter().all(|worker| worker.2.is_none()) {
                break;
            }
            let done = finished_receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("parallel predictor exceeded its explicit fixture deadline");
            assert_eq!(
                workers_state[done.worker].2,
                Some(done.receipt.task().partition_id())
            );
            workers_state[done.worker].2 = None;
            owner.accept(done.receipt);
            work_micros += done.micros;
            maximum_task_micros = maximum_task_micros.max(done.micros);
            proposal_iterations += done.proposal_iterations;
            exact_prunes += done.exact_prunes;
            cache_prunes += done.cache_prunes;
            if let Some(sample) = done.hot_cost {
                hot_cost.add(sample);
            }
            if let Some(sample) = done.pivot_exhaustion {
                pivot_samples += 1;
                pivot_totals.assessments = pivot_totals
                    .assessments
                    .checked_add(sample.assessments)
                    .unwrap();
                pivot_totals.examined_rows = pivot_totals
                    .examined_rows
                    .checked_add(sample.examined_rows)
                    .unwrap();
                pivot_totals.pruned_nodes = pivot_totals
                    .pruned_nodes
                    .checked_add(sample.pruned_nodes)
                    .unwrap();
            } else {
                pivot_unavailable += 1;
            }
            for (_, stop, active) in &workers_state {
                if active.is_some_and(|partition| owner.redundant(query.identity(), partition))
                    && !stop.swap(true, Ordering::Relaxed)
                {
                    selective_cancellations += 1;
                }
            }
        }
        // Dropping every sender wakes worker receivers, then scoped joining
        // proves complete physical drain before the next query is published.
        drop(workers_state);
        assert!(tasks > 0, "pending oracle must have an exact frontier");
        // None is explicitly unavailable, never converted into a zero sample.
        // These totals omit terminal-advance work and cannot establish that a
        // zero sampled prune count means no pruning occurred in the wave.
        eprintln!(
            "{}",
            serde_json::json!({
                "phase": "parallel_wave_cached_pivot_exhaustion",
                "query": query.identity().query_id,
                "limit": query.limit(),
                "sampled_shards": pivot_samples,
                "unavailable_shards": pivot_unavailable,
                "terminal_advance_included": false,
                "sampled_assessments": (pivot_samples != 0).then_some(pivot_totals.assessments),
                "sampled_examined_rows": (pivot_samples != 0).then_some(pivot_totals.examined_rows),
                "sampled_pruned_nodes": (pivot_samples != 0).then_some(pivot_totals.pruned_nodes),
            })
        );
        eprintln!(
            "{{\"phase\":\"parallel_wave_load\",\"tasks\":{tasks},\"workers\":{workers},\"assistance\":{assistance},\"assisted_groups\":{assisted_groups},\"selective_cancellations\":{selective_cancellations},\"elapsed_ms\":{},\"summed_task_ms\":{},\"maximum_task_ms\":{},\"sampled_proposal_iterations\":{proposal_iterations},\"sampled_exact_prunes\":{exact_prunes},\"sampled_cache_prunes\":{cache_prunes}}}",
            wave_started.elapsed().as_millis(),
            work_micros / 1_000,
            maximum_task_micros / 1_000,
        );
        // Samples end before terminal cursor ownership is released. These are
        // summed worker wall-cost samples, not elapsed wave time; MP component
        // fields overlap its inclusive total and must not be added to it.
        eprintln!(
            "{{\"phase\":\"parallel_wave_hot_cost\",\"query\":{},\"limit\":{},\"rows\":{},\"patterns\":{},\"sampled_prepare_ms\":{},\"sampled_mirror_prox_ms\":{},\"sampled_softmax_ms\":{},\"sampled_gradient_ms\":{},\"sampled_log_update_ms\":{},\"sampled_averaging_ms\":{},\"sampled_certificate_ms\":{},\"sampled_memo_ms\":{},\"sampled_rarest_ms\":{},\"sampled_top_gain_ms\":{},\"sampled_root_certificate_ms\":{},\"sampled_packing_ms\":{},\"sampled_branch_ms\":{}}}",
            query.identity().query_id,
            query.limit(),
            query.rows().len(),
            query.required().pattern_count(),
            hot_cost.prepare as f64 / 1e6,
            hot_cost.mirror_prox as f64 / 1e6,
            hot_cost.softmax as f64 / 1e6,
            hot_cost.gradients as f64 / 1e6,
            hot_cost.log_update as f64 / 1e6,
            hot_cost.averaging as f64 / 1e6,
            hot_cost.certificate as f64 / 1e6,
            hot_cost.memo as f64 / 1e6,
            hot_cost.rarest as f64 / 1e6,
            hot_cost.top_gain as f64 / 1e6,
            hot_cost.root_certificate as f64 / 1e6,
            hot_cost.packing as f64 / 1e6,
            hot_cost.branch as f64 / 1e6,
        );
        tasks
    })
}
