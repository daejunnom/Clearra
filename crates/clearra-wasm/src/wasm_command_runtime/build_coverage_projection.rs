//! Allocation-admitted projection of nominal Build portfolios into the WASM DTO.
//! The producer remains the authority for membership, probability and page ownership.

use super::*;
use clearra_host_contract::{BuildCoverageCompletenessPayload, BuildCoveragePortfolioV2Payload};

pub(super) fn project(
    product: &clearra_app::ProductCapabilityResult,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<ProductResultPayload, WasmCommandRuntimeError> {
    let report = product
        .build_coverage_portfolio_v2()
        .ok_or_else(finite_projection_error)?;
    let owner = report
        .portfolio_alternative_owner()
        .ok_or_else(finite_projection_error)?;
    let completeness = report.completeness();
    let contract = try_owned_string(product.contract().as_str(), ledger)?;
    let result_kind = try_owned_string(product.result_kind().as_str(), ledger)?;
    let mut payload = BuildCoveragePortfolioV2Payload::try_new(
        try_owned_string(report.contract_id(), ledger)?,
        try_owned_string(report.objective().as_str(), ledger)?,
        try_owned_string(report.probability_basis(), ledger)?,
        try_decimal_u128(report.source_candidate_count() as u128, ledger)?,
        try_decimal_u128(report.selected_candidate_count() as u128, ledger)?,
        try_decimal_u128(report.pattern_count() as u128, ledger)?,
        try_decimal_u128(report.required_pattern_count() as u128, ledger)?,
        try_owned_string(report.union_probability(), ledger)?,
        try_owned_string(report.normalized_solution_set_hash(), ledger)?,
        try_owned_string(
            report
                .canonical_candidate_keys()
                .first()
                .map_or("", String::as_str),
            ledger,
        )?,
        BuildCoverageCompletenessPayload::new(
            completeness.source_universe_complete(),
            completeness.coverage_rows_complete(),
            completeness.probability_weights_complete(),
            completeness.exact_minimum_proven(),
            completeness.query_bound(),
        ),
        true,
        Some(try_owned_string(owner.set_identity_sha256(), ledger)?),
    )
    .map_err(|_| finite_projection_error())?;
    if product.contract() == ProductCapabilityContract::BuildPinnedMinimals {
        let pins = report.pinned_candidate_keys();
        let pinned = try_owned_string_vec(pins, ledger)?;
        let additional = copy_additional_keys(report.canonical_candidate_keys(), pins, ledger)?;
        payload = payload
            .with_pinned_selection(pinned, additional)
            .map_err(|_| finite_projection_error())?;
    }
    Ok(ProductResultPayload::from_owned_memory_authorized_parts(
        contract,
        result_kind,
        ProductResultPayloadContent::BuildCoveragePortfolioV2(payload),
    ))
}

// Count first without allocating a temporary filtered Vec or cloning rejected
// keys. Charge vector capacity and each retained String before allocation.
fn copy_additional_keys(
    selected: &[String],
    pins: &[String],
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<Vec<String>, WasmCommandRuntimeError> {
    let count = selected.iter().filter(|key| !pins.contains(key)).count();
    let slot_size = core::mem::size_of::<String>() as u128;
    ledger.authorize_requested(
        (count as u128)
            .checked_mul(slot_size)
            .ok_or_else(finite_projection_error)?,
    )?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(count)
        .map_err(|_| finite_allocation_error())?;
    let capacity = output.capacity();
    ledger.retain_actual(
        (capacity as u128)
            .checked_mul(slot_size)
            .ok_or_else(finite_projection_error)?,
    )?;
    if capacity < count {
        return Err(finite_projection_error());
    }
    for key in selected.iter().filter(|key| !pins.contains(key)) {
        output.push(try_owned_string(key, ledger)?);
        if output.capacity() != capacity {
            return Err(finite_projection_error());
        }
    }
    Ok(output)
}

pub(super) fn copy_payload(
    source: &BuildCoveragePortfolioV2Payload,
    ledger: &mut WasmFiniteMemoryLedger,
) -> Result<BuildCoveragePortfolioV2Payload, WasmCommandRuntimeError> {
    let mut payload = BuildCoveragePortfolioV2Payload::try_new(
        try_owned_string(source.contract(), ledger)?,
        try_owned_string(source.objective(), ledger)?,
        try_owned_string(source.probability_basis(), ledger)?,
        try_owned_string(source.source_candidate_count(), ledger)?,
        try_owned_string(source.selected_candidate_count(), ledger)?,
        try_owned_string(source.pattern_count(), ledger)?,
        try_owned_string(source.required_pattern_count(), ledger)?,
        try_owned_string(source.union_probability(), ledger)?,
        try_owned_string(source.normalized_solution_set_hash(), ledger)?,
        try_owned_string(source.canonical_first_candidate_id(), ledger)?,
        source.completeness(),
        source.page_source_available(),
        try_optional_owned_string(source.page_source_identity_sha256(), ledger)?,
    )
    .map_err(|_| finite_projection_error())?;
    if !source.pinned_candidate_keys().is_empty() {
        payload = payload
            .with_pinned_selection(
                try_owned_string_vec(source.pinned_candidate_keys(), ledger)?,
                try_owned_string_vec(source.additional_candidate_keys(), ledger)?,
            )
            .map_err(|_| finite_projection_error())?;
    } else if !source.additional_candidate_keys().is_empty() {
        return Err(finite_projection_error());
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_app::{encode_ctk3_compact, Ctk3Color, Ctk3Document, Ctk3Page, Ctk3Piece};

    fn selected_document(masks: &[u64]) -> String {
        let pages = masks
            .iter()
            .map(|mask| {
                Ctk3Page::new(
                    1,
                    (0..10)
                        .map(|x| {
                            if mask & (1_u64 << x) == 0 {
                                Ctk3Color::Empty
                            } else {
                                Ctk3Color::Piece(Ctk3Piece::I)
                            }
                        })
                        .collect(),
                )
            })
            .collect();
        encode_ctk3_compact(&Ctk3Document::new(10, pages)).unwrap()
    }

    #[test]
    fn real_build_portfolios_survive_finite_projection_at_exact_memory_boundaries() {
        let runtime = WasmCommandRuntime::default()
            .with_host_capabilities(WasmHostCapabilities::new(1, false, false));
        let source = runtime
            .run_command_text(
                "clearra build-probability --base-mask 0 --target-mask 0xf --height 4 \
             --queue I --no-hold --include-mirror --backend cpu --workers 1",
            )
            .unwrap();
        let hash = &source.search_report().unwrap().normalized_solution_set_hash;
        for masks in [vec![], vec![0x3c0], vec![0xf, 0x3c0]] {
            let suffix = if masks.is_empty() {
                String::new()
            } else {
                format!(" --required-format ctk3 --required-document {} --expected-source-set-hash {hash}",
                    selected_document(&masks))
            };
            let command = format!(
                "clearra build {} --base-mask 0 --target-mask 0xf --height 4 \
                 --queue I --no-hold --objective min-cover --queue-knowledge oracle \
                 --backend cpu --workers 1{suffix}",
                if masks.is_empty() {
                    "cover"
                } else {
                    "pinned-minimals"
                }
            );
            let request = runtime.compile_command_text(&command).unwrap();
            let response = runtime.app_context().run(request);
            let expected = response
                .to_host_response()
                .product_result_payload()
                .cloned()
                .unwrap();
            let live = (core::mem::size_of_val(&response) as u128)
                + response.checked_retained_capacity_bytes().unwrap();
            let mut measured = WasmFiniteMemoryLedger::new(
                live,
                u128::MAX,
                WasmFiniteConversionRoute::PublicDirect,
            )
            .unwrap();
            let actual = try_host_product_result_payload(&response, &mut measured)
                .unwrap()
                .unwrap();
            assert_eq!(actual, expected);
            assert_eq!(
                measured.target_heap_bytes(),
                actual.checked_retained_capacity_bytes().unwrap()
            );
            let peak = live
                + core::mem::size_of::<WasmExecutionResult>() as u128
                + measured.target_heap_bytes();
            for (limit, succeeds) in [(peak, true), (peak - 1, false)] {
                let mut ledger = WasmFiniteMemoryLedger::new(
                    live,
                    limit,
                    WasmFiniteConversionRoute::PublicDirect,
                )
                .unwrap();
                let result = try_host_product_result_payload(&response, &mut ledger);
                if succeeds {
                    assert_eq!(result.unwrap(), Some(expected.clone()));
                } else {
                    assert_eq!(result.unwrap_err().code(), WASM_FINITE_MEMORY_LIMIT);
                }
            }
            // The public DTO-copy path must preserve the same selection too.
            let mut ledger = WasmFiniteMemoryLedger::new(
                live,
                u128::MAX,
                WasmFiniteConversionRoute::PublicDirect,
            )
            .unwrap();
            let copied = try_clone_public_product_result_payload(&expected, &mut ledger).unwrap();
            assert_eq!(copied, expected);
            assert_eq!(
                ledger.target_heap_bytes(),
                copied.checked_retained_capacity_bytes().unwrap()
            );
        }
    }

    #[test]
    fn additional_members_are_allocated_and_preserved_without_reconstructing_coverage() {
        let source = BuildCoveragePortfolioV2Payload::try_new(
            "build-coverage-portfolio.v2",
            "min-cover",
            "exact-union",
            "3",
            "2",
            "2",
            "2",
            "1",
            "cts1:0123456789abcdef",
            "first",
            BuildCoverageCompletenessPayload::new(true, true, true, true, true),
            true,
            Some("a".repeat(64)),
        )
        .unwrap()
        .with_pinned_selection(vec!["first".into()], vec!["second".into()])
        .unwrap();
        let live = core::mem::size_of_val(&source) as u128
            + source.checked_retained_capacity_bytes().unwrap();
        let mut ledger =
            WasmFiniteMemoryLedger::new(live, u128::MAX, WasmFiniteConversionRoute::PublicDirect)
                .unwrap();
        let actual = copy_payload(&source, &mut ledger).unwrap();
        assert_eq!(actual, source);
        assert_eq!(
            ledger.target_heap_bytes(),
            actual.checked_retained_capacity_bytes().unwrap()
        );
        let peak =
            live + core::mem::size_of::<WasmExecutionResult>() as u128 + ledger.target_heap_bytes();
        for (limit, succeeds) in [(peak, true), (peak - 1, false)] {
            let mut ledger =
                WasmFiniteMemoryLedger::new(live, limit, WasmFiniteConversionRoute::PublicDirect)
                    .unwrap();
            let result = copy_payload(&source, &mut ledger);
            if succeeds {
                assert_eq!(result.unwrap(), source);
            } else {
                assert_eq!(result.unwrap_err().code(), WASM_FINITE_MEMORY_LIMIT);
            }
        }
    }
}
