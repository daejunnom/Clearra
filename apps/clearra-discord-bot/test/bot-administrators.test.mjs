import assert from "node:assert/strict";
import test from "node:test";

import {
  DiscordBotAdministratorAuthority,
  interactionUserId,
  resolveDiscordBotAdministratorIds,
} from "../src/discord/bot-administrators.mjs";

const APPLICATION_ID = "1533373054309371924";
const OWNER_ID = "123456789012345678";

test("Discord application ownership grants stable bot-administrator IDs", () => {
  assert.deepEqual(
    resolveDiscordBotAdministratorIds({
      owner: { id: "123456789012345678", username: "daejunnnn" },
      team: null,
    }, ["223456789012345678", "223456789012345678"]),
    ["223456789012345678", "123456789012345678"],
  );
  assert.deepEqual(
    resolveDiscordBotAdministratorIds({
      owner: { id: "323456789012345678" },
      team: { owner_user_id: "423456789012345678" },
    }),
    ["423456789012345678"],
  );
  assert.throws(
    () => resolveDiscordBotAdministratorIds(null, ["not-a-user-id"]),
    /administrator user ID.*snowflake/,
  );
});

test("Discord interaction actors are resolved by immutable user ID", () => {
  assert.equal(
    interactionUserId({ member: { user: { id: "123456789012345678" } } }),
    "123456789012345678",
  );
  assert.equal(
    interactionUserId({ user: { id: "223456789012345678" } }),
    "223456789012345678",
  );
  assert.equal(interactionUserId({ user: { username: "daejunnnn" } }), null);
});

test("configured bot administrators are authorized without an application lookup", async () => {
  let applicationRequests = 0;
  const authority = new DiscordBotAdministratorAuthority({
    async application() {
      applicationRequests += 1;
      throw new Error("application lookup must remain lazy");
    },
  }, [OWNER_ID]);

  assert.equal(await authority.allows({
    application_id: APPLICATION_ID,
    member: { user: { id: OWNER_ID } },
  }), true);
  assert.equal(applicationRequests, 0);
});

test("application-owner authority is fetched only on demand and cached", async () => {
  let applicationRequests = 0;
  const authority = new DiscordBotAdministratorAuthority({
    async application() {
      applicationRequests += 1;
      return {
        id: APPLICATION_ID,
        owner: { id: OWNER_ID },
        team: null,
      };
    },
  });
  const interaction = (userId, applicationId = APPLICATION_ID) => ({
    application_id: applicationId,
    member: { user: { id: userId } },
  });

  assert.equal(applicationRequests, 0);
  assert.equal(await authority.allows(interaction(OWNER_ID)), true);
  assert.equal(applicationRequests, 1);
  assert.equal(
    await authority.allows(interaction("223456789012345678")),
    false,
  );
  assert.equal(applicationRequests, 1);
  assert.equal(
    await authority.allows(interaction(OWNER_ID, "2533373054309371924")),
    false,
  );
  assert.equal(applicationRequests, 1);
});

test("failed application-owner lookups fail closed and can be retried", async () => {
  let applicationRequests = 0;
  const authority = new DiscordBotAdministratorAuthority({
    async application() {
      applicationRequests += 1;
      if (applicationRequests === 1) throw new Error("temporary REST failure");
      return {
        id: APPLICATION_ID,
        owner: { id: OWNER_ID },
        team: null,
      };
    },
  });
  const interaction = {
    application_id: APPLICATION_ID,
    member: { user: { id: OWNER_ID } },
  };

  await assert.rejects(authority.allows(interaction), /temporary REST failure/);
  assert.equal(await authority.allows(interaction), true);
  assert.equal(applicationRequests, 2);
});

test("application-owner authority expires and follows ownership transfers", async () => {
  let now = 1_000;
  let applicationRequests = 0;
  let ownerId = OWNER_ID;
  const authority = new DiscordBotAdministratorAuthority({
    async application() {
      applicationRequests += 1;
      return {
        id: APPLICATION_ID,
        owner: { id: ownerId },
        team: null,
      };
    },
  }, [], {
    now: () => now,
    applicationOwnerTtlMs: 100,
  });
  const interaction = (userId) => ({
    application_id: APPLICATION_ID,
    member: { user: { id: userId } },
  });

  assert.equal(await authority.allows(interaction(OWNER_ID)), true);
  ownerId = "223456789012345678";
  now = 1_099;
  assert.equal(await authority.allows(interaction(OWNER_ID)), true);
  assert.equal(applicationRequests, 1);

  now = 1_100;
  assert.equal(await authority.allows(interaction(OWNER_ID)), false);
  assert.equal(
    await authority.allows(interaction("223456789012345678")),
    true,
  );
  assert.equal(applicationRequests, 2);
});
