import {
  ClearraJobExecutor,
  isSetupSearchArguments,
  prepareClearraArguments,
  searchTimeoutMsForArguments,
  tilingOnlyRequested,
} from "./clearra/command.mjs";
import { ClearraDirectExecutor } from "./clearra/direct-executor.mjs";
import { parseClearraTextRequest } from "./clearra/text-command.mjs";
import { buildCtk3Result } from "./clearra/ctk3-result.mjs";
import {
  CTK3_FILE_MIME_TYPE,
  encodeCtk3File,
  isCtk3File,
} from "ctk3";
import {
  attachmentMessage,
  fileComponentMessage,
  textMessage,
} from "./discord/rest.mjs";
import {
  assertGifBytes,
  isUnavailableDiscordAttachmentError,
  prioritizeRenderGifCandidates,
  readRenderFileImage,
  readRenderFileTargetMessageId,
  renderGifCandidate,
  resolveRenderMessageReference,
} from "./discord/render-file.mjs";
import { RestInteractionAcknowledger } from "./discord/interaction-acknowledger.mjs";
import {
  findApplicationCommand,
  formatSlashCommandHelp,
  globalCommands,
  resolveSlashCommandInvocation,
} from "./discord/slash-command-catalog.mjs";
import {
  findCommandModalCommand,
  readCommandModalLocale,
  readCommandModalOptions,
} from "./discord/field-modal.mjs";
import {
  operationErrorText,
  t,
  validationErrorText,
} from "./discord/i18n.mjs";
import {
  DiscordLocalePreferences,
} from "./discord/locale-preferences.mjs";
import { DiscordAccessPreferences } from "./discord/access-preferences.mjs";
import {
  canManageDiscordSettings,
  formatTextManagementHelp,
  isChannelEnableRequest,
  isServerResumeRequest,
  isTextManagementCandidate,
  readDiscordManagementRequest,
  readTextManagementRequest,
} from "./discord/management-command.mjs";
import { DiscordBotAdministratorAuthority } from "./discord/bot-administrators.mjs";
import {
  buildSlashCommandArgumentPlan,
  readHelpArgument,
} from "./discord/slash-command-input.mjs";
import { BoundedGifRenderer } from "./viewer/async-gif.mjs";
import {
  decodeViewerFile,
  extractViewerDocuments,
} from "./viewer/document.mjs";
import {
  extractStandaloneRenderField,
  isStandaloneRenderField,
} from "./viewer/render-input.mjs";
import { GifRenderLimitError } from "./viewer/gif.mjs";
import {
  buildClearraRendererUrl,
  buildClearraViewerUrl,
} from "./viewer/link.mjs";
import { buildSearchPreviewDocument } from "./viewer/search-preview.mjs";

const RESULT_LIMIT = 1900;
const DISCORD_CONTENT_LIMIT = 2000;
const RENDER_FILE_HISTORY_PAGE_SIZE = 100;
const RENDER_FILE_HISTORY_MAX_PAGES = 5;
const VIEWER_CANDIDATE_PATTERN = /(?:v11(?:0|5)|[Ddm]115)@|ctk3(?:b_|_|@)/i;
const INTEGRATED_PC_SERIES_RESULT_PATTERN = /^(?:Automatic PC target|자동 PC 목표): [1-6]L(?:\s|$)/;
const INTEGRATED_PC_SERIES_FILE_PATTERN = /^pc-[1-6]l-result\.ctk3$/i;
const GENERATED_RESULT_FILE_PATTERN = /^[a-z0-9_-]{1,64}-result\.ctk3$/i;
const DISCORD_SNOWFLAKE_PATTERN = /^\d{17,20}$/;
const DISCORD_EPOCH_MS = 1_420_070_400_000n;

export { globalCommands };

export class Clearrabot {
  constructor(rest, config, options = {}) {
    this.rest = rest;
    this.config = config;
    this.executor =
      options.executor ??
      (config.jobEndpoint
        ? new ClearraJobExecutor({
            endpoint: config.jobEndpoint,
            authorizationToken: config.jobToken,
            timeoutMs: config.searchTimeoutMs,
            reverseSearchTimeoutMs: config.reverseSearchTimeoutMs,
            forwardSearchTimeoutMs: config.forwardSearchTimeoutMs,
            maxOutputBytes: config.maxOutputBytes,
            pollIntervalMs: config.jobPollIntervalMs,
            cancelTimeoutMs: config.jobCancelTimeoutMs,
          })
        : new ClearraDirectExecutor(config));
    this.applicationId = options.applicationId ?? null;
    this.botUserId = options.botUserId ?? null;
    this.interactionAcknowledger =
      options.interactionAcknowledger ?? new RestInteractionAcknowledger(rest);
    this.localePreferences = options.localePreferences ??
      new DiscordLocalePreferences({ defaultLocale: config.defaultLocale });
    this.accessPreferences = options.accessPreferences ??
      new DiscordAccessPreferences();
    this.botAdministratorAuthority = options.botAdministratorAuthority ??
      new DiscordBotAdministratorAuthority(
        rest,
        config.discordAdminUserIds ?? [],
      );
    this.logger = options.logger ?? console;
    this.gifRenderer = options.gifRenderer ?? new BoundedGifRenderer({
      timeoutMs: config.oracleRenderTimeoutMs ?? 10_000,
      maxPending: config.oracleMaxPendingMessages ?? 8,
    });
    this.activeSearches = 0;
    this.pendingSearches = [];
    this.controllers = new Set();
    this.interactionDeadlineMs =
      config.interactionDeadlineMs ?? config.searchTimeoutMs ?? 3 * 60_000;
    this.setupProgressNoticeMs =
      config.setupProgressNoticeMs ?? 5 * 60_000;
    this.maxPendingSearches = config.maxPendingSearches ?? 8;
  }

  setApplicationId(applicationId) {
    this.applicationId = applicationId;
  }

  setBotUserId(botUserId) {
    this.botUserId = botUserId === undefined || botUserId === null
      ? null
      : String(botUserId);
  }

  resolveLocale(target, explicitLocale = null) {
    return this.localePreferences.resolve({
      guildId: target?.guild_id,
      channelId: target?.channel_id,
      interactionLocale: target?.locale,
    }, explicitLocale);
  }

  resolveResponseLocale(target) {
    let explicitLocale = null;
    if (findCommandModalCommand(target)) {
      try {
        explicitLocale = readCommandModalLocale(target);
      } catch {
        // Admission and terminal replies must survive malformed Modal metadata.
      }
    }
    return this.resolveLocale(target, explicitLocale);
  }

  interactionAccessDecision(interaction) {
    const guildId = interaction?.guild_id;
    if (!guildId) return Object.freeze({ allowed: true, reason: null });
    const command = findCommandModalCommand(interaction) ??
      findApplicationCommand(
        interaction?.data?.type,
        interaction?.data?.name,
      );
    let managementRequest = null;
    if (command?.kind === "management") {
      try {
        managementRequest = readDiscordManagementRequest(
          command.name,
          interaction.data?.options ?? [],
        );
      } catch {
        // Malformed management input is never a recovery command. The normal
        // handler still returns its validation error while access is enabled.
      }
    }
    if (this.accessPreferences.isGuildPaused(guildId)) {
      return isServerResumeRequest(managementRequest)
        ? Object.freeze({ allowed: true, reason: null })
        : Object.freeze({ allowed: false, reason: "guild-paused" });
    }
    const channelId = interaction?.channel_id;
    if (
      channelId &&
      this.accessPreferences.isChannelDisabled(channelId, guildId)
    ) {
      return isChannelEnableRequest(managementRequest)
        ? Object.freeze({ allowed: true, reason: null })
        : Object.freeze({ allowed: false, reason: "channel-disabled" });
    }
    return Object.freeze({ allowed: true, reason: null });
  }

  oracleAccessDecision(message, context = {}) {
    if (context.botUserId && message?.author?.id === context.botUserId) {
      // This is the downstream renderer for an already admitted command.
      return Object.freeze({ allowed: true, reason: null, management: false });
    }
    let managementRequest = null;
    let managementCandidate = false;
    for (const prefix of this.config.oracleCommandPrefixes ?? ["$", ">"] ) {
      if (!isTextManagementCandidate(message?.content, prefix)) continue;
      managementCandidate = true;
      try {
        managementRequest = readTextManagementRequest(message?.content, prefix);
      } catch {
        managementRequest = null;
      }
      break;
    }
    const guildId = message?.guild_id;
    if (!guildId) {
      return Object.freeze({
        allowed: true,
        reason: null,
        management: managementCandidate,
      });
    }
    if (this.accessPreferences.isGuildPaused(guildId)) {
      return isServerResumeRequest(managementRequest)
        ? Object.freeze({ allowed: true, reason: null, management: true })
        : Object.freeze({ allowed: false, reason: "guild-paused", management: false });
    }
    if (
      message?.channel_id &&
      this.accessPreferences.isChannelDisabled(message.channel_id, guildId)
    ) {
      return isChannelEnableRequest(managementRequest)
        ? Object.freeze({ allowed: true, reason: null, management: true })
        : Object.freeze({ allowed: false, reason: "channel-disabled", management: false });
    }
    return Object.freeze({
      allowed: true,
      reason: null,
      management: managementCandidate,
    });
  }

