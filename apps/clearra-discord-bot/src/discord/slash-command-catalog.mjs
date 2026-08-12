import { normalizeDiscordLocale } from "./i18n.mjs";
import {
  DISCORD_PC_FIELD_MAX_ROWS,
  DISCORD_WIDE_FIELD_MAX_ROWS,
} from "./slash-command-input.mjs";

// SRP rationale: this module has one behavior-level change reason: defining the
// complete localized Discord application-command registration metadata contract.

const SUB_COMMAND_OPTION = 1;
const STRING_OPTION = 3;
const INTEGER_OPTION = 4;
const CHAT_INPUT_COMMAND = 1;
const MESSAGE_COMMAND = 3;
const GUILD_CONTEXT = 0;
const GUILD_INSTALL = 0;
const MANAGE_CHANNELS_PERMISSION = String(1n << 4n);
const MANAGE_GUILD_PERMISSION = String(1n << 5n);
const FIELD_MAX_LENGTH = 6000;
const NEXT_MAX_LENGTH = 2048;
const OPTIONAL_SETTINGS_MAX_LENGTH = 256;
const DOCUMENT_MAX_LENGTH = 6000;

export const BUILTIN_KICKTABLES = Object.freeze([
  Object.freeze({ name: "SRS+ (default)", value: "srs-plus" }),
  Object.freeze({ name: "SRS", value: "srs" }),
  Object.freeze({ name: "SRS-X", value: "srs-x" }),
  Object.freeze({ name: "Jstris 180", value: "jstris-180" }),
]);

export const NATIVE_BUILTIN_KICKTABLES = Object.freeze([
  ...BUILTIN_KICKTABLES,
  Object.freeze({ name: "No kicks", value: "no-kick" }),
]);

const PC_COMMANDS = Object.freeze([
  command("path", "Find every represented perfect-clear path", "pc", "pc"),
  command("percent", "Calculate exact perfect-clear success probability", "pc", "pc", {
    note: "Equivalent to /chance.",
  }),
  command("chance", "Calculate exact perfect-clear success probability", "pc", "pc", {
    note: "Equivalent to /percent.",
  }),
  command("minimals", "Find a minimum-cover perfect-clear solution set", "pc", "pc"),
  command("score", "Score perfect-clear solutions with the Jstris profile", "pc", "pc"),
  command(
    "score-minimals",
    "Score a minimum-cover perfect-clear solution set",
    "pc",
    "pc",
  ),
  command("saves", "Analyze success probability for each PC solution", "pc", "pc", {
    note: "Equivalent to /best-save.",
  }),
  command("best-save", "Analyze success probability for each PC solution", "pc", "pc", {
    note: "Equivalent to /saves.",
  }),
]);

const TARGET_COMMANDS = Object.freeze([
  command("setup", "Measure build probability for a target shape", "target", "colored"),
  command("congruent", "Measure build probability for a target shape", "target", "colored", {
    note: "Equivalent to /setup.",
  }),
  command(
    "congruent-cover",
    "Measure build probability for a target shape",
    "target",
    "colored",
    { note: "Uses the same target-shape calculation as /setup." },
  ),
  command(
    "setup-cover",
    "Measure build probability for a target occupancy mask",
    "target",
    "colored",
    { note: "The field represents the target occupancy mask." },
  ),
  command(
    "cover-percent",
    "Measure build probability for a target occupancy mask",
    "target",
    "colored",
    { note: "The field represents the target occupancy mask." },
  ),
  command(
    "special-cover",
    "Measure T-spin coverage for a target shape",
    "target",
    "colored",
  ),
]);

const SEARCH_COMMAND_DEFINITIONS = Object.freeze([
  ...PC_COMMANDS,
  command(
    "cover",
    "Measure build probability from a base into target cells",
    "cover",
    "cover",
  ),
  ...TARGET_COMMANDS,
  command("spin-cover", "Find forward T-spin completions", "spin", "spin", {
    note: "T-spin mini (TSM) is intentionally unavailable.",
  }),
  command("spin", "Find forward T-spin completions", "spin", "spin", {
    note: "This uses the same calculation as /spin-cover; TSM is intentionally unavailable.",
  }),
  command(
    "score-finder",
    "Find the highest-Jstris-score perfect clear for one exact next queue",
    "pc",
    "score-fixed-next",
  ),
  command(
    "damage",
    "Find maximum damage for one exact next queue",
    "damage",
    "fixed-next",
    { argvPrefix: Object.freeze(["damage"]) },
  ),
  command(
    "spin-structure",
    "Find subset-minimal spin structures from an unordered piece inventory",
    "spin",
    "spin-structure",
    { argvPrefix: Object.freeze(["spin-structure"]) },
  ),
  command("pc-setup", "Rank setup candidates by joint build and PC coverage", "setup", "remaining", {
    argvPrefix: Object.freeze(["setup-finder"]),
    setupPriority: "all",
    note: "Ranks candidates by both build and perfect-clear coverage.",
  }),
  command("best-setup", "Rank setup candidates by build coverage", "setup", "remaining", {
    argvPrefix: Object.freeze(["setup-finder"]),
    setupPriority: "build",
    note: "Ranks candidates by build coverage.",
  }),
  command("dpc-finder", "Rank setup candidates by perfect-clear coverage", "setup", "remaining", {
    argvPrefix: Object.freeze(["setup-finder"]),
    setupPriority: "pc",
    note: "Ranks candidates by perfect-clear coverage.",
  }),
  command("verify", "Run one group of Clearra verification checks", "verify", "verify"),
]);

const RENDER_FILE_COMMAND = Object.freeze({
  name: "render-file",
  kind: "render-file",
  input: "render-file",
  description: "Download an original GIF from a recent Clearra field preview",
  registration: Object.freeze({
    name: "render-file",
    description: "Download an original GIF from a recent Clearra field preview",
    options: Object.freeze([
      stringOption(
        "image",
        "Clearra preview message link or ID; omit for your latest, then channel latest",
        false,
        512,
      ),
    ]),
  }),
});

const RENDER_FILE_MESSAGE_COMMAND = Object.freeze({
  name: "get-original-gif",
  kind: "render-file-message",
  registration: Object.freeze({
    type: MESSAGE_COMMAND,
    name: "Get original GIF",
    integration_types: Object.freeze([GUILD_INSTALL]),
    contexts: Object.freeze([GUILD_CONTEXT]),
  }),
});

const HELP_COMMAND = Object.freeze({
  name: "help",
  kind: "help",
  registration: Object.freeze({
    name: "help",
    description: "Show the exact syntax and limits for a Clearra command",
    options: Object.freeze([
      Object.freeze({
        type: STRING_OPTION,
        name: "arguments",
        description: "Command to explain; omit it to list every command group",
        required: false,
      }),
    ]),
  }),
});

const CHANNEL_SETTINGS_COMMAND = Object.freeze({
  name: "channel-settings",
  kind: "management",
  scope: "channel",
  registration: Object.freeze({
    name: "channel-settings",
    description: "Manage Clearra in the current channel",
    contexts: Object.freeze([GUILD_CONTEXT]),
    integration_types: Object.freeze([GUILD_INSTALL]),
    default_member_permissions: MANAGE_CHANNELS_PERMISSION,
    options: Object.freeze([
      managementSubcommand("language-show", "Show the effective channel language"),
      managementSubcommand(
        "language-set",
        "Set the channel response language",
        [languageOption()],
      ),
      managementSubcommand("language-reset", "Remove the channel language override"),
      managementSubcommand("disable", "Disable Clearra commands in this channel"),
      managementSubcommand("enable", "Enable Clearra commands in this channel"),
    ]),
  }),
});

const SERVER_SETTINGS_COMMAND = Object.freeze({
  name: "server-settings",
  kind: "management",
  scope: "guild",
  registration: Object.freeze({
    name: "server-settings",
    description: "Manage Clearra across this server",
    contexts: Object.freeze([GUILD_CONTEXT]),
    integration_types: Object.freeze([GUILD_INSTALL]),
    default_member_permissions: MANAGE_GUILD_PERMISSION,
    options: Object.freeze([
      managementSubcommand("language-show", "Show the effective server language"),
      managementSubcommand(
        "language-set",
        "Set the server response language",
        [languageOption()],
      ),
      managementSubcommand("language-reset", "Remove the server language override"),
      managementSubcommand("pause", "Disable every command except server resume"),
      managementSubcommand("resume", "Resume Clearra commands in this server"),
    ]),
  }),
});

const SEARCH_COMMANDS = Object.freeze(
  SEARCH_COMMAND_DEFINITIONS.map((definition) => {
    const argvPrefix = definition.argvPrefix ?? Object.freeze([
      "sfinder",
      definition.sfinderName ?? definition.name,
    ]);
    return Object.freeze({
      ...definition,
      kind: "search",
      argvPrefix,
      registration: Object.freeze({
        name: definition.name,
        description: definition.description,
        options: registrationOptions(definition.input),
      }),
    });
  }),
);

const FINESSE_SUBCOMMANDS = Object.freeze({
  search: finesseSubcommand(
    "search",
    "Find minimum-input build routes for a target",
    "finesse-search",
  ),
  score: finesseSubcommand(
    "score",
    "Calculate minimum inputs for a CTK3 or Fumen placement sequence",
    "finesse-score",
  ),
});

