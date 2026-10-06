//! Small actual CLI products; never enumerate an empty 24L field.
use clearra_cli::{exit::ExitCode, run_with_args};
use clearra_core_domain::board::standard_pc_board::Board256Mask;

#[test]
fn extended_tiling_real_cli_keeps_full_height_results() {
    for height in [7_u8, 8, 12, 24] {
        let starts = if height == 7 {
            vec![0, 3]
        } else {
            (0..u16::from(height)).step_by(4).collect()
        };
        let pieces = starts.len();
        let mut holes = Board256Mask::EMPTY;
        for (column, start) in starts.into_iter().enumerate() {
            for row in start..start + 4 {
                holes = holes.union(Board256Mask::singleton(row * 10 + column as u16).unwrap());
            }
        }
        let initial = Board256Mask::all_cells(u16::from(height) * 10)
            .unwrap()
            .without(holes);
        let words = initial.words();
        let board_hex = format!(
            "{:016x}{:016x}{:016x}{:016x}",
            words[3], words[2], words[1], words[0]
        );
        let command = format!(
            "clearra --format json --include-solution-data pc tiling --lines {height} --height {height} --board-mask 0x{board_hex} --pieces {pieces} --queue {} --workers 1 --backend cpu --no-hold",
            "I".repeat(pieces)
        );
        let output = run_with_args(command.split_whitespace().map(str::to_owned));
        assert_eq!(output.exit_code(), ExitCode::Success, "{output:?}");
        let json: serde_json::Value = serde_json::from_str(output.stdout()).unwrap();
        assert_eq!(json["summary"]["unique_solution_count"], 1);
        assert_eq!(json["summary"]["buildup_executed"], false);
        assert_eq!(json["summary"]["probability_calculated"], false);
        assert!(output.stdout().contains(&format!(
            "ctk2|height={height}|initial={board_hex}|placements="
        )));
    }
}