  async authorizeOracleManagementMessage(message) {
    const decision = this.oracleAccessDecision(message);
    if (decision.management !== true) return false;
    try {
      return await this.botAdministratorAuthority.allows({
        application_id: this.applicationId ?? this.config.applicationId,
        user: message?.author,
      });
    } catch {
      this.logger.warn(
        "ClearraBot administrator authority could not be checked.",
      );
      return false;
    }
  }

  accessBlockedText(decision, locale = "en") {
    return t(
      locale,
      decision?.reason === "guild-paused"
        ? "access.guild_paused"
        : "access.channel_disabled",
    );
  }

  stop() {
    this.gifRenderer.stop?.();
    for (const controller of this.controllers) controller.abort();
    this.controllers.clear();
    for (const pending of this.pendingSearches.splice(0)) {
      this.cleanupPendingSearch(pending);
      pending.reject(abortError("Clearrabot stopped."));
    }
  }

  async handleDispatch(type, data, options = {}) {
    if (type !== "INTERACTION_CREATE") return false;
    return this.handleInteraction(data, options);
  }

  async handleInteraction(interaction, context = {}) {
    const modalCommand = findCommandModalCommand(interaction);
    const applicationCommand =
      interaction.type === 2 &&
      interaction.data?.name
        ? findApplicationCommand(
            interaction.data.type,
            interaction.data.name,
          )
        : null;
    const command = modalCommand ?? applicationCommand;
    if (!command) return false;
    if (!this.applicationId) {
      this.applicationId = interaction.application_id;
    }
    const acknowledger =
      context.acknowledger ?? this.interactionAcknowledger;
    const accessDecision = this.interactionAccessDecision(interaction);
    if (!accessDecision.allowed) {
      const locale = this.resolveResponseLocale(interaction).locale;
      await acknowledger.defer(interaction, { ephemeral: true });
      await this.editInteraction(
        interaction,
        textMessage(this.accessBlockedText(accessDecision, locale)),
      );
      return true;
    }
    await acknowledger.defer(interaction, {
      ephemeral: command.kind === "management",
    });

    let rawOptions;
    let localeResolution = this.resolveLocale(interaction);
    try {
      const explicitLocale = modalCommand
        ? readCommandModalLocale(interaction)
        : null;
      localeResolution = this.resolveLocale(interaction, explicitLocale);
      rawOptions = modalCommand
        ? readCommandModalOptions(interaction, modalCommand)
        : interaction.data.options ?? [];
    } catch (error) {
      await this.editInteraction(
        interaction,
        textMessage(validationErrorText(error, localeResolution.locale)),
      );
      return true;
    }
    const locale = localeResolution.locale;

    if (command.kind === "help") {
      try {
        const requestedName = readHelpArgument(rawOptions);
        await this.editInteraction(
          interaction,
          textMessage(formatSlashCommandHelp(requestedName, locale)),
        );
      } catch (error) {
        await this.editInteraction(
          interaction,
          textMessage(validationErrorText(error, locale)),
        );
      }
      return true;
    }

    if (command.kind === "render-file") {
      await this.handleRenderFileInteraction(
        interaction,
        rawOptions,
        locale,
      );
      return true;
    }

    if (command.kind === "render-file-message") {
      await this.handleRenderFileMessageInteraction(interaction, locale);
      return true;
    }

    if (command.kind === "management") {
      await this.handleManagementInteraction(
        interaction,
        command,
        rawOptions,
        locale,
      );
      return true;
    }

    let argumentSets;
    let automaticPcTargets = false;
    let executionCommand = command;
    let executionOptions = rawOptions;
    try {
      const invocation = resolveSlashCommandInvocation(command, rawOptions);
      executionCommand = invocation.command;
      executionOptions = invocation.rawOptions;
      const argumentPlan = buildSlashCommandArgumentPlan(
        executionCommand,
        executionOptions,
      );
      automaticPcTargets = argumentPlan.automaticPcTargets;
      argumentSets = argumentPlan.argumentSets.map((tokens) =>
        prepareClearraArguments(tokens, this.searchExecutionOptions()),
      );
    } catch (error) {
      await this.editInteraction(
        interaction,
        textMessage(validationErrorText(error, locale)),
      );
      return true;
    }
    const previewDocument = safeSearchPreviewDocument(executionCommand, executionOptions);
    if (argumentSets.length === 1 && !automaticPcTargets) {
      await this.runInteractionCommand(
        interaction,
        argumentSets[0],
        previewDocument,
        locale,
      );
    } else {
      await this.runInteractionCommandSeries(
        interaction,
        argumentSets,
        previewDocument,
        locale,
      );
    }
    return true;
  }

  async handleRenderFileInteraction(interaction, rawOptions, locale) {
    let image;
    try {
      image = readRenderFileImage(rawOptions);
    } catch (error) {
      await this.editInteraction(
        interaction,
        textMessage(validationErrorText(error, locale)),
      );
      return;
    }
    await this.editInteraction(
      interaction,
      await this.buildRenderFileMessage({
        channelId: interaction.channel_id,
        guildId: interaction.guild_id ?? null,
        requesterId: interaction.member?.user?.id ?? interaction.user?.id,
        image,
      }, locale),
    );
  }

  async handleRenderFileMessageInteraction(interaction, locale) {
    let image;
    try {
      image = readRenderFileTargetMessageId(interaction);
    } catch (error) {
      await this.editInteraction(
        interaction,
        textMessage(validationErrorText(error, locale)),
      );
      return;
    }
    await this.editInteraction(
      interaction,
      await this.buildRenderFileMessage({
        channelId: interaction.channel_id,
        guildId: interaction.guild_id ?? null,
        requesterId: interaction.member?.user?.id ?? interaction.user?.id,
        image,
      }, locale),
    );
  }

  async buildRenderFileMessage(request, locale = "en") {
    const maximum = this.renderFileMaxBytes();
    const source = request?.image ?? null;
    let candidates;
    let explicit = false;
    try {
      if (source) {
        explicit = true;
        const reference = resolveRenderMessageReference(source, {
          channelId: request.channelId,
          guildId: request.guildId,
        });
        const message = await this.rest.getChannelMessage(
          reference.channelId,
          reference.messageId,
        );
        const candidate = renderGifCandidate(message, {
          applicationId: this.applicationId ?? this.config.applicationId,
          botUserId: this.botUserId,
          channelId: reference.channelId,
          maxBytes: maximum,
        });
        candidates = candidate ? [candidate] : [];
      } else {
        const messages = await this.readRecentRenderMessages(request.channelId);
        candidates = prioritizeRenderGifCandidates(messages, {
          applicationId: this.applicationId ?? this.config.applicationId,
          botUserId: this.botUserId,
          callerId: request.requesterId,
          channelId: request.channelId,
          maxBytes: maximum,
        });
      }
    } catch (error) {
      if (/^\/render-file image/.test(String(error?.message ?? ""))) {
        return textMessage(validationErrorText(error, locale));
      }
      if (isUnavailableDiscordAttachmentError(error)) {
        return textMessage(t(locale, "render_file.unavailable"));
      }
      this.logOperationFailure(error);
      return textMessage(t(locale, "render_file.failed"));
    }

    if (candidates.length === 0) {
      return textMessage(t(
        locale,
        explicit ? "render_file.invalid_selection" : "render_file.not_found",
      ));
    }

    for (const candidate of candidates) {
      try {
        const current = await this.rest.getChannelMessage(
          candidate.channelId || request.channelId,
          candidate.messageId,
        );
        const refreshed = renderGifCandidate(current, {
          applicationId: this.applicationId ?? this.config.applicationId,
          botUserId: this.botUserId,
          channelId: candidate.channelId || request.channelId,
          maxBytes: maximum,
        });
        if (!refreshed) {
          if (explicit) return textMessage(t(locale, "render_file.invalid_selection"));
          continue;
        }
        const bytes = assertGifBytes(
          await this.rest.downloadAttachment(refreshed.attachment.url, maximum),
        );
        return fileComponentMessage({
          name: "clearra-render-original.gif",
          description: t(locale, "render_file.attachment_description"),
          contentType: "image/gif",
          bytes,
        });
      } catch (error) {
        if (!explicit && isUnavailableDiscordAttachmentError(error)) continue;
        if (!isUnavailableDiscordAttachmentError(error)) {
          this.logOperationFailure(error);
        }
        return textMessage(t(locale, "render_file.unavailable"));
      }
    }
    return textMessage(t(locale, "render_file.unavailable"));
  }

