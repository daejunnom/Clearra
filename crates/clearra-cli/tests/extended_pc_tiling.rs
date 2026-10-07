//! Small actual CLI products; never enumerate an empty 24L field.
use clearra_cli::{exit::ExitCode, run_with_args};
use clearra_core_domain::board::standard_pc_board::Board256Mask;

#[test]
fn empty_extended_cli_opening_preserves_resource_failure_without_large_enumeration() {
    for height in [8, 24] {
        for product in ["pc", "pc tiling"] {
            // Bare `pc` preserves its legacy observed-queue default. The
            // product `pc tiling` parses --queue as a fixed sequence. Both
            // arms must explicitly represent the intended fixed I supply;
            // bypassing observed 7-bag validation would change public meaning.
            let queue_mode = if product == "pc" {
                " --fixed-queue"
            } else {
                ""
            };
            let command = format!(
                "clearra --format json {product} --lines {height} --queue {}{queue_mode} --workers 1 --backend cpu --no-hold --max-memory-mib 1",
                "I".repeat(height * 10 / 4),
            );
            let output = run_with_args(command.split_whitespace().map(str::to_owned));
            assert_eq!(output.exit_code(), ExitCode::InternalError, "{output:?}");
            let json: serde_json::Value = serde_json::from_str(output.stdout()).unwrap();
            assert_eq!(json["kind"], "execution-failed");
            // Preserve the legacy command's error envelope while requiring
            // the same typed resource refusal from the underlying PC engine.
            assert_eq!(
                json["error"]["code"],
                if product == "pc" {
                    "E_PC_SEARCH_INTERNAL"
                } else {
                    "E_PRODUCT_EXECUTION_FAILED"
                }
            );
            assert_eq!(
                json["resource_report"]["execution_availability"]["reason"],
                "memory-budget-exceeded"
            );
            // Schema v2 duplicates error/resource fields in `summary`. It is
            // not a successful PC summary: no counts, family, or probability
            // may be published after a finite admission failure.
            for field in [
                "unique_solution_count",
                "normalized_solution_keys",
                "count_complete",
                "probability",
            ] {
                assert!(json["summary"].get(field).is_none(), "{json}");
            }
            assert!(json.get("solution_data").is_none());
        }
    }
}

#[test]
fn extended_native_opening_does_not_bypass_observed_bag_validation() {
    let command = format!(
        "clearra --format json pc --lines 8 --queue {} --workers 1 --backend cpu --no-hold --max-memory-mib 1",
        "I".repeat(20),
    );
    let output = run_with_args(command.split_whitespace().map(str::to_owned));
    assert_eq!(output.exit_code(), ExitCode::ValidationFailed, "{output:?}");
    let json: serde_json::Value = serde_json::from_str(output.stdout()).unwrap();
    assert_eq!(json["kind"], "diagnostic");
    assert!(json["summary"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "E_SUPPLY_INVALID_DUPLICATE"));
    assert!(json.get("solution_data").is_none());
}

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
        // The explicit summary schema renders this diagnostic as a string;
        // probability_calculated is a separately registered boolean field.
        assert_eq!(json["summary"]["buildup_executed"], "false");
        assert_eq!(json["summary"]["probability_calculated"], false);
        assert_eq!(json["summary"]["tiling_family_complete"], true);
        assert_eq!(json["summary"]["count_complete"], true);
        assert_eq!(json["summary"]["workers_used"], 1);
        assert!(output.stdout().contains(&format!(
            "ctk2|height={height}|initial={board_hex}|placements="
        )));
    }
}

