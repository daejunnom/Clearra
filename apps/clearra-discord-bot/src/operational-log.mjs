import { canonicalClearraOperationalCommand } from "./clearra/command.mjs";
import {
  messageCommandCatalog,
  slashCommandCatalog,
} from "./discord/slash-command-catalog.mjs";
import {
  discordGenericCompatibilityRoutes,
  productCapabilityRegistry,
} from "./discord/capability-registry.mjs";

const SCOPES = new Set(["gateway", "interaction", "job"]);
const KINDS = new Set(["slash", "text", "render", "search"]);
const STATUSES = new Set(["succeeded", "failed", "cancelled", "delegated"]);
const TIMEOUT_CLASSES = new Set([
  "pc_reverse",
  "build_long",
  "setup_long",
  "forward_long",
  "structure_long",
  "utility_bounded",
  "diagnostic",
  "default",
]);
const DISCORD_ROOT_COMMANDS = new Set([
  ...slashCommandCatalog.map(({ name }) => name),
  ...messageCommandCatalog.map(({ name }) => name),
]);
const DISCORD_COMMAND_PATHS = collectDiscordCommandPaths();
const CAPABILITY_TELEMETRY_BY_COMMAND = collectCapabilityTelemetryIdentities();
const CAPABILITY_TELEMETRY_IDENTITIES = new Set(
  CAPABILITY_TELEMETRY_BY_COMMAND.values(),
);

function collectDiscordCommandPaths() {
  const paths = new Set(DISCORD_ROOT_COMMANDS);
  for (const command of slashCommandCatalog) {
    for (const option of command.registration?.options ?? []) {
      if (option?.type === 1 || option?.type === 2) {
        paths.add(`${command.name}.${option.name}`);
      }
    }
  }
  return paths;
}

function collectCapabilityTelemetryIdentities() {
  const identities = new Map();
  for (const capability of productCapabilityRegistry) {
    if (
      capability.status === "planned" &&
      capability.discordSurfaceStatus !== "ready"
    ) continue;
    const routes = [capability.canonical, ...capability.aliases];
    for (const route of routes) {
      if (!route.slash && !route.text) continue;
      const command = [route.root, route.subcommand].filter(Boolean).join(".");
      if (capability.status === "hidden") {
        identities.set(`sfinder.${command}`, capability.telemetryIdentity);
      } else {
        identities.set(command, capability.telemetryIdentity);
      }
    }
  }
  for (const route of discordGenericCompatibilityRoutes) {
    identities.set(route.root, route.telemetryIdentity);
  }
  return identities;
}
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
  const hasTimeoutPolicy =
    value.timeoutClass !== undefined || value.timeoutMs !== undefined;
  const timeoutClass = TIMEOUT_CLASSES.has(value.timeoutClass)
    ? value.timeoutClass
    : null;
  const timeoutMs = Number.isSafeInteger(value.timeoutMs) && value.timeoutMs > 0
    ? value.timeoutMs
    : null;
  if (hasTimeoutPolicy && (!timeoutClass || !timeoutMs)) return false;
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
    ...(hasTimeoutPolicy ? { timeoutClass, timeoutMs } : {}),
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
  // Stable capability IDs are already canonical. Accepting them here keeps
  // normalization idempotent when a catalog path is resolved before it enters
  // another privacy or persistence boundary (for example help -> meta.help).
  if (CAPABILITY_TELEMETRY_IDENTITIES.has(candidate)) return candidate;
  const capabilityIdentity = CAPABILITY_TELEMETRY_BY_COMMAND.get(candidate);
  if (capabilityIdentity) return capabilityIdentity;
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
    if (DISCORD_ROOT_COMMANDS.has(publicCommand)) {
      return CAPABILITY_TELEMETRY_BY_COMMAND.get(publicCommand) ?? publicCommand;
    }
  }
  return canonicalClearraOperationalCommand(candidate);
}