  async readRecentRenderMessages(channelId) {
    const messages = [];
    let before = null;
    for (let page = 0; page < RENDER_FILE_HISTORY_MAX_PAGES; page += 1) {
      const batch = await this.rest.getChannelMessages(channelId, {
        limit: RENDER_FILE_HISTORY_PAGE_SIZE,
        ...(before ? { before } : {}),
      });
      if (!Array.isArray(batch)) {
        throw new Error("Discord returned invalid channel history.");
      }
      messages.push(...batch);
      if (batch.length < RENDER_FILE_HISTORY_PAGE_SIZE) break;
      const lastId = batch.at(-1)?.id;
      if (typeof lastId !== "string" || lastId === before) break;
      before = lastId;
    }
    return messages;
  }

  renderFileMaxBytes() {
    const maximum = this.config.maxGifBytes ??
      this.config.oracleMaxGifBytes ??
      8 * 1024 * 1024;
    if (!Number.isSafeInteger(maximum) || maximum < 1) {
      throw new Error("The render-file size limit is invalid.");
    }
    return maximum;
  }

  async handleManagementInteraction(
    interaction,
    command,
    rawOptions,
    currentLocale,
  ) {
    try {
      const request = readDiscordManagementRequest(command.name, rawOptions);
      if (!interaction.guild_id) {
        await this.editInteraction(
          interaction,
          textMessage(t(currentLocale, "management.guild_only")),
        );
        return;
      }
      let allowed = canManageDiscordSettings(interaction, request.scope);
      if (!allowed) {
        try {
          allowed = await this.botAdministratorAuthority.allows(interaction);
        } catch {
          this.logger.warn(
            "ClearraBot administrator authority could not be checked.",
          );
        }
      }
      if (!allowed) {
        await this.editInteraction(
          interaction,
          textMessage(t(currentLocale, `management.permission.${request.scope}`)),
        );
        return;
      }
      await this.editInteraction(
        interaction,
        await this.applyManagementRequest(interaction, request, currentLocale),
      );
    } catch (error) {
      await this.editInteraction(
        interaction,
        textMessage(validationErrorText(error, currentLocale)),
      );
    }
  }

  async applyManagementRequest(target, request, currentLocale = "en") {
    if (request.action === "help") {
      return textMessage(formatTextManagementHelp(currentLocale));
    }
    const guildId = target.guild_id;
    const channelId = target.channel_id;
    if (request.action === "language-show") {
      const resolution = this.managementLocaleResolution(target, request.scope);
      return textMessage(t(resolution.locale, "language.current", {
        scope: t(resolution.locale, `language.scope.${request.scope}`),
        language: t(resolution.locale, `language.name.${resolution.locale}`),
        source: t(resolution.locale, `language.source.${resolution.source}`),
      }));
    }
    if (request.action === "language-set") {
      if (request.scope === "channel") {
        await this.localePreferences.setChannel(channelId, request.locale);
      } else {
        await this.localePreferences.setGuild(guildId, request.locale);
      }
      const locale = request.locale;
      return textMessage(t(locale, "language.updated", {
        scope: t(locale, `language.scope.${request.scope}`),
        language: t(locale, `language.name.${request.locale}`),
      }));
    }
    if (request.action === "language-reset") {
      if (request.scope === "channel") {
        await this.localePreferences.resetChannel(channelId);
      } else {
        await this.localePreferences.resetGuild(guildId);
      }
      const resolution = this.managementLocaleResolution(target, request.scope);
      return textMessage(t(resolution.locale, "language.reset", {
        scope: t(resolution.locale, `language.scope.${request.scope}`),
        language: t(resolution.locale, `language.name.${resolution.locale}`),
      }));
    }
    if (request.action === "disable") {
      await this.accessPreferences.disableChannel(channelId, guildId);
      return textMessage(t(currentLocale, "management.channel.disabled"));
    }
    if (request.action === "enable") {
      await this.accessPreferences.enableChannel(channelId, guildId);
      return textMessage(t(currentLocale, "management.channel.enabled"));
    }
    if (request.action === "pause") {
      await this.accessPreferences.pauseGuild(guildId);
      return textMessage(t(currentLocale, "management.guild.paused"));
    }
    if (request.action === "resume") {
      await this.accessPreferences.resumeGuild(guildId);
      return textMessage(t(currentLocale, "management.guild.resumed"));
    }
    throw new Error("The management action is invalid.");
  }

  managementLocaleResolution(target, scope) {
    return this.localePreferences.resolve({
      guildId: target.guild_id,
      channelId: scope === "channel" ? target.channel_id : null,
    });
  }

  async runInteractionCommandSeries(
    interaction,
    argumentSets,
    previewDocument = null,
    locale = "en",
  ) {
    const controller = new AbortController();
    const deadlineUnixMs = interactionDeadlineUnixMs(
      interaction,
      Math.min(
        ...argumentSets.map((arguments_) =>
          this.searchDeadlineDurationMs(arguments_)
        ),
      ),
    );
    let delivered = 0;
    const firstResult = promiseCapability();
    const preview = previewDocument
      ? this.buildOraclePreviewMessage(
          previewDocument,
          t(locale, "preview.searching"),
          locale,
        )
      : null;
    const firstDelivery = this.deliverInteractionResult(
      interaction,
      firstResult.promise,
      preview,
    ).then(() => {
      delivered = 1;
    });
    this.controllers.add(controller);
    try {
      await this.withSearchSlot(async () => {
        for (let index = 0; index < argumentSets.length; index += 1) {
          const arguments_ = argumentSets[index];
          const result = await this.executor.execute(arguments_, {
            signal: controller.signal,
            deadlineUnixMs,
            ...interactionJobOptions(interaction, index),
          });
          const message = labelResultMessage(
            resultMessage(result, tilingOnlyRequested(arguments_), {
              maxCtk3FileBytes: this.config.maxCtk3FileBytes,
              locale,
              resultKind: publicResultKind(arguments_),
            }),
            t(locale, "search.auto_target", {
              lines: pcLineLabel(arguments_),
            }),
          );
          if (index === 0) {
            firstResult.resolve(message);
            await firstDelivery;
          } else {
            await this.followupInteraction(interaction, message);
            delivered += 1;
          }
        }
      }, { deadlineUnixMs, signal: controller.signal });
    } catch (error) {
      const message = textMessage(this.operationFailureText(error, locale));
      if (delivered === 0) {
        firstResult.resolve(message);
        await firstDelivery;
      } else {
        await this.followupInteraction(interaction, message);
      }
    } finally {
      firstResult.resolve(textMessage(t(locale, "search.stopped")));
      this.controllers.delete(controller);
    }
  }

  async runInteractionCommand(
    interaction,
    arguments_,
    previewDocument = null,
    locale = "en",
  ) {
    const controller = new AbortController();
    const tilingOnly = tilingOnlyRequested(arguments_);
    const deadlineUnixMs = interactionDeadlineUnixMs(
      interaction,
      this.searchDeadlineDurationMs(arguments_),
    );
    this.controllers.add(controller);
    try {
      const result = this.withSearchSlot(
        () => this.executor.execute(arguments_, {
          signal: controller.signal,
          deadlineUnixMs,
          ...interactionJobOptions(interaction, 0),
        }),
        { deadlineUnixMs, signal: controller.signal },
      ).then(
        (value) => resultMessage(value, tilingOnly, {
          maxCtk3FileBytes: this.config.maxCtk3FileBytes,
          locale,
          resultKind: publicResultKind(arguments_),
        }),
        (error) => textMessage(this.operationFailureText(error, locale)),
      ).catch((error) => textMessage(this.operationFailureText(error, locale)));
      const preview = previewDocument
        ? this.buildOraclePreviewMessage(
            previewDocument,
            t(locale, "preview.searching"),
            locale,
          )
        : null;
      if (isSetupSearchArguments(arguments_) && !preview) {
        await this.deliverSetupInteractionResult(interaction, result, locale);
      } else {
        await this.deliverInteractionResult(
          interaction,
          result,
          preview,
        );
      }
    } finally {
      this.controllers.delete(controller);
    }
  }

