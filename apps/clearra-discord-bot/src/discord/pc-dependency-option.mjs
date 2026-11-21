// Discord's opt-in is intentionally narrower than the ordinary PC chance
// surface. The canonical CLI still owns engine admission and exact fallback.
export const PC_DEPENDENCY_OPTION = "dependency";
export const PC_DEPENDENCY_FLAG = "--build-dependency-dag";
export const NO_PC_DEPENDENCY_FLAG = "--no-build-dependency-dag";

const CHANCE_VALUE_OPTIONS = new Set([
  "--lines", "--height", "--board-mask", "--pieces", "--rule", "--patterns", "--hold",
]);

export function supportsPcDependencyOption({ lines, rule, patterns }) {
  const source = typeof patterns === "string"
    ? patterns.replace(/[\s,]+/gu, "").toUpperCase()
    : null;
  return Number.isInteger(lines) && lines >= 1 && lines <= 4
    && (rule === "srs-plus" || rule === "jstris-180")
    && (source === "P7" || source === "P7P4");
}

// Both gateway and job-service prepare the same argv. Reject duplicate or
// conflicting scope data instead of guessing which value the engine will use.
export function supportsPreparedPcDependencyArguments(arguments_) {
  if (arguments_[0] !== "pc" || arguments_[1] !== "chance") return false;
  const scope = new Map();
  for (let index = 2; index < arguments_.length; index += 1) {
    const token = arguments_[index];
    const equals = token.indexOf("=");
    const name = equals < 0 ? token : token.slice(0, equals);
    if (scope.has(name)) return false;
    if (token === "--no-hold") {
      scope.set(name, true);
      continue;
    }
    if (!CHANCE_VALUE_OPTIONS.has(name)) return false;
    const value = equals < 0 ? arguments_[++index] : token.slice(equals + 1);
    if (typeof value !== "string" || value.startsWith("-")) return false;
    scope.set(name, value);
  }
  const lines = scope.get("--lines");
  return /^[1-4]$/u.test(lines ?? "")
    && !(scope.has("--hold") && scope.has("--no-hold"))
    && (scope.get("--height") ?? lines) === lines
    && supportsPcDependencyOption({
      lines: Number(lines),
      rule: scope.get("--rule"),
      patterns: scope.get("--patterns"),
    });
}
