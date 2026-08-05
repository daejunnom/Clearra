use std::{
    env,
    error::Error,
    fmt,
    process::ExitCode,
    time::{Duration, Instant},
};

use clearra_app::{AppCommand, AppContext, AppRequest, ResourceBudget, SpinStructureAppCommand};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_spin_structure_search::{
    LayerMetrics, MinimalityPolicy, PieceInventory, SpinLineRequirement, SpinStructureMode,
    SpinStructureOutcome, SpinStructureQuery, SpinStructureReport, SpinStructureStageMetrics,
    SpinStructureTimingMetrics, StructureBoard, StructureOperation,
};

const DEFAULT_FIXTURE: &str = "smoke-t";
const DIGEST_DOMAIN: &[u8] = b"clearra-spin-structure-outcomes-v2";

const FIXTURES: &[FixtureSpec] = &[
    FixtureSpec {
        name: "smoke-t",
        inventory: "T",
        mode: SpinStructureMode::TSpins,
        board: [(1_u64 << 24) | (1_u64 << 26) | (1_u64 << 4), 0, 0, 0],
        height: 4,
        fill_bottom: 0,
        fill_top: 4,
        lines: SpinLineRequirement::Any,
        rule: RuleProfileId::SrsPlus,
        max_placements: Some(1),
        minimality: MinimalityPolicy::SubsetMinimal,
    },
    reference_fixture("reference-t", SpinStructureMode::TSpins),
    reference_fixture("reference-t-plus", SpinStructureMode::TSpinsPlus),
    reference_fixture("reference-all-mini", SpinStructureMode::AllMini),
    reference_fixture("reference-all-mini-plus", SpinStructureMode::AllMiniPlus),
    reference_fixture("reference-all-spin", SpinStructureMode::AllSpin),
    reference_fixture("reference-all-spin-plus", SpinStructureMode::AllSpinPlus),
];

const fn reference_fixture(name: &'static str, mode: SpinStructureMode) -> FixtureSpec {
    FixtureSpec {
        name,
        inventory: "IOTSZ",
        mode,
        board: [0x0000_0280_f8ff_ff8f, 0, 0, 0],
        height: 7,
        fill_bottom: 0,
        fill_top: 5,
        lines: SpinLineRequirement::AtLeast(1),
        rule: RuleProfileId::Srs,
        max_placements: None,
        minimality: MinimalityPolicy::SubsetMinimal,
    }
}

#[derive(Clone, Copy, Debug)]
struct FixtureSpec {
    name: &'static str,
    inventory: &'static str,
    mode: SpinStructureMode,
    board: [u64; 4],
    height: u8,
    fill_bottom: u8,
    fill_top: u8,
    lines: SpinLineRequirement,
    rule: RuleProfileId,
    max_placements: Option<u8>,
    minimality: MinimalityPolicy,
}

impl FixtureSpec {
    fn query(self) -> Result<SpinStructureQuery, BenchmarkError> {
        let inventory = PieceInventory::parse(self.inventory)
            .map_err(|error| BenchmarkError::new(format!("invalid fixture inventory: {error}")))?;
        let mut query = SpinStructureQuery::new(inventory, self.mode);
        query.initial_board = StructureBoard::from_words(self.board);
        query.height = self.height;
        query.fill_bottom = self.fill_bottom;
        query.fill_top = self.fill_top;
        query.line_requirement = self.lines;
        query.rule_profile = self.rule;
        query.max_placements = self.max_placements;
        query.minimality = self.minimality;
        query
            .validate()
            .map_err(|error| BenchmarkError::new(format!("invalid fixture query: {error}")))?;
        Ok(query)
    }
}

#[derive(Debug)]
struct BenchmarkError(String);

impl BenchmarkError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for BenchmarkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for BenchmarkError {}

#[derive(Debug)]
struct Options {
    fixtures: Vec<&'static FixtureSpec>,
    workers: u16,
    repetitions: usize,
}

