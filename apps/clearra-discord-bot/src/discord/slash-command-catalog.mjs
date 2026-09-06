import { normalizeDiscordLocale } from "./i18n.mjs";
import {
  DISCORD_PC_FIELD_MAX_ROWS,
  DISCORD_WIDE_FIELD_MAX_ROWS,
} from "./slash-command-input.mjs";
import {
  activeDiscordGenericCompatibilityRoutes,
  activeDiscordSearchCapabilities,
  findProductCapability,
} from "./capability-registry.mjs";

// SRP rationale: this module has one behavior-level change reason: defining the
// complete localized Discord application-command registration metadata contract.

const SUB_COMMAND_OPTION = 1;
const STRING_OPTION = 3;
const INTEGER_OPTION = 4;
const ATTACHMENT_OPTION = 11;
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

const SEARCH_CAPABILITIES = Object.freeze(activeDiscordSearchCapabilities());
const GENERIC_COMPATIBILITY_ROUTES = Object.freeze(
  activeDiscordGenericCompatibilityRoutes(),
);

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

const HELP_COMMAND = governedCommand("meta.help", {
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

const CHANNEL_SETTINGS_COMMAND = governedCommand("settings.channel", {
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

const SERVER_SETTINGS_COMMAND = governedCommand("settings.server", {
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

function governedCommand(capabilityId, value) {
  const capability = findProductCapability(capabilityId);
  if (!capability || capability.status !== "active") {
    throw new Error(`Active catalog command lacks runtime authority: ${capabilityId}`);
  }
  const path = [capability.canonical.root, capability.canonical.subcommand]
    .filter(Boolean)
    .join(" ");
  if (value.name !== path) {
    throw new Error(`Catalog path drift for ${capabilityId}: ${value.name}`);
  }
  return Object.freeze({
    ...value,
    capabilityId,
    timeoutClass: capability.timeoutClass,
    telemetryIdentity: capability.telemetryIdentity,
    helpPolicy: capability.helpPolicy,
    i18nPolicy: capability.i18nPolicy,
  });
}

function mergeGenericPcCompatibilityRoutes(typedCommands, genericCommands) {
  const pathIndex = typedCommands.findIndex(({ name }) => name === "path");
  if (pathIndex < 0) {
    throw new Error("Generic PC compatibility routes require the legacy path route.");
  }
  return [
    ...typedCommands.slice(0, pathIndex + 1),
    ...genericCommands,
    ...typedCommands.slice(pathIndex + 1),
  ];
}

const CANONICAL_SEARCH_GROUPS = groupedCommands(
  SEARCH_CAPABILITIES.flatMap((entry) =>
    entry.canonical.slash && entry.canonical.subcommand
      ? [capabilityVariant(entry, entry.canonical)]
      : []
  ),
  false,
);

const COMPATIBILITY_SEARCH_GROUPS = groupedCommands(
  SEARCH_CAPABILITIES.flatMap((entry) =>
    entry.aliases
      .filter((route) => route.slash && route.subcommand)
      .map((route) => capabilityVariant(entry, route))
  ),
  true,
);

const DIRECT_CANONICAL_COMMANDS = Object.freeze(
  SEARCH_CAPABILITIES
    .filter((entry) => entry.canonical.slash && !entry.canonical.subcommand)
    .map((entry) => directCapabilityCommand(entry, entry.canonical)),
);

const SLASH_TYPED_COMPATIBILITY_COMMANDS = SEARCH_CAPABILITIES.flatMap((entry) =>
  entry.aliases
    .filter((route) => route.slash && !route.subcommand)
    .map((route) => directCapabilityCommand(entry, route))
);
const SLASH_GENERIC_COMPATIBILITY_COMMANDS = GENERIC_COMPATIBILITY_ROUTES
  .filter((route) => route.slash)
  .map(genericCompatibilityCommand);
const DIRECT_COMPATIBILITY_COMMANDS = Object.freeze(mergeGenericPcCompatibilityRoutes(
  SLASH_TYPED_COMPATIBILITY_COMMANDS,
  SLASH_GENERIC_COMPATIBILITY_COMMANDS,
));

const TEXT_TYPED_COMPATIBILITY_COMMANDS = SEARCH_CAPABILITIES.flatMap((entry) =>
  entry.aliases
    .filter((route) => route.text && !route.subcommand)
    .map((route) => directCapabilityCommand(entry, route))
);
const TEXT_GENERIC_COMPATIBILITY_COMMANDS = GENERIC_COMPATIBILITY_ROUTES
  .filter((route) => route.text)
  .map(genericCompatibilityCommand);
const TEXT_DIRECT_COMMANDS = Object.freeze(mergeGenericPcCompatibilityRoutes(
  TEXT_TYPED_COMPATIBILITY_COMMANDS,
  TEXT_GENERIC_COMPATIBILITY_COMMANDS,
));

const TEXT_DIRECT_COMMANDS_BY_NAME = new Map(
  TEXT_DIRECT_COMMANDS.map((entry) => [entry.name, entry]),
);

export const representedSfinderCommandNames = Object.freeze(
  TEXT_DIRECT_COMMANDS
    .filter((entry) => entry.argvPrefix[0] === "sfinder")
    .map(({ name }) => name),
);

export const slashCommandCatalog = Object.freeze([
  HELP_COMMAND,
  RENDER_FILE_COMMAND,
  CHANNEL_SETTINGS_COMMAND,
  SERVER_SETTINGS_COMMAND,
  ...CANONICAL_SEARCH_GROUPS,
  ...DIRECT_CANONICAL_COMMANDS,
  ...COMPATIBILITY_SEARCH_GROUPS,
  ...DIRECT_COMPATIBILITY_COMMANDS,
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

// Text ingress has a wider, explicitly curated surface than slash ingress.
// In particular, hidden verification and aliases shadowed by a canonical root
// are resolved here without leaking them into command registration or help.
export function findTextCommand(name) {
  if (typeof name !== "string") return null;
  return COMMANDS_BY_NAME.get(name) ?? TEXT_DIRECT_COMMANDS_BY_NAME.get(name) ?? null;
}

export function findShadowedTextCommand(name) {
  if (typeof name !== "string") return null;
  const direct = TEXT_DIRECT_COMMANDS_BY_NAME.get(name) ?? null;
  return COMMANDS_BY_NAME.get(name)?.subcommands ? direct : null;
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
  if (!command?.subcommands) {
    return Object.freeze({ command, rawOptions });
  }
  if (!Array.isArray(rawOptions) || rawOptions.length !== 1) {
    throw new Error(`/${command.name} requires exactly one subcommand.`);
  }
  const selected = rawOptions[0];
  if (selected?.type !== SUB_COMMAND_OPTION || typeof selected.name !== "string") {
    throw new Error(`/${command.name} requires one registered subcommand.`);
  }
  const variant = command.subcommands[selected.name];
  if (!variant) {
    throw new Error(`/${command.name} does not support subcommand '${selected.name}'.`);
  }
  if (selected.options !== undefined && !Array.isArray(selected.options)) {
    throw new Error(`Discord supplied invalid /${command.name} subcommand options.`);
  }
  return Object.freeze({
    command: variant,
    rawOptions: selected.options ?? [],
  });
}

export function formatSlashCommandHelp(requestedName, locale = "en") {
  const language = normalizeDiscordLocale(locale);
  const objectiveTarget = normalizeObjectiveHelpTarget(requestedName);
  if (objectiveTarget === "objective") return objectiveHelp(language);
  if (objectiveTarget?.startsWith("objective ")) {
    return objectiveOptionHelp(objectiveTarget.slice("objective ".length), language);
  }
  const normalized = normalizeHelpTarget(requestedName);
  if (!normalized) return commandListHelp(language);
  const entry = findHelpCommand(normalized);
  if (!entry || !["search", "render-file"].includes(entry.kind)) {
    return language === "ko"
      ? `알 수 없는 Clearra 명령어 \`${requestedName}\`입니다. \`/help\`에서 명령어 목록을 확인하세요.`
      : `Unknown Clearra command \`${requestedName}\`. Use \`/help\` to list commands.`;
  }
  if (entry.subcommands) return searchGroupHelp(entry, language);
  const lines = [
    `**/${commandPath(entry)}** — ${localizedCommandDescription(entry, language)}`,
    language === "ko"
      ? `직접 입력 문법: \`${syntax(entry, language)}\``
      : `Direct syntax: \`${syntax(entry)}\``,
  ];
  if (entry.modalSchemaId !== null) {
    lines.push(language === "ko"
      ? "필수 입력을 모두 넣지 않고 명령어를 실행하면 안내 입력 창이 열립니다."
      : "Invoke the command without all required inputs to open its guided Modal form.");
  }
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

function objectiveHelp(locale) {
  if (locale === "ko") {
    return [
      "**고급 objective**",
      "PC objective는 slash 입력·Modal·autocomplete에 나타나지 않는 텍스트/CLI 고급 경로입니다. Build v2는 capability별로 닫힌 objective 선택지만 slash에 노출합니다.",
      "현재 PC objective: `all`, `unique`, `min-cover`, `tiling`",
      "Discord 텍스트 문법: `$path <필드> <큐> [줄] --objective <ID>` 또는 `>path ...`; `minimum-cover`만 `min-cover`의 호환 별칭입니다.",
      "각 문법은 `/help arguments:objective <이름>`으로 확인하세요.",
    ].join("\n");
  }
  return [
    "**Advanced objective**",
    "PC objectives are intentionally absent from slash options, Modals, and autocomplete; Build v2 exposes only capability-closed slash objective choices.",
    "Current PC objectives: `all`, `unique`, `min-cover`, `tiling`",
    "Discord text syntax: `$path <field> <queue> [lines] --objective <ID>` or `>path ...`; only `minimum-cover` is accepted as a compatibility alias for `min-cover`.",
    "Use `/help arguments:objective <name>` for one grammar.",
  ].join("\n");
}

function objectiveOptionHelp(name, locale) {
  const normalized = String(name).trim().toLowerCase();
  const canonical = ({
    all: "all",
    unique: "unique",
    "min-cover": "min-cover",
    "minimum-cover": "min-cover",
    tiling: "tiling",
  })[normalized];
  if (!canonical) {
    return locale === "ko"
      ? `알 수 없는 objective \`${name}\`입니다. \`/help arguments:objective\`에서 목록을 확인하세요.`
      : `Unknown objective \`${name}\`. Use \`/help arguments:objective\` to list objective kinds.`;
  }
  const meaning = locale === "ko"
    ? ({
      all: "모든 실행 가능한 PC 해법을 유지합니다.",
      unique: "동일한 최종 해법을 정규화해 하나씩 유지합니다.",
      "min-cover": "큐 우주를 커버하는 최소 해법 집합을 계산합니다.",
      tiling: "도달성·점수·B2B를 제외한 정확한 기하 타일링만 열거합니다.",
    })[canonical]
    : ({
      all: "Retains every executable PC solution.",
      unique: "Keeps one canonical representative of each final solution.",
      "min-cover": "Calculates a minimum solution family covering the queue universe.",
      tiling: "Enumerates exact geometry tilings without reachability, scoring, or B2B semantics.",
    })[canonical];
  return [
    `**objective ${canonical}**`,
    meaning,
    locale === "ko"
      ? `Discord 텍스트 문법: \`$path <필드> <큐> [줄] --objective ${canonical}\` 또는 \`>path ...\``
      : `Discord text syntax: \`$path <field> <queue> [lines] --objective ${canonical}\` or \`>path ...\``,
    locale === "ko"
      ? `CLI 문법: \`clearra pc --objective ${canonical} ...\``
      : `CLI syntax: \`clearra pc --objective ${canonical} ...\``,
  ].join("\n");
}

function findHelpCommand(normalized) {
  const parts = normalized.split(/\s+/u).filter(Boolean);
  if (parts.length === 2) {
    return findSlashCommand(parts[0])?.subcommands?.[parts[1]] ?? null;
  }
  const compact = normalized.replace(/[ /]+/g, "-");
  if (["finesse-search", "finesse-score"].includes(compact)) {
    return findSlashCommand("finesse")?.subcommands?.[
      compact.slice("finesse-".length)
    ] ?? null;
  }
  return findSlashCommand(normalized);
}

function searchGroupHelp(entry, locale) {
  const korean = locale === "ko";
  const subcommands = Object.values(entry.subcommands);
  const lines = [
    `**/${entry.name}** — ${localizedCommandDescription(entry, locale)}`,
    korean ? "하위 명령어:" : "Subcommands:",
    ...subcommands.map((variant) =>
      `- \`/${entry.name} ${variant.subcommand}\` — ${localizedCommandDescription(variant, locale)}`
    ),
    korean
      ? `각 입력 계약은 \`/help arguments:${entry.name} <하위-명령어>\`로 확인하세요.`
      : `Use \`/help arguments:${entry.name} <subcommand>\` for each exact input contract.`,
  ];
  return lines.join("\n");
}

function commandPath(entry) {
  return entry.subcommand
    ? `${entry.rootName ?? entry.registration.name} ${entry.subcommand}`
    : entry.name;
}

export function localizedSlashCommandName(name, locale = "en") {
  const command = findSlashCommand(name);
  if (!command) return String(name ?? "");
  return normalizeDiscordLocale(locale) === "ko"
    ? KOREAN_COMMAND_NAMES[command.name] ?? command.name
    : command.name;
}

function capabilityVariant(capability, route) {
  const publicResultKind = route.publicResultKind ?? capability.publicResultKind;
  const resultAuthorityId = route.resultAuthorityId ?? publicResultKind;
  const input = route.input ?? capability.engine.input;
  const problemContractId = route.problemContractId ?? capability.problemContractId;
  const effectClasses = route.effectClasses ?? capability.effectClasses;
  const resultContractId = route.resultContractId ?? capability.resultContractId;
  const resultAllowlist = route.engineKinds ?? capability.engineKinds;
  return Object.freeze({
    name: publicResultKind,
    rootName: route.root,
    subcommand: route.subcommand,
    kind: "search",
    group: capability.problemFamily,
    input,
    description: capability.description,
    note: capability.note,
    capabilityId: capability.id,
    problemContractId,
    algorithmFamily: capability.algorithmFamily,
    timeoutClass: capability.timeoutClass,
    effectClasses,
    helpPolicy: capability.helpPolicy,
    i18nPolicy: capability.i18nPolicy,
    resultAllowlist,
    telemetryIdentity: capability.telemetryIdentity,
    loweringAuthority: capability.loweringAuthority,
    compatibilityClassification: route.classification,
    compatibilityPreset: route.preset,
    removeIn: route.deprecateAfter,
    compatibilityLifetime: route.lifetime,
    inputSchemaId: route.inputSchemaId ?? capability.inputSchemaId,
    modalSchemaId: Object.hasOwn(route, "modalSchemaId")
      ? route.modalSchemaId
      : capability.modalSchemaId,
    resultContractId,
    publicResultKind,
    resultAuthorityId,
    argvPrefix: Object.freeze([
      ...(route.argvPrefix ?? capability.engine.argvPrefix),
    ]),
    ...(capability.engine?.setupPriority
      ? { setupPriority: capability.engine.setupPriority }
      : {}),
    ...(capability.engine?.pcObjective
      ? { pcObjective: capability.engine.pcObjective }
      : {}),
    registration: Object.freeze({
      name: route.root,
      description: capability.description,
      options: registrationOptions(input, capability.id),
    }),
  });
}

function genericCompatibilityCommand(route) {
  return Object.freeze({
    name: route.root,
    rootName: undefined,
    subcommand: null,
    kind: "search",
    group: route.problemFamily,
    input: route.input,
    description: route.description,
    note: null,
    capabilityId: route.id,
    problemContractId: route.problemContractId,
    algorithmFamily: route.algorithmFamily,
    timeoutClass: route.timeoutClass,
    effectClasses: route.effectClasses,
    helpPolicy: route.helpPolicy,
    i18nPolicy: route.i18nPolicy,
    resultAllowlist: route.engineKinds,
    telemetryIdentity: route.telemetryIdentity,
    loweringAuthority: route.loweringAuthority,
    compatibilityClassification: route.classification,
    compatibilityPreset: null,
    removeIn: route.removeIn,
    compatibilityLifetime: route.lifetime,
    inputSchemaId: route.inputSchemaId,
    modalSchemaId: route.modalSchemaId,
    resultContractId: route.resultContractId,
    publicResultKind: route.publicResultKind,
    resultAuthorityId: route.resultAuthorityId,
    argvPrefix: route.argvPrefix,
    registration: Object.freeze({
      name: route.root,
      description: route.description,
      options: registrationOptions(route.input, route.id),
    }),
  });
}

function directCapabilityCommand(capability, route) {
  const variant = capabilityVariant(capability, route);
  const publicResultKind = route.publicResultKind ?? route.root;
  const resultAuthorityId = route.resultAuthorityId ?? publicResultKind;
  return Object.freeze({
    ...variant,
    name: route.root,
    rootName: undefined,
    subcommand: null,
    publicResultKind,
    resultAuthorityId,
    registration: Object.freeze({
      name: route.root,
      description: capability.description,
      options: registrationOptions(variant.input, capability.id),
    }),
  });
}

function groupedCommands(variants, compatibility) {
  const byRoot = new Map();
  for (const variant of variants) {
    const group = byRoot.get(variant.rootName) ?? [];
    group.push(variant);
    byRoot.set(variant.rootName, group);
  }
  return Object.freeze([...byRoot].map(([root, entries]) => {
    const subcommands = Object.freeze(Object.fromEntries(
      entries.map((entry) => [entry.subcommand, entry]),
    ));
    const description = searchGroupDescription(root, compatibility);
    return Object.freeze({
      name: root,
      kind: "search",
      input: "group",
      group: root,
      description,
      subcommands,
      registration: Object.freeze({
        name: root,
        description,
        options: Object.freeze(entries.map((entry) => Object.freeze({
          type: SUB_COMMAND_OPTION,
          name: entry.subcommand,
          description: entry.description,
          options: entry.registration.options,
        }))),
      }),
    });
  }));
}

function searchGroupDescription(root, compatibility) {
  if (compatibility && root === "finesse") {
    return "Compatibility routes for minimum-input build calculations";
  }
  return ({
    pc: "Search fixed clear-to-empty perfect-clear objectives",
    build: "Search or evaluate target-build objectives",
    setup: "Rank setup candidates under explicit observation policies",
    forward: "Run ordered-queue forward state searches",
    "spin-structure": "Search unordered no-hold structural spin objectives",
    utility: "Run bounded stateless field and document utilities",
  })[root] ?? `Run ${root} calculations`;
}

function registrationOptions(input, capabilityId = null) {
  switch (input) {
    case "pc":
      return Object.freeze([
        nextOption(false),
        fieldOption(input),
        linesOption(),
        kicktableOption(),
        settingsOption(input),
      ]);
    case "pc-v2":
      return Object.freeze([
        nextOption(false),
        fieldOption(input),
        linesOption(),
        pcHoldOption(),
        kicktableOption(true),
        pcQueueKnowledgeOption(),
        spinProfileOption(false),
        onOffOption("preserve-b2b", "Require a solution that preserves back-to-back", "off"),
        onOffOption("solution-probabilities", "Include exact per-solution probabilities", "off"),
      ]);
    case "pc-path-v2":
      return Object.freeze([
        nextOption(false),
        fieldOption(input),
        linesOption(),
        pcHoldOption(),
        kicktableOption(true),
        spinProfileOption(false),
        onOffOption("preserve-b2b", "Require a path that preserves back-to-back", "off"),
      ]);
    case "pc-chance-v2":
      return Object.freeze([
        nextOption(false),
        fieldOption(input),
        linesOption(),
        pcHoldOption(),
        kicktableOption(true),
      ]);
    case "pc-save-v2":
      return Object.freeze([
        nextOption(false),
        fieldOption(input),
        linesOption(),
        pcHoldOption(),
        kicktableOption(true),
      ]);
    case "pc-allspin-exact-v1":
      return allspinRegistrationOptions(true);
    case "pc-allspin-pattern-v1":
      return allspinRegistrationOptions(false);
    case "pc-score-v2":
      return Object.freeze(["pc.score", "pc.score-minimals"].includes(capabilityId) ? [
        nextOption(false),
        fieldOption(input),
        linesOption(),
        pcHoldOption(),
        kicktableOption(true),
        pcScoreProfileOption(),
        spinProfileOption(false),
        boundedIntegerOption("initial-b2b", "Initial back-to-back chain used by scoring", 0, 65_535),
      ] : [
        nextOption(false),
        fieldOption(input),
        linesOption(),
        pcHoldOption(),
        kicktableOption(true),
        pcQueueKnowledgeOption(),
        pcScoreProfileOption(),
        spinProfileOption(false),
        onOffOption("preserve-b2b", "Require a scored solution that preserves back-to-back", "off"),
        boundedIntegerOption("initial-b2b", "Initial back-to-back chain used by scoring", 0, 65_535),
        onOffOption("solution-probabilities", "Include exact per-solution probabilities", "off"),
      ]);
    case "pc-tiling-v2":
      return Object.freeze([
        nextOption(false),
        fieldOption(input),
        linesOption(),
        pcHoldOption(),
      ]);
    case "pc-failed-v2":
      return Object.freeze([
        nextOption(false),
        fieldOption(input),
        linesOption(),
        pcHoldOption(),
        kicktableOption(true),
        pcQueueKnowledgeOption(),
        spinProfileOption(false),
        onOffOption("preserve-b2b", "Require failed-queue evaluation to preserve back-to-back", "off"),
        boundedIntegerOption("failed-count", "Maximum failed patterns returned", 1, 4_294_967_295),
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
    case "build-cover":
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
        kicktableOption(true),
        heightOption("Search height; defaults to at least 8 and never below the supplied fields"),
        buildHoldOption(),
        boundedIntegerOption(
          "source-pieces",
          "Exact source-piece window; omitted derives it from target pieces and initial hold",
          1,
          4_294_967_295,
        ),
        aggregationOption(),
        buildResultModeOption(),
        spinProfileOption(false),
        onOffOption("preserve-b2b", "Require a solution that preserves back-to-back", "off"),
        onOffOption("solution-probabilities", "Include exact per-solution probabilities", "off"),
        buildFinesseOption(),
        buildFinesseKnowledgeOption(),
        buildMirrorOption(),
        pcScoreProfileOption(),
        boundedIntegerOption("initial-b2b", "Initial back-to-back chain used by score result modes", 0, 65_535),
        boundedIntegerOption("failed-count", "Maximum failed Build patterns returned", 1, 4_294_967_295),
      ]);
    case "build-v2-cover":
      return Object.freeze([
        buildV2MaskOption("base-mask", "Canonical base board as decimal or 0x-prefixed hexadecimal"),
        buildV2MaskOption("target-mask", "Canonical target cells as decimal or 0x-prefixed hexadecimal"),
        requiredBoundedIntegerOption(
          "height",
          `Visible board height from 1 through ${DISCORD_PC_FIELD_MAX_ROWS}`,
          1,
          DISCORD_PC_FIELD_MAX_ROWS,
        ),
        buildV2SupplyOption("queue", "One exact IOTSZJL queue; mutually exclusive with patterns"),
        buildV2SupplyOption("patterns", "One queue-pattern language; mutually exclusive with queue"),
        buildHoldOption(),
        pcQueueKnowledgeOption(),
        buildV2ObjectiveOption(capabilityId),
        kicktableOption(true),
        boundedIntegerOption(
          "source-pieces",
          "Exact source-piece count; available only on build cover",
          1,
          4_294_967_295,
        ),
      ]);
    case "build-v2-target":
      return Object.freeze([
        buildV2DocumentFormatOption("target-format", "Format of the nominal colored target document"),
        buildV2DocumentOption(
          "target-document",
          "Canonical colored target CTK3 or v115 Fumen; plain grids and gray targets are rejected",
        ),
        buildV2SupplyOption("queue", "One exact IOTSZJL queue; mutually exclusive with patterns"),
        buildV2SupplyOption("patterns", "One queue-pattern language; mutually exclusive with queue"),
        buildHoldOption(),
        pcQueueKnowledgeOption(),
        buildV2ObjectiveOption(capabilityId),
        kicktableOption(true),
        ...buildV2ScoreOptions(capabilityId),
      ]);
    case "build-v2-supplied":
      return Object.freeze([
        buildV2DocumentFormatOption("solution-format", "Format of the supplied solution document"),
        buildV2DocumentOption(
          "solution-document",
          "Canonical colored solution CTK3 or v115 Fumen; a nominal target is not accepted",
        ),
        buildV2SupplyOption("queue", "One exact IOTSZJL queue; mutually exclusive with patterns"),
        buildV2SupplyOption("patterns", "One queue-pattern language; mutually exclusive with queue"),
        buildHoldOption(),
        pcQueueKnowledgeOption(),
        buildV2ObjectiveOption(capabilityId),
        kicktableOption(true),
        ...buildV2ScoreOptions(capabilityId),
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
    case "forward-spin-v2":
      return Object.freeze([
        nextOption(false),
        fieldOption(input),
        heightOption("Visible search height; defaults to at least 8 and includes the supplied field"),
        onOffOption("hold", "Whether hold is available during the ordered search", "on"),
        kicktableOption(true),
        spinProfileOption(false),
        forwardSpinLinesOption(),
        forwardSpinCategoryOption(),
        boundedIntegerOption("initial-combo", "Initial combo chain; zero means no active combo", 0, 65_535),
        boundedIntegerOption("initial-b2b", "Initial back-to-back chain", 0, 65_535),
        onOffOption("preserve-b2b", "Require the selected result to preserve back-to-back", "off"),
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
    case "forward-damage-v2":
      return Object.freeze([
        nextOption(true),
        fieldOption(input),
        heightOption("Visible search height; defaults to at least 8 and includes the supplied field"),
        onOffOption("hold", "Whether hold is available during damage search", "on"),
        kicktableOption(true),
        spinProfileOption(true),
        damageModeOption(),
        boundedIntegerOption("minimum-damage", "Required damage in at-least mode", 0, 4_294_967_295),
        boundedIntegerOption("initial-combo", "Initial combo chain; zero means no active combo", 0, 65_535),
        boundedIntegerOption("initial-b2b", "Initial back-to-back chain", 0, 65_535),
        onOffOption("preserve-b2b", "Require the selected result to preserve back-to-back", "off"),
      ]);
    case "forward-ren-v1":
      return Object.freeze([
        nextOption(true),
        fieldOption(input),
        heightOption("Visible search height; defaults to at least 8 and includes the supplied field"),
        onOffOption("hold", "Whether hold is available during exact REN search", "on"),
        kicktableOption(true),
      ]);
    case "score-fixed-next":
      return Object.freeze([
        nextOption(true),
        fieldOption(input),
        catLinesOption(),
        kicktableOption(),
        catSettingsOption(),
      ]);
    case "score-fixed-next-v2":
      return Object.freeze([
        nextOption(true),
        fieldOption(input),
        catLinesOption(),
        kicktableOption(),
        onOffOption("initial-b2b", "Initial back-to-back state used by Jstris scoring", "off"),
      ]);
    case "pc-score-finder-v2":
      return Object.freeze([
        nextOption(true),
        fieldOption(input),
        linesOption(),
        pcHoldOption(),
        kicktableOption(true),
        boundedIntegerOption("initial-b2b", "Initial back-to-back state fixed to zero or one", 0, 1),
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
    case "setup-v2":
      return Object.freeze([
        stringOption(
          "remaining",
          "Unordered 1–7-piece inventory; only IOTSZJL, with at most one duplicate kind",
          false,
          64,
        ),
        setupModeOption(),
        stringOption("qb", "Observed next-bag pieces; required only in QB mode", false, 64),
        setupQueueKnowledgeOption(),
        stringOption(
          "next-cycle-remaining",
          "Exact next-cycle residue; required count depends on the current remaining inventory",
          false,
          64,
        ),
        onOffOption("post-cycle-borrow", "Allow one next-cycle borrow when the residue permits it", "off"),
        setupLengthOption(),
        setupMaximumPiecesOption(),
        kicktableOption(true),
      ]);
    case "setup-score-v1":
      return Object.freeze([
        buildV2DocumentFormatOption(
          "document-format",
          "Format of the canonical setup candidate document",
        ),
        buildV2DocumentOption(
          "document",
          "Canonical CTK3 or v115 Fumen setup candidate document",
        ),
        buildV2SupplyOption("setup-queue", "One exact setup queue; mutually exclusive with setup-patterns"),
        buildV2SupplyOption("setup-patterns", "One setup queue-pattern language; mutually exclusive with setup-queue"),
        buildV2SupplyOption("solution-queue", "One exact continuation queue; mutually exclusive with solution-patterns"),
        buildV2SupplyOption("solution-patterns", "One continuation queue-pattern language; mutually exclusive with solution-queue"),
        boundedIntegerOption("clear", "Perfect-clear target height", 1, DISCORD_PC_FIELD_MAX_ROWS),
        onOffOption("hold", "Whether hold is available while building the setup", "on"),
        pcScoreProfileOption(),
        boundedIntegerOption("initial-b2b", "Initial back-to-back chain used by scoring", 0, 4_294_967_295),
        kicktableOption(true),
        boundedIntegerOption("max-patterns", "Maximum materialized setup and continuation patterns", 1, 4_294_967_295),
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
    case "spin-structure-v2":
      return Object.freeze([
        stringOption(
          "pieces",
          "Unordered IOTSZJL inventory; repeated letters preserve multiplicity",
          false,
          64,
        ),
        fieldOption(input),
        heightOption("Structural search height; defaults to at least 8 and includes the supplied field"),
        spinStructureLinesOption(),
        spinProfileOption(false),
        kicktableOption(true),
        boundedIntegerOption("fill-bottom", "Inclusive lower row bound for the structure region", 0, 23),
        boundedIntegerOption("fill-top", "Exclusive upper row bound for the structure region", 1, 24),
        boundedIntegerOption("max-placements", "Maximum placements retained from the supplied inventory", 1, 64),
        minimalityOption(),
      ]);
    case "spin-structure-cover-v1":
    case "spin-structure-guaranteed-v1":
      return Object.freeze([
        stringOption(
          "pieces",
          "Unordered IOTSZJL inventory; repeated letters preserve multiplicity",
          false,
          64,
        ),
        fieldOption(input),
        heightOption("Structural search height; defaults to at least 8 and includes the supplied field"),
        spinStructureLinesOption(),
        spinProfileOption(false),
        choiceOption(
          "kicktable",
          "Built-in structural rule profile; defaults to SRS+",
          [["SRS+ (default)", "srs-plus"], ["SRS", "srs"]],
        ),
        boundedIntegerOption("fill-bottom", "Inclusive lower row bound for the structure region", 0, 23),
        boundedIntegerOption("fill-top", "Exclusive upper row bound for the structure region", 1, 24),
        boundedIntegerOption("max-placements", "Maximum placements retained from the supplied inventory", 1, 64),
        minimalityOption(),
        ...(input === "spin-structure-guaranteed-v1"
          ? [stringOption(
              "final-piece",
              "Piece that must be the guaranteed final placement; defaults to T",
              false,
              1,
            )]
          : []),
        boundedIntegerOption(
          "max-patterns",
          "Maximum patterns retained by the exact structure calculation",
          1,
          100_000,
        ),
        ...(input === "spin-structure-guaranteed-v1"
          ? [onOffOption(
              "dependency-report",
              "Whether to calculate the optional dependency report",
              "off",
            )]
          : []),
      ]);
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
    case "finesse-score-v2":
      return Object.freeze([
        stringOption(
          "document",
          "CTK3 or v115 Fumen whose every page contains one placement operation",
          false,
          DOCUMENT_MAX_LENGTH,
        ),
        nextOption(false),
        kicktableOption(true),
        buildHoldOption(),
        finesseKnowledgeOption(),
        boundedIntegerOption(
          "source-pieces",
          "Maximum number of source pieces consumed from each materialized queue",
          1,
          128,
        ),
      ]);
    case "operation-document-v1":
      return Object.freeze([
        stringOption(
          "document",
          "CTK3 or v115 Fumen with one concrete locked operation on every page",
          false,
          DOCUMENT_MAX_LENGTH,
        ),
        attachmentOption(
          "attachment",
          "One CTK3 operation-document attachment; do not also set document",
        ),
        choiceOption(
          "rule-profile",
          "Built-in movement rule profile; defaults to SRS+",
          NATIVE_BUILTIN_KICKTABLES.map(({ name, value }) => [name, value]),
        ),
        choiceOption(
          "kick-profile",
          "Built-in kick profile; defaults to SRS+",
          NATIVE_BUILTIN_KICKTABLES.map(({ name, value }) => [name, value]),
        ),
        boundedIntegerOption(
          "timeout-seconds",
          "Exact-analysis timeout in seconds; defaults to 900",
          1,
          900,
        ),
      ]);
    case "field-document-v1":
      return Object.freeze([
        stringOption(
          "document",
          "One canonical CTK3 or v115 Fumen field document",
          false,
          DOCUMENT_MAX_LENGTH,
        ),
        attachmentOption(
          "attachment",
          "One UTF-8 CTK3 or v115 Fumen document; do not also set document",
        ),
      ]);
    case "fumen-transform-v1":
      return Object.freeze([
        choiceOption(
          "transform",
          "Closed lossless Fumen transform",
          [
            ["Roundtrip", "roundtrip"],
            ["Combine", "combine"],
            ["Split", "split"],
            ["Get page", "get-page"],
            ["Page shift", "page-shift"],
            ["Clean comments", "clean-comments"],
            ["Preserve comments", "preserve-comments"],
            ["To gray", "to-gray"],
            ["Mirror", "mirror"],
            ["Text to Fumen", "text-to-fumen"],
          ],
        ),
        stringOption(
          "document",
          "One canonical v115 Fumen document for a single-document transform",
          false,
          DOCUMENT_MAX_LENGTH,
        ),
        attachmentOption(
          "attachment",
          "UTF-8 v115 Fumen file; combine accepts one canonical document per line",
        ),
        stringOption(
          "documents",
          "Combine input: one canonical v115 Fumen document per non-empty line",
          false,
          DOCUMENT_MAX_LENGTH,
        ),
        boundedIntegerOption(
          "page",
          "One-based page number required only by get-page",
          1,
          4096,
        ),
        boundedIntegerOption(
          "offset",
          "Signed left-rotation offset required only by page-shift",
          -4096,
          4096,
        ),
        stringOption(
          "comments",
          "Text-to-Fumen input: one bounded Unicode comment per line",
          false,
          DOCUMENT_MAX_LENGTH,
        ),
      ]);
    case "render-document-v1":
      return Object.freeze([
        stringOption(
          "document",
          "One canonical CTK3 or v115 Fumen field document",
          false,
          DOCUMENT_MAX_LENGTH,
        ),
        attachmentOption(
          "attachment",
          "One UTF-8 CTK3 or v115 Fumen document; do not also set document",
        ),
        choiceOption(
          "artifact-format",
          "Exact Rust-rendered artifact format",
          [["PNG page", "png"], ["GIF timeline", "gif"]],
        ),
        boundedIntegerOption(
          "page",
          "One-based selected page; valid only for PNG",
          1,
          4096,
        ),
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

function allspinRegistrationOptions(exactQueue) {
  return Object.freeze([
    nextOption(exactQueue),
    fieldOption(exactQueue ? "pc-allspin-exact-v1" : "pc-allspin-pattern-v1"),
    linesOption(),
    onOffOption("hold", "Whether hold is available during B2B-preserving PC search", "on"),
    kicktableOption(true),
    spinProfileOption(false),
    boundedIntegerOption("max-patterns", "Maximum materialized queue patterns", 1, 4_294_967_295),
    boundedIntegerOption("max-nodes", "Maximum search nodes before returning an incomplete result", 1, 4_294_967_295),
    boundedIntegerOption("max-frontier-states", "Maximum retained frontier states", 1, 4_294_967_295),
    boundedIntegerOption("max-candidates", "Maximum B2B-preserving candidates", 1, 4_294_967_295),
    boundedIntegerOption("max-memory-mib", "Maximum search memory in MiB", 1, 1_048_576),
  ]);
}

function fieldOption(input) {
  const description = ["pc", "pc-v2", "pc-path-v2", "pc-chance-v2", "pc-save-v2", "pc-score-v2", "pc-score-finder-v2", "pc-tiling-v2", "pc-failed-v2", "pc-allspin-exact-v1", "pc-allspin-pattern-v1", "score-fixed-next", "score-fixed-next-v2"].includes(input)
    ? `PC field (1–${DISCORD_PC_FIELD_MAX_ROWS} rows): CTK3/Fumen/URL or grid:row/row; omit for multiline form`
    : input === "colored"
      ? `Target (1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows): CTK3/Fumen/URL or grid:row/row; omit for multiline form`
      : `Field (1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows): CTK3/Fumen/URL or grid:row/row; omit for multiline form`;
  return boardOption("field", description);
}

function pcHoldOption() {
  return buildHoldOption();
}

function pcQueueKnowledgeOption() {
  return choiceOption(
    "queue-knowledge",
    "Queue observation policy; Oracle is the full future queue",
    [["Full queue oracle (default)", "oracle"], ["Visible 7 pieces", "visible-7"]],
  );
}

function pcScoreProfileOption() {
  return choiceOption(
    "score-profile",
    "Scoring profile; canonical PC scoring defaults to TETR.IO",
    [
      ["TETR.IO (default)", "tetrio"],
      ["Guideline", "guideline"],
      ["Jstris Ultra", "jstris-ultra"],
    ],
  );
}

function finesseKnowledgeOption() {
  return choiceOption(
    "knowledge",
    "Queue knowledge used by fixed-placement finesse scoring",
    [
      ["Both queue classes (default)", "both"],
      ["Full queue oracle", "oracle"],
      ["Visible 7 pieces", "visible-7"],
    ],
  );
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

function heightOption(description) {
  return boundedIntegerOption(
    "height",
    description,
    1,
    DISCORD_WIDE_FIELD_MAX_ROWS,
  );
}

function boundedIntegerOption(name, description, minimum, maximum) {
  return Object.freeze({
    type: INTEGER_OPTION,
    name,
    description,
    required: false,
    min_value: minimum,
    max_value: maximum,
  });
}

function requiredBoundedIntegerOption(name, description, minimum, maximum) {
  return Object.freeze({
    ...boundedIntegerOption(name, description, minimum, maximum),
    required: true,
  });
}

function onOffOption(name, description, defaultValue) {
  return choiceOption(name, description, [
    [`On${defaultValue === "on" ? " (default)" : ""}`, "on"],
    [`Off${defaultValue === "off" ? " (default)" : ""}`, "off"],
  ]);
}

function spinProfileOption(includeDisabled) {
  const choices = [
    ["T-Spins", "t-spins"],
    ["T-Spins+", "t-spins-plus"],
    ["All-Spin", "all-spin"],
    ["All-Spin+", "all-spin-plus"],
    ["All-Mini", "all-mini"],
    ["All-Mini+", "all-mini-plus"],
  ];
  if (includeDisabled) choices.unshift(["Disabled", "disabled"]);
  return choiceOption(
    "spin-profile",
    "Spin recognition profile used by scoring and objective filters",
    choices,
  );
}

function aggregationOption() {
  return choiceOption(
    "aggregation",
    "Primary Build result: buildability, spin coverage, or geometry-only tiling",
    [
      ["Buildability (default)", "buildability"],
      ["Spin coverage", "spin"],
      ["Geometry-only tiling", "tiling"],
    ],
  );
}

function buildResultModeOption() {
  return choiceOption(
    "result-mode",
    "Result aggregation; non-all modes use exact Buildability evidence",
    [
      ["All solutions (default)", "all-solutions"],
      ["Complete replay paths", "complete-replay-paths"],
      ["Minimum solutions", "minimum-solutions"],
      ["Field average score", "field-average-score"],
      ["Fixed-queue maximum score", "fixed-queue-maximum-score"],
      ["Highest-score minimum set", "highest-score-minimum-set"],
      ["Failed queues", "failed-queues"],
    ],
  );
}

function damageModeOption() {
  return choiceOption(
    "damage-mode",
    "Select the maximum result or require at least minimum-damage",
    [["Maximum (default)", "maximum"], ["At least", "at-least"]],
  );
}

function forwardSpinLinesOption() {
  return Object.freeze({
    ...spinStructureLinesOption(),
    description: "Lines cleared by the selected terminal spin; defaults to any",
  });
}

function forwardSpinCategoryOption() {
  return choiceOption(
    "spin-category",
    "Terminal piece category; Other requires an All-Spin or All-Mini profile",
    [["Any (default)", "any"], ["T piece", "t"], ["Other piece", "other"]],
  );
}

function setupModeOption() {
  return choiceOption(
    "mode",
    "Setup supply mode; QB requires the qb option",
    [["Oracle (default)", "oracle"], ["Observed QB group", "qb"]],
  );
}

function minimalityOption() {
  return choiceOption(
    "minimality",
    "Structure minimization policy",
    [
      ["Subset minimal (default)", "subset-minimal"],
      ["Minimum piece count", "minimum-piece-count"],
    ],
  );
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

function buildHoldOption() {
  return choiceOption(
    "hold",
    "Initial hold state; Empty is the default and a piece means occupied hold",
    [
      ["Empty hold (default)", "empty"],
      ["Hold disabled", "disabled"],
      ...["I", "O", "T", "S", "Z", "J", "L"].map((piece) => [
        `Occupied by ${piece}`,
        piece,
      ]),
    ],
  );
}

function buildV2MaskOption(name, description) {
  return Object.freeze({
    ...stringOption(name, description, true, 66),
    required: true,
  });
}

function buildV2DocumentFormatOption(name, description) {
  return Object.freeze({
    ...choiceOption(name, description, [
      ["CTK3", "ctk3"],
      ["v115 Fumen", "fumen"],
    ]),
    required: true,
  });
}

function buildV2DocumentOption(name, description) {
  return Object.freeze({
    ...stringOption(name, description, true, DOCUMENT_MAX_LENGTH),
    required: true,
  });
}

function buildV2SupplyOption(name, description) {
  return stringOption(name, description, false, NEXT_MAX_LENGTH);
}

function buildV2ObjectiveOption(capabilityId) {
  const objectives = ({
    "build.cover": [
      ["Minimum cover (default)", "min-cover"],
      ["Maximum probability, then minimum", "max-probability-minimum"],
    ],
    "build.setup": [["Unique family (default)", "unique"], ["All candidates", "all"]],
    "build.congruent": [["Unique family (default)", "unique"], ["All candidates", "all"]],
    "build.congruent-cover": [
      ["Minimum cover (default)", "min-cover"],
      ["Maximum probability, then minimum", "max-probability-minimum"],
    ],
    "build.setup-cover": [
      ["Minimum cover (default)", "min-cover"],
      ["Maximum probability, then minimum", "max-probability-minimum"],
    ],
    "build.setup-cover-percent": [["Unique family (default)", "unique"], ["All candidates", "all"]],
    "build.setup-cover-score": [["Maximum score cover (fixed)", "max-score-cover"]],
    "build.evaluate.cover": [["All supplied solutions (fixed)", "all"]],
    "build.evaluate.minimals": [["Minimum cover (fixed)", "min-cover"]],
    "build.evaluate.score": [["Maximum score cover (fixed)", "max-score-cover"]],
    "build.evaluate.b2b-cover": [["All B2B-preserving solutions (fixed)", "all"]],
    "build.evaluate.cover-percent": [["Unique supplied solutions (fixed)", "unique"]],
  })[capabilityId];
  if (!objectives) {
    throw new Error(`Unknown Build v2 objective authority: ${capabilityId}`);
  }
  return choiceOption(
    "objective",
    "Capability-closed objective; attack is never a score tiebreaker",
    objectives,
  );
}

function buildV2ScoreOptions(capabilityId) {
  if (!["build.setup-cover-score", "build.evaluate.score"].includes(capabilityId)) {
    return [];
  }
  return [
    pcScoreProfileOption(),
    boundedIntegerOption("initial-b2b", "Initial back-to-back chain used by scoring", 0, 65_535),
  ];
}

function buildFinesseOption() {
  return choiceOption(
    "finesse",
    "Finesse evaluation; Inputs turns this into the /finesse search preset",
    [["Off (default)", "off"], ["Minimum inputs", "inputs"]],
  );
}

function buildFinesseKnowledgeOption() {
  return choiceOption(
    "finesse-knowledge",
    "Information policy for finesse=inputs; otherwise unavailable",
    [
      ["Full queue and visible 7 (default)", "both"],
      ["Full future queue", "oracle"],
      ["Visible 7 pieces", "visible-7"],
    ],
  );
}

function buildMirrorOption() {
  return choiceOption(
    "mirror",
    "Mirror policy; the /finesse search compatibility preset always excludes mirrors",
    [["Automatic (default)", "auto"], ["Include", "include"], ["Exclude", "exclude"]],
  );
}

function choiceOption(name, description, choices) {
  return Object.freeze({
    ...stringOption(name, description, false, 32),
    choices: Object.freeze(choices.map(([choiceName, value]) =>
      Object.freeze({ name: choiceName, value })
    )),
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

function attachmentOption(name, description) {
  return Object.freeze({
    type: ATTACHMENT_OPTION,
    name,
    description,
    required: false,
  });
}

function normalizeHelpTarget(value) {
  if (value === undefined || value === null) return "";
  return String(value).trim().toLowerCase().replace(/^\//, "").replaceAll("_", "-");
}

function normalizeObjectiveHelpTarget(value) {
  if (value === undefined || value === null) return null;
  const normalized = String(value).trim().toLowerCase().replace(/^\//, "");
  return normalized === "objective" || normalized.startsWith("objective ")
    ? normalized
    : null;
}

function commandListHelp(locale) {
  if (locale === "ko") {
    return [
      "**Clearra 슬래시 명령어**",
      "렌더 파일: `/render-file` 또는 미리보기 메시지의 `앱 → 원본 GIF 받기` (명령어 필드는 해당 명령 안에서 자동 렌더링)",
      "퍼펙트 클리어: `/pc path|chance|minimals|score|saves|best-save|score-minimals|tiling|failed-queue|score-finder`",
      "구축: `/build cover|probability|setup|congruent|congruent-cover|setup-cover|setup-cover-percent|setup-cover-score|evaluate-cover|evaluate-minimals|evaluate-score|evaluate-b2b-cover|evaluate-cover-percent|finesse-score`",
      "셋업 순위와 점수: `/setup joint|build|pc|score`",
      "정방향 탐색: `/forward spin|damage`",
      "구조 탐색: `/spin-structure search|cover|guaranteed`",
      "문서 유틸리티: `/utility sequence|sequence-dependencies|parity|fumen|render`",
      "호환 경로: `/finesse search|score` 및 기존 개별 명령어는 전환 기간에만 유지됩니다.",
      "고급 objective는 `/help arguments:objective`에서 확인할 수 있습니다.",
      "정확한 문법은 `/help arguments:<명령어> <하위-명령어>`로 확인하세요. 여러 줄 격자는 필드 옵션을 생략하고 입력 창에서 작성하며, 직접 입력은 `grid:윗줄/다음줄` 형식을 사용합니다.",
      `PC 탐색은 1–${DISCORD_PC_FIELD_MAX_ROWS}줄의 모든 목표 높이를 지원하며, 구축·전방 탐색 필드는 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄을 지원합니다. 정적 CTK3, v115 Fumen, 문서 링크도 지원하며 입력 색상은 모두 채워진 칸으로 처리합니다.`,
    ].join("\n");
  }
  return [
    "**Clearra slash commands**",
    "Render files: `/render-file` or `Apps → Get original GIF` on a preview message (command fields render inside their own command)",
    "Perfect clears: `/pc path|chance|minimals|score|saves|best-save|score-minimals|tiling|failed-queue|score-finder`",
    "Build: `/build cover|probability|setup|congruent|congruent-cover|setup-cover|setup-cover-percent|setup-cover-score|evaluate-cover|evaluate-minimals|evaluate-score|evaluate-b2b-cover|evaluate-cover-percent|finesse-score`",
    "Setup ranking and scoring: `/setup joint|build|pc|score`",
    "Forward search: `/forward spin|damage`",
    "Spin structures: `/spin-structure search|cover|guaranteed`",
    "Document utilities: `/utility sequence|sequence-dependencies|parity|fumen|render`",
    "Compatibility: `/finesse search|score` and legacy single-purpose names remain only for the migration window.",
    "Advanced objective syntax is documented by `/help arguments:objective`.",
    "Use `/help arguments:<command> <subcommand>` for exact syntax. Omit a board option to enter a multiline grid in the guided form; direct grids use `grid:top-row/next-row`.",
    `PC search supports every target height from 1 through ${DISCORD_PC_FIELD_MAX_ROWS} rows; build/forward fields support 1 through ${DISCORD_WIDE_FIELD_MAX_ROWS} rows. Static CTK3, v115 Fumen, and document links are also accepted; input colors mean occupied cells.`,
  ].join("\n");
}

function syntax(entry, locale = "en") {
  const path = commandPath(entry);
  if (normalizeDiscordLocale(locale) === "ko") {
    switch (entry.input) {
      case "render-file":
        return "/render-file [image:<같은 채널의 미리보기 메시지 링크|메시지 ID>]";
      case "pc":
        return `/${path} next:<패턴> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [kicktable:<내장 프로필>] [options:hold=use]`;
      case "pc-v2":
        return `/${path} next:<패턴> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<내장 프로필>] [queue-knowledge:<oracle|visible-7>] [spin-profile:<프로필>] [preserve-b2b:<on|off>] [solution-probabilities:<on|off>]`;
      case "pc-path-v2":
        return `/${path} next:<큐|패턴> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<내장 프로필>] [spin-profile:<프로필>] [preserve-b2b:<on|off>]`;
      case "pc-chance-v2":
        return `/${path} next:<패턴> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<내장 프로필>]`;
      case "pc-save-v2":
        return `/${path} next:<고정 가방 경계 패턴> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<내장 프로필>]`;
      case "pc-allspin-exact-v1":
        return `/${path} field:<초기 필드> next:<정확한 IOTSZJL 큐> spin-profile:<프로필> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<on|off>] [kicktable:<내장 프로필>] [max-patterns:<개수>] [max-nodes:<개수>] [max-frontier-states:<개수>] [max-candidates:<개수>] [max-memory-mib:<MiB>]`;
      case "pc-allspin-pattern-v1":
        return `/${path} field:<초기 필드> next:<큐 패턴> spin-profile:<프로필> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<on|off>] [kicktable:<내장 프로필>] [max-patterns:<개수>] [max-nodes:<개수>] [max-frontier-states:<개수>] [max-candidates:<개수>] [max-memory-mib:<MiB>]`;
      case "pc-score-v2":
        return ["pc.score", "pc.score-minimals"].includes(entry.capabilityId)
          ? `/${path} next:<패턴> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<내장 프로필>] [score-profile:<tetrio|guideline|jstris-ultra>] [spin-profile:<프로필>] [initial-b2b:0..65535]`
          : `/${path} next:<패턴> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<내장 프로필>] [queue-knowledge:<oracle|visible-7>] [score-profile:<tetrio|guideline|jstris-ultra>] [spin-profile:<프로필>] [preserve-b2b:<on|off>] [initial-b2b:0..65535] [solution-probabilities:<on|off>]`;
      case "pc-score-finder-v2":
        return `/${path} next:<정확한 IOTSZJL 큐> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<내장 프로필>] [initial-b2b:<on|off>]`;
      case "pc-tiling-v2":
        return `/${path} next:<패턴> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>]`;
      case "pc-failed-v2":
        return `/${path} next:<패턴> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<내장 프로필>] [queue-knowledge:<oracle|visible-7>] [spin-profile:<프로필>] [preserve-b2b:<on|off>] [failed-count:1..4294967295]`;
      case "cover":
        return `/${path} next:<패턴> base:<기존 필드> target:<추가할 칸> [kicktable:<내장 프로필>] [options:hold=use]`;
      case "build-cover":
        return `/${path} next:<패턴> base:<기존 필드> target:<추가할 칸> [result-mode:<7개 결과 집계>] [kicktable:<내장 프로필>] [hold:<disabled|empty|IOTSZJL>] [source-pieces:1..4294967295] [score-profile:<프로필>] [initial-b2b:0..65535] [failed-count:<개수>] [solution-probabilities:<on|off>] [finesse:<off|inputs>] [finesse-knowledge:<both|oracle|visible-7>] [mirror:<auto|include|exclude>]`;
      case "build-v2-cover":
        return `/${path} base-mask:<10진수|0x16진수> target-mask:<10진수|0x16진수> height:1..${DISCORD_PC_FIELD_MAX_ROWS} (queue:<정확한 큐>|patterns:<패턴>) [hold:<disabled|empty|IOTSZJL>] [queue-knowledge:<oracle|visible-7>] [objective:<허용 objective>] [kicktable:<내장 프로필>] [source-pieces:<개수>]`;
      case "build-v2-target":
        return `/${path} target-format:<ctk3|fumen> target-document:<색상 target 문서> (queue:<정확한 큐>|patterns:<패턴>) [hold:<disabled|empty|IOTSZJL>] [queue-knowledge:<oracle|visible-7>] [objective:<허용 objective>] [kicktable:<내장 프로필>]${entry.capabilityId === "build.setup-cover-score" ? " [score-profile:<tetrio|guideline|jstris-ultra>] [initial-b2b:0..65535]" : ""}`;
      case "build-v2-supplied":
        return `/${path} solution-format:<ctk3|fumen> solution-document:<색상 supplied-solution 문서> (queue:<정확한 큐>|patterns:<패턴>) [hold:<disabled|empty|IOTSZJL>] [queue-knowledge:<oracle|visible-7>] [objective:<허용 objective>] [kicktable:<내장 프로필>]${entry.capabilityId === "build.evaluate.score" ? " [score-profile:<tetrio|guideline|jstris-ultra>] [initial-b2b:0..65535]" : ""}`;
      case "colored":
        return `/${path} next:<패턴> field:<목표 필드> [kicktable:<내장 프로필>]`;
      case "spin":
        return `/${path} next:<패턴> field:<격자|CTK3|v115 Fumen|URL> [kicktable:<내장 프로필>] [options:type=TSS]`;
      case "forward-spin-v2":
        return `/${path} next:<큐|패턴> field:<격자|CTK3|v115 Fumen|URL> [height:1..24] [hold:<on|off>] [kicktable:<내장 프로필>] [spin-profile:<프로필>] [lines:<any|0..4|1+..4+>] [spin-category:<any|t|other>] [initial-combo:<0..65535>] [initial-b2b:<0..65535>] [preserve-b2b:<on|off>]`;
      case "fixed-next":
        return `/${path} next:<정확한 IOTSZJL 큐> field:<격자|CTK3|v115 Fumen|URL> [kicktable:<내장 프로필>] [options:<hold spin-profile minimum-damage initial-combo initial-b2b preserve-b2b>]`;
      case "forward-damage-v2":
        return `/${path} next:<정확한 IOTSZJL 큐> field:<격자|CTK3|v115 Fumen|URL> [height:1..24] [hold:<on|off>] [kicktable:<내장 프로필>] [spin-profile:<프로필>] [damage-mode:<maximum|at-least>] [minimum-damage:<0..4294967295>] [initial-combo:<0..65535>] [initial-b2b:<0..65535>] [preserve-b2b:<on|off>]`;
      case "forward-ren-v1":
        return `/${path} next:<최대 22개의 정확한 IOTSZJL 큐> field:<격자|CTK3|v115 Fumen|URL> [height:1..24] [hold:<on|off>] [kicktable:<내장 프로필>]`;
      case "score-fixed-next":
        return `/${path} next:<정확한 IOTSZJL 큐> field:<격자|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [kicktable:<내장 프로필>] [options:initial-b2b=false]`;
      case "score-fixed-next-v2":
        return `/${path} next:<정확한 IOTSZJL 큐> field:<격자|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [kicktable:<내장 프로필>] [initial-b2b:<on|off>]`;
      case "remaining":
        return `/${path} remaining:<순서 없는 IOTSZJL 목록> [priority:<all|build|pc>] [max-setup-pieces:1..10] [queue-knowledge:<full-queue|visible-7>] [next-cycle-remaining:<정확한 목록>] [setup-length:<auto|longer|shorter>] [kicktable:<내장 프로필>] [options:<mode qb post-cycle-borrow>]`;
      case "setup-v2":
        return `/${path} remaining:<순서 없는 IOTSZJL 목록> [mode:<oracle|qb>] [qb:<관측 미노>] [queue-knowledge:<full-queue|visible-7>] [next-cycle-remaining:<정확한 목록>] [post-cycle-borrow:<on|off>] [setup-length:<auto|longer|shorter>] [max-setup-pieces:1..10] [kicktable:<내장 프로필>]`;
      case "setup-score-v1":
        return `/${path} document-format:<ctk3|fumen> document:<색상 셋업 후보 문서> (setup-queue:<정확한 큐>|setup-patterns:<패턴>) (solution-queue:<정확한 큐>|solution-patterns:<패턴>) [clear:1..6] [hold:<on|off>] [score-profile:<tetrio|guideline|jstris-ultra>] [initial-b2b:0..4294967295] [kicktable:<내장 프로필>] [max-patterns:<개수>]`;
      case "spin-structure":
        return `/${path} pieces:<순서 없는 IOTSZJL 목록> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [lines:<any|0..4|1+..4+>] [profile:<T-Spins|T-Spins+|All-Mini(+)|All-Spin(+)>] [kicktable:<내장 프로필>] [options:<fill-bottom fill-top max-placements minimality>]`;
      case "spin-structure-v2":
        return `/${path} pieces:<순서 없는 IOTSZJL 목록> field:<grid:윗줄/다음줄|CTK3|v115 Fumen|URL> [height:1..24] [lines:<any|0..4|1+..4+>] [spin-profile:<프로필>] [kicktable:<내장 프로필>] [fill-bottom:0..23] [fill-top:1..24] [max-placements:<개수>] [minimality:<subset-minimal|minimum-piece-count>]`;
      case "spin-structure-cover-v1":
        return `/${path} pieces:<순서 없는 IOTSZJL 목록> [field:<격자|CTK3|v115 Fumen|URL>] [height:1..24] [lines:<any|1..4|1+..4+>] [spin-profile:<프로필>] [kicktable:<srs-plus|srs>] [fill-bottom:0..23] [fill-top:1..24] [max-placements:<개수>] [minimality:<subset-minimal|minimum-piece-count>] [max-patterns:1..100000]`;
      case "spin-structure-guaranteed-v1":
        return `/${path} pieces:<순서 없는 IOTSZJL 목록> [field:<격자|CTK3|v115 Fumen|URL>] [height:1..24] [lines:<any|1..4|1+..4+>] [spin-profile:<프로필>] [kicktable:<srs-plus|srs>] [fill-bottom:0..23] [fill-top:1..24] [max-placements:<개수>] [minimality:<subset-minimal|minimum-piece-count>] [final-piece:<IOTSZJL>] [max-patterns:1..100000] [dependency-report:<on|off>]`;
      case "finesse":
        return "/finesse search target:<목표 칸> next:<큐|패턴> base:<기존 필드> [options:<hold knowledge source-pieces aggregation spin-profile preserve-b2b>] | /finesse score document:<operation 포함 문서> next:<큐|패턴> [options:<hold knowledge source-pieces>]";
      case "finesse-search":
        return "/finesse search target:<목표 칸> next:<큐|패턴> base:<기존 필드> [options:<hold knowledge source-pieces aggregation spin-profile preserve-b2b>]";
      case "finesse-score":
        return "/finesse score document:<operation 포함 CTK3|v115 Fumen> next:<큐|패턴> [options:<hold knowledge source-pieces>]";
      case "finesse-score-v2":
        return `/${path} document:<operation 포함 CTK3|v115 Fumen> next:<큐|패턴> [kicktable:<내장 프로필>] [hold:<disabled|empty|IOTSZJL>] [knowledge:<both|oracle|visible-7>] [source-pieces:1..128]`;
      case "operation-document-v1":
        return `/${path} [document:<operation 포함 CTK3|v115 Fumen>] [attachment:<CTK3 파일>] [rule-profile:<내장 프로필>] [kick-profile:<내장 프로필>] [timeout-seconds:1..900]`;
      case "field-document-v1":
        return `/${path} [document:<CTK3|v115 Fumen>] [attachment:<UTF-8 문서 파일>]`;
      case "fumen-transform-v1":
        return `/${path} transform:<변환> [document:<v115 Fumen>] [documents:<줄별 Fumen>] [attachment:<UTF-8 Fumen 파일>] [page:1..4096] [offset:-4096..4096] [comments:<줄별 주석>]`;
      case "render-document-v1":
        return `/${path} [document:<CTK3|v115 Fumen>] [attachment:<UTF-8 문서 파일>] artifact-format:<png|gif> [page:1..4096]`;
      default:
        throw new Error(`Unknown slash-command input contract: ${entry.input}`);
    }
  }
  switch (entry.input) {
    case "render-file":
      return "/render-file [image:<same-channel preview message link|message ID>]";
    case "pc":
      return `/${path} next:<pattern> field:<grid:top-row/next-row|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [kicktable:<built-in>] [options:hold=use]`;
    case "pc-v2":
      return `/${path} next:<pattern> field:<grid:top-row/next-row|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<built-in>] [queue-knowledge:<oracle|visible-7>] [spin-profile:<profile>] [preserve-b2b:<on|off>] [solution-probabilities:<on|off>]`;
    case "pc-path-v2":
      return `/${path} next:<queue|pattern> field:<grid:top-row/next-row|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<built-in>] [spin-profile:<profile>] [preserve-b2b:<on|off>]`;
    case "pc-chance-v2":
      return `/${path} next:<pattern> field:<grid:top-row/next-row|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<built-in>]`;
    case "pc-save-v2":
      return `/${path} next:<fixed-bag-boundary pattern> field:<grid:top-row/next-row|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<built-in>]`;
    case "pc-allspin-exact-v1":
      return `/${path} field:<initial field> next:<exact IOTSZJL queue> spin-profile:<profile> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<on|off>] [kicktable:<built-in>] [max-patterns:<count>] [max-nodes:<count>] [max-frontier-states:<count>] [max-candidates:<count>] [max-memory-mib:<MiB>]`;
    case "pc-allspin-pattern-v1":
      return `/${path} field:<initial field> next:<queue pattern> spin-profile:<profile> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<on|off>] [kicktable:<built-in>] [max-patterns:<count>] [max-nodes:<count>] [max-frontier-states:<count>] [max-candidates:<count>] [max-memory-mib:<MiB>]`;
    case "pc-score-v2":
      return ["pc.score", "pc.score-minimals"].includes(entry.capabilityId)
        ? `/${path} next:<pattern> field:<grid:top-row/next-row|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<built-in>] [score-profile:<tetrio|guideline|jstris-ultra>] [spin-profile:<profile>] [initial-b2b:0..65535]`
        : `/${path} next:<pattern> field:<grid:top-row/next-row|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<built-in>] [queue-knowledge:<oracle|visible-7>] [score-profile:<tetrio|guideline|jstris-ultra>] [spin-profile:<profile>] [preserve-b2b:<on|off>] [initial-b2b:0..65535] [solution-probabilities:<on|off>]`;
    case "pc-score-finder-v2":
      return `/${path} next:<exact IOTSZJL queue> field:<grid:top-row/next-row|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<built-in>] [initial-b2b:<on|off>]`;
    case "pc-tiling-v2":
      return `/${path} next:<pattern> field:<grid:top-row/next-row|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>]`;
    case "pc-failed-v2":
      return `/${path} next:<pattern> field:<grid:top-row/next-row|CTK3|v115 Fumen|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [hold:<disabled|empty|IOTSZJL>] [kicktable:<built-in>] [queue-knowledge:<oracle|visible-7>] [spin-profile:<profile>] [preserve-b2b:<on|off>] [failed-count:1..4294967295]`;
    case "cover":
      return `/${path} next:<pattern> base:<field> target:<delta> [kicktable:<built-in>] [options:hold=use]`;
    case "build-cover":
      return `/${path} next:<pattern> base:<field> target:<delta> [result-mode:<7 result aggregations>] [kicktable:<built-in>] [hold:<disabled|empty|IOTSZJL>] [source-pieces:1..4294967295] [score-profile:<profile>] [initial-b2b:0..65535] [failed-count:<count>] [solution-probabilities:<on|off>] [finesse:<off|inputs>] [finesse-knowledge:<both|oracle|visible-7>] [mirror:<auto|include|exclude>]`;
    case "build-v2-cover":
      return `/${path} base-mask:<decimal|0x-hex> target-mask:<decimal|0x-hex> height:1..${DISCORD_PC_FIELD_MAX_ROWS} (queue:<exact queue>|patterns:<pattern>) [hold:<disabled|empty|IOTSZJL>] [queue-knowledge:<oracle|visible-7>] [objective:<allowed objective>] [kicktable:<built-in>] [source-pieces:<count>]`;
    case "build-v2-target":
      return `/${path} target-format:<ctk3|fumen> target-document:<colored target document> (queue:<exact queue>|patterns:<pattern>) [hold:<disabled|empty|IOTSZJL>] [queue-knowledge:<oracle|visible-7>] [objective:<allowed objective>] [kicktable:<built-in>]${entry.capabilityId === "build.setup-cover-score" ? " [score-profile:<tetrio|guideline|jstris-ultra>] [initial-b2b:0..65535]" : ""}`;
    case "build-v2-supplied":
      return `/${path} solution-format:<ctk3|fumen> solution-document:<colored supplied-solution document> (queue:<exact queue>|patterns:<pattern>) [hold:<disabled|empty|IOTSZJL>] [queue-knowledge:<oracle|visible-7>] [objective:<allowed objective>] [kicktable:<built-in>]${entry.capabilityId === "build.evaluate.score" ? " [score-profile:<tetrio|guideline|jstris-ultra>] [initial-b2b:0..65535]" : ""}`;
    case "colored":
      return `/${path} next:<pattern> field:<target> [kicktable:<built-in>]`;
    case "spin":
      return `/${path} next:<pattern> field:<grid|document|URL> [kicktable:<built-in>] [options:type=TSS]`;
    case "forward-spin-v2":
      return `/${path} next:<queue|pattern> field:<grid|document|URL> [height:1..24] [hold:<on|off>] [kicktable:<built-in>] [spin-profile:<profile>] [lines:<any|0..4|1+..4+>] [spin-category:<any|t|other>] [initial-combo:<0..65535>] [initial-b2b:<0..65535>] [preserve-b2b:<on|off>]`;
    case "fixed-next":
      return `/${path} next:<exact IOTSZJL queue> field:<grid|document|URL> [kicktable:<built-in>] [options:<hold spin-profile minimum-damage initial-combo initial-b2b preserve-b2b>]`;
    case "forward-damage-v2":
      return `/${path} next:<exact IOTSZJL queue> field:<grid|document|URL> [height:1..24] [hold:<on|off>] [kicktable:<built-in>] [spin-profile:<profile>] [damage-mode:<maximum|at-least>] [minimum-damage:<0..4294967295>] [initial-combo:<0..65535>] [initial-b2b:<0..65535>] [preserve-b2b:<on|off>]`;
    case "forward-ren-v1":
      return `/${path} next:<exact IOTSZJL queue of at most 22 pieces> field:<grid|document|URL> [height:1..24] [hold:<on|off>] [kicktable:<built-in>]`;
    case "score-fixed-next":
      return `/${path} next:<exact IOTSZJL queue> field:<grid|document|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [kicktable:<built-in>] [options:initial-b2b=false]`;
    case "score-fixed-next-v2":
      return `/${path} next:<exact IOTSZJL queue> field:<grid|document|URL> [lines:1..${DISCORD_PC_FIELD_MAX_ROWS}] [kicktable:<built-in>] [initial-b2b:<on|off>]`;
    case "remaining":
      return `/${path} remaining:<unordered IOTSZJL inventory> [priority:<all|build|pc>] [max-setup-pieces:1..10] [queue-knowledge:<full-queue|visible-7>] [next-cycle-remaining:<exact inventory>] [setup-length:<auto|longer|shorter>] [kicktable:<built-in>] [options:<mode qb post-cycle-borrow>]`;
    case "setup-v2":
      return `/${path} remaining:<unordered IOTSZJL inventory> [mode:<oracle|qb>] [qb:<observed pieces>] [queue-knowledge:<full-queue|visible-7>] [next-cycle-remaining:<exact inventory>] [post-cycle-borrow:<on|off>] [setup-length:<auto|longer|shorter>] [max-setup-pieces:1..10] [kicktable:<built-in>]`;
    case "setup-score-v1":
      return `/${path} document-format:<ctk3|fumen> document:<colored setup-candidate document> (setup-queue:<exact queue>|setup-patterns:<pattern>) (solution-queue:<exact queue>|solution-patterns:<pattern>) [clear:1..6] [hold:<on|off>] [score-profile:<tetrio|guideline|jstris-ultra>] [initial-b2b:0..4294967295] [kicktable:<built-in>] [max-patterns:<count>]`;
    case "spin-structure":
      return `/${path} pieces:<unordered IOTSZJL inventory> field:<grid:top-row/next-row|document|URL> [lines:<any|0..4|1+..4+>] [profile:<T-Spins|T-Spins+|All-Mini(+)|All-Spin(+)>] [kicktable:<built-in>] [options:<fill-bottom fill-top max-placements minimality>]`;
    case "spin-structure-v2":
      return `/${path} pieces:<unordered IOTSZJL inventory> field:<grid:top-row/next-row|document|URL> [height:1..24] [lines:<any|0..4|1+..4+>] [spin-profile:<profile>] [kicktable:<built-in>] [fill-bottom:0..23] [fill-top:1..24] [max-placements:<count>] [minimality:<subset-minimal|minimum-piece-count>]`;
    case "spin-structure-cover-v1":
      return `/${path} pieces:<unordered IOTSZJL inventory> [field:<grid|document|URL>] [height:1..24] [lines:<any|1..4|1+..4+>] [spin-profile:<profile>] [kicktable:<srs-plus|srs>] [fill-bottom:0..23] [fill-top:1..24] [max-placements:<count>] [minimality:<subset-minimal|minimum-piece-count>] [max-patterns:1..100000]`;
    case "spin-structure-guaranteed-v1":
      return `/${path} pieces:<unordered IOTSZJL inventory> [field:<grid|document|URL>] [height:1..24] [lines:<any|1..4|1+..4+>] [spin-profile:<profile>] [kicktable:<srs-plus|srs>] [fill-bottom:0..23] [fill-top:1..24] [max-placements:<count>] [minimality:<subset-minimal|minimum-piece-count>] [final-piece:<IOTSZJL>] [max-patterns:1..100000] [dependency-report:<on|off>]`;
    case "finesse":
      return "/finesse search target:<target> next:<queue|pattern> base:<base> [options:<hold knowledge source-pieces aggregation spin-profile preserve-b2b>] | /finesse score document:<operations> next:<queue|pattern> [options:<hold knowledge source-pieces>]";
    case "finesse-search":
      return "/finesse search target:<target cells> next:<queue|pattern> base:<starting field> [options:<hold knowledge source-pieces aggregation spin-profile preserve-b2b>]";
    case "finesse-score":
      return "/finesse score document:<CTK3|v115 Fumen with operations> next:<queue|pattern> [options:<hold knowledge source-pieces>]";
    case "finesse-score-v2":
      return `/${path} document:<CTK3|v115 Fumen with operations> next:<queue|pattern> [kicktable:<built-in>] [hold:<disabled|empty|IOTSZJL>] [knowledge:<both|oracle|visible-7>] [source-pieces:1..128]`;
    case "operation-document-v1":
      return `/${path} [document:<CTK3|v115 Fumen with operations>] [attachment:<CTK3 file>] [rule-profile:<built-in>] [kick-profile:<built-in>] [timeout-seconds:1..900]`;
    case "field-document-v1":
      return `/${path} [document:<CTK3|v115 Fumen>] [attachment:<UTF-8 document file>]`;
    case "fumen-transform-v1":
      return `/${path} transform:<transform> [document:<v115 Fumen>] [documents:<one Fumen per line>] [attachment:<UTF-8 Fumen file>] [page:1..4096] [offset:-4096..4096] [comments:<one per line>]`;
    case "render-document-v1":
      return `/${path} [document:<CTK3|v115 Fumen>] [attachment:<UTF-8 document file>] artifact-format:<png|gif> [page:1..4096]`;
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
    case "pc-v2":
      return [
        `\`field\` is a top-first 10-column grid of 1–${DISCORD_PC_FIELD_MAX_ROWS} rows or one static CTK3/v115 Fumen/URL; \`next\` accepts the supported fixed/group/bag pattern grammar.`,
        `\`lines\` accepts every height 1–${DISCORD_PC_FIELD_MAX_ROWS}; omitting it evaluates all feasible heights serially. \`hold\` is disabled, empty, or one occupied piece.`,
        "`queue-knowledge`, `spin-profile`, B2B preservation, and per-solution probabilities are named. A spin profile requires preservation on unscored PC objectives; visible-7 is unavailable with minimum-cover.",
        nativeKickHelp,
      ];
    case "pc-path-v2":
      return [
        `\`field\` is a 1–${DISCORD_PC_FIELD_MAX_ROWS}-row initial PC board or one static document; \`next\` is one exact queue or one supported pattern source.`,
        "This path route fixes objective and count to `all` with full-oracle queue knowledge. It accepts only hold, spin profile, B2B preservation, and the rule profile; score, tiling, probability, queue-knowledge, dependency, tablebase, and max-memory controls are closed.",
        "The typed result is an ordinary finite path family. Discord bounds that family for one response and exposes no tie, alternative-page, or cursor control.",
        nativeKickHelp,
      ];
    case "pc-chance-v2":
      return [
        `\`field\` is a top-first 10-column grid of 1–${DISCORD_PC_FIELD_MAX_ROWS} rows or one static CTK3/v115 Fumen/URL; \`next\` defines the full-oracle queue-pattern universe.`,
        `\`lines\` accepts every height 1–${DISCORD_PC_FIELD_MAX_ROWS}; omitting it evaluates all feasible heights serially. \`hold\` is disabled, empty, or one occupied piece.`,
        "This typed probability route owns unique/full-oracle semantics and exposes no objective, queue-knowledge, spin, B2B-preservation, or per-solution-probability control.",
        nativeKickHelp,
      ];
    case "pc-save-v2":
      return entry.capabilityId === "pc.saves" ? [
        `\`field\` and \`next\` use the PC contracts, but \`next\` must compile to an unambiguous fixed bag boundary; exact fixed queues and observed-suffix sources fail closed. \`lines\` accepts 1–${DISCORD_PC_FIELD_MAX_ROWS}.`,
        "A save group is the terminal hold plus the unordered remainder of the active bag. Witnesses are deduplicated once per pattern/group.",
        "Every group reports its exact whole-universe unconditional probability and its conditional probability given that a PC exists; the latter is display-only and never changes ranking.",
        nativeKickHelp,
      ] : [
        `\`field\` and \`next\` use the PC contracts, but \`next\` must compile to an unambiguous fixed bag boundary; exact fixed queues and observed-suffix sources fail closed. \`lines\` accepts 1–${DISCORD_PC_FIELD_MAX_ROWS}.`,
        "Best-save uses schema `clearra-save-v1`: maximize weighted terminal inventory (T6/I4/O3/J1/L1/S0/Z0), then minimize J+L, then maximize exact whole-universe group probability.",
        "All exact best witnesses remain a normal tie list in the typed result. Discord displays the first result in deterministic order; it does not reinterpret ties as portfolios.",
        nativeKickHelp,
      ];
    case "pc-allspin-exact-v1":
      return [
        `\`field\` is the initial PC field, never a target field; it accepts a 1–${DISCORD_PC_FIELD_MAX_ROWS}-row grid or one static document. \`next\` must be one exact IOTSZJL queue, not a pattern.`,
        `\`spin-profile\` is explicit and required. \`lines\` accepts 1–${DISCORD_PC_FIELD_MAX_ROWS}; omitting it serially evaluates feasible heights. \`hold=off\` emits only \`--no-hold\`.`,
        "This witness command applies its typed B2B-preservation preset internally. It rejects objective, scoring, queue-knowledge, source-piece, solution-probability, target-field, and caller-supplied preserve-B2B controls.",
        "Resource limits may return an explicitly incomplete result; they never silently convert it to a complete witness.",
        nativeKickHelp,
      ];
    case "pc-allspin-pattern-v1":
      return [
        `\`field\` is the initial PC field, never a target field; it accepts a 1–${DISCORD_PC_FIELD_MAX_ROWS}-row grid or one static document. \`next\` is a supported queue pattern and defines the probability universe.`,
        `\`spin-profile\` is explicit and required. \`lines\` accepts 1–${DISCORD_PC_FIELD_MAX_ROWS}; omitting it serially evaluates feasible heights. \`hold=off\` emits only \`--no-hold\`.`,
        "This probability command applies its typed B2B-preservation preset internally. It rejects objective, scoring, queue-knowledge, source-piece, solution-probability, target-field, and caller-supplied preserve-B2B controls.",
        "Resource limits may return explicitly incomplete counts or probability; completeness is shown in the result.",
        nativeKickHelp,
      ];
    case "pc-score-v2":
      if (entry.capabilityId === "pc.score") return [
        `\`field\` and \`next\` use the PC contracts; \`lines\` accepts 1–${DISCORD_PC_FIELD_MAX_ROWS} and omission evaluates feasible targets serially.`,
        "`score-profile` defaults to `tetrio`; `spin-profile` and `initial-b2b` are named scoring inputs. This route fixes the all score-summary objective and full-oracle queue semantics, and rejects objective overrides, B2B-preservation controls, resource/execution limits, and per-solution probabilities.",
        "Every built-in score profile currently reports `accuracy_level=basic-approximation` and `profile_specific_exact=false`; a selected profile is not evidence of exact profile-specific scoring. The direct `/score` compatibility alias remains the generic Jstris Ultra preset.",
        nativeKickHelp,
      ];
    case "pc-score-finder-v2":
      return [
        `\`field\` is a 1–${DISCORD_PC_FIELD_MAX_ROWS}-row initial PC board or one static document; \`next\` must be one exact IOTSZJL queue, never a pattern.`,
        "Jstris Ultra scoring, T-spin recognition, all-witness search, and CPU execution are fixed by the capability. Score/profile, spin/profile, objective, queue-knowledge, worker, backend, fallback, and max-memory overrides are unavailable.",
        "Score equality and ordering are score-only. Attack is informational and cannot select or order a witness; Discord returns only the first result in deterministic order and exposes no tie metadata.",
        nativeKickHelp,
      ];
      if (entry.capabilityId === "pc.score-minimals") return [
        `\`field\` and \`next\` use the PC contracts; \`lines\` accepts 1–${DISCORD_PC_FIELD_MAX_ROWS} and omission evaluates feasible targets serially.`,
        "This route fixes exact score-only minimum-cover semantics. Attack is informational only and cannot affect equality, eligibility, ordering, membership, or canonical selection.",
        "Discord returns exactly the first result in deterministic order from the canonical portfolio and exposes no tie, alternative-page, or cursor controls.",
        nativeKickHelp,
      ];
      return [
        `\`field\` and \`next\` use the PC contracts; \`lines\` accepts 1–${DISCORD_PC_FIELD_MAX_ROWS} and omission evaluates feasible targets serially.`,
        "`score-profile` defaults to `tetrio`; spin profile, initial/preserved B2B, queue knowledge, and per-solution probabilities are independent named options.",
        nativeKickHelp,
      ];
    case "pc-tiling-v2":
      return [
        "Geometry-only tiling preserves the native field, queue/pattern, and initial hold supply, but intentionally has no kick, score, observation, probability, or B2B options.",
        `\`lines\` accepts 1–${DISCORD_PC_FIELD_MAX_ROWS}; omission evaluates feasible targets serially.`,
      ];
    case "pc-failed-v2":
      return [
        "Failed-queue search uses the native reverse PC contract and returns patterns that cannot satisfy the target. `failed-count` bounds returned failures.",
        "Queue knowledge and B2B preservation are named; a spin profile requires preservation. Scoring, tiling, and per-solution probabilities are unavailable.",
        nativeKickHelp,
      ];
    case "cover":
      return [
        "`base` is the existing field; `target` contains only cells to add. They must not overlap. Target must be non-empty with a block count divisible by four, and base must not contain a completed row.",
        `Both fields accept 1–${DISCORD_WIDE_FIELD_MAX_ROWS} top-first grid rows or one static CTK3/v115 Fumen/URL. In a grid, use \`#\` for filled and \`_\` for empty.`,
        "`next` accepts the supported fixed/group/bag patterns. `options` selects `hold=use|avoid`. Colored CTK3 solution output remains the default.",
        kickHelp,
      ];
    case "build-cover":
      return [
        "`base` is the existing field and `target` contains only cells to add. They must not overlap, and target must be non-empty with a block count divisible by four.",
        `Both fields accept 1–${DISCORD_WIDE_FIELD_MAX_ROWS} top-first grid rows or one static CTK3/v115 Fumen/URL.`,
        "`hold` is disabled, empty, or one occupied IOTSZJL piece. `source-pieces` sets the exact source window; when omitted, the engine derives it from the target piece count and initial hold.",
        "`solution-probabilities=on` includes exact per-solution probabilities. It is unavailable with geometry-only tiling.",
        "`finesse=inputs` enables minimum-input evaluation; only then may `finesse-knowledge` be set. `mirror` controls reflected solutions. `/finesse search` retains its fixed preset.",
        "`result-mode` selects All solutions, Complete replay paths, Minimum solutions, Field average score, Fixed-queue maximum score, Highest-score minimum set, or Failed queues. Non-all modes use the explicit Buildability compatibility row; score ties never use attack and Discord selects the first result in deterministic order.",
        nativeKickHelp,
      ];
    case "build-v2-cover":
      return [
        "`base-mask`, `target-mask`, and `height` are the only accepted source form. Plain grids and target documents are rejected for this capability.",
        "Supply exactly one of `queue` or `patterns`. Hold, queue knowledge, and objective are capability-closed; `source-pieces` exists only here.",
        "Execution is CPU-only with no backend fallback and no Discord max-memory option. Exact portfolio alternatives are never requested or paged.",
        nativeKickHelp,
      ];
    case "build-v2-target":
      return [
        "`target-document` is a nominal colored target in exactly the declared CTK3 or v115 Fumen format. Plain grids, URLs, gray-only targets, and supplied-solution substitution are rejected.",
        "Supply exactly one of `queue` or `patterns`. Hold, queue knowledge, and objective are closed per capability; score options exist only on setup-cover-score.",
        "Execution is CPU-only with no backend fallback or max-memory option. Ordinary candidate families remain families; exact portfolios have no Discord alternative paging.",
        nativeKickHelp,
      ];
    case "build-v2-supplied":
      return [
        "`solution-document` is the supplied colored solution set in exactly the declared CTK3 or v115 Fumen format. A nominal target document cannot substitute for it.",
        "Supply exactly one of `queue` or `patterns`. Hold, queue knowledge, and objective are closed per capability; score options exist only on evaluate score.",
        "Execution is CPU-only with no backend fallback or max-memory option. Score equality is score-only; attack is never used for selection or ordering.",
        nativeKickHelp,
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
    case "forward-spin-v2":
      return [
        `\`field\` accepts 1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows or one static document; \`next\` accepts one queue or the supported pattern grammar. \`height\` defaults to at least 8 and cannot exclude supplied rows.`,
        "`hold`, `spin-profile`, terminal `lines`, `spin-category`, initial combo/B2B, and B2B preservation are independent named options. `other` is valid with every All-Spin(+) and All-Mini(+) profile.",
        nativeKickHelp,
      ];
    case "fixed-next":
      return [
        `\`field\` accepts 1–${DISCORD_WIDE_FIELD_MAX_ROWS} top-first rows or one static document/URL. In a grid, use \`#\` for filled and \`_\` for empty. \`next\` must be one exact IOTSZJL queue, not a pattern.`,
        "`options` keys are `hold`, `spin-profile`, `minimum-damage`, `initial-combo`, `initial-b2b`, and `preserve-b2b`. Minimum damage selects at-least mode; zero combo is omitted.",
        nativeKickHelp,
      ];
    case "forward-damage-v2":
      return [
        `\`field\` accepts 1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows or one static document; \`next\` must be one exact queue. \`height\` defaults to at least 8 and cannot exclude supplied rows.`,
        "Damage defaults to `spin-profile=all-mini-plus`, `hold=on`, and maximum mode. `damage-mode=at-least` requires `minimum-damage`; maximum mode rejects it.",
        "Initial combo/B2B and B2B preservation are named, result-affecting options.",
        nativeKickHelp,
      ];
    case "forward-ren-v1":
      return [
        `\`field\` accepts 1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows or one static document; \`next\` must be one exact queue of at most 22 pieces. \`height\` defaults to at least 8 and cannot exclude supplied rows.`,
        "Every accepted lock must clear at least one line. The first clear is REN 0; initial complete rows are normalized without counting. Discord returns the first result in deterministic order when maximum-REN witnesses tie.",
        nativeKickHelp,
      ];
    case "score-fixed-next":
      return [
        `\`field\` accepts 1–${DISCORD_PC_FIELD_MAX_ROWS} top-first rows or one static CTK3/v115 Fumen/URL. In a grid, use \`#\` for filled and \`_\` for empty. \`next\` must be one exact IOTSZJL queue, not a pattern.`,
        `\`lines\` accepts every perfect-clear target height from 1 through ${DISCORD_PC_FIELD_MAX_ROWS}. \`options\` accepts only \`initial-b2b=true|false\` and defaults to false.`,
        kickHelp,
      ];
    case "score-fixed-next-v2":
      return [
        `\`field\` accepts 1–${DISCORD_PC_FIELD_MAX_ROWS} rows or one static document; \`next\` must be one exact queue.`,
        "`lines` defaults to 4. `initial-b2b` is a named scoring-state option and defaults to off.",
        kickHelp,
      ];
    case "remaining":
      return [
        "`remaining` is an unordered inventory of 1–7 IOTSZJL pieces. At most one piece kind may appear twice; that duplicate becomes the initial hold. Three copies or multiple duplicated kinds are rejected.",
        `\`priority\` orders candidates by joint build × PC (\`all\`), build probability, or PC probability. /${commandPath(entry)} defaults to \`${entry.setupPriority}\`.`,
        "`max-setup-pieces` accepts 1–10 and defaults to 9; choose 10 to include complete perfect clears. `queue-knowledge` is `full-queue` (default) or `visible-7`.",
        "`next-cycle-remaining` is an exact unordered inventory for the following cycle. Its required count is determined by `remaining` (7→4, 4→1, 1→5, 5→2, 2→6, 6→3, 3→7), with the same duplicate rule.",
        "`setup-length` is `auto`, `longer`, or `shorter`. Auto favors longer setups for `all`/`build` and shorter setups for `pc`.",
        "`options` keys are `mode`, `qb`, and `post-cycle-borrow`. QB mode requires `qb`; borrowing is limited to cycle 7 (`remaining` has three pieces).",
        nativeKickHelp,
      ];
    case "setup-v2":
      return [
        "`remaining` is an unordered 1–7-piece inventory. At most one piece kind may appear twice; that duplicate becomes the initial hold.",
        `/${commandPath(entry)} has the semantic ranking preset \`${entry.setupPriority}\`. Supply mode, QB observation, queue knowledge, next-cycle residue, borrowing, length, and maximum setup pieces are separate named options.`,
        "QB mode requires `qb`; post-cycle borrowing is limited to a three-piece residue. Hidden Modal options are never silently discarded.",
        nativeKickHelp,
      ];
    case "setup-score-v1":
      return [
        "`document` is a canonical colored setup-candidate document in the declared CTK3 or v115 Fumen format. Plain grids, gray-only documents, format mismatch, and a supplied-solution substitute fail closed.",
        "Supply exactly one setup queue or setup pattern and exactly one solution queue or solution pattern. Clear height, hold, score profile, initial B2B, rule, and pattern bound are independent named inputs.",
        "Discord fixes CPU execution with no backend fallback and exposes no max-memory option. The result remains an ordinary bounded ranking family with no tie or attack-selection surface.",
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
    case "spin-structure-v2":
      return [
        "`pieces` is an unordered IOTSZJL inventory; repeated letters preserve multiplicity and hold is not used.",
        `\`field\` accepts 1–${DISCORD_WIDE_FIELD_MAX_ROWS} rows or a static document. Height, terminal lines, spin profile, fill bounds, maximum placements, and minimality are independent named options.`,
        "`spin-profile` selects T-Spins, T-Spins+, All-Mini(+), or All-Spin(+). Regular and Mini results are always reported separately; `+` adds the exact immobile-T fallback.",
        "`fill-bottom` must be below `fill-top`; `max-placements` cannot exceed the supplied inventory. Results default to subset-minimal structures.",
        nativeKickHelp,
      ];
    case "spin-structure-cover-v1":
      return [
        "`pieces` is an unordered inventory, never a queue or hold source. Field and structural controls use the same closed grammar as structure search.",
        "The objective is fixed to exact minimum cover. Discord keeps only the first canonical portfolio, preserves that portfolio's member role, and exposes no alternative count, cursor, page, or live handle.",
        "`max-patterns` is bounded to 1–100000. Execution is CPU-only with no backend fallback; memory budgets, tablebases, GPU controls, queue, pattern, and hold inputs are rejected.",
        "Only `srs-plus` and `srs` are accepted by this authoritative structural product.",
      ];
    case "spin-structure-guaranteed-v1":
      return [
        "`pieces` is an unordered inventory. `final-piece` defaults to T, must occur in that inventory, and is checked against the selected spin profile.",
        "`dependency-report` is off by default and remains an explicit calculation option. The result is an ordinary bounded family, never a tie portfolio.",
        "`max-patterns` is bounded to 1–100000. Execution is CPU-only with no backend fallback; memory budgets, tablebases, GPU controls, queue, pattern, and hold inputs are rejected.",
        "Only `srs-plus` and `srs` are accepted by this authoritative structural product.",
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
    case "finesse-score-v2":
      return [
        "`document` must be CTK3 or v115 Fumen, and every page must contain one placement operation in sequence. This fixed-placement score form is not a Build search alias.",
        "`next` accepts one exact queue or the supported pattern grammar. `hold`, queue `knowledge`, and `source-pieces` are separate named options.",
        nativeKickHelp,
      ];
    case "operation-document-v1":
      return [
        "Supply exactly one `document` string or one CTK3 `attachment`. Every page must retain one concrete locked operation; static fields, queues, hold, mirror, rise, quiz, and non-empty garbage rows are rejected.",
        "`rule-profile` and `kick-profile` default independently to `srs-plus`. `timeout-seconds` defaults to and cannot exceed 900.",
        entry.capabilityId === "utility.sequence"
          ? "The report losslessly normalizes the supplied operation trace and replay-checks every locked placement. Discord shows only the bounded canonical summary and trace preview."
          : "The report counts the exact accepted operation orders and returns universal precedence, its transitive reduction, independent pairs, and reachability/kick evidence. Discord shows only the canonical compact report.",
      ];
    case "field-document-v1":
      return [
        "Supply exactly one canonical CTK3/v115 Fumen `document` or one bounded UTF-8 `attachment`; grids and links are rejected.",
        "Parity is page-bound observation only. Discord validates every returned page but displays only page 1, keeps pending garbage separate, and never claims feasibility or pruning authority.",
      ];
    case "fumen-transform-v1":
      return [
        "Transforms are closed to roundtrip, combine, split, get-page, page-shift, clean-comments, preserve-comments, to-gray, mirror, and text-to-fumen.",
        "Combine accepts one canonical v115 Fumen per line in `documents` or its attachment. Get-page is one-based; positive page-shift offsets rotate left; text-to-fumen accepts one bounded Unicode comment per line.",
        "Discord returns a short summary plus the complete canonical Fumen attachment set within its count and byte limits; split pages are not portfolio alternatives.",
      ];
    case "render-document-v1":
      return [
        "Supply exactly one canonical CTK3/v115 Fumen `document` or bounded UTF-8 `attachment`, choose PNG or GIF, and use `page` only for the one-based PNG selection.",
        "The native job runner owns a private temporary output path. Discord publishes only the bounded exact Rust-rendered bytes and never exposes that server path.",
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
    case "pc-v2":
      return [
        `\`field\`에는 위쪽 줄부터 적은 10열 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 격자 또는 정적 CTK3/v115 Fumen/URL 하나를 입력하며, \`next\`에는 지원되는 고정·그룹·가방 패턴을 사용합니다.`,
        `\`lines\`는 1–${DISCORD_PC_FIELD_MAX_ROWS} 전부를 지원하며 생략하면 가능한 높이를 순서대로 판정합니다. \`hold\`는 비활성·빈 홀드·미노 하나 중 하나입니다.`,
        "`queue-knowledge`, `spin-profile`, B2B 보존, 해법별 확률은 각각 명명 옵션입니다. 점수 없는 PC에서 스핀 프로필은 B2B 보존이 필요하고 visible-7은 minimum-cover와 함께 쓸 수 없습니다.",
        nativeKickHelp,
      ];
    case "pc-path-v2":
      return [
        `\`field\`는 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 PC 초기 필드 또는 정적 문서이며 \`next\`는 정확한 큐 하나 또는 지원되는 패턴 공급원 하나입니다.`,
        "이 path 경로는 objective와 count를 `all`, 큐 공개 범위를 full-oracle로 고정합니다. 홀드, 스핀 프로필, B2B 보존, 규칙 프로필만 받으며 점수·타일링·확률·queue-knowledge·의존성·테이블베이스·최대 메모리 설정은 닫혀 있습니다.",
        "타입 결과는 일반적인 유한 path family입니다. Discord는 한 응답 안에서 family만 제한하며 tie, 대안 페이지, 커서 제어를 노출하지 않습니다.",
        nativeKickHelp,
      ];
    case "pc-chance-v2":
      return [
        `\`field\`에는 위쪽 줄부터 적은 10열 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 격자 또는 정적 CTK3/v115 Fumen/URL 하나를 입력하며, \`next\`가 전체 큐를 아는 패턴 확률 공간을 정합니다.`,
        `\`lines\`는 1–${DISCORD_PC_FIELD_MAX_ROWS} 전부를 지원하며 생략하면 가능한 높이를 순서대로 판정합니다. \`hold\`는 비활성·빈 홀드·미노 하나 중 하나입니다.`,
        "이 타입 확률 경로는 unique·전체 큐 지식 의미를 자체 소유하며 objective, queue-knowledge, 스핀, B2B 보존, 해법별 확률 옵션을 노출하지 않습니다.",
        nativeKickHelp,
      ];
    case "pc-save-v2":
      return entry.capabilityId === "pc.saves" ? [
        `\`field\`와 \`next\`는 PC 입력 계약을 사용하지만 \`next\`는 모호하지 않은 고정 가방 경계로 컴파일되어야 합니다. 정확한 고정 큐와 관측 접미 공급원은 닫힌 실패로 거부합니다. \`lines\`는 1–${DISCORD_PC_FIELD_MAX_ROWS}입니다.`,
        "세이브 그룹은 종료 시 홀드와 순서를 무시한 현재 활성 가방의 남은 미노로 정의하며, 증거는 패턴·그룹별로 한 번만 집계합니다.",
        "각 그룹은 전체 우주 기준 무조건 정확 확률과 PC 성공 조건부 확률을 함께 표시합니다. 조건부 확률은 표시값일 뿐 순위에는 사용하지 않습니다.",
        nativeKickHelp,
      ] : [
        `\`field\`와 \`next\`는 PC 입력 계약을 사용하지만 \`next\`는 모호하지 않은 고정 가방 경계로 컴파일되어야 합니다. 정확한 고정 큐와 관측 접미 공급원은 닫힌 실패로 거부합니다. \`lines\`는 1–${DISCORD_PC_FIELD_MAX_ROWS}입니다.`,
        "최적 세이브는 `clearra-save-v1` 스키마로 종료 인벤토리 가중치(T6/I4/O3/J1/L1/S0/Z0)를 최대화하고, J+L을 최소화한 뒤, 전체 우주 기준 그룹 정확 확률을 최대화합니다.",
        "정확히 동률인 최적 증거는 타입 결과에서 일반 목록으로 유지합니다. Discord는 결정적으로 정렬된 첫 결과 하나만 표시하며 동률을 포트폴리오로 바꾸지 않습니다.",
        nativeKickHelp,
      ];
    case "pc-allspin-exact-v1":
      return [
        `\`field\`는 목표 필드가 아니라 PC의 초기 필드이며 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 격자 또는 정적 문서를 받습니다. \`next\`에는 패턴이 아닌 정확한 IOTSZJL 큐 하나를 입력합니다.`,
        `\`spin-profile\`은 명시적으로 필수입니다. \`lines\`는 1–${DISCORD_PC_FIELD_MAX_ROWS}이며 생략하면 가능한 높이를 순서대로 판정합니다. \`hold=off\`는 \`--no-hold\`만 전달합니다.`,
        "이 증거 탐색은 타입이 지정된 B2B 보존 프리셋을 내부에서 적용합니다. objective, 점수, 큐 공개 범위, 소스 미노 수, 해법 확률, 목표 필드, 호출자 preserve-B2B 설정은 받지 않습니다.",
        "리소스 제한에 도달하면 불완전 결과임을 명시하며 완전한 증거로 표시하지 않습니다.",
        nativeKickHelp,
      ];
    case "pc-allspin-pattern-v1":
      return [
        `\`field\`는 목표 필드가 아니라 PC의 초기 필드이며 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 격자 또는 정적 문서를 받습니다. \`next\` 패턴은 확률의 큐 전체집합을 정의합니다.`,
        `\`spin-profile\`은 명시적으로 필수입니다. \`lines\`는 1–${DISCORD_PC_FIELD_MAX_ROWS}이며 생략하면 가능한 높이를 순서대로 판정합니다. \`hold=off\`는 \`--no-hold\`만 전달합니다.`,
        "이 확률 계산은 타입이 지정된 B2B 보존 프리셋을 내부에서 적용합니다. objective, 점수, 큐 공개 범위, 소스 미노 수, 해법 확률, 목표 필드, 호출자 preserve-B2B 설정은 받지 않습니다.",
        "리소스 제한에 도달하면 개수 또는 확률이 불완전함을 결과에 명시합니다.",
        nativeKickHelp,
      ];
    case "pc-score-v2":
      if (entry.capabilityId === "pc.score") return [
        `\`field\`와 \`next\`는 PC 입력 계약을 사용하며 \`lines\`는 1–${DISCORD_PC_FIELD_MAX_ROWS}이고 생략 시 가능한 목표를 순서대로 계산합니다.`,
        "`score-profile`은 기본값 `tetrio`이며 `spin-profile`과 `initial-b2b`는 점수 입력입니다. 이 경로는 all 점수 요약과 전체 큐 지식 의미를 고정하고 objective 재정의, B2B 보존 제어, 리소스·실행 제한, 해법별 확률을 거부합니다.",
        "현재 모든 내장 점수 프로필은 `accuracy_level=basic-approximation`, `profile_specific_exact=false`로 보고하며, 프로필 선택은 프로필별 정확 점수의 증거가 아닙니다. 직접 `/score` 호환 별칭은 기존 Jstris Ultra 프리셋으로 남습니다.",
        nativeKickHelp,
      ];
    case "pc-score-finder-v2":
      return [
        `\`field\`는 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 PC 초기 필드 또는 정적 문서이며 \`next\`에는 패턴이 아닌 정확한 IOTSZJL 큐 하나를 입력합니다.`,
        "Jstris Ultra 점수, T-spin 판정, 전체 증거 탐색, CPU 실행은 capability가 고정합니다. 점수·스핀 프로필, objective, 큐 공개 범위, worker, backend, fallback, 최대 메모리 재정의는 사용할 수 없습니다.",
        "점수 동등성과 순서는 score-only입니다. attack은 정보일 뿐 증거 선택·정렬에 사용할 수 없고 Discord는 결정적으로 정렬된 첫 결과 하나만 반환하며 tie 메타데이터를 노출하지 않습니다.",
        nativeKickHelp,
      ];
      if (entry.capabilityId === "pc.score-minimals") return [
        `\`field\`와 \`next\`는 PC 입력 계약을 사용하며 \`lines\`는 1–${DISCORD_PC_FIELD_MAX_ROWS}이고 생략 시 가능한 목표를 순서대로 계산합니다.`,
        "이 경로는 정확한 score-only 최소 커버 의미를 고정합니다. 공격력은 정보일 뿐 동등성, 적격성, 순서, 해법 집합 구성, 정규 선택에 관여하지 않습니다.",
        "Discord는 정규 포트폴리오에서 결정적으로 정렬된 첫 결과 하나만 반환하며 tie, 대안 페이지, 커서 제어를 노출하지 않습니다.",
        nativeKickHelp,
      ];
      return [
        `\`field\`와 \`next\`는 PC 입력 계약을 사용하며 \`lines\`는 1–${DISCORD_PC_FIELD_MAX_ROWS}이고 생략 시 가능한 목표를 순서대로 계산합니다.`,
        "`score-profile`은 기본값 `tetrio`이며 스핀 프로필, 초기/B2B 보존, 큐 공개 범위, 해법별 확률도 각각 명명 옵션입니다.",
        nativeKickHelp,
      ];
    case "pc-tiling-v2":
      return [
        "기하 타일링은 필드·큐/패턴·초기 홀드 공급을 보존하지만 킥·점수·관측·확률·B2B 옵션은 의도적으로 제공하지 않습니다.",
        `\`lines\`는 1–${DISCORD_PC_FIELD_MAX_ROWS}이며 생략하면 가능한 목표를 순서대로 판정합니다.`,
      ];
    case "pc-failed-v2":
      return [
        "실패 큐 탐색은 native 역방향 PC 계약으로 목표를 달성하지 못하는 패턴을 반환하며 `failed-count`가 반환 수를 제한합니다.",
        "큐 공개 범위와 B2B 보존은 명명 옵션이고, 스핀 프로필은 보존을 필요로 합니다. 점수·타일링·해법별 확률은 사용할 수 없습니다.",
        nativeKickHelp,
      ];
    case "cover":
      return [
        "`base`는 기존 필드이고 `target`에는 추가할 칸만 입력합니다. 두 필드는 겹칠 수 없고, 목표 필드는 비어 있지 않으며 블록 수가 4의 배수여야 하고, 기존 필드에는 완성된 줄이 없어야 합니다.",
        `두 필드는 위쪽 줄부터 적은 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 격자 또는 정적 CTK3/v115 Fumen/URL 하나를 받습니다. 격자에서 \`#\`은 채움, \`_\`는 빈칸입니다.`,
        "`next`에는 지원되는 고정·그룹·가방 패턴을 사용합니다. `options`에서는 홀드 사용 여부를 선택합니다. 색상을 보존한 CTK3 결과가 기본 출력입니다.",
        kickHelp,
      ];
    case "build-cover":
      return [
        "`base`는 기존 필드이고 `target`에는 추가할 칸만 입력합니다. 두 필드는 겹칠 수 없고 목표 필드는 비어 있지 않으며 블록 수가 4의 배수여야 합니다.",
        `두 필드는 위쪽 줄부터 적은 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 격자 또는 정적 CTK3/v115 Fumen/URL 하나를 받습니다.`,
        "`hold`는 비활성, 빈 홀드 또는 IOTSZJL 미노 하나가 들어 있는 홀드입니다. `source-pieces`는 정확한 소스 미노 창이며, 생략하면 엔진이 목표 미노 수와 초기 홀드에서 자동으로 계산합니다.",
        "`solution-probabilities=on`은 해법별 정확한 확률을 포함합니다. 기하학 타일링에서는 사용할 수 없습니다.",
        "`finesse=inputs`에서만 `finesse-knowledge`를 설정할 수 있습니다. `mirror`는 반전 해법을 제어하며 `/finesse search`는 고정 프리셋을 유지합니다.",
        "`result-mode`은 전체 해법, 전체 리플레이 경로, 최소 해법, 필드 평균 점수, 고정 큐 최고 점수, 최고 점수 최소 해법 집합, 실패 큐 중 하나를 선택합니다. all 이외 모드는 명시적인 구축 가능성 호환 조합으로 실행하고, 점수 동점에는 공격력을 사용하지 않으며 Discord는 결정적으로 정렬된 첫 결과 하나만 선택합니다.",
        nativeKickHelp,
      ];
    case "build-v2-cover":
      return [
        "`base-mask`, `target-mask`, `height`만 source로 받으며 일반 격자나 target 문서는 이 capability에서 거부합니다.",
        "`queue`와 `patterns` 중 정확히 하나를 입력합니다. 홀드·큐 공개 범위·objective는 capability별로 닫혀 있고 `source-pieces`는 이 경로에만 있습니다.",
        "CPU 전용이며 backend fallback과 Discord max-memory 옵션은 없습니다. exact portfolio 대안은 요청하거나 페이지로 공개하지 않습니다.",
        nativeKickHelp,
      ];
    case "build-v2-target":
      return [
        "`target-document`는 선언한 CTK3 또는 v115 Fumen 형식의 명목상 색상 target입니다. 일반 격자, URL, 회색 전용 target, supplied-solution 대체를 거부합니다.",
        "`queue`와 `patterns` 중 정확히 하나를 입력합니다. 홀드·큐 공개 범위·objective는 capability별로 닫혀 있고 점수 옵션은 setup-cover-score에만 있습니다.",
        "CPU 전용이며 backend fallback과 max-memory 옵션은 없습니다. 일반 후보 family는 family로 유지하고 exact portfolio 대안 paging은 공개하지 않습니다.",
        nativeKickHelp,
      ];
    case "build-v2-supplied":
      return [
        "`solution-document`는 선언한 CTK3 또는 v115 Fumen 형식의 supplied colored solution set입니다. 명목상 target 문서로 대체할 수 없습니다.",
        "`queue`와 `patterns` 중 정확히 하나를 입력합니다. 홀드·큐 공개 범위·objective는 capability별로 닫혀 있고 점수 옵션은 evaluate score에만 있습니다.",
        "CPU 전용이며 backend fallback과 max-memory 옵션은 없습니다. 점수 동등성은 score-only이고 attack은 선택·정렬에 사용하지 않습니다.",
        nativeKickHelp,
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
    case "forward-spin-v2":
      return [
        `\`field\`는 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 또는 정적 문서를 받으며 \`next\`는 정확한 큐 또는 지원되는 패턴을 받습니다. \`height\` 기본값은 최소 8이고 입력 필드를 잘라낼 수 없습니다.`,
        "`hold`, `spin-profile`, 마지막 스핀의 `lines`, `spin-category`, 초기 콤보/B2B, B2B 보존은 각각 명명 옵션입니다. `other`는 모든 All-Spin(+)·All-Mini(+) 프로필에서 사용할 수 있습니다.",
        nativeKickHelp,
      ];
    case "fixed-next":
      return [
        `\`field\`는 위쪽 줄부터 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 또는 정적 문서/URL 하나를 받습니다. 격자에서 \`#\`은 채움, \`_\`는 빈칸이며 \`next\`에는 패턴이 아닌 정확한 IOTSZJL 큐 하나를 입력해야 합니다.`,
        "`options` 키는 `hold`, `spin-profile`, `minimum-damage`, `initial-combo`, `initial-b2b`, `preserve-b2b`입니다.",
        nativeKickHelp,
      ];
    case "forward-damage-v2":
      return [
        `\`field\`는 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 또는 정적 문서를 받고 \`next\`는 정확한 큐 하나여야 합니다. \`height\` 기본값은 최소 8입니다.`,
        "기본값은 `spin-profile=all-mini-plus`, `hold=on`, 최대 피해 모드입니다. `damage-mode=at-least`에는 `minimum-damage`가 필요합니다.",
        "초기 콤보/B2B와 B2B 보존도 각각 명명 옵션입니다.",
        nativeKickHelp,
      ];
    case "forward-ren-v1":
      return [
        `\`field\`는 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 또는 정적 문서를 받고 \`next\`는 최대 22개 미노의 정확한 큐 하나여야 합니다. \`height\` 기본값은 최소 8입니다.`,
        "모든 배치는 한 줄 이상을 지워야 합니다. 첫 줄 삭제는 REN 0이며 초기 완성 줄은 세지 않고 정규화합니다. 최대 REN 경로가 여럿이면 Discord는 결정적으로 정렬된 첫 결과 하나만 반환합니다.",
        nativeKickHelp,
      ];
    case "score-fixed-next":
      return [
        `\`field\`는 위쪽 줄부터 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 또는 정적 CTK3/v115 Fumen/URL 하나를 받습니다. 격자에서 \`#\`은 채움, \`_\`는 빈칸이며 \`next\`에는 패턴이 아닌 정확한 IOTSZJL 큐 하나를 입력해야 합니다.`,
        `\`lines\`에는 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 중 원하는 퍼펙트 클리어 목표 높이를 지정할 수 있습니다. \`options\`에는 \`initial-b2b=true|false\`만 사용하며 기본값은 false입니다.`,
        kickHelp,
      ];
    case "score-fixed-next-v2":
      return [
        `\`field\`는 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 또는 정적 문서를 받고 \`next\`는 정확한 큐 하나여야 합니다.`,
        "`lines` 기본값은 4이며 `initial-b2b`는 기본 off인 명명 점수 상태 옵션입니다.",
        kickHelp,
      ];
    case "remaining":
      return [
        "`remaining`은 순서 없는 IOTSZJL 미노 1–7개입니다. 한 종류만 두 번 나올 수 있으며 중복 미노가 초기 홀드가 됩니다. 세 개 이상 또는 여러 종류의 중복은 허용하지 않습니다.",
        `\`priority\`는 구축 × PC 종합(\`all\`), 구축 확률 우선, PC 확률 우선으로 후보를 정렬합니다. /${commandPath(entry)}의 기본값은 \`${entry.setupPriority}\`입니다.`,
        "`max-setup-pieces`는 1–10이며 기본값은 9입니다. 완성된 PC까지 포함하려면 10을 선택합니다. `queue-knowledge`는 전체 미래 큐를 쓰는 `full-queue`(기본값) 또는 `visible-7`입니다.",
        "`next-cycle-remaining`은 다음 회차에 남을 정확한 순서 없는 미노 목록입니다. 필요한 개수는 `remaining`에 따라 7→4, 4→1, 1→5, 5→2, 2→6, 6→3, 3→7이며 중복 규칙은 같습니다.",
        "`setup-length`는 `auto`, `longer`, `shorter` 중 하나입니다. 자동은 `all`/`build`에서 긴 셋업, `pc`에서 짧은 셋업을 우선합니다.",
        "`options` 키는 `mode`, `qb`, `post-cycle-borrow`입니다. QB 모드에는 `qb`가 필요하며 빌리기는 remaining 3개인 7회차에서만 허용됩니다.",
        nativeKickHelp,
      ];
    case "setup-v2":
      return [
        "`remaining`은 순서 없는 IOTSZJL 미노 1–7개이며, 한 종류의 중복만 초기 홀드로 허용합니다.",
        `/${commandPath(entry)}에는 \`${entry.setupPriority}\` 정렬 프리셋이 적용됩니다. 모드, QB 관측, 큐 공개 범위, 다음 회차 잔여, 빌리기, 길이, 최대 셋업 미노는 각각 명명 옵션입니다.`,
        "QB 모드에는 `qb`가 필요하고 다음 회차 빌리기는 잔여 3개일 때만 허용합니다. 입력 창에서 숨겨지는 옵션은 조용히 버리지 않습니다.",
        nativeKickHelp,
      ];
    case "setup-score-v1":
      return [
        "`document`는 선언한 CTK3 또는 v115 Fumen 형식의 canonical 색상 셋업 후보 문서입니다. 일반 격자, 회색 전용 문서, 형식 불일치, supplied-solution 대체는 닫힌 실패로 거부합니다.",
        "setup queue와 setup pattern 중 정확히 하나, solution queue와 solution pattern 중 정확히 하나를 입력합니다. 클리어 높이, 홀드, 점수 프로필, 초기 B2B, 규칙, 패턴 제한은 각각 독립된 명명 입력입니다.",
        "Discord는 CPU 실행과 backend fallback 없음으로 고정하고 max-memory 옵션을 노출하지 않습니다. 결과는 일반적인 bounded ranking family이며 tie나 attack 선택 표면이 없습니다.",
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
    case "spin-structure-v2":
      return [
        "`pieces`는 순서 없는 IOTSZJL 미노 목록이며 반복 문자는 수량을 보존하고 홀드는 사용하지 않습니다.",
        `\`field\`는 1–${DISCORD_WIDE_FIELD_MAX_ROWS}줄 또는 정적 문서를 받습니다. 높이, 마지막 스핀 줄 수, 스핀 프로필, 채움 범위, 최대 배치 수, 최소화 정책은 각각 명명 옵션입니다.`,
        "`spin-profile`은 T-Spins, T-Spins+, All-Mini(+), All-Spin(+) 중 하나입니다. Regular와 Mini는 항상 따로 출력하며 `+`는 정확한 immobile T 판정을 추가합니다.",
        "`fill-bottom`은 `fill-top`보다 작아야 하고 `max-placements`는 입력 미노 수를 넘을 수 없습니다.",
        nativeKickHelp,
      ];
    case "spin-structure-cover-v1":
      return [
        "`pieces`는 순서 없는 목록이며 큐나 홀드 공급원으로 해석하지 않습니다. 필드와 구조 제어는 structure search와 같은 닫힌 문법을 사용합니다.",
        "objective는 정확한 minimum cover로 고정됩니다. Discord는 첫 canonical portfolio만 유지하며 alternative 개수, cursor, page, live handle을 노출하지 않습니다.",
        "`max-patterns`는 1–100000입니다. 실행은 CPU 전용이고 backend fallback이 없으며 메모리 예산, tablebase, GPU, 큐, 패턴, 홀드 입력은 거부합니다.",
        "이 authoritative 구조 계산은 `srs-plus`와 `srs`만 허용합니다.",
      ];
    case "spin-structure-guaranteed-v1":
      return [
        "`pieces`는 순서 없는 목록입니다. `final-piece` 기본값은 T이며 목록 안에 있어야 하고 선택한 스핀 프로필과도 일치해야 합니다.",
        "`dependency-report` 기본값은 off이며 명시적인 계산 옵션입니다. 결과는 tie portfolio가 아닌 일반 bounded family입니다.",
        "`max-patterns`는 1–100000입니다. 실행은 CPU 전용이고 backend fallback이 없으며 메모리 예산, tablebase, GPU, 큐, 패턴, 홀드 입력은 거부합니다.",
        "이 authoritative 구조 계산은 `srs-plus`와 `srs`만 허용합니다.",
      ];
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
    case "finesse-score-v2":
      return [
        "`document`에는 모든 페이지에 배치 operation이 하나씩 있는 CTK3 또는 v115 Fumen을 입력합니다. 이 고정 배치 계산 폼은 Build 탐색의 별칭이 아닙니다.",
        "`next`는 정확한 큐 또는 지원되는 패턴을 받으며 `hold`, `knowledge`, `source-pieces`는 각각 명명 옵션입니다.",
        nativeKickHelp,
      ];
    case "operation-document-v1":
      return [
        "`document` 문자열 또는 CTK3 `attachment` 중 정확히 하나를 입력합니다. 모든 페이지에는 잠금된 구체적 operation 하나가 있어야 하며 정적 필드, 큐, 홀드, 미러, rise, quiz, 비어 있지 않은 garbage는 거부됩니다.",
        "`rule-profile`과 `kick-profile`의 기본값은 각각 `srs-plus`입니다. `timeout-seconds`의 기본값과 최댓값은 900초입니다.",
        entry.capabilityId === "utility.sequence"
          ? "보고서는 입력한 operation trace를 손실 없이 정규화하고 모든 잠금 배치의 재생 일치를 확인합니다. Discord에서는 상한이 있는 canonical 요약과 trace 미리보기만 표시합니다."
          : "보고서는 허용되는 operation 순서의 정확한 개수, 보편 선행 관계와 전이 축약, 독립 쌍, 도달성·킥 증거를 계산합니다. Discord에서는 작은 canonical 보고서만 표시합니다.",
      ];
    case "field-document-v1":
      return [
        "canonical CTK3/v115 Fumen `document` 또는 상한이 있는 UTF-8 `attachment` 중 정확히 하나를 입력하며 격자와 링크는 거부됩니다.",
        "Parity는 페이지별 관찰일 뿐입니다. Discord는 반환된 모든 페이지를 확인하되 1페이지 요약만 표시하고 pending garbage를 분리하며 가능성·pruning 권위를 주장하지 않습니다.",
      ];
    case "fumen-transform-v1":
      return [
        "변환은 roundtrip, combine, split, get-page, page-shift, clean-comments, preserve-comments, to-gray, mirror, text-to-fumen으로 닫혀 있습니다.",
        "Combine은 `documents` 또는 첨부파일에서 줄마다 canonical v115 Fumen 하나를 받습니다. Get-page는 1부터 시작하고 양수 page-shift는 왼쪽 회전이며 text-to-fumen은 줄마다 Unicode 주석 하나를 받습니다.",
        "Discord는 짧은 요약과 제한 안의 완전한 canonical Fumen 첨부파일만 반환하며 split 페이지는 portfolio 대안이 아닙니다.",
      ];
    case "render-document-v1":
      return [
        "canonical CTK3/v115 Fumen 문서 하나와 PNG 또는 GIF를 선택하며 `page`는 PNG의 1부터 시작하는 페이지 선택에만 사용합니다.",
        "native job runner가 비공개 임시 출력 경로를 소유하고 Discord는 상한 안의 정확한 Rust 렌더 bytes만 게시하며 서버 경로를 노출하지 않습니다.",
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
  if (entry.rootName && entry.subcommand) {
    return KOREAN_COMMAND_NOTES[`${entry.rootName}.${entry.subcommand}`] ??
      KOREAN_COMMAND_NOTES[entry.name] ?? entry.note;
  }
  return KOREAN_COMMAND_NOTES[entry.name] ?? entry.note;
}

function koreanChoiceName(name, value, path = "") {
  if (path.endsWith(".document-format")) {
    return value === "ctk3" ? "CTK3 문서" : "Fumen v115 문서";
  }
  if (path === "help.arguments" && typeof value === "string") {
    const localized = KOREAN_COMMAND_NAMES[value];
    return localized && localized !== value
      ? `${value} — ${localized}`
      : value;
  }
  if (
    typeof value === "string" &&
    KOREAN_COMMAND_NAMES[value] &&
    !path.endsWith(".priority")
  ) {
    return KOREAN_COMMAND_NAMES[value];
  }
  if (path === "utility.fumen.transform") {
    return ({
      roundtrip: "왕복 정규화",
      combine: "문서 합치기",
      split: "페이지 나누기",
      "get-page": "페이지 가져오기",
      "page-shift": "페이지 이동",
      "clean-comments": "주석 지우기",
      "preserve-comments": "주석 보존",
      "to-gray": "회색으로 변환",
      mirror: "좌우 반전",
      "text-to-fumen": "텍스트를 Fumen으로",
    })[value] ?? name;
  }
  if (path === "utility.render.artifact-format") {
    return ({ png: "PNG 이미지", gif: "GIF 애니메이션" })[value] ?? name;
  }
  if (path.endsWith(".lines") && typeof value === "string") {
    if (value === "any") return "모든 줄 수";
    if (/^[0-4]$/.test(value)) return `정확히 ${value}줄`;
    if (/^[1-4]\+$/.test(value)) return `최소 ${value.slice(0, -1)}줄`;
  }
  if (
    (path === "spin-structure.profile" || path.endsWith(".spin-profile")) &&
    typeof value === "string"
  ) {
    return ({
      disabled: "스핀 판정 안 함",
      "t-spins": "T 스핀",
      "t-spins-plus": "T 스핀+",
      "all-mini": "전체 Mini",
      "all-mini-plus": "전체 Mini+",
      "all-spin": "전체 스핀",
      "all-spin-plus": "전체 스핀+",
    })[value] ?? name;
  }
  if (path.endsWith(".score-profile")) {
    return ({
      tetrio: "TETR.IO (기본값)",
      guideline: "가이드라인",
      "jstris-ultra": "Jstris Ultra 점수",
    })[value] ?? name;
  }
  if (path.endsWith(".objective")) {
    return ({
      "min-cover": "최소 커버",
      "max-probability-minimum": "최대 확률 최소집합",
      unique: "고유 해법",
      all: "전체 해법",
      "max-score-cover": "최대 점수 커버",
    })[value] ?? name;
  }
  if (path.endsWith(".target-format") || path.endsWith(".solution-format")) {
    return ({
      ctk3: "CTK3 문서",
      fumen: "v115 Fumen 문서",
    })[value] ?? name;
  }
  if (path.endsWith(".rule-profile") || path.endsWith(".kick-profile")) {
    return ({
      "srs-plus": "SRS+ (기본값)",
      srs: "SRS 킥",
      "srs-x": "SRS-X 킥",
      "jstris-180": "Jstris 180 킥",
      "no-kick": "킥 없음",
    })[value] ?? name;
  }
  if (path.endsWith(".hold")) {
    if (value === "empty") return "빈 홀드 (기본값)";
    if (value === "disabled") return "홀드 사용 안 함";
    if (typeof value === "string" && /^[IOTSZJL]$/.test(value)) {
      return `${value} 보유`;
    }
  }
  if (path.endsWith(".aggregation")) {
    return ({
      buildability: "빌드 가능성 (기본값)",
      spin: "스핀 커버리지",
      tiling: "기하학 타일링",
    })[value] ?? name;
  }
  if (path.endsWith(".result-mode")) {
    return ({
      "all-solutions": "전체 해법 (기본값)",
      "complete-replay-paths": "전체 리플레이 경로",
      "minimum-solutions": "최소 해법",
      "field-average-score": "필드 평균 점수",
      "fixed-queue-maximum-score": "고정 큐 최고 점수",
      "highest-score-minimum-set": "최고 점수 최소 해법 집합",
      "failed-queues": "실패 큐",
    })[value] ?? name;
  }
  if (path.endsWith(".finesse")) {
    return ({ off: "사용 안 함 (기본값)", inputs: "최소 입력 수" })[value] ?? name;
  }
  if (path.endsWith(".finesse-knowledge") || path.endsWith(".knowledge")) {
    return ({
      both: "전체 큐 및 공개 7개 (기본값)",
      oracle: "전체 미래 큐",
      "visible-7": "공개 7개",
    })[value] ?? name;
  }
  if (path.endsWith(".mirror")) {
    return ({
      auto: "자동 (기본값)",
      include: "포함",
      exclude: "제외",
    })[value] ?? name;
  }
  if (path.endsWith(".mode")) {
    return ({ oracle: "Oracle (기본값)", qb: "관측한 QB 그룹" })[value] ?? name;
  }
  if (path.endsWith(".spin-category")) {
    return ({ any: "모든 미노 (기본값)", t: "T 미노", other: "T 외 미노" })[value] ?? name;
  }
  if (path.endsWith(".damage-mode")) {
    return ({ maximum: "최대값 (기본값)", "at-least": "최소값 이상" })[value] ?? name;
  }
  if (path.endsWith(".minimality")) {
    return ({
      "subset-minimal": "부분집합 최소 (기본값)",
      "minimum-piece-count": "최소 미노 수",
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
  if (value === "auto") return "자동";
  if (value === "on") return /default/i.test(name) ? "사용 (기본값)" : "사용";
  if (value === "off") return /default/i.test(name) ? "사용 안 함 (기본값)" : "사용 안 함";
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
  pc: "pc-탐색",
  build: "빌드",
  forward: "정방향",
  utility: "유틸리티",
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
  probability: "확률",
  setup: "셋업",
  congruent: "합동",
  "congruent-cover": "합동-커버",
  "setup-cover": "셋업-커버",
  "setup-cover-percent": "셋업-커버-퍼센트",
  "setup-cover-score": "셋업-커버-점수",
  "evaluate-cover": "평가-커버",
  "evaluate-minimals": "평가-최소집합",
  "evaluate-score": "평가-점수",
  "evaluate-b2b-cover": "평가-b2b-커버",
  "evaluate-cover-percent": "평가-커버-퍼센트",
  "cover-percent": "커버-퍼센트",
  "special-cover": "특수-커버",
  "spin-cover": "스핀-커버",
  spin: "스핀",
  "score-finder": "score-finder",
  "allspin-sol": "올스핀-해법",
  "allspin-sol-finder": "올스핀-해법-탐색",
  "allspin-pres-chance": "올스핀-보존-확률",
  damage: "대미지",
  "spin-structure": "스핀-구조",
  sequence: "시퀀스",
  "sequence-dependencies": "순서-의존성",
  "pc-setup": "pc-셋업",
  "best-setup": "최적-셋업",
  "dpc-finder": "dpc-탐색",
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
  hold: "홀드",
  height: "높이",
  mode: "모드",
  qb: "qb-미노",
  aggregation: "집계",
  "score-profile": "점수-프로필",
  "result-mode": "결과-집계",
  "spin-profile": "스핀-프로필",
  "preserve-b2b": "b2b-보존",
  "initial-b2b": "초기-b2b",
  "initial-combo": "초기-콤보",
  "solution-probabilities": "해법-확률",
  "failed-count": "실패-개수",
  "max-patterns": "최대-패턴",
  "max-nodes": "최대-노드",
  "max-frontier-states": "최대-전선-상태",
  "max-candidates": "최대-후보",
  "max-memory-mib": "최대-메모리-mib",
  finesse: "피네스",
  "finesse-knowledge": "피네스-공개-범위",
  mirror: "미러",
  "post-cycle-borrow": "다음-회차-차용",
  "damage-mode": "대미지-모드",
  "minimum-damage": "최소-대미지",
  "spin-category": "스핀-종류",
  "fill-bottom": "채움-하단",
  "fill-top": "채움-상단",
  "max-placements": "최대-배치",
  minimality: "최소화-기준",
  knowledge: "공개-범위",
  "source-pieces": "소스-미노-수",
  base: "기존필드",
  target: "목표필드",
  "base-mask": "기존-마스크",
  "target-mask": "목표-마스크",
  "target-format": "목표-형식",
  "target-document": "목표-문서",
  "solution-format": "해법-형식",
  "solution-document": "해법-문서",
  queue: "정확한-큐",
  patterns: "큐-패턴",
  objective: "목표함수",
  remaining: "남은미노",
  priority: "셋업-정렬",
  "max-setup-pieces": "최대-구축-미노",
  "queue-knowledge": "큐-공개-범위",
  "next-cycle-remaining": "다음-회차-남은-미노",
  "setup-length": "셋업-길이",
  scope: "범위",
  language: "언어",
  document: "문서",
  "document-format": "문서-형식",
  "setup-queue": "셋업-큐",
  "setup-patterns": "셋업-패턴",
  "solution-queue": "후속-큐",
  "solution-patterns": "후속-패턴",
  clear: "클리어-높이",
  "final-piece": "마지막-미노",
  "dependency-report": "의존성-보고서",
  documents: "문서-목록",
  attachment: "첨부파일",
  transform: "변환",
  "artifact-format": "결과-형식",
  page: "페이지",
  offset: "이동량",
  comments: "주석-목록",
  "rule-profile": "규칙-프로필",
  "kick-profile": "킥-프로필",
  "timeout-seconds": "제한시간-초",
  "finesse.search": "탐색",
  "finesse.score": "계산",
  "pc.path": "경로",
  "pc.chance": "확률",
  "pc.minimals": "최소집합",
  "pc.score": "점수",
  "pc.saves": "세이브",
  "pc.best-save": "최적-세이브",
  "pc.score-minimals": "최소집합-점수",
  "pc.tiling": "타일링",
  "pc.failed-queue": "실패-큐",
  "pc.score-finder": "점수-탐색",
  "pc.allspin-sol": "올스핀-해법",
  "pc.allspin-pres-chance": "올스핀-보존-확률",
  "build.cover": "커버",
  "build.probability": "확률",
  "build.setup": "셋업",
  "build.congruent": "합동",
  "build.congruent-cover": "합동-커버",
  "build.setup-cover": "셋업-커버",
  "build.setup-cover-percent": "셋업-커버-퍼센트",
  "build.setup-cover-score": "셋업-커버-점수",
  "build.evaluate-cover": "평가-커버",
  "build.evaluate-minimals": "평가-최소집합",
  "build.evaluate-score": "평가-점수",
  "build.evaluate-b2b-cover": "평가-b2b-커버",
  "build.evaluate-cover-percent": "평가-커버-퍼센트",
  "build.evaluate.cover": "평가-커버",
  "build.evaluate.minimals": "평가-최소집합",
  "build.evaluate.score": "평가-점수",
  "build.evaluate.b2b-cover": "평가-b2b-커버",
  "build.evaluate.cover-percent": "평가-커버-퍼센트",
  "build.finesse-score": "피네스-계산",
  "setup.joint": "종합",
  "setup.build": "빌드-우선",
  "setup.pc": "pc-우선",
  "setup.score": "점수",
  "forward.spin": "스핀",
  "forward.damage": "대미지",
  "forward.ren": "렌",
  "spin-structure.search": "탐색",
  "spin-structure.cover": "커버",
  "spin-structure.guaranteed": "보장",
  "utility.sequence": "시퀀스",
  "utility.sequence-dependencies": "순서-의존성",
  "utility.parity": "패리티",
  "utility.fumen": "푸멘",
  "utility.render": "렌더",
  "utility.to-gray": "회색화",
  "utility.mirror": "좌우반전",
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
  pc: "목표 필드를 받지 않는 퍼펙트 클리어 역방향 탐색을 실행합니다",
  build: "기존 필드에서 목표 칸까지의 빌드 탐색과 고정 배치를 계산합니다",
  forward: "순서 있는 넥스트를 사용하는 정방향 상태 탐색을 실행합니다",
  utility: "상태를 보존하지 않는 필드 및 문서 유틸리티를 제한된 시간 안에 실행합니다",
  finesse: "최소 입력 수로 미노 배치를 탐색하거나 계산합니다",
  path: "표현되는 모든 퍼펙트 클리어 경로를 찾습니다",
  percent: "정확한 퍼펙트 클리어 성공 확률을 계산합니다",
  chance: "정확한 퍼펙트 클리어 성공 확률을 계산합니다",
  minimals: "퍼펙트 클리어를 최소 집합으로 커버하는 해법을 찾습니다",
  score: "Jstris 프로필로 퍼펙트 클리어 해법의 점수를 계산합니다",
  "score-minimals": "최소 커버 퍼펙트 클리어 해법 집합의 점수를 계산합니다",
  saves: "종료 홀드와 활성 가방 잔여 미노별 세이브 확률을 계산합니다",
  "best-save": "clearra-save-v1 순위로 최적 세이브 증거를 선택합니다",
  cover: "기존 필드에서 목표 칸까지의 구축 확률을 계산합니다",
  probability: "구축 확률과 정확한 결과 집계를 탐색합니다",
  setup: "명시한 관측 정책으로 셋업 후보의 순위를 계산합니다",
  congruent: "목표 모양의 구축 확률을 계산합니다",
  "congruent-cover": "목표 모양의 구축 확률을 계산합니다",
  "setup-cover": "목표 점유 필드의 구축 확률을 계산합니다",
  "setup-cover-percent": "색상 target 셋업의 정확한 커버 확률을 계산합니다",
  "setup-cover-score": "색상 target 셋업의 score-only 커버 포트폴리오를 찾습니다",
  "evaluate-cover": "제공한 해법 문서의 일반 커버 family를 평가합니다",
  "evaluate-minimals": "제공한 해법 문서의 정확한 최소 커버 포트폴리오를 찾습니다",
  "evaluate-score": "제공한 해법 문서의 score-only 포트폴리오를 찾습니다",
  "evaluate-b2b-cover": "제공한 해법 문서의 일반 B2B 보존 커버 family를 평가합니다",
  "evaluate-cover-percent": "제공한 해법 문서의 정확한 커버 확률을 계산합니다",
  "cover-percent": "목표 점유 필드의 구축 확률을 계산합니다",
  "special-cover": "목표 모양의 T-spin 커버리지를 계산합니다",
  "spin-cover": "선택한 스핀 프로필의 전방 완성 경로를 찾습니다",
  spin: "선택한 스핀 프로필의 전방 완성 경로를 찾습니다",
  "score-finder": "고정 넥스트 큐에서 Jstris 점수가 가장 높은 퍼펙트 클리어를 찾습니다",
  "allspin-sol": "정확한 큐에서 B2B를 보존하는 퍼펙트 클리어 증거를 찾습니다",
  "allspin-sol-finder": "정확한 큐에서 B2B를 보존하는 퍼펙트 클리어 증거를 찾습니다",
  "allspin-pres-chance": "패턴에서 B2B 보존 퍼펙트 클리어 확률을 계산합니다",
  damage: "정확한 넥스트 큐 하나에서 최대 대미지를 찾습니다",
  "spin-structure": "순서 없는 미노 목록에서 부분집합 최소 스핀 구조를 찾습니다",
  sequence: "구체적 배치 문서의 operation trace를 정규화하고 재생 일치를 확인합니다",
  "sequence-dependencies": "구체적 배치 문서에서 정확한 순서 의존성과 독립 관계를 계산합니다",
  parity: "문서의 모든 페이지에서 좌표 기반 패리티 관찰값을 계산합니다",
  fumen: "v115 Fumen에 닫힌 손실 없는 문서 변환을 적용합니다",
  render: "문서를 정확한 PNG 한 페이지 또는 GIF 타임라인으로 렌더합니다",
  "pc-setup": "구축 및 PC 커버리지로 셋업 후보의 순위를 정합니다",
  "best-setup": "구축 커버리지로 셋업 후보의 순위를 정합니다",
  "dpc-finder": "퍼펙트 클리어 커버리지로 셋업 후보의 순위를 정합니다",
});

const KOREAN_COMMAND_NOTES = Object.freeze({
  "pc.allspin-sol": "기존 allspin_sol_finder와의 호환은 명령 의도만 보장하며 상위 도구의 판정을 그대로 재현한다는 뜻이 아닙니다. 슬래시 별칭은 v0.10에 제거되고 텍스트 별칭은 장기 유지됩니다.",
  "pc.allspin-pres-chance": "기존 allspin_pres_chance와의 호환은 명령 의도만 보장하며 상위 도구의 판정을 그대로 재현한다는 뜻이 아닙니다. 슬래시 별칭은 v0.10에 제거되고 텍스트 별칭은 장기 유지됩니다.",
  "allspin-sol-finder": "명령 의도 호환만 제공하며 정확한 상위 도구 판정 일치를 보장하지 않습니다. 슬래시 별칭은 v0.10에 제거되고 텍스트 별칭은 장기 유지됩니다.",
  "allspin-pres-chance": "명령 의도 호환만 제공하며 정확한 상위 도구 판정 일치를 보장하지 않습니다. 슬래시 별칭은 v0.10에 제거되고 텍스트 별칭은 장기 유지됩니다.",
  "forward.spin": "기존 /spin과 /spin-cover는 Clearra 호환 별칭입니다. sfinder spin/spincover의 순서 없는 구조 탐색과는 별개입니다.",
  "build.probability": "결과 모드 호환성 표는 CLI가 소유한 구축 확률 계약에서 강제합니다.",
  "spin-structure": "타입이 지정된 구조 탐색 형제가 실제 제공될 때까지 최상위 명령어 형태를 유지합니다. Discord에서는 최상위 옵션과 하위 명령어를 섞을 수 없습니다.",
  percent: "/chance와 같은 기능입니다.",
  chance: "/percent와 같은 기능입니다.",
  saves: "그룹별 전체 우주 무조건 확률과 PC 성공 조건부 확률을 함께 표시합니다.",
  "best-save": "세이브 그룹 전체를 나열하지 않고 clearra-save-v1 최적 증거를 선택합니다.",
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
  "pc.path": "표현되는 모든 퍼펙트 클리어 경로를 찾습니다",
  "pc.chance": "정확한 퍼펙트 클리어 성공 확률을 계산합니다",
  "pc.minimals": "최소 커버 퍼펙트 클리어 해법 집합을 찾습니다",
  "pc.score": "명시한 프로필로 퍼펙트 클리어 해법의 점수를 계산합니다",
  "pc.saves": "종료 홀드와 활성 가방 잔여 미노별 정확한 세이브 확률을 계산합니다",
  "pc.best-save": "clearra-save-v1 순위로 최적 세이브 증거를 선택합니다",
  "pc.score-minimals": "최소 커버 퍼펙트 클리어 해법 집합의 점수를 계산합니다",
  "pc.tiling": "회전·도달 가능성을 제외한 기하학적 PC 타일링을 열거합니다",
  "pc.failed-queue": "요청한 퍼펙트 클리어를 만들 수 없는 큐를 찾습니다",
  "pc.score-finder": "정확한 넥스트에서 Jstris 최고 점수 퍼펙트 클리어를 찾습니다",
  "pc.allspin-sol": "정확한 큐에서 B2B를 보존하는 퍼펙트 클리어 증거를 찾습니다",
  "pc.allspin-pres-chance": "패턴 큐 전체집합에서 B2B 보존 퍼펙트 클리어 확률을 계산합니다",
  "build.cover": "기존 필드에서 목표 칸까지의 빌드 확률을 계산합니다",
  "build.probability": "구축 확률과 정확한 결과 집계를 탐색합니다",
  "build.setup": "색상 target의 unique 빌드 family를 찾습니다",
  "build.congruent": "색상 target의 합동 빌드 family를 찾습니다",
  "build.congruent-cover": "색상 합동 target의 정확한 커버 포트폴리오를 찾습니다",
  "build.setup-cover": "색상 target 셋업의 정확한 커버 포트폴리오를 찾습니다",
  "build.setup-cover-percent": "색상 target 셋업의 정확한 커버 확률을 계산합니다",
  "build.setup-cover-score": "색상 target 셋업의 score-only 커버 포트폴리오를 찾습니다",
  "build.evaluate-cover": "제공한 해법 문서의 일반 커버 family를 평가합니다",
  "build.evaluate-minimals": "제공한 해법 문서의 정확한 최소 커버 포트폴리오를 찾습니다",
  "build.evaluate-score": "제공한 해법 문서의 score-only 포트폴리오를 찾습니다",
  "build.evaluate-b2b-cover": "제공한 해법 문서의 일반 B2B 보존 커버 family를 평가합니다",
  "build.evaluate-cover-percent": "제공한 해법 문서의 정확한 커버 확률을 계산합니다",
  "build.evaluate.cover": "제공한 해법 문서의 일반 커버 family를 평가합니다",
  "build.evaluate.minimals": "제공한 해법 문서의 정확한 최소 커버 포트폴리오를 찾습니다",
  "build.evaluate.score": "제공한 해법 문서의 score-only 포트폴리오를 찾습니다",
  "build.evaluate.b2b-cover": "제공한 해법 문서의 일반 B2B 보존 커버 family를 평가합니다",
  "build.evaluate.cover-percent": "제공한 해법 문서의 정확한 커버 확률을 계산합니다",
  "build.finesse-score": "주어진 배치 순서의 최소 입력 수를 계산합니다",
  "setup.joint": "빌드와 PC 기준을 함께 사용해 셋업 후보의 순위를 계산합니다",
  "setup.build": "빌드 가능성을 우선해 셋업 후보의 순위를 계산합니다",
  "setup.pc": "PC 가능성을 우선해 셋업 후보의 순위를 계산합니다",
  "setup.score": "색상 셋업 문서 후보를 명시한 후속 공급으로 점수화합니다",
  "forward.spin": "선택한 스핀 프로필의 순서 있는 전방 완성 경로를 찾습니다",
  "forward.damage": "정확한 넥스트 큐에서 명시한 피해 목표를 탐색합니다",
  "forward.ren": "정확한 넥스트 큐에서 최대 REN 경로를 찾습니다",
  "spin-structure.search": "순서 없는 미노에서 부분집합 최소 스핀 구조를 찾습니다",
  "spin-structure.cover": "요청한 스핀 패턴을 덮는 최소 구조 포트폴리오를 찾습니다",
  "spin-structure.guaranteed": "요청한 마지막 미노가 보장되는 스핀 구조를 찾습니다",
  "utility.sequence": "구체적 배치 문서의 operation trace를 정규화하고 재생 일치를 확인합니다",
  "utility.sequence-dependencies": "구체적 배치 문서에서 정확한 순서 의존성과 독립 관계를 계산합니다",
  "utility.parity": "가능성이나 pruning 권위 없이 페이지별 패리티 관찰값을 계산합니다",
  "utility.fumen": "v115 Fumen에 선택한 닫힌 문서 변환을 적용합니다",
  "utility.render": "동일한 Rust renderer로 정확한 PNG 또는 GIF를 만듭니다",
  "utility.to-gray": "점유 색상만 회색으로 바꾸고 문서 identity를 보존합니다",
  "utility.mirror": "필드와 operation 조각·회전을 함께 좌우 반전합니다",
  "forward.spin.lines": "선택한 마지막 스핀이 지울 줄 수이며 기본값은 제한 없음입니다",
  arguments: "설명을 볼 명령어이며 생략하면 전체 명령어 그룹을 표시합니다",
  hold: "탐색 시작 시 사용할 홀드 상태 또는 홀드 사용 여부입니다",
  height: "입력 필드를 모두 포함하는 탐색 높이입니다",
  mode: "셋업 후보가 관측할 공급 모드입니다",
  qb: "qb 모드에서 이미 관측한 현재 가방 미노입니다",
  aggregation: "빌드 가능성, 스핀 커버리지, 기하학 타일링 중 결과 집계 방식입니다",
  "result-mode": "구축 확률 탐색에 적용할 정확한 결과 집계 방식입니다",
  "score-profile": "퍼펙트 클리어 해법에 적용할 점수 프로필입니다",
  "spin-profile": "스핀을 판정할 프로필이며 Regular와 Mini는 분리됩니다",
  "preserve-b2b": "선택한 결과가 백투백 상태를 보존하도록 요구합니다",
  "initial-b2b": "탐색 시작 시의 백투백 연속 상태입니다",
  "initial-combo": "탐색 시작 시의 콤보 연속 상태입니다",
  "solution-probabilities": "각 해법의 정확한 확률을 결과에 포함합니다",
  "failed-count": "결과에 포함할 실패 패턴의 최대 개수입니다",
  "max-patterns": "구체화할 큐 패턴의 최대 개수입니다",
  "max-nodes": "불완전 결과를 반환하기 전 탐색할 최대 노드 수입니다",
  "max-frontier-states": "유지할 탐색 전선 상태의 최대 개수입니다",
  "max-candidates": "유지할 B2B 보존 후보의 최대 개수입니다",
  "max-memory-mib": "탐색에 사용할 최대 메모리 MiB입니다",
  finesse: "빌드 결과에 최소 입력 수 계산을 포함할지 선택합니다",
  "finesse-knowledge": "최소 입력 수 계산에 공개되는 패턴 정보입니다",
  mirror: "좌우 반전 해법을 포함할지 선택합니다",
  "post-cycle-borrow": "다음 가방에서 미노 하나를 빌리도록 허용합니다",
  "damage-mode": "최대 피해 또는 최소 피해 임계값 탐색을 선택합니다",
  "minimum-damage": "최소 피해 임계값 모드에서 요구하는 피해량입니다",
  "spin-category": "마지막 스핀의 미노 종류를 제한합니다",
  "fill-bottom": "구조가 채울 수 있는 영역의 아래쪽 경계입니다",
  "fill-top": "구조가 채울 수 있는 영역의 위쪽 경계입니다",
  "max-placements": "구조 탐색에서 사용할 수 있는 최대 배치 수입니다",
  minimality: "부분집합 최소 또는 최소 미노 수 결과를 선택합니다",
  knowledge: "고정 배치 계산에 공개되는 넥스트 정보입니다",
  "source-pieces": "패턴에서 구체화할 최대 소스 미노 수입니다",
  "base-mask": "기존 보드를 나타내는 정규 10진수 또는 0x 접두 16진수 마스크입니다",
  "target-mask": "추가할 목표 칸을 나타내는 정규 10진수 또는 0x 접두 16진수 마스크입니다",
  "target-format": "명목 색상 target 문서의 형식이며 ctk3 또는 fumen입니다",
  "target-document": "미노 색상을 보존한 명목 target CTK3 또는 v115 Fumen 문서입니다",
  "solution-format": "제공한 색상 해법 문서의 형식이며 ctk3 또는 fumen입니다",
  "solution-document": "평가할 미노 색상 보존 CTK3 또는 v115 Fumen 해법 문서입니다",
  "document-format": "입력 문서와 정확히 일치하는 ctk3 또는 fumen 형식입니다",
  "setup-queue": "setup-patterns와 함께 쓸 수 없는 정확한 셋업 큐입니다",
  "setup-patterns": "setup-queue와 함께 쓸 수 없는 셋업 큐 패턴입니다",
  "solution-queue": "solution-patterns와 함께 쓸 수 없는 정확한 후속 큐입니다",
  "solution-patterns": "solution-queue와 함께 쓸 수 없는 후속 큐 패턴입니다",
  clear: "퍼펙트 클리어 목표 높이이며 1–6줄입니다",
  "final-piece": "보장된 구조에서 마지막에 배치할 미노입니다",
  "dependency-report": "구조 의존성 보고서를 계산할지 선택합니다",
  queue: "정확한 IOTSZJL 큐이며 patterns와 함께 사용할 수 없습니다",
  patterns: "지원되는 큐 패턴이며 queue와 함께 사용할 수 없습니다",
  objective: "이 Build capability가 허용하는 닫힌 목표 함수입니다",
  attachment: "문자열 문서 대신 사용할 상한이 있는 UTF-8 CTK3 또는 v115 Fumen 파일입니다",
  transform: "허용된 닫힌 Fumen 변환 하나를 선택합니다",
  documents: "combine에 사용할 canonical v115 Fumen을 비어 있지 않은 줄마다 하나씩 입력합니다",
  "artifact-format": "정확한 Rust renderer가 만들 PNG 또는 GIF 형식입니다",
  page: "get-page 또는 PNG 렌더에서 사용하는 1부터 시작하는 페이지 번호입니다",
  offset: "page-shift의 부호 있는 이동량이며 양수는 왼쪽 회전입니다",
  comments: "text-to-fumen에 사용할 Unicode 주석을 줄마다 하나씩 입력합니다",
  "rule-profile": "배치 도달성에 사용할 내장 이동 규칙 프로필이며 기본값은 SRS+입니다",
  "kick-profile": "회전 도달성에 사용할 내장 킥 프로필이며 기본값은 SRS+입니다",
  "timeout-seconds": "정확 분석 제한시간은 1–900초이며 기본값은 900초입니다",
  "build.cover.source-pieces": "정확한 소스 미노 창이며 생략하면 목표와 초기 홀드에서 자동 계산합니다",
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
  pieces: "순서 없는 IOTSZJL 미노 목록이며 반복 문자는 수량을 보존합니다",
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
