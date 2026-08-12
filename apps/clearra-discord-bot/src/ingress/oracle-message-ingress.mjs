import { writeOperationalLog } from "../operational-log.mjs";
import { OracleMessageArbitration } from "./message-arbitration.mjs";
import {
  classifyOracleMessageCommand,
  classifyOracleMessageKind,
  delegatedOracleMessageOutcome,
  exceptionOracleMessageOutcome,
  normalizeOracleMessageOutcome,
  publicOracleIngressOutcome,
} from "./oracle-message-outcome.mjs";

const GUILDS_INTENT = 1 << 0;
const GUILD_MESSAGES_INTENT = 1 << 9;
const DIRECT_MESSAGES_INTENT = 1 << 12;
const MESSAGE_CONTENT_INTENT = 1 << 15;

const DEFAULT_MAX_INPUT_CHARS = 2_000;
const DEFAULT_MAX_CONCURRENT_MESSAGES = 1;
const DEFAULT_MAX_PENDING_MESSAGES = 0;
const DEFAULT_USER_COOLDOWN_MS = 0;
const MAX_REMEMBERED_MESSAGE_IDS = 4_096;
const MAX_CONCURRENT_MANAGEMENT_MESSAGES = 1;
const MAX_PENDING_MANAGEMENT_MESSAGES = 2;

export function isOracleMessageDispatch(type) {
  return type === "MESSAGE_CREATE" || type === "MESSAGE_UPDATE";
}

export function oracleGatewayIntents(config = {}) {
  let intents = 0;
  if (!config.oracleRenderEnabled && !config.oracleTextEnabled) return intents;
  intents |= GUILD_MESSAGES_INTENT | DIRECT_MESSAGES_INTENT;
  if (config.oracleTextEnabled) intents |= MESSAGE_CONTENT_INTENT;
  return intents;
}

export class OracleMessageIngress {
  constructor(handler, config = {}, dependencies = {}) {
    if (
      typeof handler?.acceptsOracleMessage !== "function" ||
      typeof handler?.handleOracleMessage !== "function"
    ) {
      throw new TypeError("An Oracle message handler is required.");
    }

    this.handler = handler;
    this.fetchMessage = dependencies.fetchMessage ?? null;
    if (this.fetchMessage !== null && typeof this.fetchMessage !== "function") {
      throw new TypeError("The Oracle message fetch dependency is invalid.");
    }
    this.enabled = Boolean(
      config.oracleRenderEnabled || config.oracleTextEnabled,
    );
    this.textEnabled = Boolean(config.oracleTextEnabled);
    this.messageArbitration = new OracleMessageArbitration(config);
    this.commandPrefixes = this.messageArbitration.commandPrefixes;
    this.allowedChannelIds = new Set(
      Array.isArray(config.oracleAllowedChannelIds)
        ? config.oracleAllowedChannelIds.map(String)
        : [],
    );
    this.maxInputChars = positiveInteger(
      config.oracleMaxInputChars,
      DEFAULT_MAX_INPUT_CHARS,
    );
    this.maxConcurrentMessages = positiveInteger(
      config.oracleMaxConcurrentMessages,
      DEFAULT_MAX_CONCURRENT_MESSAGES,
    );
    this.maxPendingMessages = nonNegativeInteger(
      config.oracleMaxPendingMessages,
      DEFAULT_MAX_PENDING_MESSAGES,
    );
    this.maxPendingSelfMessages = nonNegativeInteger(
      config.oracleMaxPendingSelfMessages,
      DEFAULT_MAX_PENDING_MESSAGES,
    );
    this.userCooldownMs = nonNegativeInteger(
      config.oracleUserCooldownMs,
      DEFAULT_USER_COOLDOWN_MS,
    );
    this.now = dependencies.now ?? Date.now;
    this.logger = dependencies.logger ?? console;
    this.operationalScope = dependencies.operationalScope ?? null;

    this.botUserId = null;
    this.activeMessages = 0;
    this.activeManagementMessages = 0;
    this.pendingManagementMessages = [];
    this.pendingSelfMessages = [];
    this.pendingUserMessages = [];
    this.seenMessageIds = new Set();
    this.seenMessageOrder = [];
    this.userAcceptedAt = new Map();
  }

