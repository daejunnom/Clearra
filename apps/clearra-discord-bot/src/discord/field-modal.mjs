import {
  BUILTIN_KICKTABLES,
  findSlashCommand,
  localizedSlashCommandName,
  resolveSlashCommandInvocation,
} from "./slash-command-catalog.mjs";
import { normalizeDiscordLocale } from "./i18n.mjs";
import {
  DISCORD_PC_FIELD_MAX_ROWS,
  DISCORD_WIDE_FIELD_MAX_ROWS,
  requiresDiscordFieldModal,
} from "./slash-command-input.mjs";

const APPLICATION_COMMAND_INTERACTION = 2;
const MODAL_SUBMIT_INTERACTION = 5;
const CHAT_INPUT_COMMAND = 1;
const MODAL_RESPONSE = 9;
const ACTION_ROW_COMPONENT = 1;
const STRING_SELECT_COMPONENT = 3;
const TEXT_INPUT_COMPONENT = 4;
const LABEL_COMPONENT = 18;
const SHORT_TEXT_INPUT = 1;
const PARAGRAPH_TEXT_INPUT = 2;
const MODAL_TEXT_LIMIT = 4000;
const MODAL_COMPONENT_LIMIT = 5;
const MODAL_ID_PREFIX = "clearra:search:v4:";
const V3_MODAL_ID_PREFIX = "clearra:search:v3:";
const V2_MODAL_ID_PREFIX = "clearra:search:v2:";
const LEGACY_MODAL_ID_PREFIX = "clearra:board:v1:";
const PC_TARGET_ROWS = Object.freeze(Array.from(
  { length: DISCORD_PC_FIELD_MAX_ROWS },
  (_, index) => index + 1,
));

const INPUT_SCHEMAS = Object.freeze({
  pc: schema(["field", "next", "lines", "kicktable", "options"], ["field", "next"], ["field"]),
  cover: schema(["base", "target", "next", "kicktable", "options"], ["base", "target", "next"], ["base", "target"]),
  colored: schema(["field", "next", "kicktable"], ["field", "next"], ["field"]),
  spin: schema(["field", "next", "kicktable", "options"], ["field", "next"], ["field"]),
  "fixed-next": schema(["field", "next", "kicktable"], ["field", "next"], ["field"]),
  "score-fixed-next": schema(["field", "next", "lines", "kicktable", "options"], ["field", "next"], ["field"]),
  "spin-structure": schema(
    ["pieces", "field", "lines", "profile", "kicktable"],
    ["pieces", "field"],
    ["field"],
  ),
  remaining: schema(
    ["remaining", "kicktable", "priority", "max-setup-pieces", "queue-knowledge"],
    ["remaining"],
    [],
  ),
  verify: schema(["scope"], [], [], true),
  "finesse-search": schema(
    ["target", "next", "base", "kicktable", "options"],
    ["target", "next", "base"],
    ["target", "base"],
  ),
  "finesse-score": schema(
    ["document", "next", "kicktable", "options"],
    ["document", "next"],
    [],
  ),
});
const V3_REMAINING_SCHEMA = schema(
  ["remaining", "kicktable"],
  ["remaining"],
  [],
);

export function buildCommandModalResponse(interaction, locale = "en") {
  if (
    interaction?.type !== APPLICATION_COMMAND_INTERACTION ||
    interaction.data?.type !== CHAT_INPUT_COMMAND
  ) return null;
  const rootCommand = findSlashCommand(interaction.data?.name);
  const invocation = rootCommand
    ? resolveSlashCommandInvocation(rootCommand, interaction.data?.options ?? [])
    : null;
  const command = invocation?.command ?? null;
  const inputSchema = inputSchemaFor(command);
  if (!command || !inputSchema) return null;
  const language = normalizeDiscordLocale(locale);

  const supplied = readSlashValues(invocation.rawOptions, command);
  const missingRequired = inputSchema.required.some((name) => !supplied.has(name));
  const richTextBoard = inputSchema.boards.some((name) =>
    supplied.has(name) && requiresDiscordFieldModal(supplied.get(name))
  );
  if (
    command.input === "remaining" &&
    missingRequired &&
    ["next-cycle-remaining", "setup-length"].some((name) => supplied.has(name))
  ) {
    throw new Error(
      "When next-cycle-remaining or setup-length is set, remaining must also be supplied directly.",
    );
  }
  if (
    !missingRequired &&
    !richTextBoard &&
    !(inputSchema.openWhenEmpty && supplied.size === 0)
  ) return null;

  const optionsByName = new Map(
    command.registration.options.map((option) => [option.name, option]),
  );
  const components = inputSchema.order.map((name) => {
    const option = optionsByName.get(name);
    if (!option) {
      throw new Error(`Clearra Modal schema references unknown option '${name}'.`);
    }
    return modalLabel(command, option, supplied, inputSchema, language);
  });
  if (inputSchema.localeSelector) {
    components.push(localeModalLabel(language));
  }
  const response = Object.freeze({
    type: MODAL_RESPONSE,
    data: Object.freeze({
      custom_id: `${MODAL_ID_PREFIX}${modalCommandKey(command)}`,
      title: (language === "ko"
        ? `${localizedSlashCommandName(command.rootName ?? command.name, language)}${command.subcommand ? ` ${localizedFinesseSubcommand(command.subcommand, language)}` : " 탐색"} 입력`
        : `${command.rootName ?? command.name}${command.subcommand ? ` ${command.subcommand}` : " search"} form`
      ).slice(0, 45),
      components: Object.freeze(components),
    }),
  });
  validateModalResponse(response);
  return response;
}

