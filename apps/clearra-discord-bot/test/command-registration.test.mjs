import assert from "node:assert/strict";
import test from "node:test";

import {
  loadCommandRegistrationCredentials,
  synchronizeGlobalCommandRegistration,
  verifyGlobalCommandRegistration,
} from "../src/discord/command-registration.mjs";

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

test("global command registration verifies the complete catalog without relying on response order", () => {
  const expected = [
    {
      name: "path",
      name_localizations: { ko: "경로" },
      description: "Find paths",
      description_localizations: { ko: "경로 탐색" },
      type: 1,
      options: [
        {
          type: 3,
          name: "kicktable",
          name_localizations: { ko: "킥테이블" },
          description: "Rotation system",
          description_localizations: { ko: "회전 시스템" },
          required: false,
          choices: [
            {
              name: "SRS+",
              name_localizations: { ko: "SRS+" },
              value: "srs-plus",
            },
          ],
        },
      ],
    },
    { name: "help", type: 1 },
  ];
  assert.deepEqual(
    verifyGlobalCommandRegistration(expected, [
      {
        id: "2",
        application_id: "application",
        version: "version-2",
        name: "help",
        type: 1,
        default_member_permissions: null,
      },
      {
        id: "1",
        application_id: "application",
        version: "version-1",
        name: "path",
        name_localizations: { ko: "경로" },
        description: "Find paths",
        description_localizations: { ko: "경로 탐색" },
        type: 1,
        default_member_permissions: null,
        options: [
          {
            type: 3,
            name: "kicktable",
            name_localizations: { ko: "킥테이블" },
            description: "Rotation system",
            description_localizations: { ko: "회전 시스템" },
            autocomplete: false,
            choices: [
              {
                name: "SRS+",
                name_localizations: { ko: "SRS+" },
                value: "srs-plus",
              },
            ],
          },
        ],
      },
    ]),
    { count: 2, names: ["path", "help"] },
  );
  assert.throws(
    () =>
      verifyGlobalCommandRegistration(expected, [
        { id: "1", name: "path", type: 1 },
        { id: "3", name: "other", type: 1 },
      ]),
    /missing 1:help.*unexpected 1:other/,
  );
  assert.throws(
    () =>
      verifyGlobalCommandRegistration(expected, [
        { id: "1", name: "path", type: 1 },
        { id: "2", name: "path", type: 1 },
      ]),
    /did not match/,
  );

  const requiredOption = structuredClone(expected);
  requiredOption[0].options[0].required = true;
  assert.throws(
    () => verifyGlobalCommandRegistration(requiredOption, [
      { name: "help", type: 1 },
      {
        name: "path",
        name_localizations: { ko: "경로" },
        description: "Find paths",
        description_localizations: { ko: "경로 탐색" },
        type: 1,
        options: [{
          type: 3,
          name: "kicktable",
          name_localizations: { ko: "킥테이블" },
          description: "Rotation system",
          description_localizations: { ko: "회전 시스템" },
          choices: [{
            name: "SRS+",
            name_localizations: { ko: "SRS+" },
            value: "srs-plus",
          }],
        }],
      },
    ]),
    /options\[0\]\.required was missing/,
  );
});

test("global command registration verifies localizations and nested choices exactly", () => {
  const expected = [{
    name: "path",
    description: "Find paths",
    description_localizations: { ko: "경로 탐색" },
    options: [{
      type: 3,
      name: "kicktable",
      name_localizations: { ko: "킥테이블" },
      description: "Rotation system",
      choices: [{
        name: "SRS+",
        name_localizations: { ko: "SRS+" },
        value: "srs-plus",
      }],
    }],
  }];

  assert.throws(
    () => verifyGlobalCommandRegistration(expected, [{
      name: "path",
      description: "Find paths",
      description_localizations: {},
      options: [{
        type: 3,
        name: "kicktable",
        name_localizations: { ko: "킥테이블" },
        description: "Rotation system",
        choices: [{
          name: "SRS+",
          name_localizations: { ko: "SRS+" },
          value: "srs-plus",
        }],
      }],
    }]),
    /1:path\.description_localizations\.ko was missing/,
  );

  const wrongOptionLocalization = structuredClone(expected[0]);
  wrongOptionLocalization.options[0].name_localizations.ko = "회전표";
  assert.throws(
    () => verifyGlobalCommandRegistration(expected, [wrongOptionLocalization]),
    /options\[0\]\.name_localizations\.ko expected "킥테이블", received "회전표"/,
  );

  assert.throws(
    () => verifyGlobalCommandRegistration(expected, [{
      name: "path",
      description: "Find paths",
      description_localizations: { ko: "경로 탐색", ja: "経路探索" },
      options: [{
        type: 3,
        name: "kicktable",
        name_localizations: { ko: "킥테이블" },
        description: "Rotation system",
        choices: [{
          name: "SRS+",
          name_localizations: { ko: "SRS+" },
          value: "srs-plus",
        }],
      }],
    }]),
    /description_localizations contained unexpected locale ja/,
  );

  assert.throws(
    () => verifyGlobalCommandRegistration(expected, [{
      name: "path",
      description: "Find paths",
      description_localizations: { ko: "경로 탐색" },
      options: [{
        type: 3,
        name: "kicktable",
        name_localizations: { ko: "킥테이블" },
        description: "Rotation system",
        choices: [{
          name: "SRS+",
          name_localizations: { ko: "SRS+" },
          value: "srs",
        }],
      }],
    }]),
    /options\[0\]\.choices\[0\]\.value expected "srs-plus", received "srs"/,
  );

  assert.throws(
    () => verifyGlobalCommandRegistration(expected, [{
      name: "path",
      description: "Find paths",
      description_localizations: { ko: "경로 탐색" },
      options: [],
    }]),
    /1:path\.options\.length expected 1, received 0/,
  );
});

