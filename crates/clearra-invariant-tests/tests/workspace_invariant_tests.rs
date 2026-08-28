#![cfg_attr(not(feature = "native-c-core"), allow(unused_imports))]

use clearra_build_coverage::template::{BuildSlot, BuildSlotId, BuildTemplate};
use clearra_core_domain::{
    board::cell::CellCoord,
    ids::setup_id::{BuildVariantId, SetupFamilyId, TilingVariantId},
    pc::pc_target::PcTarget,
    piece::piece_kind::PieceKind,
};
use clearra_core_executor::CoreExecutor;
use clearra_coverage::{
    pattern::{
        pattern_bitset::PatternBitSet, pattern_id::PatternId,
        weighted_pattern_set::WeightedPatternSet,
    },
    probability::union_probability::union_probability,
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_geometry::placement::placement_mask::PlacementMask;
use clearra_pc_graph::{
    classification::{BagPhaseClassifier, ChainClassifier},
    dag::{CheckpointSchedule, ContinuationHint},
    request::{
        opening_pc_search_query::OpeningPcSearchQuery, pc_queue_input::PcQueueInput,
        pc_scenario_query::PcScenarioQuery, PcScenarioBoard, PieceWindow,
    },
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_problem::ProblemCompiler;
use clearra_setup_search::{
    coverage::{
        setup_coverage_builder::SetupCoverageBuilder, setup_union_coverage::SetupUnionCoverage,
    },
    identity::{build_identity::BuildIdentity, shape_family::ShapeFamily},
    variant::build_variant::BuildVariant,
};
use clearra_supply::queue::{fixed_sequence::FixedSequence, observed_queue::ObservedQueue};
use clearra_validation::{
    diagnostic::diagnostic_code::{DiagnosticCode, DiagnosticSeverity},
    validators::supply_validator::validate_observed_queue,
};
use std::{fs, path::Path};

pub fn read_workspace_directory(path: &str) -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf();
    let directory = root.join(path);
    let mut entries = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("failed to read directory {path}: {error}"))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("failed to enumerate directory {path}: {error}"));
    entries.sort_by_key(|entry| entry.path());

    let mut text = String::new();
    for entry in entries {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            let relative = entry_path
                .strip_prefix(&root)
                .expect("workspace child")
                .to_string_lossy()
                .replace('\\', "/");
            text.push_str(&read_workspace_directory(&relative));
        } else {
            text.push_str(
                &std::fs::read_to_string(&entry_path).unwrap_or_else(|error| {
                    panic!("failed to read {}: {error}", entry_path.display())
                }),
            );
            text.push('\n');
        }
    }
    text
}
pub fn read_workspace_responsibility(path: &str) -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf();
    let relative_path = std::path::Path::new(path);
    let mut text = std::fs::read_to_string(root.join(relative_path))
        .unwrap_or_else(|error| panic!("failed to read {path}: {error}"));
    let parent = relative_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));
    let stem = relative_path
        .file_stem()
        .and_then(|value| value.to_str())
        .expect("responsibility file stem");

    for suffix in ["functions", "types", "impls", "methods", "api"] {
        let companion = parent.join(format!("{stem}_{suffix}"));
        if root.join(&companion).is_dir() {
            text.push('\n');
            text.push_str(&read_workspace_directory(
                &companion.to_string_lossy().replace('\\', "/"),
            ));
        }
    }
    text
}
pub fn read_workspace_contract_surface(path: &str) -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf();
    let relative_path = std::path::Path::new(path);
    let mut text = read_workspace_responsibility(path);
    let file_name = relative_path
        .file_name()
        .and_then(|value| value.to_str())
        .expect("contract file name");

    if file_name == "lib.rs" || file_name == "mod.rs" {
        let parent = relative_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        text.push('\n');
        text.push_str(&read_workspace_directory(
            &parent.to_string_lossy().replace('\\', "/"),
        ));
    }

    if relative_path.extension().and_then(|value| value.to_str()) == Some("rs") {
        let parent = relative_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        let stem = relative_path
            .file_stem()
            .and_then(|value| value.to_str())
            .expect("Rust contract stem");
        for suffix in ["tests", "contract_tests"] {
            let test_file = parent.join(format!("{stem}_{suffix}.rs"));
            if root.join(&test_file).is_file() {
                text.push('\n');
                text.push_str(&read_workspace_responsibility(
                    &test_file.to_string_lossy().replace('\\', "/"),
                ));
            }
        }
    }

    if relative_path.extension().and_then(|value| value.to_str()) == Some("ps1") {
        let parent = relative_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        let stem = relative_path
            .file_stem()
            .and_then(|value| value.to_str())
            .expect("PowerShell contract stem");
        let mut entries = std::fs::read_dir(root.join(parent))
            .expect("PowerShell contract directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("PowerShell contract entries");
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let entry_name = entry.file_name().to_string_lossy().to_string();
            if !entry_name.starts_with(&format!("{stem}."))
                && !entry_name.starts_with(&format!("{stem}_"))
            {
                continue;
            }
            let companion = parent.join(&entry_name);
            text.push('\n');
            if entry.path().is_dir() {
                text.push_str(&read_workspace_directory(
                    &companion.to_string_lossy().replace('\\', "/"),
                ));
            } else {
                text.push_str(&read_workspace_responsibility(
                    &companion.to_string_lossy().replace('\\', "/"),
                ));
            }
        }
    }
    text
}