export function findCommandModalCommand(interaction) {
  const route = commandModalRoute(interaction);
  return route?.command ?? null;
}

export function readCommandModalOptions(interaction, command) {
  const route = commandModalRoute(interaction);
  if (modalCommandKey(route?.command) !== modalCommandKey(command)) {
    throw new Error("Discord supplied an unknown or outdated Clearra command Modal.");
  }
  const inputSchema = inputSchemaForVersion(command, route.version);
  const expected = new Map(
    command.registration.options.map((option) => [option.name, option]),
  );
  const submitted = new Map();
  for (const component of flattenModalComponents(interaction.data?.components)) {
    const name = typeof component?.custom_id === "string" ? component.custom_id : "";
    if (name === "locale" && route.version >= 3 && inputSchema.localeSelector) {
      readSubmittedLocale(component);
      continue;
    }
    const option = expected.get(name);
    if (!option) {
      throw new Error(`Discord supplied unsupported Modal input '${name || "unknown"}'.`);
    }
    if (submitted.has(name)) {
      throw new Error(`Discord supplied Modal input '${name}' more than once.`);
    }
    submitted.set(name, readSubmittedComponent(command, option, component));
  }

  if (route.version >= 2) {
    for (const name of inputSchema.order) {
      if (!submitted.has(name)) {
        throw new Error(`Discord command Modal omitted expected input '${name}'.`);
      }
    }
  }

  const options = [];
  for (const [name, option] of expected) {
    const raw = submitted.get(name);
    if (raw === undefined || raw === null || String(raw).trim() === "") {
      if (inputSchema.required.includes(name)) {
        throw new Error(`${name} is required in the Clearra command Modal.`);
      }
      continue;
    }
    if (route.version >= 2 && isOmittedSelectValue(name, raw)) continue;
    if (option.type === 4) {
      if (!/^\d+$/.test(String(raw).trim())) {
        throw new Error(`${name} must be an integer.`);
      }
      options.push({ name, value: Number(String(raw).trim()) });
    } else {
      options.push({ name, value: String(raw) });
    }
  }
  return options;
}

export function readCommandModalLocale(interaction) {
  const route = commandModalRoute(interaction);
  if (!route || route.version < 3) return null;
  const inputSchema = inputSchemaForVersion(route.command, route.version);
  if (!inputSchema.localeSelector) return null;
  const locales = flattenModalComponents(interaction.data?.components)
    .filter((component) => component?.custom_id === "locale");
  if (locales.length !== 1) {
    throw new Error("Discord command input form must contain one language selector.");
  }
  return readSubmittedLocale(locales[0]);
}

// Rolling-deploy aliases keep v1 imports and in-flight Modal submissions valid.
export const buildMissingBoardModalResponse = buildCommandModalResponse;
export const findFieldModalCommand = findCommandModalCommand;
export const readFieldModalOptions = readCommandModalOptions;

function schema(order, required, boards, openWhenEmpty = false) {
  return Object.freeze({
    order: Object.freeze(order),
    required: Object.freeze(required),
    boards: Object.freeze(boards),
    openWhenEmpty,
    localeSelector: order.length < MODAL_COMPONENT_LIMIT,
  });
}

function modalCommandKey(command) {
  if (!command) return "";
  const root = command.rootName ?? command.name;
  return command.subcommand ? `${root}~${command.subcommand}` : root;
}

function localizedFinesseSubcommand(subcommand, locale) {
  if (normalizeDiscordLocale(locale) !== "ko") return subcommand;
  return subcommand === "search" ? "탐색" : subcommand === "score" ? "계산" : subcommand;
}

