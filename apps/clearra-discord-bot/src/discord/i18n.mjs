export const DEFAULT_DISCORD_LOCALE = "en";
export const SUPPORTED_DISCORD_LOCALES = Object.freeze(["en", "ko"]);

const CATALOGS = Object.freeze({
  en: Object.freeze({
    "preview.searching": "Preview ready. Clearra search is running…",
    "preview.result": "ClearraBot rendered the Clearra result.",
    "preview.document": "Document preview ready.",
    "preview.invalid_attachment": "Clearra could not render the CTK3 attachment because it is invalid or exceeds the preview limits.",
    "preview.invalid_input": "Clearra could not render the Fumen or CTK3 input because it is invalid or exceeds the preview limits.",
    "preview.image_failed": "The GIF preview could not be rendered.",
    "preview.image_limit": "The image preview exceeds the supported size or frame limit.",
    "preview.attachment_description": "Fumen and CTK3 preview",
    "render_file.not_found": "No recent Clearra GIF preview was found in this channel.",
    "render_file.invalid_selection": "The selected message does not contain an available Clearra GIF preview.",
    "render_file.unavailable": "The original GIF is no longer available. Try a newer preview.",
    "render_file.failed": "The original GIF could not be retrieved. Please try again.",
    "render_file.attachment_description": "Original Clearra GIF preview",
    "viewer.open": "Open in Clearra: {url}",
    "viewer.link_too_long": "The direct viewer link exceeds Discord's 2,000-character limit. Open the Clearra CTK renderer and load the attached CTK3 document: {url}",
    "viewer.document_description": "CTK3 document for the Clearra renderer",
    "viewer.document_failed": "The document could not be rendered.",
    "search.stopped": "Clearra search stopped before producing a result.",
    "search.busy": "Clearra is busy. Please try again shortly.",
    "search.cancelled": "The search was cancelled.",
    "search.timeout": "The calculation exceeded its time limit. Please try again.",
    "search.failed": "The request could not be completed. Please try again.",
    "search.no_text": "Clearra completed without text output.",
    "search.auto_target": "Automatic PC target: {lines}",
    "result.completed": "Clearra {kind} completed{partial}.",
    "result.partial_suffix": " with a partial result",
    "result.ctk3_pages": "CTK3 pages: {count}",
    "result.warning": "Warning: {warning}",
    "result.additional_warnings": "Warning: {count} additional incomplete-result conditions.",
    "result.generic_kind": "search",
    "result.kind.pc": "perfect-clear search",
    "result.kind.path": "path search",
    "result.kind.setup": "setup search",
    "result.kind.build": "build-probability search",
    "result.kind.score": "Jstris-score perfect-clear search",
    "result.kind.cover": "coverage search",
    "result.kind.damage": "damage search",
    "result.kind.spin": "spin search",
    "result.kind.spin_structure": "spin-structure search",
    "result.kind.verify": "verification",
    "result.kind.finesse_search": "finesse search",
    "result.kind.finesse_score": "finesse score",
    "finesse.exact_total_inputs": "Minimum total",
    "finesse.average.oracle": "Full-queue average",
    "finesse.average.visible_7": "Visible-7 average",
    "finesse.oracle_on_covered_average": "Full-queue average on visible-7 successes",
    "finesse.information_penalty": "Information cost",
    "finesse.success_probability_gap": "Success probability gap",
    "finesse.success_probability": "Success probability",
    "finesse.successful_queues": "Successful queues",
    "finesse.inputs": "{count} inputs",
    "finesse.input": "{count} input",
    "summary.coverage_probability": "Coverage probability",
    "summary.probability": "Probability",
    "summary.weighted_probability": "Weighted probability",
    "summary.total_solution_count": "Solutions",
    "summary.unique_solution_count": "Unique solutions",
    "summary.normalized_unique_solution_count": "Normalized solutions",
    "summary.result_count": "Results",
    "summary.regular_count": "Regular structures",
    "summary.mini_count": "Mini structures",
    "summary.minimum_placements": "Minimum placements",
    "summary.maximum_damage": "Maximum damage",
    "warning.incomplete": "Some results may be incomplete.",
    "warning.truncated": "Some results were omitted because a result limit was reached.",
    "warning.tiling_only": "WARNING: Buildability and probability are skipped. Results may include solutions that cannot be built.",
    "result.title": "Clearra result:",
    "result.output_description": "Clearra command output",
    "result.ctk3_description": "Complete color-preserving Clearra CTK3 result",
    "error.request": "Clearra could not complete the request: {message}",
    "error.validation": "Check the command input and try again.",
    "error.form": "Clearra could not open the input form. Please try again.",
    "error.unsupported_command": "Only supported ClearraBot commands are enabled.",
    "security.rate_limited": "Too many requests were sent from this account. Please wait briefly and try again.",
    "security.repeated_request": "The same request was submitted too quickly. Please wait briefly before trying it again.",
    "access.guild_paused": "Clearra commands are temporarily unavailable in this server.",
    "access.channel_disabled": "Clearra commands are unavailable in this channel.",
    "management.guild_only": "This setting can be changed only inside a server.",
    "management.permission.channel": "Manage Channels permission or ClearraBot administrator access is required.",
    "management.permission.guild": "Manage Server permission or ClearraBot administrator access is required.",
    "management.channel.disabled": "Clearra commands are now disabled in this channel.",
    "management.channel.enabled": "Clearra commands are now enabled in this channel.",
    "management.guild.paused": "Clearra commands are now paused in this server. Only server resume remains available.",
    "management.guild.resumed": "Clearra commands are now available in this server.",
    "language.current": "Effective {scope} language: {language} ({source}).",
    "language.updated": "The {scope} default language is now {language}.",
    "language.reset": "The {scope} language override was removed. The effective language is {language}.",
    "language.scope.channel": "channel",
    "language.scope.guild": "server",
    "language.source.channel": "channel setting",
    "language.source.guild": "server setting",
    "language.source.global": "global default",
    "language.name.en": "English",
    "language.name.ko": "Korean",
    "language.guild_only": "Language settings can be changed only inside a server.",
    "language.permission.channel": "Manage Channels permission or ClearraBot administrator access is required to change the channel language.",
    "language.permission.guild": "Manage Server permission or ClearraBot administrator access is required to change the server language.",
  }),
  ko: Object.freeze({
    "preview.searching": "미리보기가 준비되었습니다. Clearra 탐색을 진행하고 있습니다…",
    "preview.result": "Clearra 결과 미리보기가 준비되었습니다.",
    "preview.document": "문서 미리보기가 준비되었습니다.",
    "preview.invalid_attachment": "CTK3 첨부 파일이 올바르지 않거나 미리보기 제한을 넘어 렌더링할 수 없습니다.",
    "preview.invalid_input": "Fumen 또는 CTK3 입력이 올바르지 않거나 미리보기 제한을 넘어 렌더링할 수 없습니다.",
    "preview.image_failed": "이미지 미리보기를 렌더링할 수 없습니다.",
    "preview.image_limit": "이미지 미리보기가 지원되는 크기 또는 프레임 제한을 넘습니다.",
    "preview.attachment_description": "Fumen 및 CTK3 미리보기",
    "render_file.not_found": "이 채널에서 최근 Clearra GIF 미리보기를 찾지 못했습니다.",
    "render_file.invalid_selection": "선택한 메시지에 사용할 수 있는 Clearra GIF 미리보기가 없습니다.",
    "render_file.unavailable": "원본 GIF를 더 이상 사용할 수 없습니다. 더 최근 미리보기를 선택해 주세요.",
    "render_file.failed": "원본 GIF를 가져오지 못했습니다. 다시 시도해 주세요.",
    "render_file.attachment_description": "Clearra 원본 GIF 미리보기",
    "viewer.open": "Clearra에서 열기: {url}",
    "viewer.link_too_long": "바로 열기 링크가 Discord의 2,000자 제한을 넘습니다. Clearra CTK 렌더러를 열고 첨부된 CTK3 문서를 불러오세요: {url}",
    "viewer.document_description": "Clearra 렌더러용 CTK3 문서",
    "viewer.document_failed": "문서를 렌더링할 수 없습니다.",
    "search.stopped": "결과를 만들기 전에 Clearra 탐색이 중단되었습니다.",
    "search.busy": "요청이 많습니다. 잠시 후 다시 시도해 주세요.",
    "search.cancelled": "탐색이 취소되었습니다.",
    "search.timeout": "제한 시간 안에 연산을 마치지 못했습니다. 다시 시도해 주세요.",
    "search.failed": "요청을 완료할 수 없습니다. 다시 시도해 주세요.",
    "search.no_text": "Clearra가 텍스트 출력 없이 작업을 완료했습니다.",
    "search.auto_target": "자동 PC 목표: {lines}",
    "result.completed": "Clearra {kind}을(를) 완료했습니다{partial}.",
    "result.partial_suffix": "(일부 결과)",
    "result.ctk3_pages": "CTK3 페이지: {count}",
    "result.warning": "주의: {warning}",
    "result.additional_warnings": "주의: 불완전한 결과 조건이 {count}개 더 있습니다.",
    "result.generic_kind": "탐색",
    "result.kind.pc": "퍼펙트 클리어 탐색",
    "result.kind.path": "경로 탐색",
    "result.kind.setup": "셋업 탐색",
    "result.kind.build": "구축 확률 탐색",
    "result.kind.score": "Jstris 점수 퍼펙트 클리어 탐색",
    "result.kind.cover": "커버리지 탐색",
    "result.kind.damage": "대미지 탐색",
    "result.kind.spin": "스핀 탐색",
    "result.kind.spin_structure": "스핀 구조 탐색",
    "result.kind.verify": "검증",
    "result.kind.finesse_search": "피네스 탐색",
    "result.kind.finesse_score": "피네스 계산",
    "finesse.exact_total_inputs": "최소 총 입력",
    "finesse.average.oracle": "전체 큐 평균",
    "finesse.average.visible_7": "공개 7개 평균",
    "finesse.oracle_on_covered_average": "공개 7개 성공 큐의 전체 큐 평균",
    "finesse.information_penalty": "정보 비용",
    "finesse.success_probability_gap": "성공 확률 차이",
    "finesse.success_probability": "성공 확률",
    "finesse.successful_queues": "성공 큐",
    "finesse.inputs": "{count}입력",
    "finesse.input": "{count}입력",
    "summary.coverage_probability": "커버 확률",
    "summary.probability": "확률",
    "summary.weighted_probability": "가중 확률",
    "summary.total_solution_count": "해법",
    "summary.unique_solution_count": "고유 해법",
    "summary.normalized_unique_solution_count": "정규화 해법",
    "summary.result_count": "결과",
    "summary.regular_count": "Regular 구조",
    "summary.mini_count": "Mini 구조",
    "summary.minimum_placements": "최소 배치 수",
    "summary.maximum_damage": "최대 대미지",
    "warning.incomplete": "일부 결과가 불완전할 수 있습니다.",
    "warning.truncated": "결과 제한에 도달해 일부 결과가 생략되었습니다.",
    "warning.tiling_only": "주의: 구축 가능성과 확률 계산을 생략했습니다. 구축할 수 없는 해법이 포함될 수 있습니다.",
    "result.title": "Clearra 결과:",
    "result.output_description": "Clearra 명령어 출력",
    "result.ctk3_description": "색상을 보존한 전체 Clearra CTK3 결과",
    "error.request": "Clearra가 요청을 완료하지 못했습니다: {message}",
    "error.validation": "명령어 입력을 확인한 뒤 다시 시도해 주세요.",
    "error.form": "입력 창을 열 수 없습니다. 다시 시도해 주세요.",
    "error.unsupported_command": "지원되는 ClearraBot 명령어만 사용할 수 있습니다.",
    "security.rate_limited": "이 계정에서 요청을 너무 많이 보냈습니다. 잠시 후 다시 시도해 주세요.",
    "security.repeated_request": "같은 요청이 너무 빠르게 반복되었습니다. 잠시 후 다시 시도해 주세요.",
    "access.guild_paused": "이 서버에서는 Clearra 명령을 일시적으로 사용할 수 없습니다.",
    "access.channel_disabled": "이 채널에서는 Clearra 명령을 사용할 수 없습니다.",
    "management.guild_only": "이 설정은 서버 안에서만 변경할 수 있습니다.",
    "management.permission.channel": "채널 관리 권한 또는 ClearraBot 관리자 권한이 필요합니다.",
    "management.permission.guild": "서버 관리 권한 또는 ClearraBot 관리자 권한이 필요합니다.",
    "management.channel.disabled": "이 채널에서 Clearra 명령을 비활성화했습니다.",
    "management.channel.enabled": "이 채널에서 Clearra 명령을 다시 활성화했습니다.",
    "management.guild.paused": "이 서버의 Clearra 명령을 일시정지했습니다. 서버 재개만 사용할 수 있습니다.",
    "management.guild.resumed": "이 서버에서 Clearra 명령을 다시 사용할 수 있습니다.",
    "language.current": "현재 {scope} 적용 언어: {language} ({source}).",
    "language.updated": "{scope} 기본 언어를 {language}(으)로 변경했습니다.",
    "language.reset": "{scope} 언어 설정을 삭제했습니다. 현재 적용 언어는 {language}입니다.",
    "language.scope.channel": "채널",
    "language.scope.guild": "서버",
    "language.source.channel": "채널 설정",
    "language.source.guild": "서버 설정",
    "language.source.global": "전체 기본값",
    "language.name.en": "영어",
    "language.name.ko": "한국어",
    "language.guild_only": "언어 설정은 서버 안에서만 변경할 수 있습니다.",
    "language.permission.channel": "채널 언어를 변경하려면 채널 관리 권한 또는 ClearraBot 관리자 권한이 필요합니다.",
    "language.permission.guild": "서버 언어를 변경하려면 서버 관리 권한 또는 ClearraBot 관리자 권한이 필요합니다.",
  }),
});

