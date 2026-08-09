import assert from "node:assert/strict";
import test from "node:test";

import {
  assertDiscordCatalogComplete,
  matchDiscordLocale,
  operationErrorText,
  t,
  validationErrorText,
} from "../src/discord/i18n.mjs";
import {
  formatSlashCommandHelp,
  globalCommands,
} from "../src/discord/slash-command-catalog.mjs";

test("English and Korean Discord catalogs stay complete", () => {
  assert.equal(assertDiscordCatalogComplete(), true);
  assert.equal(t("en", "language.name.en"), "English");
  assert.equal(t("ko-KR", "language.name.ko"), "한국어");
  assert.match(formatSlashCommandHelp("path", "ko"), /퍼펙트 클리어/u);
  assert.equal(matchDiscordLocale("ko-KR"), "ko");
  assert.equal(matchDiscordLocale("en-US"), "en");
  assert.equal(matchDiscordLocale("ja"), null);
});

test("public validation and operation errors hide deployment details", () => {
  const internalValidation = validationErrorText(
    new Error("Clearra Modal schema references an unknown worker allocation."),
    "en",
  );
  assert.equal(
    internalValidation,
    "Clearra could not complete the request: Check the command input and try again.",
  );
  assert.doesNotMatch(internalValidation, /schema|worker|allocation/i);

  const koreanValidation = validationErrorText(
    new Error("Discord supplied an invalid component ID."),
    "ko",
  );
  assert.match(koreanValidation, /입력을 확인/u);
  assert.doesNotMatch(koreanValidation, /Discord|component/i);

  const localizedFieldValidation = validationErrorText(
    new Error("field grid rows must be exactly 10 columns wide."),
    "ko",
  );
  assert.match(localizedFieldValidation, /필드 격자의 각 줄은 정확히 10칸/u);
  assert.doesNotMatch(localizedFieldValidation, /\bfield\b/i);

  const filesystemValidation = validationErrorText(
    new Error("EACCES: permission denied, open '/var/lib/clearra/private.json'"),
    "en",
  );
  assert.equal(
    filesystemValidation,
    "Clearra could not complete the request: Check the command input and try again.",
  );
  assert.doesNotMatch(filesystemValidation, /EACCES|\/var\/lib|permission/i);

  const malformedModal = validationErrorText(
    new Error("Discord Modal select 'kicktable' contains unsupported value 'private'."),
    "en",
  );
  assert.equal(
    malformedModal,
    "Clearra could not complete the request: Check the command input and try again.",
  );
  assert.doesNotMatch(malformedModal, /Modal|kicktable|private/i);

  const operation = operationErrorText(
    new Error("Cloud Run job service at https://private.example failed"),
    "en",
  );
  assert.equal(operation, "The request could not be completed. Please try again.");
  assert.doesNotMatch(operation, /Cloud Run|job service|https:/i);

  assert.doesNotMatch(t("en", "search.busy"), /server|worker|engine/i);
  assert.doesNotMatch(t("ko", "search.busy"), /서버|워커|엔진/u);
  assert.doesNotMatch(
    validationErrorText(new Error("private engine server failed"), "en"),
    /engine|server/i,
  );
  assert.match(
    validationErrorText(
      new Error("queue-knowledge must be full-queue or visible-7."),
      "en",
    ),
    /full-queue.*visible-7/u,
  );
  assert.doesNotMatch(
    validationErrorText(new Error("private oracle mode failed"), "en"),
    /oracle/i,
  );
});

