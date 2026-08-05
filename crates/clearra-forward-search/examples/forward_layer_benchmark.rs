use std::{
    collections::BTreeMap,
    env,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use clearra_core_domain::{
    board::standard_pc_board::Board256Mask,
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
};
use clearra_forward_search::{
    ForwardParallelCoordinator, ForwardParallelProduce, ForwardParallelWorker, ForwardSearchMode,
    ForwardSearchOutcome, ForwardSearchQuery, ForwardSearchReport, ForwardSearchSession,
    ForwardSpinCategory, ForwardSpinTarget,
};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;

const DEFAULT_WORKERS: usize = 8;
const DEFAULT_BATCH_SIZE: usize = 256;

fn main() {
    let options = Options::parse(env::args().skip(1)).unwrap_or_else(|message| {
        eprintln!("{message}");
        std::process::exit(2);
    });
    let query = scenario(&options.scenario).unwrap_or_else(|message| {
        eprintln!("{message}");
        std::process::exit(2);
    });
    let result = match options.mode.as_str() {
        "serial" => run_serial(query),
        "parallel" => run_parallel(query, options.workers, options.batch_size),
        _ => Err("--mode must be serial or parallel".to_owned()),
    }
    .unwrap_or_else(|message| {
        eprintln!("{message}");
        std::process::exit(1);
    });
    println!("{}", result.to_json(&options));
}

struct Options {
    scenario: String,
    mode: String,
    workers: usize,
    batch_size: usize,
}

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut scenario = None;
        let mut mode = "parallel".to_owned();
        let mut workers = DEFAULT_WORKERS;
        let mut batch_size = DEFAULT_BATCH_SIZE;
        let mut args = args.peekable();
        while let Some(option) = args.next() {
            let value = args
                .next()
                .ok_or_else(|| format!("missing value for {option}"))?;
            match option.as_str() {
                "--scenario" => scenario = Some(value),
                "--mode" => mode = value,
                "--workers" => workers = positive(&value, "--workers")?,
                "--batch-size" => batch_size = positive(&value, "--batch-size")?,
                _ => return Err(format!("unsupported option {option}")),
            }
        }
        Ok(Self {
            scenario: scenario.ok_or_else(|| {
                "--scenario is required (spin-t-plus, spin-all-mini-plus, damage-oljtt, or damage-iotszl)"
                    .to_owned()
            })?,
            mode,
            workers,
            batch_size,
        })
    }
}

fn positive(value: &str, option: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{option} must be a positive integer"))
}

fn scenario(name: &str) -> Result<ForwardSearchQuery, String> {
    let spin = |profile| {
        ForwardSearchQuery::new(
            Board256Mask::from_words([0x280f8ffff8f, 0, 0, 0]),
            8,
            pieces("IOTSZ"),
            true,
            RuleProfileId::SrsPlus,
            profile,
            None,
            None,
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::new(Some(1), ForwardSpinCategory::T)),
        )
    };
    match name {
        "spin-t-plus" => Ok(spin(SpinProfileId::TSpinsPlus)),
        "spin-all-mini-plus" => Ok(spin(SpinProfileId::AllMiniPlus)),
        "damage-oljtt" => {
            let board = 0x38f_u64
                | (0x387_u64 << 10)
                | (0x303_u64 << 20)
                | (0x303_u64 << 30)
                | (0x300_u64 << 40);
            Ok(ForwardSearchQuery::new(
                Board256Mask::from_words([board, 0, 0, 0]),
                8,
                pieces("OLJTT"),
                false,
                RuleProfileId::SrsPlus,
                SpinProfileId::TSpins,
                None,
                None,
                ForwardSearchMode::MaximumDamage,
            ))
        }
        "damage-iotszl" => Ok(ForwardSearchQuery::new(
            Board256Mask::EMPTY,
            8,
            pieces("IOTSZL"),
            true,
            RuleProfileId::SrsPlus,
            SpinProfileId::AllMiniPlus,
            None,
            None,
            ForwardSearchMode::MaximumDamage,
        )),
        _ => Err(format!("unknown benchmark scenario {name}")),
    }
}

fn pieces(value: &str) -> Vec<PieceKind> {
    value
        .chars()
        .map(|piece| PieceKind::from_ascii(piece).expect("fixed benchmark queue"))
        .collect()
}

#[derive(Default)]
struct LayerMetrics {
    produce: Duration,
    worker_cpu: Duration,
    absorb: Duration,
    wait: Duration,
    wall_started: Option<Instant>,
    wall_finished: Option<Instant>,
    batches: u64,
    work_items: u64,
    input_bytes: u64,
    output_bytes: u64,
    visited_states: u64,
    generated_locks: u64,
    peak_frontier: usize,
}