export function normalizeDiscordLocale(value, fallback = DEFAULT_DISCORD_LOCALE) {
  const normalizedFallback = matchDiscordLocale(fallback) ?? DEFAULT_DISCORD_LOCALE;
  return matchDiscordLocale(value) ?? normalizedFallback;
}

export function matchDiscordLocale(value) {
  if (typeof value !== "string") return null;
  const normalized = value.trim().toLowerCase().replaceAll("_", "-");
  if (normalized === "ko" || normalized.startsWith("ko-")) return "ko";
  if (normalized === "en" || normalized.startsWith("en-")) return "en";
  return null;
}

export function isSupportedDiscordLocale(value) {
  return supportedLocale(value) !== null;
}

export function t(locale, key, values = {}) {
  const language = normalizeDiscordLocale(locale);
  const template = CATALOGS[language][key] ?? CATALOGS.en[key];
  if (typeof template !== "string") throw new Error(`Unknown Discord translation key '${key}'.`);
  return template.replace(/\{([a-z0-9_]+)\}/gi, (_match, name) =>
    Object.hasOwn(values, name) ? String(values[name]) : `{${name}}`
  );
}

export function validationErrorText(error, locale) {
  const message = error instanceof Error ? error.message : String(error ?? "");
  if (normalizeDiscordLocale(locale) === "en") {
    return t("en", "error.request", { message: safePublicValidationMessage(message, "en") });
  }
  return t("ko", "error.request", {
    message: koreanValidationMessage(message),
  });
}

