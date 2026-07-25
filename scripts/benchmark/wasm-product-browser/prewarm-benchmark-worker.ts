import {
  disposeDistributedWorkers,
  prewarmDistributedWorkers
} from '../../../apps/clearra-web/src/workers/DistributedWasmJobRunner';
import { loadClearraWasmModule } from '../../../apps/clearra-web/src/workers/clearraWasmRuntime';

type PrewarmRequest = {
  workerCount: number;
  gpuWarmup: boolean;
};

self.onmessage = (message: MessageEvent<PrewarmRequest>) => {
  void runPrewarmBenchmark(message.data.workerCount, message.data.gpuWarmup);
};

async function runPrewarmBenchmark(requestedWorkerCount: number, gpuWarmup: boolean) {
  const workerCount = Math.max(1, Math.floor(requestedWorkerCount));
  const started = performance.now();
  try {
    const wasm = await loadClearraWasmModule();
    const moduleLoaded = performance.now();
    const gpuStatus = gpuWarmup ? await wasm.prewarm_gpu(null) : 'skipped';
    const gpuWarmed = performance.now();
    await prewarmDistributedWorkers(workerCount, wasm.compiled_module());
    const verifiersWarmed = performance.now();
    self.postMessage({
      type: 'completed',
      workerCount,
      verifierCount: workerCount - 1,
      gpuStatus,
      timings: {
        moduleLoadMs: moduleLoaded - started,
        gpuWarmupMs: gpuWarmed - moduleLoaded,
        verifierWarmupMs: verifiersWarmed - gpuWarmed,
        totalMs: verifiersWarmed - started
      }
    });
  } catch (error) {
    self.postMessage({
      type: 'failed',
      workerCount,
      elapsedMs: performance.now() - started,
      message: error instanceof Error ? error.message : String(error)
    });
  } finally {
    disposeDistributedWorkers();
  }
}
