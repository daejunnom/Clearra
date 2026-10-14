const TERMINAL_SUPPLY_MODE = "--validate-terminal-supply-json";
const DISCORD_SCORE_MINIMALS_MODE =
  "--validate-discord-score-minimals-json";
const DISCORD_CANONICAL_RESULT_MODE =
  "--validate-discord-canonical-result-json";
const TERMINAL_SUPPLY_EXPECTED_COUNT = 18;
const TERMINAL_SUPPLY_EXPECTED_HASH = "cts1:8a7fc484d9b49994";
const TERMINAL_SUPPLY_INITIAL_MASK = 0x1c0701c07n;
const TERMINAL_SUPPLY_FULL_FIELD = (1n << 40n) - 1n;
const CANONICAL_PIECE_ORDER = Object.freeze([
  "I",
  "O",
  "T",
  "S",
  "Z",
  "J",
  "L",
]);
const FNV_OFFSET = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;
const U64_MASK = (1n << 64n) - 1n;

const commandArguments = process.argv.slice(2);
if (commandArguments.length > 0) {
  if (commandArguments.length === 2 &&
      commandArguments[0] === DISCORD_CANONICAL_RESULT_MODE) {
    const raw = await readStandardInput();
    const structured = JSON.parse(raw);
    const publicKind = commandArguments[1];
    if (["pc-path", "pc-minimals", "pc-score", "pc-score-finder"].includes(publicKind)) {
      const { projectDiscordTypedProductResult } = await import(new URL(
        "../../apps/clearra-discord-bot/src/discord/typed-product-result.mjs",
        import.meta.url,
      ));
      const projection = projectDiscordTypedProductResult(structured);
      if (projection === null) {
        throw new Error(`Discord ${publicKind} canonical projection is unavailable`);
      }
    } else if (["pc-saves", "pc-best-save"].includes(publicKind)) {
      const {
        selectDiscordBestSaveWinner,
        validDiscordPcSaveResult,
      } = await import(new URL(
        "../../apps/clearra-discord-bot/src/discord/pc-save-result.mjs",
        import.meta.url,
      ));
      const saveKind = publicKind === "pc-saves" ? "saves" : "best-save";
      if (!validDiscordPcSaveResult(structured, saveKind)) {
        throw new Error(`release CLI ${publicKind} JSON is not a valid Discord result`);
      }
      if (saveKind === "best-save" && selectDiscordBestSaveWinner(structured.summary) === null) {
        throw new Error("Discord best-save core-owned canonical winner is unavailable");
      }
    } else if (publicKind === "forward-ren") {
      const { buildCtk3Result } = await import(new URL(
        "../../apps/clearra-discord-bot/src/clearra/ctk3-result.mjs",
        import.meta.url,
      ));
      const rendered = buildCtk3Result(structured);
      if (rendered === null || rendered.pageCount !== 1) {
        throw new Error("Discord REN core-owned canonical outcome did not render exactly once");
      }
    } else {
      throw new Error(`unsupported Discord canonical result kind: ${publicKind}`);
    }
    console.log(`Discord ${publicKind} release asset JSON passed`);
    process.exit(0);
  } else if (commandArguments.length === 1 &&
      commandArguments[0] === DISCORD_SCORE_MINIMALS_MODE) {
    const raw = await readStandardInput();
    const structured = JSON.parse(raw);
    const {
      discordPcScoreMinimalsResultProjection,
      validDiscordPcScoreMinimalsResult,
    } = await import(new URL(
      "../../apps/clearra-discord-bot/src/discord/pc-score-minimals-result.mjs",
      import.meta.url,
    ));
    if (!validDiscordPcScoreMinimalsResult(structured)) {
      throw new Error(
        "release CLI score-minimals JSON is not a valid Discord canonical result",
      );
    }
    const projection = discordPcScoreMinimalsResultProjection(structured);
    if (projection === null) {
      throw new Error("Discord score-minimals projection is unavailable");
    }
    console.log(
      `Discord score-minimals release asset JSON passed: candidate=${projection.canonicalCandidateId}`,
    );
    process.exit(0);
  } else if (
    commandArguments.length === 3 &&
    commandArguments[0] === TERMINAL_SUPPLY_MODE &&
    commandArguments[1] === "--expected-source-commit" &&
    /^[0-9a-f]{40}$/u.test(commandArguments[2])
  ) {
    const raw = await readStandardInput();
    const evidence = validateTerminalSupplyJson(raw, commandArguments[2]);
    console.log(
      `Terminal-supply release asset JSON passed: solutions=${evidence.solutionCount} hash=${evidence.normalizedSetHash}`,
    );
    process.exit(0);
  }
  throw new Error(
    `unsupported release CLI smoke validator arguments: ${commandArguments.join(" ")}`,
  );
}

