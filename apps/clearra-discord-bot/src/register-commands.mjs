import { globalCommands } from "./discord/slash-command-catalog.mjs";
import {
  loadCommandRegistrationCredentials,
  synchronizeGlobalCommandRegistration,
} from "./discord/command-registration.mjs";
import { DiscordRestClient } from "./discord/rest.mjs";

const { token, applicationId: configuredApplicationId } =
  loadCommandRegistrationCredentials();

const rest = new DiscordRestClient(token);
const applicationId = configuredApplicationId || (await rest.application()).id;
const synchronization = await synchronizeGlobalCommandRegistration(
  rest,
  applicationId,
  globalCommands,
);
console.info(
  `${synchronization.changed ? "Updated and verified" : "Verified unchanged"} ` +
    `${synchronization.count} global Clearra application commands: ` +
    synchronization.names.join(", "),
);
