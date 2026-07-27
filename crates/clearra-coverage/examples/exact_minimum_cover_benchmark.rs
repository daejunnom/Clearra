use std::{env, process::ExitCode, time::Instant};

use clearra_coverage::{
    cover::exact_minimum_cover::exact_minimum_cover, pattern::pattern_bitset::PatternBitSet,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let row_count = argument(1, 8_192)?;
    let pattern_count = argument(2, 5_040)?;
    let distinct_row_count = argument(3, 64)?;
    if row_count < distinct_row_count || distinct_row_count == 0 {
        return Err("row_count must be at least distinct_row_count, which must be positive".into());
    }

    let required = PatternBitSet::all(pattern_count);
    let rows = (0..row_count)
        .map(|row_index| {
            let partition = row_index % distinct_row_count;
            let mut words = vec![0_u64; pattern_count.div_ceil(u64::BITS as usize)];
            for pattern in (partition..pattern_count).step_by(distinct_row_count) {
                words[pattern / u64::BITS as usize] |= 1_u64 << (pattern % u64::BITS as usize);
            }
            PatternBitSet::from_words(pattern_count, words)
                .expect("benchmark rows use the requested universe")
        })
        .collect::<Vec<_>>();

    let started = Instant::now();
    let result = exact_minimum_cover(&required, &rows)
        .map_err(|error| format!("minimum-cover failed: {error:?}"))?;
    let elapsed = started.elapsed();
    if !result.complete() || result.row_indices().len() != distinct_row_count {
        return Err(format!(
            "unexpected result: complete={} selected={}",
            result.complete(),
            result.row_indices().len()
        ));
    }

    println!(
        "{{\"rows\":{row_count},\"patterns\":{pattern_count},\"distinct_rows\":{distinct_row_count},\"selected_rows\":{},\"complete\":{},\"elapsed_ms\":{:.3}}}",
        result.row_indices().len(),
        result.complete(),
        elapsed.as_secs_f64() * 1_000.0
    );
    Ok(())
}

fn argument(index: usize, default: usize) -> Result<usize, String> {
    env::args()
        .nth(index)
        .map_or(Ok(default), |value| {
            value
                .parse()
                .map_err(|_| format!("argument {index} must be a positive integer"))
        })
        .and_then(|value| {
            (value > 0)
                .then_some(value)
                .ok_or_else(|| format!("argument {index} must be positive"))
        })
}
