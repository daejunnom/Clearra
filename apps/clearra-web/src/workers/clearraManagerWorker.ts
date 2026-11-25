import { installDedicatedDistributedManager } from './DedicatedDistributedManager';

// This realm loads no WASM instance. Its only work is scheduling, transport,
// cancellation and durable result ownership for the source compute worker.
installDedicatedDistributedManager(self as unknown as Parameters<typeof installDedicatedDistributedManager>[0]);
