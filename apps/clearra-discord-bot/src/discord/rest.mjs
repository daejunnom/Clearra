const API_ROOT = "https://discord.com/api/v10";
const DEFAULT_REQUEST_TIMEOUT_MS = 30_000;
const MAX_RATE_LIMIT_DELAY_MS = 30_000;
const DISCORD_SNOWFLAKE = /^\d{17,20}$/;

export class DiscordRestClient {
  constructor(token = null, fetchImplementation = fetch, options = {}) {
    this.token = token || null;
    this.fetch = fetchImplementation;
    this.requestTimeoutMs = positiveInteger(
      options.requestTimeoutMs,
      DEFAULT_REQUEST_TIMEOUT_MS,
    );
  }

  async application() {
    return this.request("GET", "/oauth2/applications/@me");
  }

  async registerGlobalCommands(applicationId, commands) {
    return this.request(
      "PUT",
      `/applications/${applicationId}/commands`,
      commands,
      [],
      true,
      { retryServerErrors: false },
    );
  }

  async getGlobalCommands(applicationId) {
    return this.request(
      "GET",
      `/applications/${applicationId}/commands?with_localizations=true`,
    );
  }

  async deferInteraction(interaction, options = {}) {
    const response = { type: 5 };
    if (options.ephemeral === true) {
      response.data = { flags: 1 << 6 };
    }
    return this.createInteractionResponse(interaction, response);
  }

  async createInteractionResponse(interaction, response) {
    return this.request(
      "POST",
      `/interactions/${interaction.id}/${interaction.token}/callback`,
      response,
      [],
      false,
      { retryServerErrors: false },
    );
  }

  async editOriginalInteraction(applicationId, interactionToken, message) {
    return this.request(
      "PATCH",
      `/webhooks/${applicationId}/${interactionToken}/messages/@original`,
      message.payload,
      message.files,
      false,
    );
  }

  async createInteractionFollowup(applicationId, interactionToken, message) {
    return this.request(
      "POST",
      `/webhooks/${applicationId}/${interactionToken}`,
      message.payload,
      message.files,
      false,
      { retryServerErrors: false },
    );
  }

  async createChannelMessage(channelId, message) {
    return this.request(
      "POST",
      `/channels/${channelId}/messages`,
      message.payload,
      message.files,
      true,
      { retryServerErrors: false },
    );
  }

  async editChannelMessage(channelId, messageId, message) {
    return this.request(
      "PATCH",
      `/channels/${channelId}/messages/${messageId}`,
      message.payload,
      message.files,
    );
  }

  async getChannelMessage(channelId, messageId) {
    const channel = discordSnowflake(channelId, "channel ID");
    const message = discordSnowflake(messageId, "message ID");
    return this.request("GET", `/channels/${channel}/messages/${message}`);
  }

  async getChannelMessages(channelId, options = {}) {
    const channel = discordSnowflake(channelId, "channel ID");
    const limit = options.limit ?? 100;
    if (!Number.isSafeInteger(limit) || limit < 1 || limit > 100) {
      throw new Error("Discord channel-message limit must be from 1 through 100.");
    }
    const parameters = new URLSearchParams({ limit: String(limit) });
    if (options.before !== undefined && options.before !== null) {
      parameters.set(
        "before",
        discordSnowflake(options.before, "before message ID"),
      );
    }
    return this.request(
      "GET",
      `/channels/${channel}/messages?${parameters.toString()}`,
    );
  }