const FINESSE_COMMAND = Object.freeze({
  name: "finesse",
  kind: "search",
  input: "finesse",
  description: "Find or score minimum-input tetromino placements",
  subcommands: FINESSE_SUBCOMMANDS,
  registration: Object.freeze({
    name: "finesse",
    description: "Find or score minimum-input tetromino placements",
    options: Object.freeze(
      Object.values(FINESSE_SUBCOMMANDS).map(({ subcommand, description, registration }) =>
        Object.freeze({
          type: SUB_COMMAND_OPTION,
          name: subcommand,
          description,
          options: registration.options,
        })
      ),
    ),
  }),
});

export const representedSfinderCommandNames = Object.freeze(
  SEARCH_COMMANDS.map(({ name }) => name),
);

export const slashCommandCatalog = Object.freeze([
  HELP_COMMAND,
  RENDER_FILE_COMMAND,
  CHANNEL_SETTINGS_COMMAND,
  SERVER_SETTINGS_COMMAND,
  FINESSE_COMMAND,
  ...SEARCH_COMMANDS,
]);

export const messageCommandCatalog = Object.freeze([
  RENDER_FILE_MESSAGE_COMMAND,
]);

const COMMANDS_BY_NAME = new Map(
  slashCommandCatalog.map((entry) => [entry.name, entry]),
);
const MESSAGE_COMMANDS_BY_NAME = new Map(
  messageCommandCatalog.map((entry) => [entry.registration.name, entry]),
);

export function findSlashCommand(name) {
  if (typeof name !== "string") return null;
  return COMMANDS_BY_NAME.get(name) ?? null;
}

export function findMessageCommand(name) {
  return typeof name === "string"
    ? MESSAGE_COMMANDS_BY_NAME.get(name) ?? null
    : null;
}

export function findApplicationCommand(type, name) {
  if (type === CHAT_INPUT_COMMAND) return findSlashCommand(name);
  if (type === MESSAGE_COMMAND) return findMessageCommand(name);
  return null;
}

export function resolveSlashCommandInvocation(command, rawOptions = []) {
  if (command?.input !== "finesse") {
    return Object.freeze({ command, rawOptions });
  }
  if (!Array.isArray(rawOptions) || rawOptions.length !== 1) {
    throw new Error("/finesse requires exactly one search or score subcommand.");
  }
  const selected = rawOptions[0];
  if (selected?.type !== SUB_COMMAND_OPTION || typeof selected.name !== "string") {
    throw new Error("/finesse requires a search or score subcommand.");
  }
  const variant = command.subcommands[selected.name];
  if (!variant) {
    throw new Error("/finesse subcommand must be search or score.");
  }
  if (selected.options !== undefined && !Array.isArray(selected.options)) {
    throw new Error("Discord supplied invalid /finesse subcommand options.");
  }
  return Object.freeze({
    command: variant,
    rawOptions: selected.options ?? [],
  });
}

export function formatSlashCommandHelp(requestedName, locale = "en") {
  const language = normalizeDiscordLocale(locale);
  const normalized = normalizeHelpTarget(requestedName);
  if (!normalized) return commandListHelp(language);
  const entry = findHelpCommand(normalized);
  if (!entry || !["search", "render-file"].includes(entry.kind)) {
    return language === "ko"
      ? `알 수 없는 Clearra 명령어 \`${requestedName}\`입니다. \`/help\`에서 명령어 목록을 확인하세요.`
      : `Unknown Clearra command \`${requestedName}\`. Use \`/help\` to list commands.`;
  }
  const lines = [
    `**/${entry.name}${entry.subcommand ? ` ${entry.subcommand}` : ""}** — ${localizedCommandDescription(entry, language)}`,
    language === "ko"
      ? `직접 입력 문법: \`${syntax(entry, language)}\``
      : `Direct syntax: \`${syntax(entry)}\``,
    language === "ko"
      ? "필수 입력을 모두 넣지 않고 명령어를 실행하면 안내 입력 창이 열립니다."
      : "Invoke the command without all required inputs to open its guided Modal form.",
  ];
  if (entry.kind === "search" && ["field", "base", "target"].some((name) =>
    entry.registration.options.some((option) => option.name === name)
  )) {
    lines.push(language === "ko"
      ? "여러 줄 `#`/`_` 격자는 필드 옵션을 생략하고 입력 창에서 작성하세요. 직접 입력은 `grid:윗줄/다음줄` 형식입니다."
      : "For a multiline `#`/`_` grid, omit the board option and use the Modal. Direct input uses `grid:top-row/next-row`.");
  }
  lines.push(...inputHelp(entry, language));
  if (entry.note) {
    lines.push(language === "ko" ? `참고: ${localizedNote(entry, language)}` : `Note: ${entry.note}`);
  }
  lines.push(language === "ko"
    ? "전체 명령어 그룹을 보려면 인수 없이 `/help`를 사용하세요."
    : "Use `/help` without arguments to list every command group.");
  return lines.join("\n");
}

function findHelpCommand(normalized) {
  const compact = normalized.replace(/[ /]+/g, "-");
  if (["finesse-search", "finesse-score"].includes(compact)) {
    return FINESSE_SUBCOMMANDS[compact.slice("finesse-".length)];
  }
  return findSlashCommand(normalized);
}

export function localizedSlashCommandName(name, locale = "en") {
  const command = findSlashCommand(name);
  if (!command) return String(name ?? "");
  return normalizeDiscordLocale(locale) === "ko"
    ? KOREAN_COMMAND_NAMES[command.name] ?? command.name
    : command.name;
}

function command(name, description, group, input, extras = {}) {
  return Object.freeze({ name, description, group, input, ...extras });
}

function finesseSubcommand(subcommand, description, input) {
  return Object.freeze({
    name: "finesse",
    rootName: "finesse",
    subcommand,
    kind: "search",
    group: "finesse",
    input,
    description,
    argvPrefix: Object.freeze(["finesse", subcommand]),
    registration: Object.freeze({
      name: "finesse",
      description,
      options: registrationOptions(input),
    }),
  });
}

function registrationOptions(input) {
  switch (input) {
    case "pc":
      return Object.freeze([
        nextOption(false),
        fieldOption(input),
        linesOption(),
        kicktableOption(),
        settingsOption(input),
      ]);
    case "cover":
      return Object.freeze([
        nextOption(false),
        boardOption(
          "base",
          `Base (1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows): CTK3/Fumen/URL or grid:row/row; omit for multiline form`,
        ),
        boardOption(
          "target",
          `Target (1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows): CTK3/Fumen/URL or grid:row/row; omit for multiline form`,
        ),
        kicktableOption(),
        settingsOption(input),
      ]);
    case "colored":
      return Object.freeze([nextOption(false), fieldOption(input), kicktableOption()]);
    case "spin":
      return Object.freeze([
        nextOption(false),
        fieldOption(input),
        kicktableOption(),
        settingsOption(input),
      ]);
    case "fixed-next":
      return Object.freeze([
        nextOption(true),
        fieldOption(input),
        kicktableOption(true),
        nativeSettingsOption(
          "Options: hold, spin-profile, minimum-damage, initial-combo, initial-b2b, preserve-b2b",
        ),
      ]);
    case "score-fixed-next":
      return Object.freeze([
        nextOption(true),
        fieldOption(input),
        catLinesOption(),
        kicktableOption(),
        catSettingsOption(),
      ]);
    case "remaining":
      return Object.freeze([
        stringOption(
          "remaining",
          "Unordered 1–7-piece inventory; only IOTSZJL, with at most one duplicate kind",
          false,
          64,
        ),
        setupPriorityOption(),
        setupMaximumPiecesOption(),
        setupQueueKnowledgeOption(),
        stringOption(
          "next-cycle-remaining",
          "Exact next-cycle residue; required count depends on the current remaining inventory",
          false,
          64,
        ),
        setupLengthOption(),
        kicktableOption(true),
        nativeSettingsOption(
          "Options: mode, qb, and post-cycle-borrow as space-separated key=value entries",
        ),
      ]);
    case "spin-structure":
      return Object.freeze([
        stringOption(
          "pieces",
          "Unordered IOTSZJL inventory; repeated letters preserve multiplicity",
          false,
          64,
        ),
        fieldOption(input),
        spinStructureLinesOption(),
        spinStructureProfileOption(),
        kicktableOption(true),
        nativeSettingsOption(
          "Options: fill-bottom, fill-top, max-placements, and minimality as key=value entries",
        ),
      ]);
    case "verify":
      return Object.freeze([verifyScopeOption()]);
    case "finesse-search":
      return Object.freeze([
        boardOption(
          "target",
          `Target cells (1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows): static CTK3/Fumen/URL or grid:row/row`,
        ),
        nextOption(false),
        boardOption(
          "base",
          `Starting field (1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows): static CTK3/Fumen/URL or grid:row/row`,
        ),
        kicktableOption(true),
        finesseSettingsOption("search"),
      ]);
    case "finesse-score":
      return Object.freeze([
        stringOption(
          "document",
          "CTK3 or v115 Fumen whose every page contains one placement operation",
          false,
          DOCUMENT_MAX_LENGTH,
        ),
        nextOption(false),
        kicktableOption(true),
        finesseSettingsOption("score"),
      ]);
    default:
      throw new Error(`Unknown slash-command input contract: ${input}`);
  }
}

