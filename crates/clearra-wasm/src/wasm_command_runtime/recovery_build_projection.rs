//! Allocation-admitted copy of the paired-Build response (no geometry recomputation).
use super::*;
use clearra_host_contract::{RecoveryBuildPayload,RecoveryBuildExamplePayload,RecoveryBuildStepPayload};

pub(super) fn copy_step(source: &RecoveryBuildStepPayload, ledger: &mut WasmFiniteMemoryLedger) -> Result<RecoveryBuildStepPayload,WasmCommandRuntimeError> {
    Ok(RecoveryBuildStepPayload {
        source_index: try_owned_string(&source.source_index,ledger)?,
        result_target: source.result_target,
        piece: try_owned_string(&source.piece,ledger)?,
        rotation: source.rotation,
        x: source.x,
        y: source.y,
        hold_decision: try_owned_string(&source.hold_decision,ledger)?,
        board_before_mask: try_owned_string(&source.board_before_mask,ledger)?,
        placement_mask: try_owned_string(&source.placement_mask,ledger)?,
        board_after_mask: try_owned_string(&source.board_after_mask,ledger)?,
        cleared_rows: source.cleared_rows,
        cleared_lines: source.cleared_lines,
        recognized_spin: source.recognized_spin,
        b2b_active: source.b2b_active,
        middle_complete: source.middle_complete,
    })
}

pub(super) fn copy_example(source: &RecoveryBuildExamplePayload, ledger: &mut WasmFiniteMemoryLedger) -> Result<RecoveryBuildExamplePayload,WasmCommandRuntimeError> {
    Ok(RecoveryBuildExamplePayload {
        first_pattern: try_owned_string(&source.first_pattern,ledger)?,
        second_pattern: try_owned_string(&source.second_pattern,ledger)?,
        first_queue: try_owned_string(&source.first_queue,ledger)?,
        second_queue: try_owned_string(&source.second_queue,ledger)?,
        status: try_owned_string(&source.status,ledger)?,
        terminal_board_mask: try_owned_string(&source.terminal_board_mask,ledger)?,
        effective_max_early: try_owned_string(&source.effective_max_early,ledger)?,
        actual_early: try_owned_string(&source.actual_early,ledger)?,
        exchange_balance: try_owned_vec(&source.exchange_balance,ledger,|value, _| Ok(*value))?,
        steps: try_owned_vec(&source.steps,ledger,copy_step)?,
    })
}

pub(super) fn copy_payload(source: &RecoveryBuildPayload, ledger: &mut WasmFiniteMemoryLedger) -> Result<RecoveryBuildPayload,WasmCommandRuntimeError> {
    Ok(RecoveryBuildPayload {
        input_identity: try_owned_string(&source.input_identity,ledger)?,
        height: source.height,
        start_board_mask: try_owned_string(&source.start_board_mask,ledger)?,
        middle_target_mask: try_owned_string(&source.middle_target_mask,ledger)?,
        result_target_mask: try_owned_string(&source.result_target_mask,ledger)?,
        first_supply: try_owned_string(&source.first_supply,ledger)?,
        second_supply: try_owned_string(&source.second_supply,ledger)?,
        early_limit: try_optional_owned_string(source.early_limit.as_deref(),ledger)?,
        allow_piece_exchange: source.allow_piece_exchange,
        hold_enabled: source.hold_enabled,
        preserve_b2b: source.preserve_b2b,
        initial_b2b: source.initial_b2b,
        rule_profile: try_owned_string(&source.rule_profile,ledger)?,
        spin_profile: try_owned_string(&source.spin_profile,ledger)?,
        complete: source.complete,
        pattern_count: try_owned_string(&source.pattern_count,ledger)?,
        evaluated_pattern_count: try_owned_string(&source.evaluated_pattern_count,ledger)?,
        normal_count: try_owned_string(&source.normal_count,ledger)?,
        recovery_count: try_owned_string(&source.recovery_count,ledger)?,
        no_path_count: try_owned_string(&source.no_path_count,ledger)?,
        state_count: try_owned_string(&source.state_count,ledger)?,
        normal_probability: try_owned_string(&source.normal_probability,ledger)?,
        recovery_probability: try_owned_string(&source.recovery_probability,ledger)?,
        no_path_probability: try_owned_string(&source.no_path_probability,ledger)?,
        all_paths_enumerated: source.all_paths_enumerated,
        examples: try_owned_vec(&source.examples,ledger,copy_example)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_build_exact_memory_boundary_preserves_executed_inputs_and_examples() {
        let runtime=WasmCommandRuntime::default().with_host_capabilities(WasmHostCapabilities::new(1,false,false));
        let result=runtime.run_command_text("clearra recovery build --start-mask 0 --middle-mask 0xf --result-mask 0xc030 --height 8 --first-supply O --second-supply I --allow-piece-exchange --no-hold").unwrap();
        let ProductResultPayloadContent::RecoveryBuild(payload)=result.app_response().product_result_payload().unwrap().content() else {panic!("typed paired build")};
        assert_eq!(payload.recovery_count,"1");
        let live=4096_u128;
        let mut measured=WasmFiniteMemoryLedger::new(live,u128::MAX,WasmFiniteConversionRoute::PublicDirect).unwrap();
        let copy=copy_payload(payload,&mut measured).unwrap();
        assert_eq!(&copy,payload);
        assert_eq!(measured.target_heap_bytes(),copy.checked_retained_capacity_bytes().unwrap());
        let peak=live+core::mem::size_of::<WasmExecutionResult>() as u128+measured.target_heap_bytes();
        for (limit,success) in [(peak,true),(peak-1,false)] {
            let mut ledger=WasmFiniteMemoryLedger::new(live,limit,WasmFiniteConversionRoute::PublicDirect).unwrap();
            let copied=copy_payload(payload,&mut ledger);
            if success {assert_eq!(&copied.unwrap(),payload);} else {assert_eq!(copied.unwrap_err().code(),WASM_FINITE_MEMORY_LIMIT);}
        }
    }
}