enum ParseAction {
    Run(Options),
    Help,
    List,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SemanticSnapshot {
    digest: u64,
    regular: usize,
    mini: usize,
    minimum_placements: Option<u8>,
    complete: bool,
    workers_used: u16,
    stages: SpinStructureStageMetrics,
    layers: Vec<LayerMetrics>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("spin-structure native benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), BenchmarkError> {
    match parse_options(env::args().skip(1))? {
        ParseAction::Help => {
            print_usage();
            Ok(())
        }
        ParseAction::List => {
            print_fixtures();
            Ok(())
        }
        ParseAction::Run(options) => run_benchmarks(options),
    }
}

fn parse_options(
    arguments: impl IntoIterator<Item = String>,
) -> Result<ParseAction, BenchmarkError> {
    let mut arguments = arguments.into_iter();
    let mut fixture_names = Vec::new();
    let mut all_fixtures = false;
    let mut workers = 1_u16;
    let mut repetitions = 1_usize;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-h" | "--help" => return Ok(ParseAction::Help),
            "--list" => return Ok(ParseAction::List),
            "--all" => all_fixtures = true,
            "--fixture" => fixture_names.push(next_value(&mut arguments, "--fixture")?),
            "--workers" => {
                workers = parse_nonzero(&next_value(&mut arguments, "--workers")?, "workers")?
            }
            "-n" | "--repetitions" => {
                repetitions =
                    parse_nonzero(&next_value(&mut arguments, "--repetitions")?, "repetitions")?
            }
            _ if argument.starts_with("--fixture=") => {
                fixture_names.push(argument["--fixture=".len()..].to_owned())
            }
            _ if argument.starts_with("--workers=") => {
                workers = parse_nonzero(&argument["--workers=".len()..], "workers")?
            }
            _ if argument.starts_with("--repetitions=") => {
                repetitions = parse_nonzero(&argument["--repetitions=".len()..], "repetitions")?
            }
            _ => return Err(BenchmarkError::new(format!("unknown argument: {argument}"))),
        }
    }

    if all_fixtures && !fixture_names.is_empty() {
        return Err(BenchmarkError::new(
            "--all cannot be combined with explicit --fixture values",
        ));
    }
    if all_fixtures {
        return Ok(ParseAction::Run(Options {
            fixtures: FIXTURES.iter().collect(),
            workers,
            repetitions,
        }));
    }
    if fixture_names.is_empty() {
        fixture_names.push(DEFAULT_FIXTURE.to_owned());
    }
    let fixtures = fixture_names
        .iter()
        .map(|name| find_fixture(name))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ParseAction::Run(Options {
        fixtures,
        workers,
        repetitions,
    }))
}

fn next_value(
    arguments: &mut impl Iterator<Item = String>,
    option: &str,
) -> Result<String, BenchmarkError> {
    arguments
        .next()
        .ok_or_else(|| BenchmarkError::new(format!("{option} requires a value")))
}

fn parse_nonzero<T>(value: &str, name: &str) -> Result<T, BenchmarkError>
where
    T: std::str::FromStr + PartialEq + From<u8>,
{
    let value = value
        .parse::<T>()
        .map_err(|_| BenchmarkError::new(format!("invalid {name}: {value}")))?;
    if value == T::from(0) {
        return Err(BenchmarkError::new(format!("{name} must be at least 1")));
    }
    Ok(value)
}

fn find_fixture(name: &str) -> Result<&'static FixtureSpec, BenchmarkError> {
    FIXTURES
        .iter()
        .find(|fixture| fixture.name == name)
        .ok_or_else(|| BenchmarkError::new(format!("unknown fixture: {name}")))
}

fn print_usage() {
    println!(
        "usage: cargo run -p clearra-app --release --example \
spin_structure_native_benchmark -- \
[--fixture NAME ... | --all] [--workers N] [--repetitions N]\n\
default fixture: {DEFAULT_FIXTURE}\n\
--fixture is repeatable; each selected fixture runs exactly N measured requests\n\
--list prints the fixed fixture names without running a search"
    );
}

fn print_fixtures() {
    for fixture in FIXTURES {
        println!(
            "fixture={} inventory={} mode={} height={} fill={}..{} lines={} rule={:?} max_placements={} minimality={}",
            fixture.name,
            fixture.inventory,
            fixture.mode.as_str(),
            fixture.height,
            fixture.fill_bottom,
            fixture.fill_top,
            fixture.lines.as_str(),
            fixture.rule,
            optional_u8(fixture.max_placements),
            fixture.minimality.as_str(),
        );
    }
}

fn run_benchmarks(options: Options) -> Result<(), BenchmarkError> {
    let context = AppContext::default();
    println!("schema=clearra-spin-structure-native-benchmark-v1");
    println!(
        "stage_order=build_states,fill_checks,support_locks,corner_checks,entry_states,verification_checks,exact_state_deduplications,exact_outcome_deduplications"
    );
    println!(
        "layer_order=depth,input_states,piece_choices,reachable_locks,generated_states,exact_duplicates,terminal_candidates,accepted_regular,accepted_mini"
    );

    for fixture in options.fixtures {
        run_fixture(&context, fixture, options.workers, options.repetitions)?;
    }
    Ok(())
}

