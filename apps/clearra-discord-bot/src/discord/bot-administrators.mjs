const DISCORD_SNOWFLAKE = /^\d{17,20}$/;
const DEFAULT_APPLICATION_OWNER_TTL_MS = 5 * 60_000;

export class DiscordBotAdministratorAuthority {
  constructor(rest, configuredUserIds = [], options = {}) {
    this.rest = rest;
    this.configuredUserIds = new Set(
      configuredUserIds.map((value) =>
        requiredSnowflake(value, "administrator user ID")),
    );
    this.applicationAdministratorIds = null;
    this.applicationId = null;
    this.applicationRequest = null;
    this.now = options.now ?? Date.now;
    this.applicationOwnerTtlMs = positiveInteger(
      options.applicationOwnerTtlMs,
      DEFAULT_APPLICATION_OWNER_TTL_MS,
    );
    this.applicationOwnerExpiresAt = 0;
  }

  async allows(interaction) {
    const actorId = interactionUserId(interaction);
    if (!actorId) return false;
    if (this.configuredUserIds.has(actorId)) return true;
    if (typeof this.rest?.application !== "function") return false;

    const interactionApplicationId = optionalIdentifier(
      interaction?.application_id,
    );
    if (
      interactionApplicationId &&
      this.applicationId &&
      interactionApplicationId !== this.applicationId
    ) {
      return false;
    }

    const administratorIds = await this.#applicationAdministrators();
    if (
      interactionApplicationId &&
      this.applicationId !== interactionApplicationId
    ) {
      return false;
    }
    return administratorIds.has(actorId);
  }

  async #applicationAdministrators() {
    if (
      this.applicationAdministratorIds &&
      this.now() < this.applicationOwnerExpiresAt
    ) {
      return this.applicationAdministratorIds;
    }
    if (!this.applicationRequest) {
      this.applicationRequest = this.rest.application()
        .then((application) => {
          this.applicationId = requiredSnowflake(
            application?.id,
            "application ID",
          );
          this.applicationAdministratorIds = new Set(
            resolveDiscordBotAdministratorIds(application),
          );
          this.applicationOwnerExpiresAt =
            this.now() + this.applicationOwnerTtlMs;
          return this.applicationAdministratorIds;
        })
        .finally(() => {
          this.applicationRequest = null;
        });
    }
    return this.applicationRequest;
  }
}

export function resolveDiscordBotAdministratorIds(
  application,
  configuredUserIds = [],
) {
  const administratorIds = new Set(
    configuredUserIds.map((value) => requiredSnowflake(value, "administrator user ID")),
  );

  const applicationOwnerId = application?.team?.owner_user_id ??
    application?.owner?.id ??
    null;
  if (applicationOwnerId !== null) {
    administratorIds.add(
      requiredSnowflake(applicationOwnerId, "application owner user ID"),
    );
  }

  return Object.freeze([...administratorIds]);
}

export function interactionUserId(interaction) {
  const value = interaction?.member?.user?.id ?? interaction?.user?.id ?? null;
  return typeof value === "string" && DISCORD_SNOWFLAKE.test(value)
    ? value
    : null;
}

function requiredSnowflake(value, label) {
  const normalized = String(value ?? "").trim();
  if (!DISCORD_SNOWFLAKE.test(normalized)) {
    throw new Error(`Discord ${label} must be a 17-20 digit snowflake.`);
  }
  return normalized;
}

function optionalIdentifier(value) {
  const normalized = String(value ?? "").trim();
  return normalized || null;
}

function positiveInteger(value, fallback) {
  if (value === undefined) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("Discord application-owner cache TTL must be positive.");
  }
  return parsed;
}
