import { canonicalClearraOperationalCommand } from "./clearra/command.mjs";

const SCOPES = new Set(["gateway", "interaction", "job"]);
const KINDS = new Set(["slash", "text", "render", "search"]);
const STATUSES = new Set(["succeeded", "failed", "cancelled"]);
const DISCORD_ROOT_COMMANDS = new Set([
  "best-save",
  "best-setup",
  "chance",
  "channel-settings",
  "congruent",
  "congruent-cover",
  "cover",
  "cover-percent",
  "damage",
  "dpc-finder",
  "get-original-gif",
  "help",
  "minimals",
  "path",
  "pc-setup",
  "percent",
  "render-file",
  "saves",
  "score",
  "score-finder",
  "score-minimals",
  "server-settings",
  "setup",
  "setup-cover",
  "special-cover",
  "spin",
  "spin-cover",
  "verify",
]);
const DISCORD_COMMAND_PATHS = new Set([
  ...DISCORD_ROOT_COMMANDS,
  "channel-settings.disable",
  "channel-settings.enable",
  "channel-settings.language-reset",
  "channel-settings.language-set",
  "channel-settings.language-show",
  "server-settings.language-reset",
  "server-settings.language-set",
  "server-settings.language-show",
  "server-settings.pause",
  "server-settings.resume",
]);
/**
 * Emits one allow-listed, metadata-only terminal record. Arbitrary fields and
 * error messages are intentionally not accepted, so command input, Discord
 * users, routing IDs, tokens, URLs and process output cannot enter this log.
 */
export function writeOperationalLog(logger, value) {
  const scope = SCOPES.has(value?.scope) ? value.scope : null;
  const kind = KINDS.has(value?.kind) ? value.kind : null;
  const status = STATUSES.has(value?.status) ? value.status : null;
  if (!scope || !kind || !status) return false;
  const command = canonicalOperationalCommand(value.command);
  const durationMs = Number.isFinite(value.durationMs)
    ? Math.max(0, Math.round(value.durationMs))
    : null;
  const timestamp = new Date(value.at ?? Date.now());
  const record = {
    event: "clearra.operation",
    at: Number.isFinite(timestamp.getTime())
      ? timestamp.toISOString()
      : new Date().toISOString(),
    scope,
    kind,
    command,
    status,
    durationMs,
  };
  try {
    const method = status === "failed" ? "error" : "info";
    logger?.[method]?.(JSON.stringify(record));
    return true;
  } catch {
    return false;
  }
}

/**
 * Reduces a caller-supplied label to a command that actually exists in the
 * Discord catalog or the curated Clearra execution policy. Unknown but
 * syntactically command-like text is deliberately rejected.
 */
export function canonicalOperationalCommand(value) {
  if (typeof value !== "string") return null;
  const candidate = value.trim().toLowerCase().replaceAll("_", "-");
  if (!candidate) return null;

  const [root, ...suffix] = candidate.split(".");
  if (DISCORD_ROOT_COMMANDS.has(root)) {
    const canonical = [root, ...suffix].join(".");
    if (DISCORD_COMMAND_PATHS.has(canonical)) return canonical;
  }
  if (DISCORD_COMMAND_PATHS.has(candidate)) return candidate;
  // Compatibility routes retain an internal sfinder prefix. Public telemetry
  // uses the same identity as the equivalent slash command so one operation is
  // never split into transport-specific rows.
  if (candidate.startsWith("sfinder.")) {
    const publicCommand = candidate.slice("sfinder.".length);
    if (DISCORD_ROOT_COMMANDS.has(publicCommand)) return publicCommand;
  }
  return canonicalClearraOperationalCommand(candidate);
}