  async deliverInteractionResult(interaction, resultPromise, previewPromise = null) {
    if (!previewPromise) {
      await this.editInteraction(interaction, await resultPromise);
      return;
    }
    const taggedResult = Promise.resolve(resultPromise).then((message) => ({
      kind: "result",
      message,
    }));
    const taggedPreview = Promise.resolve(previewPromise).then((message) => ({
      kind: "preview",
      message,
    }));
    const first = await Promise.race([taggedResult, taggedPreview]);
    if (first.kind === "preview") {
      const posted = await this.editInteraction(interaction, first.message);
      const result = (await taggedResult).message;
      await this.editInteraction(
        interaction,
        combinePreviewAndResult(first.message, result, posted?.attachments),
      );
      return;
    }
    const preview = (await taggedPreview).message;
    await this.editInteraction(
      interaction,
      combinePreviewAndResult(preview, first.message),
    );
  }

  async deliverSetupInteractionResult(interaction, resultPromise, locale = "en") {
    const result = Promise.resolve(resultPromise).then((message) => ({
      kind: "result",
      message,
    }));
    const progress = delayedValue(
      setupProgressDelayMs(interaction, this.setupProgressNoticeMs),
      { kind: "progress" },
    );
    const first = await Promise.race([result, progress.promise]);
    if (first.kind === "result") {
      progress.cancel();
      await this.editInteraction(interaction, first.message);
      return;
    }

    try {
      await this.editInteraction(
        interaction,
        textMessage(t(locale, "search.setup_long_running")),
      );
    } catch (error) {
      this.logger?.warn?.(
        `Clearra progress update failed: ${safeErrorDiagnostic(error)}`,
      );
    }
    await this.editInteraction(interaction, (await result).message);
  }

  async readAttachmentDocuments(attachments = []) {
    return this.readBoundedAttachmentDocuments(attachments);
  }

  async readBoundedAttachmentDocuments(attachments = [], options = {}) {
    const documents = [];
    for (const attachment of attachments ?? []) {
      if (documents.length >= (options.maxDocuments ?? 10)) break;
      if (!isCtk3File({
        name: attachment?.filename,
        type: attachment?.content_type,
      })) continue;
      const limit =
        options.maxBytes ??
        this.config.maxCtk3FileBytes ??
        24 * 1024 * 1024;
      if (Number(attachment.size) > limit) {
        throw new Error("The CTK3 attachment is too large.");
      }
      if (!attachment.url) throw new Error("The CTK3 attachment URL is missing.");
      const bytes = await this.rest.downloadAttachment(attachment.url, limit);
      documents.push(decodeViewerFile(bytes, {
        maxPages: options.maxPages,
        maxSourceChars: options.maxSourceChars,
      }));
    }
    return documents;
  }

  acceptsOracleMessage(message, context = {}) {
    if (!message?.id || !message.channel_id) return false;
    const selfMessage = Boolean(
      context.botUserId && message.author?.id === context.botUserId,
    );
    if (selfMessage) {
      const attachments = message.attachments ?? [];
      if (attachments.some((attachment) =>
        attachment?.filename === "clearra-input-preview.gif"
      )) return false;
      if (attachments.some((attachment) =>
        GENERATED_RESULT_FILE_PATTERN.test(String(attachment?.filename ?? ""))
      )) return false;
      if (
        INTEGRATED_PC_SERIES_RESULT_PATTERN.test(String(message.content ?? "")) ||
        attachments.some((attachment) =>
          INTEGRATED_PC_SERIES_FILE_PATTERN.test(String(attachment?.filename ?? ""))
        )
      ) {
        return false;
      }
      return Boolean(
        this.config.oracleRenderEnabled &&
        attachments.some((attachment) =>
          isCtk3File({
            name: attachment?.filename,
            type: attachment?.content_type,
          })
        ),
      );
    }
    const content = String(message.content ?? "");
    let commandCandidate = false;
    if (this.config.oracleTextEnabled) {
      const prefixes = this.config.oracleCommandPrefixes ?? ["$", ">"];
      for (const prefix of prefixes) {
        if (!content.trimStart().startsWith(prefix)) continue;
        try {
          commandCandidate =
            readTextManagementRequest(content, prefix) !== null ||
            parseClearraTextRequest(
              content,
              prefix,
              this.searchExecutionOptions(),
            ) !== null;
        } catch {
          commandCandidate = true;
        }
        break;
      }
    }
    const renderCandidate = Boolean(
      this.config.oracleRenderEnabled &&
      (
        VIEWER_CANDIDATE_PATTERN.test(content) ||
        isStandaloneRenderField(content, {
          maxRows: 24,
          maxSourceChars: this.config.oracleMaxInputChars ?? 2_000,
        }) ||
        (message.attachments ?? []).some((attachment) =>
          isCtk3File({
            name: attachment?.filename,
            type: attachment?.content_type,
          })
        )
      ),
    );
    return commandCandidate || renderCandidate;
  }

  async handleOracleMessage(message) {
    const accessDecision = message?.author?.bot === true
      ? { allowed: true, reason: null }
      : this.oracleAccessDecision(message);
    if (!accessDecision.allowed) return false;
    const prepared = await this.prepareOracleMessage(message);
    if (
      prepared.automaticPcTargets ||
      prepared.argumentSets?.length > 1
    ) {
      await this.runOracleMessageCommandSeries(
        message,
        prepared.argumentSets,
        prepared.previewDocument,
        prepared.locale,
      );
    } else if (prepared.arguments_) {
      await this.runOracleMessageCommand(
        message,
        prepared.arguments_,
        prepared.previewDocument,
        prepared.locale,
      );
    }
    return prepared.handled;
  }

  async beginOracleMessage(message) {
    return this.handleOracleMessage(message);
  }

  async prepareOracleMessage(message) {
    const content = String(message.content ?? "");
    const selfMessage = message.author?.bot === true;
    const locale = this.resolveLocale(message).locale;
    let arguments_ = null;
    let argumentSets = [];
    let textRequest = null;
    if (!selfMessage && this.config.oracleTextEnabled) {
      const prefixes = this.config.oracleCommandPrefixes ?? ["$", ">"];
      const managementCandidate = prefixes.some((prefix) =>
        isTextManagementCandidate(content, prefix)
      );
      if (
        managementCandidate &&
        !(await this.authorizeOracleManagementMessage(message))
      ) {
        return { handled: false, locale };
      }
      let managementRequest = null;
      try {
        for (const prefix of prefixes) {
          managementRequest = readTextManagementRequest(content, prefix);
          if (managementRequest) break;
          textRequest = parseClearraTextRequest(
            content,
            prefix,
            this.searchExecutionOptions(),
          );
          if (textRequest) {
            arguments_ = textRequest.arguments_;
            argumentSets = textRequest.argumentSets ??
              (arguments_ ? [arguments_] : []);
            break;
          }
        }
      } catch (error) {
        const independentResponse = prefixes.some((prefix) =>
          isRenderFileTextInvocation(content, prefix)
        );
        const response = textMessage(validationErrorText(error, locale));
        await this.rest.createChannelMessage(
          message.channel_id,
          independentResponse ? response : replyMessage(response, message.id),
        );
        return { handled: true, locale };
      }
      if (managementRequest) {
        if (!message.guild_id && managementRequest.action !== "help") {
          await this.rest.createChannelMessage(
            message.channel_id,
            replyMessage(
              textMessage(t(locale, "management.guild_only")),
              message.id,
            ),
          );
          return { handled: true, locale };
        }
        const response = await this.applyManagementRequest(
          message,
          managementRequest,
          locale,
        );
        await this.rest.createChannelMessage(
          message.channel_id,
          replyMessage(response, message.id),
        );
        return { handled: true, locale };
      }
    }

    if (textRequest?.command?.kind === "help") {
      await this.rest.createChannelMessage(
        message.channel_id,
        replyMessage(
          textMessage(formatSlashCommandHelp(textRequest.helpTarget, locale)),
          message.id,
        ),
      );
      return { handled: true, locale };
    }

    if (textRequest?.command?.kind === "render-file") {
      const supplied = readRenderFileImage(textRequest.rawOptions);
      const response = await this.buildRenderFileMessage({
        channelId: message.channel_id,
        guildId: message.guild_id ?? null,
        requesterId: message.author?.id,
        image: supplied ?? message.message_reference?.message_id ?? null,
      }, locale);
      await this.rest.createChannelMessage(
        message.channel_id,
        response,
      );
      return { handled: true, locale };
    }

    let documents = [];
    const inlineViewerCandidate = VIEWER_CANDIDATE_PATTERN.test(content);
    if (this.config.oracleRenderEnabled) {
      const limits = this.oracleViewerLimits();
      try {
        if (textRequest?.command?.kind !== "search") {
          const standalone = textRequest
            ? null
            : extractStandaloneRenderField(content, limits);
          documents = standalone
            ? [standalone]
            : extractViewerDocuments(content, limits);
          if (documents.length === 0) {
            documents = await this.readBoundedAttachmentDocuments(
              message.attachments,
              { ...limits, maxSourceChars: undefined },
            );
          }
        }
      } catch {
        await this.rest.createChannelMessage(
          message.channel_id,
          replyMessage(
            textMessage(
              t(locale, "preview.invalid_attachment"),
            ),
            message.id,
          ),
        );
        return { handled: true, locale };
      }
      documents = mergeViewerDocuments(documents).slice(0, 1);
      if (
        textRequest?.command?.kind !== "search" &&
        inlineViewerCandidate &&
        documents.length === 0
      ) {
        await this.rest.createChannelMessage(
          message.channel_id,
          replyMessage(
            textMessage(
              t(locale, "preview.invalid_input"),
            ),
            message.id,
          ),
        );
        return { handled: true, locale };
      }
    }
    if (argumentSets.length === 0 && documents.length === 0) {
      return { handled: false, locale };
    }

    if (argumentSets.length === 0) {
      if (documents.length > 0) {
        await this.createOraclePreview(
          message,
          documents[0],
          false,
          selfMessage,
          locale,
        );
      }
      return { handled: true, locale };
    }
    return {
      handled: true,
      arguments_: argumentSets.length === 1 ? argumentSets[0] : null,
      argumentSets,
      automaticPcTargets: textRequest?.automaticPcTargets === true,
      locale,
      previewDocument:
        (textRequest?.command
          ? safeSearchPreviewDocument(
              textRequest.command,
              textRequest.rawOptions,
            )
          : null) ??
        documents[0] ??
        null,
    };
  }

