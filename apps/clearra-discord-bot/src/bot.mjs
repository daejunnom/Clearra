import {
  ClearraJobExecutor,
  prepareClearraArguments,
  tilingOnlyRequested,
} from "./clearra/command.mjs";
import { ClearraDirectExecutor } from "./clearra/direct-executor.mjs";
import { buildCtk3Result } from "./clearra/ctk3-result.mjs";
import {
  CTK3_FILE_MIME_TYPE,
  encodeCtk3File,
  isCtk3File,
} from "ctk3";
import {
  attachmentMessage,
  textMessage,
} from "./discord/rest.mjs";
import { RestInteractionAcknowledger } from "./discord/interaction-acknowledger.mjs";
import {
  findSlashCommand,
  formatSlashCommandHelp,
  globalCommands,
} from "./discord/slash-command-catalog.mjs";
import {
  buildSlashCommandArguments,
  readHelpArgument,
} from "./discord/slash-command-input.mjs";
import {
  decodeViewerFile,
  extractViewerDocuments,
} from "./viewer/document.mjs";
import { GifRenderLimitError, renderDocumentGif } from "./viewer/gif.mjs";
import {
  buildClearraRendererUrl,
  buildClearraViewerUrl,
} from "./viewer/link.mjs";

const RESULT_LIMIT = 1900;
const DISCORD_CONTENT_LIMIT = 2000;

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
            maxOutputBytes: config.maxOutputBytes,
            pollIntervalMs: config.jobPollIntervalMs,
            cancelTimeoutMs: config.jobCancelTimeoutMs,
          })
        : new ClearraDirectExecutor(config));
    this.applicationId = options.applicationId ?? null;
    this.interactionAcknowledger =
      options.interactionAcknowledger ?? new RestInteractionAcknowledger(rest);
    this.activeSearches = 0;
    this.pendingSearches = [];
    this.controllers = new Set();
    this.interactionDeadlineMs =
      config.interactionDeadlineMs ?? config.searchTimeoutMs ?? 3 * 60_000;
    this.maxPendingSearches = config.maxPendingSearches ?? 8;
  }

  setApplicationId(applicationId) {
    this.applicationId = applicationId;
  }

  stop() {
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
    if (
      interaction.type !== 2 ||
      interaction.data?.type !== 1 ||
      !interaction.data?.name
    ) return false;
    if (!this.applicationId) {
      this.applicationId = interaction.application_id;
    }
    const name = interaction.data.name;
    const command = findSlashCommand(name);
    if (!command) return false;
    const acknowledger =
      context.acknowledger ?? this.interactionAcknowledger;
    await acknowledger.defer(interaction);

    if (command.kind === "help") {
      try {
        const requestedName = readHelpArgument(interaction.data.options ?? []);
        await this.editInteraction(
          interaction,
          textMessage(formatSlashCommandHelp(requestedName)),
        );
      } catch (error) {
        await this.editInteraction(interaction, textMessage(errorText(error)));
      }
      return true;
    }

    let arguments_;
    try {
      const tokens = buildSlashCommandArguments(
        command,
        interaction.data.options ?? [],
      );
      arguments_ = prepareClearraArguments(
        tokens,
        this.searchExecutionOptions(),
      );
    } catch (error) {
      await this.editInteraction(interaction, textMessage(errorText(error)));
      return true;
    }
    await this.runInteractionCommand(interaction, arguments_);
    return true;
  }

  async runInteractionCommand(interaction, arguments_) {
    const controller = new AbortController();
    const tilingOnly = tilingOnlyRequested(arguments_);
    const deadlineUnixMs = Date.now() + this.interactionDeadlineMs;
    this.controllers.add(controller);
    try {
      const result = await this.withSearchSlot(
        () => this.executor.execute(arguments_, {
          signal: controller.signal,
          jobId: `discord-${interaction.id}`,
          deadlineUnixMs,
        }),
        { deadlineUnixMs, signal: controller.signal },
      );
      await this.editInteraction(
        interaction,
        resultMessage(result, tilingOnly, {
          maxCtk3FileBytes: this.config.maxCtk3FileBytes,
        }),
      );
    } catch (error) {
      await this.editInteraction(interaction, textMessage(errorText(error)));
    } finally {
      this.controllers.delete(controller);
    }
  }

  async readAttachmentDocuments(attachments = []) {
    const documents = [];
    for (const attachment of attachments ?? []) {
      if (!isCtk3File({
        name: attachment?.filename,
        type: attachment?.content_type,
      })) continue;
      const limit = this.config.maxCtk3FileBytes ?? 24 * 1024 * 1024;
      if (Number(attachment.size) > limit) {
        throw new Error("The CTK3 attachment is too large.");
      }
      if (!attachment.url) throw new Error("The CTK3 attachment URL is missing.");
      const bytes = await this.rest.downloadAttachment(attachment.url, limit);
      documents.push(decodeViewerFile(bytes));
    }
    return documents;
  }

  async sendViewerReplies(send, documents, delayMs = 500) {
    for (let index = 0; index < Math.min(10, documents.length); index += 1) {
      const document = documents[index];
      try {
        const viewerUrl = buildClearraViewerUrl(this.config.viewerBaseUrl, document);
        const directContent = `Open in Clearra: ${viewerUrl}`;
        const files = [];
        let content = directContent;
        if (directContent.length > DISCORD_CONTENT_LIMIT) {
          const rendererUrl = buildClearraRendererUrl(this.config.viewerBaseUrl).href;
          content =
            "The direct viewer link exceeds Discord's 2,000-character limit. " +
            `Open the Clearra CTK renderer and load the attached CTK3 document: ${rendererUrl}`;
          files.push({
            name: `clearra-view-${index + 1}.ctk3`,
            description: "CTK3 document for the Clearra renderer",
            contentType: CTK3_FILE_MIME_TYPE,
            bytes: encodeCtk3File(document.document),
          });
        }

        try {
          const gif = renderDocumentGif(document.document, {
            delayMs: Math.round(delayMs),
            maxBytes: this.config.maxGifBytes,
          });
          files.unshift(
            {
              name: `clearra-view-${index + 1}.gif`,
              description: "Clearrabot Fumen and CTK3 preview",
              contentType: "image/gif",
              bytes: gif,
            },
          );
        } catch (error) {
          const detail =
            error instanceof GifRenderLimitError
              ? error.message
              : "The GIF preview could not be rendered.";
          content = `${content}\n${detail}`;
        }
        await send(attachmentMessage(content, files));
      } catch (error) {
        await send(textMessage("The document could not be rendered."));
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
      this.applicationId || interaction.application_id,
      interaction.token,
      message,
    );
  }

  handleInteractionFailure(interaction, error) {
    return this.editInteraction(interaction, textMessage(errorText(error)));
  }

  followupInteraction(interaction, message) {
    return this.rest.createInteractionFollowup(
      this.applicationId || interaction.application_id,
      interaction.token,
      message,
    );
  }
}