function commandModalRoute(interaction) {
  if (interaction?.type !== MODAL_SUBMIT_INTERACTION) return null;
  const customId = interaction.data?.custom_id;
  if (typeof customId !== "string") return null;
  let version;
  let name;
  if (customId.startsWith(MODAL_ID_PREFIX)) {
    version = 4;
    name = customId.slice(MODAL_ID_PREFIX.length);
  } else if (customId.startsWith(V3_MODAL_ID_PREFIX)) {
    version = 3;
    name = customId.slice(V3_MODAL_ID_PREFIX.length);
  } else if (customId.startsWith(V2_MODAL_ID_PREFIX)) {
    version = 2;
    name = customId.slice(V2_MODAL_ID_PREFIX.length);
  } else if (customId.startsWith(LEGACY_MODAL_ID_PREFIX)) {
    version = 1;
    name = customId.slice(LEGACY_MODAL_ID_PREFIX.length);
  } else {
    return null;
  }
  if (!name || name.includes(":")) return null;
  const [rootName, subcommand, extra] = name.split("~");
  if (extra !== undefined) return null;
  const root = findSlashCommand(rootName);
  const command = subcommand
    ? root?.input === "finesse"
      ? root.subcommands?.[subcommand] ?? null
      : null
    : root;
  const inputSchema = inputSchemaForVersion(command, version);
  if (!command || !inputSchema) return null;
  if (version === 1 && inputSchema.boards.length === 0) return null;
  return Object.freeze({ command, version });
}

function modalLabel(command, option, supplied, inputSchema, locale) {
  const select = modalSelectSpec(command, option.name, locale);
  const component = select
    ? selectComponent(command, option, supplied, select)
    : textComponent(command, option, supplied, inputSchema);
  return Object.freeze({
    type: LABEL_COMPONENT,
    label: modalLabelText(command.input, option.name, locale),
    description: modalDescription(command.input, option.name, locale),
    component: Object.freeze(component),
  });
}

function localeModalLabel(locale) {
  return Object.freeze({
    type: LABEL_COMPONENT,
    label: locale === "ko" ? "언어" : "Language",
    description: locale === "ko"
      ? "이 요청의 응답 언어입니다. 채널·서버 기본값보다 우선합니다."
      : "Response language for this request; overrides channel and server defaults.",
    component: Object.freeze({
      type: STRING_SELECT_COMPONENT,
      custom_id: "locale",
      placeholder: locale === "ko" ? "응답 언어 선택" : "Choose response language",
      required: true,
      min_values: 1,
      max_values: 1,
      options: Object.freeze([
        Object.freeze({ label: locale === "ko" ? "영어" : "English", value: "en", ...(locale === "en" ? { default: true } : {}) }),
        Object.freeze({ label: locale === "ko" ? "한국어" : "Korean", value: "ko", ...(locale === "ko" ? { default: true } : {}) }),
      ]),
    }),
  });
}

function textComponent(command, option, supplied, inputSchema) {
  const board = inputSchema.boards.includes(option.name);
  const required = inputSchema.required.includes(option.name);
  const value = supplied.has(option.name)
    ? modalValue(supplied.get(option.name), option.name)
    : board
      ? emptyGrid(defaultBoardRows(command.input))
      : undefined;
  return {
    type: TEXT_INPUT_COMPONENT,
    custom_id: option.name,
    style: board ? PARAGRAPH_TEXT_INPUT : SHORT_TEXT_INPUT,
    required,
    max_length: modalMaximum(option),
    placeholder: modalPlaceholder(command.input, option.name),
    ...(required ? { min_length: 1 } : {}),
    ...(value === undefined ? {} : { value }),
  };
}

function selectComponent(command, option, supplied, spec) {
  const selected = supplied.has(option.name)
    ? normalizeSelectValue(command.input, option.name, supplied.get(option.name))
    : spec.defaultValue;
  if (!spec.options.some(({ value }) => value === selected)) {
    throw new Error(`${option.name} has a value that cannot be represented in the Clearra Modal.`);
  }
  return {
    type: STRING_SELECT_COMPONENT,
    custom_id: option.name,
    placeholder: spec.placeholder,
    required: true,
    min_values: 1,
    max_values: 1,
    options: Object.freeze(
      spec.options.map((choice) => Object.freeze({
        ...choice,
        ...(choice.value === selected ? { default: true } : {}),
      })),
    ),
  };
}