  oracleViewerLimits() {
    return {
      maxDocuments: 1,
      maxPages: this.config.oracleMaxPages ?? 128,
      maxSourceChars: this.config.oracleMaxInputChars ?? 2_000,
      maxBytes: this.config.oracleMaxCtk3FileBytes ?? 8 * 1024 * 1024,
    };
  }

  async createOraclePreview(
    message,
    document,
    searching,
    resultPreview,
    locale = "en",
  ) {
    const status = searching
      ? t(locale, "preview.searching")
      : resultPreview
        ? t(locale, "preview.result")
        : t(locale, "preview.document");
    const preview = await this.buildOraclePreviewMessage(document, status, locale);
    return this.rest.createChannelMessage(
      message.channel_id,
      replyMessage(preview, message.id),
    );
  }

  async buildOraclePreviewMessage(document, status, locale = "en") {
    let linkedStatus = status;
    let viewerLine = null;
    let renderFailure = null;
    if (this.config.viewerBaseUrl) {
      try {
        const viewerUrl = buildClearraViewerUrl(this.config.viewerBaseUrl, document);
        viewerLine = t(locale, "viewer.open", { url: viewerUrl });
        linkedStatus = `${status}\n${viewerLine}`;
      } catch {
        // The GIF remains useful if a deployment deliberately omits the viewer URL.
      }
    }
    let content = linkedStatus.length <= DISCORD_CONTENT_LIMIT
      ? linkedStatus
      : status;
    const files = [];
    try {
      files.push({
        name: "clearra-input-preview.gif",
        description: t(locale, "preview.attachment_description"),
        contentType: "image/gif",
        bytes: await this.gifRenderer.render(document.document, {
          maxBytes: this.config.oracleMaxGifBytes ?? 8 * 1024 * 1024,
          maxFrames: this.config.oracleMaxPages ?? 128,
        }),
      });
    } catch (error) {
      this.logRenderFailure(error);
      const detail = error instanceof GifRenderLimitError ||
          error?.name === "GifRenderLimitError"
        ? localizedRenderLimit(error, locale)
        : t(locale, "preview.image_failed");
      renderFailure = detail;
      content = `${content}\n${detail}`.slice(0, DISCORD_CONTENT_LIMIT);
    }
    return {
      ...attachmentMessage(content, files),
      viewerLine,
      renderFailure,
    };
  }

  async runOracleMessageCommand(
    message,
    arguments_,
    previewDocument,
    locale = "en",
  ) {
    const controller = new AbortController();
    const tilingOnly = tilingOnlyRequested(arguments_);
    const deadlineUnixMs = Date.now() + this.searchDeadlineDurationMs(arguments_);
    this.controllers.add(controller);
    try {
      const result = this.withSearchSlot(
        () => this.executor.execute(arguments_, {
          signal: controller.signal,
          deadlineUnixMs,
        }),
        { deadlineUnixMs, signal: controller.signal },
      ).then(
        (value) => resultMessage(value, tilingOnly, {
          maxCtk3FileBytes: this.config.maxCtk3FileBytes,
          locale,
          resultKind: publicResultKind(arguments_),
        }),
        (error) => textMessage(this.operationFailureText(error, locale)),
      ).catch((error) => textMessage(this.operationFailureText(error, locale)));
      const preview = previewDocument
        ? this.buildOraclePreviewMessage(
            previewDocument,
            t(locale, "preview.searching"),
            locale,
          )
        : null;
      if (isSetupSearchArguments(arguments_) && !preview) {
        await this.deliverSetupOracleMessageResult(message, result, locale);
      } else {
        await this.deliverOracleMessageResult(message, result, preview);
      }
    } finally {
      this.controllers.delete(controller);
    }
  }

  async runOracleMessageCommandSeries(
    message,
    argumentSets,
    previewDocument = null,
    locale = "en",
  ) {
    const controller = new AbortController();
    const deadlineUnixMs = Date.now() + Math.min(
      ...argumentSets.map((arguments_) =>
        this.searchDeadlineDurationMs(arguments_)
      ),
    );
    let delivered = 0;
    const firstResult = promiseCapability();
    const preview = previewDocument
      ? this.buildOraclePreviewMessage(
          previewDocument,
          t(locale, "preview.searching"),
          locale,
        )
      : null;
    const firstDelivery = this.deliverOracleMessageResult(
      message,
      firstResult.promise,
      preview,
    ).then(() => {
      delivered = 1;
    });
    this.controllers.add(controller);
    try {
      await this.withSearchSlot(async () => {
        for (let index = 0; index < argumentSets.length; index += 1) {
          const arguments_ = argumentSets[index];
          const result = await this.executor.execute(arguments_, {
            signal: controller.signal,
            deadlineUnixMs,
          });
          const outgoing = labelResultMessage(
            resultMessage(result, tilingOnlyRequested(arguments_), {
              maxCtk3FileBytes: this.config.maxCtk3FileBytes,
              locale,
              resultKind: publicResultKind(arguments_),
            }),
            t(locale, "search.auto_target", {
              lines: pcLineLabel(arguments_),
            }),
          );
          if (index === 0) {
            firstResult.resolve(outgoing);
            await firstDelivery;
          } else {
            await this.rest.createChannelMessage(
              message.channel_id,
              replyMessage(outgoing, message.id),
            );
            delivered += 1;
          }
        }
      }, { deadlineUnixMs, signal: controller.signal });
    } catch (error) {
      const outgoing = textMessage(this.operationFailureText(error, locale));
      if (delivered === 0) {
        firstResult.resolve(outgoing);
        await firstDelivery;
      } else {
        await this.rest.createChannelMessage(
          message.channel_id,
          replyMessage(outgoing, message.id),
        );
      }
    } finally {
      firstResult.resolve(
        textMessage(t(locale, "search.stopped")),
      );
      this.controllers.delete(controller);
    }
  }

  async deliverOracleMessageResult(message, resultPromise, previewPromise = null) {
    if (!previewPromise) {
      await this.rest.createChannelMessage(
        message.channel_id,
        replyMessage(await resultPromise, message.id),
      );
      return;
    }
    const taggedResult = Promise.resolve(resultPromise).then((outgoing) => ({
      kind: "result",
      outgoing,
    }));
    const taggedPreview = Promise.resolve(previewPromise).then((outgoing) => ({
      kind: "preview",
      outgoing,
    }));
    const first = await Promise.race([taggedResult, taggedPreview]);
    if (first.kind === "preview") {
      const posted = await this.rest.createChannelMessage(
        message.channel_id,
        replyMessage(first.outgoing, message.id),
      );
      const result = (await taggedResult).outgoing;
      const finalMessage = combinePreviewAndResult(
        first.outgoing,
        result,
        posted?.attachments,
      );
      try {
        await this.rest.editChannelMessage(
          message.channel_id,
          posted.id,
          finalMessage,
        );
      } catch {
        await this.rest.createChannelMessage(
          message.channel_id,
          replyMessage(result, posted.id ?? message.id),
        );
      }
      return;
    }
    const preview = (await taggedPreview).outgoing;
    await this.rest.createChannelMessage(
      message.channel_id,
      replyMessage(combinePreviewAndResult(preview, first.outgoing), message.id),
    );
  }

