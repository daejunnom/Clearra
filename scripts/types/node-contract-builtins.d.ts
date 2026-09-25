declare module 'node:assert/strict' {
  type AssertionErrorMatcher =
    | Error
    | RegExp
    | Record<string, unknown>
    | ((error: unknown) => boolean);

  interface StrictAssert {
    equal(actual: unknown, expected: unknown, message?: string | Error): void;
    notEqual(actual: unknown, expected: unknown, message?: string | Error): void;
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

// Minimal, dependency-free declarations for the Node 22 clock APIs used by
// executable contracts. Keep this shim typed; it is not a wildcard module.
declare module 'node:test' {
  interface MockFunctionContext {
    restore(): void;
  }
  interface MockTimers {
    enable(options: { apis: Array<'setInterval' | 'setTimeout' | 'setImmediate' | 'Date'>; now?: number | Date }): void;
    tick(milliseconds: number): void;
    reset(): void;
  }
  interface MockTracker {
    readonly timers: MockTimers;
    method<T extends object, K extends keyof T>(
      object: T,
      methodName: K,
      implementation: T[K] extends (...args: infer A) => infer R ? (...args: A) => R : never
    ): T[K] & { readonly mock: MockFunctionContext };
  }
  export const mock: MockTracker;
}