fn variant(id: u32, patterns: impl IntoIterator<Item = PatternId>) -> BuildVariant {
    BuildVariant::new(
        BuildVariantId::new(id),
        TilingVariantId::new(1),
        BuildIdentity::new(0b1111, Some(PieceKind::I)),
        PatternBitSet::from_patterns(4, patterns).expect("coverage"),
    )
}

fn setup_probability<'a>(
    family: ShapeFamily,
    variants: impl IntoIterator<Item = &'a BuildVariant>,
    weights: &WeightedPatternSet,
) -> f64 {
    let mut builder = SetupCoverageBuilder::new(family, 4);
    for variant in variants {
        builder.push_variant(variant).expect("variant");
    }
    let matrix = builder.build().expect("matrix");
    let union = SetupUnionCoverage::from_matrix(family.id(), &matrix);

    union_probability(union.covered_patterns(), weights)
        .expect("probability")
        .get()
}

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn active_manifest_lines(contents: &str) -> impl Iterator<Item = &str> {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

fn rust_sources(root: &Path) -> Vec<std::path::PathBuf> {
    let mut result = Vec::new();
    collect_rust_sources(root, &mut result);
    result
}

fn collect_rust_sources(root: &Path, result: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(root).expect("source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            collect_rust_sources(&path, result);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            result.push(path);
        }
    }
}

mod case_invariant_setup_union_probability_is_invariant_to_variant_order {
    use super::*;

    #[test]
    fn invariant_setup_union_probability_is_invariant_to_variant_order() {
        let family_id = SetupFamilyId::new(1);
        let family = ShapeFamily::new(family_id, 0b1111);
        let variant_a = variant(10, [PatternId::new(0), PatternId::new(1)]);
        let variant_b = variant(11, [PatternId::new(1), PatternId::new(2)]);
        let weights = WeightedPatternSet::uniform(4).expect("weights");

        let forward = setup_probability(family, [&variant_a, &variant_b], &weights);
        let reversed = setup_probability(family, [&variant_b, &variant_a], &weights);

        assert_eq!(forward, reversed);
    }
}

mod case_invariant_checkpoint_schedule_and_continuation_hint_are_label_contracts {
    use super::*;

    #[test]
    fn invariant_checkpoint_schedule_and_continuation_hint_are_label_contracts() {
        let schedule =
            CheckpointSchedule::for_opening_target(PcTarget::six_lines()).expect("schedule");
        let hint = ContinuationHint::for_remaining_queue(10);
        let bag_phase = BagPhaseClassifier::classify_standard_7(8);

        assert_eq!(schedule.label(), "6L");
        assert_eq!(
            schedule.partition_labels(),
            vec!["6", "2+4", "4+2", "2+2+2"]
        );
        assert_eq!(schedule.checkpoint_count(), 8);
        assert!(hint.is_available());
        assert_eq!(hint.next_target(), Some(PcTarget::four_lines()));
        assert_eq!(hint.next_label(), "4L");
        assert_eq!(hint.min_required_pieces(), 10);
        assert_eq!(bag_phase.bag_index(), 1);
        assert_eq!(bag_phase.offset(), 1);
    }
}

mod case_invariant_opening_and_scenario_are_chain_labels_not_solver_paths {
    use super::*;

    #[test]
    fn invariant_opening_and_scenario_are_chain_labels_not_solver_paths() {
        let two_line =
            CheckpointSchedule::for_opening_target(PcTarget::two_lines()).expect("2L schedule");
        let four_line =
            CheckpointSchedule::for_opening_target(PcTarget::four_lines()).expect("4L schedule");

        assert_eq!(
            ChainClassifier::opening(PcTarget::two_lines()).as_str(),
            "opening-2l"
        );
        assert_eq!(
            ChainClassifier::opening(PcTarget::four_lines()).as_str(),
            "opening-4l"
        );
        assert_eq!(ChainClassifier::scenario().as_str(), "scenario");
        assert_eq!(two_line.partition_labels(), vec!["2"]);
        assert_eq!(four_line.partition_labels(), vec!["4", "2+2"]);
    }
}