  async deliverSetupOracleMessageResult(message, resultPromise, locale = "en") {
    const result = Promise.resolve(resultPromise).then((outgoing) => ({
      kind: "result",
      outgoing,
    }));
    const progress = delayedValue(
      this.setupProgressNoticeMs,
      { kind: "progress" },
    );
    const first = await Promise.race([result, progress.promise]);
    if (first.kind === "result") {
      progress.cancel();
      await this.rest.createChannelMessage(
        message.channel_id,
        replyMessage(first.outgoing, message.id),
      );
      return;
    }

    let posted = null;
    try {
      posted = await this.rest.createChannelMessage(
        message.channel_id,
        replyMessage(
          textMessage(t(locale, "search.setup_long_running")),
          message.id,
        ),
      );
    } catch (error) {
      this.logger?.warn?.(
        `Clearra progress update failed: ${safeErrorDiagnostic(error)}`,
      );
    }
    const final = (await result).outgoing;
    if (posted?.id) {
      try {
        await this.rest.editChannelMessage(
          message.channel_id,
          posted.id,
          final,
        );
        return;
      } catch {
        // Preserve the final result even if the progress message disappeared.
      }
    }
    await this.rest.createChannelMessage(
      message.channel_id,
      replyMessage(final, posted?.id ?? message.id),
    );
  }

  searchDeadlineDurationMs(arguments_) {
    return Math.min(
      this.interactionDeadlineMs,
      searchTimeoutMsForArguments(arguments_, this.config),
    );
  }

  async sendViewerReplies(send, documents, delayMs = 500, locale = "en") {
    for (let index = 0; index < Math.min(10, documents.length); index += 1) {
      const document = documents[index];
      try {
        const viewerUrl = buildClearraViewerUrl(this.config.viewerBaseUrl, document);
        const directContent = t(locale, "viewer.open", { url: viewerUrl });
        const files = [];
        let content = directContent;
        if (directContent.length > DISCORD_CONTENT_LIMIT) {
          const rendererUrl = buildClearraRendererUrl(this.config.viewerBaseUrl).href;
          content = t(locale, "viewer.link_too_long", { url: rendererUrl });
          files.push({
            name: `clearra-view-${index + 1}.ctk3`,
            description: t(locale, "viewer.document_description"),
            contentType: CTK3_FILE_MIME_TYPE,
            bytes: encodeCtk3File(document.document),
          });
        }

        try {
          const gif = await this.gifRenderer.render(document.document, {
            delayMs: Math.round(delayMs),
            maxBytes: this.config.maxGifBytes,
          });
          files.unshift(
            {
              name: `clearra-view-${index + 1}.gif`,
              description: t(locale, "preview.attachment_description"),
              contentType: "image/gif",
              bytes: gif,
            },
          );
        } catch (error) {
          this.logRenderFailure(error);
          const detail = error instanceof GifRenderLimitError ||
              error?.name === "GifRenderLimitError"
            ? localizedRenderLimit(error, locale)
            : t(locale, "preview.image_failed");
          content = `${content}\n${detail}`;
        }
        await send(attachmentMessage(content, files));
      } catch {
        await send(textMessage(t(locale, "viewer.document_failed")));
      }
    }
  }

  withSearchSlot(run, options = {}) {
    const deadlineUnixMs = Number(options.deadlineUnixMs);
    if (!Number.isSafeInteger(deadlineUnixMs) || deadlineUnixMs <= Date.now()) {
      return Promise.reject(interactionDeadlineError());
    }
    if (this.activeSearches < this.config.maxConcurrentSearches) {
      return this.useSearchSlot(run);
    }
    if (this.pendingSearches.length >= this.maxPendingSearches) {
      return Promise.reject(
        new Error("Clearra is busy. Please try the slash command again shortly."),
      );
    }
    return new Promise((resolve, reject) => {
      const pending = {
        run,
        resolve,
        reject,
        deadlineUnixMs,
        signal: options.signal,
        timeout: null,
        abort: null,
      };
      const removeAndReject = (error) => {
        const index = this.pendingSearches.indexOf(pending);
        if (index < 0) return;
        this.pendingSearches.splice(index, 1);
        this.cleanupPendingSearch(pending);
        reject(error);
      };
      pending.abort = () => removeAndReject(abortError("Clearra search was cancelled."));
      pending.timeout = setTimeout(
        () => removeAndReject(interactionDeadlineError()),
        Math.max(1, deadlineUnixMs - Date.now()),
      );
      this.pendingSearches.push(pending);
      options.signal?.addEventListener("abort", pending.abort, { once: true });
      if (options.signal?.aborted) {
        pending.abort();
      }
    });
  }

  searchExecutionOptions() {
    return {
      workers: this.config.searchWorkersPerSession,
      useAllLogicalProcessors: this.config.useAllLogicalProcessors,
      logicalProcessors: this.config.processLogicalProcessors,
      outputFormat: "json",
      includeSolutionData: true,
    };
  }

  async useSearchSlot(run) {
    this.activeSearches += 1;
    try {
      return await run();
    } finally {
      this.activeSearches -= 1;
      this.startNextSearch();
    }
  }

  startNextSearch() {
    while (this.pendingSearches.length > 0) {
      const next = this.pendingSearches.shift();
      this.cleanupPendingSearch(next);
      if (next.signal?.aborted) {
        next.reject(abortError("Clearra search was cancelled."));
        continue;
      }
      if (next.deadlineUnixMs <= Date.now()) {
        next.reject(interactionDeadlineError());
        continue;
      }
      this.useSearchSlot(next.run).then(next.resolve, next.reject);
      return;
    }
  }

  cleanupPendingSearch(pending) {
    if (pending.timeout) clearTimeout(pending.timeout);
    pending.signal?.removeEventListener("abort", pending.abort);
  }

  editInteraction(interaction, message) {
    return this.rest.editOriginalInteraction(
      interaction.application_id || this.applicationId,
      interaction.token,
      message,
    );
  }

  handleInteractionFailure(interaction, error) {
    const locale = this.resolveResponseLocale(interaction).locale;
    return this.editInteraction(
      interaction,
      textMessage(this.operationFailureText(error, locale)),
    );
  }

  operationFailureText(error, locale = "en") {
    this.logOperationFailure(error);
    return operationErrorText(error, locale);
  }

  logOperationFailure(error) {
    this.logger?.error?.(`Clearra request failed: ${safeErrorDiagnostic(error)}`);
  }

  logRenderFailure(error) {
    this.logger?.warn?.(`Clearra preview render failed: ${safeErrorDiagnostic(error)}`);
  }

  followupInteraction(interaction, message) {
    return this.rest.createInteractionFollowup(
      interaction.application_id || this.applicationId,
      interaction.token,
      message,
    );
  }
}

function interactionJobOptions(interaction, ordinal) {
  const interactionId = interaction?.id;
  if (!DISCORD_SNOWFLAKE_PATTERN.test(interactionId)) return {};
  return { jobId: `discord-interaction:${interactionId}:${ordinal}` };
}

function interactionDeadlineUnixMs(interaction, durationMs) {
  const interactionId = interaction?.id;
  if (!DISCORD_SNOWFLAKE_PATTERN.test(interactionId)) {
    return Date.now() + durationMs;
  }
  const createdAt = Number(
    (BigInt(interactionId) >> 22n) + DISCORD_EPOCH_MS,
  );
  return createdAt + durationMs;
}

function setupProgressDelayMs(interaction, durationMs) {
  return Math.max(
    0,
    interactionDeadlineUnixMs(interaction, durationMs) - Date.now(),
  );
}

function delayedValue(delayMs, value) {
  let timeout = null;
  const promise = new Promise((resolve) => {
    timeout = setTimeout(resolve, Math.max(0, delayMs), value);
    timeout.unref?.();
  });
  return Object.freeze({
    promise,
    cancel() {
      if (timeout !== null) clearTimeout(timeout);
      timeout = null;
    },
  });
}

