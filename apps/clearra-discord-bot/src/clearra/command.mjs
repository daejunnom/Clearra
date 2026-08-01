import { spawn } from "node:child_process";

const ALLOWED_COMMANDS = new Set(["pc", "setup", "path", "percent", "cover"]);
const PARALLEL_SEARCH_COMMANDS = new Set(["pc", "setup", "path"]);
const FILE_OPTIONS = new Set([
  "--fixture",
  "--input",
  "--output",
  "--wgsl",
  "--kick-profile-json",
]);
const CONTROLLED_OPTIONS = new Map([
  ["--tablebase", 0],
  ["--no-tablebase", 0],
  ["--build-dependency-dag", 0],
  ["--no-build-dependency-dag", 0],
  ["--workers", 1],
  ["--cpu-threads", 1],
  ["--use-all-cpu-threads", 0],
  ["--format", 1],
]);

export function parseClearraMessage(content, prefix = "!", execution = {}) {
  const trimmed = content.trim();
  if (!trimmed.startsWith(prefix)) return null;
  const body = trimmed.slice(prefix.length).trim();
  if (!body) return null;
  const tokens = tokenizeCommand(body);
  const first = tokens[0]?.toLowerCase();
  if (first === "clearra") tokens.shift();
  else if (!ALLOWED_COMMANDS.has(first)) return null;
  return prepareClearraArguments(tokens, execution);
}

export function prepareClearraArguments(tokens, execution = {}) {
  if (!Array.isArray(tokens) || tokens.length === 0) {
    throw new Error("Enter a Clearra command.");
  }
  if (tokens.length > 256) throw new Error("The command has too many arguments.");
  const command = tokens[0].toLowerCase();
  if (!ALLOWED_COMMANDS.has(command)) {
    throw new Error("Discord supports pc, setup, path, percent, and cover searches.");
  }

  const output = [command];
  for (let index = 1; index < tokens.length; index += 1) {
    const token = tokens[index];
    if (FILE_OPTIONS.has(token.toLowerCase())) {
      throw new Error("File and custom-code inputs are not available through Discord.");
    }
    const controlledWidth = CONTROLLED_OPTIONS.get(token.toLowerCase());
    if (controlledWidth !== undefined) {
      index += controlledWidth;
      continue;
    }
    output.push(token);
  }

  if (command === "pc") {
    output.push("--no-tablebase", "--no-build-dependency-dag");
  } else if (command === "setup") {
    output.push("--no-tablebase");
  }
  if (PARALLEL_SEARCH_COMMANDS.has(command) && execution.workers !== undefined) {
    const workers = Number(execution.workers);
    if (!Number.isSafeInteger(workers) || workers < 1) {
      throw new Error("Clearrabot received an invalid search worker allocation.");
    }
    output.push("--workers", String(workers));
    if (execution.useAllLogicalProcessors) {
      output.push("--use-all-cpu-threads");
    }
  }
  output.push("--format", "text");
  return output;
}

export function tilingOnlyRequested(arguments_) {
  if (!Array.isArray(arguments_)) return false;
  for (let index = 0; index < arguments_.length; index += 1) {
    const token = String(arguments_[index]).toLowerCase();
    if (token === "--tiling-only") return true;
    if (
      token === "--objective" &&
      String(arguments_[index + 1] ?? "").toLowerCase() === "tiling"
    ) {
      return true;
    }
  }
  return false;
}

export function tokenizeCommand(source) {
  if (source.length > 8192) throw new Error("The command is too long.");
  const tokens = [];
  let token = "";
  let quote = null;
  let escaped = false;

  for (const character of source) {
    if (escaped) {
      token += character;
      escaped = false;
      continue;
    }
    if (character === "\\") {
      escaped = true;
      continue;
    }
    if (quote) {
      if (character === quote) quote = null;
      else token += character;
      continue;
    }
    if (character === "'" || character === '"') {
      quote = character;
      continue;
    }
    if (/\s/.test(character)) {
      if (token) {
        tokens.push(token);
        token = "";
      }
      continue;
    }
    token += character;
  }
  if (escaped) token += "\\";
  if (quote) throw new Error("The command contains an unterminated quote.");
  if (token) tokens.push(token);
  return tokens;
}

export class ClearraProcessExecutor {
  constructor(options = {}) {
    this.executable = options.executable ?? "clearra";
    this.timeoutMs = options.timeoutMs ?? 3 * 60_000;
    this.maxOutputBytes = options.maxOutputBytes ?? 4 * 1024 * 1024;
  }

  execute(arguments_, options = {}) {
    return new Promise((resolve, reject) => {
      const child = spawn(this.executable, arguments_, {
        shell: false,
        windowsHide: true,
        stdio: ["ignore", "pipe", "pipe"],
      });
      let stdout = "";
      let stderr = "";
      let outputBytes = 0;
      let settled = false;

      const finish = (callback) => {
        if (settled) return;
        settled = true;
        clearTimeout(timeout);
        options.signal?.removeEventListener("abort", abort);
        callback();
      };
      const append = (current, chunk) => {
        outputBytes += chunk.length;
        if (outputBytes > this.maxOutputBytes) {
          child.kill("SIGKILL");
          finish(() => reject(new Error("Clearra produced too much Discord output.")));
          return current;
        }
        return current + chunk.toString("utf8");
      };
      const abort = () => {
        child.kill("SIGKILL");
        finish(() => reject(abortError("Clearra search was cancelled.")));
      };
      const timeout = setTimeout(() => {
        child.kill("SIGKILL");
        finish(() =>
          reject(
            new Error(
              `Clearrabot search exceeded the ${timeoutLabel(this.timeoutMs)} time limit.`,
            ),
          ),
        );
      }, this.timeoutMs);

      options.signal?.addEventListener("abort", abort, { once: true });
      if (options.signal?.aborted) {
        abort();
        return;
      }
      child.stdout.on("data", (chunk) => {
        stdout = append(stdout, chunk);
      });
      child.stderr.on("data", (chunk) => {
        stderr = append(stderr, chunk);
      });
      child.on("error", (error) => {
        finish(() => reject(new Error(`Clearra could not start: ${error.message}`)));
      });
      child.on("close", (code, signal) => {
        finish(() =>
          resolve({
            exitCode: code ?? -1,
            signal,
            stdout: stdout.trim(),
            stderr: stderr.trim(),
          }),
        );
      });
    });
  }
}

function timeoutLabel(milliseconds) {
  if (milliseconds % 60_000 === 0) return `${milliseconds / 60_000}-minute`;
  if (milliseconds % 1_000 === 0) return `${milliseconds / 1_000}-second`;
  return `${milliseconds}-millisecond`;
}

function abortError(message) {
  const error = new Error(message);
  error.name = "AbortError";
  return error;
}
