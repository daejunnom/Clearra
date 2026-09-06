import assert from 'node:assert/strict';
import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';
import { withHostExecutionTiming } from '../src/workers/HostExecutionProfile';

const failed = { event: 'failed', job_id: 7, diagnostics: { diagnostics: [] } } as unknown as ClearraWasmWorkerEvent;
assert.equal(withHostExecutionTiming(failed, null), failed);
const progress = { event: 'progress' } as ClearraWasmWorkerEvent;
assert.equal(withHostExecutionTiming(progress, { source_ms: 1 }), progress);
const first = withHostExecutionTiming(failed, { source_ms: 2 });
const second = withHostExecutionTiming(first, { product_prepare_ms: 3 }) as ClearraWasmWorkerEvent & { search_profile: any };
assert.deepEqual(second.search_profile.host_execution, { source_ms: 2, product_prepare_ms: 3 });
assert.equal(second.job_id, 7);
assert.equal((failed as any).search_profile, undefined);
const old = { ...failed, search_profile: { verifier_transport: { unchanged: true }, host_execution: { source_ms: 4 } } };
const retained = withHostExecutionTiming(old, { module_prepare_ms: 5 }) as any;
assert.equal(retained.search_profile.verifier_transport, old.search_profile.verifier_transport);
assert.deepEqual(retained.search_profile.host_execution, { source_ms: 4, module_prepare_ms: 5 });