impl LayerMetrics {
    fn mark_started(&mut self) {
        self.wall_started.get_or_insert_with(Instant::now);
    }

    fn mark_finished(&mut self) {
        self.wall_finished = Some(Instant::now());
    }

    fn wall(&self) -> Duration {
        self.wall_started
            .zip(self.wall_finished)
            .map_or(Duration::ZERO, |(start, finish)| {
                finish.duration_since(start)
            })
    }
}

struct BenchmarkResult {
    prepare: Duration,
    worker_initialization: Duration,
    search: Duration,
    finish: Duration,
    report: ForwardSearchReport,
    outcome_digest: u64,
    layers: BTreeMap<usize, LayerMetrics>,
}

fn run_serial(query: ForwardSearchQuery) -> Result<BenchmarkResult, String> {
    let control = ExecutionControl::new(ExecutionCancellationToken::new());
    let prepare_started = Instant::now();
    let session = ForwardSearchSession::new(query).map_err(|error| format!("{error:?}"))?;
    let prepare = prepare_started.elapsed();
    let search_started = Instant::now();
    let report = session
        .run_to_completion(&control)
        .map_err(|error| format!("{error:?}"))?;
    let search = search_started.elapsed();
    let outcome_digest = outcome_digest(&report);
    Ok(BenchmarkResult {
        prepare,
        worker_initialization: Duration::ZERO,
        search,
        finish: Duration::ZERO,
        report,
        outcome_digest,
        layers: BTreeMap::new(),
    })
}

enum WorkerRequest {
    Consume { layer: usize, bytes: Vec<u8> },
    Stop,
}

enum WorkerResponse {
    Ready {
        worker: usize,
        elapsed: Duration,
    },
    Consumed {
        worker: usize,
        layer: usize,
        items: usize,
        input_bytes: usize,
        output: Vec<u8>,
        elapsed: Duration,
    },
    Failed {
        worker: usize,
        reason: String,
    },
}