  setBotUserId(id) {
    this.botUserId = id === undefined || id === null ? null : String(id);
    return this;
  }

  async acceptDispatch(type, message) {
    if (
      isOracleMessageDispatch(type) &&
      this.enabled &&
      typeof message?.id === "string" &&
      this.seenMessageIds.has(message.id)
    ) {
      return { accepted: false, reason: "duplicate-message" };
    }

    let candidate = message;
    if (this.#shouldHydrateUpdate(type, message)) {
      candidate = await this.#hydrateUpdate(message);
    }

    return this.#acceptPrepared(type, candidate);
  }

  #acceptPrepared(type, message) {
    const rejection = this.#rejectionReason(type, message);
    if (rejection !== null) return rejection;

    const selfMessage = message.author.id === this.botUserId;
    if (this.messageArbitration.delegatesPrefixedText(
      message,
      this.botUserId,
    )) {
      this.#rememberMessageId(message.id);
      return this.#completeDelegation(message);
    }
    let acceptedByHandler;
    try {
      acceptedByHandler = this.handler.acceptsOracleMessage(message, {
        botUserId: this.botUserId,
      });
    } catch {
      return Promise.reject(new Error("Oracle message acceptance failed."));
    }
    if (acceptedByHandler !== true) {
      return {
        accepted: false,
        reason: "unsupported-message",
      };
    }

    const managementMessage = this.handler.oracleAccessDecision?.(message, {
      botUserId: this.botUserId,
    })?.management === true;
    if (this.messageArbitration.delegatesAcceptedCandidate(
      message,
      this.botUserId,
      managementMessage,
    )) {
      this.#rememberMessageId(message.id);
      return this.#completeDelegation(message);
    }
    if (managementMessage) {
      this.#rememberMessageId(message.id);
      return this.#authorizeAndScheduleManagement(message);
    }

    const pendingMessages = selfMessage
      ? this.pendingSelfMessages
      : this.pendingUserMessages;
    const pendingLimit = selfMessage
      ? this.maxPendingSelfMessages
      : this.maxPendingMessages;
    if (
      this.activeMessages >= this.maxConcurrentMessages &&
      pendingMessages.length >= pendingLimit
    ) {
      return { accepted: false, reason: "message-queue-full" };
    }

    const now = Date.now();
    if (!selfMessage && this.userCooldownMs > 0) {
      const lastAcceptedAt = this.userAcceptedAt.get(message.author.id);
      if (
        lastAcceptedAt !== undefined &&
        now - lastAcceptedAt < this.userCooldownMs
      ) {
        return { accepted: false, reason: "user-cooldown" };
      }
    }

    this.#rememberMessageId(message.id);
    if (!selfMessage && this.userCooldownMs > 0) {
      this.#rememberUserAcceptance(message.author.id, now);
    }

