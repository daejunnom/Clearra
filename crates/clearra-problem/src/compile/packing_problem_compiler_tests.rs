use clearra_core_domain::{board::board_size::BoardSize, pc::pc_target::PcTarget};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};

use super::*;
use crate::{
    compile::problem_compiler::ProblemCompiler,
    query::{BuildProblemLimits, BuildQuery, BuildTemplateBridge},
};

#[test]
fn opening_problem_compiles_to_packing_spec() {
    let problem =
        ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(PcTarget::two_lines()))
            .expect("problem");
    let spec = PackingProblemCompiler::compile(&problem).expect("packing");

    assert_eq!(spec.kind(), PackingProblemKind::OpeningPc);
    assert_eq!(spec.kind().as_u32(), 1);
    assert_eq!(spec.max_pieces(), 5);
}

#[test]
fn scenario_problem_compiles_to_packing_spec() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(3, 0),
        Default::default(),
        PieceWindow::new(6),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    let spec = PackingProblemCompiler::compile(&problem).expect("packing");

    assert_eq!(spec.kind(), PackingProblemKind::ScenarioPc);
    assert_eq!(spec.max_pieces(), 6);
}

#[test]
fn build_bridge_problem_compiles_to_packing_spec_without_build_coverage_dependency() {
    let query = BuildQuery::coverage_bridge(
        BuildTemplateBridge::new("template", BoardSize::new(10, 4).expect("board"), 2),
        8,
        BuildProblemLimits::new(16, 8),
    );
    let problem = ProblemCompiler::compile_build(&query).expect("problem");
    let spec = PackingProblemCompiler::compile(&problem).expect("packing");

    assert_eq!(spec.kind(), PackingProblemKind::Build);
    assert_eq!(spec.max_pieces(), 2);
}