#[cfg(feature = "native-c-core")]
mod case_invariant_core_executor_uses_checkpoint_schedule_metadata_without_cache_fields {
    use super::*;

    #[test]
    fn invariant_core_executor_uses_checkpoint_schedule_metadata_without_cache_fields() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])),
        );
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("opening problem");
        let result = CoreExecutor::execute(&problem).expect("opening execution");
        let fields = result.summary_fields();

        assert!(fields.contains(&(
            "checkpoint_schedule_source".to_owned(),
            "clearra-pc-graph-labels".to_owned()
        )));
        assert!(fields.contains(&("checkpoint_schedule_partitions".to_owned(), "2".to_owned())));
        assert!(fields.contains(&(
            "checkpoint_schedule_checkpoint_count".to_owned(),
            "1".to_owned()
        )));
        assert!(fields.iter().all(|(key, _)| !key.contains("cache")));
        assert!(fields.iter().all(|(key, _)| !key.starts_with("frontier")));
    }
}

#[cfg(feature = "native-c-core")]
mod case_invariant_observed_opening_uses_same_schedule_metadata_without_cache_counters {
    use super::*;

    #[test]
    fn invariant_observed_opening_uses_same_schedule_metadata_without_cache_counters() {
        let query =
            OpeningPcSearchQuery::new(PcTarget::four_lines()).with_queue(PcQueueInput::observed(
                ObservedQueue::new(vec![PieceKind::I, PieceKind::O, PieceKind::T]),
            ));
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("opening problem");
        let result = CoreExecutor::execute(&problem).expect("opening execution");
        let fields = result.summary_fields();

        assert!(fields.contains(&(
            "checkpoint_schedule_source".to_owned(),
            "clearra-pc-graph-labels".to_owned()
        )));
        assert!(fields.contains(&(
            "checkpoint_schedule_partitions".to_owned(),
            "4|2+2".to_owned()
        )));
        assert!(fields.contains(&(
            "checkpoint_schedule_checkpoint_count".to_owned(),
            "3".to_owned()
        )));
        assert!(fields.contains(&("queue_mode".to_owned(), "observed".to_owned())));
        assert!(fields.iter().all(|(key, _)| !key.contains("cache")));
        assert!(fields.iter().all(|(key, _)| !key.starts_with("frontier")));
    }
}

#[cfg(feature = "native-c-core")]
mod case_invariant_scenario_service_keeps_full_queue_for_min_remaining_queue {
    use super::*;

    #[test]
    fn invariant_scenario_service_keeps_full_queue_for_min_remaining_queue() {
        let layout = Board64Layout::standard_10_by_lines(4).expect("layout");
        let registry = standard_tetromino_registry();
        let o_piece = registry.get(PieceKind::O).expect("O");
        let setup = PlacementMask::new(
            layout,
            o_piece,
            clearra_core_domain::piece::rotation::RotationState::Zero,
            0,
            0,
        )
        .expect("setup mask");
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, setup.mask()),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
                PieceKind::I,
                PieceKind::T,
                PieceKind::S,
                PieceKind::Z,
            ])),
            PieceWindow::new(4),
        )
        .with_allow_hold(false)
        .with_min_remaining_queue(4);

        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("scenario problem");
        let result = CoreExecutor::execute(&problem).expect("scenario execution");
        let fields = result.summary_fields();

        assert!(fields.contains(&("solution_found".to_owned(), "true".to_owned())));
        assert!(fields.contains(&("queue_len".to_owned(), "8".to_owned())));
        assert!(fields.contains(&("piece_window".to_owned(), "4".to_owned())));
        assert!(fields.contains(&("min_remaining_queue".to_owned(), "4".to_owned())));
    }
}

mod case_invariant_observed_supply_ambiguity_is_warning_not_error {
    use super::*;

    #[test]
    fn invariant_observed_supply_ambiguity_is_warning_not_error() {
        let report = validate_observed_queue(&ObservedQueue::new(vec![PieceKind::I, PieceKind::O]));

        assert!(!report.has_errors());
        assert!(report.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::WSupplyAmbiguousObservedWindow
                && diagnostic.severity() == DiagnosticSeverity::Warning
        }));
    }
}

mod case_invariant_build_template_defaults_to_standard_board_and_explicit_slot_geometry {
    use super::*;

    #[test]
    fn invariant_build_template_defaults_to_standard_board_and_explicit_slot_geometry() {
        let template = BuildTemplate::new(
            "workspace-build-template",
            vec![BuildSlot::new(
                BuildSlotId::new(1),
                vec![CellCoord::new_unchecked(0, 0)],
            )],
        );

        assert_eq!(template.board_size().width(), 10);
        assert_eq!(template.board_size().height(), 20);
        assert_eq!(
            template.slots()[0].cells()[0],
            CellCoord::new_unchecked(0, 0)
        );
    }
}