function finesseSettingsOption(mode) {
  return stringOption(
    "options",
    mode === "search"
      ? "Options: hold, knowledge, source-pieces, aggregation, spin-profile, preserve-b2b"
      : "Options: hold, knowledge, and source-pieces as key=value entries",
    false,
    OPTIONAL_SETTINGS_MAX_LENGTH,
  );
}

function fieldOption(input) {
  const description = input === "pc" || input === "score-fixed-next"
    ? `PC field (1–${DISCORD_PC_FIELD_MAX_ROWS} rows): CTK3/Fumen/URL or grid:row/row; omit for multiline form`
    : input === "colored"
      ? `Target (1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows): CTK3/Fumen/URL or grid:row/row; omit for multiline form`
      : `Field (1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows): CTK3/Fumen/URL or grid:row/row; omit for multiline form`;
  return boardOption("field", description);
}

function boardOption(name, description) {
  return stringOption(name, description, false, FIELD_MAX_LENGTH);
}

function linesOption() {
  return Object.freeze({
    type: INTEGER_OPTION,
    name: "lines",
    description: `PC target height 1–${DISCORD_PC_FIELD_MAX_ROWS}; omit to evaluate every height through ${DISCORD_PC_FIELD_MAX_ROWS}`,
    required: false,
    min_value: 1,
    max_value: DISCORD_PC_FIELD_MAX_ROWS,
    choices: Object.freeze(
      Array.from(
        { length: DISCORD_PC_FIELD_MAX_ROWS },
        (_, index) => index + 1,
      ).map((value) => Object.freeze({ name: `${value} line`, value })),
    ),
  });
}

function catLinesOption() {
  return Object.freeze({
    type: INTEGER_OPTION,
    name: "lines",
    description: `Perfect-clear target height 1–${DISCORD_PC_FIELD_MAX_ROWS}; defaults to 4`,
    required: false,
    min_value: 1,
    max_value: DISCORD_PC_FIELD_MAX_ROWS,
    choices: Object.freeze(
      Array.from(
        { length: DISCORD_PC_FIELD_MAX_ROWS },
        (_, index) => index + 1,
      ).map((value) => Object.freeze({ name: `${value} line`, value })),
    ),
  });
}

function spinStructureLinesOption() {
  const choices = [
    ["Any line count", "any"],
    ...Array.from({ length: 5 }, (_, lines) => [`Exactly ${lines} lines`, String(lines)]),
    ...Array.from({ length: 4 }, (_, index) => {
      const lines = index + 1;
      return [`At least ${lines} lines`, `${lines}+`];
    }),
  ];
  return Object.freeze({
    ...stringOption(
      "lines",
      "Lines cleared by the terminal spin; defaults to at least one",
      false,
      8,
    ),
    choices: Object.freeze(
      choices.map(([name, value]) => Object.freeze({ name, value })),
    ),
  });
}

function spinStructureProfileOption() {
  return Object.freeze({
    ...stringOption(
      "profile",
      "Spin recognition profile; Regular and Mini remain separate",
      false,
      32,
    ),
    choices: Object.freeze([
      ["T-Spins", "t-spins"],
      ["T-Spins+", "t-spins-plus"],
      ["All-Mini", "all-mini"],
      ["All-Mini+", "all-mini-plus"],
      ["All-Spin", "all-spin"],
      ["All-Spin+", "all-spin-plus"],
    ].map(([name, value]) => Object.freeze({ name, value }))),
  });
}

function nextOption(fixed) {
  return stringOption(
    "next",
    fixed
      ? "Exact queue using only IOTSZJL; omit it to enter the queue in the Modal"
      : "Queue/pattern such as *!, *p4, or [IOSZ]p2; omit it to use the Modal",
    false,
    NEXT_MAX_LENGTH,
  );
}

function kicktableOption(native = false) {
  return Object.freeze({
    ...stringOption(
      "kicktable",
      "Built-in kick table; defaults to SRS+ across Clearra",
      false,
      32,
    ),
    choices: native ? NATIVE_BUILTIN_KICKTABLES : BUILTIN_KICKTABLES,
  });
}

function setupPriorityOption() {
  return Object.freeze({
    ...stringOption(
      "priority",
      "Setup ordering: joint build × PC, build probability first, or PC probability first",
      false,
      16,
    ),
    choices: Object.freeze([
      Object.freeze({ name: "Joint build × PC", value: "all" }),
      Object.freeze({ name: "Build probability first", value: "build" }),
      Object.freeze({ name: "PC probability first", value: "pc" }),
    ]),
  });
}

function setupMaximumPiecesOption() {
  return Object.freeze({
    type: INTEGER_OPTION,
    name: "max-setup-pieces",
    description: "Maximum pieces in a setup candidate; defaults to 9 and 10 includes complete PCs",
    required: false,
    min_value: 1,
    max_value: 10,
    choices: Object.freeze(
      Array.from({ length: 10 }, (_, index) => index + 1).map((value) =>
        Object.freeze({ name: `${value} piece${value === 1 ? "" : "s"}`, value })
      ),
    ),
  });
}

function setupQueueKnowledgeOption() {
  return Object.freeze({
    ...stringOption(
      "queue-knowledge",
      "Queue information available while ranking setups; defaults to the full future queue",
      false,
      16,
    ),
    choices: Object.freeze([
      Object.freeze({ name: "Full future queue", value: "full-queue" }),
      Object.freeze({ name: "Visible 7 pieces", value: "visible-7" }),
    ]),
  });
}

function setupLengthOption() {
  return Object.freeze({
    ...stringOption(
      "setup-length",
      "Setup-length preference; Auto follows the selected ordering",
      false,
      16,
    ),
    choices: Object.freeze([
      Object.freeze({ name: "Auto", value: "auto" }),
      Object.freeze({ name: "Prefer longer setups", value: "longer" }),
      Object.freeze({ name: "Prefer shorter setups", value: "shorter" }),
    ]),
  });
}

function settingsOption(input) {
  return stringOption(
    "options",
      input === "spin"
        ? "T-spin target type as type=TSS|TSD|TST|ANY; TSM is unavailable"
      : "Hold policy as hold=use|avoid",
    false,
    OPTIONAL_SETTINGS_MAX_LENGTH,
  );
}

function catSettingsOption() {
  return stringOption(
    "options",
    "Initial back-to-back state as initial-b2b=true|false; defaults to false",
    false,
    OPTIONAL_SETTINGS_MAX_LENGTH,
  );
}

function nativeSettingsOption(description) {
  return stringOption(
    "options",
    description,
    false,
    OPTIONAL_SETTINGS_MAX_LENGTH,
  );
}

function verifyScopeOption() {
  return Object.freeze({
    type: STRING_OPTION,
    name: "scope",
    description: "Verification group; omit or choose All in the Modal to run every check",
    required: false,
    choices: Object.freeze(
      ["pc", "setup", "cover", "build", "kicks"].map((value) =>
        Object.freeze({ name: value, value }),
      ),
    ),
  });
}

function managementSubcommand(name, description, options = []) {
  return Object.freeze({
    type: SUB_COMMAND_OPTION,
    name,
    description,
    ...(options.length > 0 ? { options: Object.freeze(options) } : {}),
  });
}

function languageOption() {
  return Object.freeze({
    type: STRING_OPTION,
    name: "language",
    description: "Language to use for ClearraBot responses and input forms",
    required: true,
    choices: Object.freeze([
      Object.freeze({ name: "English", value: "en" }),
      Object.freeze({ name: "Korean", value: "ko" }),
    ]),
  });
}

function stringOption(name, description, required, maxLength) {
  return Object.freeze({
    type: STRING_OPTION,
    name,
    description,
    required,
    max_length: maxLength,
  });
}