function readSlashValues(rawOptions, command) {
  if (!Array.isArray(rawOptions)) {
    throw new Error("Discord supplied invalid slash-command options.");
  }
  const allowed = new Map(
    command.registration.options.map((option) => [option.name, option]),
  );
  const values = new Map();
  for (const option of rawOptions) {
    const name = typeof option?.name === "string" ? option.name : "";
    if (!allowed.has(name)) {
      throw new Error(`Discord supplied unsupported option '${name || "unknown"}'.`);
    }
    if (values.has(name)) {
      throw new Error(`Discord supplied option '${name}' more than once.`);
    }
    values.set(name, option.value);
  }
  return values;
}

function readSubmittedComponent(command, option, component) {
  if (component?.type === TEXT_INPUT_COMPONENT) {
    if (typeof component.value !== "string") {
      throw new Error(`Discord Modal input '${option.name}' must be text.`);
    }
    if (component.value.length > MODAL_TEXT_LIMIT) {
      throw new Error(`Discord Modal input '${option.name}' exceeds 4,000 characters.`);
    }
    return component.value;
  }
  if (component?.type === STRING_SELECT_COMPONENT) {
    if (!Array.isArray(component.values) || component.values.length !== 1) {
      throw new Error(`Discord Modal select '${option.name}' must contain exactly one value.`);
    }
    const value = component.values[0];
    if (typeof value !== "string" || value.length > 100) {
      throw new Error(`Discord Modal select '${option.name}' contains an invalid value.`);
    }
    const spec = modalSelectSpec(command, option.name);
    if (!spec || !spec.options.some((choice) => choice.value === value)) {
      throw new Error(`Discord Modal select '${option.name}' contains unsupported value '${value}'.`);
    }
    return value;
  }
  throw new Error("Discord supplied an unsupported command Modal component.");
}

function readSubmittedLocale(component) {
  if (
    component?.type !== STRING_SELECT_COMPONENT ||
    !Array.isArray(component.values) ||
    component.values.length !== 1 ||
    !["en", "ko"].includes(component.values[0])
  ) {
    throw new Error("Discord supplied an invalid language selection.");
  }
  return component.values[0];
}