#[test]
fn extended_parallel_tiling_real_cli_keeps_complete_keys_and_full_height() {
    const HEIGHT: u8 = 24;
    let mut holes = Board256Mask::EMPTY;
    for row in 0..4 {
        for column in 0..4 {
            holes = holes.union(Board256Mask::singleton(row * 10 + column).unwrap());
        }
    }
    // No complete initial row, and one forced column per remaining block.
    // This leaves exactly two small families rather than a large 24L search.
    for row in 4..u16::from(HEIGHT) {
        let column = 5 + (row - 4) / 4;
        holes = holes.union(Board256Mask::singleton(row * 10 + column).unwrap());
    }
    let pieces = holes.count_ones() as usize / 4;
    let initial = Board256Mask::all_cells(u16::from(HEIGHT) * 10)
        .unwrap()
        .without(holes);
    let words = initial.words();
    let board_hex = format!(
        "{:016x}{:016x}{:016x}{:016x}",
        words[3], words[2], words[1], words[0]
    );
    let logical_processors = std::thread::available_parallelism().map_or(1, usize::from);
    let all_cpu_opt_in = if logical_processors < 3 {
        " --use-all-cpu-threads"
    } else {
        ""
    };
    let mut baseline = None;
    for workers in [1, 2] {
        let command = format!(
            "clearra --format json --include-solution-data pc tiling --lines {HEIGHT} --height {HEIGHT} --board-mask 0x{board_hex} --pieces {pieces} --queue {} --workers {workers}{all_cpu_opt_in} --backend cpu --no-hold",
            "I".repeat(pieces)
        );
        let output = run_with_args(command.split_whitespace().map(str::to_owned));
        if workers > logical_processors {
            assert_eq!(output.exit_code(), ExitCode::ValidationFailed, "{output:?}");
            assert!(
                baseline.is_some(),
                "the valid one-worker baseline completed"
            );
            continue;
        }
        assert_eq!(output.exit_code(), ExitCode::Success, "{output:?}");
        let json: serde_json::Value = serde_json::from_str(output.stdout()).unwrap();
        assert_eq!(json["summary"]["unique_solution_count"], 2);
        assert_eq!(json["summary"]["workers_requested"], workers);
        assert_eq!(json["summary"]["workers_used"], workers);
        assert_eq!(json["summary"]["tiling_family_complete"], true);
        assert_eq!(json["summary"]["count_complete"], true);
        assert_eq!(json["summary"]["buildup_executed"], "false");
        assert_eq!(json["contract"]["solution_data"]["status"], "complete");
        let keys = &json["contract"]["artifacts"]["solution_keys"];
        assert_eq!(keys.as_array().unwrap().len(), 2);
        for key in keys.as_array().unwrap() {
            let identity =
                clearra_core_domain::solution::ExtendedTilingSolutionKey::parse_canonical(
                    key.as_str().unwrap(),
                )
                .unwrap();
            assert_eq!(identity.height(), HEIGHT);
            assert_eq!(identity.initial_board(), initial);
            assert_eq!(identity.placement_count(), pieces);
        }
        let hash = &json["summary"]["normalized_solution_set_hash"];
        assert!(hash.as_str().is_some());
        let snapshot = (hash.clone(), keys.clone());
        if let Some(expected) = baseline.as_ref() {
            assert_eq!(&snapshot, expected);
        } else {
            baseline = Some(snapshot);
        }
    }
}

#[test]
fn extended_ordinary_pc_real_cli_verifies_buildup_without_truncating_the_field() {
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
        let words = Board256Mask::all_cells(u16::from(height) * 10)
            .unwrap()
            .without(holes)
            .words();
        let board_hex = format!(
            "{:016x}{:016x}{:016x}{:016x}",
            words[3], words[2], words[1], words[0]
        );
        let command = format!(
            "clearra --format json --include-solution-data pc --lines {height} --height {height} --board-mask 0x{board_hex} --pieces {pieces} --queue {} --workers 1 --backend cpu --no-hold",
            "I".repeat(pieces)
        );
        let output = run_with_args(command.split_whitespace().map(str::to_owned));
        assert_eq!(output.exit_code(), ExitCode::Success, "{output:?}");
        let json: serde_json::Value = serde_json::from_str(output.stdout()).unwrap();
        assert_eq!(json["summary"]["unique_solution_count"], 1);
        assert_eq!(json["summary"]["buildup_executed"], "true");
        assert_eq!(json["summary"]["count_complete"], true);
        assert_eq!(json["summary"]["workers_used"], 1);
        assert!(output.stdout().contains(&format!(
            "ctk2|height={height}|initial={board_hex}|placements="
        )));
    }
}
