import type { ClearraDesktopCliCommandRequest } from '../host/clearraDesktopHost.ts';

/**
 * Serialize canonical CLI arguments for the browser command-text transport.
 * Desktop callers keep the same arguments as an array, so values containing
 * whitespace are never reconstructed by splitting this display/transport text.
 */
export function serializeCliCommandArguments(arguments_: readonly string[]): string {
  return arguments_.map(quoteCliCommandToken).join(' ');
}

export function cliCommandRequestForDesktop(
  arguments_: readonly string[],
  language: 'en' | 'ko'
): ClearraDesktopCliCommandRequest {
  const argumentsCopy = arguments_.map(requireCliCommandToken);
  return {
    app_request_model: 'clearra-cli/CommandRequest',
    command: 'cli',
    language,
    arguments: argumentsCopy
  };
}

function quoteCliCommandToken(value: string): string {
  requireCliCommandToken(value);
  if (
    value !== '' &&
    !/[\s\u0085\u0001-\u001f\u007f|&;`<>"'\\]/u.test(value) &&
    !value.includes('$(')
  ) {
    return value;
  }
  return `"${value.replace(/\\/gu, '\\\\').replace(/"/gu, '\\"')}"`;
}

function requireCliCommandToken(value: string): string {
  if (value.includes('\0')) {
    throw new Error('CLI command values must not contain NUL');
  }
  return value;
}