test("global command synchronization preserves matching command versions", async () => {
  const catalog = [{ name: "help", type: 1 }];
  let writes = 0;
  const result = await synchronizeGlobalCommandRegistration(
    {
      async getGlobalCommands() {
        return [{
          id: "123456789012345678",
          application_id: "223456789012345678",
          version: "323456789012345678",
          name: "help",
          type: 1,
        }];
      },
      async registerGlobalCommands() {
        writes += 1;
        return [];
      },
    },
    "223456789012345678",
    catalog,
  );

  assert.deepEqual(result, { changed: false, count: 1, names: ["help"] });
  assert.equal(writes, 0);
});

test("global command synchronization manages an exact message context command", async () => {
  const catalog = [
    { name: "render-file", type: 1, description: "Download a GIF" },
    {
      name: "Get original GIF",
      name_localizations: { ko: "원본 GIF 받기" },
      type: 3,
      integration_types: [0],
      contexts: [0],
    },
  ];
  const current = [
    {
      id: "123456789012345678",
      application_id: "223456789012345678",
      version: "323456789012345678",
      name: "render-file",
      type: 1,
      description: "Download a GIF",
    },
    {
      id: "423456789012345678",
      application_id: "223456789012345678",
      version: "523456789012345678",
      name: "Get original GIF",
      name_localizations: { ko: "원본 GIF 받기" },
      type: 3,
      description: "",
      integration_types: [0],
      contexts: [0],
    },
  ];
  let writes = 0;
  const result = await synchronizeGlobalCommandRegistration(
    {
      async getGlobalCommands() { return current; },
      async registerGlobalCommands() {
        writes += 1;
        return [];
      },
    },
    "223456789012345678",
    catalog,
  );

  assert.deepEqual(result, {
    changed: false,
    count: 2,
    names: ["render-file", "Get original GIF"],
  });
  assert.equal(writes, 0);
});

test("global command synchronization writes once and bounds stale readback polling", async () => {
  const catalog = [{ name: "help", type: 1 }];
  const exact = [{
    id: "123456789012345678",
    application_id: "223456789012345678",
    version: "323456789012345678",
    name: "help",
    type: 1,
  }];
  const reads = [[], [], exact];
  const delays = [];
  let writes = 0;
  const result = await synchronizeGlobalCommandRegistration(
    {
      async getGlobalCommands() {
        return reads.shift();
      },
      async registerGlobalCommands(_applicationId, commands) {
        writes += 1;
        assert.deepEqual(commands, catalog);
        return exact;
      },
    },
    "223456789012345678",
    catalog,
    {
      readbackAttempts: 2,
      retryDelayMs: 10,
      async wait(milliseconds) { delays.push(milliseconds); },
    },
  );

  assert.deepEqual(result, { changed: true, count: 1, names: ["help"] });
  assert.equal(writes, 1);
  assert.deepEqual(delays, [10]);
});

test("global command synchronization never repeats a PUT when readback stays stale", async () => {
  const catalog = [{ name: "help", type: 1 }];
  const exact = [{
    id: "123456789012345678",
    application_id: "223456789012345678",
    version: "323456789012345678",
    name: "help",
    type: 1,
  }];
  let writes = 0;
  await assert.rejects(
    synchronizeGlobalCommandRegistration(
      {
        async getGlobalCommands() { return []; },
        async registerGlobalCommands() {
          writes += 1;
          return exact;
        },
      },
      "223456789012345678",
      catalog,
      { readbackAttempts: 2, retryDelayMs: 0, async wait() {} },
    ),
    /readback did not converge after 2 attempts/,
  );
  assert.equal(writes, 1);
});

test("global command synchronization confirms one ambiguous PUT by readback", async () => {
  const catalog = [{ name: "help", type: 1 }];
  const exact = [{
    id: "123456789012345678",
    application_id: "223456789012345678",
    version: "323456789012345678",
    name: "help",
    type: 1,
  }];
  let reads = 0;
  let writes = 0;
  const result = await synchronizeGlobalCommandRegistration(
    {
      async getGlobalCommands() {
        reads += 1;
        return reads === 1 ? [] : exact;
      },
      async registerGlobalCommands() {
        writes += 1;
        const error = new Error("Discord API 500: ambiguous failure");
        error.discordAmbiguous = true;
        throw error;
      },
    },
    "223456789012345678",
    catalog,
    { readbackAttempts: 2, retryDelayMs: 0, async wait() {} },
  );

  assert.deepEqual(result, { changed: true, count: 1, names: ["help"] });
  assert.equal(reads, 2);
  assert.equal(writes, 1);
});

test("global command synchronization refuses to erase unmanaged command types", async () => {
  let writes = 0;
  await assert.rejects(
    synchronizeGlobalCommandRegistration(
      {
        async getGlobalCommands() {
          return [
            {
              id: "123456789012345678",
              application_id: "223456789012345678",
              version: "323456789012345678",
              name: "help",
              type: 1,
            },
            {
              id: "423456789012345678",
              application_id: "223456789012345678",
              version: "523456789012345678",
              name: "inspect",
              type: 2,
            },
          ];
        },
        async registerGlobalCommands() { writes += 1; },
      },
      "223456789012345678",
      [{ name: "help", type: 1 }],
    ),
    /refused to overwrite unmanaged command 2:inspect/,
  );
  assert.equal(writes, 0);
});
