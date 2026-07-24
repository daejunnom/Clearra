use clearra_app::AppCommand;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_forward_search::{ForwardSearchMode, ForwardSpinCategory};
use clearra_pc_graph::request::SupplyWindowSize;
use clearra_scoring::profile::SpinProfileId;

use super::*;

#[test]
fn wasm_command_compiles_to_app_request() {
    let request = WebCommandParser::parse("clearra pc --lines 2 --backend cpu")
        .expect("web command")
        .to_app_request()
        .expect("AppRequest");

    match request.command() {
        AppCommand::Pc(command) => {
            assert_eq!(command.query().target().lines(), 2);
            assert_eq!(command.query().execution_policy().backend().as_str(), "cpu");
        }
        _ => panic!("expected AppCommand::Pc"),
    }
}

#[test]
fn opening_pc_command_preserves_observed_source_piece_count() {
    let request = WebCommandParser::parse("clearra pc --lines 4 --backend cpu --source-pieces 10")
        .expect("web command")
        .to_app_request()
        .expect("AppRequest");

    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };
    assert_eq!(
        command.query().supply_window_size(),
        Some(SupplyWindowSize::new(10))
    );
}

#[test]
fn pc_back_to_back_preservation_is_an_execution_constraint_not_a_score_request() {
    let request = WebCommandParser::parse(
        "clearra pc --lines 2 --queue IOTSL --preserve-b2b --spin-profile all-mini-plus",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };
    let objective = command.query().objective();
    assert!(!objective.score().requested());
    assert!(objective.execution_constraints().preserves_back_to_back());
    assert_eq!(
        objective.execution_constraints().spin_profile().as_str(),
        "all-mini-plus"
    );
}

#[test]
fn pc_score_table_selection_is_independent_from_rotation_rule() {
    let request = WebCommandParser::parse(
        "clearra pc --lines 2 --queue IOTSL --score --score-profile jstris-ultra --rule srs-plus",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };
    assert_eq!(
        command.query().objective().score().profile().as_str(),
        "jstris-ultra"
    );
    assert_eq!(command.query().rule().id().as_str(), "srs-plus");
}

#[test]
fn build_probability_back_to_back_preservation_reaches_the_core_query() {
    let request = WebCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror --preserve-b2b --spin-profile t-spins-plus",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::BuildProbability(command) = request.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    let objective = command.query().core_query().objective();
    assert!(!objective.score().requested());
    assert!(objective.execution_constraints().preserves_back_to_back());
    assert_eq!(
        objective.execution_constraints().spin_profile().as_str(),
        "t-spins-plus"
    );
}

#[test]
fn build_probability_preserves_rule_spin_profile_and_initial_hold() {
    let request = WebCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --hold T --no-mirror --aggregate spin --rule srs-x --spin-profile all-mini-plus",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::BuildProbability(command) = request.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    assert_eq!(
        command.query().core_query().rule().id(),
        clearra_rules::profile::rule_profile::RuleProfileId::SrsX
    );
    assert_eq!(
        command.query().core_query().hold_state().piece(),
        Some(PieceKind::T)
    );
    assert!(command.query().core_query().allow_hold());
    assert_eq!(
        command
            .query()
            .aggregation()
            .spin_profile()
            .expect("spin aggregation")
            .as_str(),
        "all-mini-plus"
    );
}

