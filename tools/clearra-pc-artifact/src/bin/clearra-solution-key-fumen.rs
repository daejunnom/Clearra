use std::{env, fs, path::PathBuf};

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_fumen::{ColoredSolutionFumenExporter, ColoredSolutionPage, ColoredSolutionPlacement};

const KEY_PREFIX: &str = "ctk1|initial=";
const PLACEMENTS_SEPARATOR: &str = "|placements=";

fn main() {
    if let Err(error) = run() {
        eprintln!("clearra-solution-key-fumen: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let input = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: clearra-solution-key-fumen <keys.txt> <output.txt>".to_owned())?;
    let output = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: clearra-solution-key-fumen <keys.txt> <output.txt>".to_owned())?;
    if args.next().is_some() {
        return Err("expected exactly two paths".to_owned());
    }

    let source = fs::read_to_string(&input)
        .map_err(|error| format!("failed to read {}: {error}", input.display()))?;
    let pages = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
        .map(|(index, line)| parse_page(line, index + 1))
        .collect::<Result<Vec<_>, _>>()?;
    let encoded = ColoredSolutionFumenExporter::encode(&pages)
        .map_err(|error| format!("failed to encode {} pages: {error:?}", pages.len()))?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(&output, format!("{encoded}\n"))
        .map_err(|error| format!("failed to write {}: {error}", output.display()))?;
    println!("pages={} output={}", pages.len(), output.display());
    Ok(())
}

fn parse_page(line: &str, page_number: usize) -> Result<ColoredSolutionPage, String> {
    let payload = line
        .strip_prefix(KEY_PREFIX)
        .ok_or_else(|| format!("page {page_number}: unsupported normalized-key version"))?;
    let (initial, placements) = payload
        .split_once(PLACEMENTS_SEPARATOR)
        .ok_or_else(|| format!("page {page_number}: placements section is missing"))?;
    let initial_board_mask = u64::from_str_radix(initial, 16)
        .map_err(|_| format!("page {page_number}: invalid initial board mask"))?;
    let placements = placements
        .split(',')
        .map(|placement| parse_placement(placement, page_number))
        .collect::<Result<Vec<_>, _>>()?;
    ColoredSolutionPage::new(10, 4, initial_board_mask, placements)
        .map(|page| page.with_comment(format!("Clearra TETRIO-180 comparison {page_number}")))
        .map_err(|error| format!("page {page_number}: invalid colored solution: {error:?}"))
}

fn parse_placement(
    placement: &str,
    page_number: usize,
) -> Result<ColoredSolutionPlacement, String> {
    let (piece, cells) = placement
        .split_once(':')
        .ok_or_else(|| format!("page {page_number}: invalid placement {placement}"))?;
    let piece = match piece {
        "I" => PieceKind::I,
        "O" => PieceKind::O,
        "T" => PieceKind::T,
        "S" => PieceKind::S,
        "Z" => PieceKind::Z,
        "J" => PieceKind::J,
        "L" => PieceKind::L,
        _ => return Err(format!("page {page_number}: unknown piece {piece}")),
    };
    let cells_mask = u64::from_str_radix(cells, 16)
        .map_err(|_| format!("page {page_number}: invalid placement mask {cells}"))?;
    Ok(ColoredSolutionPlacement::new(piece, cells_mask))
}