export function operationErrorText(error, locale) {
  const message = error instanceof Error ? error.message : String(error ?? "");
  let key = "search.failed";
  if (error?.name === "AbortError" || /cancel(?:led|ed)/i.test(message)) {
    key = "search.cancelled";
  } else if (/time(?:d)? out|time limit|deadline/i.test(message)) {
    key = "search.timeout";
  } else if (/\bbusy\b|concurrency limit|\b429\b|\b503\b/i.test(message)) {
    key = "search.busy";
  }
  return t(locale, key);
}

export function modalErrorText(locale) {
  return t(locale, "error.form");
}

export function assertDiscordCatalogComplete() {
  const english = Object.keys(CATALOGS.en).sort();
  for (const locale of SUPPORTED_DISCORD_LOCALES) {
    const keys = Object.keys(CATALOGS[locale]).sort();
    if (keys.length !== english.length || keys.some((key, index) => key !== english[index])) {
      throw new Error(`Discord translation catalog '${locale}' is incomplete.`);
    }
  }
  return true;
}

function supportedLocale(value) {
  return typeof value === "string" && SUPPORTED_DISCORD_LOCALES.includes(value)
    ? value
    : null;
}

function safePublicValidationMessage(message, locale) {
  if (!message || containsInternalSurface(message)) return t(locale, "error.validation");
  return message.slice(0, 1200);
}

