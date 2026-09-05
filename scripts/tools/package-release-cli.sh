#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT_DIR="${1:?usage: package-release-cli.sh OUTPUT_DIR VERSION [TARGET_TRIPLE]}"
VERSION="${2:?usage: package-release-cli.sh OUTPUT_DIR VERSION [TARGET_TRIPLE]}"
TARGET_TRIPLE="${3:-x86_64-unknown-linux-gnu}"

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.-]+)?$ ]]; then
    printf 'invalid release version: %s\n' "$VERSION" >&2
    exit 2
fi
if [[ "$TARGET_TRIPLE" != "x86_64-unknown-linux-gnu" ]]; then
    printf 'unsupported CLI release target: %s\n' "$TARGET_TRIPLE" >&2
    exit 2
fi
if [[ ! "${CLEARRA_SOURCE_COMMIT:-}" =~ ^[0-9a-f]{40}$ ]] ||
   [[ ! "${CLEARRA_ENGINE_BUILD_ID:-}" =~ ^[0-9a-f]{40}$ ]] ||
   [[ "$CLEARRA_ENGINE_BUILD_ID" != "$CLEARRA_SOURCE_COMMIT" ]]; then
    printf 'release CLI requires identical full lowercase source and engine commit IDs\n' >&2
    exit 2
fi

BUILD_ROOT="${CLEARRA_RELEASE_BUILD_ROOT:-${RUNNER_TEMP:-${TMPDIR:-/tmp}}/clearra-release}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$BUILD_ROOT/cargo-target}"

mkdir -p "$CARGO_TARGET_DIR" "$OUTPUT_DIR"
cargo build \
    --manifest-path "$ROOT/Cargo.toml" \
    --locked \
    --release \
    --package clearra-cli \
    --features wasm-cpu-runtime,webgpu-search

BINARY="$CARGO_TARGET_DIR/release/clearra"
if [[ ! -x "$BINARY" ]]; then
    printf 'clearra release binary was not produced: %s\n' "$BINARY" >&2
    exit 2
fi
RELEASE_BINARY="$OUTPUT_DIR/Clearra-CLI-v${VERSION}-linux-x86_64"
install -m 0755 "$BINARY" "$RELEASE_BINARY"

if ! command -v node >/dev/null 2>&1; then
    printf 'node is required to validate release CLI JSON smoke output\n' >&2
    exit 2
fi