    return new Promise((resolve, reject) => {
      const job = { message, resolve, reject };
      if (this.activeMessages < this.maxConcurrentMessages) {
        this.#start(job);
      } else {
        pendingMessages.push(job);
      }
    });
  }

  #shouldHydrateUpdate(type, message) {
    return Boolean(
      type === "MESSAGE_UPDATE" &&
      this.enabled &&
      message &&
      typeof message === "object" &&
      typeof message.id === "string" &&
      message.id.length > 0 &&
      typeof message.channel_id === "string" &&
      message.channel_id.length > 0 &&
      (message.author === undefined || message.author === null) &&
      typeof message.webhook_id === "string" &&
      message.webhook_id.length > 0 &&
      Array.isArray(message.attachments) &&
      message.attachments.length > 0
    );
  }

  async #hydrateUpdate(partial) {
    try {
      if (!this.fetchMessage) throw new Error("message fetch unavailable");
      const fetched = await this.fetchMessage(partial.channel_id, partial.id);
      if (
        !fetched ||
        typeof fetched !== "object" ||
        fetched.id !== partial.id ||
        fetched.channel_id !== partial.channel_id
      ) {
        throw new Error("message identity mismatch");
      }
      return {
        ...fetched,
        ...partial,
        author: fetched.author,
      };
    } catch {
      throw new Error("Oracle message update hydration failed.");
    }
  }

  #rejectionReason(type, message) {
    if (!isOracleMessageDispatch(type)) {
      return { accepted: false, reason: "gateway-message-events-disabled" };
    }
    if (!this.enabled) {
      return { accepted: false, reason: "oracle-message-events-disabled" };
    }
    if (
      !message ||
      typeof message !== "object" ||
      typeof message.id !== "string" ||
      message.id.length === 0 ||
      typeof message.channel_id !== "string" ||
      typeof message.author?.id !== "string"
    ) {
      return { accepted: false, reason: "invalid-message" };
    }
    if (this.seenMessageIds.has(message.id)) {
      return { accepted: false, reason: "duplicate-message" };
    }

    const selfMessage = message.author.id === this.botUserId;
    if (type === "MESSAGE_UPDATE") {
      if (!selfMessage || message.author.bot !== true) {
        return { accepted: false, reason: "message-update-not-self" };
      }
      if (
        typeof message.webhook_id !== "string" ||
        message.webhook_id.length === 0
      ) {
        return { accepted: false, reason: "message-update-not-webhook" };
      }
      if (
        !Array.isArray(message.attachments) ||
        message.attachments.length === 0
      ) {
        return { accepted: false, reason: "message-update-without-attachments" };
      }
    }
    if (!selfMessage && message.webhook_id) {
      return { accepted: false, reason: "webhook-message" };
    }
    if (!selfMessage && message.author.bot) {
      return { accepted: false, reason: "bot-message" };
    }

    const accessDecision = this.handler.oracleAccessDecision?.(message, {
      botUserId: this.botUserId,
    });
    if (accessDecision?.allowed === false) {
      return {
        accepted: false,
        reason: accessDecision.reason ?? "command-disabled",
      };
    }

    if (!selfMessage && accessDecision?.management !== true) {
      if (
        this.allowedChannelIds.size > 0 &&
        !this.allowedChannelIds.has(message.channel_id)
      ) {
        return { accepted: false, reason: "channel-not-allowed" };
      }
      if (
        !this.textEnabled &&
        message.guild_id &&
        !messageMentionsBot(message, this.botUserId)
      ) {
        return { accepted: false, reason: "explicit-invocation-required" };
      }
    }

    const content = message.content ?? "";
    if (typeof content !== "string") {
      return { accepted: false, reason: "invalid-message" };
    }
    if (content.length > this.maxInputChars) {
      return { accepted: false, reason: "message-too-long" };
    }
    return null;
  }

  #rememberMessageId(id) {
    this.seenMessageIds.add(id);
    this.seenMessageOrder.push(id);
    if (this.seenMessageOrder.length <= MAX_REMEMBERED_MESSAGE_IDS) return;

    const expiredId = this.seenMessageOrder.shift();
    this.seenMessageIds.delete(expiredId);
  }

  #rememberUserAcceptance(userId, acceptedAt) {
    this.userAcceptedAt.delete(userId);
    this.userAcceptedAt.set(userId, acceptedAt);

    for (const [candidateId, candidateAcceptedAt] of this.userAcceptedAt) {
      if (acceptedAt - candidateAcceptedAt < this.userCooldownMs) break;
      this.userAcceptedAt.delete(candidateId);
    }
  }

  #start(job) {
    this.activeMessages += 1;
    void this.#run(job);
  }

  async #completeDelegation(message) {
    const startedAt = numericClock(this.now);
    const outcome = delegatedOracleMessageOutcome();
    await this.#observeOutcome(message, outcome);
    this.#writeTerminalLog(message, outcome.status, startedAt);
    return {
      accepted: false,
      reason: "delegated",
      owner: "sfinder-man",
    };
  }

  #authorizeAndScheduleManagement(message) {
    if (typeof this.handler.authorizeOracleManagementMessage !== "function") {
      return { accepted: false, reason: "management-authorization-unavailable" };
    }
    if (
      this.activeManagementMessages >= MAX_CONCURRENT_MANAGEMENT_MESSAGES &&
      this.pendingManagementMessages.length >= MAX_PENDING_MANAGEMENT_MESSAGES
    ) {
      return { accepted: false, reason: "management-queue-full" };
    }
    return new Promise((resolve, reject) => {
      const job = { message, resolve, reject };
      if (this.activeManagementMessages < MAX_CONCURRENT_MANAGEMENT_MESSAGES) {
        this.#startManagement(job);
      } else {
        this.pendingManagementMessages.push(job);
      }
    });
  }

  #startManagement(job) {
    this.activeManagementMessages += 1;
    void this.#runManagement(job);
  }

  async #runManagement(job) {
    const startedAt = numericClock(this.now);
    try {
      let authorized = false;
      try {
        authorized = await this.handler.authorizeOracleManagementMessage(
          job.message,
        );
      } catch {
        authorized = false;
      }
      if (!authorized) {
        job.resolve({ accepted: false, reason: "management-not-authorized" });
        return;
      }

      try {
        const outcome = normalizeOracleMessageOutcome(
          await this.handler.handleOracleMessage(job.message),
        );
        await this.#observeOutcome(job.message, outcome);
        this.#writeTerminalLog(job.message, outcome.status, startedAt);
        job.resolve(publicOracleIngressOutcome(outcome));
      } catch (error) {
        const outcome = exceptionOracleMessageOutcome(error);
        await this.#observeOutcome(job.message, outcome);
        this.#writeTerminalLog(job.message, outcome.status, startedAt);
        job.reject(new Error("Oracle message handling failed."));
      }
    } finally {
      this.activeManagementMessages -= 1;
      const next = this.pendingManagementMessages.shift();
      if (next) this.#startManagement(next);
    }
  }

  async #run(job) {
    const startedAt = numericClock(this.now);
    try {
      const outcome = normalizeOracleMessageOutcome(
        await this.handler.handleOracleMessage(job.message),
      );
      await this.#observeOutcome(job.message, outcome);
      this.#writeTerminalLog(job.message, outcome.status, startedAt);
      job.resolve(publicOracleIngressOutcome(outcome));
    } catch (error) {
      const outcome = exceptionOracleMessageOutcome(error);
      await this.#observeOutcome(job.message, outcome);
      this.#writeTerminalLog(job.message, outcome.status, startedAt);
      job.reject(new Error("Oracle message handling failed."));
    } finally {
      this.activeMessages -= 1;
      const next =
        this.pendingSelfMessages.shift() ?? this.pendingUserMessages.shift();
      if (next) this.#start(next);
    }
  }

  async #observeOutcome(message, outcome) {
    if (typeof this.handler.observeOracleMessageOutcome !== "function") return;
    try {
      await this.handler.observeOracleMessageOutcome(message, outcome);
    } catch {
      // A private status observer is not allowed to strand public ingress or
      // retain its concurrency slot. The durable store owns its own retry and
      // restart reconciliation policy.
    }
  }

  #writeTerminalLog(message, status, startedAt) {
    if (!this.operationalScope) return;
    const completedAt = numericClock(this.now);
    writeOperationalLog(this.logger, {
      scope: this.operationalScope,
      kind: classifyOracleMessageKind(
        message?.content,
        this.commandPrefixes,
      ),
      command: classifyOracleMessageCommand(
        message?.content,
        this.commandPrefixes,
      ),
      status,
      durationMs:
        startedAt === null || completedAt === null
          ? null
          : completedAt - startedAt,
    });
  }
}

function numericClock(clock) {
  try {
    const value = Number(clock());
    return Number.isFinite(value) ? value : null;
  } catch {
    return null;
  }
}

function messageMentionsBot(message, botUserId) {
  if (!botUserId || !Array.isArray(message?.mentions)) return false;
  return message.mentions.some((mentioned) => mentioned?.id === botUserId);
}

function positiveInteger(value, fallback) {
  return Number.isSafeInteger(value) && value > 0 ? value : fallback;
}

function nonNegativeInteger(value, fallback) {
  return Number.isSafeInteger(value) && value >= 0 ? value : fallback;
}
