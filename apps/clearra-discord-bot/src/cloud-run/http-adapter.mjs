import { createServer } from "node:http";

import { InlineDeferredInteractionAcknowledger } from "../discord/interaction-acknowledger.mjs";
import { DiscordInteractionSignatureVerifier } from "./discord-signature.mjs";

const DISCORD_PING = 1;
const EPHEMERAL_MESSAGE_FLAG = 1 << 6;

export class CloudRunDiscordInteractionAdapter {
  constructor(options) {
    this.ingress = options.ingress;
    this.host = options.host ?? "0.0.0.0";
    this.port = options.port ?? 8080;
    this.interactionPath = options.interactionPath ?? "/interactions";
    this.maxBodyBytes = options.maxBodyBytes ?? 1024 * 1024;
    this.logger = options.logger ?? console;
    this.verifier =
      options.verifier ??
      new DiscordInteractionSignatureVerifier(options.publicKey);
    this.acknowledger =
      options.acknowledger ?? new InlineDeferredInteractionAcknowledger();
    this.backgroundTasks = new Set();
    this.server = createServer((request, response) => {
      void this.handleRequest(request, response).catch((error) => {
        this.logger.error(error instanceof Error ? error.message : error);
        if (!response.headersSent) {
          respondJson(response, 500, { error: "interaction_adapter_failed" });
        } else if (!response.writableEnded) {
          response.destroy();
        }
      });
    });
    this.server.requestTimeout = 10_000;
    this.server.headersTimeout = 10_000;
  }

  listen() {
    return new Promise((resolve, reject) => {
      const onError = (error) => {
        this.server.off("listening", onListening);
        reject(error);
      };
      const onListening = () => {
        this.server.off("error", onError);
        const address = this.server.address();
        resolve(address);
      };
      this.server.once("error", onError);
      this.server.once("listening", onListening);
      this.server.listen(this.port, this.host);
    });
  }

  async close() {
    if (!this.server.listening) return;
    await new Promise((resolve, reject) => {
      this.server.close((error) => (error ? reject(error) : resolve()));
    });
  }

  async drain() {
    await Promise.allSettled([...this.backgroundTasks]);
  }

  async handleRequest(request, response) {
    const pathname = requestPathname(request.url);
    if (request.method === "GET" && pathname === "/healthz") {
      respondJson(response, 200, { status: "ok" });
      return;
    }
    if (request.method !== "POST" || pathname !== this.interactionPath) {
      respondJson(response, 404, { error: "not_found" });
      return;
    }

    let rawBody;
    try {
      rawBody = await readBoundedBody(request, this.maxBodyBytes);
    } catch (error) {
      if (error instanceof RequestBodyLimitError) {
        respondJson(response, 413, { error: "request_too_large" });
        return;
      }
      throw error;
    }

    const signature = singleHeader(request.headers["x-signature-ed25519"]);
    const timestamp = singleHeader(request.headers["x-signature-timestamp"]);
    if (!this.verifier.verify(rawBody, signature, timestamp)) {
      respondJson(response, 401, { error: "invalid_request_signature" });
      return;
    }

    let interaction;
    try {
      interaction = JSON.parse(rawBody.toString("utf8"));
    } catch {
      respondJson(response, 400, { error: "invalid_json" });
      return;
    }
    if (interaction.type === DISCORD_PING) {
      respondJson(response, 200, { type: DISCORD_PING });
      return;
    }
    if (!this.ingress.accepts(interaction)) {
      respondJson(response, 200, {
        type: 4,
        data: {
          content: "Only Clearrabot slash commands are enabled.",
          flags: EPHEMERAL_MESSAGE_FLAG,
          allowed_mentions: { parse: [] },
        },
      });
      return;
    }

    respondJson(response, 200, { type: 5 });
    this.startBackgroundTask(
      this.ingress.accept(interaction, { acknowledger: this.acknowledger }),
    );
  }

  startBackgroundTask(task) {
    const tracked = Promise.resolve(task)
      .catch((error) => {
        this.logger.error(error instanceof Error ? error.message : error);
      })
      .finally(() => this.backgroundTasks.delete(tracked));
    this.backgroundTasks.add(tracked);
  }
}

class RequestBodyLimitError extends Error {}

async function readBoundedBody(request, limit) {
  const chunks = [];
  let length = 0;
  for await (const chunk of request) {
    const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    length += bytes.byteLength;
    if (length > limit) throw new RequestBodyLimitError();
    chunks.push(bytes);
  }
  return Buffer.concat(chunks, length);
}

function singleHeader(value) {
  return Array.isArray(value) ? value[0] : value;
}

function requestPathname(value) {
  try {
    return new URL(value ?? "/", "http://localhost").pathname;
  } catch {
    return "/__invalid_request_path__";
  }
}

function respondJson(response, statusCode, payload) {
  const body = JSON.stringify(payload);
  response.writeHead(statusCode, {
    "content-type": "application/json; charset=utf-8",
    "content-length": Buffer.byteLength(body),
    "cache-control": "no-store",
  });
  response.end(body);
}
