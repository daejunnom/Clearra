import { EventEmitter } from "node:events";

const GATEWAY_URL = "wss://gateway.discord.gg/?v=10&encoding=json";

export class DiscordGateway extends EventEmitter {
  constructor(token, options = {}) {
    super();
    this.token = token;
    this.intents = options.intents ?? 0;
    this.stopped = false;
    this.socket = null;
    this.sequence = null;
    this.sessionId = null;
    this.resumeUrl = null;
    this.heartbeatTimer = null;
    this.heartbeatAcknowledged = true;
  }

  async run() {
    let backoff = 500;
    while (!this.stopped) {
      try {
        await this.connectOnce();
        backoff = 500;
      } catch (error) {
        if (!this.stopped) this.emit("error", error);
      }
      if (!this.stopped) {
        await sleep(backoff);
        backoff = Math.min(30_000, backoff * 2);
      }
    }
  }

  stop() {
    this.stopped = true;
    this.clearHeartbeat();
    this.socket?.close(1000, "shutdown");
    this.socket = null;
  }

  connectOnce() {
    return new Promise((resolve, reject) => {
      if (typeof WebSocket !== "function") {
        reject(new Error("This Node.js runtime does not provide WebSocket."));
        return;
      }
      const socket = new WebSocket(this.resumeUrl || GATEWAY_URL);
      this.socket = socket;
      let connected = false;
      let closed = false;

      const finish = (error) => {
        if (closed) return;
        closed = true;
        this.clearHeartbeat();
        if (this.socket === socket) this.socket = null;
        if (error) reject(error);
        else resolve();
      };

      socket.addEventListener("open", () => {
        connected = true;
      });
      socket.addEventListener("message", (event) => {
        try {
          this.handlePayload(socket, JSON.parse(String(event.data)));
        } catch (error) {
          socket.close(4002, "decode error");
          finish(error);
        }
      });
      socket.addEventListener("error", () => {
        finish(new Error("Discord Gateway connection failed."));
      });
      socket.addEventListener("close", (event) => {
        if (event.code === 4004 || event.code === 4014) {
          this.stopped = true;
          finish(new Error(`Discord Gateway rejected the bot (${event.code}).`));
          return;
        }
        if (event.code === 4007 || event.code === 4009) this.resetSession();
        finish(connected ? null : new Error("Discord Gateway closed before connecting."));
      });
    });
  }

  handlePayload(socket, payload) {
    if (payload.s !== null && payload.s !== undefined) this.sequence = payload.s;
    switch (payload.op) {
      case 0:
        if (payload.t === "READY") {
          this.sessionId = payload.d.session_id;
          this.resumeUrl = `${payload.d.resume_gateway_url}/?v=10&encoding=json`;
        } else if (payload.t === "RESUMED") {
          this.heartbeatAcknowledged = true;
        }
        this.emit("dispatch", payload.t, payload.d);
        break;
      case 1:
        this.sendHeartbeat(socket);
        break;
      case 7:
        socket.close(4000, "reconnect");
        break;
      case 9:
        if (!payload.d) this.resetSession();
        setTimeout(() => socket.close(4000, "invalid session"), 1000);
        break;
      case 10:
        this.startHeartbeat(socket, payload.d.heartbeat_interval);
        if (this.sessionId && this.sequence !== null) this.resume(socket);
        else this.identify(socket);
        break;
      case 11:
        this.heartbeatAcknowledged = true;
        break;
      default:
        break;
    }
  }

  identify(socket) {
    socket.send(
      JSON.stringify({
        op: 2,
        d: {
          token: this.token,
          intents: this.intents,
          properties: {
            os: process.platform,
            browser: "clearra",
            device: "clearra",
          },
        },
      }),
    );
  }

  resume(socket) {
    socket.send(
      JSON.stringify({
        op: 6,
        d: {
          token: this.token,
          session_id: this.sessionId,
          seq: this.sequence,
        },
      }),
    );
  }

  startHeartbeat(socket, interval) {
    this.clearHeartbeat();
    const firstDelay = Math.floor(Math.random() * interval);
    this.heartbeatTimer = setTimeout(() => {
      this.sendHeartbeat(socket);
      this.heartbeatTimer = setInterval(() => this.sendHeartbeat(socket), interval);
    }, firstDelay);
  }

  sendHeartbeat(socket) {
    if (!this.heartbeatAcknowledged) {
      socket.close(4000, "heartbeat timeout");
      return;
    }
    this.heartbeatAcknowledged = false;
    socket.send(JSON.stringify({ op: 1, d: this.sequence }));
  }

  clearHeartbeat() {
    if (this.heartbeatTimer !== null) {
      clearTimeout(this.heartbeatTimer);
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  resetSession() {
    this.sessionId = null;
    this.resumeUrl = null;
    this.sequence = null;
  }
}

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}