throw new Error("provide a release result JSON validation mode");

async function readStandardInput() {
  process.stdin.setEncoding("utf8");
  let raw = "";
  for await (const chunk of process.stdin) raw += chunk;
  return raw;
}

function validateTerminalSupplyJson(raw, expectedSourceCommit) {
  if (raw.trim().length === 0) {
    throw new Error("terminal-supply release asset returned empty JSON");
  }
  const response = JSON.parse(raw);
  requireEqual(response?.schema_version, 2, "schema_version");
  requireEqual(response?.kind, "pc-scenario", "kind");
  const expectedIdentity = {
    source_commit: expectedSourceCommit,
    engine_build_id: expectedSourceCommit,
    contract_schema_version: "clearra.search.contract.v2",
    supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
    artifact_schema_version: "clearra.solution-data.v1",
  };
  for (const [field, expected] of Object.entries(expectedIdentity)) {
    requireEqual(
      response?.runtime_identity?.[field],
      expected,
      `runtime_identity.${field}`,
    );
  }

  const expectedSummary = {
    actual_backend: "wasm-cpu",
    unique_solution_count: TERMINAL_SUPPLY_EXPECTED_COUNT,
    normalized_unique_solution_count: TERMINAL_SUPPLY_EXPECTED_COUNT,
    solution_count_calculated: true,
    solution_set_materialized: true,
    solution_keys_materialized_count: TERMINAL_SUPPLY_EXPECTED_COUNT,
    solution_keys_complete: true,
    count_complete: true,
    supply_window_resolution: "projected-terminal-lookahead",
    projects_unplaced_lookahead: true,
    projects_standard_bag_lookahead: false,
    source_sequence_length: 7,
    total_possible_pattern_count: "1",
    normalized_solution_set_hash: TERMINAL_SUPPLY_EXPECTED_HASH,
    actual_normalized_solution_set_hash: TERMINAL_SUPPLY_EXPECTED_HASH,
  };
  for (const [field, expected] of Object.entries(expectedSummary)) {
    requireEqual(response?.summary?.[field], expected, `summary.${field}`);
  }

  requireEqual(
    response?.contract?.command?.kind,
    "pc-scenario",
    "contract.command.kind",
  );
  requireEqual(
    response?.contract?.command?.input_mode,
    "inline",
    "contract.command.input_mode",
  );
  requireEqual(
    response?.contract?.pc?.search?.backend_selected,
    "wasm-cpu",
    "contract.pc.search.backend_selected",
  );
  requireEqual(
    response?.contract?.pc?.execution_report?.replay?.trace_steps,
    7,
    "contract.pc.execution_report.replay.trace_steps",
  );
  requireEqual(
    response?.contract?.solution_data?.requested,
    true,
    "contract.solution_data.requested",
  );
  requireEqual(
    response?.contract?.solution_data?.status,
    "complete",
    "contract.solution_data.status",
  );
  requireEqual(
    response?.contract?.solution_data?.reason,
    null,
    "contract.solution_data.reason",
  );
  requireEqual(
    response?.contract?.artifacts?.schema_version,
    "clearra.solution-data.v1",
    "contract.artifacts.schema_version",
  );

  const solutionKeys = response?.contract?.artifacts?.solution_keys;
  if (!Array.isArray(solutionKeys)) {
    throw new Error("contract.artifacts.solution_keys must be an array");
  }
  requireEqual(
    solutionKeys.length,
    TERMINAL_SUPPLY_EXPECTED_COUNT,
    "solution_keys.length",
  );
  for (let index = 0; index < solutionKeys.length; index += 1) {
    const key = solutionKeys[index];
    if (typeof key !== "string") {
      throw new Error(`solution_keys[${index}] must be a string`);
    }
    if (index > 0 && solutionKeys[index - 1] >= key) {
      throw new Error("solution_keys must be strictly sorted and unique");
    }
    assertCanonicalTerminalSolutionKey(key);
  }

  const recomputedHash = normalizedSetHash(solutionKeys);
  requireEqual(
    recomputedHash,
    TERMINAL_SUPPLY_EXPECTED_HASH,
    "independently recomputed solution hash",
  );
  return Object.freeze({
    solutionCount: solutionKeys.length,
    normalizedSetHash: recomputedHash,
  });
}

