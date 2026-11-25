import type { ClearraWasmModule } from './clearraWasmRuntime';

/** The same source instance can be local or owned by a dedicated compute worker.
 * Compiled code is immutable and shared once; mutable WASM handles never move. */
type SourceMethod<K extends keyof ClearraWasmModule> =
  NonNullable<ClearraWasmModule[K]> extends (...args: infer Args) => infer Result
    ? (...args: Args) => Result | Promise<Result>
    : never;

type SourceKey = Extract<keyof ClearraWasmModule,
  `distributed_${string}` | 'profile_start' | 'profile_finish'>;

export type DistributedSourceRuntime = {
  [K in keyof Pick<ClearraWasmModule, SourceKey>]: SourceMethod<K>
} & Pick<ClearraWasmModule, 'compiled_module'> & {
  /** Capture the mutable routing key in the same source turn as issuance. */
  takeExactTask?: (jobId: number) => DistributedSourceTask | Promise<DistributedSourceTask>;
};

export type DistributedSourceTask = { task: ArrayBuffer | null; key: ArrayBuffer | null };