function resultMessage(result, tilingOnly = false, options = {}) {
  const locale = options.locale ?? "en";
  if (result.exitCode === 0 && result.stdout) {
    const parsed = parseStructuredResult(result.stdout);
    const structured = parsed
      ? {
          ...parsed,
          kind: requestedStructuredResultKind(parsed, options.resultKind),
        }
      : parsed;
    if (structured) {
      const ctk3 = buildCtk3Result(parsed);
      if (ctk3) return ctk3ResultMessage(structured, ctk3, tilingOnly, options);
      const empty = structuredCompleteness(structured.summary, structured.finesse_report);
      return textMessage(
        structuredResultSummary(
          structured,
          0,
          empty.complete,
          empty.warnings,
          tilingOnly,
          locale,
        ),
      );
    }
  }
  if (result.exitCode !== 0) {
    if (!isCliUserInputError(result)) {
      return textMessage(operationErrorText(new Error("search failed"), locale));
    }
    const message = t(locale, "error.validation");
    return textMessage(tilingOnly
      ? `${t(locale, "warning.tiling_only")}\n\n${message}`
      : message);
  }
  let output = result.stdout || t(locale, "search.no_text");
  if (looksLikeUnsafeCommandOutput(output)) {
    return textMessage(operationErrorText(new Error("search failed"), locale));
  }
  if (tilingOnly) output = `${t(locale, "warning.tiling_only")}\n\n${output}`;
  if (output.length <= RESULT_LIMIT) {
    return textMessage(fenced(output));
  }
  return attachmentMessage(
    tilingOnly
      ? `${t(locale, "warning.tiling_only")}\n\n${t(locale, "result.title")}`
      : t(locale, "result.title"),
    [
    {
      name: "clearra-result.txt",
      description: t(locale, "result.output_description"),
      contentType: "text/plain; charset=utf-8",
      bytes: new TextEncoder().encode(output),
    },
    ],
  );
}

function requestedStructuredResultKind(structured, fallback) {
  if (
    isPlainObject(structured?.finesse_report) &&
    (fallback === "finesse-search" || fallback === "finesse-score")
  ) return fallback;
  return publicStructuredResultKind(structured?.kind, fallback);
}

function publicResultKind(arguments_) {
  const namespace = String(arguments_?.[0] ?? "").toLowerCase();
  const command = String(arguments_?.[1] ?? "")
    .toLowerCase()
    .replaceAll("_", "-");
  if (namespace === "sfinder" && PUBLIC_RESULT_KINDS.has(command)) {
    return command;
  }
  if (namespace === "finesse" && ["search", "score"].includes(command)) {
    return `finesse-${command}`;
  }
  const direct = namespace.replaceAll("_", "-");
  return PUBLIC_RESULT_KINDS.has(direct) ? direct : "search";
}

function ctk3ResultMessage(structured, ctk3, tilingOnly, options) {
  const locale = options.locale ?? "en";
  const finesseCompleteness = structuredCompleteness(
    null,
    structured.finesse_report,
  );
  const bytes = new TextEncoder().encode(ctk3.source);
  const limit = options.maxCtk3FileBytes ?? 24 * 1024 * 1024;
  if (!Number.isSafeInteger(limit) || limit < 1) {
    throw new Error("The CTK3 result size limit is invalid.");
  }
  if (bytes.byteLength > limit) {
    throw new Error(
      `The complete CTK3 result is ${bytes.byteLength} bytes, exceeding the ${limit}-byte Discord limit. No pages were truncated.`,
    );
  }
  return attachmentMessage(
    structuredResultSummary(
      structured,
      ctk3.pageCount,
      ctk3.complete && finesseCompleteness.complete,
      [...ctk3.warnings, ...finesseCompleteness.warnings],
      tilingOnly,
      locale,
    ),
    [
      {
        name: `${safeResultKind(structured.kind)}-result.ctk3`,
        description: t(locale, "result.ctk3_description"),
        contentType: CTK3_FILE_MIME_TYPE,
        bytes,
      },
    ],
  );
}

function parseStructuredResult(source) {
  try {
    const value = JSON.parse(source);
    return value && typeof value === "object" && !Array.isArray(value)
      ? value
      : null;
  } catch {
    return null;
  }
}

function structuredResultSummary(
  structured,
  pageCount,
  complete,
  warnings,
  tilingOnly,
  locale = "en",
) {
  const lines = [];
  if (tilingOnly) lines.push(t(locale, "warning.tiling_only"), "");
  lines.push(
    t(locale, "result.completed", {
      kind: friendlyResultKind(structured.kind, locale),
      partial: complete ? "" : t(locale, "result.partial_suffix"),
    }),
    t(locale, "result.ctk3_pages", { count: pageCount }),
  );
  const summary = structured.summary;
  if (summary && typeof summary === "object" && !Array.isArray(summary)) {
    for (const key of RESULT_SUMMARY_FIELDS) {
      const value = summary[key];
      if (typeof value === "string" || typeof value === "number") {
        lines.push(`${t(locale, `summary.${key}`)}: ${summaryValue(key, value)}`);
      }
    }
  }
  lines.push(...finesseReportLines(structured.finesse_report, locale));
  const warningKinds = [...new Set(warnings.map(publicWarningKind))];
  const visibleWarnings = warningKinds.slice(0, 3);
  for (const warning of visibleWarnings) {
    lines.push(t(locale, "result.warning", {
      warning: t(locale, `warning.${warning}`),
    }));
  }
  if (warningKinds.length > visibleWarnings.length) {
    lines.push(t(locale, "result.additional_warnings", {
      count: warningKinds.length - visibleWarnings.length,
    }));
  }
  return lines.join("\n").slice(0, DISCORD_CONTENT_LIMIT);
}

function summaryValue(key, value) {
  if (!PROBABILITY_SUMMARY_FIELDS.has(key)) return String(value);
  const probability = Number(value);
  if (!Number.isFinite(probability) || probability < 0 || probability > 1) {
    return String(value);
  }
  return `${Number((probability * 100).toFixed(4))}%`;
}

function safeResultKind(value) {
  return typeof value === "string" && /^[a-z0-9_-]{1,64}$/.test(value)
    ? value
    : "search";
}

function publicStructuredResultKind(value, fallback = "search") {
  const normalized = typeof value === "string"
    ? value.toLowerCase().replaceAll("_", "-")
    : "";
  if (PUBLIC_RESULT_KINDS.has(normalized)) return normalized;
  return PUBLIC_RESULT_KINDS.has(fallback) ? fallback : "search";
}

function friendlyResultKind(value, locale) {
  const kind = safeResultKind(value);
  const normalizedKind = kind.replaceAll("_", "-");
  if (normalizedKind === "spin-structure") {
    return t(locale, "result.kind.spin_structure");
  }
  if (normalizedKind === "finesse-search" || normalizedKind === "finesse-score") {
    return t(locale, `result.kind.${normalizedKind.replace("-", "_")}`);
  }
  if (locale === "en") return kind;
  const translationKind =
    normalizedKind === "pc" || normalizedKind.includes("perfect-clear")
      ? "pc"
      : normalizedKind.includes("path")
        ? "path"
        : normalizedKind.includes("score-finder")
          ? "score"
          : normalizedKind.includes("setup")
            ? "setup"
            : normalizedKind.includes("build")
              ? "build"
              : normalizedKind.includes("cover") || normalizedKind.includes("percent") || normalizedKind.includes("chance")
                ? "cover"
                : normalizedKind.includes("damage") || normalizedKind.includes("best-save")
                  ? "damage"
                  : normalizedKind.includes("spin")
                    ? "spin"
                    : normalizedKind.includes("verify")
                      ? "verify"
                      : null;
  return t(locale, translationKind
    ? `result.kind.${translationKind}`
    : "result.generic_kind");
}

function structuredCompleteness(summary, finesseReport = null) {
  const warnings = [];
  let complete = true;
  if (summary && typeof summary === "object" && !Array.isArray(summary)) {
    for (const [key, value] of Object.entries(summary)) {
      if (/(?:^|_)complete$/.test(key) && value === false) {
        complete = false;
        warnings.push("incomplete");
      } else if (/(?:^|_)truncated$/.test(key) && value === true) {
        complete = false;
        warnings.push("truncated");
      }
    }
  }
  if (isPlainObject(finesseReport)) {
    if (finesseReport.complete === false) {
      complete = false;
      warnings.push("incomplete");
    }
    if (Array.isArray(finesseReport.policy_results) &&
      finesseReport.policy_results.some((result) => isPlainObject(result) && result.complete === false)) {
      complete = false;
      warnings.push("incomplete");
    }
  }
  return { complete, warnings };
}

