import {
  ClearraProcessExecutor,
  parseClearraMessage,
  prepareClearraArguments,
  tilingOnlyRequested,
  tokenizeCommand,
} from "./clearra/command.mjs";
import { encodeCtk3 } from "ctk3";
import {
  attachmentMessage,
  textMessage,
} from "./discord/rest.mjs";
import { extractViewerDocuments } from "./viewer/document.mjs";
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
      new ClearraProcessExecutor({
        executable: config.executable,
        timeoutMs: config.searchTimeoutMs,
      });
    this.applicationId = options.applicationId ?? null;
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

  async handleDispatch(type, data) {
    if (type === "MESSAGE_CREATE") await this.handleMessage(data);
    else if (type === "INTERACTION_CREATE") await this.handleInteraction(data);
  }

  async handleMessage(message) {
    if (message.author?.bot || !message.channel_id) return;
    let arguments_ = null;
    let commandError = null;
    try {
      arguments_ = parseClearraMessage(
        message.content || "",
        this.config.prefix,
        this.searchExecutionOptions(),
      );
    } catch (error) {
      commandError = error;
    }

    if (commandError) {
      await this.rest.createChannelMessage(
        message.channel_id,
        textMessage(errorText(commandError)),
      );
      return;
    }
    if (arguments_) {
      await this.runMessageCommand(message, arguments_);
      return;
    }

    const documents = extractViewerDocuments(message.content || "");
    if (documents.length > 0) {
      await this.sendViewerReplies(
        (reply) => this.rest.createChannelMessage(message.channel_id, reply),
        documents,
      );
    }
  }

  async handleInteraction(interaction) {
    if (interaction.type !== 2 || !interaction.data?.name) return;
    if (!this.applicationId) {
      this.applicationId = interaction.application_id;
    }
    const name = interaction.data.name;
    if (name !== "clearra" && name !== "view") return;
    await this.rest.deferInteraction(interaction);

    const options = Object.fromEntries(
      (interaction.data.options ?? []).map((option) => [option.name, option.value]),
    );
    if (name === "view") {
      const source = String(options.document ?? "");
      const documents = extractViewerDocuments(source);
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

    const commandText = String(options.command ?? "");
    let arguments_;
    try {
      const tokens = tokenizeCommand(commandText);
      if (tokens[0]?.toLowerCase() === "clearra") tokens.shift();
      arguments_ = prepareClearraArguments(tokens, this.searchExecutionOptions());
    } catch (error) {
      await this.editInteraction(interaction, textMessage(errorText(error)));
      return;
    }
    await this.runInteractionCommand(interaction, arguments_, commandText);
  }

  async runMessageCommand(message, arguments_) {
    const controller = new AbortController();
    const tilingOnly = tilingOnlyRequested(arguments_);
    this.controllers.add(controller);
    try {
      const result = await this.withSearchSlot(() =>
        this.executor.execute(arguments_, { signal: controller.signal }),
      );
      await this.rest.createChannelMessage(
        message.channel_id,
        resultMessage(result, tilingOnly),
      );
      const documents = extractViewerDocuments(message.content || "");
      await this.sendViewerReplies(
        (reply) => this.rest.createChannelMessage(message.channel_id, reply),
        documents,
      );
    } catch (error) {
      await this.rest.createChannelMessage(
        message.channel_id,
        textMessage(errorText(error)),
      );
    } finally {
      this.controllers.delete(controller);
    }
  }

  async runInteractionCommand(interaction, arguments_, source) {
    const controller = new AbortController();
    const tilingOnly = tilingOnlyRequested(arguments_);
    this.controllers.add(controller);
    try {
      const result = await this.withSearchSlot(() =>
        this.executor.execute(arguments_, { signal: controller.signal }),
      );
      await this.editInteraction(interaction, resultMessage(result, tilingOnly));
      const documents = extractViewerDocuments(source);
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

  async sendViewerReplies(send, documents, delayMs = 500) {
    for (let index = 0; index < Math.min(10, documents.length); index += 1) {
      const document = documents[index];
      try {
        const viewerUrl = buildClearraViewerUrl(this.config.viewerBaseUrl, document);
        const directContent = `Open in Clearra: ${viewerUrl}`;
        const files = [];
        let content = directContent;
        if (directContent.length > DISCORD_CONTENT_LIMIT) {
          const ctk3 = encodeCtk3(document.document);
          const rendererUrl = buildClearraRendererUrl(this.config.viewerBaseUrl).href;
          content =
            "The direct viewer link exceeds Discord's 2,000-character limit. " +
            `Open the Clearra CTK renderer and load the attached CTK3 document: ${rendererUrl}`;
          files.push({
            name: `clearra-view-${index + 1}.ctk3`,
            description: "CTK3 document for the Clearra renderer",
            contentType: "text/plain; charset=utf-8",
            bytes: new TextEncoder().encode(ctk3),
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
        required: true,
        max_length: 6000,
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