function normalizeHelpTarget(value) {
  if (value === undefined || value === null) return "";
  return String(value).trim().toLowerCase().replace(/^\//, "").replaceAll("_", "-");
}

function commandListHelp(locale) {
  if (locale === "ko") {
    return [
      "**Clearra 슬래시 명령어**",
      "렌더 파일: `/render-file` 또는 미리보기 메시지의 `앱 → 원본 GIF 받기` (명령어 필드는 해당 명령 안에서 자동 렌더링)",
      "퍼펙트 클리어: `/path`, `/percent`, `/chance`, `/minimals`, `/score`, `/score-minimals`, `/saves`, `/best-save`, `/score-finder`",
      "구축 확률: `/cover`, `/setup`, `/congruent`, `/congruent-cover`, `/setup-cover`, `/cover-percent`, `/special-cover`",
      "전방 탐색: `/spin-cover`, `/spin`, `/damage`",
      "스핀 구조 탐색: `/spin-structure`",
      "피네스: `/finesse search`, `/finesse score`",
      "셋업 순위: `/pc-setup`, `/best-setup`, `/dpc-finder`",
      "검증: `/verify`",
      "정확한 문법은 `/help arguments:<명령어>`로 확인하세요. 여러 줄 격자는 필드 옵션을 생략하고 입력 창에서 작성하며, 직접 입력은 `grid:윗줄/다음줄` 형식을 사용합니다.",
      `PC 탐색은 1–${DISCORD_PC_FIELD_MAX_ROWS}줄의 모든 목표 높이를 지원하며, 구축·전방 탐색 필드는 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄을 지원합니다. 정적 CTK3, v115 Fumen, 문서 링크도 지원하며 입력 색상은 모두 채워진 칸으로 처리합니다.`,
    ].join("\n");
  }
  return [
    "**Clearra slash commands**",
    "Render files: `/render-file` or `Apps → Get original GIF` on a preview message (command fields render inside their own command)",
    "Perfect clears: `/path`, `/percent`, `/chance`, `/minimals`, `/score`, `/score-minimals`, `/saves`, `/best-save`, `/score-finder`",
    "Build probability: `/cover`, `/setup`, `/congruent`, `/congruent-cover`, `/setup-cover`, `/cover-percent`, `/special-cover`",
    "Forward search: `/spin-cover`, `/spin`, `/damage`",
    "Spin structures: `/spin-structure`",
    "Finesse: `/finesse search`, `/finesse score`",
    "Setup ranking: `/pc-setup`, `/best-setup`, `/dpc-finder`",
    "Checks: `/verify`",
    "Use `/help arguments:<command>` for exact syntax. Omit a board option to enter a multiline grid in the guided form; direct grids use `grid:top-row/next-row`.",
    `PC search supports every target height from 1 through ${DISCORD_PC_FIELD_MAX_ROWS} rows; build/forward fields support 1 through ${DISCORD_WIDE_FIELD_MAX_ROWS} rows. Static CTK3, v115 Fumen, and document links are also accepted; input colors mean occupied cells.`,
  ].join("\n");
}

function syntax(entry, locale = "en") {
  if (normalizeDiscordLocale(locale) === "ko") {
    switch (entry.input) {
      case "render-file":
        return "/render-file [image:<같은 채널의 미리보기 메시지 링크|메시지 ID>]";
      case "pc":
        return `/${entry.name} next:<패턴> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [kicktable:<내장 프로필>] [options:hold=use]`;
      case "cover":
        return `/${entry.name} next:<패턴> base:<기존 필드> target:<추가할 칸> [kicktable:<내장 프로필>] [options:hold=use]`;
      case "colored":
        return `/${entry.name} next:<패턴> field:<목표 필드> [kicktable:<내장 프로필>]`;
      case "spin":
        return `/${entry.name} next:<패턴> field:<격자|CTK3|v115 Fumen|URL> [kicktable:<내장 프로필>] [options:type=TSS]`;
      case "fixed-next":
        return `/${entry.name} next:<정확한 IOTSZJL 큐> field:<격자|CTK3|v115 Fumen|URL> [kicktable:<내장 프로필>] [options:<hold spin-profile minimum-damage initial-combo initial-b2b preserve-b2b>]`;
      case "score-fixed-next":
        return `/${entry.name} next:<정확한 IOTSZJL 큐> field:<격자|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [kicktable:<내장 프로필>] [options:initial-b2b=false]`;
      case "remaining":
        return `/${entry.name} remaining:<순서 없는 IOTSZJL 목록> [priority:<all|build|pc>] [max-setup-pieces:1..10] [queue-knowledge:<full-queue|visible-7>] [next-cycle-remaining:<정확한 목록>] [setup-length:<auto|longer|shorter>] [kicktable:<내장 프로필>] [options:<mode qb post-cycle-borrow>]`;
      case "spin-structure":
        return `/${entry.name} pieces:<순서 없는 IOTSZJL 목록> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:<any|0..4|1+..4+>] [profile:<T-Spins|T-Spins+|All-Mini(+)|All-Spin(+)>] [kicktable:<내장 프로필>] [options:<fill-bottom fill-top max-placements minimality>]`;
      case "verify":
        return `/${entry.name} [scope:<pc|setup|cover|build|kicks>]`;
      case "finesse":
        return "/finesse search target:<목표 칸> next:<큐|패턴> base:<기존 필드> [options:<hold knowledge source-pieces aggregation spin-profile preserve-b2b>] | /finesse score document:<operation 포함 문서> next:<큐|패턴> [options:<hold knowledge source-pieces>]";
      case "finesse-search":
        return "/finesse search target:<목표 칸> next:<큐|패턴> base:<기존 필드> [options:<hold knowledge source-pieces aggregation spin-profile preserve-b2b>]";
      case "finesse-score":
        return "/finesse score document:<operation 포함 CTK3|v115 Fumen> next:<큐|패턴> [options:<hold knowledge source-pieces>]";
      default:
        throw new Error(`Unknown slash-command input contract: ${entry.input}`);
    }
  }
  switch (entry.input) {
    case "render-file":
      return "/render-file [image:<same-channel preview message link|message ID>]";
    case "pc":
      return `/${entry.name} next:<pattern> field:<grid:top-row/next-row|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [kicktable:<built-in>] [options:hold=use]`;
    case "cover":
      return `/${entry.name} next:<pattern> base:<field> target:<delta> [kicktable:<built-in>] [options:hold=use]`;
    case "colored":
      return `/${entry.name} next:<pattern> field:<target> [kicktable:<built-in>]`;
    case "spin":
      return `/${entry.name} next:<pattern> field:<grid|document|URL> [kicktable:<built-in>] [options:type=TSS]`;
    case "fixed-next":
      return `/${entry.name} next:<exact IOTSZJL queue> field:<grid|document|URL> [kicktable:<built-in>] [options:<hold spin-profile minimum-damage initial-combo initial-b2b preserve-b2b>]`;
    case "score-fixed-next":
      return `/${entry.name} next:<exact IOTSZJL queue> field:<grid|document|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [kicktable:<built-in>] [options:initial-b2b=false]`;
    case "remaining":
      return `/${entry.name} remaining:<unordered IOTSZJL inventory> [priority:<all|build|pc>] [max-setup-pieces:1..10] [queue-knowledge:<full-queue|visible-7>] [next-cycle-remaining:<exact inventory>] [setup-length:<auto|longer|shorter>] [kicktable:<built-in>] [options:<mode qb post-cycle-borrow>]`;
    case "spin-structure":
      return `/${entry.name} pieces:<unordered IOTSZJL inventory> field:<grid:top-row/next-row|document|URL> [lines:<any|0..4|1+..4+>] [profile:<T-Spins|T-Spins+|All-Mini(+)|All-Spin(+)>] [kicktable:<built-in>] [options:<fill-bottom fill-top max-placements minimality>]`;
    case "verify":
      return `/${entry.name} [scope:<pc|setup|cover|build|kicks>]`;
    case "finesse":
      return "/finesse search target:<target> next:<queue|pattern> base:<base> [options:<hold knowledge source-pieces aggregation spin-profile preserve-b2b>] | /finesse score document:<operations> next:<queue|pattern> [options:<hold knowledge source-pieces>]";
    case "finesse-search":
      return "/finesse search target:<target cells> next:<queue|pattern> base:<starting field> [options:<hold knowledge source-pieces aggregation spin-profile preserve-b2b>]";
    case "finesse-score":
      return "/finesse score document:<CTK3|v115 Fumen with operations> next:<queue|pattern> [options:<hold knowledge source-pieces>]";
    default:
      throw new Error(`Unknown slash-command input contract: ${entry.input}`);
  }
}

function inputHelp(entry, locale = "en") {
  if (locale === "ko") return koreanInputHelp(entry);
  const kickHelp = "`kicktable` is one of `srs-plus`, `srs`, `srs-x`, or `jstris-180`; Clearra defaults to `srs-plus`.";
  const nativeKickHelp = "`kicktable` is `srs-plus`, `srs`, `srs-x`, `jstris-180`, or `no-kick`; Clearra defaults to `srs-plus`.";
  switch (entry.input) {
    case "render-file":
      return [
        "For the simplest exact selection, open a Clearra preview message's Apps menu and choose `Get original GIF`. No message ID is needed.",
        "For text commands, reply to the preview with `$render-file` or `>render-file`; the replied-to message becomes the exact source.",
        "`image` accepts a Clearra preview message ID or a Discord message link from the current channel.",
        "When omitted, Clearra checks recent channel history for your newest preview first, then the newest preview from anyone. Deleted or unavailable attachments cannot be recovered.",
        "The result is the original `.gif` file without a reply reference. CTK3, Fumen, and plain field inputs on search commands are rendered inside those commands.",
      ];
    case "pc":
      return [
        `\`field\` is a top-first 10-column grid of 1–${DISCORD_PC_FIELD_MAX_ROWS} rows or one static CTK3/v115 Fumen/URL. In a grid, use \`#\` for filled and \`_\` for empty.`,
        "`next` accepts fixed IOTSZJL queues, `*pN`/`*!`, `P<N>`, piece groups/complements, and `;` alternatives. Letters are case-insensitive; automatic PC alternatives must have equal length.",
        `\`lines\` accepts every height 1–${DISCORD_PC_FIELD_MAX_ROWS}, including odd values. Input-form \`Auto\` evaluates all 1–${DISCORD_PC_FIELD_MAX_ROWS}-row targets serially. \`options\` accepts only \`hold=use|avoid\`.`,
        kickHelp,
      ];
    case "cover":
      return [
        "`base` is the existing field; `target` contains only cells to add. They must not overlap. Target must be non-empty with a block count divisible by four, and base must not contain a completed row.",
        `Both fields accept 1–${DISCORD_WIDE_FIELD_MAX_ROWS} top-first grid rows or one static CTK3/v115 Fumen/URL. In a grid, use \`#\` for filled and \`_\` for empty.`,
        "`next` accepts the supported fixed/group/bag patterns. `options` selects `hold=use|avoid`. Colored CTK3 solution output remains the default.",
        kickHelp,
      ];
    case "colored":
      return [
        "`field` is a non-empty target occupancy mask whose block count is divisible by four. All CTK3/Fumen colors are treated as occupied cells.",
        `A plain grid accepts 1–${DISCORD_WIDE_FIELD_MAX_ROWS} top-first rows; use \`#\` for filled and \`_\` for empty. \`next\` uses the supported fixed/group/bag pattern grammar.`,
        kickHelp,
      ];
    case "spin":
      return [
        `\`field\` accepts 1–${DISCORD_WIDE_FIELD_MAX_ROWS} top-first rows or one static CTK3/v115 Fumen/URL. In a grid, use \`#\` for filled and \`_\` for empty. \`next\` uses the supported fixed/group/bag grammar.`,
        "`options` selects TSS, TSD, TST, or any T-spin; TSM is intentionally unavailable.",
        kickHelp,
      ];
    case "fixed-next":
      return [
        `\`field\` accepts 1–${DISCORD_WIDE_FIELD_MAX_ROWS} top-first rows or one static document/URL. In a grid, use \`#\` for filled and \`_\` for empty. \`next\` must be one exact IOTSZJL queue, not a pattern.`,
        "`options` keys are `hold`, `spin-profile`, `minimum-damage`, `initial-combo`, `initial-b2b`, and `preserve-b2b`. Minimum damage selects at-least mode; zero combo is omitted.",
        nativeKickHelp,
      ];
    case "score-fixed-next":
      return [
        `\`field\` accepts 1–${DISCORD_PC_FIELD_MAX_ROWS} top-first rows or one static CTK3/v115 Fumen/URL. In a grid, use \`#\` for filled and \`_\` for empty. \`next\` must be one exact IOTSZJL queue, not a pattern.`,
        `\`lines\` accepts every perfect-clear target height from 1 through ${DISCORD_PC_FIELD_MAX_ROWS}. \`options\` accepts only \`initial-b2b=true|false\` and defaults to false.`,
        kickHelp,
      ];
    case "remaining":
      return [
        "`remaining` is an unordered inventory of 1–7 IOTSZJL pieces. At most one piece kind may appear twice; that duplicate becomes the initial hold. Three copies or multiple duplicated kinds are rejected.",
        `\`priority\` orders candidates by joint build × PC (\`all\`), build probability, or PC probability. /${entry.name} defaults to \`${entry.setupPriority}\`.`,
        "`max-setup-pieces` accepts 1–10 and defaults to 9; choose 10 to include complete perfect clears. `queue-knowledge` is `full-queue` (default) or `visible-7`.",
        "`next-cycle-remaining` is an exact unordered inventory for the following cycle. Its required count is determined by `remaining` (7→4, 4→1, 1→5, 5→2, 2→6, 6→3, 3→7), with the same duplicate rule.",
        "`setup-length` is `auto`, `longer`, or `shorter`. Auto favors longer setups for `all`/`build` and shorter setups for `pc`.",
        "`options` keys are `mode`, `qb`, and `post-cycle-borrow`. QB mode requires `qb`; borrowing is limited to cycle 7 (`remaining` has three pieces).",
        nativeKickHelp,
      ];
    case "spin-structure":
      return [
        "`pieces` is an unordered IOTSZJL inventory. Repeated letters are multiplicities, not a queue, and hold is not used.",
        `\`field\` accepts 1–${DISCORD_WIDE_FIELD_MAX_ROWS} top-first rows or one static CTK3/v115 Fumen/URL. In a grid, use \`#\` for filled and \`_\` for empty.`,
        "`profile` selects T-Spins, T-Spins+, All-Mini(+), or All-Spin(+). Regular and Mini results are always reported separately; `+` adds the exact immobile-T fallback.",
        "`lines` applies to the terminal spin and defaults to `1+`. Results are subset-minimal across the supplied inventory.",
        "`options` keys are `fill-bottom`, `fill-top`, `max-placements`, and `minimality`. Fill bottom must be below fill top.",
        nativeKickHelp,
      ];
    case "verify":
      return [
        "Choose `pc`, `setup`, `cover`, `build`, or `kicks`. Choose input-form `All` or omit `scope` to run every check.",
      ];
    case "finesse":
      return [
        "Use `search` to find minimum-input builds, or `score` to calculate the minimum inputs for a placement sequence.",
        "Search `options`: `hold`, `knowledge`, `source-pieces`, `aggregation`, `spin-profile`, `preserve-b2b`. Score accepts only `hold`, `knowledge`, `source-pieces`.",
        "Finesse counts inputs, not frames; hard drop and hold each cost one input.",
        nativeKickHelp,
      ];
    case "finesse-search":
      return [
        "`base` is the starting field and `target` contains only cells to add. Both accept one static CTK3/v115 Fumen/URL or a 1–24-row grid.",
        "`next` accepts either one exact IOTSZJL queue or the supported pattern grammar. Exact queues are evaluated as fixed queues.",
        "`options` keys are `hold`, `knowledge`, `source-pieces`, `aggregation`, `spin-profile`, and `preserve-b2b`; tiling is unavailable. Defaults are `hold=empty knowledge=both`.",
        nativeKickHelp,
      ];
    case "finesse-score":
      return [
        "`document` must be CTK3 or v115 Fumen, and every page must contain one placement operation in sequence. Static fields and plain grids are not accepted here.",
        "`next` accepts either one exact IOTSZJL queue or the supported pattern grammar. The result is a minimum-input score for the supplied placements, not a reconstruction of past play.",
        "`options` keys are exactly `hold`, `knowledge`, and `source-pieces`; the default is `hold=empty knowledge=both`.",
        nativeKickHelp,
      ];
    default:
      throw new Error(`Unknown slash-command input contract: ${entry.input}`);
  }
}

function koreanInputHelp(entry) {
  const kickHelp = "`kicktable`은 `srs-plus`, `srs`, `srs-x`, `jstris-180` 중 하나이며 기본값은 `srs-plus`입니다.";
  const nativeKickHelp = "`kicktable`은 `srs-plus`, `srs`, `srs-x`, `jstris-180`, `no-kick` 중 하나이며 기본값은 `srs-plus`입니다.";
  switch (entry.input) {
    case "render-file":
      return [
        "가장 간단하게 정확히 지정하려면 Clearra 미리보기 메시지의 앱 메뉴에서 `원본 GIF 받기`를 선택합니다. 메시지 ID는 필요하지 않습니다.",
        "텍스트 명령어는 미리보기에 답장하면서 `$render-file` 또는 `>render-file`을 입력하면 답장 대상을 정확한 원본으로 사용합니다.",
        "`image`에는 현재 채널에 있는 Clearra 미리보기 메시지 ID 또는 Discord 메시지 링크를 입력합니다.",
        "생략하면 최근 채널 기록에서 본인의 최신 미리보기를 먼저 찾고, 없으면 전체 최신 미리보기를 찾습니다. 삭제되었거나 사용할 수 없게 된 첨부 파일은 복구할 수 없습니다.",
        "결과는 답글 참조가 없는 원본 `.gif` 파일입니다. 탐색 명령어에 입력한 CTK3, Fumen, 일반 필드는 해당 명령어 내부에서 렌더링됩니다.",
      ];
    case "pc":
      return [
        `\`field\`에는 위쪽 줄부터 적은 10열 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 격자 또는 정적 CTK3/v115 Fumen/URL 하나를 입력합니다. 격자에서 \`#\`은 채움, \`_\`는 빈칸입니다.`,
        "`next`에는 고정 IOTSZJL 큐, `*pN`/`*!`, `P<N>`, 미노 그룹·여집합, `;` 대안을 사용할 수 있습니다. 영문 대소문자를 구분하지 않으며 자동 PC 대안의 길이는 같아야 합니다.",
        `\`lines\`에는 홀수를 포함해 1–${DISCORD_PC_FIELD_MAX_ROWS} 중 원하는 높이를 지정합니다. 입력 창에서 \`자동\`을 고르면 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 전체 목표를 순서대로 판정합니다. \`options\`에는 \`hold=use|avoid\`만 사용할 수 있습니다.`,
        kickHelp,
      ];
    case "cover":
      return [
        "`base`는 기존 필드이고 `target`에는 추가할 칸만 입력합니다. 두 필드는 겹칠 수 없고, 목표 필드는 비어 있지 않으며 블록 수가 4의 배수여야 하고, 기존 필드에는 완성된 줄이 없어야 합니다.",
        `두 필드는 위쪽 줄부터 적은 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 격자 또는 정적 CTK3/v115 Fumen/URL 하나를 받습니다. 격자에서 \`#\`은 채움, \`_\`는 빈칸입니다.`,
        "`next`에는 지원되는 고정·그룹·가방 패턴을 사용합니다. `options`에서는 홀드 사용 여부를 선택합니다. 색상을 보존한 CTK3 결과가 기본 출력입니다.",
        kickHelp,
      ];
    case "colored":
      return [
        "`field`는 비어 있지 않고 블록 수가 4의 배수인 목표 점유 필드입니다. CTK3/Fumen의 모든 색상은 채워진 칸으로 처리합니다.",
        `일반 격자는 위쪽 줄부터 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄을 받습니다. \`#\`은 채움, \`_\`는 빈칸이며, \`next\`에는 지원되는 고정·그룹·가방 패턴을 사용합니다.`,
        kickHelp,
      ];
    case "spin":
      return [
        `\`field\`는 위쪽 줄부터 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 또는 정적 CTK3/v115 Fumen/URL 하나를 받습니다. 격자에서 \`#\`은 채움, \`_\`는 빈칸이며 \`next\`에는 지원되는 고정·그룹·가방 패턴을 사용합니다.`,
        "`options`에서 TSS, TSD, TST 또는 모든 T-spin을 선택합니다. TSM은 지원하지 않습니다.",
        kickHelp,
      ];
    case "fixed-next":
      return [
        `\`field\`는 위쪽 줄부터 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 또는 정적 문서/URL 하나를 받습니다. 격자에서 \`#\`은 채움, \`_\`는 빈칸이며 \`next\`에는 패턴이 아닌 정확한 IOTSZJL 큐 하나를 입력해야 합니다.`,
        "`options` 키는 `hold`, `spin-profile`, `minimum-damage`, `initial-combo`, `initial-b2b`, `preserve-b2b`입니다.",
        nativeKickHelp,
      ];
    case "score-fixed-next":
      return [
        `\`field\`는 위쪽 줄부터 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 또는 정적 CTK3/v115 Fumen/URL 하나를 받습니다. 격자에서 \`#\`은 채움, \`_\`는 빈칸이며 \`next\`에는 패턴이 아닌 정확한 IOTSZJL 큐 하나를 입력해야 합니다.`,
        `\`lines\`에는 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 중 원하는 퍼펙트 클리어 목표 높이를 지정할 수 있습니다. \`options\`에는 \`initial-b2b=true|false\`만 사용하며 기본값은 false입니다.`,
        kickHelp,
      ];
    case "remaining":
      return [
        "`remaining`은 순서 없는 IOTSZJL 미노 1–7개입니다. 한 종류만 두 번 나올 수 있으며 중복 미노가 초기 홀드가 됩니다. 세 개 이상 또는 여러 종류의 중복은 허용하지 않습니다.",
        `\`priority\`는 구축 × PC 종합(\`all\`), 구축 확률 우선, PC 확률 우선으로 후보를 정렬합니다. /${entry.name}의 기본값은 \`${entry.setupPriority}\`입니다.`,
        "`max-setup-pieces`는 1–10이며 기본값은 9입니다. 완성된 PC까지 포함하려면 10을 선택합니다. `queue-knowledge`는 전체 미래 큐를 쓰는 `full-queue`(기본값) 또는 `visible-7`입니다.",
        "`next-cycle-remaining`은 다음 회차에 남을 정확한 순서 없는 미노 목록입니다. 필요한 개수는 `remaining`에 따라 7→4, 4→1, 1→5, 5→2, 2→6, 6→3, 3→7이며 중복 규칙은 같습니다.",
        "`setup-length`는 `auto`, `longer`, `shorter` 중 하나입니다. 자동은 `all`/`build`에서 긴 셋업, `pc`에서 짧은 셋업을 우선합니다.",
        "`options` 키는 `mode`, `qb`, `post-cycle-borrow`입니다. QB 모드에는 `qb`가 필요하며 빌리기는 remaining 3개인 7회차에서만 허용됩니다.",
        nativeKickHelp,
      ];
    case "spin-structure":
      return [
        "`pieces`는 순서 없는 IOTSZJL 미노 목록입니다. 반복 문자는 수량을 뜻하며 큐나 홀드로 해석하지 않습니다.",
        `\`field\`에는 위쪽 줄부터 적은 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 격자 또는 정적 CTK3/v115 Fumen/URL 하나를 입력합니다. 격자에서 \`#\`은 채움, \`_\`는 빈칸입니다.`,
        "`profile`은 T-Spins, T-Spins+, All-Mini(+), All-Spin(+) 중 하나입니다. Regular와 Mini는 항상 따로 출력하며 `+`는 정확한 immobile T 판정을 추가합니다.",
        "`lines`는 마지막 스핀이 지우는 줄에 적용되며 기본값은 `1+`입니다. 결과는 입력 미노 안에서 부분집합 최소 구조입니다.",
        "`options` 키는 `fill-bottom`, `fill-top`, `max-placements`, `minimality`입니다.",
        nativeKickHelp,
      ];
    case "verify":
      return ["`pc`, `setup`, `cover`, `build`, `kicks` 중 하나를 고르세요. 모든 검증을 실행하려면 입력 창에서 `전체`를 고르거나 `scope`를 생략하세요."];
    case "finesse":
      return [
        "`search`는 최소 입력 구축 경로를 찾고, `score`는 지정한 배치 순서의 최소 입력 수를 계산합니다.",
        "탐색 `options`는 `hold`, `knowledge`, `source-pieces`, `aggregation`, `spin-profile`, `preserve-b2b`이며 계산은 앞의 세 키만 받습니다.",
        "피네스는 프레임이 아닌 입력 수를 세며, 하드 드롭과 홀드는 각각 1입력입니다.",
        nativeKickHelp,
      ];
    case "finesse-search":
      return [
        "`base`는 시작 필드이고 `target`에는 추가할 칸만 입력합니다. 두 필드 모두 정적 CTK3/v115 Fumen/URL 또는 1–24줄 격자를 받습니다.",
        "`next`에는 정확한 IOTSZJL 큐 하나 또는 지원되는 패턴 문법을 입력합니다. 정확한 큐는 고정 큐로 계산합니다.",
        "`options` 키는 `hold`, `knowledge`, `source-pieces`, `aggregation`, `spin-profile`, `preserve-b2b`이며 tiling은 지원하지 않습니다.",
        nativeKickHelp,
      ];
    case "finesse-score":
      return [
        "`document`에는 모든 페이지에 배치 operation이 하나씩 있는 CTK3 또는 v115 Fumen을 입력합니다. 정적 필드와 일반 격자는 받지 않습니다.",
        "`next`에는 정확한 IOTSZJL 큐 하나 또는 지원되는 패턴 문법을 입력합니다. 결과는 주어진 배치의 최소 입력 점수이며 과거 플레이를 복원한 값이 아닙니다.",
        "`options` 키는 `hold`, `knowledge`, `source-pieces`만 허용합니다.",
        nativeKickHelp,
      ];
    default:
      throw new Error(`Unknown slash-command input contract: ${entry.input}`);
  }
}

function localizedRegistration(entry) {
  const registration = entry.registration;
  const koreanName = KOREAN_COMMAND_NAMES[entry.name] ?? registration.name;
  const hasDescription = typeof registration.description === "string";
  const koreanDescription = hasDescription
    ? localizedCommandDescription(entry, "ko")
    : null;
  return Object.freeze({
    ...registration,
    ...localizationProperty("name_localizations", registration.name, koreanName),
    ...(hasDescription
      ? localizationProperty(
          "description_localizations",
          registration.description,
          koreanDescription,
        )
      : {}),
    ...(registration.options
      ? { options: Object.freeze(registration.options.map((option) =>
          localizeRegistrationOption(option, entry.name)
        )) }
      : {}),
  });
}

function localizeRegistrationOption(option, commandName) {
  const path = `${commandName}.${option.name}`;
  const koreanName = KOREAN_OPTION_NAMES[path] ??
    KOREAN_OPTION_NAMES[option.name] ?? option.name;
  const koreanDescription = KOREAN_OPTION_DESCRIPTIONS[path] ??
    koreanRangeOptionDescription(option) ??
    KOREAN_OPTION_DESCRIPTIONS[option.name] ?? option.description;
  return Object.freeze({
    ...option,
    ...localizationProperty("name_localizations", option.name, koreanName),
    ...localizationProperty(
      "description_localizations",
      option.description,
      koreanDescription,
    ),
    ...(option.options
      ? { options: Object.freeze(option.options.map((nested) =>
          localizeRegistrationOption(nested, path)
        )) }
      : {}),
    ...(option.choices
      ? { choices: Object.freeze(option.choices.map((choice) => Object.freeze({
          ...choice,
          ...localizationProperty(
            "name_localizations",
            choice.name,
            koreanChoiceName(choice.name, choice.value, path),
          ),
        }))) }
      : {}),
  });
}

function koreanRangeOptionDescription(option) {
  if (option?.name !== "field" || typeof option.description !== "string") {
    return null;
  }
  if (option.description.includes(`1–${DISCORD_PC_FIELD_MAX_ROWS} rows`)) {
    return `10열 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 PC 필드, CTK3/v115 Fumen 또는 문서 링크`;
  }
  if (option.description.includes(`1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows`)) {
    return `10열 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 필드, CTK3/v115 Fumen 또는 문서 링크`;
  }
  return null;
}

function localizationProperty(property, original, localized) {
  return typeof localized === "string" && localized !== original
    ? { [property]: Object.freeze({ ko: localized }) }
    : {};
}

function localizedCommandDescription(entry, locale) {
  if (locale !== "ko") return entry.description ?? entry.registration.description;
  if (entry.rootName === "finesse" && entry.subcommand) {
    return KOREAN_OPTION_DESCRIPTIONS[`finesse.${entry.subcommand}`] ?? entry.description;
  }
  return KOREAN_COMMAND_DESCRIPTIONS[entry.name] ??
    entry.description ?? entry.registration.description;
}

function localizedNote(entry, locale) {
  if (locale !== "ko") return entry.note;
  return KOREAN_COMMAND_NOTES[entry.name] ?? entry.note;
}

function koreanChoiceName(name, value, path = "") {
  if (path === "help.arguments" && typeof value === "string") {
    const localized = KOREAN_COMMAND_NAMES[value];
    return localized && localized !== value
      ? `${value} — ${localized}`
      : value;
  }
  if (typeof value === "string" && KOREAN_COMMAND_NAMES[value]) {
    return KOREAN_COMMAND_NAMES[value];
  }
  if (path === "spin-structure.lines" && typeof value === "string") {
    if (value === "any") return "모든 줄 수";
    if (/^[0-4]$/.test(value)) return `정확히 ${value}줄`;
    if (/^[1-4]\+$/.test(value)) return `최소 ${value.slice(0, -1)}줄`;
  }
  if (path === "spin-structure.profile" && typeof value === "string") {
    return ({
      "t-spins": "T 스핀",
      "t-spins-plus": "T 스핀+",
      "all-mini": "전체 Mini",
      "all-mini-plus": "전체 Mini+",
      "all-spin": "전체 스핀",
      "all-spin-plus": "전체 스핀+",
    })[value] ?? name;
  }
  if (path.endsWith(".priority")) {
    return ({
      all: "구축 × PC 종합",
      build: "구축 확률 우선",
      pc: "PC 확률 우선",
    })[value] ?? name;
  }
  if (path.endsWith(".max-setup-pieces") && typeof value === "number") {
    return `${value}개`;
  }
  if (path.endsWith(".queue-knowledge")) {
    return ({
      oracle: "전체 미래 큐",
      "full-queue": "전체 미래 큐",
      "visible-7": "공개 7개",
    })[value] ?? name;
  }
  if (path.endsWith(".setup-length")) {
    return ({
      auto: "자동",
      longer: "긴 셋업 우선",
      shorter: "짧은 셋업 우선",
    })[value] ?? name;
  }
  if (value === "en") return "영어";
  if (value === "ko") return "한국어";
  if (value === "channel") return "채널";
  if (value === "guild") return "서버";
  if (value === "all") return "전체";
  if (value === "auto") {
    return `자동 — 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 전체 판정`;
  }
  if (typeof value === "number" && / line$/.test(name)) return `${value}줄`;
  if (typeof value === "string" && /^hold=use$/.test(value)) return "홀드 사용";
  if (typeof value === "string" && /^hold=avoid$/.test(value)) return "홀드 사용 안 함";
  if (typeof value === "string" && /^type=TSS$/.test(value)) return "T-spin 싱글";
  if (typeof value === "string" && /^type=TSD$/.test(value)) return "T-spin 더블";
  if (typeof value === "string" && /^type=TST$/.test(value)) return "T-spin 트리플";
  if (typeof value === "string" && /^type=ANY$/.test(value)) return "모든 T-spin";
  if (path === "finesse.search.options" || path === "finesse.score.options") {
    const hold = value.includes("hold=avoid") ? "홀드 사용 안 함" : "홀드 사용";
    const knowledge = value.includes("knowledge=visible-7")
      ? "공개 7개"
      : value.includes("knowledge=oracle")
        ? "전체 큐"
        : "전체 큐 및 공개 7개";
    return `${hold} · ${knowledge}${value === "hold=use knowledge=both" ? " (기본값)" : ""}`;
  }
  if (value === "initial_b2b=false") return "초기 B2B 사용 안 함 (기본값)";
  if (value === "initial_b2b=true") return "초기 B2B 사용";
  if (value === "pc") return "퍼펙트 클리어";
  if (value === "build") return "빌드";
  if (value === "kicks") return "킥";
  if (name === "SRS+ (default)") return "SRS+ (기본값)";
  if (value === "no-kick") return "킥 없음";
  return name;
}

const KOREAN_COMMAND_NAMES = Object.freeze({
  help: "도움말",
  "render-file": "렌더-파일",
  "get-original-gif": "원본 GIF 받기",
  "channel-settings": "채널-설정",
  "server-settings": "서버-설정",
  finesse: "피네스",
  path: "경로",
  percent: "퍼센트",
  chance: "확률",
  minimals: "최소집합",
  score: "점수",
  "score-minimals": "최소집합-점수",
  saves: "세이브",
  "best-save": "최적-세이브",
  cover: "커버",
  setup: "셋업",
  congruent: "합동",
  "congruent-cover": "합동-커버",
  "setup-cover": "셋업-커버",
  "cover-percent": "커버-퍼센트",
  "special-cover": "특수-커버",
  "spin-cover": "스핀-커버",
  spin: "스핀",
  "score-finder": "score-finder",
  damage: "대미지",
  "spin-structure": "스핀-구조",
  "pc-setup": "pc-셋업",
  "best-setup": "최적-셋업",
  "dpc-finder": "dpc-탐색",
  verify: "검증",
});

const KOREAN_OPTION_NAMES = Object.freeze({
  arguments: "명령어",
  image: "이미지",
  next: "넥스트",
  field: "필드",
  lines: "줄",
  kicktable: "킥테이블",
  options: "옵션",
  pieces: "미노",
  profile: "프로필",
  base: "기존필드",
  target: "목표필드",
  remaining: "남은미노",
  priority: "셋업-정렬",
  "max-setup-pieces": "최대-구축-미노",
  "queue-knowledge": "큐-공개-범위",
  "next-cycle-remaining": "다음-회차-남은-미노",
  "setup-length": "셋업-길이",
  scope: "범위",
  language: "언어",
  document: "문서",
  "finesse.search": "탐색",
  "finesse.score": "계산",
  "channel-settings.language-show": "언어-확인",
  "channel-settings.language-set": "언어-설정",
  "channel-settings.language-reset": "언어-초기화",
  "channel-settings.disable": "비활성화",
  "channel-settings.enable": "활성화",
  "server-settings.language-show": "언어-확인",
  "server-settings.language-set": "언어-설정",
  "server-settings.language-reset": "언어-초기화",
  "server-settings.pause": "일시정지",
  "server-settings.resume": "재개",
});

const KOREAN_COMMAND_DESCRIPTIONS = Object.freeze({
  help: "Clearra 명령어의 정확한 문법과 제한을 표시합니다",
  "render-file": "최근 필드 미리보기의 원본 GIF 파일을 받습니다",
  "channel-settings": "현재 채널의 Clearra 설정을 관리합니다",
  "server-settings": "이 서버의 Clearra 설정을 관리합니다",
  finesse: "최소 입력 수로 미노 배치를 탐색하거나 계산합니다",
  path: "표현되는 모든 퍼펙트 클리어 경로를 찾습니다",
  percent: "정확한 퍼펙트 클리어 성공 확률을 계산합니다",
  chance: "정확한 퍼펙트 클리어 성공 확률을 계산합니다",
  minimals: "퍼펙트 클리어를 최소 집합으로 커버하는 해법을 찾습니다",
  score: "Jstris 프로필로 퍼펙트 클리어 해법의 점수를 계산합니다",
  "score-minimals": "최소 커버 퍼펙트 클리어 해법 집합의 점수를 계산합니다",
  saves: "각 PC 해법의 성공 확률을 분석합니다",
  "best-save": "각 PC 해법의 성공 확률을 분석합니다",
  cover: "기존 필드에서 목표 칸까지의 구축 확률을 계산합니다",
  setup: "목표 모양의 구축 확률을 계산합니다",
  congruent: "목표 모양의 구축 확률을 계산합니다",
  "congruent-cover": "목표 모양의 구축 확률을 계산합니다",
  "setup-cover": "목표 점유 필드의 구축 확률을 계산합니다",
  "cover-percent": "목표 점유 필드의 구축 확률을 계산합니다",
  "special-cover": "목표 모양의 T-spin 커버리지를 계산합니다",
  "spin-cover": "전방 T-spin 완성 경로를 찾습니다",
  spin: "전방 T-spin 완성 경로를 찾습니다",
  "score-finder": "고정 넥스트 큐에서 Jstris 점수가 가장 높은 퍼펙트 클리어를 찾습니다",
  damage: "정확한 넥스트 큐 하나에서 최대 대미지를 찾습니다",
  "spin-structure": "순서 없는 미노 목록에서 부분집합 최소 스핀 구조를 찾습니다",
  "pc-setup": "구축 및 PC 커버리지로 셋업 후보의 순위를 정합니다",
  "best-setup": "구축 커버리지로 셋업 후보의 순위를 정합니다",
  "dpc-finder": "퍼펙트 클리어 커버리지로 셋업 후보의 순위를 정합니다",
  verify: "Clearra 검증 항목 한 그룹을 실행합니다",
});

const KOREAN_COMMAND_NOTES = Object.freeze({
  percent: "/chance와 같은 기능입니다.",
  chance: "/percent와 같은 기능입니다.",
  saves: "/best-save와 같은 기능입니다.",
  "best-save": "/saves와 같은 기능입니다.",
  congruent: "/setup과 같은 기능입니다.",
  "congruent-cover": "/setup과 같은 목표 모양 계산을 사용합니다.",
  "setup-cover": "필드는 목표 점유 마스크를 나타냅니다.",
  "cover-percent": "필드는 목표 점유 마스크를 나타냅니다.",
  "spin-cover": "T-spin mini(TSM)는 지원하지 않습니다.",
  spin: "/spin-cover와 같은 계산을 사용하며 TSM은 지원하지 않습니다.",
  "pc-setup": "구축과 퍼펙트 클리어 커버리지를 함께 기준으로 삼습니다.",
  "best-setup": "구축 커버리지를 기준으로 삼습니다.",
  "dpc-finder": "퍼펙트 클리어 커버리지를 기준으로 삼습니다.",
});

const KOREAN_OPTION_DESCRIPTIONS = Object.freeze({
  arguments: "설명을 볼 명령어이며 생략하면 전체 명령어 그룹을 표시합니다",
  "render-file.image": "현재 채널의 Clearra 미리보기 메시지 링크 또는 ID이며 생략하면 최근 파일을 찾습니다",
  "score-finder.next": "정확한 IOTSZJL 큐이며 생략하면 입력 창에서 작성합니다",
  "score-finder.field": `1–${DISCORD_PC_FIELD_MAX_ROWS}줄 PC 필드: grid:줄/줄 또는 문서, 여러 줄은 입력 창 사용`,
  "score-finder.lines": `퍼펙트 클리어 목표 높이는 1–${DISCORD_PC_FIELD_MAX_ROWS}줄이며 기본값은 4줄입니다`,
  "score-finder.options": "초기 B2B 상태이며 기본값은 사용 안 함입니다",
  "damage.next": "정확한 IOTSZJL 큐이며 생략하면 입력 창에서 작성합니다",
  "damage.options": "hold, spin-profile, minimum-damage, initial-combo, initial-b2b, preserve-b2b 설정",
  "spin-structure.pieces": "순서 없는 IOTSZJL 미노 목록이며 반복 문자는 수량을 보존합니다",
  "spin-structure.field": `1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 필드: grid:줄/줄 또는 문서, 여러 줄은 입력 창 사용`,
  "spin-structure.lines": "마지막 스핀이 지우는 줄 수이며 기본값은 한 줄 이상입니다",
  "spin-structure.profile": "Regular와 Mini를 분리하는 스핀 판정 프로필입니다",
  "spin-structure.options": "fill-bottom, fill-top, max-placements, minimality 설정",
  next: "넥스트 큐 또는 패턴이며 생략하면 입력 창에서 작성합니다",
  field: "10열 필드: grid:줄/줄 또는 CTK3/Fumen/문서, 여러 줄은 입력 창 사용",
  lines: `PC 목표 높이 1–${DISCORD_PC_FIELD_MAX_ROWS}이며 생략하면 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 전체를 자동 판정합니다`,
  kicktable: "내장 킥테이블이며 기본값은 SRS+입니다",
  options: "추가 선택 설정",
  base: `기존 필드 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄: grid:줄/줄 또는 문서, 여러 줄은 입력 창 사용`,
  target: `목표 칸 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄: grid:줄/줄 또는 문서, 여러 줄은 입력 창 사용`,
  remaining: "순서 없는 IOTSZJL 미노 1–7개",
  priority: "셋업 후보를 구축 × PC 종합, 구축 확률 우선, PC 확률 우선 중 하나로 정렬합니다",
  "max-setup-pieces": "셋업 후보에 포함할 최대 미노 수는 1–10이며 기본값은 9입니다",
  "queue-knowledge": "셋업 순위 계산에 공개되는 큐 범위이며 기본값은 전체 미래 큐입니다",
  "next-cycle-remaining": "다음 회차에 남아야 하는 정확한 순서 없는 미노 목록입니다",
  "setup-length": "셋업 길이 선호도이며 자동은 선택한 정렬 기준을 따릅니다",
  "pc-setup.options": "mode, qb, post-cycle-borrow 셋업 설정",
  "best-setup.options": "mode, qb, post-cycle-borrow 셋업 설정",
  "dpc-finder.options": "mode, qb, post-cycle-borrow 셋업 설정",
  scope: "검증 범위",
  "verify.scope": "실행할 검증 그룹이며 생략하면 모든 검증을 실행합니다",
  language: "ClearraBot 응답과 입력 창에 사용할 언어",
  document: "각 페이지에 배치 operation이 하나씩 있는 CTK3 또는 v115 Fumen 문서",
  "finesse.search": "목표 필드까지의 최소 입력 구축 경로를 찾습니다",
  "finesse.score": "CTK3 또는 Fumen 배치 순서의 최소 입력 수를 계산합니다",
  "finesse.search.target": `추가할 목표 칸 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄: 정적 CTK3/Fumen/문서 링크 또는 grid:줄/줄`,
  "finesse.search.base": `시작 필드 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄: 정적 CTK3/Fumen/문서 링크 또는 grid:줄/줄`,
  "finesse.search.options": "hold, knowledge, source-pieces, aggregation, spin-profile, preserve-b2b 설정",
  "finesse.score.document": "모든 페이지에 배치 operation이 하나씩 있는 CTK3 또는 v115 Fumen 문서",
  "finesse.score.options": "hold, knowledge, source-pieces 설정",
  "channel-settings.language-show": "현재 채널에 적용되는 언어를 표시합니다",
  "channel-settings.language-set": "현재 채널의 응답 언어를 설정합니다",
  "channel-settings.language-reset": "현재 채널의 언어 설정을 삭제합니다",
  "channel-settings.disable": "현재 채널에서 Clearra 명령을 비활성화합니다",
  "channel-settings.enable": "현재 채널에서 Clearra 명령을 다시 활성화합니다",
  "server-settings.language-show": "이 서버에 적용되는 언어를 표시합니다",
  "server-settings.language-set": "이 서버의 응답 언어를 설정합니다",
  "server-settings.language-reset": "이 서버의 언어 설정을 삭제합니다",
  "server-settings.pause": "서버 재개를 제외한 모든 Clearra 명령을 비활성화합니다",
  "server-settings.resume": "이 서버의 Clearra 명령을 다시 활성화합니다",
});

export function assertDiscordRegistrationLimits(commands) {
  if (!Array.isArray(commands) || commands.length > 100) {
    throw new Error("Discord global command count exceeds the supported limit.");
  }
  for (const command of commands) {
    assertRegistrationNode(command, `/${command?.name ?? "unknown"}`, true);
  }
}

function assertRegistrationNode(node, path, command = false) {
  const name = String(node?.name ?? "");
  const maximumNameLength = command && node?.type === MESSAGE_COMMAND ? 32 : 32;
  if (name.length < 1 || name.length > maximumNameLength) {
    throw new Error(`${path} name exceeds Discord's 1–${maximumNameLength} character limit.`);
  }
  if (node?.description !== undefined) {
    const description = String(node.description);
    if (description.length < 1 || description.length > 100) {
      throw new Error(`${path} description exceeds Discord's 1–100 character limit.`);
    }
  }
  const options = node?.options ?? [];
  if (!Array.isArray(options) || options.length > 25) {
    throw new Error(`${path} exposes more than 25 Discord options.`);
  }
  for (const option of options) {
    const optionPath = `${path} ${option?.name ?? "unknown"}`;
    const choices = option?.choices ?? [];
    if (!Array.isArray(choices) || choices.length > 25) {
      throw new Error(`${optionPath} exposes more than 25 Discord choices.`);
    }
    for (const choice of choices) {
      if (String(choice?.name ?? "").length < 1 || String(choice.name).length > 100) {
        throw new Error(`${optionPath} has a choice name outside Discord's 1–100 character limit.`);
      }
      if (typeof choice?.value === "string" && choice.value.length > 100) {
        throw new Error(`${optionPath} has a string choice value longer than 100 characters.`);
      }
    }
    assertRegistrationNode(option, optionPath);
  }
}

const GLOBAL_COMMANDS = [
  ...slashCommandCatalog.map(localizedRegistration),
  ...messageCommandCatalog.map(localizedRegistration),
];
assertDiscordRegistrationLimits(GLOBAL_COMMANDS);
export const globalCommands = Object.freeze(GLOBAL_COMMANDS);