run_json_smoke() {
    local name="${1:?smoke name is required}"
    local expected_summary_json="${2:?expected summary JSON is required}"
    shift 2

    local json
    if ! json="$("$RELEASE_BINARY" "$@")"; then
        printf 'Clearra CLI %s smoke failed\n' "$name" >&2
        exit 2
    fi
    if ! printf '%s' "$json" | \
        CLEARRA_SMOKE_NAME="$name" \
        CLEARRA_EXPECTED_SUMMARY_JSON="$expected_summary_json" \
        node -e '
            const fs = require("node:fs");
            const name = process.env.CLEARRA_SMOKE_NAME;
            const expected = JSON.parse(process.env.CLEARRA_EXPECTED_SUMMARY_JSON);
            const structured = Object.hasOwn(expected, "summary") ||
                Object.hasOwn(expected, "resource_report");
            const expectedSummary = structured ? (expected.summary ?? {}) : expected;
            const expectedResourceReport = structured ? (expected.resource_report ?? {}) : {};
            const raw = fs.readFileSync(0, "utf8");
            if (raw.trim().length === 0) {
                throw new Error(`Clearra CLI ${name} smoke returned empty JSON`);
            }
            const parsed = JSON.parse(raw);
            const expectedCommit = process.env.CLEARRA_SOURCE_COMMIT;
            const identity = parsed?.runtime_identity;
            if (identity?.source_commit !== expectedCommit ||
                identity?.engine_build_id !== expectedCommit ||
                identity?.contract_schema_version !== "clearra.search.contract.v2" ||
                identity?.supply_semantics_id !==
                    "clearra.supply.projected-terminal-lookahead.v1" ||
                identity?.artifact_schema_version !== "clearra.solution-data.v1") {
                throw new Error(`Clearra CLI ${name} smoke returned a mismatched product build identity`);
            }
            if (Object.hasOwn(expected, "kind") && parsed?.kind !== expected.kind) {
                throw new Error(
                    `Clearra CLI ${name} smoke expected kind=${JSON.stringify(expected.kind)}, ` +
                    `received ${JSON.stringify(parsed?.kind)}`
                );
            }
            if (Object.hasOwn(expected, "command_kind") &&
                parsed?.contract?.command?.kind !== expected.command_kind) {
                throw new Error(
                    `Clearra CLI ${name} smoke expected contract.command.kind=` +
                    `${JSON.stringify(expected.command_kind)}, received ` +
                    `${JSON.stringify(parsed?.contract?.command?.kind)}`
                );
            }
            if (Object.hasOwn(expected, "tiling_family_complete") &&
                parsed?.contract?.pc?.tiling?.family_complete !==
                    expected.tiling_family_complete) {
                throw new Error(
                    `Clearra CLI ${name} smoke expected contract.pc.tiling.family_complete=` +
                    `${JSON.stringify(expected.tiling_family_complete)}, received ` +
                    `${JSON.stringify(parsed?.contract?.pc?.tiling?.family_complete)}`
                );
            }
            if (Object.hasOwn(expected, "tiling_family_incomplete_reason") &&
                parsed?.contract?.pc?.tiling?.family_incomplete_reason !==
                    expected.tiling_family_incomplete_reason) {
                throw new Error(
                    `Clearra CLI ${name} smoke expected ` +
                    `contract.pc.tiling.family_incomplete_reason=` +
                    `${JSON.stringify(expected.tiling_family_incomplete_reason)}, received ` +
                    `${JSON.stringify(parsed?.contract?.pc?.tiling?.family_incomplete_reason)}`
                );
            }
            for (const [key, value] of Object.entries(expectedSummary)) {
                if (parsed?.summary?.[key] !== value) {
                    throw new Error(
                        `Clearra CLI ${name} smoke expected summary.${key}=${JSON.stringify(value)}, ` +
                        `received ${JSON.stringify(parsed?.summary?.[key])}`
                    );
                }
            }
            for (const [key, value] of Object.entries(expectedResourceReport)) {
                if (parsed?.resource_report?.[key] !== value) {
                    throw new Error(
                        `Clearra CLI ${name} smoke expected resource_report.${key}=${JSON.stringify(value)}, ` +
                        `received ${JSON.stringify(parsed?.resource_report?.[key])}`
                    );
                }
            }
            if (name === "rules-export-srs-x") {
                const embedded = JSON.parse(parsed?.summary?.json ?? "null");
                const halfTurns = new Set(["0:2", "2:0", "R:L", "L:R"]);
                const halfTurnCount = Array.isArray(embedded?.entries)
                    ? embedded.entries.filter((entry) =>
                        halfTurns.has(`${entry?.from}:${entry?.to}`)).length
                    : 0;
                if (embedded?.id !== "srs-x" || embedded?.source_rule !== "srs-x" ||
                    embedded?.entries?.length !== 84 || halfTurnCount !== 28) {
                    throw new Error(
                        "Clearra CLI SRS-X export did not preserve its canonical 84-transition profile"
                    );
                }
            }
        '
    then
        printf 'Clearra CLI %s smoke returned invalid or unexpected JSON\n' "$name" >&2
        exit 2
    fi
    if [[ "$name" == "terminal-supply-p0" ]] && ! printf '%s' "$json" | \
        node "$ROOT/scripts/tools/validate-release-cli-smokes.mjs" \
            --validate-terminal-supply-json \
            --expected-source-commit "$CLEARRA_SOURCE_COMMIT"
    then
        printf 'Clearra CLI %s smoke violated the terminal-supply product contract\n' "$name" >&2
        exit 2
    fi
    if [[ "$name" == "pc-score-minimals" ]] && ! printf '%s' "$json" | \
        node "$ROOT/scripts/tools/validate-release-cli-smokes.mjs" \
            --validate-discord-score-minimals-json
    then
        printf 'Clearra CLI %s smoke violated the Discord canonical projection contract\n' "$name" >&2
        exit 2
    fi
    case "$name" in
        pc-path|pc-minimals|pc-score|pc-score-finder|pc-saves|pc-best-save|forward-ren)
            if ! printf '%s' "$json" | \
                node "$ROOT/scripts/tools/validate-release-cli-smokes.mjs" \
                    --validate-discord-canonical-result-json "$name"
            then
                printf 'Clearra CLI %s smoke violated the Discord core-owned canonical result contract\n' "$name" >&2
                exit 2
            fi
            ;;
    esac
}

