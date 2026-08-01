import { Clearrabot, globalCommands } from "./bot.mjs";
import { loadDiscordBotConfig } from "./config.mjs";
import { DiscordGateway } from "./discord/gateway.mjs";
import { DiscordRestClient } from "./discord/rest.mjs";

const config = loadDiscordBotConfig();
console.info(
  `Clearrabot PC/path/setup CPU allocation: ${config.searchWorkersPerSession} worker(s) per session; ` +
    `${config.processLogicalProcessors} logical processor(s) visible; ` +
    `${config.maxConcurrentSearches} concurrent session(s).`,
);
const rest = new DiscordRestClient(config.token);
const application = await rest.application();
const bot = new Clearrabot(rest, config, {
  applicationId: application.id,
});

if (config.registerCommands) {
  await rest.registerGlobalCommands(application.id, globalCommands);
}

const gateway = new DiscordGateway(config.token);
gateway.on("dispatch", (type, data) => {
  void bot.handleDispatch(type, data).catch((error) => {
    console.error(error instanceof Error ? error.message : error);
  });
});
gateway.on("error", (error) => {
  console.error(error instanceof Error ? error.message : error);
});

let stopping = false;
function stop() {
  if (stopping) return;
  stopping = true;
  bot.stop();
  gateway.stop();
}

process.once("SIGINT", stop);
process.once("SIGTERM", stop);
await gateway.run();