function resultMessage(result, tilingOnly = false, options = {}) {
  if (result.exitCode === 0 && result.stdout) {
    const structured = parseStructuredResult(result.stdout);
    if (structured) {
      const ctk3 = buildCtk3Result(structured);
      if (ctk3) return ctk3ResultMessage(structured, ctk3, tilingOnly, options);
      const empty = structuredCompleteness(structured.summary);
      return textMessage(
        structuredResultSummary(
          structured,
          0,
          empty.complete,
          empty.warnings,
          tilingOnly,
        ),
      );
    }
  }
  let output =
    result.exitCode === 0
      ? result.stdout || "Clearra completed without text output."
      : result.stderr || result.stdout || `Clearra exited with code ${result.exitCode}.`;
  if (tilingOnly) output = `${TILING_ONLY_WARNING}\n\n${output}`;
  if (output.length <= RESULT_LIMIT) {
    return textMessage(fenced(output));
  }
  return attachmentMessage(
    tilingOnly ? `${TILING_ONLY_WARNING}\n\nClearra result:` : "Clearra result:",
    [
    {
      name: "clearra-result.txt",
      description: "Clearra command output",
      contentType: "text/plain; charset=utf-8",
      bytes: new TextEncoder().encode(output),
    },
    ],
  );
}

function ctk3ResultMessage(structured, ctk3, tilingOnly, options) {
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
      ctk3.complete,
      ctk3.warnings,
      tilingOnly,
    ),
    [
      {
        name: `${safeResultKind(structured.kind)}-result.ctk3`,
        description: "Complete color-preserving Clearra CTK3 result",
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
) {
  const lines = [];
  if (tilingOnly) lines.push(TILING_ONLY_WARNING, "");
  lines.push(
    `Clearra ${safeResultKind(structured.kind)} completed${complete ? "" : " with a partial result"}.`,
    `CTK3 pages: ${pageCount}`,
  );
  const summary = structured.summary;
  if (summary && typeof summary === "object" && !Array.isArray(summary)) {
    for (const [key, label] of RESULT_SUMMARY_FIELDS) {
      const value = summary[key];
      if (typeof value === "string" || typeof value === "number") {
        lines.push(`${label}: ${value}`);
      }
    }
  }
  const visibleWarnings = warnings.slice(0, 3);
  for (const warning of visibleWarnings) lines.push(`Warning: ${warning}`);
  if (warnings.length > visibleWarnings.length) {
    lines.push(`Warning: ${warnings.length - visibleWarnings.length} additional incomplete-result conditions.`);
  }
  return lines.join("\n").slice(0, DISCORD_CONTENT_LIMIT);
}

function safeResultKind(value) {
  return typeof value === "string" && /^[a-z0-9-]{1,64}$/.test(value)
    ? value
    : "search";
}

function structuredCompleteness(summary) {
  const warnings = [];
  let complete = true;
  if (!summary || typeof summary !== "object" || Array.isArray(summary)) {
    return { complete, warnings };
  }
  for (const [key, value] of Object.entries(summary)) {
    if (/(?:^|_)complete$/.test(key) && value === false) {
      complete = false;
      warnings.push(`Search summary reports ${key}=false.`);
    } else if (/(?:^|_)truncated$/.test(key) && value === true) {
      complete = false;
      warnings.push(`Search summary reports ${key}=true.`);
    }
  }
  return { complete, warnings };
}

const RESULT_SUMMARY_FIELDS = Object.freeze([
  ["coverage_probability", "Coverage probability"],
  ["probability", "Probability"],
  ["weighted_probability", "Weighted probability"],
  ["total_solution_count", "Solutions"],
  ["unique_solution_count", "Unique solutions"],
  ["normalized_unique_solution_count", "Normalized solutions"],
  ["result_count", "Results"],
  ["maximum_damage", "Maximum damage"],
]);

const TILING_ONLY_WARNING =
  "WARNING: BuildUp and probability are skipped. Results may include solutions that cannot be built.";

function mergeViewerDocuments(...groups) {
  const unique = new Map();
  for (const document of groups.flat()) {
    const key = `${document.format}:${document.source}`;
    if (!unique.has(key)) unique.set(key, document);
  }
  return [...unique.values()];
}

function fenced(value) {
  return `\`\`\`text\n${value.replaceAll("```", "'''")}\n\`\`\``;
}

function errorText(error) {
  const message = error instanceof Error ? error.message : String(error);
  return `Clearra could not complete the request: ${message}`.slice(0, RESULT_LIMIT);
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
