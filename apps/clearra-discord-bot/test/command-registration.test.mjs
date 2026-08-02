import assert from "node:assert/strict";
import test from "node:test";

import { loadCommandRegistrationCredentials } from "../src/discord/command-registration.mjs";

test("command registration explains the Windows masked-token helper", () => {
  assert.throws(
    () => loadCommandRegistrationCredentials({}),
    /register:commands:windows/,
  );
});

test("command registration rejects a directly assigned PowerShell SecureString", () => {
  assert.throws(
    () =>
      loadCommandRegistrationCredentials({
        DISCORD_TOKEN: "System.Security.SecureString",
      }),
    /SecureString object name/,
  );
});

test("command registration normalizes credentials before the Discord request", () => {
  assert.deepEqual(
    loadCommandRegistrationCredentials({
      DISCORD_TOKEN: "  test-token  ",
      DISCORD_APPLICATION_ID: " 1533373054309371924 ",
    }),
    {
      token: "test-token",
      applicationId: "1533373054309371924",
    },
  );
  assert.throws(
    () =>
      loadCommandRegistrationCredentials({
        DISCORD_TOKEN: "test-token",
        DISCORD_APPLICATION_ID: "not-an-application-id",
      }),
    /17-20 digit Discord application ID/,
  );
});