#[test]
fn browser_command_accepts_completed_group_followed_by_bag_permutation() {
    WebCommandParser::parse(
        "clearra pc --lines 2 --backend cpu --patterns [^TSZ]!P4 --max-patterns 20160",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");
}

#[test]
fn scenario_initial_hold_remains_independent_from_p7_queue() {
    let request = WebCommandParser::parse(
        "clearra pc --lines 4 --board-mask 0x80787 --height 4 --pieces 8 --patterns P7 --hold S --backend cpu",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Scenario(command) = request.command() else {
        panic!("expected AppCommand::Scenario");
    };
    assert_eq!(command.query().hold_state().piece(), Some(PieceKind::S));
    assert!(matches!(
        command.query().remaining_queue(),
        clearra_pc_graph::request::PcQueueInput::Standard7Bag
    ));
    assert_eq!(
        command.query().supply_window_size(),
        Some(SupplyWindowSize::new(7))
    );
}

#[test]
fn web_command_preserves_cpu_pool_options_in_the_typed_request() {
    let request = WebCommandParser::parse(
        "clearra pc --lines 2 --backend cpu --workers 1 --use-all-cpu-threads --cpu-warmup",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };
    let policy = command.query().execution_policy();
    assert_eq!(policy.workers(), 1);
    assert!(policy.use_all_logical_processors());
    assert!(policy.cpu_warmup());
}

#[test]
fn web_command_preserves_gpu_device_and_warmup_in_the_typed_request() {
    let request = WebCommandParser::parse(
        "clearra pc --lines 2 --backend gpu --gpu-device 3 --gpu-warmup --allow-backend-fallback",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };
    let policy = command.query().execution_policy();
    assert_eq!(policy.gpu_device().as_display_string(), "3");
    assert!(policy.gpu_warmup());
    assert!(policy.allow_backend_fallback());
}

#[test]
fn web_command_requires_opt_in_for_the_reserved_logical_processor() {
    let hardware = clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit();
    if hardware <= 1 {
        return;
    }
    let command = format!("clearra pc --lines 2 --workers {hardware}");
    assert_eq!(
        WebCommandParser::parse(&command)
            .expect_err("reserved processor requires opt-in")
            .code(),
        WebCommandErrorCode::InvalidValue
    );
    WebCommandParser::parse(&format!("{command} --use-all-cpu-threads"))
        .expect("explicit all-CPU command");
}

#[test]
fn wasm_runtime_does_not_use_native_path_semantics() {
    let error = WebCommandParser::parse("clearra pc --input C:\\temp\\field.txt")
        .expect_err("native path is rejected");

    assert_eq!(error.code(), WebCommandErrorCode::NativePathSemantics);
}

#[test]
fn wasm_runtime_does_not_spawn_process() {
    let error = WebCommandParser::parse("clearra pc --lines 2 | clearra verify")
        .expect_err("process syntax is rejected");

    assert_eq!(error.code(), WebCommandErrorCode::ProcessSemantics);
}

#[test]
fn browser_file_input_uses_virtual_file_handle() {
    let handle = WebVirtualFileHandle::new("field-json", "field.json", "application/json", 128)
        .expect("virtual handle");

    assert_eq!(handle.origin_kind(), "browser-file-input");
    assert_eq!(handle.display_name(), "field.json");
}

#[test]
fn virtual_file_handle_rejects_native_paths() {
    let error = WebVirtualFileHandle::new("bad", "C:\\secret\\field.json", "text/plain", 8)
        .expect_err("native paths forbidden");

    assert_eq!(error.code(), WebCommandErrorCode::NativePathSemantics);
}

#[test]
fn damage_command_compiles_to_typed_forward_search() {
    let request = WebCommandParser::parse(
        "clearra damage --board-mask 0x0 --height 4 --queue TI --no-hold --rule srs-plus --spin-profile t-spins --initial-b2b 2",
    )
    .expect("damage command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Damage(command) = request.command() else {
        panic!("expected AppCommand::Damage");
    };
    assert_eq!(
        command.query().piece_source().fixed_sequence(),
        Some(&[PieceKind::T, PieceKind::I][..])
    );
    assert!(!command.query().hold_enabled());
    assert_eq!(
        command.query().rule_profile(),
        clearra_rules::profile::rule_profile::RuleProfileId::SrsPlus
    );
    assert_eq!(command.query().spin_profile(), SpinProfileId::TSpins);
    assert_eq!(command.query().initial_back_to_back(), Some(1));
    assert_eq!(command.query().mode(), ForwardSearchMode::MaximumDamage);
}

#[test]
fn damage_command_preserves_minimum_damage_enumeration_policy() {
    let request = WebCommandParser::parse(
        "clearra damage --board-mask 0x0 --height 4 --queue TI --minimum-damage 6",
    )
    .expect("damage threshold command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Damage(command) = request.command() else {
        panic!("expected AppCommand::Damage");
    };
    assert_eq!(command.query().mode(), ForwardSearchMode::DamageAtLeast(6));
}

#[test]
fn spin_finder_command_preserves_profile_and_target_group() {
    let request = WebCommandParser::parse(
        "clearra spin-finder --board-mask 0x0 --height 8 --queue J --hold --rule srs-x --spin-profile all-mini-plus --lines 2 --spin-category other",
    )
    .expect("spin command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::SpinFinder(command) = request.command() else {
        panic!("expected AppCommand::SpinFinder");
    };
    assert!(command.query().hold_enabled());
    assert_eq!(
        command.query().rule_profile(),
        clearra_rules::profile::rule_profile::RuleProfileId::SrsX
    );
    assert_eq!(command.query().spin_profile(), SpinProfileId::AllMiniPlus);
    let ForwardSearchMode::SpinFinder(target) = command.query().mode() else {
        panic!("expected spin-finder mode");
    };
    assert_eq!(target.lines(), Some(2));
    assert_eq!(target.category(), ForwardSpinCategory::Other);
}

#[test]
fn spin_finder_accepts_eight_piece_pattern_but_damage_remains_fixed_queue_only() {
    let request = WebCommandParser::parse(
        "clearra spin-finder --patterns P7P1 --spin-profile t-spins --lines any",
    )
    .expect("eight-piece pattern");
    let query = request.forward_search_query().expect("forward query");
    assert_eq!(query.height(), 8);
    assert!(query.piece_source().is_pattern());
    assert_eq!(query.piece_source().sequence_len(), 8);

    let too_long =
        WebCommandParser::parse("clearra spin-finder --patterns IOTSZLJIO --spin-profile t-spins")
            .expect_err("nine-piece pattern must be rejected");
    assert_eq!(too_long.code(), WebCommandErrorCode::InvalidValue);

    let damage_pattern = WebCommandParser::parse("clearra damage --patterns [TI]")
        .expect_err("damage pattern must be rejected");
    assert_eq!(
        damage_pattern.code(),
        WebCommandErrorCode::UnsupportedCommand
    );
}
