// SRP: retain ordered terminal entries separately from their optional text
// presentation. Product result pages do not need a pretty-printed JSON copy.
import type { ClearraHostAppResponse } from './wasmCommandClient';

export type WasmTerminalResponseEntry = Readonly<{
  kind: 'response';
  /** Pending text owner only; successful formatting releases this reference. */
  response: ClearraHostAppResponse | null;
}>;

export type WasmTerminalLine = string | WasmTerminalResponseEntry;

// Pending entries share the immutable protocol response with the result store.
// After formatting, a history entry owns only its text. Weak keys also release
// either representation when the history is cleared.
const pendingResponses = new WeakMap<WasmTerminalResponseEntry, ClearraHostAppResponse>();
const formattedResponses = new WeakMap<WasmTerminalResponseEntry, string>();
const FORMATTING_FAILED = 'E_WASM_TERMINAL_FORMAT: Response text could not be formatted. Displaying the terminal again will retry; structured results and diagnostics are unchanged.';

export function deferWasmTerminalResponse(
  response: ClearraHostAppResponse
): WasmTerminalResponseEntry {
  const entry: WasmTerminalResponseEntry = {
    kind: 'response',
    get response() { return pendingResponses.get(this) ?? null; }
  };
  pendingResponses.set(entry, response);
  return entry;
}

export function formatWasmTerminalLine(line: WasmTerminalLine): string {
  if (typeof line === 'string') return line;
  let formatted = formattedResponses.get(line);
  if (formatted === undefined) {
    const response = pendingResponses.get(line);
    if (response === undefined) return FORMATTING_FAILED;
    try {
      formatted = JSON.stringify(response, null, 2);
      if (typeof formatted !== 'string') return FORMATTING_FAILED;
    } catch {
      // No cache or owner release on failure: a later display can retry, and
      // a presentation-only failure never changes execution/diagnostic state.
      return FORMATTING_FAILED;
    }
    formattedResponses.set(line, formatted);
    pendingResponses.delete(line);
  }
  return formatted;
}

export function formatWasmTerminalTranscript(lines: readonly WasmTerminalLine[]): string {
  return lines.map(formatWasmTerminalLine).join('\n');
}