mod case_coverage_crate_does_not_depend_on_scoring_crate {
    use super::*;

    #[test]
    fn coverage_crate_does_not_depend_on_scoring_crate() {
        let root = workspace_root();
        let coverage_manifest =
            fs::read_to_string(root.join("crates/clearra-coverage/Cargo.toml")).expect("manifest");

        assert!(
            !active_manifest_lines(&coverage_manifest).any(|line| line.contains("clearra-scoring")),
            "clearra-coverage must use opaque core-domain ids, not clearra-scoring"
        );

        for source in rust_sources(&root.join("crates/clearra-coverage/src")) {
            let contents = fs::read_to_string(&source).expect("coverage source");
            assert!(
                !contents.contains("clearra_scoring"),
                "clearra-coverage source must not import clearra_scoring: {}",
                source.display()
            );
        }
    }
}

mod case_problem_crate_does_not_depend_on_scoring_implementation_crate {
    use super::*;

    #[test]
    fn problem_crate_does_not_depend_on_scoring_implementation_crate() {
        let root = workspace_root();
        let problem_manifest =
            fs::read_to_string(root.join("crates/clearra-problem/Cargo.toml")).expect("manifest");

        assert!(
            !active_manifest_lines(&problem_manifest).any(|line| line.contains("clearra-scoring")),
            "clearra-problem must own SpinTargetRequest without depending on clearra-scoring"
        );

        for source in rust_sources(&root.join("crates/clearra-problem/src")) {
            let contents = fs::read_to_string(&source).expect("problem source");
            assert!(
                !contents.contains("clearra_scoring"),
                "clearra-problem source must not import clearra_scoring: {}",
                source.display()
            );
        }
    }
}

mod case_security_fix_map_mentions_all_known_risks {
    use super::*;

    #[test]
    fn security_fix_map_mentions_all_known_risks() {
        let root = workspace_root();
        let security_map =
            fs::read_to_string(root.join("docs/security-fix-map.md")).expect("security fix map");

        for marker in [
            "security_fix_map_mentions_all_known_risks",
            "architecture_validation_rejects_silent_gpu_fallback",
            "architecture_validation_rejects_runtime_raw_svg",
            "architecture_validation_rejects_gui_subprocess",
            "architecture_validation_rejects_unbounded_ffi_pointer_count",
            "architecture_validation_rejects_unsafe_outside_core_ffi_raw",
            "SEC-C-MEM-001",
            "SEC-C-MEM-002",
            "SEC-C-MEM-003",
            "SEC-FFI-001",
            "SEC-FFI-002",
            "SEC-GPU-001",
            "SEC-GPU-002",
            "SEC-COV-001",
            "SEC-REN-001",
            "SEC-SVG-001",
            "SEC-GUI-001",
            "SEC-WASM-001",
        ] {
            assert!(
                security_map.contains(marker),
                "security fix map must mention {marker}"
            );
        }
    }
}

mod case_unsafe_allowed_only_in_core_ffi_raw {
    use super::*;

    #[test]
    fn unsafe_allowed_only_in_core_ffi_raw() {
        let validator =
            read_workspace_contract_surface("scripts/architecture/validate_unsafe_boundary.ps1");

        assert!(
            validator.contains("Test-RustUnsafeBoundaryAllowed"),
            "unsafe boundary validator must centralize its allowlist"
        );
        assert!(
            validator.contains("crates/clearra-core-ffi/src/raw/")
                && validator
                    .contains("crates/clearra-core-ffi/src/memory/native_memory_bindings.rs")
                && validator.contains("crates/clearra-platform-fs/src/linux.rs")
                && validator.contains("crates/clearra-platform-fs/src/windows.rs")
                && !validator.contains("crates/clearra-core-ffi/src/native/buildup.rs")
                && !validator.contains("crates/clearra-core-ffi/src/buildup/build_variant_view.rs"),
            "unsafe boundary allowlist must keep BuildUp allocation and pointer copies in raw and isolate platform filesystem ABI calls"
        );
        assert!(
            validator.contains("must not contain unsafe/raw pointer boundary code outside clearra-core-ffi raw/native binding allowlist"),
            "unsafe boundary validator must reject unsafe outside the allowlist"
        );
        assert!(
            validator.contains(
                r"\bunsafe\s*(?:\{|\(|(?:const\s+)?fn\b|impl\b|(?:auto\s+)?trait\b|extern\b|static\b|\|\|)",
            )
                && validator.contains("tie-snapshot-path-unsafe"),
            "unsafe boundary syntax detection must include Rust 2024 unsafe attributes without treating domain reason strings as code"
        );
    }
}
