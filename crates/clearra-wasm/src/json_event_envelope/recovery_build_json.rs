//! Exact JSON projection of the paired-Build DTO.
use super::*;
use clearra_host_contract::{RecoveryBuildPayload,RecoveryBuildExamplePayload,RecoveryBuildStepPayload};

pub(super) fn write_step(object: &mut JsonObject<'_>, source: &RecoveryBuildStepPayload) {
    object.string("source_index",&source.source_index);
    object.boolean("result_target",source.result_target);
    object.string("piece",&source.piece);
    object.number("rotation",source.rotation);
    object.number("x",source.x);
    object.number("y",source.y);
    object.string("hold_decision",&source.hold_decision);
    object.string("board_before_mask",&source.board_before_mask);
    object.string("placement_mask",&source.placement_mask);
    object.string("board_after_mask",&source.board_after_mask);
    object.number("cleared_rows",source.cleared_rows);
    object.number("cleared_lines",source.cleared_lines);
    object.boolean("recognized_spin",source.recognized_spin);
    object.boolean("b2b_active",source.b2b_active);
    object.boolean("middle_complete",source.middle_complete);
}

pub(super) fn write_example(object: &mut JsonObject<'_>, source: &RecoveryBuildExamplePayload) {
    object.string("first_pattern",&source.first_pattern);
    object.string("second_pattern",&source.second_pattern);
    object.string("first_queue",&source.first_queue);
    object.string("second_queue",&source.second_queue);
    object.string("status",&source.status);
    object.string("terminal_board_mask",&source.terminal_board_mask);
    object.string("effective_max_early",&source.effective_max_early);
    object.string("actual_early",&source.actual_early);
    object.array("exchange_balance",|output| write_number_array(output,&source.exchange_balance));
    object.array("steps",|output| write_object_array(output,&source.steps,write_step));
}

pub(super) fn write_payload(object: &mut JsonObject<'_>, source: &RecoveryBuildPayload) {
    object.string("input_identity",&source.input_identity);
    object.number("height",source.height);
    object.string("start_board_mask",&source.start_board_mask);
    object.string("middle_target_mask",&source.middle_target_mask);
    object.string("result_target_mask",&source.result_target_mask);
    object.string("first_supply",&source.first_supply);
    object.string("second_supply",&source.second_supply);
    object.optional_string("early_limit",source.early_limit.as_deref());
    object.boolean("allow_piece_exchange",source.allow_piece_exchange);
    object.boolean("hold_enabled",source.hold_enabled);
    object.boolean("preserve_b2b",source.preserve_b2b);
    object.boolean("initial_b2b",source.initial_b2b);
    object.string("rule_profile",&source.rule_profile);
    object.string("spin_profile",&source.spin_profile);
    object.boolean("complete",source.complete);
    object.string("pattern_count",&source.pattern_count);
    object.string("evaluated_pattern_count",&source.evaluated_pattern_count);
    object.string("normal_count",&source.normal_count);
    object.string("recovery_count",&source.recovery_count);
    object.string("no_path_count",&source.no_path_count);
    object.string("state_count",&source.state_count);
    object.string("normal_probability",&source.normal_probability);
    object.string("recovery_probability",&source.recovery_probability);
    object.string("no_path_probability",&source.no_path_probability);
    object.boolean("all_paths_enumerated",source.all_paths_enumerated);
    object.array("examples",|output| write_object_array(output,&source.examples,write_example));
}
fn write_number_array(output: &mut JsonSink, values: &[i16]) {
    output.push('[');
    for (index, value) in values.iter().enumerate() {
        if index != 0 { output.push(','); }
        let _ = write!(output,"{value}");
    }
    output.push(']');
}