fn run_fixture(
    context: &AppContext,
    fixture: &FixtureSpec,
    workers: u16,
    repetitions: usize,
) -> Result<(), BenchmarkError> {
    let query = fixture.query()?;
    println!(
        "fixture_begin name={} repetitions={} workers_requested={} inventory={} mode={} height={} fill={}..{} lines={} rule={:?} max_placements={} minimality={}",
        fixture.name,
        repetitions,
        workers,
        fixture.inventory,
        fixture.mode.as_str(),
        fixture.height,
        fixture.fill_bottom,
        fixture.fill_top,
        fixture.lines.as_str(),
        fixture.rule,
        optional_u8(fixture.max_placements),
        fixture.minimality.as_str(),
    );

    let mut durations = Vec::with_capacity(repetitions);
    let mut expected: Option<SemanticSnapshot> = None;
    for repetition in 1..=repetitions {
        let request = AppRequest::new(AppCommand::SpinStructure(SpinStructureAppCommand::new(
            query.clone(),
        )))
        .with_resource_budget(ResourceBudget::new(workers, None, None));

        // The timer surrounds the typed AppContext request. Query construction,
        // digesting, validation of deterministic counters, and printing stay
        // outside the measured interval and outside the engine's hot loops.
        let started = Instant::now();
        let response = context.run(request);
        let elapsed = started.elapsed();
        let report = response
            .render_model()
            .and_then(|model| model.spin_structure_result())
            .ok_or_else(|| {
                let detail = response.error().map_or_else(
                    || format!("status={:?}", response.status()),
                    |error| format!("code={:?} message={}", error.code(), error.message()),
                );
                BenchmarkError::new(format!("fixture {} failed: {detail}", fixture.name))
            })?;

        let snapshot = semantic_snapshot(report);
        let timings = report.timings;
        if let Some(first) = &expected {
            if first != &snapshot {
                return Err(BenchmarkError::new(format!(
                    "fixture {} changed semantic output or fixed counters between repetitions 1 and {repetition}: first={:016x} current={:016x}",
                    fixture.name, first.digest, snapshot.digest,
                )));
            }
        } else {
            expected = Some(snapshot.clone());
        }
        durations.push(elapsed);
        print_run(
            fixture.name,
            repetition,
            repetitions,
            elapsed,
            &snapshot,
            timings,
        );
    }

    let expected = expected.expect("positive repetition count");
    print_summary(fixture.name, workers, &durations, &expected);
    Ok(())
}

fn semantic_snapshot(report: &SpinStructureReport) -> SemanticSnapshot {
    let mut layers = report.layers.clone();
    layers.sort_by_key(|layer| layer.depth);
    SemanticSnapshot {
        digest: outcome_digest(report),
        regular: report.regular.len(),
        mini: report.mini.len(),
        minimum_placements: report.minimum_placements,
        complete: report.complete,
        workers_used: report.workers_used(),
        stages: report.stages,
        layers,
    }
}

fn print_run(
    fixture: &str,
    repetition: usize,
    repetitions: usize,
    elapsed: Duration,
    snapshot: &SemanticSnapshot,
    timings: SpinStructureTimingMetrics,
) {
    println!(
        "run fixture={} repetition={}/{} wall_ms={:.3} workers_used={} complete={} outcomes={} regular={} mini={} minimum_placements={} outcome_digest_fnv1a64={:016x}",
        fixture,
        repetition,
        repetitions,
        elapsed.as_secs_f64() * 1_000.0,
        snapshot.workers_used,
        snapshot.complete,
        snapshot.regular + snapshot.mini,
        snapshot.regular,
        snapshot.mini,
        optional_u8(snapshot.minimum_placements),
        snapshot.digest,
    );
    let stages = snapshot.stages;
    println!(
        "stages fixture={} repetition={} build_states={} fill_checks={} support_locks={} corner_checks={} entry_states={} verification_checks={} exact_state_deduplications={} exact_outcome_deduplications={}",
        fixture,
        repetition,
        stages.build_states,
        stages.fill_checks,
        stages.support_locks,
        stages.corner_checks,
        stages.entry_states,
        stages.verification_checks,
        stages.exact_state_deduplications,
        stages.exact_outcome_deduplications,
    );
    println!(
        "timings fixture={} repetition={} fill_ms={:.3} expansion_ms={:.3} finalization_ms={:.3} measured_work_ms={:.3}",
        fixture,
        repetition,
        ns_ms(timings.fill_ns),
        ns_ms(timings.expansion_ns),
        ns_ms(timings.finalization_ns),
        ns_ms(
            timings
                .fill_ns
                .saturating_add(timings.expansion_ns)
                .saturating_add(timings.finalization_ns)
        ),
    );
    for layer in &snapshot.layers {
        println!(
            "layer fixture={} repetition={} depth={} work_ms={:.3} input_states={} piece_choices={} reachable_locks={} generated_states={} exact_duplicates={} terminal_candidates={} accepted_regular={} accepted_mini={}",
            fixture,
            repetition,
            layer.depth,
            ns_ms(timings.layer_ns[usize::from(layer.depth)]),
            layer.input_states,
            layer.piece_choices,
            layer.reachable_locks,
            layer.generated_states,
            layer.exact_duplicates,
            layer.terminal_candidates,
            layer.accepted_regular,
            layer.accepted_mini,
        );
    }
}