test("Korean public validation errors preserve actionable input details", () => {
  const cases = [
    ["field URL must contain exactly one CTK3 or Fumen document.", "필드 URL에는 CTK3 또는 Fumen 문서가 정확히 하나 있어야 합니다."],
    ["v110 Fumen is not supported by the Clearra search decoder; use v115.", "Clearra 탐색에서는 v110 Fumen을 지원하지 않습니다. v115를 사용해 주세요."],
    ["CTK3 search fields must be exactly 10 columns wide.", "CTK3 탐색 필드는 정확히 10열이어야 합니다."],
    ["field grid must contain from one through six rows.", "필드 격자는 1–6줄이어야 합니다."],
    ["target grid must contain from one through twenty-four rows.", "목표 필드 격자는 1–24줄이어야 합니다."],
    ["next '*' must be followed immediately by ! or pN.", "넥스트의 `*` 뒤에는 공백 없이 `!` 또는 `pN`이 와야 합니다."],
    ["next pattern contains an empty alternative.", "넥스트 패턴에는 빈 대안을 사용할 수 없습니다."],
    ["next '*' must be followed by ! or pN.", "넥스트의 `*` 뒤에는 `!` 또는 `pN`이 와야 합니다."],
    ["next standard-bag draws may not exceed seven pieces per group.", "넥스트 표준 가방의 각 그룹에서는 7개를 초과해 뽑을 수 없습니다."],
    ["next pattern has an unterminated piece group.", "넥스트 패턴의 미노 그룹이 닫히지 않았습니다."],
    ["next pattern contains an invalid piece group.", "넥스트 패턴의 미노 그룹이 올바르지 않습니다."],
    ["next pattern piece group must leave at least one choice.", "넥스트 패턴의 미노 그룹에는 선택 가능한 미노가 하나 이상 남아야 합니다."],
    ["next pattern has an unexpected bag token after a piece group.", "넥스트 패턴의 미노 그룹 뒤에 예상하지 못한 가방 토큰이 있습니다."],
    ["next pattern draws more pieces than its group contains.", "넥스트 패턴은 그룹에 포함된 미노 수보다 많이 뽑을 수 없습니다."],
    ["next pattern draw count is missing.", "넥스트 패턴의 뽑기 개수를 입력해 주세요."],
    ["next pattern draw count must be a positive integer.", "넥스트 패턴의 뽑기 개수는 양의 정수여야 합니다."],
    ["lines and legacy options clear/lines may not be specified together.", "줄 입력과 기존 options의 clear/lines 설정을 동시에 사용할 수 없습니다."],
    ["options clear must be an integer from 1 through 6.", "기존 options의 clear 값은 1부터 6까지의 정수여야 합니다."],
    ["options type must be TSS, TSD, TST, TSPIN, T-SPIN, or ANY; TSM is unavailable.", "options의 type은 TSS, TSD, TST, TSPIN, T-SPIN, ANY 중 하나여야 하며 TSM은 지원하지 않습니다."],
    ["--arguments requires exactly one command name.", "/help의 --arguments에는 명령어 이름을 정확히 하나 입력해야 합니다."],
    ["Text command /help accepts at most one command name.", "텍스트 /help에는 명령어 이름을 하나만 입력할 수 있습니다."],
    ["help arguments cannot be empty.", "/help의 명령어 인수는 비워 둘 수 없습니다."],
    ["help arguments exceeds the 64-character limit.", "/help의 명령어 인수는 64자를 넘을 수 없습니다."],
    ["The command contains an unterminated code block.", "명령어의 코드 블록이 닫히지 않았습니다."],
    ["A command code block cannot be empty.", "명령어 코드 블록은 비워 둘 수 없습니다."],
    ["priority must be all, build, or pc.", "셋업 정렬은 all, build, pc 중 하나여야 합니다."],
    ["queue-knowledge must be full-queue or visible-7.", "큐 공개 범위는 full-queue 또는 visible-7이어야 합니다."],
    ["setup-length must be auto, longer, or shorter.", "셋업 길이는 auto, longer, shorter 중 하나여야 합니다."],
    ["remaining must contain from 1 through 7 pieces.", "남은 미노에는 미노를 1–7개 입력해야 합니다."],
    ["next-cycle-remaining must contain exactly 1 piece when remaining contains 4.", "남은 미노가 4개이면 다음 회차 남은 미노는 정확히 1개여야 합니다."],
    ["When next-cycle-remaining or setup-length is set, remaining must also be supplied directly.", "다음 회차 남은 미노 또는 셋업 길이를 먼저 설정했다면 남은 미노도 슬래시 명령어에 직접 입력해 주세요."],
  ];
  const generic = validationErrorText(new Error("unmapped public validation"), "ko");

  for (const [source, expected] of cases) {
    const localized = validationErrorText(new Error(source), "ko");
    assert.notEqual(localized, generic, source);
    assert.match(localized, new RegExp(escapeRegExp(expected), "u"), source);
  }

  for (const source of [
    "options clear must be an integer from 1 through 6.",
    "help arguments cannot be empty.",
    "help arguments exceeds the 64-character limit.",
    "A command code block cannot be empty.",
  ]) {
    assert.doesNotMatch(
      validationErrorText(new Error(source), "ko"),
      /options clear|help arguments|A command code block/i,
      source,
    );
  }
});