function modalSelectSpec(command, name, locale = "en") {
  const input = command?.input ?? "";
  const korean = normalizeDiscordLocale(locale) === "ko";
  if (input === "spin-structure" && name === "lines") {
    return selectSpec(
      "1+",
      korean ? "마지막 스핀의 줄 수 선택" : "Choose terminal-spin line count",
      [
        { label: korean ? "모든 줄 수" : "Any line count", value: "any" },
        ...Array.from({ length: 5 }, (_, value) => ({
          label: korean
            ? `정확히 ${value}줄`
            : `Exactly ${value} line${value === 1 ? "" : "s"}`,
          value: String(value),
        })),
        ...Array.from({ length: 4 }, (_, index) => {
          const value = index + 1;
          return {
            label: korean
              ? `최소 ${value}줄`
              : `At least ${value} line${value === 1 ? "" : "s"}`,
            value: `${value}+`,
          };
        }),
      ],
    );
  }
  if (input === "spin-structure" && name === "profile") {
    return selectSpec(
      "t-spins",
      korean ? "스핀 판정 프로필 선택" : "Choose a spin-recognition profile",
      [
        { label: korean ? "T 스핀" : "T-Spins", value: "t-spins" },
        { label: korean ? "T 스핀+" : "T-Spins+", value: "t-spins-plus" },
        { label: korean ? "전체 Mini" : "All-Mini", value: "all-mini" },
        { label: korean ? "전체 Mini+" : "All-Mini+", value: "all-mini-plus" },
        { label: korean ? "전체 스핀" : "All-Spin", value: "all-spin" },
        { label: korean ? "전체 스핀+" : "All-Spin+", value: "all-spin-plus" },
      ],
    );
  }
  if (name === "lines") {
    if (input === "score-fixed-next") {
      return selectSpec(
        "4",
        korean ? "퍼펙트 클리어 줄 수 선택" : "Choose perfect-clear rows",
        PC_TARGET_ROWS.map((value) => ({
          label: korean ? `${value}줄` : `${value} row${value === 1 ? "" : "s"}`,
          value: String(value),
        })),
      );
    }
    return selectSpec(
      "auto",
      korean ? "PC 목표 줄 선택" : "Choose PC target rows",
      [
        {
          label: korean
            ? `자동 — 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 전체 판정`
            : `Auto — evaluate rows 1–${DISCORD_PC_FIELD_MAX_ROWS}`,
          value: "auto",
        },
        ...PC_TARGET_ROWS.map((value) => ({
          label: korean ? `${value}줄` : `${value} row${value === 1 ? "" : "s"}`,
          value: String(value),
        })),
      ],
    );
  }
  if (name === "kicktable") {
    return selectSpec(
      "srs-plus",
      korean ? "내장 킥테이블 선택" : "Choose a built-in kick table",
      BUILTIN_KICKTABLES.map(({ name: label, value }) => ({
        label: korean && value === "srs-plus" ? "SRS+ (기본값)" : label,
        value,
      })),
    );
  }
  if (input === "remaining" && name === "priority") {
    const defaultValue = command.setupPriority ?? "all";
    return selectSpec(
      defaultValue,
      korean ? "셋업 정렬 기준 선택" : "Choose setup ordering",
      [
        [korean ? "구축 × PC 종합" : "Joint build × PC", "all"],
        [korean ? "구축 확률 우선" : "Build probability first", "build"],
        [korean ? "PC 확률 우선" : "PC probability first", "pc"],
      ].map(([label, value]) => ({
        label: value === defaultValue
          ? `${label}${korean ? " (기본값)" : " (default)"}`
          : label,
        value,
      })),
    );
  }
  if (input === "remaining" && name === "max-setup-pieces") {
    return selectSpec(
      "9",
      korean ? "최대 구축 미노 수 선택" : "Choose maximum setup pieces",
      Array.from({ length: 10 }, (_, index) => {
        const value = index + 1;
        return {
          label: korean
            ? `${value}개${value === 9 ? " (기본값)" : ""}`
            : `${value} piece${value === 1 ? "" : "s"}${value === 9 ? " (default)" : ""}`,
          value: String(value),
        };
      }),
    );
  }
  if (input === "remaining" && name === "queue-knowledge") {
    return selectSpec(
      "oracle",
      korean ? "큐 공개 범위 선택" : "Choose queue knowledge",
      [
        {
          label: korean ? "전체 미래 큐 (기본값)" : "Full future queue (default)",
          value: "oracle",
        },
        {
          label: korean ? "공개 7개" : "Visible 7 pieces",
          value: "visible-7",
        },
      ],
    );
  }
  if (name === "options" && input === "spin") {
    return selectSpec("type=TSS", korean ? "T-spin 목표 선택" : "Choose a T-spin target", [
      { label: korean ? "T-spin 싱글" : "T-spin single", value: "type=TSS" },
      { label: korean ? "T-spin 더블" : "T-spin double", value: "type=TSD" },
      { label: korean ? "T-spin 트리플" : "T-spin triple", value: "type=TST" },
      { label: korean ? "모든 T-spin" : "Any T-spin", value: "type=ANY" },
    ]);
  }
  if (name === "options" && input === "score-fixed-next") {
    return selectSpec(
      "initial_b2b=false",
      korean ? "초기 B2B 상태 선택" : "Choose initial B2B state",
      [
        {
          label: korean ? "초기 B2B 사용 안 함 (기본값)" : "Initial B2B disabled (default)",
          value: "initial_b2b=false",
        },
        {
          label: korean ? "초기 B2B 사용" : "Initial B2B enabled",
          value: "initial_b2b=true",
        },
      ],
    );
  }
  if (name === "options" && (input === "pc" || input === "cover")) {
    return selectSpec("hold=use", korean ? "홀드 정책 선택" : "Choose hold policy", [
      { label: korean ? "홀드 사용" : "Use hold", value: "hold=use" },
      { label: korean ? "홀드 사용 안 함" : "Avoid hold", value: "hold=avoid" },
    ]);
  }
  if (name === "options" && (input === "finesse-search" || input === "finesse-score")) {
    const choices = [];
    for (const [holdLabel, hold] of [
      [korean ? "홀드 사용" : "Use hold", "use"],
      [korean ? "홀드 사용 안 함" : "Avoid hold", "avoid"],
    ]) {
      for (const [knowledgeLabel, knowledge] of [
        [korean ? "전체 큐 및 공개 7개" : "Full queue and visible 7", "both"],
        [korean ? "전체 큐" : "Full queue", "oracle"],
        [korean ? "공개 7개" : "Visible 7", "visible-7"],
      ]) {
        choices.push({
          label: `${holdLabel} · ${knowledgeLabel}`,
          value: `hold=${hold} knowledge=${knowledge}`,
        });
      }
    }
    return selectSpec(
      "hold=use knowledge=both",
      korean ? "홀드 및 큐 공개 정책 선택" : "Choose hold and queue knowledge",
      choices,
    );
  }
  if (name === "scope" && input === "verify") {
    return selectSpec("all", korean ? "검증 범위 선택" : "Choose verification scope", [
      { label: korean ? "전체 검증" : "All checks", value: "all" },
      ...["pc", "setup", "cover", "build", "kicks"].map((value) => ({
        label: korean ? KOREAN_VERIFY_SCOPE_LABELS[value] : value,
        value,
      })),
    ]);
  }
  return null;
}