function finesseReportLines(report, locale) {
  if (!isPlainObject(report)) return [];
  const lines = [];
  const exact = nonNegativeFiniteNumber(report.exact_total_inputs);
  if (exact !== null) {
    lines.push(`${t(locale, "finesse.exact_total_inputs")}: ${formatInputCount(exact, locale)}`);
  }
  if (!Array.isArray(report.policy_results)) return lines;

  const results = new Map();
  for (const result of report.policy_results) {
    if (!isPlainObject(result) || !["oracle", "visible-7"].includes(result.policy)) continue;
    if (!results.has(result.policy)) results.set(result.policy, result);
  }
  for (const policy of ["oracle", "visible-7"]) {
    const result = results.get(policy);
    if (!result) continue;
    const average = nonNegativeFiniteNumber(result.overall_average_inputs);
    if (average !== null) {
      lines.push(`${t(locale, `finesse.average.${policy.replace("-", "_")}`)}: ${formatInputCount(average, locale)}`);
    }
    const successProbability = unitProbability(result.successful_probability_mass);
    if (successProbability !== null) {
      lines.push(`${t(locale, "finesse.success_probability")}: ${Number((successProbability * 100).toFixed(4))}%`);
    }
    const successfulQueues = nonNegativeInteger(result.successful_unique_queue_count);
    const totalQueues = nonNegativeInteger(result.total_unique_queue_count);
    if (successfulQueues !== null && totalQueues !== null) {
      lines.push(`${t(locale, "finesse.successful_queues")}: ${successfulQueues}/${totalQueues}`);
    }
    if (policy !== "visible-7") continue;
    const oracleCovered = nonNegativeFiniteNumber(result.oracle_on_covered_average_inputs);
    if (oracleCovered !== null) {
      lines.push(`${t(locale, "finesse.oracle_on_covered_average")}: ${formatInputCount(oracleCovered, locale)}`);
    }
    const penalty = nonNegativeFiniteNumber(result.information_penalty_inputs);
    if (penalty !== null) {
      lines.push(`${t(locale, "finesse.information_penalty")}: ${formatInputCount(penalty, locale)}`);
    }
    const probabilityGap = unitProbability(result.success_probability_gap);
    if (probabilityGap !== null) {
      lines.push(`${t(locale, "finesse.success_probability_gap")}: ${Number((probabilityGap * 100).toFixed(4))}%`);
    }
  }
  return lines;
}

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function nonNegativeFiniteNumber(value) {
  const number = typeof value === "number"
    ? value
    : typeof value === "string" && value.length <= 64 && /^(?:0|[1-9]\d*)(?:\.\d+)?$/.test(value)
      ? Number(value)
      : null;
  return number !== null && Number.isFinite(number) && number >= 0 ? number : null;
}

function nonNegativeInteger(value) {
  const number = nonNegativeFiniteNumber(value);
  return number !== null && Number.isSafeInteger(number) ? number : null;
}

function unitProbability(value) {
  const number = nonNegativeFiniteNumber(value);
  return number !== null && number <= 1 ? number : null;
}

function formatInputCount(value, locale) {
  const formatted = Number(value.toFixed(4));
  return t(locale, formatted === 1 ? "finesse.input" : "finesse.inputs", {
    count: formatted,
  });
}


const RESULT_SUMMARY_FIELDS = Object.freeze([
  "coverage_probability",
  "probability",
  "weighted_probability",
  "total_solution_count",
  "unique_solution_count",
  "normalized_unique_solution_count",
  "result_count",
  "regular_count",
  "mini_count",
  "minimum_placements",
  "maximum_damage",
]);

const PUBLIC_RESULT_KINDS = new Set([
  "search",
  "pc",
  "path",
  "percent",
  "chance",
  "minimals",
  "score",
  "score-minimals",
  "saves",
  "best-save",
  "cover",
  "setup",
  "congruent",
  "congruent-cover",
  "setup-cover",
  "cover-percent",
  "special-cover",
  "build-probability",
  "build-coverage",
  "spin-cover",
  "spin",
  "score-finder",
  "damage",
  "spin-structure",
  "pc-setup",
  "best-setup",
  "dpc-finder",
  "verify",
  "finesse-search",
  "finesse-score",
]);

const PROBABILITY_SUMMARY_FIELDS = new Set([
  "coverage_probability",
  "probability",
  "weighted_probability",
]);

function mergeViewerDocuments(...groups) {
  const unique = new Map();
  for (const document of groups.flat()) {
    const key = `${document.format}:${document.source}`;
    if (!unique.has(key)) unique.set(key, document);
  }
  return [...unique.values()];
}

function combinePreviewAndResult(preview, result, postedAttachments = []) {
  const retained = Array.isArray(postedAttachments)
    ? postedAttachments
        .filter((attachment) => attachment?.id && attachment?.filename)
        .map((attachment) => ({
          id: attachment.id,
          filename: attachment.filename,
        }))
    : [];
  const suffix = preview.viewerLine ?? preview.renderFailure ?? null;
  const resultContent = String(result.payload?.content ?? "");
  const content = suffix && resultContent.length + suffix.length + 1 <= DISCORD_CONTENT_LIMIT
    ? `${resultContent}\n${suffix}`
    : resultContent;
  const payload = { ...result.payload, content };
  if (retained.length > 0) payload.attachments = retained;
  return {
    payload,
    files: retained.length > 0
      ? [...result.files]
      : [...preview.files, ...result.files],
  };
}

function promiseCapability() {
  let resolve;
  const promise = new Promise((settle) => {
    resolve = settle;
  });
  return { promise, resolve };
}

function isRenderFileTextInvocation(content, prefix) {
  const source = String(content ?? "").trimStart();
  if (!source.startsWith(prefix)) return false;
  const command = source.slice(prefix.length).trimStart().split(/\s+/, 1)[0];
  return command?.toLowerCase().replaceAll("_", "-") === "render-file";
}

function replyMessage(message, messageId) {
  return {
    payload: {
      ...message.payload,
      allowed_mentions: {
        ...(message.payload.allowed_mentions ?? {}),
        parse: [],
        replied_user: false,
      },
      message_reference: {
        message_id: messageId,
        fail_if_not_exists: false,
      },
    },
    files: message.files,
  };
}

function fenced(value) {
  return `\`\`\`text\n${value.replaceAll("```", "'''")}\n\`\`\``;
}

function publicWarningKind(warning) {
  return /truncat|limit reached|omitted/i.test(String(warning ?? ""))
    ? "truncated"
    : "incomplete";
}

function isCliUserInputError(result) {
  const source = String(result?.stderr || result?.stdout || "")
    .replace(/\u001b\[[0-9;]*m/g, "")
    .trim();
  if (!source || source.length > RESULT_LIMIT) return false;
  if (!/\bE_[A-Z0-9_]{2,80}\b/.test(source)) return false;
  if (!/\bE_[A-Z0-9_]*(?:INVALID|MISSING|UNSUPPORTED|PARSE|FORMAT|VALUE|ARGUMENT|OPTION|FIELD|PATTERN)[A-Z0-9_]*\b/.test(source)) {
    return false;
  }
  return true;
}

function looksLikeUnsafeCommandOutput(source) {
  const output = String(source ?? "").trim();
  if (!output) return false;
  if ((output.startsWith("{") || output.startsWith("[")) && !parseStructuredResult(output)) {
    return true;
  }
  return containsPrivateOperationalDetail(output);
}

function containsPrivateOperationalDetail(message) {
  return /(?:oracle|cloud\s*run|gateway|job service|job id|job state|protocol|endpoint|authorization|token|worker|logical processor|process signal|exit code|runtime|vCPU|OCI|vault|engine|server|backend|tablebase|Web?GPU|WASM|spawn|EACCES|EPERM|ENOENT|node_modules|\bsyscall\b|\bat\s+file:|https?:\/\/|(?:^|\s)[A-Za-z]:[\\/]|\/(?:home|root|var|tmp|opt|workspace)\/)/i.test(message);
}

function safeSearchPreviewDocument(command, rawOptions) {
  try {
    return buildSearchPreviewDocument(command, rawOptions);
  } catch {
    // The authoritative field parser already accepted the search. Preview
    // postprocessing must never strand a slash or text command.
    return null;
  }
}

function localizedRenderLimit(_error, locale) {
  return t(locale, "preview.image_limit");
}

function safeErrorDiagnostic(error) {
  const name = safeDiagnosticPart(error?.name, "Error");
  const code = safeDiagnosticPart(error?.code, null);
  return code ? `${name} (${code})` : name;
}

function safeDiagnosticPart(value, fallback) {
  return typeof value === "string" && /^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(value)
    ? value
    : fallback;
}

function abortError(message) {
  const error = new Error(message);
  error.name = "AbortError";
  return error;
}

function interactionDeadlineError() {
  return new Error(
    "Clearra could not start the search before the Discord interaction deadline.",
  );
}

function pcLineLabel(arguments_) {
  const index = arguments_.indexOf("--lines");
  const lines = index >= 0 ? arguments_[index + 1] : "unknown";
  return `${lines}L`;
}

function labelResultMessage(message, label) {
  const content = `${label}\n${message.payload.content ?? ""}`.slice(0, DISCORD_CONTENT_LIMIT);
  return {
    payload: { ...message.payload, content },
    files: message.files.map((file) => ({
      ...file,
      name: file.name.replace(/^pc-result\./, `pc-${label.slice(-2).toLowerCase()}-result.`),
    })),
  };
}
