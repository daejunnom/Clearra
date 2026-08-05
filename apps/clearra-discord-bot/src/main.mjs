import { Clearrabot, globalCommands } from "./bot.mjs";
import { loadDiscordBotConfig } from "./config.mjs";
import { DiscordGateway } from "./discord/gateway.mjs";
import { synchronizeGlobalCommandRegistration } from "./discord/command-registration.mjs";
import { RestInteractionAcknowledger } from "./discord/interaction-acknowledger.mjs";
import { DiscordRestClient } from "./discord/rest.mjs";
import { DiscordLocalePreferences } from "./discord/locale-preferences.mjs";
import { DiscordAccessPreferences } from "./discord/access-preferences.mjs";
import {
  isOracleMessageDispatch,
  OracleMessageIngress,
  oracleGatewayIntents,
} from "./ingress/oracle-message-ingress.mjs";
import { SlashCommandIngress } from "./ingress/slash-command-ingress.mjs";

const RUNTIME_EXTENSION_SYMBOL = Symbol.for("clearra.runtime.extension");
const runtimeExtension = readRuntimeExtension();
const config = loadDiscordBotConfig();
const workerSummary =
  config.workerAuthority === "remote"
    ? `remote job service owns search workers; ${config.maxConcurrentSearches} remote job(s)`
    : `${
        config.searchWorkersPerSession === undefined
          ? "native runtime-selected worker(s)"
          : `${config.searchWorkersPerSession} worker(s)`
      } per session; ` +
      `${config.processLogicalProcessors} logical processor(s) visible; ` +
      `${config.maxConcurrentSearches} concurrent session(s)`;
const executionSummary = config.jobEndpoint
  ? "remote HTTP job execution"
  : "in-process Clearra execution";
console.info(
  `Clearrabot execution allocation: ${workerSummary}; ` +
    `Oracle Gateway slash-command ingress; ${executionSummary}.`,
);
const rest = new DiscordRestClient(config.token);
const localePreferences = await new DiscordLocalePreferences({
  defaultLocale: config.defaultLocale,
  storePath: config.localeStorePath,
}).load();
const accessPreferences = await new DiscordAccessPreferences({
  storePath: config.accessStorePath,
}).load();
const runtimeState = await callRuntimeExtension("initialize", {
  config,
  rest,
  localePreferences,
  accessPreferences,
});
let applicationId = config.applicationId;
if (config.registerCommands && !applicationId) {
  const application = await rest.application();
  applicationId = application.id;
}
const bot = new Clearrabot(rest, config, {
  applicationId,
  localePreferences,
  accessPreferences,
});
await callRuntimeExtension("attachBot", runtimeContext({ bot, applicationId }));
const interactionAcknowledger = new RestInteractionAcknowledger(rest);
let slashCommandIngress = new SlashCommandIngress(bot, {
  acknowledger: interactionAcknowledger,
  operationalScope: "gateway",
});
slashCommandIngress = attachedValue(
  await callRuntimeExtension(
    "attachSlashIngress",
    runtimeContext({ bot, ingress: slashCommandIngress, applicationId }),
  ),
  slashCommandIngress,
);
const oracleHandler = attachedValue(
  await callRuntimeExtension(
    "wrapOracleHandler",
    runtimeContext({ bot, handler: bot, applicationId }),
  ),
  bot,
);
let oracleMessageIngress = new OracleMessageIngress(
  oracleHandler,
  config,
  {
    fetchMessage: (channelId, messageId) =>
      rest.getChannelMessage(channelId, messageId),
  },
);
oracleMessageIngress = attachedValue(
  await callRuntimeExtension(
    "attachMessageIngress",
    runtimeContext({
      bot,
      handler: oracleHandler,
      ingress: oracleMessageIngress,
      applicationId,
    }),
  ),
  oracleMessageIngress,
);

if (config.registerCommands) {
  await synchronizeGlobalCommandRegistration(
    rest,
    applicationId,
    globalCommands,
  );
}

let gateway = null;
let closePromise = null;
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
  closePromise ??= closeRuntime();
  void closePromise.finally(finishShutdown);
}

process.once("SIGINT", stop);
process.once("SIGTERM", stop);