test("Discord registration localizes names without redundant values or collisions", () => {
  const effectiveKoreanNames = new Set();
  for (const command of globalCommands) {
    assertLocalizedTree(command);
    const effective = command.name_localizations?.ko ?? command.name;
    assert.equal(effectiveKoreanNames.has(effective), false, effective);
    effectiveKoreanNames.add(effective);
  }
  assert.equal(
    globalCommands.find(({ name }) => name === "help")?.name_localizations?.ko,
    "도움말",
  );
  assert.equal(
    globalCommands.find(({ type }) => type === 3)?.name_localizations?.ko,
    "원본 GIF 받기",
  );

  const helpArgument = globalCommands
    .find(({ name }) => name === "help")
    ?.options[0];
  assert.equal(helpArgument?.type, 3);
  assert.equal(helpArgument?.required, false);
  assert.equal(helpArgument?.choices, undefined);
  assert.equal(helpArgument?.autocomplete, undefined);

  const renderFile = globalCommands.find(({ name }) => name === "render-file");
  assert.equal(renderFile?.options[0].name_localizations?.ko, "이미지");

  const spinOptions = globalCommands
    .find(({ name }) => name === "spin")
    ?.options.find(({ name }) => name === "options");
  assert.deepEqual(
    spinOptions?.choices.map(effectiveKoreanChoiceName),
    ["T-spin 싱글", "T-spin 더블", "T-spin 트리플", "모든 T-spin"],
  );

  const scoreFinder = globalCommands.find(({ name }) => name === "score-finder");
  assert.equal(scoreFinder?.name_localizations?.ko, undefined);
  assert.match(scoreFinder?.description_localizations?.ko ?? "", /Jstris.*퍼펙트 클리어/u);
  assert.deepEqual(
    scoreFinder?.options.find(({ name }) => name === "options")
      ?.choices.map(effectiveKoreanChoiceName),
    ["초기 B2B 사용 안 함 (기본값)", "초기 B2B 사용"],
  );
  const scoreFinderLines = scoreFinder?.options.find(({ name }) => name === "lines");
  assert.match(scoreFinderLines?.description ?? "", /1–6/);
  assert.match(scoreFinderLines?.description_localizations?.ko ?? "", /1–6줄/u);
  assert.doesNotMatch(scoreFinderLines?.description ?? "", /default/i);
  assert.doesNotMatch(scoreFinderLines?.description_localizations?.ko ?? "", /4줄/u);

  const pathField = globalCommands
    .find(({ name }) => name === "path")
    ?.options.find(({ name }) => name === "field");
  assert.match(pathField?.description ?? "", /1–6 rows/);
  assert.match(pathField?.description_localizations?.ko ?? "", /1–6줄/u);

  const setupField = globalCommands
    .find(({ name }) => name === "setup")
    ?.options.find(({ name }) => name === "field");
  assert.match(setupField?.description ?? "", /1–24 rows/);
  assert.match(setupField?.description_localizations?.ko ?? "", /1–24줄/u);

  const cover = globalCommands.find(({ name }) => name === "cover");
  for (const optionName of ["base", "target"]) {
    const option = cover?.options.find(({ name }) => name === optionName);
    assert.match(option?.description ?? "", /1–24 rows/);
    assert.match(option?.description_localizations?.ko ?? "", /1–24줄/u);
  }
  const damage = globalCommands.find(({ name }) => name === "damage");
  assert.equal(damage?.name_localizations?.ko, "대미지");
  assert.match(damage?.description_localizations?.ko ?? "", /최대 대미지/u);

  const verifyScope = globalCommands
    .find(({ name }) => name === "verify")
    ?.options.find(({ name }) => name === "scope");
  assert.equal(
    verifyScope?.description_localizations?.ko,
    "실행할 검증 그룹이며 생략하면 모든 검증을 실행합니다",
  );
  assert.deepEqual(
    verifyScope?.choices.map(effectiveKoreanChoiceName),
    ["퍼펙트 클리어", "셋업", "커버", "빌드", "킥"],
  );

  const setupRanking = globalCommands.find(({ name }) => name === "pc-setup");
  assert.deepEqual(setupRanking?.options.map(({ name }) => name), [
    "remaining",
    "priority",
    "max-setup-pieces",
    "queue-knowledge",
    "next-cycle-remaining",
    "setup-length",
    "kicktable",
  ]);
  assert.deepEqual(
    setupRanking?.options.find(({ name }) => name === "priority")
      ?.choices.map(effectiveKoreanChoiceName),
    ["구축 × PC 종합", "구축 확률 우선", "PC 확률 우선"],
  );
  assert.deepEqual(
    setupRanking?.options.find(({ name }) => name === "queue-knowledge")
      ?.choices.map(effectiveKoreanChoiceName),
    ["전체 미래 큐", "공개 7개"],
  );
  assert.deepEqual(
    setupRanking?.options.find(({ name }) => name === "setup-length")
      ?.choices.map(effectiveKoreanChoiceName),
    ["자동", "긴 셋업 우선", "짧은 셋업 우선"],
  );
  assert.match(formatSlashCommandHelp("best-setup", "ko"), /기본값은 `build`/u);
});

