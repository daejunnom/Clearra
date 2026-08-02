const DISCORD_SNOWFLAKE = /^\d{17,20}$/;

export function loadCommandRegistrationCredentials(environment = process.env) {
  const token = environment.DISCORD_TOKEN?.trim() ?? "";
  if (!token) {
    throw new Error(
      "DISCORD_TOKEN is required to register commands. Set it in the same " +
        "terminal process, or on Windows run the register:commands:windows " +
        "workspace script for a masked prompt.",
    );
  }
  if (token === "System.Security.SecureString") {
    throw new Error(
      "DISCORD_TOKEN contains a PowerShell SecureString object name, not the " +
        "Discord bot token. Use the register:commands:windows workspace script.",
    );
  }

  const applicationId = environment.DISCORD_APPLICATION_ID?.trim() || null;
  if (applicationId && !DISCORD_SNOWFLAKE.test(applicationId)) {
    throw new Error(
      "DISCORD_APPLICATION_ID must be the 17-20 digit Discord application ID.",
    );
  }
  return Object.freeze({ token, applicationId });
}
