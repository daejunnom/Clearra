import { Clearrabot, globalCommands } from "./bot.mjs";
import { CloudRunDiscordInteractionAdapter } from "./cloud-run/http-adapter.mjs";
import { loadDiscordBotConfig } from "./config.mjs";
import { DiscordGateway } from "./discord/gateway.mjs";
import { RestInteractionAcknowledger } from "./discord/interaction-acknowledger.mjs";
import { DiscordRestClient } from "./discord/rest.mjs";
import { SlashCommandIngress } from "./ingress/slash-command-ingress.mjs";

const config = loadDiscordBotConfig();
console.info(
  `Clearrabot PC/path/setup CPU allocation: ${config.searchWorkersPerSession} worker(s) per session; ` +
    `${config.processLogicalProcessors} logical processor(s) visible; ` +
    `${config.maxConcurrentSearches} concurrent session(s); ` +
    `${config.ingressMode} slash-command ingress; HTTP job execution enabled.`,
);
const rest = new DiscordRestClient(config.token);
let applicationId = config.applicationId;
if (config.registerCommands && !applicationId) {
  applicationId = (await rest.application()).id;
}
const bot = new Clearrabot(rest, config, {
  applicationId,
});
const slashCommandIngress = new SlashCommandIngress(bot, {
  acknowledger: new RestInteractionAcknowledger(rest),
});

if (config.registerCommands) {
  await rest.registerGlobalCommands(applicationId, globalCommands);
}

let gateway = null;
let cloudRunAdapter = null;
let finishShutdown;
const shutdown = new Promise((resolve) => {
  finishShutdown = resolve;
});

let stopping = false;
function stop() {
  if (stopping) return;
  stopping = true;
  bot.stop();
  gateway?.stop();
  if (cloudRunAdapter) {
    void closeCloudRunAdapter(cloudRunAdapter).finally(finishShutdown);
  } else {
    finishShutdown();
  }
}

process.once("SIGINT", stop);
process.once("SIGTERM", stop);

if (config.ingressMode === "cloud-run") {
  cloudRunAdapter = new CloudRunDiscordInteractionAdapter({
    ingress: slashCommandIngress,
    publicKey: config.publicKey,
    host: config.listenHost,
    port: config.port,
    interactionPath: config.interactionPath,
    maxBodyBytes: config.maxInteractionBodyBytes,
  });
  const address = await cloudRunAdapter.listen();
  const port = typeof address === "object" && address ? address.port : config.port;
  console.info(
    `Clearrabot Cloud Run interaction endpoint listening on ` +
      `${config.listenHost}:${port}${config.interactionPath}.`,
  );
  await shutdown;
} else {
  gateway = new DiscordGateway(config.token, { intents: 0 });
  gateway.on("dispatch", (type, data) => {
    void slashCommandIngress.acceptDispatch(type, data).catch((error) => {
      console.error(error instanceof Error ? error.message : error);
    });
  });
  gateway.on("error", (error) => {
    console.error(error instanceof Error ? error.message : error);
  });
  await gateway.run();
}

async function closeCloudRunAdapter(adapter) {
  await adapter.close();
  await Promise.race([
    adapter.drain(),
    new Promise((resolve) => setTimeout(resolve, config.jobCancelTimeoutMs)),
  ]);
}
