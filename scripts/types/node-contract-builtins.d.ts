declare module 'node:assert/strict' {
  type AssertionErrorMatcher =
    | Error
    | RegExp
    | Record<string, unknown>
    | ((error: unknown) => boolean);

  interface StrictAssert {
    equal(actual: unknown, expected: unknown, message?: string | Error): void;
    deepEqual(actual: unknown, expected: unknown, message?: string | Error): void;
    match(actual: string, expected: RegExp, message?: string | Error): void;
    doesNotMatch(actual: string, expected: RegExp, message?: string | Error): void;
    ok(value: unknown, message?: string | Error): asserts value;
    rejects(
      block: Promise<unknown> | (() => Promise<unknown>),
      error?: AssertionErrorMatcher,
      message?: string | Error
    ): Promise<void>;
    throws(
      block: () => unknown,
      error?: AssertionErrorMatcher,
      message?: string | Error
    ): void;
    doesNotThrow(
      block: () => unknown,
      error?: AssertionErrorMatcher | string,
      message?: string | Error
    ): void;
  }

  const assert: StrictAssert;
  export default assert;
}

declare module 'node:fs/promises' {
  export function readFile(path: string | URL, encoding: 'utf8'): Promise<string>;
}

declare module 'node:path' {
  export function resolve(...paths: string[]): string;
}

declare const process: {
  cwd(): string;
};