fn run_parallel(
    query: ForwardSearchQuery,
    workers: usize,
    batch_size: usize,
) -> Result<BenchmarkResult, String> {
    if workers < 2 {
        return Err("parallel benchmark requires at least two workers".to_owned());
    }
    if !ForwardParallelCoordinator::is_worthwhile(&query, workers) {
        return Err("scenario does not enter the product multi-worker path".to_owned());
    }
    let control = ExecutionControl::new(ExecutionCancellationToken::new());
    let prepare_started = Instant::now();
    let mut coordinator =
        ForwardParallelCoordinator::new(query, workers).map_err(|error| error.reason())?;
    let initialization = coordinator.worker_initialization();
    let prepare = prepare_started.elapsed();

    let worker_initialization_started = Instant::now();
    let (response_sender, response_receiver) = mpsc::channel();
    let mut request_senders = Vec::with_capacity(workers - 1);
    let mut handles = Vec::with_capacity(workers - 1);
    for worker in 0..workers - 1 {
        let (request_sender, request_receiver) = mpsc::sync_channel(1);
        let response_sender = response_sender.clone();
        let initialization = initialization.clone();
        let worker_control = control.clone();
        handles.push(thread::spawn(move || {
            worker_main(
                worker,
                initialization,
                request_receiver,
                response_sender,
                worker_control,
            );
        }));
        request_senders.push(request_sender);
    }
    drop(response_sender);
    let mut available = Vec::with_capacity(workers - 1);
    for _ in 0..workers - 1 {
        match response_receiver
            .recv()
            .map_err(|error| error.to_string())?
        {
            WorkerResponse::Ready { worker, .. } => available.push(worker),
            WorkerResponse::Failed { reason, .. } => return Err(reason),
            WorkerResponse::Consumed { .. } => {
                return Err("worker consumed before initialization completed".to_owned())
            }
        }
    }
    let worker_initialization = worker_initialization_started.elapsed();

    let search_started = Instant::now();
    let mut layers = BTreeMap::<usize, LayerMetrics>::new();
    let mut in_flight = 0_usize;
    let mut producer_complete = false;
    while !producer_complete || in_flight > 0 {
        while !producer_complete && !available.is_empty() {
            let layer = coordinator.progress().layer_index;
            let produce_started = Instant::now();
            let (status, batch) = coordinator
                .produce(batch_size, &control)
                .map_err(|error| error.reason())?;
            let elapsed = produce_started.elapsed();
            let metrics = layers.entry(layer).or_default();
            metrics.mark_started();
            metrics.produce += elapsed;
            match status {
                ForwardParallelProduce::Batch => {
                    let worker = available.pop().expect("available worker");
                    metrics.batches += 1;
                    metrics.input_bytes = metrics.input_bytes.saturating_add(batch.len() as u64);
                    request_senders[worker]
                        .send(WorkerRequest::Consume {
                            layer,
                            bytes: batch,
                        })
                        .map_err(|error| error.to_string())?;
                    in_flight += 1;
                }
                ForwardParallelProduce::Pending => break,
                ForwardParallelProduce::Completed => producer_complete = true,
                ForwardParallelProduce::Cancelled => {
                    return Err("parallel benchmark cancelled".to_owned())
                }
            }
        }

        if in_flight == 0 {
            if producer_complete {
                break;
            }
            continue;
        }
        let wait_started = Instant::now();
        let response = response_receiver
            .recv()
            .map_err(|error| error.to_string())?;
        let wait = wait_started.elapsed();
        match response {
            WorkerResponse::Consumed {
                worker,
                layer,
                items,
                input_bytes,
                output,
                elapsed,
            } => {
                let absorb_started = Instant::now();
                coordinator
                    .absorb(&output, &control)
                    .map_err(|error| error.reason())?;
                let absorb = absorb_started.elapsed();
                let progress = coordinator.progress();
                let metrics = layers.entry(layer).or_default();
                metrics.worker_cpu += elapsed;
                metrics.absorb += absorb;
                metrics.wait += wait;
                metrics.work_items = metrics.work_items.saturating_add(items as u64);
                debug_assert_eq!(metrics.input_bytes >= input_bytes as u64, true);
                metrics.output_bytes = metrics.output_bytes.saturating_add(output.len() as u64);
                metrics.visited_states = progress.visited_states;
                metrics.generated_locks = progress.generated_locks;
                metrics.peak_frontier = metrics.peak_frontier.max(progress.layer_total);
                metrics.mark_finished();
                available.push(worker);
                in_flight -= 1;
            }
            WorkerResponse::Failed { worker, reason } => {
                return Err(format!("worker {worker} failed: {reason}"))
            }
            WorkerResponse::Ready { worker, elapsed } => {
                return Err(format!(
                    "worker {worker} reported duplicate readiness after {} ns",
                    elapsed.as_nanos()
                ))
            }
        }
    }
    let search = search_started.elapsed();

    let finish_started = Instant::now();
    let report = coordinator
        .finish(workers)
        .map_err(|error| error.reason())?;
    let finish = finish_started.elapsed();
    for sender in &request_senders {
        let _ = sender.send(WorkerRequest::Stop);
    }
    for handle in handles {
        handle
            .join()
            .map_err(|_| "forward benchmark worker panicked".to_owned())?;
    }
    let outcome_digest = outcome_digest(&report);
    Ok(BenchmarkResult {
        prepare,
        worker_initialization,
        search,
        finish,
        report,
        outcome_digest,
        layers,
    })
}

fn worker_main(
    worker: usize,
    initialization: Vec<u8>,
    requests: Receiver<WorkerRequest>,
    responses: mpsc::Sender<WorkerResponse>,
    control: ExecutionControl,
) {
    let started = Instant::now();
    let mut worker_state = match ForwardParallelWorker::new(&initialization) {
        Ok(worker_state) => worker_state,
        Err(error) => {
            let _ = responses.send(WorkerResponse::Failed {
                worker,
                reason: error.reason().to_owned(),
            });
            return;
        }
    };
    if responses
        .send(WorkerResponse::Ready {
            worker,
            elapsed: started.elapsed(),
        })
        .is_err()
    {
        return;
    }
    while let Ok(request) = requests.recv() {
        match request {
            WorkerRequest::Consume { layer, bytes } => {
                let input_bytes = bytes.len();
                let started = Instant::now();
                match worker_state.consume(&bytes, &control) {
                    Ok((items, output)) => {
                        let response = WorkerResponse::Consumed {
                            worker,
                            layer,
                            items,
                            input_bytes,
                            output,
                            elapsed: started.elapsed(),
                        };
                        if responses.send(response).is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = responses.send(WorkerResponse::Failed {
                            worker,
                            reason: error.reason().to_owned(),
                        });
                        return;
                    }
                }
            }
            WorkerRequest::Stop => return,
        }
    }
}

fn outcome_digest(report: &ForwardSearchReport) -> u64 {
    let mut outcome_hashes = report
        .outcomes()
        .iter()
        .map(hash_outcome)
        .collect::<Vec<_>>();
    outcome_hashes.sort_unstable();
    let mut hash = Fnv1a::new();
    hash.u64(outcome_hashes.len() as u64);
    for outcome in outcome_hashes {
        hash.u64(outcome);
    }
    hash.finish()
}