function koreanValidationMessage(message) {
  const exact = KOREAN_VALIDATION_MESSAGES.get(message);
  if (exact) return exact;
  for (const [pattern, replacement] of KOREAN_VALIDATION_PATTERNS) {
    const match = pattern.exec(message);
    if (match) return replacement(...match.slice(1));
  }
  return t("ko", "error.validation");
}

function containsInternalSurface(message) {
  return /(oracle|cloud\s*run|gateway|job service|job id|job state|protocol|endpoint|authorization|token|worker|logical processor|process signal|exit code|runtime|vCPU|OCI|vault|engine|calculation server|schema|catalog|contract|allocation|policy|out of bounds|component IDs?|Discord supplied|Discord (?:command )?(?:input )?form|Discord Modal|Modal (?:input|select|component|limit)|payload URL|cannot be represented|received an invalid|\bE(?:ACCES|PERM|NOENT|IO)\b|\bsyscall\b|\b(?:backend|tablebase|Web?GPU|WASM)\b|node_modules|\bat\s+file:|(?:^|\s)[A-Za-z]:[\\/]|\/(?:home|root|var|tmp|opt|workspace)\/)/i.test(message);
}

const KOREAN_VALIDATION_MESSAGES = new Map([
  ["Enter a Clearra command.", "Clearra 명령어를 입력해 주세요."],
  ["The command has too many arguments.", "명령어 인수가 너무 많습니다."],
  ["The command is too long.", "명령어가 너무 깁니다."],
  ["The command contains an unterminated quote.", "명령어의 따옴표가 닫히지 않았습니다."],
  ["File and custom-code inputs are not available through Discord.", "Discord에서는 파일 경로와 사용자 코드 입력을 사용할 수 없습니다."],
  ["target must contain at least one occupied cell.", "목표 필드에는 블록이 하나 이상 있어야 합니다."],
  ["base and target must not overlap; target contains only cells to add.", "기존 필드와 목표 필드는 겹칠 수 없습니다. 목표 필드에는 추가할 블록만 입력해 주세요."],
  ["target occupied-cell count must be divisible by four.", "목표 필드의 블록 수는 4의 배수여야 합니다."],
  ["base must not contain an already completed row.", "기존 필드에는 이미 완성된 줄이 없어야 합니다."],
  ["next must be a queue or pattern, not a command-line option.", "넥스트에는 명령어 옵션이 아닌 큐 또는 패턴을 입력해 주세요."],
  ["next must be an exact queue containing only IOTSZJL pieces.", "넥스트에는 IOTSZJL만 사용한 정확한 큐를 입력해 주세요."],
  ["field URL is invalid.", "필드 URL이 올바르지 않습니다."],
  ["field URL must contain exactly one CTK3 or Fumen document.", "필드 URL에는 CTK3 또는 Fumen 문서가 정확히 하나 있어야 합니다."],
  ["field document cannot be empty.", "필드 문서는 비워 둘 수 없습니다."],
  ["field must contain one raw CTK3 or v115 Fumen document.", "필드에는 CTK3 또는 v115 Fumen 문서 하나를 입력해 주세요."],
  ["v110 Fumen is not supported by the Clearra search decoder; use v115.", "Clearra 탐색에서는 v110 Fumen을 지원하지 않습니다. v115를 사용해 주세요."],
  ["CTK3 search fields must be exactly 10 columns wide.", "CTK3 탐색 필드는 정확히 10열이어야 합니다."],
  ["CTK3 field could not be decoded.", "CTK3 필드를 해석할 수 없습니다."],
  ["next pattern alternatives must have the same piece count.", "넥스트 패턴의 각 대안은 미노 수가 같아야 합니다."],
  ["next pattern must contain at least one piece.", "넥스트 패턴에는 미노가 하나 이상 있어야 합니다."],
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
  ["remaining must contain only IOTSZJL pieces.", "남은 미노에는 IOTSZJL만 사용할 수 있습니다."],
  ["Verify scope must be pc, setup, cover, build, or kicks.", "검증 범위는 pc, setup, cover, build, kicks 중 하나여야 합니다."],
]);

