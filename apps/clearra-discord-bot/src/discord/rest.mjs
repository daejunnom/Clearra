const API_ROOT = "https://discord.com/api/v10";
const DEFAULT_REQUEST_TIMEOUT_MS = 30_000;
const MAX_RATE_LIMIT_DELAY_MS = 30_000;

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
    return this.request("PUT", `/applications/${applicationId}/commands`, commands);
  }

  async deferInteraction(interaction) {
    return this.request(
      "POST",
      `/interactions/${interaction.id}/${interaction.token}/callback`,
      { type: 5 },
      [],
      false,
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
    );
  }

  async createChannelMessage(channelId, message) {
    return this.request(
      "POST",
      `/channels/${channelId}/messages`,
      message.payload,
      message.files,
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
      response = await this.fetch(parsed, {
        method: "GET",
        headers: { "user-agent": "Clearrabot/0.1" },
        redirect: "error",
        signal: AbortSignal.timeout(this.requestTimeoutMs),
      });
    } catch (error) {
      throw discordNetworkError(error);
    }
    if (!response.ok) {
      throw new Error(`Discord attachment ${response.status}.`);
    }
    const declaredLength = Number(response.headers.get("content-length"));
    if (Number.isFinite(declaredLength) && declaredLength > limit) {
      throw new Error("The CTK3 attachment is too large.");
    }
    if (!response.body) {
      const bytes = new Uint8Array(await response.arrayBuffer());
      if (bytes.byteLength > limit) throw new Error("The CTK3 attachment is too large.");
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
          throw new Error("The CTK3 attachment is too large.");
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

  async request(method, path, payload, files = [], authenticate = true) {
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
        const attachments = files.map((file, index) => ({
          id: index,
          filename: file.name,
          description: file.description,
        }));
        form.set(
          "payload_json",
          JSON.stringify({ ...payload, attachments }),
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
        response = await this.fetch(`${API_ROOT}${path}`, {
          method,
          headers,
          body,
          signal: AbortSignal.timeout(this.requestTimeoutMs),
        });
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
      if (response.status >= 500 && attempt < 3) {
        await sleep(250 * 2 ** attempt);
        attempt += 1;
        continue;
      }
      if (!response.ok) {
        const detail = (await response.text()).slice(0, 1000);
        throw new Error(`Discord API ${response.status}: ${detail}`);
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

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function positiveInteger(value, fallback) {
  if (value === undefined) return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("The Discord request timeout is invalid.");
  }
  return parsed;
}

function discordNetworkError(error) {
  if (
    error?.name === "TimeoutError" ||
    error?.name === "AbortError"
  ) {
    return new Error("Discord API request timed out.");
  }
  const detail = error instanceof Error && error.message
    ? `: ${error.message}`
    : "";
  return new Error(`Discord API request failed${detail}`);
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