fn ns_ms(value: u64) -> f64 {
    value as f64 / 1_000_000.0
}

fn print_summary(
    fixture: &str,
    workers_requested: u16,
    durations: &[Duration],
    snapshot: &SemanticSnapshot,
) {
    let mut milliseconds = durations
        .iter()
        .map(|duration| duration.as_secs_f64() * 1_000.0)
        .collect::<Vec<_>>();
    milliseconds.sort_by(f64::total_cmp);
    let mean = milliseconds.iter().sum::<f64>() / milliseconds.len() as f64;
    let median = if milliseconds.len() % 2 == 0 {
        let upper = milliseconds.len() / 2;
        (milliseconds[upper - 1] + milliseconds[upper]) / 2.0
    } else {
        milliseconds[milliseconds.len() / 2]
    };
    println!(
        "fixture_end name={} repetitions={} workers_requested={} workers_used={} min_ms={:.3} median_ms={:.3} mean_ms={:.3} max_ms={:.3} outcome_digest_fnv1a64={:016x}",
        fixture,
        milliseconds.len(),
        workers_requested,
        snapshot.workers_used,
        milliseconds[0],
        median,
        mean,
        milliseconds[milliseconds.len() - 1],
        snapshot.digest,
    );
}

fn optional_u8(value: Option<u8>) -> String {
    value.map_or_else(|| "none".to_owned(), |value| value.to_string())
}

fn outcome_digest(report: &SpinStructureReport) -> u64 {
    let mut digest = Fnv1a64::new();
    digest.write(DIGEST_DOMAIN);
    digest.write_bool(report.complete);
    write_outcome_partition(&mut digest, 0, &report.regular);
    write_outcome_partition(&mut digest, 1, &report.mini);
    digest.finish()
}

fn write_outcome_partition(digest: &mut Fnv1a64, partition: u8, outcomes: &[SpinStructureOutcome]) {
    let mut outcome_digests = outcomes.iter().map(digest_outcome).collect::<Vec<_>>();
    outcome_digests.sort_unstable();
    digest.write_u8(partition);
    digest.write_u64(outcome_digests.len() as u64);
    for outcome in outcome_digests {
        digest.write_u64(outcome);
    }
}

fn digest_outcome(outcome: &SpinStructureOutcome) -> u64 {
    let mut digest = Fnv1a64::new();
    digest.write_bool(outcome.mini);
    digest.write_u64(digest_operation(outcome.logical_spin()));
    digest.write_u32(outcome.logical_spin_cleared_rows());
    let mut build = outcome
        .logical_operations()
        .iter()
        .copied()
        .map(digest_operation)
        .collect::<Vec<_>>();
    build.sort_unstable();
    digest.write_u64(build.len() as u64);
    for placement in build {
        digest.write_u64(placement);
    }
    digest.finish()
}

fn digest_operation(operation: StructureOperation) -> u64 {
    let mut digest = Fnv1a64::new();
    digest.write_u8(operation.piece().as_ascii() as u8);
    digest.write_u8(operation.rotation().quarter_turns());
    digest.write_i8(operation.x());
    digest.write_i8(operation.y());
    digest.write_board(operation.mask());
    digest.write_u32(operation.need_deleted_rows());
    digest.finish()
}

struct Fnv1a64(u64);

impl Fnv1a64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    const fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn write_board(&mut self, board: StructureBoard) {
        for word in board.words() {
            self.write_u64(word);
        }
    }

    fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    fn write_i8(&mut self, value: i8) {
        self.write(&value.to_le_bytes());
    }

    fn write_u8(&mut self, value: u8) {
        self.write(&[value]);
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    const fn finish(self) -> u64 {
        self.0
    }
}