  async downloadAttachment(url, maxBytes) {
    const parsed = discordAttachmentUrl(url);
    const limit = Number(maxBytes);
    if (!Number.isSafeInteger(limit) || limit < 1) {
      throw new RangeError("The Discord attachment size limit is invalid.");
    }
    let response;
    try {
      response = await fetchWithTimeout(
        this.fetch,
        parsed,
        {
          method: "GET",
          headers: { "user-agent": "Clearrabot/0.1" },
          redirect: "error",
        },
        this.requestTimeoutMs,
      );
    } catch (error) {
      throw discordNetworkError(error);
    }
    if (!response.ok) {
      const error = new Error(`Discord attachment ${response.status}.`);
      error.discordStatus = response.status;
      throw error;
    }
    const declaredLength = Number(response.headers.get("content-length"));
    if (Number.isFinite(declaredLength) && declaredLength > limit) {
      throw new Error("The Discord attachment is too large.");
    }
    if (!response.body) {
      const bytes = new Uint8Array(await response.arrayBuffer());
      if (bytes.byteLength > limit) throw new Error("The Discord attachment is too large.");
      return bytes;
    }

    const chunks = [];
    const reader = response.body.getReader();
    let length = 0;
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        length += value.byteLength;
        if (length > limit) {
          await reader.cancel();
          throw new Error("The Discord attachment is too large.");
        }
        chunks.push(value);
      }
    } finally {
      reader.releaseLock();
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    return bytes;
  }

  async request(
    method,
    path,
    payload,
    files = [],
    authenticate = true,
    options = {},
  ) {
    if (authenticate && !this.token) {
      throw new Error("DISCORD_TOKEN is required for this Discord API request.");
    }
    let attempt = 0;
    while (true) {
      const headers = new Headers({
        "user-agent": "Clearrabot/0.1",
      });
      if (authenticate) headers.set("authorization", `Bot ${this.token}`);
      let body;
      if (files.length > 0) {
        const form = new FormData();
        const retainedAttachments = Array.isArray(payload?.attachments)
          ? payload.attachments
          : [];
        const uploadedAttachments = files.map((file, index) => ({
          id: index,
          filename: file.name,
          description: file.description,
        }));
        form.set(
          "payload_json",
          JSON.stringify({
            ...payload,
            attachments: [...retainedAttachments, ...uploadedAttachments],
          }),
        );
        files.forEach((file, index) => {
          form.set(
            `files[${index}]`,
            new Blob([file.bytes], { type: file.contentType }),
            file.name,
          );
        });
        body = form;
      } else if (payload !== undefined) {
        headers.set("content-type", "application/json");
        body = JSON.stringify(payload);
      }

      let response;
      try {
        response = await fetchWithTimeout(
          this.fetch,
          `${API_ROOT}${path}`,
          { method, headers, body },
          this.requestTimeoutMs,
        );
      } catch (error) {
        throw discordNetworkError(error);
      }
      if (response.status === 429 && attempt < 4) {
        const rateLimit = await response.json();
        const delayMs = Math.ceil(Number(rateLimit.retry_after ?? 1) * 1000);
        if (!Number.isFinite(delayMs) || delayMs < 0 || delayMs > MAX_RATE_LIMIT_DELAY_MS) {
          throw new Error("Discord API returned an unsafe rate-limit delay.");
        }
        await sleep(delayMs);
        attempt += 1;
        continue;
      }
      if (
        response.status >= 500 &&
        attempt < 3 &&
        options.retryServerErrors !== false
      ) {
        await sleep(250 * 2 ** attempt);
        attempt += 1;
        continue;
      }
      if (!response.ok) {
        const detail = (await response.text()).slice(0, 1000);
        const error = new Error(`Discord API ${response.status}: ${detail}`);
        error.discordStatus = response.status;
        const discordCode = discordApiErrorCode(detail);
        if (discordCode !== null) error.discordCode = discordCode;
        error.discordAmbiguous = response.status >= 500;
        throw error;
      }
      if (response.status === 204) return null;
      return response.json();
    }
  }
}

export function textMessage(content) {
  return {
    payload: {
      content,
      allowed_mentions: { parse: [] },
    },
    files: [],
  };
}

export function attachmentMessage(content, files) {
  return {
    payload: {
      content,
      allowed_mentions: { parse: [] },
    },
    files,
  };
}

export function fileComponentMessage(file) {
  if (!file || typeof file.name !== "string" || !file.name) {
    throw new Error("A Discord file component requires a filename.");
  }
  return {
    payload: {
      flags: 1 << 15,
      allowed_mentions: { parse: [] },
      components: [{
        type: 13,
        file: { url: `attachment://${file.name}` },
      }],
    },
    files: [file],
  };
}

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function fetchWithTimeout(fetchImplementation, url, options, timeoutMs) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetchImplementation(url, {
      ...options,
      signal: controller.signal,
    });
  } finally {
    clearTimeout(timeout);
  }
}

function positiveInteger(value, fallback) {
  if (value === undefined) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("The Discord request timeout is invalid.");
  }
  return parsed;
}

function discordSnowflake(value, name) {
  if (typeof value !== "string" || !DISCORD_SNOWFLAKE.test(value)) {
    throw new Error(`Discord ${name} must be a 17-20 digit snowflake.`);
  }
  return value;
}

function discordNetworkError(error) {
  let output;
  if (
    error?.name === "TimeoutError" ||
    error?.name === "AbortError"
  ) {
    output = new Error("Discord API request timed out.");
  } else {
    const detail = error instanceof Error && error.message
      ? `: ${error.message}`
      : "";
    output = new Error(`Discord API request failed${detail}`);
  }
  output.discordAmbiguous = true;
  return output;
}

function discordApiErrorCode(detail) {
  try {
    const code = JSON.parse(detail)?.code;
    return Number.isSafeInteger(code) ? code : null;
  } catch {
    return null;
  }
}

function discordAttachmentUrl(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error("The Discord attachment URL is invalid.");
  }
  const allowedHosts = new Set([
    "cdn.discordapp.com",
    "media.discordapp.net",
  ]);
  if (url.protocol !== "https:" || !allowedHosts.has(url.hostname)) {
    throw new Error("The Discord attachment URL is not trusted.");
  }
  return url;
}
