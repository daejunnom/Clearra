import { globalCommands } from "./discord/slash-command-catalog.mjs";
import { loadCommandRegistrationCredentials } from "./discord/command-registration.mjs";
import { DiscordRestClient } from "./discord/rest.mjs";

const { token, applicationId: configuredApplicationId } =
  loadCommandRegistrationCredentials();

const rest = new DiscordRestClient(token);
const applicationId =
  configuredApplicationId || (await rest.application()).id;
await rest.registerGlobalCommands(applicationId, globalCommands);
console.info(`Registered ${globalCommands.length} Clearra slash commands.`);