fn hash_outcome(outcome: &ForwardSearchOutcome) -> u64 {
    let mut hash = Fnv1a::new();
    hash.u32(outcome.source_pattern_index());
    hash.u64(outcome.source_queue().len() as u64);
    for piece in outcome.source_queue() {
        hash.byte(piece.as_ascii() as u8);
    }
    hash.byte(outcome.group().map_or(0, |group| match group.as_str() {
        "t" => 1,
        "other" => 2,
        "integrated" => 3,
        _ => 255,
    }));
    for word in outcome.final_board() {
        hash.u64(word);
    }
    hash.byte(
        outcome
            .spin_piece()
            .map_or(0, |piece| piece.as_ascii() as u8),
    );
    hash.byte(u8::from(outcome.spin_mini()));
    hash.byte(outcome.spin_lines());
    hash.u32(outcome.total_damage());
    hash.u64(outcome.path().len() as u64);
    for step in outcome.path() {
        hash.byte(step.piece().as_ascii() as u8);
        hash.byte(step.rotation().quarter_turns());
        hash.byte(step.x() as u8);
        hash.byte(step.y() as u8);
        hash.bytes(step.hold_decision().as_bytes());
        hash.byte(0);
        hash.byte(step.cleared_lines());
        if let Some((piece, mini)) = step.spin() {
            hash.byte(piece as u8);
            hash.byte(u8::from(mini));
        } else {
            hash.byte(0);
            hash.byte(0);
        }
        hash.u32(step.damage());
        hash.u32(step.total_damage());
        for word in step.placement_mask() {
            hash.u64(word);
        }
        hash.u32(step.cleared_row_mask());
        for word in step.board_after() {
            hash.u64(word);
        }
    }
    hash.finish()
}

struct Fnv1a(u64);

impl Fnv1a {
    const fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    fn byte(&mut self, value: u8) {
        self.0 ^= u64::from(value);
        self.0 = self.0.wrapping_mul(0x100000001b3);
    }

    fn bytes(&mut self, values: &[u8]) {
        for value in values {
            self.byte(*value);
        }
    }

    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    const fn finish(self) -> u64 {
        self.0
    }
}

impl BenchmarkResult {
    fn to_json(&self, options: &Options) -> String {
        let layers = self
            .layers
            .iter()
            .map(|(index, metrics)| {
                format!(
                    concat!(
                        "{{\"depth\":{},\"wall_ns\":{},\"produce_ns\":{},",
                        "\"worker_cpu_ns\":{},\"absorb_ns\":{},\"wait_ns\":{},",
                        "\"batches\":{},\"work_items\":{},\"input_bytes\":{},",
                        "\"output_bytes\":{},\"visited_states_end\":{},",
                        "\"generated_locks_end\":{},\"peak_frontier\":{}}}"
                    ),
                    index,
                    metrics.wall().as_nanos(),
                    metrics.produce.as_nanos(),
                    metrics.worker_cpu.as_nanos(),
                    metrics.absorb.as_nanos(),
                    metrics.wait.as_nanos(),
                    metrics.batches,
                    metrics.work_items,
                    metrics.input_bytes,
                    metrics.output_bytes,
                    metrics.visited_states,
                    metrics.generated_locks,
                    metrics.peak_frontier,
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            concat!(
                "{{\"schema_version\":1,\"scenario\":\"{}\",\"mode\":\"{}\",",
                "\"workers\":{},\"batch_size\":{},\"prepare_ns\":{},",
                "\"worker_initialization_ns\":{},\"search_ns\":{},\"finish_ns\":{},",
                "\"complete\":{},\"workers_used\":{},\"visited_states\":{},",
                "\"generated_locks\":{},\"peak_frontier\":{},\"outcome_count\":{},",
                "\"maximum_damage\":{},\"outcome_digest_fnv1a64\":\"{:016x}\",",
                "\"layers\":[{}]}}"
            ),
            options.scenario,
            options.mode,
            options.workers,
            options.batch_size,
            self.prepare.as_nanos(),
            self.worker_initialization.as_nanos(),
            self.search.as_nanos(),
            self.finish.as_nanos(),
            self.report.complete(),
            self.report.workers_used(),
            self.report.visited_states(),
            self.report.generated_locks(),
            self.report.peak_frontier(),
            self.report.outcomes().len(),
            self.report
                .maximum_damage()
                .map_or("null".to_owned(), |value| value.to_string()),
            self.outcome_digest,
            layers,
        )
    }
}