function assertCanonicalTerminalSolutionKey(key) {
  const match = /^ctk1\|initial=([0-9a-f]{16})\|placements=(.*)$/.exec(key);
  if (!match || BigInt(`0x${match[1]}`) !== TERMINAL_SUPPLY_INITIAL_MASK) {
    throw new Error(`non-canonical terminal-supply solution key: ${key}`);
  }

  const encodedPlacements = match[2].split(",");
  requireEqual(encodedPlacements.length, 7, "canonical placement count");
  const masksByPiece = new Map(
    CANONICAL_PIECE_ORDER.map((piece) => [piece, []]),
  );
  for (const encoded of encodedPlacements) {
    const placement = /^([IOTSZJL]):([0-9a-f]{16})$/.exec(encoded);
    if (!placement) throw new Error(`non-canonical placement: ${encoded}`);
    masksByPiece.get(placement[1]).push(BigInt(`0x${placement[2]}`));
  }

  let occupied = TERMINAL_SUPPLY_INITIAL_MASK;
  const canonicalPlacements = [];
  for (const piece of CANONICAL_PIECE_ORDER) {
    const masks = masksByPiece.get(piece).sort(compareBigInt);
    requireEqual(masks.length, 1, `canonical ${piece} placement count`);
    for (const mask of masks) {
      requireEqual(popcount(mask), 4, `canonical ${piece} cell count`);
      if ((occupied & mask) !== 0n) {
        throw new Error(`canonical ${piece} placement overlaps occupied cells`);
      }
      occupied |= mask;
      canonicalPlacements.push(`${piece}:${hex64(mask)}`);
    }
  }
  requireEqual(
    occupied,
    TERMINAL_SUPPLY_FULL_FIELD,
    "terminal-supply completed field mask",
  );
  requireEqual(
    key,
    `ctk1|initial=${hex64(TERMINAL_SUPPLY_INITIAL_MASK)}|placements=${canonicalPlacements.join(",")}`,
    "canonical solution key encoding",
  );
}

function normalizedSetHash(keys) {
  let hash = FNV_OFFSET;
  for (const key of [...keys].sort()) {
    for (const byte of Buffer.from(`${key}\0`, "utf8")) {
      hash ^= BigInt(byte);
      hash = (hash * FNV_PRIME) & U64_MASK;
    }
  }
  return `cts1:${hash.toString(16).padStart(16, "0")}`;
}

function popcount(value) {
  let count = 0;
  for (let remaining = value; remaining !== 0n; remaining &= remaining - 1n)
    count += 1;
  return count;
}

function compareBigInt(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function hex64(value) {
  return value.toString(16).padStart(16, "0");
}

function requireEqual(actual, expected, label) {
  if (actual !== expected) {
    throw new Error(
      `${label} mismatch: expected ${String(expected)}, received ${String(actual)}`,
    );
  }
}
