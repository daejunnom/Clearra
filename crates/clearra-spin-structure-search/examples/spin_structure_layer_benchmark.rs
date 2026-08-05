use std::{env, process::ExitCode, time::Instant};

use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_spin_structure_search::{
    MinimalityPolicy, PieceInventory, SpinStructureMode, SpinStructureQuery, SpinStructureSearcher,
    StructureBoard,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("spin-structure benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args().skip(1);
    let inventory = PieceInventory::parse(arguments.next().as_deref().unwrap_or("IOTSZ"))?;
    let mode = arguments
        .next()
        .as_deref()
        .and_then(SpinStructureMode::parse)
        .unwrap_or(SpinStructureMode::TSpins);
    let max_placements = arguments.next().and_then(|value| value.parse::<u8>().ok());
    let board_word = arguments
        .next()
        .as_deref()
        .map(parse_board_word)
        .transpose()?
        .unwrap_or(0x0000_0280_f8ff_ff8f);

    let mut query = SpinStructureQuery::new(inventory, mode);
    query.initial_board = StructureBoard::from_words([board_word, 0, 0, 0]);
    query.height = 7;
    query.fill_bottom = 0;
    query.fill_top = 5;
    query.rule_profile = RuleProfileId::Srs;
    query.max_placements = max_placements;
    query.minimality = MinimalityPolicy::SubsetMinimal;

    let started = Instant::now();
    let report = SpinStructureSearcher::run(query)?;
    let elapsed = started.elapsed();
    println!(
        "mode={} outcomes={} regular={} mini={} minimum={:?} elapsed_ms={:.3}",
        mode.as_str(),
        report.outcome_count(),
        report.regular.len(),
        report.mini.len(),
        report.minimum_placements,
        elapsed.as_secs_f64() * 1_000.0,
    );
    println!(
        "timings fill_ms={:.3} expansion_ms={:.3} finalization_ms={:.3}",
        report.timings.fill_ns as f64 / 1_000_000.0,
        report.timings.expansion_ns as f64 / 1_000_000.0,
        report.timings.finalization_ns as f64 / 1_000_000.0,
    );
    for layer in &report.layers {
        println!(
            "layer={} work_ms={:.3} input={} choices={} locks={} generated={} dedup={} checked={} regular={} mini={}",
            layer.depth,
            report.timings.layer_ns[usize::from(layer.depth)] as f64 / 1_000_000.0,
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
    Ok(())
}

fn parse_board_word(value: &str) -> Result<u64, std::num::ParseIntError> {
    u64::from_str_radix(value.trim().trim_start_matches("0x"), 16)
}