const extraGatewayIntents = gatewayIntentBits(
  await callRuntimeExtension(
    "gatewayIntents",
    runtimeContext({ bot, applicationId }),
  ),
);
gateway = new DiscordGateway(config.token, {
  intents: oracleGatewayIntents(config) | extraGatewayIntents,
});
gateway.on("dispatch", (type, data) => {
  void handleGatewayDispatch(type, data);
});
gateway.on("error", () => {
  console.error("Discord Gateway connection failed.");
});
await gateway.run();
stop();
await shutdown;

async function handleGatewayDispatch(type, data) {
  if (type === "READY") {
    const username = data?.user?.username || "Discord bot";
    applicationId ||= data?.application?.id ?? null;
    oracleMessageIngress.setBotUserId?.(data?.user?.id);
    bot.setBotUserId(data?.user?.id);
    await safelyCallRuntimeExtension(
      "onReady",
      runtimeContext({
        bot,
        gateway,
        data,
        applicationId,
      }),
    );
    const oracleFeatures = [
      config.oracleRenderEnabled ? "single-image rendering" : null,
      config.oracleTextEnabled ? "remote text-command proxying" : null,
    ].filter(Boolean);
    const oracleBoundary = config.oracleTextEnabled
      ? "automatic self results, explicit user invocations, and allow-listed text channels"
      : "automatic self results and explicit mention/DM invocations";
    console.info(
      oracleFeatures.length > 0
        ? `Oracle Gateway connected as ${username}; Gateway slash ingress and ${oracleFeatures.join(" and ")} enabled for ${oracleBoundary}.`
        : `Oracle Gateway connected as ${username}; Gateway slash ingress enabled; text and image message ingress remains disabled.`,
    );
  } else if (type === "RESUMED") {
    console.info(
      config.oracleRenderEnabled || config.oracleTextEnabled
        ? "Oracle Gateway session resumed; slash and configured message ingress restored."
        : "Oracle Gateway session resumed; slash ingress restored and message ingress remains disabled.",
    );
  }
  void safelyCallRuntimeExtension(
    "onDispatch",
    runtimeContext({
      bot,
      gateway,
      type,
      data,
      applicationId,
    }),
  );
  const ingress = isOracleMessageDispatch(type)
    ? oracleMessageIngress
    : slashCommandIngress;
  try {
    await ingress.acceptDispatch(type, data);
  } catch {
    console.error("Discord request handling failed.");
  }
}

async function closeRuntime() {
  await safelyCallRuntimeExtension(
    "close",
    runtimeContext({
      bot,
      gateway,
      slashCommandIngress,
      messageIngress: oracleMessageIngress,
      applicationId,
    }),
  );
}

function readRuntimeExtension() {
  const extension = globalThis[RUNTIME_EXTENSION_SYMBOL];
  if (extension === undefined || extension === null) return null;
  if (typeof extension !== "object" && typeof extension !== "function") {
    throw new TypeError("The Clearra runtime extension is invalid.");
  }
  return extension;
}

async function callRuntimeExtension(name, context) {
  if (!runtimeExtension) return undefined;
  const hook = runtimeExtension[name];
  if (hook === undefined || hook === null) return undefined;
  if (typeof hook !== "function") {
    throw new TypeError(`The Clearra runtime extension hook ${name} is invalid.`);
  }
  return hook.call(runtimeExtension, Object.freeze(context));
}

async function safelyCallRuntimeExtension(name, context) {
  try {
    return await callRuntimeExtension(name, context);
  } catch {
    console.warn(`Clearra runtime extension hook ${name} failed.`);
    return undefined;
  }
}

function runtimeContext(additional = {}) {
  return {
    config,
    rest,
    localePreferences,
    accessPreferences,
    state: runtimeState,
    ...additional,
  };
}

function attachedValue(value, fallback) {
  return value === undefined || value === null ? fallback : value;
}

function gatewayIntentBits(value) {
  if (value === undefined || value === null) return 0;
  if (!Number.isSafeInteger(value) || value < 0 || value > 0x7fffffff) {
    throw new TypeError("The Clearra runtime extension Gateway intents are invalid.");
  }
  return value;
}