run_json_smoke rules '{}' \
    --format json rules list
run_json_smoke rules-export-srs-x \
    '{"action":"export","profile":"srs-x"}' \
    --format json rules export --profile srs-x
run_json_smoke solver '{}' \
    --format json pc --lines 2 --queue IJLOO --fixed --no-hold
run_json_smoke pc-tiling \
    '{"kind":"pc-tiling-family.v1","command_kind":"pc-tiling-family.v1","tiling_family_complete":true,"tiling_family_incomplete_reason":"none","summary":{"coverage_probability":"not-calculated","probability_calculated":false,"probability_complete":false,"supply_probability_complete":false,"resource_probability_complete":false},"resource_report":{"truncated":false,"truncation_reason":null,"probability_complete":false}}' \
    --format json pc tiling --lines 2 --queue IIOOO --no-hold \
    --backend cpu --workers 1
run_json_smoke failed-queue '{}' \
    --format json failed-queue --lines 2 --patterns P5 --backend cpu --failed-count 7
run_json_smoke build-probability \
    '{"actual_backend":"wasm-cpu-build-probability","probability_calculated":true,"solution_found":true}' \
    --format json build-probability \
    --base-mask 0x0 --target-mask 0xf --height 4 \
    --queue I --no-hold --no-mirror --backend cpu --workers 1
run_json_smoke pc-srs-x \
    '{"rule_profile":"srs-x","effective_kick_model":"srs-x","solution_found":true}' \
    --format json pc --lines 2 --queue IIOOO --fixed --no-hold \
    --rule srs-x --backend cpu --workers 1
run_json_smoke pc-path \
    '{"kind":"pc-path-family.v2","command_kind":"pc-path-family.v2","summary":{"capability_id":"pc.path","canonical_selection":"smallest-canonical-candidate-id","complete":true}}' \
    --format json pc path \
    --board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --queue I
run_json_smoke pc-minimals \
    '{"kind":"pc-minimum-cover.v2","command_kind":"pc-minimum-cover.v2","summary":{"capability_id":"pc.minimals","canonical_selection":"smallest-canonical-candidate-id","alternative_index":"1","member_page_number":"1"}}' \
    --format json pc minimals \
    --board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --queue I
run_json_smoke pc-score \
    '{"kind":"pc-score-summary.v2","command_kind":"pc-score-summary.v2","summary":{"capability_id":"pc.score","payload_kind":"pc-score-field-summary","score_solution_field_contract":"pc-score-solution-field-average.v1","score_solution_field_average_basis":"whole-materialized-pattern-universe-failed-pc-zero","score_overall_basis":"all-materialized-patterns-failed-pc-zero","score_summary_complete":true}}' \
    --format json pc score \
    --board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --queue I
run_json_smoke pc-score-finder \
    '{"kind":"pc-fixed-score-witness.v2","command_kind":"pc-fixed-score-witness.v2","summary":{"capability_id":"pc.score-finder","score_pattern_canonical_selection":"smallest-canonical-candidate-id","score_pattern_winner_complete":true}}' \
    --format json pc score-finder \
    --board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --queue I