const KOREAN_VERIFY_SCOPE_LABELS = Object.freeze({
  pc: "퍼펙트 클리어",
  setup: "셋업",
  cover: "커버리지",
  build: "빌드",
  kicks: "킥",
});

function selectSpec(defaultValue, placeholder, options) {
  return Object.freeze({
    defaultValue,
    placeholder,
    options: Object.freeze(options.map((option) => Object.freeze(option))),
  });
}

function normalizeSelectValue(input, name, raw) {
  if (name === "lines") return String(raw);
  if (name === "kicktable") return String(raw).trim().toLowerCase();
  if (input === "spin-structure" && name === "profile") {
    return String(raw).trim().toLowerCase().replaceAll("_", "-");
  }
  if (name === "scope") return String(raw).trim().toLowerCase();
  if (name === "options" && (input === "pc" || input === "cover")) {
    const value = String(raw).trim().toLowerCase();
    if (["hold=use", "hold=true", "hold=yes", "hold=on"].includes(value)) return "hold=use";
    if (["hold=avoid", "hold=false", "hold=no", "hold=off"].includes(value)) return "hold=avoid";
    return value;
  }
  if (name === "options" && (input === "finesse-search" || input === "finesse-score")) {
    const value = String(raw).trim().toLowerCase().replaceAll("_", "-");
    const fields = new Map(value.split(/\s+/).map((token) => token.split("=", 2)));
    const hold = ["avoid", "false", "no", "off"].includes(fields.get("hold"))
      ? "avoid"
      : "use";
    const knowledge = fields.get("knowledge") ?? "both";
    return `hold=${hold} knowledge=${knowledge}`;
  }
  if (name === "options" && input === "spin") {
    const value = String(raw).trim().toUpperCase();
    if (["TYPE=TSPIN", "TYPE=T-SPIN"].includes(value)) return "type=ANY";
    return value.replace(/^TYPE=/, "type=");
  }
  if (name === "options" && input === "score-fixed-next") {
    const value = String(raw).trim().toLowerCase().replaceAll("-", "_");
    if (["initial_b2b=true", "initial_b2b=yes", "initial_b2b=on"].includes(value)) {
      return "initial_b2b=true";
    }
    if (["initial_b2b=false", "initial_b2b=no", "initial_b2b=off"].includes(value)) {
      return "initial_b2b=false";
    }
    return value;
  }
  return String(raw);
}

function isOmittedSelectValue(name, value) {
  return (name === "lines" && value === "auto") ||
    (name === "scope" && value === "all");
}

function modalValue(value, name) {
  if (typeof value !== "string" && typeof value !== "number") {
    throw new Error(`${name} cannot be copied into the Clearra command Modal.`);
  }
  const text = String(value);
  if (text.length > MODAL_TEXT_LIMIT) {
    throw new Error(
      `${name} exceeds Discord's 4,000-character Modal limit; provide every required input directly or use a shorter payload URL.`,
    );
  }
  return text;
}

function modalMaximum(option) {
  if (option.type === 4) return 1;
  return Math.min(option.max_length ?? MODAL_TEXT_LIMIT, MODAL_TEXT_LIMIT);
}

