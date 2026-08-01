import {
  ClearraJobExecutor,
  prepareClearraArguments,
  tilingOnlyRequested,
  tokenizeCommand,
} from "./clearra/command.mjs";
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

export class Clearrabot {
  constructor(rest, config, options = {}) {
    this.rest = rest;
    this.config = config;
    this.executor =
      options.executor ??
      new ClearraJobExecutor({
        endpoint: config.jobEndpoint,
        authorizationToken: config.jobToken,
        timeoutMs: config.searchTimeoutMs,
        pollIntervalMs: config.jobPollIntervalMs,
        cancelTimeoutMs: config.jobCancelTimeoutMs,
      });
    this.applicationId = options.applicationId ?? null;
    this.interactionAcknowledger =
      options.interactionAcknowledger ?? new RestInteractionAcknowledger(rest);
    this.activeSearches = 0;
    this.pendingSearches = [];
    this.controllers = new Set();
  }

  setApplicationId(applicationId) {
    this.applicationId = applicationId;
  }

  stop() {
    for (const controller of this.controllers) controller.abort();
    this.controllers.clear();
    for (const pending of this.pendingSearches.splice(0)) {
      pending.reject(abortError("Clearrabot stopped."));
    }
  }

  async handleDispatch(type, data, options = {}) {
    if (type !== "INTERACTION_CREATE") return false;
    await this.handleInteraction(data, options);
    return true;
  }

  async handleInteraction(interaction, context = {}) {
    if (
      interaction.type !== 2 ||
      interaction.data?.type !== 1 ||
      !interaction.data?.name
    ) return;
    if (!this.applicationId) {
      this.applicationId = interaction.application_id;
    }
    const name = interaction.data.name;
    if (name !== "clearra" && name !== "view") return;
    const acknowledger =
      context.acknowledger ?? this.interactionAcknowledger;
    await acknowledger.defer(interaction);

    const options = Object.fromEntries(
      (interaction.data.options ?? []).map((option) => [option.name, option.value]),
    );
    let attachmentDocuments;
    try {
      const attachment = options.file
        ? interaction.data.resolved?.attachments?.[String(options.file)]
        : null;
      attachmentDocuments = await this.readAttachmentDocuments(
        attachment ? [attachment] : [],
      );
    } catch (error) {
      await this.editInteraction(interaction, textMessage(errorText(error)));
      return;
    }
    if (name === "view") {
      const source = String(options.document ?? "");
      const documents = mergeViewerDocuments(
        extractViewerDocuments(source),
        attachmentDocuments,
      );
      await this.editInteraction(
        interaction,
        textMessage(
          documents.length > 0
            ? "Viewer generated."
            : "No valid Fumen or CTK3 document was found.",
        ),
      );
      if (documents.length > 0) {
        await this.sendViewerReplies(
          (reply) => this.followupInteraction(interaction, reply),
          documents,
          Number(options.duration ?? 0.5) * 1000,
        );
      }
      return;
    }

    const commandText = appendViewerSources(
      String(options.command ?? ""),
      attachmentDocuments,
    );
    let arguments_;
    try {
      const tokens = tokenizeCommand(commandText);
      if (tokens[0]?.toLowerCase() === "clearra") tokens.shift();
      arguments_ = prepareClearraArguments(tokens, this.searchExecutionOptions());
    } catch (error) {
      await this.editInteraction(interaction, textMessage(errorText(error)));
      return;
    }
    await this.runInteractionCommand(
      interaction,
      arguments_,
      commandText,
      attachmentDocuments,
    );
  }

  async runInteractionCommand(
    interaction,
    arguments_,
    source,
    attachmentDocuments = [],
  ) {
    const controller = new AbortController();
    const tilingOnly = tilingOnlyRequested(arguments_);
    this.controllers.add(controller);
    try {
      const result = await this.withSearchSlot(() =>
        this.executor.execute(arguments_, {
          signal: controller.signal,
          jobId: `discord-${interaction.id}`,
        }),
      );
      await this.editInteraction(interaction, resultMessage(result, tilingOnly));
      const documents = mergeViewerDocuments(
        extractViewerDocuments(source),
        attachmentDocuments,
      );
      await this.sendViewerReplies(
        (reply) => this.followupInteraction(interaction, reply),
        documents,
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

  withSearchSlot(run) {
    if (this.activeSearches < this.config.maxConcurrentSearches) {
      return this.useSearchSlot(run);
    }
    return new Promise((resolve, reject) => {
      this.pendingSearches.push({ run, resolve, reject });
    });
  }

  searchExecutionOptions() {
    return {
      workers: this.config.searchWorkersPerSession,
      useAllLogicalProcessors: this.config.useAllLogicalProcessors,
    };
  }

  async useSearchSlot(run) {
    this.activeSearches += 1;
    try {
      return await run();
    } finally {
      this.activeSearches -= 1;
      const next = this.pendingSearches.shift();
      if (next) this.useSearchSlot(next.run).then(next.resolve, next.reject);
    }
  }

  editInteraction(interaction, message) {
    return this.rest.editOriginalInteraction(
      this.applicationId || interaction.application_id,
      interaction.token,
      message,
    );
  }

  followupInteraction(interaction, message) {
    return this.rest.createInteractionFollowup(
      this.applicationId || interaction.application_id,
      interaction.token,
      message,
    );
  }
}

export const globalCommands = [
  {
    name: "clearra",
    description: "Run a Clearra search",
    options: [
      {
        type: 3,
        name: "command",
        description: "Clearra command after the executable name",
        required: true,
        max_length: 4096,
      },
      {
        type: 11,
        name: "file",
        description: "Optional CTK3 field document",
        required: false,
      },
    ],
  },
  {
    name: "view",
    description: "Render a Fumen or CTK3 document",
    options: [
      {
        type: 3,
        name: "document",
        description: "Fumen, CTK3, or Clearra viewer URL",
        required: false,
        max_length: 6000,
      },
      {
        type: 11,
        name: "file",
        description: "CTK3 document file",
        required: false,
      },
      {
        type: 10,
        name: "duration",
        description: "Seconds per GIF frame",
        required: false,
        min_value: 0.02,
        max_value: 60,
      },
    ],
  },
];

function resultMessage(result, tilingOnly = false) {
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

const TILING_ONLY_WARNING =
  "WARNING: BuildUp and probability are skipped. Results may include solutions that cannot be built.";

function appendViewerSources(source, documents) {
  const additions = documents.map((document) => document.source);
  return [source.trim(), ...additions].filter(Boolean).join(" ");
}

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