function effectiveKoreanChoiceName(choice) {
  return choice.name_localizations?.ko ?? choice.name;
}

function assertLocalizedTree(value, kind = "command", path = value.name) {
  const invariantName = isInvariantKoreanName(value, kind, path);
  if (invariantName) {
    assert.equal(value.name_localizations?.ko, undefined, `${path} has a redundant Korean name localization`);
  } else {
    assert.equal(
      typeof value.name_localizations?.ko,
      "string",
      `${path} is missing its Korean name localization`,
    );
    assert.notEqual(value.name_localizations.ko, value.name, `${path} repeats its English name`);
  }

  if (typeof value.description === "string") {
    assert.equal(
      typeof value.description_localizations?.ko,
      "string",
      `${path} is missing its Korean description localization`,
    );
    assert.notEqual(
      value.description_localizations.ko,
      value.description,
      `${path} repeats its English description`,
    );
  }

  for (const choice of value.choices ?? []) {
    assertLocalizedTree(choice, "choice", `${path}[${choice.value}]`);
  }
  for (const option of value.options ?? []) {
    assertLocalizedTree(option, "option", `${path}.${option.name}`);
  }
}

function isInvariantKoreanName(value, kind, path) {
  if (kind === "command" && value.name === "score-finder") return true;
  if (kind !== "choice") return false;
  return /\.kicktable\[[^\]]+\]$/u.test(path)
    && new Set(["SRS", "SRS-X", "Jstris 180"]).has(value.name);
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