function modalLabelText(input, name, locale = "en") {
  const maximum = maximumBoardRows(input);
  if (normalizeDiscordLocale(locale) === "ko") {
    return ({
      field: `필드 — 10열, 1–${maximum}줄`,
      base: `기존 필드 — 10열, 1–${maximum}줄`,
      target: `목표 차이 — 10열, 1–${maximum}줄`,
      document: "배치 문서 — CTK3 / v115 Fumen",
      next: isFixedQueueInput(input) ? "정확한 넥스트 큐" : "넥스트 큐 / 패턴",
      lines: input === "spin-structure" ? "마지막 스핀 줄 수" : "PC 목표 줄",
      kicktable: "킥테이블",
      options: input === "spin"
        ? "T-spin 목표"
        : input === "score-fixed-next"
          ? "초기 B2B"
          : input.startsWith("finesse-")
            ? "홀드 및 큐 공개 정책"
            : "홀드 정책",
      remaining: "남은 미노",
      priority: "셋업 정렬",
      "max-setup-pieces": "최대 구축 미노",
      "queue-knowledge": "큐 공개 범위",
      pieces: "순서 없는 미노 목록",
      profile: "스핀 판정 프로필",
      scope: "검증 범위",
    })[name] ?? name;
  }
  return ({
    field: `Field — 10 columns, 1–${maximum} rows`,
    base: `Base — 10 columns, 1–${maximum} rows`,
    target: `Target delta — 10 columns, 1–${maximum} rows`,
    document: "Placement document — CTK3 / v115 Fumen",
    next: isFixedQueueInput(input) ? "Exact next queue" : "Next queue / pattern",
    lines: input === "spin-structure" ? "Terminal-spin line count" : "PC target rows",
    kicktable: "Kick table",
    options: input === "spin"
      ? "T-spin target"
      : input === "score-fixed-next"
        ? "Initial B2B"
        : input.startsWith("finesse-")
          ? "Hold and queue knowledge"
          : "Hold policy",
    remaining: "Remaining-piece inventory",
    priority: "Setup ordering",
    "max-setup-pieces": "Maximum setup pieces",
    "queue-knowledge": "Queue knowledge",
    pieces: "Unordered piece inventory",
    profile: "Spin-recognition profile",
    scope: "Verification scope",
  })[name] ?? name;
}

function modalDescription(input, name, locale = "en") {
  if (normalizeDiscordLocale(locale) === "ko") {
    if (["field", "base", "target"].includes(name)) {
      return "위쪽 줄부터 입력. 권장: #은 채움, _는 빈칸. CTK3, v115 Fumen, URL도 지원.";
    }
    if (name === "document") return "모든 페이지에 배치 operation이 하나씩 있어야 하며 색상은 미리보기에 보존됩니다.";
    if (name === "next") return isFixedQueueInput(input)
      ? "정확한 IOTSZJL 큐만 입력하며 영문 대소문자를 구분하지 않습니다."
      : "고정 큐 또는 지원되는 그룹·가방 패턴이며 영문 대소문자를 구분하지 않습니다.";
    if (name === "pieces") return "IOTSZJL 미노 목록이며 반복 문자는 수량을 뜻합니다. 큐나 홀드로 해석하지 않습니다.";
    if (name === "profile") return "모든 프로필에서 Regular와 Mini를 분리하며 +는 immobile T 판정을 추가합니다.";
    if (name === "lines") return input === "score-fixed-next"
      ? `1–${DISCORD_PC_FIELD_MAX_ROWS}줄 퍼펙트 클리어 목표 높이 중 하나를 선택합니다.`
      : input === "spin-structure"
        ? "마지막 스핀이 지우는 줄 수이며 기본값은 최소 1줄입니다."
      : `자동은 필드와 넥스트로 1–${DISCORD_PC_FIELD_MAX_ROWS}줄 전체를 판정하고 성립하는 목표를 탐색합니다.`;
    if (name === "kicktable") return "내장 프로필만 지원하며 사용자 킥 JSON은 지원하지 않습니다.";
    if (name === "options") return input === "spin"
      ? "TSM은 지원하지 않습니다."
      : input === "score-fixed-next"
        ? "초기 B2B 상태를 선택하며 기본값은 사용 안 함입니다."
        : input.startsWith("finesse-")
          ? "홀드 사용 여부와 전체 큐/공개 7개 계산 범위를 선택합니다."
          : "홀드 사용 여부를 선택합니다.";
    if (name === "remaining") return "IOTSZJL 미노 1–7개를 입력하며 한 종류만 두 번 사용할 수 있습니다.";
    if (name === "priority") return "구축 × PC 종합, 구축 확률 우선, PC 확률 우선 중 하나로 정렬합니다.";
    if (name === "max-setup-pieces") return "1–10개이며 기본값은 9개입니다. 10개는 완성된 PC도 포함합니다.";
    if (name === "queue-knowledge") return "전체 미래 큐 또는 실제로 공개되는 7개만 사용합니다.";
    return "범위 하나를 선택하거나 전체를 선택해 모든 검증을 실행합니다.";
  }
  if (["field", "base", "target"].includes(name)) {
    return "Top first. Prefer # for filled and _ for empty. CTK3, Fumen, and URLs also work.";
  }
  if (name === "document") return "Every page needs one placement operation; colors are preserved for the preview.";
  if (name === "next") return isFixedQueueInput(input)
    ? "Exact IOTSZJL queue only; piece letters are case-insensitive."
    : "Fixed queue or supported group/bag pattern; letters are case-insensitive.";
  if (name === "pieces") return "IOTSZJL inventory; repeats are multiplicities, not queue order or hold.";
  if (name === "profile") return "Every profile keeps Regular and Mini separate; + adds the immobile-T fallback.";
  if (name === "lines") return input === "score-fixed-next"
    ? `Choose any perfect-clear target height from 1 through ${DISCORD_PC_FIELD_MAX_ROWS} rows.`
    : input === "spin-structure"
      ? "Lines cleared by the terminal spin; the default is at least one."
    : `Auto evaluates rows 1–${DISCORD_PC_FIELD_MAX_ROWS} from the field and next, then searches valid targets.`;
  if (name === "kicktable") return "Built-in profiles only; custom kick JSON remains intentionally unavailable.";
  if (name === "options") return input === "spin"
    ? "TSM remains intentionally unavailable."
    : input === "score-fixed-next"
      ? "Choose the initial B2B state; disabled is the default."
      : input.startsWith("finesse-")
        ? "Choose hold and full-queue/visible-7 calculation scope."
        : "Use or avoid hold.";
  if (name === "remaining") return "Use 1–7 IOTSZJL pieces; at most one piece kind may occur twice.";
  if (name === "priority") return "Order by joint build × PC, build probability first, or PC probability first.";
  if (name === "max-setup-pieces") return "Choose 1–10; the default is 9, while 10 includes complete PCs.";
  if (name === "queue-knowledge") return "Use the full future queue or only the 7 pieces visible during play.";
  return "Choose one scope, or All to run every verification group.";
}

