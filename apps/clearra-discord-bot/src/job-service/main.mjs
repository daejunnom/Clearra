import { loadClearraJobServiceConfig } from "./config.mjs";
import { ClearraCommandRunner } from "./runner.mjs";
import { ClearraJobService } from "./server.mjs";

const config = loadClearraJobServiceConfig();
const runner = new ClearraCommandRunner(config);
const service = new ClearraJobService(config, runner);
const address = await service.listen();
const port = typeof address === "object" && address ? address.port : config.port;
console.info(
  `Clearra job service listening on ${config.host}:${port}; ` +
    `${config.searchWorkersPerSession} worker(s) per job; ` +
    `${config.processLogicalProcessors} logical processor(s) visible; ` +
    `${config.maxConcurrentJobs} concurrent job(s).`,
);

let stopping = false;
async function stop() {
  if (stopping) return;
  stopping = true;
  try {
    await service.close();
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}

process.once("SIGINT", () => void stop());
process.once("SIGTERM", () => void stop());