run_json_smoke pc-saves \
    '{"kind":"pc-save-groups.v2","command_kind":"pc-save-groups.v2","summary":{"save_contract":"pc-save-groups.v2"}}' \
    --format json pc saves \
    --board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --patterns I
run_json_smoke pc-best-save \
    '{"kind":"pc-best-save.v2","command_kind":"pc-best-save.v2","summary":{"best_save_contract":"pc-best-save.v2","best_save_canonical_selection":"smallest-canonical-candidate-id"}}' \
    --format json pc best-save \
    --board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --patterns I
run_json_smoke forward-ren \
    '{"kind":"ren","command_kind":"ren","summary":{"complete":true,"maximum_ren":0}}' \
    --format json --include-solution-data ren \
    --board-mask 0x3f --height 4 --queue I --no-hold --workers 1
run_json_smoke pc-score-minimals \
    '{"kind":"pc-score-portfolio.v2","summary":{"capability_id":"pc.score-minimals","result_contract":"pc-score-portfolio.v2","payload_kind":"coverage-portfolio","alternative_index":"1","member_page_number":"1","page_handle_available":true,"score_minimals_score_equality":"score-only","score_minimals_attack_role":"informational-only","score_minimals_canonical_selection":"smallest-canonical-candidate-id"},"resource_report":{"probability_complete":true,"count_complete":true,"truncated":false,"truncation_reason":null,"count_truncated_reason":null,"renormalized":false}}' \
    --format json pc score-minimals \
    --board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --queue I

score_minimals_tie_dir="$(mktemp -d "$BUILD_ROOT/discord-score-minimals.XXXXXX")"
score_minimals_tie_snapshot="$score_minimals_tie_dir/portfolio.jsonl"
if ! score_minimals_tie_json="$("$RELEASE_BINARY" \
    --format json pc score-minimals \
    --board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --queue I \
    --ties --tie-snapshot "$score_minimals_tie_snapshot")"
then
    printf 'Clearra CLI explicit score-minimals portfolio smoke failed\n' >&2
    exit 2
fi
if ! printf '%s' "$score_minimals_tie_json" | node -e '
    const fs = require("node:fs");
    const structured = JSON.parse(fs.readFileSync(0, "utf8"));
    if (!structured?.summary?.portfolio_alternative_page) {
        throw new Error("explicit score-minimals output omitted its portfolio page");
    }
'; then
    printf 'Clearra CLI explicit score-minimals portfolio smoke omitted its opt-in page\n' >&2
    exit 2
fi
if printf '%s' "$score_minimals_tie_json" | \
    node "$ROOT/scripts/tools/validate-release-cli-smokes.mjs" \
        --validate-discord-score-minimals-json >/dev/null 2>&1
then
    printf 'Discord accepted explicit score-minimals tie metadata\n' >&2
    exit 2
fi
run_json_smoke terminal-supply-p0 \
    '{"summary":{"actual_backend":"wasm-cpu","unique_solution_count":18,"normalized_unique_solution_count":18,"solution_count_calculated":true,"solution_set_materialized":true,"solution_keys_materialized_count":18,"solution_keys_complete":true,"count_complete":true,"supply_window_resolution":"projected-terminal-lookahead","projects_unplaced_lookahead":true,"projects_standard_bag_lookahead":false,"source_sequence_length":7,"total_possible_pattern_count":"1","normalized_solution_set_hash":"cts1:8a7fc484d9b49994","actual_normalized_solution_set_hash":"cts1:8a7fc484d9b49994"}}' \
    --format json --include-solution-data pc-scenario \
    --field 0x1c0701c07 --visible-height 4 --queue STOILJZ \
    --max-pieces 7 --exact-pieces 7 --count-policy count-unique \
    --backend cpu --workers 1

printf 'cli_release_binary=%s\n' "$RELEASE_BINARY"
