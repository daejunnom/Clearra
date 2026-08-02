const STRING_OPTION = 3;
const FIELD_MAX_LENGTH = 6000;
const NEXT_MAX_LENGTH = 2048;
const OPTIONAL_SETTINGS_MAX_LENGTH = 256;

const PC_COMMANDS = Object.freeze([
  command("path", "Find exact PC paths", "pc", "pc"),
  command("percent", "Measure PC success probability", "pc", "pc", {
    note: "This currently uses the same Clearra calculation as /chance.",
  }),
  command("chance", "Measure PC success probability", "pc", "pc", {
    note: "This currently uses the same Clearra calculation as /percent.",
  }),
  command("minimals", "Find a minimum-cover PC solution set", "pc", "pc"),
  command("score", "Score PC solutions with the supported Jstris profile", "pc", "pc"),
  command(
    "score-minimals",
    "Score a minimum-cover PC solution set",
    "pc",
    "pc",
  ),
  command("saves", "Analyze probabilities for each PC solution", "pc", "pc", {
    note: "This currently uses the same Clearra calculation as /best-save.",
  }),
  command("best-save", "Analyze probabilities for each PC solution", "pc", "pc", {
    note: "This currently uses the same Clearra calculation as /saves.",
  }),
]);

const COLORED_TARGET_COMMANDS = Object.freeze([
  command("setup", "Measure a target shape's build probability", "target", "colored"),
  command("congruent", "Measure a target shape's build probability", "target", "colored", {
    note: "This currently matches /setup and does not expose sfinder-man's garbage option.",
  }),
  command(
    "congruent-cover",
    "Measure a target shape's build probability",
    "target",
    "colored",
    { note: "This currently matches /setup and does not expose sfinder-man's mode/mirror/garbage options." },
  ),
  command(
    "setup-cover",
    "Measure a target shape's build probability",
    "target",
    "colored",
    { note: "This is Clearra's represented colored-target contract, not sfinder-man's two-queue form." },
  ),
  command(
    "cover-percent",
    "Measure a target shape's build probability",
    "target",
    "colored",
    { note: "This is Clearra's represented colored-target contract, not sfinder-man's multi-queue form." },
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
  ...COLORED_TARGET_COMMANDS,
  command("spin-cover", "Find forward T-spin completions", "spin", "spin", {
    note: "T-spin mini (TSM) is intentionally unavailable.",
  }),
  command("spin", "Find forward T-spin completions", "spin", "spin", {
    note: "This currently uses the same Clearra calculation as /spin-cover; TSM is unavailable.",
  }),
  command("cat-finder", "Find damage for one exact next queue", "damage", "fixed-next"),
  command("pc-setup", "Rank setup candidates by joint coverage", "setup", "remaining", {
    note: "This is Clearra's remaining-piece setup contract, not sfinder-man's -sp/-p form.",
  }),
  command("best-setup", "Rank setup candidates by build coverage", "setup", "remaining", {
    note: "This is Clearra's remaining-piece setup contract, not sfinder-man's PC-number form.",
  }),
  command("dpc-finder", "Rank setup candidates by PC coverage", "setup", "remaining", {
    note: "This is Clearra's remaining-piece priority=pc contract, not sfinder-man's exact-queue form.",
  }),
  command("verify", "Run Clearra verification checks", "verify", "verify"),
]);

const HELP_CHOICES = Object.freeze(
  SEARCH_COMMAND_DEFINITIONS.map(({ name }) => Object.freeze({ name, value: name })),
);

const HELP_COMMAND = Object.freeze({
  name: "help",
  kind: "help",
  registration: Object.freeze({
    name: "help",
    description: "Show Clearra slash-command syntax",
    options: Object.freeze([
      Object.freeze({
        type: STRING_OPTION,
        name: "arguments",
        description: "Command name to explain; omit it to list all commands",
        required: false,
        choices: HELP_CHOICES,
      }),
    ]),
  }),
});

const SEARCH_COMMANDS = Object.freeze(
  SEARCH_COMMAND_DEFINITIONS.map((definition) => {
    const argvPrefix = Object.freeze(["sfinder", definition.name]);
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

export const representedSfinderCommandNames = Object.freeze(
  SEARCH_COMMANDS.map(({ name }) => name),
);

export const slashCommandCatalog = Object.freeze([HELP_COMMAND, ...SEARCH_COMMANDS]);

const COMMANDS_BY_NAME = new Map(
  slashCommandCatalog.map((entry) => [entry.name, entry]),
);

export const globalCommands = Object.freeze(
  slashCommandCatalog.map(({ registration }) => registration),
);

export function findSlashCommand(name) {
  return typeof name === "string" ? COMMANDS_BY_NAME.get(name) ?? null : null;
}

export function formatSlashCommandHelp(requestedName) {
  const normalized = normalizeHelpTarget(requestedName);
  if (!normalized) return commandListHelp();
  const entry = COMMANDS_BY_NAME.get(normalized);
  if (!entry || entry.kind !== "search") {
    return `Unknown Clearra command \`${requestedName}\`. Use \`/help\` to list commands.`;
  }

  const lines = [
    `**/${entry.name}** — ${entry.description}`,
    `Syntax: \`${syntax(entry)}\``,
    ...inputHelp(entry.input),
  ];
  if (entry.note) lines.push(`Note: ${entry.note}`);
  lines.push("Use `/help` without arguments to list every command group.");
  return lines.join("\n");
}

function command(name, description, group, input, extras = {}) {
  return Object.freeze({ name, description, group, input, ...extras });
}

function registrationOptions(input) {
  switch (input) {
    case "pc":
      return Object.freeze([
        fieldOption(false),
        nextOption(false),
        settingsOption("Optional settings: clear=1..6 hold=use|avoid"),
      ]);
    case "cover":
      return Object.freeze([
        boardOption(
          "base",
          "Colorless starting field as single-page CTK3/v115 Fumen or a payload URL",
        ),
        boardOption(
          "target",
          "Colorless cells to add as single-page CTK3/v115 Fumen or a payload URL",
        ),
        nextOption(false),
        settingsOption("Optional setting: hold=use|avoid"),
      ]);
    case "colored":
      return Object.freeze([fieldOption(false, true), nextOption(false)]);
    case "spin":
      return Object.freeze([
        fieldOption(false),
        nextOption(false),
        settingsOption("Optional setting: type=TSS|TSD|TST|TSPIN|T-SPIN|ANY"),
      ]);
    case "fixed-next":
      return Object.freeze([fieldOption(false), nextOption(true)]);
    case "remaining":
      return Object.freeze([
        stringOption(
          "remaining",
          "Unordered remaining-piece inventory using IOTSZJL",
          true,
          64,
        ),
      ]);
    case "verify":
      return Object.freeze([
        Object.freeze({
          type: STRING_OPTION,
          name: "scope",
          description: "Optional verification scope; omit it to run all checks",
          required: false,
          choices: Object.freeze(
            ["pc", "setup", "cover", "build", "kicks"].map((value) =>
              Object.freeze({ name: value, value }),
            ),
          ),
        }),
      ]);
    default:
      throw new Error(`Unknown slash-command input contract: ${input}`);
  }
}

function fieldOption(_multiplePages, colored = false) {
  const description = colored
    ? "Colorless target shape as single-page CTK3/v115 Fumen or a payload URL"
    : "Colorless field as single-page CTK3/v115 Fumen or a payload URL";
  return boardOption("field", description);
}

function boardOption(name, description) {
  return stringOption(name, description, true, FIELD_MAX_LENGTH);
}

function nextOption(fixed) {
  return stringOption(
    "next",
    fixed
      ? "Exact next queue using only IOTSZJL pieces; pattern grammar is not used"
      : "Sfinder next-pattern expression such as *! or [IOSZ]p2",
    true,
    NEXT_MAX_LENGTH,
  );
}

function settingsOption(description) {
  return stringOption(
    "options",
    description,
    false,
    OPTIONAL_SETTINGS_MAX_LENGTH,
  );
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

function commandListHelp() {
  return [
    "**Clearra slash commands**",
    "PC: `/path`, `/percent`, `/chance`, `/minimals`, `/score`, `/score-minimals`, `/saves`, `/best-save`",
    "Solutions: `/cover`",
    "Target shapes: `/setup`, `/congruent`, `/congruent-cover`, `/setup-cover`, `/cover-percent`, `/special-cover`",
    "Forward search: `/spin-cover`, `/spin`, `/cat-finder`",
    "Setup ranking: `/pc-setup`, `/best-setup`, `/dpc-finder`",
    "Checks: `/verify`",
    "Use `/help arguments:<command>` for exact syntax. Board inputs accept 10-column CTK3 or v115 Fumen text, or a URL containing one value; input colors are treated only as occupied cells.",
  ].join("\n");
}

function syntax(entry) {
  switch (entry.input) {
    case "pc":
      return `/${entry.name} field:<CTK3|Fumen> next:<pattern> [options:\"clear=4 hold=use\"]`;
    case "cover":
      return `/${entry.name} base:<CTK3|Fumen> target:<CTK3|Fumen> next:<pattern> [options:\"hold=use\"]`;
    case "colored":
      return `/${entry.name} field:<CTK3|Fumen> next:<pattern>`;
    case "spin":
      return `/${entry.name} field:<CTK3|Fumen> next:<pattern> [options:\"type=TSS\"]`;
    case "fixed-next":
      return `/${entry.name} field:<CTK3|Fumen> next:<fixed queue>`;
    case "remaining":
      return `/${entry.name} remaining:<unordered IOTSZJL inventory>`;
    case "verify":
      return `/${entry.name} [scope:<pc|setup|cover|build|kicks>]`;
    default:
      throw new Error(`Unknown slash-command input contract: ${entry.input}`);
  }
}

function inputHelp(input) {
  switch (input) {
    case "pc":
      return [
        "`field` is one operation-free page. `next` supports fixed queues, `*pN`/`*!`, piece groups/complements, and `;` alternatives.",
        "`options` allows only `clear=1..6` (default 4) and `hold=use|avoid` (default use).",
        "The represented compatibility contract uses the Jstris-180 rotation rule.",
      ];
    case "cover":
      return [
        "`base` is the existing field. `target` contains only the new cells to build, not the final `base ∪ target` board; the two masks must not overlap.",
        "Both board inputs require one operation-free 10-column CTK3/v115 Fumen page. CTK3 and Fumen may be mixed, and every input color (including Fumen grey) means the same occupied cell.",
        "`next` supports the represented fixed/group/bag pattern subset.",
        "`options` allows only `hold=use|avoid` (default use).",
        "The represented compatibility contract uses the Jstris-180 rotation rule.",
        "Colored solution documents are emitted as CTK3 by default; input documents are never converted to Fumen.",
      ];
    case "colored":
      return [
        "`field` is one operation-free colorless target shape. Every CTK3/Fumen color is treated as the same occupied cell; `next` supports the represented fixed/group/bag pattern subset.",
        "The represented compatibility contract uses Jstris-180 and disables horizontal mirror.",
      ];
    case "spin":
      return [
        "`field` is one operation-free page. `next` supports the represented fixed/group/bag pattern subset.",
        "`options` allows only `type=TSS|TSD|TST|TSPIN|T-SPIN|ANY` (default TSS); TSM is unavailable.",
        "The represented compatibility contract uses the Jstris-180 rotation rule.",
      ];
    case "fixed-next":
      return [
        "`field` is one operation-free page. `next` must be an exact IOTSZJL queue, not a pattern.",
        "The represented compatibility contract uses the Jstris-180 rotation rule.",
      ];
    case "remaining":
      return [
        "`remaining` is an unordered inventory containing only IOTSZJL pieces.",
        "The represented compatibility contract uses the Jstris-180 rotation rule.",
      ];
    case "verify":
      return ["Omit `scope` to run all checks. This command does not use a search worker pool."];
    default:
      throw new Error(`Unknown slash-command input contract: ${input}`);
  }
}