function modalPlaceholder(input, name) {
  if (["field", "base", "target"].includes(name)) return emptyGrid(defaultBoardRows(input));
  if (name === "next") return isFixedQueueInput(input) ? "IOTSZJL" : "*! or [IOSZ]p2";
  if (name === "document") return "ctk3_… or v115@…";
  if (name === "pieces") return "IOTSZJL";
  if (name === "remaining") return "IOTSZJL";
  return "";
}

function defaultBoardRows(input) {
  return input === "pc" || input === "score-fixed-next" ? 4 : 8;
}

function maximumBoardRows(input) {
  return input === "pc" || input === "score-fixed-next"
    ? DISCORD_PC_FIELD_MAX_ROWS
    : DISCORD_WIDE_FIELD_MAX_ROWS;
}

function isFixedQueueInput(input) {
  return input === "fixed-next" || input === "score-fixed-next";
}

function emptyGrid(rows) {
  return Array.from({ length: rows }, () => "__________").join("\n");
}

function inputSchemaFor(command) {
  return command?.kind === "search"
    ? INPUT_SCHEMAS[command.input] ?? null
    : null;
}

function inputSchemaForVersion(command, version) {
  if ((version === 2 || version === 3) && command?.input === "remaining") {
    return V3_REMAINING_SCHEMA;
  }
  return inputSchemaFor(command);
}

function validateModalResponse(response) {
  const { custom_id: customId, title, components } = response.data;
  if (customId.length < 1 || customId.length > 100) throw new Error("Clearra Modal custom ID is out of bounds.");
  if (title.length < 1 || title.length > 45) throw new Error("Clearra Modal title is out of bounds.");
  if (components.length < 1 || components.length > MODAL_COMPONENT_LIMIT) {
    throw new Error("Clearra command Modal must contain from one through five inputs.");
  }
  const ids = new Set();
  for (const label of components) {
    if (label.label.length < 1 || label.label.length > 45) throw new Error("Clearra Modal label is out of bounds.");
    if (label.description?.length > 100) throw new Error("Clearra Modal description is out of bounds.");
    const component = label.component;
    if (ids.has(component.custom_id)) throw new Error("Clearra Modal component IDs must be unique.");
    ids.add(component.custom_id);
    if (component.placeholder?.length > (component.type === TEXT_INPUT_COMPONENT ? 100 : 150)) {
      throw new Error("Clearra Modal placeholder is out of bounds.");
    }
  }
}

function flattenModalComponents(components) {
  if (!Array.isArray(components)) {
    throw new Error("Discord supplied invalid command Modal components.");
  }
  const output = [];
  for (const outer of components) {
    if (outer?.type === LABEL_COMPONENT && outer.component) {
      output.push(outer.component);
    } else if (
      outer?.type === ACTION_ROW_COMPONENT &&
      Array.isArray(outer.components) &&
      outer.components.length === 1
    ) {
      output.push(outer.components[0]);
    } else {
      throw new Error("Discord supplied an invalid command Modal layout.");
    }
  }
  return output;
}