const KOREAN_VALIDATION_PATTERNS = Object.freeze([
  [/^(.+) is required in the Clearra command Modal\.$/, (name) => `${koreanInputName(name)} 입력은 필수입니다.`],
  [/^\/(.+) input is required\.$/, (name) => `${koreanInputName(name)} 입력은 필수입니다.`],
  [/^(.+) cannot be empty\.$/, (name) => `${koreanInputName(name)} 입력은 비워 둘 수 없습니다.`],
  [/^(.+) must be text\.$/, (name) => `${koreanInputName(name)} 입력은 텍스트여야 합니다.`],
  [/^(.+) exceeds the (\d+)-character limit\.$/, (name, limit) => `${koreanInputName(name)} 입력은 ${limit}자를 넘을 수 없습니다.`],
  [/^(.+) must be an integer from (\d+) through (\d+)\.$/, (name, min, max) => `${koreanInputName(name)} 값은 ${min}부터 ${max}까지의 정수여야 합니다.`],
  [/^(.+) grid rows must be exactly 10 columns wide\.$/, (name) => `${koreanInputName(name)} 격자의 각 줄은 정확히 10칸이어야 합니다.`],
  [
    /^(.+) grid must contain from one through (six|twenty-four) rows\.$/,
    (name, maximum) =>
      `${koreanInputName(name)} 격자는 1–${maximum === "six" ? "6" : "24"}줄이어야 합니다.`,
  ],
  [/^(.+) CTK3 must contain exactly one page\.$/, (name) => `${koreanInputName(name)} CTK3에는 페이지가 정확히 하나 있어야 합니다.`],
  [/^(.+) CTK3 exceeds the (\d+)-row limit\.$/, (name, rows) => `${koreanInputName(name)} CTK3는 ${rows}줄을 넘을 수 없습니다.`],
  [/^(.+) Fumen could not be decoded\.$/, (name) => `${koreanInputName(name)} Fumen을 해석할 수 없습니다.`],
  [/^(.+) Fumen must contain exactly one page\.$/, (name) => `${koreanInputName(name)} Fumen에는 페이지가 정확히 하나 있어야 합니다.`],
  [/^(.+) requires a value\.$/, (name) => `${koreanInputName(name)} 값을 입력해 주세요.`],
]);

const KOREAN_INPUT_NAMES = Object.freeze({
  arguments: "명령어",
  image: "이미지",
  field: "필드",
  base: "기존 필드",
  target: "목표 필드",
  next: "넥스트",
  lines: "줄",
  kicktable: "킥테이블",
  options: "옵션",
  remaining: "남은 미노",
  scope: "범위",
});

function koreanInputName(name) {
  return KOREAN_INPUT_NAMES[String(name).trim().toLowerCase()] ?? name;
}
